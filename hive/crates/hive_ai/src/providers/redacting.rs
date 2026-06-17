//! Egress-redaction provider decorator.
//!
//! [`RedactingProvider`] wraps any [`AiProvider`] and scrubs secrets from the
//! fully-assembled outbound [`ChatRequest`] — every `message.content` and the
//! `system_prompt` — immediately before the request is dispatched to the inner
//! provider. This is the single chokepoint that guarantees no secret or
//! registered API key in outbound request content (context files, RAG chunks,
//! tool/bash output, knowledge files, the user's typed text) ever reaches a
//! model provider.
//!
//! It reuses the `hive_shield` secret detectors via
//! [`hive_shield::redact_secrets`] (non-circular: `hive_ai -> hive_shield`).
//!
//! Every provider registered in [`crate::service::AiService`] is wrapped in one
//! of these, so all egress paths — chat, the agent swarm (via the routing
//! handle), agents, and tool outputs — are covered uniformly, because every
//! path ultimately resolves a provider from that map and calls `chat` /
//! `stream_chat`.

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::providers::{AiProvider, ProviderError};
use crate::types::{ChatRequest, ChatResponse, ModelInfo, ProviderType, StreamChunk};

/// Wraps an [`AiProvider`], redacting secrets from every outbound request.
pub struct RedactingProvider {
    inner: Arc<dyn AiProvider>,
    /// Registered API keys (the literal strings) that must never egress. Shared
    /// via `Arc` so wrapping every provider does not duplicate the key list.
    registered_keys: Arc<Vec<String>>,
}

impl RedactingProvider {
    /// Wrap `inner`, redacting `registered_keys` (plus all shield-detected
    /// secrets) from outbound requests.
    pub fn new(inner: Arc<dyn AiProvider>, registered_keys: Vec<String>) -> Self {
        Self {
            inner,
            registered_keys: Arc::new(registered_keys),
        }
    }

    /// Wrap `inner` with an already-shared key list (avoids cloning the Vec).
    pub fn with_shared_keys(inner: Arc<dyn AiProvider>, registered_keys: Arc<Vec<String>>) -> Self {
        Self {
            inner,
            registered_keys,
        }
    }

    /// Produce a redacted clone of `request`, scrubbing every message content
    /// and the system prompt. Returns the redacted request and the total number
    /// of redactions performed across all fields.
    fn redact_request(&self, request: &ChatRequest) -> (ChatRequest, usize) {
        let mut redacted = request.clone();
        let keys = self.registered_keys.as_slice();
        let mut total = 0usize;

        for message in &mut redacted.messages {
            let (new_content, count) = hive_shield::redact_secrets(&message.content, keys);
            if count > 0 {
                message.content = new_content;
                total += count;
            }
        }

        if let Some(system_prompt) = redacted.system_prompt.as_mut() {
            let (new_prompt, count) = hive_shield::redact_secrets(system_prompt, keys);
            if count > 0 {
                *system_prompt = new_prompt;
                total += count;
            }
        }

        (redacted, total)
    }
}

#[async_trait]
impl AiProvider for RedactingProvider {
    fn provider_type(&self) -> ProviderType {
        self.inner.provider_type()
    }

    fn name(&self) -> &str {
        self.inner.name()
    }

    async fn is_available(&self) -> bool {
        self.inner.is_available().await
    }

    async fn get_models(&self) -> Vec<ModelInfo> {
        self.inner.get_models().await
    }

    async fn chat(&self, request: &ChatRequest) -> Result<ChatResponse, ProviderError> {
        let (redacted, count) = self.redact_request(request);
        if count > 0 {
            // NEVER log the secret values — only the count and provider name.
            tracing::warn!(
                count,
                provider = self.inner.name(),
                "redacted {count} secret(s) from outbound request to {}",
                self.inner.name()
            );
        }
        self.inner.chat(&redacted).await
    }

    async fn stream_chat(
        &self,
        request: &ChatRequest,
    ) -> Result<mpsc::Receiver<StreamChunk>, ProviderError> {
        let (redacted, count) = self.redact_request(request);
        if count > 0 {
            // NEVER log the secret values — only the count and provider name.
            tracing::warn!(
                count,
                provider = self.inner.name(),
                "redacted {count} secret(s) from outbound request to {}",
                self.inner.name()
            );
        }
        self.inner.stream_chat(&redacted).await
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ChatMessage, FinishReason, MessageRole, TokenUsage};
    use std::sync::Mutex;

    /// A mock provider that records the last `ChatRequest` it received.
    struct RecordingProvider {
        last_request: Mutex<Option<ChatRequest>>,
    }

    impl RecordingProvider {
        fn new() -> Self {
            Self {
                last_request: Mutex::new(None),
            }
        }
    }

    #[async_trait]
    impl AiProvider for RecordingProvider {
        fn provider_type(&self) -> ProviderType {
            ProviderType::Anthropic
        }

        fn name(&self) -> &str {
            "recording-mock"
        }

        async fn is_available(&self) -> bool {
            true
        }

        async fn get_models(&self) -> Vec<ModelInfo> {
            Vec::new()
        }

        async fn chat(&self, request: &ChatRequest) -> Result<ChatResponse, ProviderError> {
            *self.last_request.lock().unwrap() = Some(request.clone());
            Ok(ChatResponse {
                content: "ok".to_string(),
                model: request.model.clone(),
                usage: TokenUsage::default(),
                finish_reason: FinishReason::Stop,
                thinking: None,
                tool_calls: None,
            })
        }

        async fn stream_chat(
            &self,
            request: &ChatRequest,
        ) -> Result<mpsc::Receiver<StreamChunk>, ProviderError> {
            *self.last_request.lock().unwrap() = Some(request.clone());
            let (_tx, rx) = mpsc::channel(1);
            Ok(rx)
        }
    }

    fn request_with(messages: Vec<ChatMessage>, system_prompt: Option<String>) -> ChatRequest {
        ChatRequest {
            messages,
            model: "claude-test".to_string(),
            max_tokens: 4096,
            temperature: None,
            system_prompt,
            tools: None,
            cache_system_prompt: false,
        }
    }

    #[tokio::test]
    async fn redacts_all_secret_shapes_and_registered_key() {
        let registered_key = "sk-secretRegisteredKey123456".to_string();
        let aws = format!("AKIA{}", "IOSFODNN7EXAMPLE");
        let ghp = format!("ghp_{}", "A".repeat(40));
        let openai_like = "sk-proj-abcdefghijklmnopqrstuvwxyz1234567890";
        let pem = "-----BEGIN RSA PRIVATE KEY-----";

        let inner = Arc::new(RecordingProvider::new());
        let provider = RedactingProvider::new(inner.clone(), vec![registered_key.clone()]);

        // Spread the secrets across messages + system prompt, mixed with
        // verbatim "context file" / "tool output" style content.
        let messages = vec![
            ChatMessage::text(
                MessageRole::User,
                format!("Here is my AWS key {aws} from a context file"),
            ),
            ChatMessage::text(
                MessageRole::Tool,
                format!("tool output: github token {ghp} and openai {openai_like}"),
            ),
            ChatMessage::text(MessageRole::User, format!("registered: {registered_key}")),
        ];
        let system_prompt = Some(format!("system context contains a pem {pem} header"));

        let request = request_with(messages, system_prompt);
        provider.chat(&request).await.unwrap();

        let received = inner.last_request.lock().unwrap().clone().unwrap();
        let all: String = received
            .messages
            .iter()
            .map(|m| m.content.clone())
            .collect::<Vec<_>>()
            .join("\n")
            + "\n"
            + received.system_prompt.as_deref().unwrap_or("");

        // None of the secrets appear verbatim in what the inner provider saw.
        assert!(!all.contains(&aws), "AWS key leaked: {all}");
        assert!(!all.contains(&ghp), "GitHub PAT leaked: {all}");
        assert!(!all.contains(openai_like), "OpenAI key leaked: {all}");
        assert!(!all.contains(pem), "PEM header leaked: {all}");
        assert!(
            !all.contains(&registered_key),
            "registered key leaked: {all}"
        );
        // And the placeholder is present.
        assert!(all.contains("‹REDACTED:"), "no redaction placeholder: {all}");
    }

    #[tokio::test]
    async fn clean_request_passes_through_unchanged() {
        let inner = Arc::new(RecordingProvider::new());
        let provider = RedactingProvider::new(
            inner.clone(),
            vec!["sk-secretRegisteredKey123456".to_string()],
        );

        let original_user = "Please refactor the sort function for readability.";
        let original_tool = "Build succeeded in 4.2s with 0 warnings.";
        let original_system = "You are a helpful coding assistant.";

        let messages = vec![
            ChatMessage::text(MessageRole::User, original_user),
            ChatMessage::text(MessageRole::Tool, original_tool),
        ];
        let request = request_with(messages, Some(original_system.to_string()));
        provider.chat(&request).await.unwrap();

        let received = inner.last_request.lock().unwrap().clone().unwrap();
        // Byte-for-byte identical — no false-positive mangling.
        assert_eq!(received.messages[0].content, original_user);
        assert_eq!(received.messages[1].content, original_tool);
        assert_eq!(received.system_prompt.as_deref(), Some(original_system));
    }
}
