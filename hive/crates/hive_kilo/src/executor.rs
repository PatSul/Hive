//! `KiloAiExecutor` — implements [`hive_agents::hivemind::AiExecutor`] for Kilo.
//!
//! While [`crate::provider::KiloAiProvider`] fits into `AiService`'s provider
//! table (stateless, ephemeral sessions per call), `KiloAiExecutor` is
//! designed for use *inside* `HiveMind` and `Coordinator` where the executor
//! is called many times for a single task across multiple agent roles.
//!
//! # Session forking strategy
//!
//! ```text
//! HiveMind task begins
//!   └── KiloAiExecutor::start_task() → create root KiloSession
//!         ├── Architect role   → execute on root session
//!         ├── Coder role       → fork(root) → execute on fork
//!         ├── Reviewer role    → fork(root) → execute on fork
//!         └── ... each parallel role gets its own fork
//!   └── KiloAiExecutor::end_task() → close root + all forks
//! ```
//!
//! For the simple `AiExecutor::execute` path (called without lifecycle hooks),
//! the executor falls back to the ephemeral strategy: create → execute → close.

use std::sync::Arc;

use hive_agents::hivemind::AiExecutor;
use hive_ai::types::{
    ChatRequest, ChatResponse, FinishReason, MessageRole, TokenUsage, ToolCall,
};
use tokio::sync::Mutex;
use tracing::{debug, warn};

use crate::client::KiloClient;
use crate::config::KiloConfig;
use crate::events::KiloEvent;
use crate::session::{CreateSessionRequest, KiloMessage, KiloSession};

// ---------------------------------------------------------------------------
// KiloAiExecutor
// ---------------------------------------------------------------------------

/// An [`AiExecutor`] that routes single-task AI calls through a Kilo session.
///
/// Registered with `HiveMind::new(KiloAiExecutor::new(config))` from the
/// application's wiring layer.
pub struct KiloAiExecutor {
    client: Arc<KiloClient>,
    /// Shared root session for the current task (if any).
    /// Guarded by a `Mutex` so forked-session bookkeeping is safe across
    /// the `async` calls the HiveMind makes concurrently.
    root_session: Mutex<Option<KiloSession>>,
}

impl KiloAiExecutor {
    /// Create an executor from an explicit config.
    pub fn new(config: KiloConfig) -> Self {
        Self {
            client: Arc::new(KiloClient::new(config)),
            root_session: Mutex::new(None),
        }
    }

    /// Convenience constructor used in tests.
    pub fn from_client(client: Arc<KiloClient>) -> Self {
        Self {
            client,
            root_session: Mutex::new(None),
        }
    }

    // -----------------------------------------------------------------------
    // Lifecycle helpers (optional — HiveMind orchestration layer calls these)
    // -----------------------------------------------------------------------

    /// Start a rooted-session task.  Creates a root Kilo session that will be
    /// shared (via forking) across all agent roles in the current HiveMind run.
    ///
    /// Call before the first `execute()` in a HiveMind task.
    pub async fn start_task(
        &self,
        model: Option<&str>,
        system: Option<&str>,
    ) -> crate::error::KiloResult<()> {
        let session = self
            .client
            .create_session(CreateSessionRequest {
                model: model.map(str::to_owned),
                system: system.map(str::to_owned),
                max_tokens: None,
                workspace: None,
            })
            .await?;
        debug!("KiloAiExecutor: root session started — {}", session.id);
        *self.root_session.lock().await = Some(session);
        Ok(())
    }

    /// End the rooted-session task.  Closes the root session (Kilo also closes
    /// all its forks automatically).
    pub async fn end_task(&self) {
        let session = self.root_session.lock().await.take();
        if let Some(s) = session {
            debug!("KiloAiExecutor: closing root session {}", s.id);
            if let Err(e) = self.client.close_session(&s.id).await {
                warn!("KiloAiExecutor: failed to close root session: {e}");
            }
        }
    }

    // -----------------------------------------------------------------------
    // Core execution logic
    // -----------------------------------------------------------------------

    /// Execute a single `ChatRequest` within a (possibly forked) session.
    async fn execute_in_session(
        &self,
        session: &KiloSession,
        request: &ChatRequest,
    ) -> Result<ChatResponse, String> {
        // Build message content: collapse conversation into one user turn.
        let mut parts: Vec<String> = Vec::new();
        if let Some(ref sp) = request.system_prompt {
            parts.push(format!("[System]: {sp}"));
        }
        for msg in &request.messages {
            let role = match msg.role {
                MessageRole::User => "User",
                MessageRole::Assistant => "Assistant",
                MessageRole::System => "System",
                MessageRole::Error => "Error",
                MessageRole::Tool => "Tool",
            };
            parts.push(format!("[{role}]: {}", msg.content));
        }
        let content = parts.join("\n\n");

        self.client
            .send_message(&session.id, KiloMessage::user(content))
            .await
            .map_err(|e| e.to_string())?;

        let mut rx = self
            .client
            .subscribe_events(&session.id)
            .await
            .map_err(|e| e.to_string())?;

        // Collect response.
        let mut text = String::new();
        let mut tool_calls: Vec<ToolCall> = Vec::new();
        let mut usage = None;
        let mut finish = FinishReason::Stop;

        while let Some(event) = rx.recv().await {
            match event {
                KiloEvent::TextDelta { content: delta } => {
                    text.push_str(&delta);
                }
                KiloEvent::ToolCall(tc) => {
                    tool_calls.push(ToolCall {
                        id: tc.id,
                        name: tc.name,
                        input: tc.input,
                    });
                }
                KiloEvent::Done {
                    stop_reason: sr,
                    usage: u,
                } => {
                    finish = match sr.as_deref() {
                        Some("max_tokens") | Some("length") => FinishReason::Length,
                        _ => FinishReason::Stop,
                    };
                    usage = u.map(|u| TokenUsage {
                        prompt_tokens: u.input_tokens,
                        completion_tokens: u.output_tokens,
                        total_tokens: u.total_tokens(),
                        cache_creation_input_tokens: u.cache_write_tokens,
                        cache_read_input_tokens: u.cache_read_tokens,
                    });
                    break;
                }
                KiloEvent::Error { message, .. } => {
                    return Err(format!("Kilo error: {message}"));
                }
                _ => {}
            }
        }

        Ok(ChatResponse {
            content: text,
            model: request.model.clone(),
            usage: usage.unwrap_or_default(),
            finish_reason: finish,
            thinking: None,
            tool_calls: if tool_calls.is_empty() {
                None
            } else {
                Some(tool_calls)
            },
        })
    }
}

impl AiExecutor for KiloAiExecutor {
    /// Execute a single AI request.
    ///
    /// Strategy:
    /// - If a root session is active (set by `start_task`), fork it and use
    ///   the fork for this call.  The fork is closed after the call completes.
    /// - Otherwise, create an ephemeral session (create → use → close).
    async fn execute(&self, request: &ChatRequest) -> Result<ChatResponse, String> {
        let root = self.root_session.lock().await.clone();

        if let Some(ref root_session) = root {
            // Rooted mode: fork the root session.
            let fork = self
                .client
                .fork_session(&root_session.id)
                .await
                .map_err(|e| e.to_string())?;
            debug!(
                "KiloAiExecutor: forked session {} from root {}",
                fork.id, root_session.id
            );
            let result = self.execute_in_session(&fork, request).await;
            // Close the fork regardless of success/failure.
            let _ = self.client.close_session(&fork.id).await;
            result
        } else {
            // Ephemeral mode: create → use → close.
            let model = request
                .model
                .strip_prefix("kilo:")
                .unwrap_or(&request.model)
                .to_owned();

            let session = self
                .client
                .create_session(CreateSessionRequest {
                    model: Some(model),
                    system: request.system_prompt.clone(),
                    max_tokens: Some(request.max_tokens),
                    workspace: None,
                })
                .await
                .map_err(|e| e.to_string())?;

            let result = self.execute_in_session(&session, request).await;
            let _ = self.client.close_session(&session.id).await;
            result
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::KiloConfig;

    /// Smoke test: executor is constructible and does not panic.
    #[test]
    fn executor_builds() {
        let _exec = KiloAiExecutor::new(KiloConfig::default());
    }
}
