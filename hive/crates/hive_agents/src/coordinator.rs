//! Multi-Agent Coordinator — task dispatch with dependency ordering.
//!
//! Reads specifications from the `specs` module, decomposes them into tasks
//! via AI, and dispatches to specialist agent personas in dependency-ordered
//! waves. Tasks within a wave are independent and executed sequentially.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::sync::broadcast;

use hive_ai::rag::RagService;
use hive_ai::types::{ChatMessage, ChatRequest, MessageRole, ModelTier};

use crate::activity::budget::{BudgetDecision, BudgetEnforcer};

use crate::hivemind::{AiExecutor, default_model_for_tier};
use crate::personas::{Persona, PersonaKind, PersonaRegistry, execute_with_persona_model};
use crate::pipeline::{PipelineConfig, TaskPipeline};
use crate::specs::Spec;

// ---------------------------------------------------------------------------
// Task Events — live progress reporting
// ---------------------------------------------------------------------------

/// Events emitted during coordinator task execution for live UI updates.
#[derive(Clone, Debug, Serialize)]
pub enum TaskEvent {
    /// A new plan has been created with the given tasks.
    PlanCreated {
        plan_id: String,
        tasks: Vec<TaskEventInfo>,
    },
    /// A specific task has started execution.
    TaskStarted {
        task_id: String,
        description: String,
        persona: String,
    },
    /// Progress update for a running task.
    TaskProgress {
        task_id: String,
        progress: f32,
        message: String,
    },
    /// A task completed successfully.
    TaskCompleted {
        task_id: String,
        duration_ms: u64,
        cost: f64,
        output_preview: String,
    },
    /// A task failed with an error.
    TaskFailed {
        task_id: String,
        error: String,
    },
    /// All tasks in the plan have finished.
    AllComplete {
        total_cost: f64,
        total_duration_ms: u64,
        success_count: usize,
        failure_count: usize,
    },
}

/// Summary info for a task in a plan, used in the `PlanCreated` event.
#[derive(Clone, Debug, Serialize)]
pub struct TaskEventInfo {
    pub id: String,
    pub description: String,
    pub persona: String,
    pub dependencies: Vec<String>,
    /// The user-pinned model override for this task, if any.
    pub model_override: Option<String>,
}

// ---------------------------------------------------------------------------
// Coordinator Config
// ---------------------------------------------------------------------------

/// Configuration for the multi-agent coordinator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoordinatorConfig {
    /// Maximum number of tasks to run in parallel.
    pub max_parallel: usize,
    /// Total cost limit in USD for the entire execution.
    pub cost_limit: f64,
    /// Time limit in seconds for the entire execution.
    pub time_limit_secs: u64,
    /// Model ID to use for the coordination/planning step.
    pub model_for_coordination: String,
    /// When set, tasks are executed through the hybrid pipeline with
    /// context curation, validation gates, and retry logic.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pipeline: Option<PipelineConfig>,
    /// RAG service for pipeline context curation (not serialized).
    #[serde(skip)]
    pub rag: Option<Arc<Mutex<RagService>>>,
    /// Optional budget enforcer for pre-flight cost checks.
    #[serde(skip)]
    pub budget: Option<Arc<BudgetEnforcer>>,
}

impl Default for CoordinatorConfig {
    fn default() -> Self {
        Self {
            max_parallel: 4,
            cost_limit: 10.0,
            time_limit_secs: 600,
            model_for_coordination: default_model_for_tier(ModelTier::Mid),
            pipeline: None,
            rag: None,
            budget: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Task Plan
// ---------------------------------------------------------------------------

/// A planned task to be dispatched to a specialist persona.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlannedTask {
    pub id: String,
    pub description: String,
    pub persona: PersonaKind,
    pub dependencies: Vec<String>,
    pub priority: u8,
    /// Optional user-pinned model override. When set, the task uses this
    /// specific model instead of the persona's default `model_tier`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_override: Option<String>,
}

/// The complete task plan produced by the coordinator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskPlan {
    pub tasks: Vec<PlannedTask>,
}

impl TaskPlan {
    /// Return task IDs that have no dependencies (ready to run immediately).
    pub fn root_tasks(&self) -> Vec<&PlannedTask> {
        self.tasks
            .iter()
            .filter(|t| t.dependencies.is_empty())
            .collect()
    }

    /// Return task IDs that depend on the given completed task.
    pub fn dependents_of(&self, task_id: &str) -> Vec<&PlannedTask> {
        self.tasks
            .iter()
            .filter(|t| t.dependencies.iter().any(|d| d == task_id))
            .collect()
    }

    /// Validate the plan: check for missing dependencies and cycles.
    pub fn validate(&self) -> Result<(), String> {
        let ids: HashSet<&str> = self.tasks.iter().map(|t| t.id.as_str()).collect();

        // Check all dependencies reference existing tasks.
        for task in &self.tasks {
            for dep in &task.dependencies {
                if !ids.contains(dep.as_str()) {
                    return Err(format!(
                        "Task '{}' depends on unknown task '{dep}'",
                        task.id
                    ));
                }
            }
            // No self-dependency.
            if task.dependencies.contains(&task.id) {
                return Err(format!("Task '{}' depends on itself", task.id));
            }
        }

        // Simple cycle detection via topological sort.
        let mut in_deg: HashMap<&str, usize> = self
            .tasks
            .iter()
            .map(|t| (t.id.as_str(), t.dependencies.len()))
            .collect();

        let mut queue: Vec<&str> = in_deg
            .iter()
            .filter(|(_, deg)| **deg == 0)
            .map(|(id, _)| *id)
            .collect();
        let mut visited = 0;

        while let Some(current) = queue.pop() {
            visited += 1;
            for task in &self.tasks {
                if task.dependencies.iter().any(|d| d == current) {
                    let deg = in_deg.get_mut(task.id.as_str())
                        .ok_or_else(|| format!("Task '{}' missing from in-degree map", task.id))?;
                    *deg -= 1;
                    if *deg == 0 {
                        queue.push(task.id.as_str());
                    }
                }
            }
        }

        if visited != self.tasks.len() {
            return Err("Dependency cycle detected in task plan".into());
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Task Result
// ---------------------------------------------------------------------------

/// Result of executing a single planned task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    pub task_id: String,
    pub persona: PersonaKind,
    pub output: String,
    pub cost: f64,
    pub duration_ms: u64,
    pub success: bool,
    pub error: Option<String>,
}

// ---------------------------------------------------------------------------
// Coordinator Result
// ---------------------------------------------------------------------------

/// Complete result of a coordinator execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoordinatorResult {
    pub plan: TaskPlan,
    pub results: Vec<TaskResult>,
    pub total_cost: f64,
    pub total_duration_ms: u64,
    pub spec_updates: Vec<String>,
}

impl CoordinatorResult {
    pub fn successful_tasks(&self) -> usize {
        self.results.iter().filter(|r| r.success).count()
    }

    pub fn failed_tasks(&self) -> usize {
        self.results.iter().filter(|r| !r.success).count()
    }
}

// ---------------------------------------------------------------------------
// Coordinator
// ---------------------------------------------------------------------------

/// Multi-agent coordinator that decomposes specs into tasks and dispatches
/// them to specialist personas with dependency-aware parallelism.
pub struct Coordinator<E: AiExecutor> {
    pub config: CoordinatorConfig,
    executor: Arc<E>,
    registry: PersonaRegistry,
    event_tx: broadcast::Sender<TaskEvent>,
}

impl<E: AiExecutor + 'static> Coordinator<E> {
    pub fn new(config: CoordinatorConfig, executor: E) -> Self {
        let (event_tx, _) = broadcast::channel(64);
        Self {
            config,
            executor: Arc::new(executor),
            registry: PersonaRegistry::new(),
            event_tx,
        }
    }

    /// Create a coordinator with a custom persona registry.
    pub fn with_registry(
        config: CoordinatorConfig,
        executor: E,
        registry: PersonaRegistry,
    ) -> Self {
        let (event_tx, _) = broadcast::channel(64);
        Self {
            config,
            executor: Arc::new(executor),
            registry,
            event_tx,
        }
    }

    /// Subscribe to live task events from this coordinator.
    pub fn subscribe(&self) -> broadcast::Receiver<TaskEvent> {
        self.event_tx.subscribe()
    }

    /// Use AI to decompose a specification into a task plan.
    pub async fn plan_from_spec(&self, spec: &Spec) -> Result<TaskPlan, String> {
        let prompt = format!(
            "Decompose the following specification into concrete tasks.\n\n\
             Title: {}\n\
             Description: {}\n\n\
             For each task, specify:\n\
             - A short ID (e.g. task-1, task-2)\n\
             - A description\n\
             - Which persona should handle it (investigate, implement, verify, critique, debug, code_review)\n\
             - Dependencies (other task IDs that must complete first)\n\
             - Priority (1=highest, 5=lowest)\n\n\
             Return ONLY a JSON array of objects with fields: id, description, persona, dependencies, priority.",
            spec.title, spec.description
        );

        let request = ChatRequest {
            messages: vec![ChatMessage::text(MessageRole::User, prompt)],
            model: self.config.model_for_coordination.clone(),
            max_tokens: 4096,
            temperature: Some(0.2),
            system_prompt: Some(
                "You are a project planning assistant. Return valid JSON only.".into(),
            ),
            tools: None,
            cache_system_prompt: false,
        };

        let response = self.executor.execute(&request).await?;
        parse_task_plan(&response.content)
    }

    /// Emit a task event, ignoring errors when there are no subscribers.
    fn emit(&self, event: TaskEvent) {
        let _ = self.event_tx.send(event);
    }

    /// Execute a task plan, respecting dependency ordering and parallelism limits.
    /// Emits `TaskEvent`s for live progress tracking via `subscribe()`.
    ///
    /// When `config.pipeline` is set, tasks are executed through the hybrid
    /// pipeline with context curation, validation gates, and retry logic.
    /// Failed tasks block their dependents (dependents are skipped).
    pub async fn execute_plan(&self, plan: &TaskPlan) -> CoordinatorResult {
        let start = Instant::now();
        let mut results: Vec<TaskResult> = Vec::new();
        let mut completed: HashSet<String> = HashSet::new();
        let mut failed: HashSet<String> = HashSet::new();
        let mut remaining: Vec<PlannedTask> = plan.tasks.clone();

        // Build pipeline if configured.
        let pipeline = self.config.pipeline.as_ref().map(|cfg| {
            TaskPipeline::new(cfg.clone(), self.executor.clone(), self.config.rag.clone(), None)
        });

        // Emit PlanCreated with all task info.
        let plan_id = format!("plan-{}", start.elapsed().as_nanos());
        self.emit(TaskEvent::PlanCreated {
            plan_id: plan_id.clone(),
            tasks: plan
                .tasks
                .iter()
                .map(|t| TaskEventInfo {
                    id: t.id.clone(),
                    description: t.description.clone(),
                    persona: t.persona.to_string(),
                    dependencies: t.dependencies.clone(),
                    model_override: t.model_override.clone(),
                })
                .collect(),
        });

        // Process tasks in waves: each wave contains tasks whose dependencies
        // are all satisfied.
        while !remaining.is_empty() {
            // Check time limit.
            if start.elapsed().as_secs() >= self.config.time_limit_secs {
                break;
            }

            // Check cost limit.
            let current_cost: f64 = results.iter().map(|r| r.cost).sum();
            if current_cost >= self.config.cost_limit {
                break;
            }

            // Skip tasks whose dependencies include a failed task.
            let (skipped, viable): (Vec<PlannedTask>, Vec<PlannedTask>) = remaining
                .into_iter()
                .partition(|t| t.dependencies.iter().any(|d| failed.contains(d)));

            for task in &skipped {
                let skip_result = TaskResult {
                    task_id: task.id.clone(),
                    persona: task.persona.clone(),
                    output: String::new(),
                    cost: 0.0,
                    duration_ms: 0,
                    success: false,
                    error: Some("Skipped: dependency failed".into()),
                };
                self.emit(TaskEvent::TaskFailed {
                    task_id: task.id.clone(),
                    error: "Skipped: dependency failed".into(),
                });
                failed.insert(task.id.clone());
                results.push(skip_result);
            }

            // Find tasks that are ready to execute (all deps satisfied).
            let (ready, not_ready): (Vec<PlannedTask>, Vec<PlannedTask>) = viable
                .into_iter()
                .partition(|t| t.dependencies.iter().all(|d| completed.contains(d)));

            remaining = not_ready;

            if ready.is_empty() {
                // No tasks are ready but some remain — implies unresolvable dependencies.
                break;
            }

            // Limit batch to max_parallel tasks per wave.
            let batch_size = ready.len().min(self.config.max_parallel);
            let batch: Vec<PlannedTask> = ready.into_iter().take(batch_size).collect();

            // Execute tasks in this wave. Tasks within a wave are independent
            // (no mutual dependencies) so ordering does not matter. We execute
            // them sequentially here because AiExecutor::execute returns a
            // non-Send future, preventing tokio::spawn. The wave structure
            // still provides correct dependency ordering across waves.
            for task in &batch {
                // Budget pre-flight check: estimate per-task cost and consult the
                // budget enforcer. Blocked tasks are marked as failed and skipped.
                if let Some(ref budget) = self.config.budget {
                    let task_cost_estimate = 0.10_f64;
                    let decision = budget.check(&task.id, task_cost_estimate);
                    match decision {
                        BudgetDecision::Blocked { .. } => {
                            self.emit(TaskEvent::TaskFailed {
                                task_id: task.id.clone(),
                                error: "Budget limit reached".into(),
                            });
                            let skip_result = TaskResult {
                                task_id: task.id.clone(),
                                persona: task.persona.clone(),
                                output: String::new(),
                                cost: 0.0,
                                duration_ms: 0,
                                success: false,
                                error: Some("Budget limit reached".into()),
                            };
                            failed.insert(task.id.clone());
                            results.push(skip_result);
                            continue;
                        }
                        _ => {} // Proceed or Warning — both allow execution
                    }
                }

                // Emit TaskStarted before execution.
                self.emit(TaskEvent::TaskStarted {
                    task_id: task.id.clone(),
                    description: task.description.clone(),
                    persona: task.persona.to_string(),
                });

                let persona = self
                    .registry
                    .get(&task.persona)
                    .or_else(|| self.registry.get(&PersonaKind::Implement))
                    .cloned()
                    .unwrap_or_else(|| {
                        // Last resort: synthesize a minimal persona so we never panic.
                        Persona {
                            name: "fallback".into(),
                            kind: PersonaKind::Implement,
                            description: "Fallback persona".into(),
                            system_prompt: String::new(),
                            model_tier: ModelTier::Mid,
                            tools: Vec::new(),
                            max_tokens: 4096,
                        }
                    });

                // Execute via pipeline (with validation + retry) or directly.
                let task_result = if let Some(ref pipe) = pipeline {
                    pipe.execute(task, &persona, &results).await
                } else {
                    let output = execute_with_persona_model(
                        &persona,
                        &task.description,
                        self.executor.as_ref(),
                        None,
                        task.model_override.as_deref(),
                    )
                    .await;

                    TaskResult {
                        task_id: task.id.clone(),
                        persona: task.persona.clone(),
                        output: output.content,
                        cost: output.cost,
                        duration_ms: output.duration_ms,
                        success: output.success,
                        error: output.error,
                    }
                };

                // Emit TaskCompleted or TaskFailed based on result.
                if task_result.success {
                    let preview = if task_result.output.len() > 200 {
                        format!("{}...", &task_result.output[..200])
                    } else {
                        task_result.output.clone()
                    };
                    self.emit(TaskEvent::TaskCompleted {
                        task_id: task_result.task_id.clone(),
                        duration_ms: task_result.duration_ms,
                        cost: task_result.cost,
                        output_preview: preview,
                    });
                    completed.insert(task_result.task_id.clone());
                } else {
                    self.emit(TaskEvent::TaskFailed {
                        task_id: task_result.task_id.clone(),
                        error: task_result
                            .error
                            .clone()
                            .unwrap_or_else(|| "Unknown error".into()),
                    });
                    failed.insert(task_result.task_id.clone());
                }

                results.push(task_result);
            }
        }

        let total_cost: f64 = results.iter().map(|r| r.cost).sum();
        let total_duration_ms = start.elapsed().as_millis() as u64;

        // Generate spec update summaries from successful results.
        let spec_updates: Vec<String> = results
            .iter()
            .filter(|r| r.success && !r.output.is_empty())
            .map(|r| format!("[{}] {}: completed", r.task_id, r.persona))
            .collect();

        let success_count = results.iter().filter(|r| r.success).count();
        let failure_count = results.iter().filter(|r| !r.success).count();

        // Emit AllComplete event.
        self.emit(TaskEvent::AllComplete {
            total_cost,
            total_duration_ms,
            success_count,
            failure_count,
        });

        CoordinatorResult {
            plan: plan.clone(),
            results,
            total_cost,
            total_duration_ms,
            spec_updates,
        }
    }

    /// Plan from a spec and then execute the plan.
    pub async fn execute_spec(&self, spec: &Spec) -> Result<CoordinatorResult, String> {
        let plan = self.plan_from_spec(spec).await?;
        plan.validate()?;
        Ok(self.execute_plan(&plan).await)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parse the AI response into a `TaskPlan`. Handles both raw JSON arrays
/// and markdown-wrapped code blocks.
fn parse_task_plan(response: &str) -> Result<TaskPlan, String> {
    // Strip markdown code fences if present.
    let content = response
        .trim()
        .strip_prefix("```json")
        .or_else(|| response.trim().strip_prefix("```"))
        .unwrap_or(response.trim());
    let content = content.strip_suffix("```").unwrap_or(content).trim();

    // Try to parse as a JSON array of task objects.
    let raw_tasks: Vec<RawTask> =
        serde_json::from_str(content).map_err(|e| format!("Failed to parse task plan: {e}"))?;

    let tasks = raw_tasks
        .into_iter()
        .map(|raw| PlannedTask {
            id: raw.id,
            description: raw.description,
            persona: parse_persona_kind(&raw.persona),
            dependencies: raw.dependencies,
            priority: raw.priority,
            model_override: None,
        })
        .collect();

    Ok(TaskPlan { tasks })
}

/// Intermediate type for JSON deserialization of planned tasks.
#[derive(Deserialize)]
struct RawTask {
    id: String,
    description: String,
    persona: String,
    #[serde(default)]
    dependencies: Vec<String>,
    #[serde(default = "default_priority")]
    priority: u8,
}

fn default_priority() -> u8 {
    3
}

/// Parse a persona kind string into the enum variant.
fn parse_persona_kind(s: &str) -> PersonaKind {
    match s.to_lowercase().as_str() {
        "investigate" => PersonaKind::Investigate,
        "implement" => PersonaKind::Implement,
        "verify" => PersonaKind::Verify,
        "critique" => PersonaKind::Critique,
        "debug" => PersonaKind::Debug,
        "code_review" | "codereview" => PersonaKind::CodeReview,
        other => PersonaKind::Custom(other.to_string()),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use hive_ai::types::{ChatResponse, FinishReason, TokenUsage};
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct MockExecutor {
        response: String,
        should_fail: bool,
        call_count: Arc<AtomicUsize>,
    }

    impl MockExecutor {
        fn new(response: &str) -> Self {
            Self {
                response: response.into(),
                should_fail: false,
                call_count: Arc::new(AtomicUsize::new(0)),
            }
        }

        fn failing() -> Self {
            Self {
                response: String::new(),
                should_fail: true,
                call_count: Arc::new(AtomicUsize::new(0)),
            }
        }
    }

    impl AiExecutor for MockExecutor {
        async fn execute(&self, _request: &ChatRequest) -> Result<ChatResponse, String> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            if self.should_fail {
                return Err("Mock failure".into());
            }
            Ok(ChatResponse {
                content: self.response.clone(),
                model: "mock-model".into(),
                usage: TokenUsage {
                    prompt_tokens: 50,
                    completion_tokens: 100,
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

    fn sample_plan() -> TaskPlan {
        TaskPlan {
            tasks: vec![
                PlannedTask {
                    id: "task-1".into(),
                    description: "Investigate the codebase".into(),
                    persona: PersonaKind::Investigate,
                    dependencies: vec![],
                    priority: 1,
                    model_override: None,
                },
                PlannedTask {
                    id: "task-2".into(),
                    description: "Implement the feature".into(),
                    persona: PersonaKind::Implement,
                    dependencies: vec!["task-1".into()],
                    priority: 2,
                    model_override: None,
                },
                PlannedTask {
                    id: "task-3".into(),
                    description: "Verify the implementation".into(),
                    persona: PersonaKind::Verify,
                    dependencies: vec!["task-2".into()],
                    priority: 3,
                    model_override: None,
                },
            ],
        }
    }

    #[test]
    fn default_config_values() {
        let config = CoordinatorConfig::default();
        assert_eq!(config.max_parallel, 4);
        assert_eq!(config.cost_limit, 10.0);
        assert_eq!(config.time_limit_secs, 600);
    }

    #[test]
    fn task_plan_root_tasks() {
        let plan = sample_plan();
        let roots = plan.root_tasks();
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].id, "task-1");
    }

    #[test]
    fn task_plan_dependents_of() {
        let plan = sample_plan();
        let dependents = plan.dependents_of("task-1");
        assert_eq!(dependents.len(), 1);
        assert_eq!(dependents[0].id, "task-2");

        let dependents_of_2 = plan.dependents_of("task-2");
        assert_eq!(dependents_of_2.len(), 1);
        assert_eq!(dependents_of_2[0].id, "task-3");

        let dependents_of_3 = plan.dependents_of("task-3");
        assert!(dependents_of_3.is_empty());
    }

    #[test]
    fn task_plan_validate_valid() {
        let plan = sample_plan();
        assert!(plan.validate().is_ok());
    }

    #[test]
    fn task_plan_validate_missing_dependency() {
        let plan = TaskPlan {
            tasks: vec![PlannedTask {
                id: "task-1".into(),
                description: "Do something".into(),
                persona: PersonaKind::Implement,
                dependencies: vec!["nonexistent".into()],
                priority: 1,
                model_override: None,
            }],
        };
        let result = plan.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unknown task"));
    }

    #[test]
    fn task_plan_validate_self_dependency() {
        let plan = TaskPlan {
            tasks: vec![PlannedTask {
                id: "task-1".into(),
                description: "Do something".into(),
                persona: PersonaKind::Implement,
                dependencies: vec!["task-1".into()],
                priority: 1,
                model_override: None,
            }],
        };
        let result = plan.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("depends on itself"));
    }

    #[test]
    fn task_plan_validate_cycle() {
        let plan = TaskPlan {
            tasks: vec![
                PlannedTask {
                    id: "a".into(),
                    description: "A".into(),
                    persona: PersonaKind::Investigate,
                    dependencies: vec!["b".into()],
                    priority: 1,
                    model_override: None,
                },
                PlannedTask {
                    id: "b".into(),
                    description: "B".into(),
                    persona: PersonaKind::Implement,
                    dependencies: vec!["a".into()],
                    priority: 1,
                    model_override: None,
                },
            ],
        };
        let result = plan.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("cycle"));
    }

    #[test]
    fn parse_task_plan_from_json() {
        let json = r#"[
            {"id": "t1", "description": "Investigate", "persona": "investigate", "dependencies": [], "priority": 1},
            {"id": "t2", "description": "Implement", "persona": "implement", "dependencies": ["t1"], "priority": 2}
        ]"#;

        let plan = parse_task_plan(json).unwrap();
        assert_eq!(plan.tasks.len(), 2);
        assert_eq!(plan.tasks[0].persona, PersonaKind::Investigate);
        assert_eq!(plan.tasks[1].dependencies, vec!["t1"]);
    }

    #[test]
    fn parse_task_plan_from_markdown_fenced() {
        let md = "```json\n[\n{\"id\":\"t1\",\"description\":\"Do it\",\"persona\":\"verify\",\"dependencies\":[],\"priority\":1}\n]\n```";
        let plan = parse_task_plan(md).unwrap();
        assert_eq!(plan.tasks.len(), 1);
        assert_eq!(plan.tasks[0].persona, PersonaKind::Verify);
    }

    #[test]
    fn parse_task_plan_invalid_json() {
        let result = parse_task_plan("not json at all");
        assert!(result.is_err());
    }

    #[test]
    fn parse_persona_kind_all_variants() {
        assert_eq!(parse_persona_kind("investigate"), PersonaKind::Investigate);
        assert_eq!(parse_persona_kind("implement"), PersonaKind::Implement);
        assert_eq!(parse_persona_kind("verify"), PersonaKind::Verify);
        assert_eq!(parse_persona_kind("critique"), PersonaKind::Critique);
        assert_eq!(parse_persona_kind("debug"), PersonaKind::Debug);
        assert_eq!(parse_persona_kind("code_review"), PersonaKind::CodeReview);
        assert_eq!(parse_persona_kind("codereview"), PersonaKind::CodeReview);
        assert_eq!(
            parse_persona_kind("something_else"),
            PersonaKind::Custom("something_else".into())
        );
    }

    #[tokio::test]
    async fn execute_plan_runs_tasks_in_order() {
        let executor = MockExecutor::new("Task output");
        let call_count = executor.call_count.clone();
        let coordinator = Coordinator::new(CoordinatorConfig::default(), executor);

        let plan = sample_plan();
        let result = coordinator.execute_plan(&plan).await;

        // All 3 tasks should have been executed.
        assert_eq!(result.results.len(), 3);
        assert_eq!(result.successful_tasks(), 3);
        assert_eq!(result.failed_tasks(), 0);
        assert_eq!(call_count.load(Ordering::SeqCst), 3);

        // task-1 should appear before task-2, and task-2 before task-3.
        let ids: Vec<&str> = result.results.iter().map(|r| r.task_id.as_str()).collect();
        let pos_1 = ids.iter().position(|id| *id == "task-1").unwrap();
        let pos_2 = ids.iter().position(|id| *id == "task-2").unwrap();
        let pos_3 = ids.iter().position(|id| *id == "task-3").unwrap();
        assert!(pos_1 < pos_2);
        assert!(pos_2 < pos_3);
    }

    #[tokio::test]
    async fn execute_plan_handles_failures() {
        let executor = MockExecutor::failing();
        let coordinator = Coordinator::new(CoordinatorConfig::default(), executor);

        let plan = TaskPlan {
            tasks: vec![PlannedTask {
                id: "t1".into(),
                description: "Will fail".into(),
                persona: PersonaKind::Implement,
                dependencies: vec![],
                priority: 1,
                model_override: None,
            }],
        };

        let result = coordinator.execute_plan(&plan).await;
        assert_eq!(result.results.len(), 1);
        assert_eq!(result.failed_tasks(), 1);
        assert!(result.results[0].error.is_some());
    }

    #[tokio::test]
    async fn execute_plan_parallel_independent_tasks() {
        let executor = MockExecutor::new("output");
        let call_count = executor.call_count.clone();
        let coordinator = Coordinator::new(CoordinatorConfig::default(), executor);

        // Two independent tasks — should be dispatched in the same wave.
        let plan = TaskPlan {
            tasks: vec![
                PlannedTask {
                    id: "a".into(),
                    description: "Task A".into(),
                    persona: PersonaKind::Investigate,
                    dependencies: vec![],
                    priority: 1,
                    model_override: None,
                },
                PlannedTask {
                    id: "b".into(),
                    description: "Task B".into(),
                    persona: PersonaKind::Implement,
                    dependencies: vec![],
                    priority: 1,
                    model_override: None,
                },
            ],
        };

        let result = coordinator.execute_plan(&plan).await;
        assert_eq!(result.results.len(), 2);
        assert_eq!(call_count.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn coordinator_result_metrics() {
        let executor = MockExecutor::new("output");
        let coordinator = Coordinator::new(CoordinatorConfig::default(), executor);
        let plan = sample_plan();

        let result = coordinator.execute_plan(&plan).await;
        assert!(result.total_duration_ms > 0 || result.total_duration_ms == 0);
        assert_eq!(result.spec_updates.len(), result.successful_tasks());
    }

    #[test]
    fn coordinator_result_serialization() {
        let result = CoordinatorResult {
            plan: TaskPlan { tasks: vec![] },
            results: vec![TaskResult {
                task_id: "t1".into(),
                persona: PersonaKind::Implement,
                output: "done".into(),
                cost: 0.01,
                duration_ms: 500,
                success: true,
                error: None,
            }],
            total_cost: 0.01,
            total_duration_ms: 500,
            spec_updates: vec!["[t1] implement: completed".into()],
        };

        let json = serde_json::to_string(&result).unwrap();
        let deserialized: CoordinatorResult = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.results.len(), 1);
        assert_eq!(deserialized.total_cost, 0.01);
    }

    #[test]
    fn empty_plan_validates() {
        let plan = TaskPlan { tasks: vec![] };
        assert!(plan.validate().is_ok());
    }

    // -----------------------------------------------------------------------
    // TaskEvent tests
    // -----------------------------------------------------------------------

    #[test]
    fn task_event_serialization() {
        let event = TaskEvent::PlanCreated {
            plan_id: "plan-1".into(),
            tasks: vec![TaskEventInfo {
                id: "t1".into(),
                description: "Do something".into(),
                persona: "Implement".into(),
                dependencies: vec![],
                model_override: None,
            }],
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("PlanCreated"));
        assert!(json.contains("plan-1"));

        let completed = TaskEvent::TaskCompleted {
            task_id: "t1".into(),
            duration_ms: 500,
            cost: 0.05,
            output_preview: "done".into(),
        };
        let json2 = serde_json::to_string(&completed).unwrap();
        assert!(json2.contains("TaskCompleted"));
        assert!(json2.contains("0.05"));
    }

    #[test]
    fn task_event_broadcast_channel() {
        let (tx, mut rx1) = broadcast::channel::<TaskEvent>(16);
        let mut rx2 = tx.subscribe();

        let event = TaskEvent::TaskStarted {
            task_id: "t1".into(),
            description: "Test task".into(),
            persona: "Implement".into(),
        };

        tx.send(event.clone()).unwrap();

        let received1 = rx1.try_recv().unwrap();
        let received2 = rx2.try_recv().unwrap();

        // Verify both receivers got the event.
        assert!(matches!(received1, TaskEvent::TaskStarted { task_id, .. } if task_id == "t1"));
        assert!(matches!(received2, TaskEvent::TaskStarted { task_id, .. } if task_id == "t1"));
    }

    #[test]
    fn task_event_info_fields() {
        let info = TaskEventInfo {
            id: "task-42".into(),
            description: "Investigate auth module".into(),
            persona: "Investigate".into(),
            dependencies: vec!["task-1".into(), "task-2".into()],
            model_override: None,
        };
        assert_eq!(info.id, "task-42");
        assert_eq!(info.dependencies.len(), 2);
    }

    #[test]
    fn coordinator_subscribe_returns_receiver() {
        let executor = MockExecutor::new("output");
        let coordinator = Coordinator::new(CoordinatorConfig::default(), executor);
        let _rx = coordinator.subscribe();
        // Just verify subscribe() works without panicking.
    }

    #[tokio::test]
    async fn execute_plan_emits_events_in_order() {
        let executor = MockExecutor::new("Task output");
        let coordinator = Coordinator::new(CoordinatorConfig::default(), executor);
        let mut rx = coordinator.subscribe();

        let plan = TaskPlan {
            tasks: vec![
                PlannedTask {
                    id: "t1".into(),
                    description: "First task".into(),
                    persona: PersonaKind::Investigate,
                    dependencies: vec![],
                    priority: 1,
                    model_override: None,
                },
                PlannedTask {
                    id: "t2".into(),
                    description: "Second task".into(),
                    persona: PersonaKind::Implement,
                    dependencies: vec!["t1".into()],
                    priority: 2,
                    model_override: None,
                },
            ],
        };

        let _result = coordinator.execute_plan(&plan).await;

        // Collect all events from the receiver.
        let mut events = Vec::new();
        while let Ok(event) = rx.try_recv() {
            events.push(event);
        }

        // Expected sequence: PlanCreated, TaskStarted(t1), TaskCompleted(t1),
        //                    TaskStarted(t2), TaskCompleted(t2), AllComplete
        assert!(events.len() >= 6, "Expected at least 6 events, got {}", events.len());

        assert!(matches!(&events[0], TaskEvent::PlanCreated { .. }));
        assert!(matches!(&events[1], TaskEvent::TaskStarted { task_id, .. } if task_id == "t1"));
        assert!(matches!(&events[2], TaskEvent::TaskCompleted { task_id, .. } if task_id == "t1"));
        assert!(matches!(&events[3], TaskEvent::TaskStarted { task_id, .. } if task_id == "t2"));
        assert!(matches!(&events[4], TaskEvent::TaskCompleted { task_id, .. } if task_id == "t2"));
        assert!(matches!(&events[5], TaskEvent::AllComplete { success_count: 2, failure_count: 0, .. }));
    }

    #[tokio::test]
    async fn execute_plan_emits_task_failed_event() {
        let executor = MockExecutor::failing();
        let coordinator = Coordinator::new(CoordinatorConfig::default(), executor);
        let mut rx = coordinator.subscribe();

        let plan = TaskPlan {
            tasks: vec![PlannedTask {
                id: "t1".into(),
                description: "Will fail".into(),
                persona: PersonaKind::Implement,
                dependencies: vec![],
                priority: 1,
                model_override: None,
            }],
        };

        let _result = coordinator.execute_plan(&plan).await;

        let mut events = Vec::new();
        while let Ok(event) = rx.try_recv() {
            events.push(event);
        }

        // Expected: PlanCreated, TaskStarted, TaskFailed, AllComplete
        assert!(events.len() >= 4, "Expected at least 4 events, got {}", events.len());
        assert!(matches!(&events[0], TaskEvent::PlanCreated { .. }));
        assert!(matches!(&events[1], TaskEvent::TaskStarted { task_id, .. } if task_id == "t1"));
        assert!(matches!(&events[2], TaskEvent::TaskFailed { task_id, .. } if task_id == "t1"));
        assert!(matches!(&events[3], TaskEvent::AllComplete { success_count: 0, failure_count: 1, .. }));
    }
}
