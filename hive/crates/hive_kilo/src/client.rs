//! Low-level HTTP client for the Kilo REST API.
//!
//! [`KiloClient`] wraps a `reqwest::Client` and exposes one method per Kilo
//! endpoint.  All methods are `async` and return [`crate::error::KiloResult`].
//!
//! Authentication: when a password is configured the client attaches an
//! `Authorization: Basic base64("kilo:{password}")` header to every request,
//! matching Kilo's HTTP Basic Auth scheme.
//!
//! SSE streaming is returned as a raw byte stream; higher-level callers
//! (see [`crate::provider`] and [`crate::executor`]) are responsible for
//! parsing [`crate::events::KiloEvent`]s from the stream.

use std::sync::Arc;

use futures::StreamExt;
use reqwest::{Client, RequestBuilder};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::{debug, warn};

use crate::config::KiloConfig;
use crate::error::{KiloError, KiloResult};
use crate::events::{KiloEvent, parse_sse_line};
use crate::session::{CreateSessionRequest, KiloMessage, KiloSession, SessionInfo};

// ---------------------------------------------------------------------------
// Provider / model list types
// ---------------------------------------------------------------------------

/// A model entry returned by `GET /provider`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct KiloModel {
    /// Unique model identifier (e.g. `"anthropic/claude-opus-4-5"`).
    pub id: String,
    /// Human-readable name.
    #[serde(default)]
    pub name: Option<String>,
    /// Provider that owns this model (e.g. `"anthropic"`).
    #[serde(default)]
    pub provider: Option<String>,
    /// Approximate context-window size in tokens.
    #[serde(default)]
    pub context_window: Option<u32>,
    /// Whether the model supports vision (image) inputs.
    #[serde(default)]
    pub vision: bool,
}

/// Top-level response from `GET /provider`.
#[derive(Debug, Deserialize)]
pub struct ProviderListResponse {
    /// Flat list of all models across all providers.
    #[serde(default)]
    pub models: Vec<KiloModel>,
    /// Alternatively, providers may be grouped.
    #[serde(default)]
    pub providers: Vec<ProviderGroup>,
}

/// A named provider group containing one or more models.
#[derive(Debug, Deserialize)]
pub struct ProviderGroup {
    /// Provider identifier (e.g. `"anthropic"`).
    pub id: String,
    /// Models belonging to this provider.
    #[serde(default)]
    pub models: Vec<KiloModel>,
}

// ---------------------------------------------------------------------------
// Config response
// ---------------------------------------------------------------------------

/// Subset of the Kilo config response used for health-checking.
#[derive(Debug, Deserialize)]
pub struct KiloConfigResponse {
    /// Server version string.
    #[serde(default)]
    pub version: Option<String>,
    /// Status indicator (`"running"`, etc.).
    #[serde(default)]
    pub status: Option<String>,
}

// ---------------------------------------------------------------------------
// MCP types
// ---------------------------------------------------------------------------

/// A tool exposed by an MCP server managed by Kilo.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct KiloMcpTool {
    /// Tool name as registered in the MCP server.
    pub name: String,
    /// Human-readable description.
    #[serde(default)]
    pub description: Option<String>,
    /// JSON Schema describing the tool's input parameters.
    #[serde(default)]
    pub input_schema: Option<serde_json::Value>,
}

/// An MCP server managed by Kilo.
#[derive(Debug, Clone, Deserialize)]
pub struct KiloMcpServer {
    /// Name/identifier of the MCP server.
    pub name: String,
    /// Connection command (for stdio servers).
    #[serde(default)]
    pub command: Option<String>,
    /// List of tools the server exposes.
    #[serde(default)]
    pub tools: Vec<KiloMcpTool>,
}

/// Response from `GET /mcp`.
#[derive(Debug, Deserialize)]
pub struct McpListResponse {
    #[serde(default)]
    pub servers: Vec<KiloMcpServer>,
}

// ---------------------------------------------------------------------------
// PTY types
// ---------------------------------------------------------------------------

/// Request body for `POST /pty`.
#[derive(Debug, Serialize)]
pub struct CreatePtyRequest {
    /// Shell command to execute (e.g. `"bash"`, `"zsh"`).
    pub command: String,
    /// Terminal columns (default 80).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cols: Option<u32>,
    /// Terminal rows (default 24).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rows: Option<u32>,
    /// Working directory for the shell.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
}

/// A PTY session created by `POST /pty`.
#[derive(Debug, Clone, Deserialize)]
pub struct KiloPtySession {
    /// Unique PTY session ID.
    pub id: String,
    /// OS process ID of the shell.
    #[serde(default)]
    pub pid: Option<u32>,
}

// ---------------------------------------------------------------------------
// KiloClient
// ---------------------------------------------------------------------------

/// Low-level HTTP client for all Kilo REST endpoints.
///
/// Construct one via [`KiloClient::new`] and share it via `Arc<KiloClient>`
/// across providers, executors, and bridges.
pub struct KiloClient {
    config: Arc<KiloConfig>,
    http: Client,
}

impl std::fmt::Debug for KiloClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KiloClient")
            .field("base_url", &self.config.base_url)
            .finish()
    }
}

impl KiloClient {
    /// Create a new client from the given configuration.
    pub fn new(config: KiloConfig) -> Self {
        let http = Client::builder()
            .connect_timeout(std::time::Duration::from_secs(config.connect_timeout_secs))
            .build()
            .expect("reqwest client build should not fail");

        Self {
            config: Arc::new(config),
            http,
        }
    }

    /// Access the underlying config (e.g. to read `base_url`).
    pub fn config(&self) -> &KiloConfig {
        &self.config
    }

    // -----------------------------------------------------------------------
    // Auth helper
    // -----------------------------------------------------------------------

    /// Attach Basic Auth credentials when a password is configured.
    fn auth(&self, builder: RequestBuilder) -> RequestBuilder {
        if let Some(ref pw) = self.config.password {
            use base64::Engine as _;
            let credentials = base64::engine::general_purpose::STANDARD
                .encode(format!("kilo:{pw}"));
            builder.header("Authorization", format!("Basic {credentials}"))
        } else {
            builder
        }
    }

    // -----------------------------------------------------------------------
    // Health
    // -----------------------------------------------------------------------

    /// Return `true` if Kilo is running and reachable within the connect timeout.
    ///
    /// Uses `GET /config` with a short deadline — safe to call on the UI thread.
    pub async fn health(&self) -> bool {
        let url = format!("{}/config", self.config.base_url);
        let builder = self.http.get(&url);
        let builder = self.auth(builder);
        match builder
            .timeout(std::time::Duration::from_secs(2))
            .send()
            .await
        {
            Ok(r) => r.status().is_success(),
            Err(e) => {
                debug!("Kilo health-check failed: {e}");
                false
            }
        }
    }

    // -----------------------------------------------------------------------
    // Config endpoint
    // -----------------------------------------------------------------------

    /// Fetch the raw Kilo server config.  Primarily used for health checking
    /// and version detection.
    pub async fn get_config(&self) -> KiloResult<KiloConfigResponse> {
        let url = format!("{}/config", self.config.base_url);
        let builder = self.auth(self.http.get(&url));
        let resp = builder.send().await.map_err(|e| KiloError::Unavailable {
            url: url.clone(),
            source: e,
        })?;
        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(KiloError::api(status, body));
        }
        Ok(resp.json().await?)
    }

    // -----------------------------------------------------------------------
    // Provider / model list
    // -----------------------------------------------------------------------

    /// List all models available through Kilo's model router.
    ///
    /// Returns a flat `Vec<KiloModel>`, merging both the `models` array and
    /// any `providers[].models` arrays in the response.
    pub async fn list_models(&self) -> KiloResult<Vec<KiloModel>> {
        let url = format!("{}/provider", self.config.base_url);
        let builder = self.auth(self.http.get(&url));
        let resp = builder.send().await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(KiloError::api(status, body));
        }

        let data: ProviderListResponse = resp.json().await?;
        let mut models = data.models;
        for group in data.providers {
            let provider_id = group.id.clone();
            for mut m in group.models {
                if m.provider.is_none() {
                    m.provider = Some(provider_id.clone());
                }
                // Prefix ID with provider if not already namespaced.
                if !m.id.contains('/') {
                    m.id = format!("{provider_id}/{}", m.id);
                }
                models.push(m);
            }
        }
        Ok(models)
    }

    // -----------------------------------------------------------------------
    // Session lifecycle
    // -----------------------------------------------------------------------

    /// Create a new Kilo coding session.
    pub async fn create_session(&self, req: CreateSessionRequest) -> KiloResult<KiloSession> {
        let url = format!("{}/session", self.config.base_url);
        let builder = self.auth(self.http.post(&url)).json(&req);
        let resp = builder.send().await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(KiloError::api(status, body));
        }

        let info: SessionInfo = resp.json().await?;
        Ok(info.into())
    }

    /// Fork an existing session.  The fork inherits the parent's workspace
    /// context (open files, MCP connections, conversation history) but runs
    /// independently from the point of forking onward.
    pub async fn fork_session(&self, session_id: &str) -> KiloResult<KiloSession> {
        let url = format!("{}/session/{session_id}/fork", self.config.base_url);
        let builder = self.auth(self.http.post(&url));
        let resp = builder.send().await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(KiloError::api(status, body));
        }

        let info: SessionInfo = resp.json().await?;
        Ok(info.into())
    }

    /// Close a session and release its server-side resources.
    ///
    /// Silently ignores 404 errors (already-closed sessions).
    pub async fn close_session(&self, session_id: &str) -> KiloResult<()> {
        let url = format!("{}/session/{session_id}", self.config.base_url);
        let builder = self.auth(self.http.delete(&url));
        let resp = builder.send().await?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            // Already closed — not an error.
            return Ok(());
        }
        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(KiloError::api(status, body));
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Chat
    // -----------------------------------------------------------------------

    /// Send a message to an existing session (non-blocking — starts generation).
    ///
    /// The response stream is consumed separately via [`subscribe_events`].
    pub async fn send_message(&self, session_id: &str, msg: KiloMessage) -> KiloResult<()> {
        let url = format!("{}/session/{session_id}/chat", self.config.base_url);
        let builder = self.auth(self.http.post(&url)).json(&msg);
        let resp = builder.send().await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(KiloError::api(status, body));
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // SSE event subscription
    // -----------------------------------------------------------------------

    /// Subscribe to the SSE event stream for a session.
    ///
    /// Returns an `mpsc::Receiver<KiloEvent>` that yields events until the
    /// `Done` or `Error` event is received, at which point the sender is
    /// dropped and the channel closes.
    ///
    /// File-change events are forwarded through the same channel — the caller
    /// decides how to handle them.
    pub async fn subscribe_events(
        &self,
        session_id: &str,
    ) -> KiloResult<mpsc::Receiver<KiloEvent>> {
        let url = format!("{}/session/{session_id}/event", self.config.base_url);
        let builder = self
            .auth(self.http.get(&url))
            .header("Accept", "text/event-stream")
            .header("Cache-Control", "no-cache");

        let resp = builder.send().await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(KiloError::api(status, body));
        }

        let (tx, rx) = mpsc::channel::<KiloEvent>(128);
        let session_id = session_id.to_owned();

        tokio::spawn(async move {
            let mut stream = resp.bytes_stream();
            let mut buffer = String::new();

            while let Some(result) = stream.next().await {
                let bytes = match result {
                    Ok(b) => b,
                    Err(e) => {
                        warn!("Kilo SSE read error (session {}): {e}", session_id);
                        break;
                    }
                };

                buffer.push_str(&String::from_utf8_lossy(&bytes));

                // Process complete SSE lines.
                while let Some(newline_pos) = buffer.find('\n') {
                    let line = buffer[..newline_pos].trim().to_owned();
                    buffer.drain(..=newline_pos);

                    match parse_sse_line(&line) {
                        None => {} // blank / comment
                        Some(Ok(event)) => {
                            let is_terminal = matches!(
                                event,
                                KiloEvent::Done { .. } | KiloEvent::Error { .. }
                            );
                            if tx.send(event).await.is_err() {
                                return; // receiver dropped
                            }
                            if is_terminal {
                                return;
                            }
                        }
                        Some(Err(e)) => {
                            debug!("Skipping malformed Kilo SSE line: {e}");
                        }
                    }
                }
            }

            // Stream ended without a terminal event.
            let _ = tx
                .send(KiloEvent::Error {
                    message: "SSE stream ended without a done event".into(),
                    code: Some("stream_interrupted".into()),
                })
                .await;
        });

        Ok(rx)
    }

    // -----------------------------------------------------------------------
    // Global event stream
    // -----------------------------------------------------------------------

    /// Subscribe to the global event stream (`/global/event`).
    ///
    /// This stream emits cross-session events such as server status changes.
    /// It uses the same [`KiloEvent`] type — events not meaningful in this
    /// context will simply be skipped by the caller.
    pub async fn subscribe_global_events(&self) -> KiloResult<mpsc::Receiver<KiloEvent>> {
        let url = format!("{}/global/event", self.config.base_url);
        let builder = self
            .auth(self.http.get(&url))
            .header("Accept", "text/event-stream")
            .header("Cache-Control", "no-cache");

        let resp = builder.send().await?;
        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(KiloError::api(status, body));
        }

        let (tx, rx) = mpsc::channel::<KiloEvent>(64);
        tokio::spawn(async move {
            let mut stream = resp.bytes_stream();
            let mut buffer = String::new();

            while let Some(result) = stream.next().await {
                let bytes = match result {
                    Ok(b) => b,
                    Err(e) => {
                        warn!("Kilo global SSE read error: {e}");
                        break;
                    }
                };
                buffer.push_str(&String::from_utf8_lossy(&bytes));
                while let Some(pos) = buffer.find('\n') {
                    let line = buffer[..pos].trim().to_owned();
                    buffer.drain(..=pos);
                    if let Some(Ok(event)) = parse_sse_line(&line) {
                        if tx.send(event).await.is_err() {
                            return;
                        }
                    }
                }
            }
        });

        Ok(rx)
    }

    // -----------------------------------------------------------------------
    // MCP
    // -----------------------------------------------------------------------

    /// List all MCP servers currently managed by Kilo.
    pub async fn list_mcp_servers(&self) -> KiloResult<Vec<KiloMcpServer>> {
        let url = format!("{}/mcp", self.config.base_url);
        let builder = self.auth(self.http.get(&url));
        let resp = builder.send().await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(KiloError::api(status, body));
        }

        let data: McpListResponse = resp.json().await?;
        Ok(data.servers)
    }

    // -----------------------------------------------------------------------
    // PTY
    // -----------------------------------------------------------------------

    /// Create a new managed PTY session.
    pub async fn create_pty(
        &self,
        command: impl Into<String>,
        cols: Option<u32>,
        rows: Option<u32>,
        cwd: Option<String>,
    ) -> KiloResult<KiloPtySession> {
        let url = format!("{}/pty", self.config.base_url);
        let req = CreatePtyRequest {
            command: command.into(),
            cols,
            rows,
            cwd,
        };
        let builder = self.auth(self.http.post(&url)).json(&req);
        let resp = builder.send().await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(KiloError::api(status, body));
        }

        Ok(resp.json().await?)
    }

    /// Write data to an active PTY session (keyboard input).
    pub async fn pty_write(&self, pty_id: &str, data: &str) -> KiloResult<()> {
        let url = format!("{}/pty/{pty_id}/write", self.config.base_url);
        let payload = serde_json::json!({ "data": data });
        let builder = self.auth(self.http.post(&url)).json(&payload);
        let resp = builder.send().await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(KiloError::api(status, body));
        }
        Ok(())
    }

    /// Close a PTY session (sends SIGHUP/EOF).
    pub async fn close_pty(&self, pty_id: &str) -> KiloResult<()> {
        let url = format!("{}/pty/{pty_id}", self.config.base_url);
        let builder = self.auth(self.http.delete(&url));
        let resp = builder.send().await?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(());
        }
        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(KiloError::api(status, body));
        }
        Ok(())
    }
}
