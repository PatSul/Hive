//! Venice API provider.
//!
//! Uses the OpenAI compatibility layer in the Venice API (`/api/v1/chat/completions`).

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
struct VeniceChatRequest {
    model: String,
    messages: Vec<VeniceMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream_options: Option<StreamOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<VeniceTool>>,
}

#[derive(Debug, Serialize)]
struct StreamOptions {
    include_usage: bool,
}

#[derive(Debug, Serialize)]
struct VeniceTool {
    #[serde(rename = "type")]
    tool_type: String,
    function: VeniceFunction,
}

#[derive(Debug, Serialize)]
struct VeniceFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct VeniceMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<VeniceToolCallMsg>>,
}

#[derive(Debug, Serialize)]
struct VeniceToolCallMsg {
    id: String,
    #[serde(rename = "type")]
    call_type: String,
    function: VeniceFunctionCall,
}

#[derive(Debug, Serialize)]
struct VeniceFunctionCall {
    name: String,
    arguments: String,
}

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

/// Venice AI provider (Llama, DeepSeek, etc).
pub struct VeniceProvider {
    api_key: Option<String>,
    base_url: String,
    client: reqwest::Client,
}

impl VeniceProvider {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key: if api_key.is_empty() {
                None
            } else {
                Some(api_key)
            },
            base_url: "https://api.venice.ai/api/v1".into(),
            client: reqwest::Client::new(),
        }
    }

    fn convert_messages(
        messages: &[ChatMessage],
        system_prompt: Option<&str>,
    ) -> Vec<VeniceMessage> {
        let mut out = Vec::with_capacity(messages.len() + 1);

        if let Some(sys) = system_prompt {
            out.push(VeniceMessage {
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
                crate::types::MessageRole::Error => "user", // Venice doesn't strictly map error, using user
                crate::types::MessageRole::Tool => "tool",
            };

            if m.role == crate::types::MessageRole::Tool {
                out.push(VeniceMessage {
                    role: role.into(),
                    content: Some(serde_json::Value::String(m.content.clone())),
                    tool_call_id: m.tool_call_id.clone(),
                    tool_calls: None,
                });
                continue;
            }

            if m.role == crate::types::MessageRole::Assistant
                && let Some(ref calls) = m.tool_calls
            {
                let tc_msgs: Vec<VeniceToolCallMsg> = calls
                    .iter()
                    .map(|c| VeniceToolCallMsg {
                        id: c.id.clone(),
                        call_type: "function".into(),
                        function: VeniceFunctionCall {
                            name: c.name.clone(),
                            arguments: serde_json::to_string(&c.input).unwrap_or_default(),
                        },
                    })
                    .collect();
                out.push(VeniceMessage {
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

            out.push(VeniceMessage {
                role: role.into(),
                content: Some(serde_json::Value::String(m.content.clone())),
                tool_call_id: None,
                tool_calls: None,
            });
        }

        out
    }

    fn build_body(&self, request: &ChatRequest, stream: bool) -> VeniceChatRequest {
        VeniceChatRequest {
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
                    .map(|t| VeniceTool {
                        tool_type: "function".into(),
                        function: VeniceFunction {
                            name: t.name.clone(),
                            description: t.description.clone(),
                            parameters: t.input_schema.clone(),
                        },
                    })
                    .collect()
            }),
        }
    }

    fn require_key(&self) -> Result<&str, ProviderError> {
        self.api_key.as_deref().ok_or(ProviderError::InvalidKey)
    }

    async fn post_completions(
        &self,
        body: &VeniceChatRequest,
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
                "Venice API error {status}: {text}"
            )));
        }

        Ok(resp)
    }
}

#[async_trait]
impl AiProvider for VeniceProvider {
    fn provider_type(&self) -> ProviderType {
        ProviderType::Venice
    }

    fn name(&self) -> &str {
        "Venice"
    }

    async fn is_available(&self) -> bool {
        self.api_key.as_ref().is_some_and(|k| !k.is_empty())
    }

    async fn get_models(&self) -> Vec<ModelInfo> {
        let mut static_models: Vec<ModelInfo> = super::venice_catalog::builtin_models();

        let key = match self.require_key() {
            Ok(k) => k,
            Err(_) => return static_models,
        };

        #[derive(serde::Deserialize)]
        struct VeniceModelsData {
            data: Option<Vec<VeniceModelObj>>,
        }
        #[derive(serde::Deserialize)]
        struct VeniceModelObj {
            id: String,
        }

        let url = format!("{}/models", self.base_url);
        if let Ok(resp) = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {key}"))
            .send()
            .await
        {
            if let Ok(parsed) = resp.json::<VeniceModelsData>().await {
                if let Some(data) = parsed.data {
                    let static_ids: std::collections::HashSet<_> =
                        static_models.iter().map(|m| m.id.clone()).collect();
                    for api_model in data {
                        if !static_ids.contains(&api_model.id) {
                            static_models.push(ModelInfo {
                                id: api_model.id.clone(),
                                name: api_model.id.clone(),
                                provider: "venice".into(),
                                provider_type: ProviderType::Venice,
                                tier: crate::types::ModelTier::Mid,
                                context_window: 32_768,
                                input_price_per_mtok: 0.0,
                                output_price_per_mtok: 0.0,
                                capabilities: crate::types::ModelCapabilities::new(&[
                                    crate::types::ModelCapability::ToolUse,
                                ]),
                                release_date: None,
                            });
                        }
                    }
                }
            }
        }

        static_models
    }

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
            .ok_or_else(|| ProviderError::Other("No choices in Venice response".into()))?;

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

        // Some Venice models may output reasoning blocks. Extract that if provided in the API extension.
        // For standard OpenAI compatibility we assume they might not map `thinking` natively in the same way,
        // but if they do, we can parse it from the response if we adjust ChatCompletionResponse.

        Ok(ChatResponse {
            content,
            model: data.model,
            usage,
            finish_reason,
            thinking: None,
            tool_calls,
        })
    }

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
