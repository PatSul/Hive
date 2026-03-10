//! Task handler — routes A2A messages to Hive orchestrators.
//!
//! Receives incoming A2A [`Message`]s, resolves which Hive skill to invoke,
//! creates an A2A [`Task`], executes orchestration, and streams status
//! updates through a broadcast channel.
//!
//! # Design note
//!
//! The [`AiExecutor`] trait uses `async fn` whose future is not guaranteed
//! `Send`, which prevents `tokio::spawn`. Orchestration is therefore
//! awaited inline within [`HiveTaskHandler::handle_message`]. The task is
//! stored in `active_tasks` *before* execution begins (in `Working` state)
//! so concurrent callers can observe it, and updated in-place when
//! orchestration completes.

use std::collections::HashMap;
use std::sync::Arc;

use a2a_rs::{Artifact, Message, Part, Task, TaskState, TaskStatus, TaskStatusUpdateEvent};
use chrono::Utc;
use serde_json::{Map, Value};
use tokio::sync::{broadcast, Mutex};
use uuid::Uuid;

use hive_agents::coordinator::{Coordinator, CoordinatorConfig};
use hive_agents::hivemind::{AiExecutor, HiveMind, HiveMindConfig};
use hive_agents::queen::Queen;
use hive_agents::specs::Spec;
use hive_agents::swarm::SwarmConfig;
use hive_ai::providers::AiProvider;
use hive_ai::types::{ChatMessage, ChatRequest, ChatResponse, MessageRole};

use crate::agent_card::{
    is_supported_skill, SKILL_COORDINATOR, SKILL_HIVEMIND, SKILL_QUEEN, SKILL_SINGLE,
};
use crate::bridge;
use crate::config::ServerDefaults;
use crate::error::A2aError;

// ---------------------------------------------------------------------------
// ArcExecutor — bridge to pass Arc<E> where E: AiExecutor is expected by value
// ---------------------------------------------------------------------------

/// Adapter that forwards `AiExecutor` calls to a concrete `AiProvider`.
///
/// This lets the A2A server run against the same provider abstraction the UI
/// already uses without teaching the rest of the orchestration stack about
/// `AiProvider`.
pub struct ProviderExecutor {
    provider: Arc<dyn AiProvider>,
    default_model: String,
}

impl ProviderExecutor {
    /// Create a new provider-backed executor.
    pub fn new(provider: Arc<dyn AiProvider>, default_model: impl Into<String>) -> Self {
        Self {
            provider,
            default_model: default_model.into(),
        }
    }
}

impl AiExecutor for ProviderExecutor {
    async fn execute(&self, request: &ChatRequest) -> Result<ChatResponse, String> {
        let mut request = request.clone();
        if request.model.trim().is_empty() {
            request.model = self.default_model.clone();
        }

        self.provider
            .chat(&request)
            .await
            .map_err(|e| e.to_string())
    }
}

/// Thin wrapper that lets us pass `Arc<E>` to APIs that consume an
/// `E: AiExecutor` by value (HiveMind::new, Coordinator::new).
struct ArcExecutor<E: AiExecutor>(Arc<E>);

impl<E: AiExecutor> AiExecutor for ArcExecutor<E> {
    async fn execute(&self, request: &ChatRequest) -> Result<ChatResponse, String> {
        self.0.execute(request).await
    }
}

// ---------------------------------------------------------------------------
// Active task tracking
// ---------------------------------------------------------------------------

/// An active task being processed by the handler.
pub struct ActiveTask {
    /// The A2A task (updated as orchestration progresses).
    pub a2a_task: Task,
    /// Broadcast sender for status update events.
    pub event_tx: broadcast::Sender<TaskStatusUpdateEvent>,
}

// ---------------------------------------------------------------------------
// HiveTaskHandler
// ---------------------------------------------------------------------------

/// The Hive task handler — routes A2A messages to Hive orchestrators.
///
/// Generic over `E: AiExecutor` so that any AI provider backend can be
/// plugged in. Holds a shared `Arc<E>` and clones it into each
/// orchestration invocation.
pub struct HiveTaskHandler<E: AiExecutor + 'static> {
    executor: Arc<E>,
    defaults: ServerDefaults,
    active_tasks: Arc<Mutex<HashMap<String, ActiveTask>>>,
}

impl<E: AiExecutor + 'static> HiveTaskHandler<E> {
    /// Create a new task handler with the given executor and server defaults.
    pub fn new(executor: Arc<E>, defaults: ServerDefaults) -> Self {
        Self {
            executor,
            defaults,
            active_tasks: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Handle an incoming A2A message for the given task ID.
    ///
    /// 1. Extracts text from the message.
    /// 2. Resolves which Hive skill to invoke.
    /// 3. Creates an A2A `Task` in the `Working` state and stores it.
    /// 4. Awaits orchestration inline (AiExecutor futures are not `Send`).
    /// 5. Updates the task to `Completed` or `Failed` and sends a final
    ///    status update event.
    /// 6. Returns the completed `Task`.
    pub async fn handle_message(&self, task_id: &str, message: &Message) -> Result<Task, A2aError> {
        let task_text = bridge::extract_message_text(message);
        if task_text.trim().is_empty() {
            return Err(A2aError::Bridge("Message contains no text content".into()));
        }

        let skill_id = resolve_skill(
            &task_text,
            message.metadata.as_ref(),
            &self.defaults.default_skill,
        );

        if !is_supported_skill(&skill_id) {
            return Err(A2aError::UnsupportedSkill(skill_id));
        }

        let context_id = message
            .context_id
            .clone()
            .unwrap_or_else(|| Uuid::new_v4().to_string());

        // Build the initial working status message.
        let status_msg = Message::agent_text(
            format!("Processing with skill: {}", skill_id),
            Uuid::new_v4().to_string(),
        );

        let a2a_task = Task {
            id: task_id.to_string(),
            context_id: context_id.clone(),
            status: TaskStatus {
                state: TaskState::Working,
                message: Some(status_msg),
                timestamp: Some(Utc::now()),
            },
            artifacts: None,
            history: Some(vec![message.clone()]),
            metadata: None,
            kind: "task".to_string(),
        };

        let (event_tx, _) = broadcast::channel::<TaskStatusUpdateEvent>(64);

        // Store the task in Working state before execution begins.
        {
            let mut tasks = self.active_tasks.lock().await;
            tasks.insert(
                task_id.to_string(),
                ActiveTask {
                    a2a_task: a2a_task.clone(),
                    event_tx: event_tx.clone(),
                },
            );
        }

        // Execute the skill inline (AiExecutor futures are not Send).
        let result = execute_skill(
            &skill_id,
            &task_text,
            Arc::clone(&self.executor),
            &self.defaults,
        )
        .await;

        // Update the stored task with the result.
        let final_task = {
            let mut tasks = self.active_tasks.lock().await;
            if let Some(active) = tasks.get_mut(task_id) {
                match result {
                    Ok(artifact) => {
                        active.a2a_task.add_artifact(artifact);
                        let done_msg = Message::agent_text(
                            "Task completed successfully".into(),
                            Uuid::new_v4().to_string(),
                        );
                        active
                            .a2a_task
                            .update_status(TaskState::Completed, Some(done_msg.clone()));

                        let _ = event_tx.send(TaskStatusUpdateEvent {
                            task_id: task_id.to_string(),
                            context_id: context_id.clone(),
                            kind: "status-update".to_string(),
                            status: TaskStatus {
                                state: TaskState::Completed,
                                message: Some(done_msg),
                                timestamp: Some(Utc::now()),
                            },
                            final_: true,
                            metadata: None,
                        });
                    }
                    Err(ref e) => {
                        let err_msg = Message::agent_text(
                            format!("Task failed: {}", e),
                            Uuid::new_v4().to_string(),
                        );
                        active
                            .a2a_task
                            .update_status(TaskState::Failed, Some(err_msg.clone()));

                        let _ = event_tx.send(TaskStatusUpdateEvent {
                            task_id: task_id.to_string(),
                            context_id: context_id.clone(),
                            kind: "status-update".to_string(),
                            status: TaskStatus {
                                state: TaskState::Failed,
                                message: Some(err_msg),
                                timestamp: Some(Utc::now()),
                            },
                            final_: true,
                            metadata: None,
                        });
                    }
                }
                active.a2a_task.clone()
            } else {
                a2a_task
            }
        };

        Ok(final_task)
    }

    /// Retrieve a task by ID from the active tasks map.
    pub async fn get_task(&self, task_id: &str) -> Result<Task, A2aError> {
        let tasks = self.active_tasks.lock().await;
        tasks
            .get(task_id)
            .map(|active| active.a2a_task.clone())
            .ok_or_else(|| A2aError::TaskNotFound(task_id.to_string()))
    }

    /// Subscribe to status update events for a given task.
    pub async fn subscribe(
        &self,
        task_id: &str,
    ) -> Result<broadcast::Receiver<TaskStatusUpdateEvent>, A2aError> {
        let tasks = self.active_tasks.lock().await;
        tasks
            .get(task_id)
            .map(|active| active.event_tx.subscribe())
            .ok_or_else(|| A2aError::TaskNotFound(task_id.to_string()))
    }
}

// ---------------------------------------------------------------------------
// Skill resolution
// ---------------------------------------------------------------------------

/// Resolve which Hive skill to invoke.
///
/// Resolution order:
/// 1. Explicit `skill_id` in message metadata.
/// 2. Inferred from message text via keyword heuristics.
/// 3. Falls back to the provided default.
pub fn resolve_skill(text: &str, metadata: Option<&Map<String, Value>>, default: &str) -> String {
    // 1. Check explicit skill_id in metadata.
    if let Some(skill_id) = bridge::extract_skill_id(metadata) {
        return skill_id;
    }

    // 2. Infer from text if non-empty.
    if !text.trim().is_empty() {
        return bridge::infer_skill(text).to_string();
    }

    // 3. Fall back to default.
    default.to_string()
}

// ---------------------------------------------------------------------------
// Skill execution
// ---------------------------------------------------------------------------

/// Execute a specific Hive skill and return the resulting A2A artifact.
///
/// Dispatches to the appropriate orchestrator based on `skill_id`:
/// - `"single"` — one-shot ChatRequest via the executor.
/// - `"hivemind"` — HiveMind multi-agent pipeline.
/// - `"coordinator"` — dependency-ordered parallel task coordinator.
/// - `"queen"` — multi-team swarm orchestration.
///
/// Takes `Arc<E>` because HiveMind/Coordinator/Queen consume their executor
/// by value; the [`ArcExecutor`] wrapper bridges ownership.
pub async fn execute_skill<E: AiExecutor + 'static>(
    skill_id: &str,
    task_text: &str,
    executor: Arc<E>,
    defaults: &ServerDefaults,
) -> Result<Artifact, A2aError> {
    match skill_id {
        SKILL_SINGLE => execute_single(task_text, executor.as_ref()).await,
        SKILL_HIVEMIND => execute_hivemind(task_text, executor, defaults).await,
        SKILL_COORDINATOR => execute_coordinator(task_text, executor, defaults).await,
        SKILL_QUEEN => execute_queen(task_text, executor, defaults).await,
        other => Err(A2aError::UnsupportedSkill(other.to_string())),
    }
}

/// Execute a single-shot AI call and wrap the response in an Artifact.
async fn execute_single<E: AiExecutor>(
    task_text: &str,
    executor: &E,
) -> Result<Artifact, A2aError> {
    let request = ChatRequest {
        messages: vec![ChatMessage::text(MessageRole::User, task_text)],
        model: String::new(), // Let the executor pick the default model.
        max_tokens: 4096,
        temperature: Some(0.7),
        system_prompt: Some("You are a helpful AI coding assistant.".into()),
        tools: None,
        cache_system_prompt: false,
    };

    let response = executor
        .execute(&request)
        .await
        .map_err(A2aError::Provider)?;

    Ok(Artifact {
        artifact_id: format!("single-{}", Uuid::new_v4()),
        name: Some("Single Agent Result".into()),
        description: Some("One-shot AI response".into()),
        parts: vec![Part::text(response.content)],
        metadata: None,
    })
}

/// Execute a HiveMind multi-agent pipeline.
async fn execute_hivemind<E: AiExecutor + 'static>(
    task_text: &str,
    executor: Arc<E>,
    defaults: &ServerDefaults,
) -> Result<Artifact, A2aError> {
    let config = HiveMindConfig {
        cost_limit_usd: defaults.max_budget_usd,
        time_limit_secs: defaults.max_time_seconds,
        ..HiveMindConfig::default()
    };

    let hivemind = HiveMind::new(config, ArcExecutor(executor));
    let result = hivemind.execute(task_text).await;

    Ok(bridge::orchestration_result_to_artifact(&result))
}

/// Execute a Coordinator task pipeline.
async fn execute_coordinator<E: AiExecutor + 'static>(
    task_text: &str,
    executor: Arc<E>,
    defaults: &ServerDefaults,
) -> Result<Artifact, A2aError> {
    let config = CoordinatorConfig {
        cost_limit: defaults.max_budget_usd,
        time_limit_secs: defaults.max_time_seconds,
        ..CoordinatorConfig::default()
    };

    let coordinator = Coordinator::new(config, ArcExecutor(executor));

    // The coordinator works with Spec objects. We create a minimal spec
    // from the task text so it can plan and execute.
    let spec = Spec::new("a2a-task", "A2A Task", task_text);

    let result = coordinator
        .execute_spec(&spec)
        .await
        .map_err(A2aError::Provider)?;

    Ok(bridge::coordinator_result_to_artifact(&result))
}

/// Execute a Queen swarm orchestration.
async fn execute_queen<E: AiExecutor + 'static>(
    task_text: &str,
    executor: Arc<E>,
    defaults: &ServerDefaults,
) -> Result<Artifact, A2aError> {
    let config = SwarmConfig {
        total_cost_limit_usd: defaults.max_budget_usd,
        total_time_limit_secs: defaults.max_time_seconds,
        ..SwarmConfig::default()
    };

    let queen = Queen::new(config, executor);
    let result = queen.execute(task_text).await.map_err(A2aError::Provider)?;

    Ok(bridge::swarm_result_to_artifact(&result))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use hive_ai::types::{FinishReason, TokenUsage};

    // -- MockExecutor -------------------------------------------------------

    struct MockExecutor;

    impl AiExecutor for MockExecutor {
        async fn execute(&self, request: &ChatRequest) -> Result<ChatResponse, String> {
            Ok(ChatResponse {
                content: format!(
                    "Mock: {}",
                    request
                        .messages
                        .last()
                        .map(|m| m.content.as_str())
                        .unwrap_or("")
                ),
                model: "mock".into(),
                usage: TokenUsage::default(),
                finish_reason: FinishReason::Stop,
                thinking: None,
                tool_calls: None,
            })
        }
    }

    // -- resolve_skill tests ------------------------------------------------

    #[test]
    fn test_resolve_skill_explicit_metadata() {
        let mut metadata = serde_json::Map::new();
        metadata.insert("skill_id".into(), "coordinator".into());
        assert_eq!(
            resolve_skill("any text", Some(&metadata), "hivemind"),
            "coordinator"
        );
    }

    #[test]
    fn test_resolve_skill_inferred() {
        // "Review this code for bugs" is short (<100 chars), no multi-agent keywords.
        assert_eq!(
            resolve_skill("Review this code for bugs", None, "hivemind"),
            "single"
        );
    }

    #[test]
    fn test_resolve_skill_default_fallback() {
        assert_eq!(resolve_skill("", None, "coordinator"), "coordinator");
    }

    #[test]
    fn test_resolve_skill_inferred_queen() {
        assert_eq!(
            resolve_skill("Deploy across all teams", None, "single"),
            "queen"
        );
    }

    #[test]
    fn test_resolve_skill_inferred_coordinator() {
        assert_eq!(
            resolve_skill("Follow these steps carefully", None, "single"),
            "coordinator"
        );
    }

    #[test]
    fn test_resolve_skill_inferred_hivemind() {
        assert_eq!(
            resolve_skill("Architect a new auth system", None, "single"),
            "hivemind"
        );
    }

    // -- execute_skill tests ------------------------------------------------

    #[tokio::test]
    async fn test_execute_single_skill() {
        let executor = Arc::new(MockExecutor);
        let defaults = ServerDefaults::default();
        let artifact = execute_skill("single", "Hello world", executor, &defaults).await;
        assert!(artifact.is_ok());
        let artifact = artifact.unwrap();
        assert!(artifact.artifact_id.starts_with("single-"));
        assert_eq!(artifact.name, Some("Single Agent Result".into()));

        // Verify artifact contains mock response text.
        let text = bridge::artifact_to_text(&artifact);
        assert!(text.contains("Mock: Hello world"));
    }

    #[tokio::test]
    async fn test_execute_unsupported_skill() {
        let executor = Arc::new(MockExecutor);
        let defaults = ServerDefaults::default();
        let result = execute_skill("nonexistent", "test", executor, &defaults).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            A2aError::UnsupportedSkill(name) => assert_eq!(name, "nonexistent"),
            other => panic!("Expected UnsupportedSkill, got: {:?}", other),
        }
    }

    // -- HiveTaskHandler tests ----------------------------------------------

    #[tokio::test]
    async fn test_handle_message_creates_task() {
        let handler = HiveTaskHandler::new(Arc::new(MockExecutor), ServerDefaults::default());
        let message = a2a_rs::Message::user_text("Hello".into(), uuid::Uuid::new_v4().to_string());
        let task = handler.handle_message("task-1", &message).await;
        assert!(task.is_ok());
        let task = task.unwrap();
        assert_eq!(task.id, "task-1");
        // Since execution is inline, task should already be completed.
        assert_eq!(task.status.state, TaskState::Completed);
    }

    #[tokio::test]
    async fn test_handle_message_empty_text_rejected() {
        let handler = HiveTaskHandler::new(Arc::new(MockExecutor), ServerDefaults::default());
        let message = Message {
            role: a2a_rs::Role::User,
            parts: vec![],
            metadata: None,
            reference_task_ids: None,
            message_id: Uuid::new_v4().to_string(),
            task_id: None,
            context_id: None,
            kind: "message".into(),
        };
        let result = handler.handle_message("task-empty", &message).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_task() {
        let handler = HiveTaskHandler::new(Arc::new(MockExecutor), ServerDefaults::default());
        let message = a2a_rs::Message::user_text("Hello".into(), uuid::Uuid::new_v4().to_string());
        handler.handle_message("task-1", &message).await.unwrap();

        let task = handler.get_task("task-1").await;
        assert!(task.is_ok());
        assert_eq!(task.unwrap().id, "task-1");

        let missing = handler.get_task("nonexistent").await;
        assert!(missing.is_err());
        match missing.unwrap_err() {
            A2aError::TaskNotFound(id) => assert_eq!(id, "nonexistent"),
            other => panic!("Expected TaskNotFound, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_subscribe_existing_task() {
        let handler = HiveTaskHandler::new(Arc::new(MockExecutor), ServerDefaults::default());
        let message = a2a_rs::Message::user_text("Hello".into(), uuid::Uuid::new_v4().to_string());
        handler.handle_message("task-sub", &message).await.unwrap();

        let rx = handler.subscribe("task-sub").await;
        assert!(rx.is_ok());
    }

    #[tokio::test]
    async fn test_subscribe_missing_task() {
        let handler = HiveTaskHandler::new(Arc::new(MockExecutor), ServerDefaults::default());
        let result = handler.subscribe("no-such-task").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_handle_message_stores_artifact() {
        let handler = HiveTaskHandler::new(Arc::new(MockExecutor), ServerDefaults::default());
        let message =
            a2a_rs::Message::user_text("Hello world".into(), uuid::Uuid::new_v4().to_string());
        let task = handler.handle_message("task-art", &message).await.unwrap();

        // Task should have an artifact from the single skill execution.
        assert!(task.artifacts.is_some());
        let artifacts = task.artifacts.unwrap();
        assert_eq!(artifacts.len(), 1);
        assert!(artifacts[0].artifact_id.starts_with("single-"));

        let text = bridge::artifact_to_text(&artifacts[0]);
        assert!(text.contains("Mock: Hello world"));
    }

    #[tokio::test]
    async fn test_handle_message_with_explicit_skill_metadata() {
        let handler = HiveTaskHandler::new(Arc::new(MockExecutor), ServerDefaults::default());

        let mut metadata = Map::new();
        metadata.insert("skill_id".into(), Value::String("single".into()));

        let message = Message {
            role: a2a_rs::Role::User,
            parts: vec![Part::text(
                "Do something complex with teams and swarm".into(),
            )],
            metadata: Some(metadata),
            reference_task_ids: None,
            message_id: Uuid::new_v4().to_string(),
            task_id: None,
            context_id: None,
            kind: "message".into(),
        };

        // Despite text containing queen keywords, explicit metadata overrides.
        let task = handler.handle_message("task-meta", &message).await.unwrap();
        assert_eq!(task.id, "task-meta");
        assert_eq!(task.status.state, TaskState::Completed);

        // Single skill produces artifact starting with "single-".
        let artifact = &task.artifacts.unwrap()[0];
        assert!(artifact.artifact_id.starts_with("single-"));
    }

    #[tokio::test]
    async fn test_handle_message_preserves_history() {
        let handler = HiveTaskHandler::new(Arc::new(MockExecutor), ServerDefaults::default());
        let message =
            a2a_rs::Message::user_text("Test history".into(), uuid::Uuid::new_v4().to_string());
        let task = handler.handle_message("task-hist", &message).await.unwrap();

        // History should contain the original user message plus the completion message.
        assert!(task.history.is_some());
        let history = task.history.unwrap();
        assert!(history.len() >= 1);
        // First message should be the user's.
        assert_eq!(history[0].role, a2a_rs::Role::User);
    }
}
