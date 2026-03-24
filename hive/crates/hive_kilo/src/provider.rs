//! `KiloAiProvider` — implements [`hive_ai::providers::AiProvider`] for Kilo.
//!
//! This wraps the Kilo REST API so that Hive's routing layer can select Kilo
//! as a provider and dispatch `chat` / `stream_chat` calls through it, just
//! like any other backend (Anthropic, Ollama, etc.).
//!
//! # Session strategy
//!
//! Both `chat` and `stream_chat` use the **ephemeral-session** strategy:
//! - `POST /session` to create a session
//! - `POST /session/{id}/chat` to send the request
//! - Collect the full response via `GET /session/{id}/event` SSE
//! - `DELETE /session/{id}` to close the session
//!
//! This keeps the provider stateless from Hive's point of view and avoids
//! leaked sessions on errors (the session is always closed in a `finally`
//! block pattern).  For long-running multi-step work the `KiloAiExecutor`
//! crate offers session reuse via forking.

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use hive_ai::providers::{AiProvider, ProviderError};
use hive_ai::types::{
    ChatMessage, ChatRequest, ChatResponse, FinishReason, MessageRole, ModelCapabilities,
    ModelCapability, ModelInfo, ModelTier, ProviderType, StopReason, StreamChunk, TokenUsage,
    ToolCall,
};

use crate::client::{KiloClient, KiloModel};
use crate::error::KiloError;
use crate::events::KiloEvent;
use crate::session::{CreateSessionRequest, KiloMessage};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Convert Hive's `ChatRequest` into the fields Kilo needs.
///
/// Kilo expects a single `user` message.  We prepend a `system` message from
/// the system prompt (if any) and then concatenate the conversation history
/// into a single user turn so the Kilo session has full context.
fn build_user_content(request: &ChatRequest) -> String {
    let mut parts: Vec<String> = Vec::new();

    for msg in &request.messages {
        let role_label = match msg.role {
            MessageRole::User => "User",
            MessageRole::Assistant => "Assistant",
            MessageRole::System => "System",
            MessageRole::Error => "Error",
            MessageRole::Tool => "Tool",
        };
        parts.push(format!("[{role_label}]: {}", msg.content));
    }

    parts.join("\n\n")
}

/// Derive a Hive `FinishReason` from Kilo's `stop_reason` string.
fn map_stop_reason(reason: Option<&str>) -> FinishReason {
    match reason {
        Some("end_turn") | Some("stop") | None => FinishReason::Stop,
        Some("max_tokens") | Some("length") => FinishReason::Length,
        Some("content_filter") => FinishReason::ContentFilter,
        _ => FinishReason::Stop,
    }
}

/// Map a `KiloModel` entry to Hive's `ModelInfo`.
fn kilo_model_to_info(m: KiloModel) -> ModelInfo {
    let id = m.id.clone();
    let name = m.name.unwrap_or_else(|| {
        // Use the last segment of "provider/model" as a display name.
        id.split('/').last().unwrap_or(&id).to_owned()
    });
    let mut caps = vec![];
    if m.vision {
        caps.push(ModelCapability::Vision);
    }
    // Kilo models that support tool use — inferred from name heuristics.
    if id.contains("claude") || id.contains("gpt") || id.contains("gemini") {
        caps.push(ModelCapability::ToolUse);
    }

    ModelInfo {
        id: id.clone(),
        name,
        provider: "kilo".into(),
        provider_type: ProviderType::Kilo,
        // All Kilo-routed models are tagged Mid by default — cost depends on
        // the underlying provider which Hive cannot know at this layer.
        tier: ModelTier::Mid,
        context_window: m.context_window.unwrap_or(128_000),
        // Kilo abstracts pricing; report 0 so the UI shows "via Kilo".
        input_price_per_mtok: 0.0,
        output_price_per_mtok: 0.0,
        capabilities: ModelCapabilities::new(&caps),
        release_date: None,
    }
}

// ---------------------------------------------------------------------------
// KiloAiProvider
// ---------------------------------------------------------------------------

/// An [`AiProvider`] implementation that routes through a locally-running
/// Kilo coding agent daemon.
///
/// Construct via [`KiloAiProvider::new`] and register it with [`hive_ai`]'s
/// `AiService` from the `hive_app` wiring layer:
///
/// ```ignore
/// // In hive_app — avoids a circular dependency between hive_kilo and hive_ai.
/// let kilo = Arc::new(KiloAiProvider::new(config.kilo_url.as_deref(), config.kilo_password.as_deref()));
/// ai_service.register_external_provider(ProviderType::Kilo, kilo);
/// ```
pub struct KiloAiProvider {
    client: Arc<KiloClient>,
}

impl KiloAiProvider {
    /// Create a new provider pointing at the given Kilo server.
    ///
    /// `url` defaults to `http://localhost:4096` when `None` or empty.
    /// `password` is used for HTTP Basic Auth when the Kilo server has
    /// `KILO_SERVER_PASSWORD` set.
    pub fn new(url: Option<&str>, password: Option<&str>) -> Self {
        let config = crate::config::KiloConfig::from_service_config(url, password);
        Self {
            client: Arc::new(KiloClient::new(config)),
        }
    }

    /// Construct from an already-built [`KiloClient`].
    pub fn from_client(client: Arc<KiloClient>) -> Self {
        Self { client }
    }

    /// Collect a full text response from an SSE event receiver.
    ///
    /// Concatenates all `TextDelta` events and returns the accumulated content
    /// together with usage stats from the `Done` event.
    async fn collect_response(
        mut rx: mpsc::Receiver<KiloEvent>,
    ) -> (String, Option<crate::events::KiloUsage>, FinishReason, Vec<ToolCall>) {
        let mut content = String::new();
        let mut thinking = String::new();
        let mut usage = None;
        let mut stop_reason = None::<String>;
        let mut tool_calls: Vec<ToolCall> = Vec::new();

        while let Some(event) = rx.recv().await {
            match event {
                KiloEvent::TextDelta { content: delta } => {
                    content.push_str(&delta);
                }
                KiloEvent::ThinkingDelta { content: delta } => {
                    thinking.push_str(&delta);
                }
                KiloEvent::ToolCall(tc) => {
                    tool_calls.push(ToolCall {
                        id: tc.id,
                        name: tc.name,
                        input: tc.input,
                    });
                }
                KiloEvent::Done { stop_reason: sr, usage: u } => {
                    stop_reason = sr;
                    usage = u;
                    break;
                }
                KiloEvent::Error { message, .. } => {
                    warn!("Kilo SSE error event: {message}");
                    break;
                }
                // FileChange / ToolResult events are discarded at this layer;
                // use KiloAiExecutor for richer handling.
                _ => {}
            }
        }

        let finish = map_stop_reason(stop_reason.as_deref());
        let _ = thinking; // available for future use
        (content, usage, finish, tool_calls)
    }
}

#[async_trait]
impl AiProvider for KiloAiProvider {
    fn provider_type(&self) -> ProviderType {
        ProviderType::Kilo
    }

    fn name(&self) -> &str {
        "Kilo (Local Agent)"
    }

    /// Ping Kilo's `/config` endpoint with a 2-second timeout.
    async fn is_available(&self) -> bool {
        self.client.health().await
    }

    /// Fetch the full model list from Kilo's `/provider` endpoint.
    async fn get_models(&self) -> Vec<ModelInfo> {
        match self.client.list_models().await {
            Ok(models) => {
                info!("Kilo returned {} models", models.len());
                models.into_iter().map(kilo_model_to_info).collect()
            }
            Err(KiloError::Unavailable { .. }) => {
                debug!("Kilo not reachable — returning no models");
                vec![]
            }
            Err(e) => {
                warn!("Failed to list Kilo models: {e}");
                vec![]
            }
        }
    }

    /// Non-streaming chat using an ephemeral Kilo session.
    async fn chat(&self, request: &ChatRequest) -> Result<ChatResponse, ProviderError> {
        // Determine the model to ask Kilo for.
        let model = if request.model.starts_with("kilo:")
            || self
                .client
                .config()
                .default_model
                .as_deref()
                .is_some_and(|m| m == request.model)
        {
            // Strip the "kilo:" prefix if present.
            request
                .model
                .strip_prefix("kilo:")
                .unwrap_or(&request.model)
                .to_owned()
        } else {
            request.model.clone()
        };

        // 1. Create ephemeral session.
        let session = self
            .client
            .create_session(CreateSessionRequest {
                model: Some(model),
                system: request.system_prompt.clone(),
                max_tokens: Some(request.max_tokens),
                workspace: None,
            })
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;

        debug!("Kilo session created: {}", session.id);

        // 2. Build user message (conversation history collapsed to one turn).
        let user_content = build_user_content(request);
        let message = KiloMessage::user(user_content);

        // 3. Send message.
        if let Err(e) = self.client.send_message(&session.id, message).await {
            let _ = self.client.close_session(&session.id).await;
            return Err(ProviderError::Network(e.to_string()));
        }

        // 4. Subscribe to SSE and collect the full response.
        let rx = match self.client.subscribe_events(&session.id).await {
            Ok(r) => r,
            Err(e) => {
                let _ = self.client.close_session(&session.id).await;
                return Err(ProviderError::Network(e.to_string()));
            }
        };

        let (content, kilo_usage, finish_reason, tool_calls) =
            Self::collect_response(rx).await;

        // 5. Clean up the session.
        let _ = self.client.close_session(&session.id).await;

        let usage = kilo_usage
            .map(|u| TokenUsage {
                prompt_tokens: u.input_tokens,
                completion_tokens: u.output_tokens,
                total_tokens: u.total_tokens(),
                cache_creation_input_tokens: u.cache_write_tokens,
                cache_read_input_tokens: u.cache_read_tokens,
            })
            .unwrap_or_default();

        Ok(ChatResponse {
            content,
            model: request.model.clone(),
            usage,
            finish_reason,
            thinking: None,
            tool_calls: if tool_calls.is_empty() {
                None
            } else {
                Some(tool_calls)
            },
        })
    }

    /// Streaming chat using an ephemeral Kilo session.
    ///
    /// `FileChange` events are silently dropped at this layer — they don't map
    /// to `StreamChunk`.  Use `KiloAiExecutor` if you need file-change events.
    async fn stream_chat(
        &self,
        request: &ChatRequest,
    ) -> Result<mpsc::Receiver<StreamChunk>, ProviderError> {
        let model = request
            .model
            .strip_prefix("kilo:")
            .unwrap_or(&request.model)
            .to_owned();

        // 1. Create session.
        let session = self
            .client
            .create_session(CreateSessionRequest {
                model: Some(model),
                system: request.system_prompt.clone(),
                max_tokens: Some(request.max_tokens),
                workspace: None,
            })
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;

        // 2. Send the user message.
        let user_content = build_user_content(request);
        if let Err(e) = self
            .client
            .send_message(&session.id, KiloMessage::user(user_content))
            .await
        {
            let _ = self.client.close_session(&session.id).await;
            return Err(ProviderError::Network(e.to_string()));
        }

        // 3. Subscribe to SSE.
        let mut kilo_rx = match self.client.subscribe_events(&session.id).await {
            Ok(r) => r,
            Err(e) => {
                let _ = self.client.close_session(&session.id).await;
                return Err(ProviderError::Network(e.to_string()));
            }
        };

        // 4. Forward Kilo events → StreamChunks in a background task.
        let (tx, rx) = mpsc::channel::<StreamChunk>(128);
        let client = Arc::clone(&self.client);
        let session_id = session.id.clone();

        tokio::spawn(async move {
            let mut accumulated_tool_calls: Vec<ToolCall> = Vec::new();

            while let Some(event) = kilo_rx.recv().await {
                match event {
                    KiloEvent::TextDelta { content } => {
                        if tx
                            .send(StreamChunk {
                                content,
                                done: false,
                                thinking: None,
                                usage: None,
                                tool_calls: None,
                                stop_reason: None,
                            })
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    KiloEvent::ThinkingDelta { content } => {
                        if tx
                            .send(StreamChunk {
                                content: String::new(),
                                done: false,
                                thinking: Some(content),
                                usage: None,
                                tool_calls: None,
                                stop_reason: None,
                            })
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    KiloEvent::ToolCall(tc) => {
                        accumulated_tool_calls.push(ToolCall {
                            id: tc.id,
                            name: tc.name,
                            input: tc.input,
                        });
                    }
                    KiloEvent::Done { stop_reason, usage } => {
                        let hive_usage = usage.map(|u| TokenUsage {
                            prompt_tokens: u.input_tokens,
                            completion_tokens: u.output_tokens,
                            total_tokens: u.total_tokens(),
                            cache_creation_input_tokens: u.cache_write_tokens,
                            cache_read_input_tokens: u.cache_read_tokens,
                        });
                        let stop = match stop_reason.as_deref() {
                            Some("tool_use") => Some(StopReason::ToolUse),
                            Some("max_tokens") => Some(StopReason::MaxTokens),
                            _ => Some(StopReason::EndTurn),
                        };
                        let tool_calls = if accumulated_tool_calls.is_empty() {
                            None
                        } else {
                            Some(std::mem::take(&mut accumulated_tool_calls))
                        };
                        let _ = tx
                            .send(StreamChunk {
                                content: String::new(),
                                done: true,
                                thinking: None,
                                usage: hive_usage,
                                tool_calls,
                                stop_reason: stop,
                            })
                            .await;
                        break;
                    }
                    KiloEvent::Error { message, .. } => {
                        warn!("Kilo stream error: {message}");
                        let _ = tx
                            .send(StreamChunk {
                                content: String::new(),
                                done: true,
                                thinking: None,
                                usage: None,
                                tool_calls: None,
                                stop_reason: None,
                            })
                            .await;
                        break;
                    }
                    // FileChange / ToolResult — not forwarded through StreamChunk.
                    _ => {}
                }
            }

            // Always close the ephemeral session when the stream ends.
            let _ = client.close_session(&session_id).await;
        });

        Ok(rx)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_type_is_kilo() {
        let provider = KiloAiProvider::new(None, None);
        assert_eq!(provider.provider_type(), ProviderType::Kilo);
        assert_eq!(provider.name(), "Kilo (Local Agent)");
    }

    #[test]
    fn build_user_content_includes_roles() {
        let messages = vec![
            ChatMessage::text(MessageRole::User, "Hello"),
            ChatMessage::text(MessageRole::Assistant, "Hi there"),
            ChatMessage::text(MessageRole::User, "How are you?"),
        ];
        let request = ChatRequest {
            messages,
            model: "test".into(),
            max_tokens: 1024,
            temperature: None,
            system_prompt: None,
            tools: None,
            cache_system_prompt: false,
        };
        let content = build_user_content(&request);
        assert!(content.contains("[User]: Hello"));
        assert!(content.contains("[Assistant]: Hi there"));
    }

    #[test]
    fn map_stop_reason_defaults_to_stop() {
        assert_eq!(map_stop_reason(None), FinishReason::Stop);
        assert_eq!(map_stop_reason(Some("end_turn")), FinishReason::Stop);
        assert_eq!(map_stop_reason(Some("max_tokens")), FinishReason::Length);
    }

    #[test]
    fn kilo_model_to_info_defaults() {
        let m = KiloModel {
            id: "anthropic/claude-opus-4-5".into(),
            name: None,
            provider: Some("anthropic".into()),
            context_window: Some(200_000),
            vision: false,
        };
        let info = kilo_model_to_info(m);
        assert_eq!(info.provider_type, ProviderType::Kilo);
        assert_eq!(info.name, "claude-opus-4-5");
        assert_eq!(info.context_window, 200_000);
    }
}
