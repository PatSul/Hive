//! Hive Gateway provider — proxies AI requests through the Hive Cloud gateway.
//!
//! The gateway acts as an OpenAI-compatible relay, allowing Hive Cloud
//! subscribers to access multiple AI models under a single billing umbrella.
//! Authentication uses a JWT token obtained during cloud login.

use async_trait::async_trait;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::warn;

use super::{AiProvider, ProviderError};
use crate::types::{
    ChatMessage, ChatRequest, ChatResponse, FinishReason, ModelCapabilities, ModelInfo, ModelTier,
    ProviderType, StreamChunk, StopReason, TokenUsage,
};

// ---------------------------------------------------------------------------
// Wire types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct GatewayRequest {
    model: String,
    messages: Vec<GatewayMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
}

#[derive(Debug, Serialize)]
struct GatewayMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct GatewayResponse {
    choices: Vec<GatewayChoice>,
    usage: Option<GatewayUsage>,
}

#[derive(Debug, Deserialize)]
struct GatewayChoice {
    message: Option<GatewayMsg>,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GatewayMsg {
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GatewayUsage {
    prompt_tokens: Option<u32>,
    completion_tokens: Option<u32>,
    total_tokens: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct GatewayModelList {
    data: Vec<GatewayModelEntry>,
}

#[derive(Debug, Deserialize)]
struct GatewayModelEntry {
    id: String,
    #[allow(dead_code)]
    #[serde(default)]
    owned_by: Option<String>,
}

/// SSE frame from streaming response.
#[derive(Debug, Deserialize)]
struct SseFrame {
    choices: Vec<SseChoice>,
    usage: Option<GatewayUsage>,
}

#[derive(Debug, Deserialize)]
struct SseChoice {
    delta: Option<SseDelta>,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SseDelta {
    content: Option<String>,
}

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

/// Hive Cloud AI gateway provider.
pub struct HiveGatewayProvider {
    gateway_url: String,
    jwt: String,
    client: reqwest::Client,
}

impl HiveGatewayProvider {
    /// Create a new Hive Gateway provider.
    pub fn new(gateway_url: String, jwt: String) -> Self {
        Self {
            gateway_url: gateway_url.trim_end_matches('/').to_string(),
            jwt,
            client: reqwest::Client::new(),
        }
    }

    fn convert_messages(
        messages: &[ChatMessage],
        system_prompt: Option<&str>,
    ) -> Vec<GatewayMessage> {
        let mut out = Vec::with_capacity(messages.len() + 1);

        if let Some(sys) = system_prompt {
            out.push(GatewayMessage {
                role: "system".into(),
                content: sys.to_string(),
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
            out.push(GatewayMessage {
                role: role.into(),
                content: m.content.clone(),
            });
        }

        out
    }

    fn map_error(status: reqwest::StatusCode, text: &str) -> ProviderError {
        if status == reqwest::StatusCode::UNAUTHORIZED
            || status == reqwest::StatusCode::FORBIDDEN
        {
            return ProviderError::InvalidKey;
        }
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return ProviderError::RateLimit;
        }
        if status == reqwest::StatusCode::REQUEST_TIMEOUT
            || status == reqwest::StatusCode::GATEWAY_TIMEOUT
        {
            return ProviderError::Timeout;
        }
        ProviderError::Other(format!("Hive Gateway error {status}: {text}"))
    }
}

#[async_trait]
impl AiProvider for HiveGatewayProvider {
    fn provider_type(&self) -> ProviderType {
        ProviderType::HiveGateway
    }

    fn name(&self) -> &str {
        "Hive Gateway"
    }

    async fn is_available(&self) -> bool {
        let url = format!("{}/health", self.gateway_url);
        match self.client.get(&url).send().await {
            Ok(resp) => resp.status().is_success(),
            Err(_) => false,
        }
    }

    async fn get_models(&self) -> Vec<ModelInfo> {
        let url = format!("{}/gateway/v1/models", self.gateway_url);
        let resp = match self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.jwt))
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                warn!("Hive Gateway model list failed: {e}");
                return Vec::new();
            }
        };

        if !resp.status().is_success() {
            warn!(
                "Hive Gateway model list returned {}",
                resp.status()
            );
            return Vec::new();
        }

        let body: GatewayModelList = match resp.json().await {
            Ok(b) => b,
            Err(e) => {
                warn!("Hive Gateway model list parse error: {e}");
                return Vec::new();
            }
        };

        body.data
            .into_iter()
            .map(|entry| ModelInfo {
                id: entry.id.clone(),
                name: entry.id,
                provider: "hive_gateway".into(),
                provider_type: ProviderType::HiveGateway,
                tier: ModelTier::Mid,
                context_window: 128_000,
                input_price_per_mtok: 0.0,
                output_price_per_mtok: 0.0,
                capabilities: ModelCapabilities::default(),
                release_date: None,
            })
            .collect()
    }

    async fn chat(&self, request: &ChatRequest) -> Result<ChatResponse, ProviderError> {
        let url = format!("{}/gateway/v1/chat", self.gateway_url);

        let body = GatewayRequest {
            model: request.model.clone(),
            messages: Self::convert_messages(&request.messages, request.system_prompt.as_deref()),
            stream: false,
            max_tokens: Some(request.max_tokens),
            temperature: request.temperature,
        };

        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.jwt))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(Self::map_error(status, &text));
        }

        let gateway_resp: GatewayResponse = resp
            .json()
            .await
            .map_err(|e| ProviderError::Other(format!("Parse error: {e}")))?;

        let content = gateway_resp
            .choices
            .first()
            .and_then(|c| c.message.as_ref())
            .and_then(|m| m.content.clone())
            .unwrap_or_default();

        let finish_reason = gateway_resp
            .choices
            .first()
            .and_then(|c| c.finish_reason.as_deref())
            .map(|r| match r {
                "stop" => FinishReason::Stop,
                "length" => FinishReason::Length,
                "tool_calls" => FinishReason::Stop,
                _ => FinishReason::Stop,
            })
            .unwrap_or(FinishReason::Stop);

        let usage = gateway_resp
            .usage
            .map(|u| TokenUsage {
                prompt_tokens: u.prompt_tokens.unwrap_or(0),
                completion_tokens: u.completion_tokens.unwrap_or(0),
                total_tokens: u.total_tokens.unwrap_or(0),
            })
            .unwrap_or(TokenUsage {
                prompt_tokens: 0,
                completion_tokens: 0,
                total_tokens: 0,
            });

        Ok(ChatResponse {
            content,
            model: request.model.clone(),
            usage,
            finish_reason,
            thinking: None,
            tool_calls: None,
        })
    }

    async fn stream_chat(
        &self,
        request: &ChatRequest,
    ) -> Result<mpsc::Receiver<StreamChunk>, ProviderError> {
        let url = format!("{}/gateway/v1/chat", self.gateway_url);

        let body = GatewayRequest {
            model: request.model.clone(),
            messages: Self::convert_messages(&request.messages, request.system_prompt.as_deref()),
            stream: true,
            max_tokens: Some(request.max_tokens),
            temperature: request.temperature,
        };

        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.jwt))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(Self::map_error(status, &text));
        }

        let (tx, rx) = mpsc::channel(64);
        let byte_stream = resp.bytes_stream();

        tokio::spawn(async move {
            let mut stream = byte_stream;
            let mut buf = String::new();

            while let Some(chunk_result) = stream.next().await {
                let bytes = match chunk_result {
                    Ok(b) => b,
                    Err(e) => {
                        let _ = tx
                            .send(StreamChunk {
                                content: format!("[error] {e}"),
                                done: true,
                                thinking: None,
                                usage: None,
                                tool_calls: None,
                                stop_reason: None,
                            })
                            .await;
                        break;
                    }
                };

                buf.push_str(&String::from_utf8_lossy(&bytes));

                // Process complete SSE lines.
                while let Some(pos) = buf.find('\n') {
                    let line = buf[..pos].trim().to_string();
                    buf = buf[pos + 1..].to_string();

                    if line.is_empty() || line.starts_with(':') {
                        continue;
                    }

                    if let Some(data) = line.strip_prefix("data: ") {
                        if data.trim() == "[DONE]" {
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
                            return;
                        }

                        if let Ok(frame) = serde_json::from_str::<SseFrame>(data) {
                            if let Some(choice) = frame.choices.first() {
                                // Content delta
                                if let Some(ref delta) = choice.delta {
                                    if let Some(ref text) = delta.content {
                                        let _ = tx
                                            .send(StreamChunk {
                                                content: text.clone(),
                                                done: false,
                                                thinking: None,
                                                usage: None,
                                                tool_calls: None,
                                                stop_reason: None,
                                            })
                                            .await;
                                    }
                                }

                                // Finish reason
                                if let Some(ref reason) = choice.finish_reason {
                                    let stop = match reason.as_str() {
                                        "stop" => StopReason::EndTurn,
                                        "length" => StopReason::MaxTokens,
                                        _ => StopReason::EndTurn,
                                    };
                                    let usage = frame.usage.map(|u| TokenUsage {
                                        prompt_tokens: u.prompt_tokens.unwrap_or(0),
                                        completion_tokens: u.completion_tokens.unwrap_or(0),
                                        total_tokens: u.total_tokens.unwrap_or(0),
                                    });
                                    let _ = tx
                                        .send(StreamChunk {
                                            content: String::new(),
                                            done: true,
                                            thinking: None,
                                            usage,
                                            tool_calls: None,
                                            stop_reason: Some(stop),
                                        })
                                        .await;
                                }
                            }
                        }
                    }
                }
            }

            // Stream ended without [DONE].
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
        });

        Ok(rx)
    }
}
