//! Doubao / BytePlus provider (Volcano Engine).
//!
//! BytePlus offers an OpenAI-compatible API at
//! `https://ark.ap-southeast.bytepluses.com/api/v3`.
//! Streaming uses the same SSE wire format parsed by [`super::openai_sse`].

use async_trait::async_trait;
use serde::Serialize;
use tokio::sync::mpsc;

use super::openai_sse::{self, ChatCompletionResponse};
use super::{AiProvider, ProviderError};
use crate::types::{
    ChatMessage, ChatRequest, ChatResponse, FinishReason, ModelInfo, ProviderType, StreamChunk,
    TokenUsage, ToolCall,
};

// ---------------------------------------------------------------------------
// Wire types (serialization only)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct DoubaoChatRequest {
    model: String,
    messages: Vec<DoubaoMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream_options: Option<StreamOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<DoubaoTool>>,
}

#[derive(Debug, Serialize)]
struct StreamOptions {
    include_usage: bool,
}

#[derive(Debug, Serialize)]
struct DoubaoTool {
    #[serde(rename = "type")]
    tool_type: String,
    function: DoubaoFunction,
}

#[derive(Debug, Serialize)]
struct DoubaoFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct DoubaoMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<DoubaoToolCallMsg>>,
}

#[derive(Debug, Serialize)]
struct DoubaoToolCallMsg {
    id: String,
    #[serde(rename = "type")]
    call_type: String,
    function: DoubaoFunctionCall,
}

#[derive(Debug, Serialize)]
struct DoubaoFunctionCall {
    name: String,
    arguments: String,
}

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

/// Doubao / BytePlus provider (Doubao Pro, Doubao Lite, etc.).
pub struct DoubaoProvider {
    api_key: Option<String>,
    base_url: String,
    client: reqwest::Client,
}

impl DoubaoProvider {
    /// Create a new Doubao provider.
    ///
    /// Pass an empty string for `api_key` to create an unavailable provider
    /// that can still be configured later.
    pub fn new(api_key: String) -> Self {
        Self {
            api_key: if api_key.is_empty() {
                None
            } else {
                Some(api_key)
            },
            base_url: "https://ark.ap-southeast.bytepluses.com/api/v3".into(),
            client: reqwest::Client::new(),
        }
    }

    /// Create a provider with a custom base URL.
    pub fn with_base_url(api_key: String, base_url: String) -> Self {
        Self {
            api_key: if api_key.is_empty() {
                None
            } else {
                Some(api_key)
            },
            base_url,
            client: reqwest::Client::new(),
        }
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    /// Convert generic messages to the Doubao wire format.
    fn convert_messages(
        messages: &[ChatMessage],
        system_prompt: Option<&str>,
    ) -> Vec<DoubaoMessage> {
        let mut out = Vec::with_capacity(messages.len() + 1);

        if let Some(sys) = system_prompt {
            out.push(DoubaoMessage {
                role: "system".into(),
                content: Some(serde_json::Value::String(sys.to_string())),
                tool_call_id: None,
                tool_calls: None,
            });
        }

        for m in messages {
            let role = match m.role {
                crate::types::MessageRole::User => "user",
                crate::types::MessageRole::Assistant => "assistant",
                crate::types::MessageRole::System => "system",
                crate::types::MessageRole::Error => "user",
                crate::types::MessageRole::Tool => "tool",
            };

            // Tool result messages.
            if m.role == crate::types::MessageRole::Tool {
                out.push(DoubaoMessage {
                    role: role.into(),
                    content: Some(serde_json::Value::String(m.content.clone())),
                    tool_call_id: m.tool_call_id.clone(),
                    tool_calls: None,
                });
                continue;
            }

            // Assistant messages with tool_calls.
            if m.role == crate::types::MessageRole::Assistant
                && let Some(ref calls) = m.tool_calls
            {
                let tc_msgs: Vec<DoubaoToolCallMsg> = calls
                    .iter()
                    .map(|c| DoubaoToolCallMsg {
                        id: c.id.clone(),
                        call_type: "function".into(),
                        function: DoubaoFunctionCall {
                            name: c.name.clone(),
                            arguments: serde_json::to_string(&c.input).unwrap_or_default(),
                        },
                    })
                    .collect();
                out.push(DoubaoMessage {
                    role: role.into(),
                    content: if m.content.is_empty() {
                        None
                    } else {
                        Some(serde_json::Value::String(m.content.clone()))
                    },
                    tool_call_id: None,
                    tool_calls: Some(tc_msgs),
                });
                continue;
            }

            out.push(DoubaoMessage {
                role: role.into(),
                content: Some(serde_json::Value::String(m.content.clone())),
                tool_call_id: None,
                tool_calls: None,
            });
        }

        out
    }

    /// Build the JSON request body.
    fn build_body(&self, request: &ChatRequest, stream: bool) -> DoubaoChatRequest {
        DoubaoChatRequest {
            model: request.model.clone(),
            messages: Self::convert_messages(&request.messages, request.system_prompt.as_deref()),
            stream,
            max_tokens: Some(request.max_tokens),
            temperature: request.temperature,
            stream_options: if stream {
                Some(StreamOptions {
                    include_usage: true,
                })
            } else {
                None
            },
            tools: request.tools.as_ref().map(|defs| {
                defs.iter()
                    .map(|t| DoubaoTool {
                        tool_type: "function".into(),
                        function: DoubaoFunction {
                            name: t.name.clone(),
                            description: t.description.clone(),
                            parameters: t.input_schema.clone(),
                        },
                    })
                    .collect()
            }),
        }
    }

    /// Get the API key or return an error.
    fn require_key(&self) -> Result<&str, ProviderError> {
        self.api_key.as_deref().ok_or(ProviderError::InvalidKey)
    }

    /// Send a POST to the chat completions endpoint.
    async fn post_completions(
        &self,
        body: &DoubaoChatRequest,
    ) -> Result<reqwest::Response, ProviderError> {
        let key = self.require_key()?;
        let url = format!("{}/chat/completions", self.base_url);

        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {key}"))
            .header("Content-Type", "application/json")
            .json(body)
            .send()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;

        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            return Err(ProviderError::InvalidKey);
        }
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(ProviderError::RateLimit);
        }
        if status == reqwest::StatusCode::REQUEST_TIMEOUT
            || status == reqwest::StatusCode::GATEWAY_TIMEOUT
        {
            return Err(ProviderError::Timeout);
        }
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(ProviderError::Other(format!(
                "Doubao API error {status}: {text}"
            )));
        }

        Ok(resp)
    }
}

#[async_trait]
impl AiProvider for DoubaoProvider {
    fn provider_type(&self) -> ProviderType {
        ProviderType::Doubao
    }

    fn name(&self) -> &str {
        "Doubao"
    }

    async fn is_available(&self) -> bool {
        self.api_key.as_ref().is_some_and(|k| !k.is_empty())
    }

    async fn get_models(&self) -> Vec<ModelInfo> {
        let mut static_models: Vec<ModelInfo> =
            crate::model_registry::models_for_provider(ProviderType::Doubao)
                .into_iter()
                .cloned()
                .collect();

        // Try to enrich with live catalog.
        if let Ok(key) = self.require_key() {
            if let Ok(live) = super::doubao_catalog::fetch_doubao_models(key).await {
                let static_ids: std::collections::HashSet<_> =
                    static_models.iter().map(|m| m.id.clone()).collect();
                for model in live {
                    if !static_ids.contains(&model.id) {
                        static_models.push(model);
                    }
                }
            }
        }

        static_models
    }

    /// Non-streaming chat completion.
    async fn chat(&self, request: &ChatRequest) -> Result<ChatResponse, ProviderError> {
        let body = self.build_body(request, false);
        let resp = self.post_completions(&body).await?;

        let data: ChatCompletionResponse = resp
            .json()
            .await
            .map_err(|e| ProviderError::Other(format!("JSON parse error: {e}")))?;

        let choice = data
            .choices
            .first()
            .ok_or_else(|| ProviderError::Other("No choices in Doubao response".into()))?;

        let content = choice.message.content.clone().unwrap_or_default();

        let finish_reason = match choice.finish_reason.as_deref() {
            Some("stop") => FinishReason::Stop,
            Some("length") => FinishReason::Length,
            Some("content_filter") => FinishReason::ContentFilter,
            _ => FinishReason::Stop,
        };

        let usage = data
            .usage
            .map(|u| {
                let p = u.prompt_tokens.unwrap_or(0);
                let c = u.completion_tokens.unwrap_or(0);
                TokenUsage {
                    prompt_tokens: p,
                    completion_tokens: c,
                    total_tokens: u.total_tokens.unwrap_or(p + c),
                    cache_creation_input_tokens: None,
                    cache_read_input_tokens: None,
                }
            })
            .unwrap_or_default();

        let tool_calls = choice.message.tool_calls.as_ref().map(|tcs| {
            tcs.iter()
                .map(|tc| ToolCall {
                    id: tc.id.clone(),
                    name: tc.function.name.clone(),
                    input: serde_json::from_str(&tc.function.arguments)
                        .unwrap_or(serde_json::Value::Object(serde_json::Map::new())),
                })
                .collect()
        });

        Ok(ChatResponse {
            content,
            model: data.model,
            usage,
            finish_reason,
            thinking: None,
            tool_calls,
        })
    }

    /// Streaming chat completion via SSE.
    async fn stream_chat(
        &self,
        request: &ChatRequest,
    ) -> Result<mpsc::Receiver<StreamChunk>, ProviderError> {
        let body = self.build_body(request, true);
        let resp = self.post_completions(&body).await?;

        let (tx, rx) = mpsc::channel::<StreamChunk>(64);

        tokio::spawn(async move {
            openai_sse::drive_sse_stream(resp, tx).await;
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
    use crate::types::{ChatMessage, ChatRequest, MessageRole};

    fn sample_request(model: &str) -> ChatRequest {
        ChatRequest {
            messages: vec![ChatMessage::text(MessageRole::User, "Hello")],
            model: model.into(),
            max_tokens: 1024,
            temperature: Some(0.7),
            system_prompt: None,
            tools: None,
            cache_system_prompt: false,
        }
    }

    #[test]
    fn build_body_standard() {
        let provider = DoubaoProvider::new("doubao-test".into());
        let req = sample_request("doubao-pro-256k");
        let body = provider.build_body(&req, false);

        assert_eq!(body.model, "doubao-pro-256k");
        assert_eq!(body.max_tokens, Some(1024));
        assert_eq!(body.temperature, Some(0.7));
        assert!(!body.stream);
        assert!(body.stream_options.is_none());
    }

    #[test]
    fn build_body_stream_includes_usage() {
        let provider = DoubaoProvider::new("doubao-test".into());
        let req = sample_request("doubao-pro-256k");
        let body = provider.build_body(&req, true);

        assert!(body.stream);
        assert!(body.stream_options.is_some());
        assert!(body.stream_options.unwrap().include_usage);
    }

    #[test]
    fn build_body_with_system_prompt() {
        let provider = DoubaoProvider::new("doubao-test".into());
        let mut req = sample_request("doubao-pro-256k");
        req.system_prompt = Some("You are helpful.".into());
        let body = provider.build_body(&req, false);

        assert_eq!(body.messages.len(), 2);
        assert_eq!(body.messages[0].role, "system");
        assert_eq!(body.messages[1].role, "user");
    }

    #[test]
    fn convert_messages_maps_roles() {
        let messages = vec![
            ChatMessage::text(MessageRole::User, "hi"),
            ChatMessage::text(MessageRole::Assistant, "hello"),
            ChatMessage::text(MessageRole::System, "context"),
        ];
        let converted = DoubaoProvider::convert_messages(&messages, None);
        assert_eq!(converted.len(), 3);
        assert_eq!(converted[0].role, "user");
        assert_eq!(converted[1].role, "assistant");
        assert_eq!(converted[2].role, "system");
    }

    #[test]
    fn provider_type_is_doubao() {
        let provider = DoubaoProvider::new("test".into());
        assert_eq!(provider.provider_type(), ProviderType::Doubao);
    }

    #[test]
    fn name_is_doubao() {
        let provider = DoubaoProvider::new("test".into());
        assert_eq!(provider.name(), "Doubao");
    }

    #[tokio::test]
    async fn is_available_with_key() {
        let provider = DoubaoProvider::new("real-key".into());
        assert!(provider.is_available().await);
    }

    #[tokio::test]
    async fn is_unavailable_without_key() {
        let provider = DoubaoProvider::new(String::new());
        assert!(!provider.is_available().await);
    }

    #[test]
    fn require_key_returns_error_without_key() {
        let provider = DoubaoProvider::new(String::new());
        assert!(provider.require_key().is_err());
    }

    #[test]
    fn default_base_url() {
        let provider = DoubaoProvider::new("key".into());
        assert_eq!(
            provider.base_url,
            "https://ark.ap-southeast.bytepluses.com/api/v3"
        );
    }

    #[test]
    fn custom_base_url() {
        let provider = DoubaoProvider::with_base_url("key".into(), "http://custom:8080".into());
        assert_eq!(provider.base_url, "http://custom:8080");
    }
}
