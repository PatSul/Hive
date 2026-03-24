//! Bridge between Kilo's `/pty` endpoint and Hive's terminal layer.
//!
//! Kilo can manage PTY (pseudo-terminal) sessions on behalf of the agent.
//! `KiloPtyBridge` exposes Kilo PTY sessions as a thin wrapper that mirrors
//! the interface Hive's terminal infrastructure expects, so the UI layer can
//! treat a Kilo-managed shell identically to a local shell.
//!
//! # Status
//!
//! This is a **scaffold** — the full integration with `hive_terminal`'s trait
//! system is deferred pending finalisation of that crate's public API.  The
//! bridge today exposes the raw read/write primitives; wiring into the
//! terminal panel will be done in a follow-up phase.
//!
//! # Architecture
//!
//! ```text
//! hive_terminal::Panel  ←→  KiloPtyBridge
//!                               └──→ KiloClient::create_pty / pty_write / close_pty
//!                               └──→ GET /pty/{id}/output  SSE (terminal output stream)
//! ```

use std::sync::Arc;

use futures::StreamExt;
use tokio::sync::mpsc;
use tracing::{debug, warn};

use crate::client::{KiloClient, KiloPtySession};
use crate::error::{KiloError, KiloResult};

// ---------------------------------------------------------------------------
// PTY output event
// ---------------------------------------------------------------------------

/// A chunk of raw terminal output received from a Kilo PTY session.
#[derive(Debug, Clone)]
pub struct PtyOutput {
    /// Raw bytes (typically UTF-8 but may contain terminal escape sequences).
    pub data: String,
    /// Whether the PTY process has exited (last event before the channel closes).
    pub exit: bool,
}

// ---------------------------------------------------------------------------
// KiloPtyBridge
// ---------------------------------------------------------------------------

/// Manages a Kilo PTY session and provides read/write access to the shell.
pub struct KiloPtyBridge {
    client: Arc<KiloClient>,
    /// The active PTY session, if one has been started.
    pty: Option<KiloPtySession>,
}

impl KiloPtyBridge {
    /// Create a bridge backed by the given [`KiloClient`].  No PTY is started
    /// yet — call [`start`] to create one.
    pub fn new(client: Arc<KiloClient>) -> Self {
        Self { client, pty: None }
    }

    // -----------------------------------------------------------------------
    // Lifecycle
    // -----------------------------------------------------------------------

    /// Start a new PTY session (e.g. `"bash"` or `"zsh"`).
    ///
    /// Returns the PTY session info for bookkeeping purposes.
    pub async fn start(
        &mut self,
        command: impl Into<String>,
        cols: u32,
        rows: u32,
        cwd: Option<String>,
    ) -> KiloResult<&KiloPtySession> {
        let session = self
            .client
            .create_pty(command, Some(cols), Some(rows), cwd)
            .await?;
        debug!("KiloPtyBridge: PTY started — id={} pid={:?}", session.id, session.pid);
        self.pty = Some(session);
        Ok(self.pty.as_ref().unwrap())
    }

    /// Close the current PTY session.
    pub async fn stop(&mut self) -> KiloResult<()> {
        if let Some(ref pty) = self.pty {
            self.client.close_pty(&pty.id).await?;
            debug!("KiloPtyBridge: PTY closed — id={}", pty.id);
        }
        self.pty = None;
        Ok(())
    }

    /// Return the active PTY session, if any.
    pub fn session(&self) -> Option<&KiloPtySession> {
        self.pty.as_ref()
    }

    // -----------------------------------------------------------------------
    // Read / write
    // -----------------------------------------------------------------------

    /// Write raw input to the PTY (keyboard data, commands, etc.).
    pub async fn write(&self, data: &str) -> KiloResult<()> {
        let pty = self.pty.as_ref().ok_or_else(|| {
            KiloError::Other("KiloPtyBridge: no active PTY session".into())
        })?;
        self.client.pty_write(&pty.id, data).await
    }

    /// Subscribe to the PTY output stream.
    ///
    /// Returns an `mpsc::Receiver<PtyOutput>` that yields raw terminal data
    /// chunks until the process exits.  The channel is closed when an exit
    /// event is received.
    ///
    /// The output stream is read from `GET /pty/{id}/output` SSE.
    pub async fn subscribe_output(&self) -> KiloResult<mpsc::Receiver<PtyOutput>> {
        let pty = self.pty.as_ref().ok_or_else(|| {
            KiloError::Other("KiloPtyBridge: no active PTY session".into())
        })?;

        let url = format!("{}/pty/{}/output", self.client.config().base_url, pty.id);
        let resp = reqwest::Client::new()
            .get(&url)
            .header("Accept", "text/event-stream")
            .send()
            .await
            .map_err(KiloError::Network)?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(KiloError::api(status, body));
        }

        let (tx, rx) = mpsc::channel::<PtyOutput>(256);

        tokio::spawn(async move {
            let mut stream = resp.bytes_stream();
            let mut buffer = String::new();

            while let Some(result) = stream.next().await {
                let bytes = match result {
                    Ok(b) => b,
                    Err(e) => {
                        warn!("Kilo PTY output stream error: {e}");
                        break;
                    }
                };
                buffer.push_str(&String::from_utf8_lossy(&bytes));

                while let Some(pos) = buffer.find('\n') {
                    let line = buffer[..pos].trim().to_owned();
                    buffer.drain(..=pos);
                    if line.is_empty() || line.starts_with(':') {
                        continue;
                    }
                    let json = line.strip_prefix("data: ").unwrap_or(&line);
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(json) {
                        let data = v
                            .get("data")
                            .and_then(|d| d.as_str())
                            .unwrap_or_default()
                            .to_owned();
                        let exit = v
                            .get("type")
                            .and_then(|t| t.as_str())
                            .is_some_and(|t| t == "exit");

                        if tx
                            .send(PtyOutput { data, exit })
                            .await
                            .is_err()
                        {
                            return;
                        }
                        if exit {
                            return;
                        }
                    }
                }
            }
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
    use crate::config::KiloConfig;

    #[test]
    fn bridge_starts_without_pty() {
        let client = Arc::new(KiloClient::new(KiloConfig::default()));
        let bridge = KiloPtyBridge::new(client);
        assert!(bridge.session().is_none());
    }
}
