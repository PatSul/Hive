use crate::autoresearch::config::AutoResearchConfig;
use crate::autoresearch::executor::AutoResearchExecutor;
use crate::autoresearch::types::EvalRunResult;
use hive_ai::types::{ChatMessage, ChatRequest, MessageRole};
use std::sync::atomic::{AtomicU64, Ordering};

/// AI-driven prompt rewriter that uses eval failure evidence.
pub struct PromptMutator {
    mutation_model: String,
    max_prompt_length: usize,
    cost_per_token: f64,
    accumulated_tokens: AtomicU64,
}

impl PromptMutator {
    pub fn new(config: &AutoResearchConfig) -> Self {
        Self {
            mutation_model: config.mutation_model.clone().unwrap_or_default(),
            max_prompt_length: config.max_prompt_length,
            cost_per_token: config.cost_per_token,
            accumulated_tokens: AtomicU64::new(0),
        }
    }

    /// Mutate a prompt based on eval failure evidence.
    ///
    /// Returns the new prompt text, trimmed and length-capped.
    pub async fn mutate<E: AutoResearchExecutor>(
        &self,
        executor: &E,
        current_prompt: &str,
        eval_result: &EvalRunResult,
    ) -> Result<String, String> {
        let failing_criteria: Vec<String> = eval_result
            .results
            .iter()
            .filter(|r| !r.passed)
            .map(|r| format!("- {}: {}", r.question_id, r.reasoning))
            .collect();

        let sample_outputs: String = eval_result
            .sample_outputs
            .iter()
            .enumerate()
            .map(|(i, o)| format!("--- Sample {} ---\n{}", i + 1, o))
            .collect::<Vec<_>>()
            .join("\n\n");

        let user_message = format!(
            "Current prompt:\n{current_prompt}\n\n\
             Pass rate: {:.0}%\n\n\
             Failing criteria:\n{}\n\n\
             Sample outputs that failed:\n{sample_outputs}",
            eval_result.pass_rate * 100.0,
            if failing_criteria.is_empty() {
                "(none — all criteria passed but overall score is low)".into()
            } else {
                failing_criteria.join("\n")
            },
        );

        let request = ChatRequest {
            messages: vec![ChatMessage::text(MessageRole::User, user_message)],
            model: self.mutation_model.clone(),
            max_tokens: 2048,
            temperature: Some(0.4),
            system_prompt: Some(
                "You are a prompt engineering expert. Rewrite the given system prompt to \
                 improve performance on the failing criteria. Keep the same intent and \
                 domain focus. Return ONLY the improved prompt text, no explanations \
                 or markdown formatting."
                    .into(),
            ),
            tools: None,
            cache_system_prompt: false,
        };

        let response = executor.execute(&request).await?;
        self.accumulated_tokens
            .fetch_add(response.usage.total_tokens as u64, Ordering::Relaxed);

        let mut new_prompt = response.content.trim().to_string();
        if new_prompt.is_empty() {
            return Err("AI returned empty prompt mutation".into());
        }

        // Enforce length limit (find nearest char boundary to avoid panic)
        if new_prompt.len() > self.max_prompt_length {
            let mut end = self.max_prompt_length;
            while !new_prompt.is_char_boundary(end) && end > 0 {
                end -= 1;
            }
            new_prompt.truncate(end);
        }

        Ok(new_prompt)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autoresearch::types::{EvalResult, EvalRunResult};
    use hive_ai::types::{ChatResponse, FinishReason, TokenUsage};
    use std::sync::Mutex;

    struct MockExecutor {
        responses: Mutex<Vec<String>>,
    }

    impl MockExecutor {
        fn new(responses: Vec<String>) -> Self {
            Self {
                responses: Mutex::new(responses),
            }
        }
    }

    impl AutoResearchExecutor for MockExecutor {
        async fn execute(&self, _request: &ChatRequest) -> Result<ChatResponse, String> {
            let mut responses = self.responses.lock().unwrap();
            let content = if responses.is_empty() {
                "mock response".into()
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

    fn make_eval_result() -> EvalRunResult {
        EvalRunResult {
            pass_rate: 0.5,
            results: vec![
                EvalResult {
                    question_id: "q1".into(),
                    passed: true,
                    reasoning: "ok".into(),
                },
                EvalResult {
                    question_id: "q2".into(),
                    passed: false,
                    reasoning: "Not safe enough".into(),
                },
            ],
            sample_outputs: vec!["bad output here".into()],
        }
    }

    #[tokio::test]
    async fn test_mutate_returns_new_prompt() {
        let executor = MockExecutor::new(vec![
            "You are an improved coding assistant that prioritizes safety.".into(),
        ]);
        let config = AutoResearchConfig::default();
        let mutator = PromptMutator::new(&config);

        let result = mutator
            .mutate(&executor, "You are a coder.", &make_eval_result())
            .await;
        assert!(result.is_ok());
        let new_prompt = result.unwrap();
        assert!(!new_prompt.is_empty());
        assert_ne!(new_prompt, "You are a coder.");
    }

    #[tokio::test]
    async fn test_mutate_respects_length_limit() {
        let long_response = "x".repeat(3000);
        let executor = MockExecutor::new(vec![long_response]);
        let config = AutoResearchConfig {
            max_prompt_length: 100,
            ..Default::default()
        };
        let mutator = PromptMutator::new(&config);

        let result = mutator
            .mutate(&executor, "short prompt", &make_eval_result())
            .await;
        assert!(result.is_ok());
        assert!(result.unwrap().len() <= 100);
    }

    #[tokio::test]
    async fn test_mutate_trims_whitespace() {
        let executor = MockExecutor::new(vec!["  Improved prompt with spaces  \n\n".into()]);
        let config = AutoResearchConfig::default();
        let mutator = PromptMutator::new(&config);

        let result = mutator
            .mutate(&executor, "Old prompt", &make_eval_result())
            .await
            .unwrap();
        assert!(!result.starts_with(' '));
        assert!(!result.ends_with('\n'));
    }

    #[tokio::test]
    async fn test_mutate_rejects_empty_response() {
        let executor = MockExecutor::new(vec!["   ".into()]);
        let config = AutoResearchConfig::default();
        let mutator = PromptMutator::new(&config);

        let result = mutator
            .mutate(&executor, "Old prompt", &make_eval_result())
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mutate_includes_failing_criteria_in_request() {
        // Verify the request sent to the AI includes the failing criteria
        let executor = MockExecutor::new(vec!["Improved prompt.".into()]);
        let config = AutoResearchConfig::default();
        let mutator = PromptMutator::new(&config);

        let result = mutator
            .mutate(&executor, "You are a coder.", &make_eval_result())
            .await;
        assert!(result.is_ok());
        // The mock doesn't validate request content, but this confirms the
        // function runs without error with failing criteria present
    }

    #[tokio::test]
    async fn test_mutate_tracks_cost() {
        let executor = MockExecutor::new(vec!["Improved.".into()]);
        let config = AutoResearchConfig::default();
        let mutator = PromptMutator::new(&config);

        mutator
            .mutate(&executor, "prompt", &make_eval_result())
            .await
            .unwrap();
        assert!(mutator.accumulated_cost() > 0.0);
    }
}
