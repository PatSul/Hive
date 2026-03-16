use crate::autoresearch::config::AutoResearchConfig;
use crate::autoresearch::eval_suite::EvalSuite;
use crate::autoresearch::executor::AutoResearchExecutor;
use crate::autoresearch::types::{EvalResult, EvalRunResult};
use hive_ai::types::{ChatMessage, ChatRequest, MessageRole};
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::warn;

/// Executes a skill and judges output against eval criteria.
pub struct EvalRunner {
    eval_samples: u32,
    eval_model: String,
    skill_model: String,
    cost_per_token: f64,
    accumulated_tokens: AtomicU64,
}

impl EvalRunner {
    pub fn new(config: &AutoResearchConfig) -> Self {
        Self {
            eval_samples: config.eval_samples_per_iteration,
            eval_model: config.eval_model.clone().unwrap_or_default(),
            skill_model: config.skill_execution_model.clone().unwrap_or_default(),
            cost_per_token: config.cost_per_token,
            accumulated_tokens: AtomicU64::new(0),
        }
    }

    /// Run the full eval: execute skill N times, judge each, average pass rates.
    pub async fn run_eval<E: AutoResearchExecutor>(
        &self,
        executor: &E,
        suite: &EvalSuite,
        prompt: &str,
        test_input: &str,
    ) -> Result<EvalRunResult, String> {
        let mut sample_pass_rates = Vec::new();
        let mut sample_outputs = Vec::new();
        let mut last_results = Vec::new();
        let mut success_count = 0u32;

        for _ in 0..self.eval_samples {
            // Call 1: Execute the skill
            let skill_request = ChatRequest {
                messages: vec![ChatMessage::text(MessageRole::User, test_input.to_string())],
                model: self.skill_model.clone(),
                max_tokens: 2048,
                temperature: Some(0.3),
                system_prompt: Some(prompt.to_string()),
                tools: None,
                cache_system_prompt: false,
            };

            let skill_response = match executor.execute(&skill_request).await {
                Ok(resp) => {
                    self.accumulated_tokens.fetch_add(resp.usage.total_tokens as u64, Ordering::Relaxed);
                    resp
                }
                Err(e) => {
                    warn!(error = %e, "Skill execution failed for sample");
                    continue; // Skip this sample
                }
            };

            let skill_output = skill_response.content.clone();
            sample_outputs.push(skill_output.clone());

            // Call 2: Judge the output
            let questions_text: String = suite
                .questions
                .iter()
                .map(|q| format!("- id: \"{}\", question: \"{}\"", q.id, q.question))
                .collect::<Vec<_>>()
                .join("\n");

            let judge_request = ChatRequest {
                messages: vec![ChatMessage::text(
                    MessageRole::User,
                    format!(
                        "Skill output:\n{skill_output}\n\nEval questions:\n{questions_text}"
                    ),
                )],
                model: self.eval_model.clone(),
                max_tokens: 1024,
                temperature: Some(0.1),
                system_prompt: Some(
                    "You are an eval judge. For each question, respond with a JSON array of \
                     {\"id\": string, \"passed\": bool, \"reasoning\": string}. \
                     Judge ONLY based on the output provided. Return ONLY the JSON array."
                        .into(),
                ),
                tools: None,
                cache_system_prompt: false,
            };

            match executor.execute(&judge_request).await {
                Ok(judge_response) => {
                    self.accumulated_tokens.fetch_add(
                        judge_response.usage.total_tokens as u64,
                        Ordering::Relaxed,
                    );
                    match parse_judgment(&judge_response.content) {
                        Ok(results) => {
                            let pass_rate = suite.weighted_pass_rate(&results);
                            sample_pass_rates.push(pass_rate);
                            last_results = results;
                            success_count += 1;
                        }
                        Err(e) => {
                            warn!(error = %e, "Failed to parse eval judgment");
                            // Skip this sample
                        }
                    }
                }
                Err(e) => {
                    warn!(error = %e, "Eval judgment call failed");
                    // Skip this sample
                }
            }
        }

        // Compute average pass rate from successful samples
        let pass_rate = if sample_pass_rates.is_empty() {
            0.0 // All samples failed
        } else {
            sample_pass_rates.iter().sum::<f64>() / sample_pass_rates.len() as f64
        };

        if success_count > 0 && success_count < self.eval_samples.div_ceil(2) {
            warn!(
                success_count,
                total = self.eval_samples,
                "More than half of eval samples failed"
            );
        }

        Ok(EvalRunResult {
            pass_rate,
            results: last_results,
            sample_outputs,
        })
    }

    /// Return total accumulated cost from all AI calls.
    pub fn accumulated_cost(&self) -> f64 {
        self.accumulated_tokens.load(Ordering::Relaxed) as f64 * self.cost_per_token
    }

    /// Return total accumulated tokens.
    pub fn accumulated_tokens(&self) -> u64 {
        self.accumulated_tokens.load(Ordering::Relaxed)
    }

    /// Reset accumulated cost tracking.
    pub fn reset_cost(&self) {
        self.accumulated_tokens.store(0, Ordering::Relaxed);
    }
}

/// Intermediate type for deserializing judge JSON which uses `id` instead of `question_id`.
#[derive(serde::Deserialize)]
struct JudgeEntry {
    id: String,
    passed: bool,
    reasoning: String,
}

impl From<JudgeEntry> for EvalResult {
    fn from(e: JudgeEntry) -> Self {
        EvalResult {
            question_id: e.id,
            passed: e.passed,
            reasoning: e.reasoning,
        }
    }
}

/// Parse a judgment JSON array from an AI response.
///
/// Handles responses wrapped in markdown code fences.
fn parse_judgment(response: &str) -> Result<Vec<EvalResult>, String> {
    let start = response.find('[').ok_or("No JSON array found")?;
    let end = response.rfind(']').ok_or("No closing bracket")?;
    if end <= start {
        return Err("Malformed JSON array".into());
    }
    let json_str = &response[start..=end];
    let entries: Vec<JudgeEntry> =
        serde_json::from_str(json_str).map_err(|e| format!("JSON parse error: {e}"))?;
    Ok(entries.into_iter().map(EvalResult::from).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autoresearch::eval_suite::EvalQuestion;
    use hive_ai::types::{ChatResponse, FinishReason, TokenUsage};
    use std::sync::Mutex;

    struct MockExecutor {
        responses: Mutex<Vec<String>>,
    }

    impl MockExecutor {
        fn new(responses: Vec<String>) -> Self {
            Self { responses: Mutex::new(responses) }
        }
    }

    impl AutoResearchExecutor for MockExecutor {
        async fn execute(&self, _request: &ChatRequest) -> Result<ChatResponse, String> {
            let mut responses = self.responses.lock().unwrap();
            let content = if responses.is_empty() {
                "mock response".to_string()
            } else {
                responses.remove(0)
            };
            Ok(ChatResponse {
                content,
                model: "mock".into(),
                usage: TokenUsage {
                    prompt_tokens: 10,
                    completion_tokens: 20,
                    total_tokens: 30,
                    cache_creation_input_tokens: None,
                    cache_read_input_tokens: None,
                },
                finish_reason: FinishReason::Stop,
                thinking: None,
                tool_calls: None,
            })
        }
    }

    fn make_suite() -> EvalSuite {
        EvalSuite::from_explicit("test-skill".into(), vec![
            EvalQuestion { id: "q1".into(), question: "Is it correct?".into(), weight: 1.0 },
            EvalQuestion { id: "q2".into(), question: "Is it safe?".into(), weight: 1.0 },
        ])
    }

    fn good_judgment_json() -> String {
        serde_json::json!([
            {"id": "q1", "passed": true, "reasoning": "Looks correct"},
            {"id": "q2", "passed": true, "reasoning": "No safety issues"}
        ]).to_string()
    }

    fn mixed_judgment_json() -> String {
        serde_json::json!([
            {"id": "q1", "passed": true, "reasoning": "Correct"},
            {"id": "q2", "passed": false, "reasoning": "Not safe"}
        ]).to_string()
    }

    #[tokio::test]
    async fn test_run_eval_all_pass() {
        let executor = MockExecutor::new(vec![
            "skill output 1".into(), good_judgment_json(),  // sample 1
            "skill output 2".into(), good_judgment_json(),  // sample 2
            "skill output 3".into(), good_judgment_json(),  // sample 3
        ]);
        let config = AutoResearchConfig::default();
        let runner = EvalRunner::new(&config);
        let suite = make_suite();

        let result = runner.run_eval(
            &executor, &suite, "You are a coder.", "Write hello world",
        ).await;
        assert!(result.is_ok());
        let run = result.unwrap();
        assert!((run.pass_rate - 1.0).abs() < f64::EPSILON);
        assert_eq!(run.sample_outputs.len(), 3);
    }

    #[tokio::test]
    async fn test_run_eval_mixed_results() {
        let executor = MockExecutor::new(vec![
            "output 1".into(), mixed_judgment_json(),  // sample 1
            "output 2".into(), mixed_judgment_json(),  // sample 2
            "output 3".into(), mixed_judgment_json(),  // sample 3
        ]);
        let config = AutoResearchConfig::default();
        let runner = EvalRunner::new(&config);
        let suite = make_suite();

        let result = runner.run_eval(
            &executor, &suite, "You are a coder.", "Write hello world",
        ).await;
        assert!(result.is_ok());
        let run = result.unwrap();
        assert!((run.pass_rate - 0.5).abs() < 0.01);
    }

    #[tokio::test]
    async fn test_run_eval_returns_cost() {
        let executor = MockExecutor::new(vec![
            "output".into(), good_judgment_json(),
            "output".into(), good_judgment_json(),
            "output".into(), good_judgment_json(),
        ]);
        let config = AutoResearchConfig::default();
        let runner = EvalRunner::new(&config);
        let suite = make_suite();

        let result = runner.run_eval(
            &executor, &suite, "prompt", "input",
        ).await.unwrap();
        let _ = result;
        // Each call uses 30 tokens, 6 calls total = 180 tokens
        // 180 * 0.000003 = 0.00054
        assert!(runner.accumulated_cost() > 0.0);
    }

    #[tokio::test]
    async fn test_parse_judgment_from_markdown_fences() {
        let fenced = format!("```json\n{}\n```", good_judgment_json());
        let executor = MockExecutor::new(vec![
            "output".into(), fenced.clone(),
            "output".into(), fenced.clone(),
            "output".into(), fenced,
        ]);
        let config = AutoResearchConfig::default();
        let runner = EvalRunner::new(&config);
        let suite = make_suite();

        let result = runner.run_eval(
            &executor, &suite, "prompt", "input",
        ).await.unwrap();
        assert!((result.pass_rate - 1.0).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn test_malformed_judgment_treated_as_failure() {
        let executor = MockExecutor::new(vec![
            "output".into(), "not valid json".into(),
            "output".into(), "also bad".into(),
            "output".into(), "still bad".into(),
        ]);
        let config = AutoResearchConfig::default();
        let runner = EvalRunner::new(&config);
        let suite = make_suite();

        let result = runner.run_eval(
            &executor, &suite, "prompt", "input",
        ).await.unwrap();
        // All samples fail to parse -> pass_rate = 0.0
        assert!((result.pass_rate - 0.0).abs() < f64::EPSILON);
    }
}
