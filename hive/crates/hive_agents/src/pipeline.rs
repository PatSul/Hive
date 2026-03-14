//! Hybrid Workflow Pipeline — deterministic control over AI execution.
//!
//! Implements the Stripe Minions pattern: each agent task flows through a
//! state machine of `CURATE_CONTEXT → AI_EXECUTE → VALIDATE → (retry or complete)`.
//! Deterministic systems control the AI, not the other way around.
//!
//! The pipeline is opt-in: when `CoordinatorConfig.pipeline` is `Some`, the
//! coordinator routes tasks through `TaskPipeline::execute` instead of calling
//! `execute_with_persona_model` directly.

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use hive_ai::rag::RagService;
use hive_core::SecurityGateway;

use crate::coordinator::{PlannedTask, TaskResult};
use crate::hivemind::AiExecutor;
use crate::personas::{execute_with_persona_model, Persona, PersonaKind};

// ---------------------------------------------------------------------------
// Pipeline Stage (for observability / logging)
// ---------------------------------------------------------------------------

/// The current stage of the pipeline state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum PipelineStage {
    ContextCuration,
    AiExecution,
    Validation,
    Complete,
    Failed,
}

// ---------------------------------------------------------------------------
// Pipeline Config
// ---------------------------------------------------------------------------

/// Configuration for the hybrid pipeline. Controls retry behavior,
/// context budget, and which validation gates are active.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineConfig {
    /// Maximum number of retry attempts after validation failure (default: 2).
    pub max_retries: u32,
    /// Approximate token budget for curated context (default: 4096).
    pub context_token_budget: usize,
    /// Which validation gates to apply after AI execution.
    pub validation_gates: Vec<ValidationGateKind>,
    /// Whether to run context curation before AI execution (default: true).
    pub enable_context_curation: bool,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            max_retries: 2,
            context_token_budget: 4096,
            validation_gates: vec![
                ValidationGateKind::OutputNotEmpty,
                ValidationGateKind::RefusalDetection,
            ],
            enable_context_curation: true,
        }
    }
}

// ---------------------------------------------------------------------------
// Validation Gates
// ---------------------------------------------------------------------------

/// The kinds of validation gate available for pipeline configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ValidationGateKind {
    /// Rejects blank or whitespace-only output.
    OutputNotEmpty,
    /// Detects common LLM refusal patterns ("I cannot", "As an AI", etc.).
    RefusalDetection,
    /// Scans shell-like lines through `SecurityGateway::check_command`.
    SecurityScan,
    /// For Implement/Debug personas: requires at least one code block.
    CodeBlockPresence,
    /// User-defined regex that output must NOT match.
    CustomForbiddenPattern(String),
}

/// Result of running a single validation gate.
#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub passed: bool,
    pub message: String,
}

impl ValidationResult {
    fn pass() -> Self {
        Self {
            passed: true,
            message: String::new(),
        }
    }

    fn fail(message: impl Into<String>) -> Self {
        Self {
            passed: false,
            message: message.into(),
        }
    }
}

/// Run a single validation gate against task output.
fn run_gate(
    kind: &ValidationGateKind,
    output: &str,
    task: &PlannedTask,
    security: Option<&SecurityGateway>,
) -> ValidationResult {
    match kind {
        ValidationGateKind::OutputNotEmpty => {
            if output.trim().is_empty() {
                ValidationResult::fail("Output is empty")
            } else {
                ValidationResult::pass()
            }
        }

        ValidationGateKind::RefusalDetection => {
            let refusal_patterns = [
                "i cannot",
                "i can't",
                "i'm unable",
                "i am unable",
                "as an ai",
                "i don't have the ability",
                "i'm not able",
            ];
            let lower = output.to_lowercase();
            // Only flag if the refusal appears in the first 200 chars
            // (genuine responses may quote these phrases later).
            let prefix = if lower.len() > 200 { &lower[..200] } else { &lower };
            for pattern in &refusal_patterns {
                if prefix.contains(pattern) {
                    return ValidationResult::fail(format!(
                        "Detected refusal pattern: \"{pattern}\""
                    ));
                }
            }
            ValidationResult::pass()
        }

        ValidationGateKind::SecurityScan => {
            let Some(gateway) = security else {
                return ValidationResult::pass();
            };
            // Extract lines that look like shell commands (start with $ or >).
            for line in output.lines() {
                let trimmed = line.trim();
                let cmd = if let Some(rest) = trimmed.strip_prefix("$ ") {
                    rest
                } else if let Some(rest) = trimmed.strip_prefix("> ") {
                    rest
                } else {
                    continue;
                };
                if let Err(reason) = gateway.check_command(cmd) {
                    return ValidationResult::fail(format!(
                        "Security gate blocked command: {reason}"
                    ));
                }
            }
            ValidationResult::pass()
        }

        ValidationGateKind::CodeBlockPresence => {
            // Only enforce for Implement and Debug personas.
            let needs_code = matches!(
                task.persona,
                PersonaKind::Implement | PersonaKind::Debug
            );
            if needs_code && !output.contains("```") {
                ValidationResult::fail(
                    "Implement/Debug persona must produce at least one code block",
                )
            } else {
                ValidationResult::pass()
            }
        }

        ValidationGateKind::CustomForbiddenPattern(pattern) => {
            match Regex::new(pattern) {
                Ok(re) => {
                    if re.is_match(output) {
                        ValidationResult::fail(format!(
                            "Output matches forbidden pattern: {pattern}"
                        ))
                    } else {
                        ValidationResult::pass()
                    }
                }
                Err(e) => {
                    // Bad regex — treat as pass but log.
                    tracing::warn!("Invalid forbidden pattern regex '{pattern}': {e}");
                    ValidationResult::pass()
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Context Curation
// ---------------------------------------------------------------------------

/// Context assembled deterministically before the AI executes.
#[derive(Debug, Clone)]
pub struct CuratedContext {
    /// Relevant code/doc snippets from the RAG index.
    pub relevant_snippets: String,
    /// Outputs from completed dependency tasks.
    pub prior_outputs: String,
    /// Estimated token count for the combined context.
    pub token_estimate: usize,
}

/// Gather curated context for a task. Deterministic — no AI calls.
///
/// - If a `RagService` is provided, queries it with the task description.
/// - Always gathers outputs from completed dependency tasks.
fn curate_context(
    task: &PlannedTask,
    prior_results: &[TaskResult],
    rag: Option<&RagService>,
    token_budget: usize,
) -> CuratedContext {
    // Gather dependency outputs (deterministic lookup).
    let mut prior_parts = Vec::new();
    for dep_id in &task.dependencies {
        if let Some(dep_result) = prior_results.iter().find(|r| r.task_id == *dep_id) {
            if dep_result.success && !dep_result.output.is_empty() {
                // Truncate long dependency outputs to share the budget.
                let max_dep_chars = 2000;
                let output = if dep_result.output.len() > max_dep_chars {
                    format!("{}...", &dep_result.output[..max_dep_chars])
                } else {
                    dep_result.output.clone()
                };
                prior_parts.push(format!("[{}] {}", dep_id, output));
            }
        }
    }
    let prior_outputs = prior_parts.join("\n\n");

    // RAG context: use remaining token budget after prior outputs.
    let prior_tokens = prior_outputs.len() / 4; // rough estimate
    let rag_budget = token_budget.saturating_sub(prior_tokens);

    let relevant_snippets = if rag_budget > 100 {
        rag.map(|r| r.build_context(&task.description, rag_budget))
            .unwrap_or_default()
    } else {
        String::new()
    };

    let snippet_tokens = relevant_snippets.len() / 4;
    let token_estimate = prior_tokens + snippet_tokens;

    CuratedContext {
        relevant_snippets,
        prior_outputs,
        token_estimate,
    }
}

// ---------------------------------------------------------------------------
// Task Pipeline
// ---------------------------------------------------------------------------

/// The hybrid task pipeline. Wraps `execute_with_persona_model` with
/// deterministic context curation, validation gates, and retry logic.
pub struct TaskPipeline<E: AiExecutor> {
    pub config: PipelineConfig,
    executor: Arc<E>,
    rag: Option<Arc<RagService>>,
    security: Option<Arc<SecurityGateway>>,
}

impl<E: AiExecutor> TaskPipeline<E> {
    /// Create a new pipeline with the given configuration.
    pub fn new(
        config: PipelineConfig,
        executor: Arc<E>,
        rag: Option<Arc<RagService>>,
        security: Option<Arc<SecurityGateway>>,
    ) -> Self {
        Self {
            config,
            executor,
            rag,
            security,
        }
    }

    /// Execute a task through the hybrid pipeline:
    /// `CURATE → EXECUTE → VALIDATE → (retry or done)`.
    ///
    /// Returns a `TaskResult` with accumulated cost across all attempts.
    pub async fn execute(
        &self,
        task: &PlannedTask,
        persona: &Persona,
        prior_results: &[TaskResult],
    ) -> TaskResult {
        // --- Stage 1: Context Curation (deterministic) ---
        let context = if self.config.enable_context_curation {
            curate_context(
                task,
                prior_results,
                self.rag.as_deref(),
                self.config.context_token_budget,
            )
        } else {
            curate_context(task, prior_results, None, 0)
        };

        let enriched_description = if context.relevant_snippets.is_empty()
            && context.prior_outputs.is_empty()
        {
            task.description.clone()
        } else {
            let mut desc = task.description.clone();
            if !context.prior_outputs.is_empty() {
                desc.push_str("\n\n## Prior Task Outputs\n");
                desc.push_str(&context.prior_outputs);
            }
            if !context.relevant_snippets.is_empty() {
                desc.push_str("\n\n## Relevant Context\n");
                desc.push_str(&context.relevant_snippets);
            }
            desc
        };

        let mut retries = 0u32;
        let mut total_cost = 0.0f64;
        let mut total_duration_ms = 0u64;
        let mut last_feedback: Option<String> = None;

        loop {
            // --- Stage 2: AI Execution (non-deterministic) ---
            let addendum = last_feedback.as_ref().map(|fb| {
                format!(
                    "Your previous attempt (retry {retries}) failed validation:\n{fb}\n\n\
                     Please fix the issues and try again."
                )
            });

            let output = execute_with_persona_model(
                persona,
                &enriched_description,
                self.executor.as_ref(),
                addendum.as_deref(),
                task.model_override.as_deref(),
            )
            .await;

            total_cost += output.cost;
            total_duration_ms += output.duration_ms;

            // If the AI call itself failed, return immediately (no point validating).
            if !output.success {
                return TaskResult {
                    task_id: task.id.clone(),
                    persona: task.persona.clone(),
                    output: output.content,
                    cost: total_cost,
                    duration_ms: total_duration_ms,
                    success: false,
                    error: output.error,
                };
            }

            // --- Stage 3: Validation (deterministic) ---
            let mut all_passed = true;
            let mut feedback = Vec::new();

            for gate_kind in &self.config.validation_gates {
                let result =
                    run_gate(gate_kind, &output.content, task, self.security.as_deref());
                if !result.passed {
                    all_passed = false;
                    feedback.push(result.message);
                }
            }

            if all_passed {
                return TaskResult {
                    task_id: task.id.clone(),
                    persona: task.persona.clone(),
                    output: output.content,
                    cost: total_cost,
                    duration_ms: total_duration_ms,
                    success: true,
                    error: None,
                };
            }

            // --- Stage 4: Retry or Fail ---
            retries += 1;
            if retries > self.config.max_retries {
                let error_msg = format!(
                    "Validation failed after {} retries: {}",
                    retries - 1,
                    feedback.join("; ")
                );
                return TaskResult {
                    task_id: task.id.clone(),
                    persona: task.persona.clone(),
                    output: output.content,
                    cost: total_cost,
                    duration_ms: total_duration_ms,
                    success: false,
                    error: Some(error_msg),
                };
            }

            last_feedback = Some(feedback.join("\n"));
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hivemind::AiExecutor;
    use hive_ai::types::{ChatRequest, ChatResponse, FinishReason, TokenUsage};
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Mock executor that returns configurable responses.
    struct MockExecutor {
        responses: Vec<String>,
        call_count: AtomicUsize,
    }

    impl MockExecutor {
        fn new(responses: Vec<String>) -> Self {
            Self {
                responses,
                call_count: AtomicUsize::new(0),
            }
        }
    }

    impl AiExecutor for MockExecutor {
        async fn execute(&self, _request: &ChatRequest) -> Result<ChatResponse, String> {
            let idx = self.call_count.fetch_add(1, Ordering::SeqCst);
            let content = self
                .responses
                .get(idx)
                .cloned()
                .unwrap_or_else(|| "default response".to_string());
            Ok(ChatResponse {
                content,
                model: "mock-model".to_string(),
                usage: TokenUsage {
                    prompt_tokens: 100,
                    completion_tokens: 50,
                    total_tokens: 150,
                    cache_creation_input_tokens: None,
                    cache_read_input_tokens: None,
                },
                finish_reason: FinishReason::Stop,
                thinking: None,
                tool_calls: None,
            })
        }
    }

    fn test_task(id: &str, desc: &str) -> PlannedTask {
        PlannedTask {
            id: id.to_string(),
            description: desc.to_string(),
            persona: PersonaKind::Implement,
            dependencies: Vec::new(),
            priority: 1,
            model_override: None,
        }
    }

    fn test_persona() -> Persona {
        Persona {
            name: "test".to_string(),
            kind: PersonaKind::Implement,
            description: "Test persona".to_string(),
            system_prompt: "You are a test agent.".to_string(),
            model_tier: hive_ai::types::ModelTier::Mid,
            tools: Vec::new(),
            max_tokens: 4096,
        }
    }

    #[tokio::test]
    async fn test_pipeline_passes_valid_output() {
        let executor = Arc::new(MockExecutor::new(vec![
            "```rust\nfn main() {}\n```".to_string(),
        ]));
        let pipeline = TaskPipeline::new(
            PipelineConfig {
                validation_gates: vec![
                    ValidationGateKind::OutputNotEmpty,
                    ValidationGateKind::CodeBlockPresence,
                ],
                ..Default::default()
            },
            executor,
            None,
            None,
        );

        let task = test_task("t1", "Write a main function");
        let result = pipeline.execute(&task, &test_persona(), &[]).await;

        assert!(result.success);
        assert!(result.error.is_none());
    }

    #[tokio::test]
    async fn test_pipeline_rejects_empty_output_and_retries() {
        let executor = Arc::new(MockExecutor::new(vec![
            "".to_string(), // initial call: empty
            "".to_string(), // retry 1: still empty
            "".to_string(), // retry 2: still empty
        ]));
        let pipeline = TaskPipeline::new(
            PipelineConfig {
                max_retries: 2,
                validation_gates: vec![ValidationGateKind::OutputNotEmpty],
                enable_context_curation: false,
                ..Default::default()
            },
            executor.clone(),
            None,
            None,
        );

        let task = test_task("t1", "Do something");
        let result = pipeline.execute(&task, &test_persona(), &[]).await;

        assert!(!result.success);
        assert!(result.error.as_ref().unwrap().contains("Validation failed"));
        // 1 initial + 2 retries = 3 total AI calls
        assert_eq!(executor.call_count.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_pipeline_retries_then_succeeds() {
        let executor = Arc::new(MockExecutor::new(vec![
            "".to_string(),              // attempt 1: empty
            "got it done".to_string(),   // attempt 2: valid
        ]));
        let pipeline = TaskPipeline::new(
            PipelineConfig {
                max_retries: 3,
                validation_gates: vec![ValidationGateKind::OutputNotEmpty],
                enable_context_curation: false,
                ..Default::default()
            },
            executor.clone(),
            None,
            None,
        );

        let task = test_task("t1", "Do something");
        let result = pipeline.execute(&task, &test_persona(), &[]).await;

        assert!(result.success);
        assert_eq!(executor.call_count.load(Ordering::SeqCst), 2);
        // Cost should be accumulated across both attempts.
        assert!(result.cost > 0.0);
    }

    #[tokio::test]
    async fn test_pipeline_detects_refusal() {
        let executor = Arc::new(MockExecutor::new(vec![
            "I cannot help with that request.".to_string(),
            "I'm unable to assist.".to_string(),
            "As an AI, I cannot do that.".to_string(),
        ]));
        let pipeline = TaskPipeline::new(
            PipelineConfig {
                max_retries: 2,
                validation_gates: vec![ValidationGateKind::RefusalDetection],
                enable_context_curation: false,
                ..Default::default()
            },
            executor,
            None,
            None,
        );

        let task = test_task("t1", "Write code");
        let result = pipeline.execute(&task, &test_persona(), &[]).await;

        assert!(!result.success);
        assert!(result.error.as_ref().unwrap().contains("refusal"));
    }

    #[tokio::test]
    async fn test_pipeline_code_block_gate_implement() {
        let executor = Arc::new(MockExecutor::new(vec![
            "Here is the plan but no code".to_string(),
            "```rust\nfn solve() {}\n```".to_string(),
        ]));
        let pipeline = TaskPipeline::new(
            PipelineConfig {
                max_retries: 2,
                validation_gates: vec![ValidationGateKind::CodeBlockPresence],
                enable_context_curation: false,
                ..Default::default()
            },
            executor.clone(),
            None,
            None,
        );

        let task = test_task("t1", "Implement the solution");
        let result = pipeline.execute(&task, &test_persona(), &[]).await;

        assert!(result.success);
        assert_eq!(executor.call_count.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn test_pipeline_code_block_gate_skips_non_implement() {
        let executor = Arc::new(MockExecutor::new(vec![
            "The architecture looks fine.".to_string(),
        ]));
        let pipeline = TaskPipeline::new(
            PipelineConfig {
                validation_gates: vec![ValidationGateKind::CodeBlockPresence],
                enable_context_curation: false,
                ..Default::default()
            },
            executor,
            None,
            None,
        );

        // Critique persona — code block gate should not apply.
        let mut task = test_task("t1", "Review the code");
        task.persona = PersonaKind::Critique;
        let mut persona = test_persona();
        persona.kind = PersonaKind::Critique;

        let result = pipeline.execute(&task, &persona, &[]).await;
        assert!(result.success);
    }

    #[tokio::test]
    async fn test_context_curation_gathers_dependencies() {
        let prior = vec![
            TaskResult {
                task_id: "dep-1".to_string(),
                persona: PersonaKind::Investigate,
                output: "Found 3 files needing changes".to_string(),
                cost: 0.5,
                duration_ms: 100,
                success: true,
                error: None,
            },
            TaskResult {
                task_id: "dep-2".to_string(),
                persona: PersonaKind::Investigate,
                output: "API schema documented".to_string(),
                cost: 0.3,
                duration_ms: 80,
                success: true,
                error: None,
            },
        ];

        let mut task = test_task("t3", "Implement based on investigation");
        task.dependencies = vec!["dep-1".to_string(), "dep-2".to_string()];

        let ctx = curate_context(&task, &prior, None, 4096);

        assert!(ctx.prior_outputs.contains("dep-1"));
        assert!(ctx.prior_outputs.contains("Found 3 files"));
        assert!(ctx.prior_outputs.contains("dep-2"));
        assert!(ctx.prior_outputs.contains("API schema"));
    }

    #[tokio::test]
    async fn test_context_curation_skips_failed_deps() {
        let prior = vec![TaskResult {
            task_id: "dep-1".to_string(),
            persona: PersonaKind::Investigate,
            output: "partial output".to_string(),
            cost: 0.1,
            duration_ms: 50,
            success: false,
            error: Some("timed out".to_string()),
        }];

        let mut task = test_task("t2", "Continue from dep-1");
        task.dependencies = vec!["dep-1".to_string()];

        let ctx = curate_context(&task, &prior, None, 4096);
        assert!(ctx.prior_outputs.is_empty());
    }

    #[test]
    fn test_custom_forbidden_pattern_gate() {
        let task = test_task("t1", "test");
        let gate = ValidationGateKind::CustomForbiddenPattern("TODO|FIXME".to_string());

        let result = run_gate(&gate, "This code has a TODO item", &task, None);
        assert!(!result.passed);

        let result = run_gate(&gate, "Clean production code", &task, None);
        assert!(result.passed);
    }

    #[test]
    fn test_refusal_detection_ignores_late_mentions() {
        let task = test_task("t1", "test");
        let gate = ValidationGateKind::RefusalDetection;

        // "I cannot" appearing after the first 200 chars should not trigger.
        let mut output = "A".repeat(250);
        output.push_str(" I cannot do this.");
        let result = run_gate(&gate, &output, &task, None);
        assert!(result.passed);

        // But at the start it should trigger.
        let result = run_gate(&gate, "I cannot do this task.", &task, None);
        assert!(!result.passed);
    }

    #[test]
    fn test_pipeline_config_default() {
        let config = PipelineConfig::default();
        assert_eq!(config.max_retries, 2);
        assert_eq!(config.context_token_budget, 4096);
        assert!(config.enable_context_curation);
        assert_eq!(config.validation_gates.len(), 2);
    }
}
