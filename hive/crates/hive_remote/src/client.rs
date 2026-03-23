use crate::relay::RelayFrame;
use anyhow::Context;
use futures::{SinkExt, StreamExt};
use hive_core::config::HiveConfig;
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use tracing::{error, info, warn};

/// A simple relay client for connecting to the hive-cloud WebSocket hub.
pub struct RelayClient {
    server_url: String,
}

impl RelayClient {
    /// Create a new relay client.
    ///
    /// Requires `wss://` URLs in production. `ws://localhost` and
    /// `ws://127.0.0.1` are allowed for local development only.
    pub fn new(url: &str) -> Result<Self, String> {
        if !url.starts_with("wss://")
            && !url.starts_with("ws://localhost")
            && !url.starts_with("ws://127.0.0.1")
        {
            return Err("Relay URL must use wss:// (except localhost for development)".into());
        }
        Ok(Self {
            server_url: url.to_string(),
        })
    }

    /// Return the relay server URL.
    pub fn server_url(&self) -> &str {
        &self.server_url
    }

    /// Connects to the relay server and spawns background tasks for reading and writing.
    /// Returns a channel to send `RelayFrame`s directly to the server, and another to receive them.
    pub async fn connect(
        &self,
        node_id: &str,
    ) -> anyhow::Result<(
        mpsc::UnboundedSender<RelayFrame>,
        mpsc::UnboundedReceiver<RelayFrame>,
    )> {
        let config_token = HiveConfig::load().ok().and_then(|config| config.cloud_jwt);
        let session_token = Self::resolve_session_token(
            std::env::var("HIVE_SESSION_TOKEN").ok(),
            std::env::var("HIVE_CLOUD_JWT").ok().or(config_token),
        )
        .context(
            "No relay session token configured. Set HIVE_SESSION_TOKEN or log in to Hive Cloud.",
        )?;

        self.connect_with_token(node_id, &session_token).await
    }

    pub async fn connect_with_token(
        &self,
        node_id: &str,
        session_token: &str,
    ) -> anyhow::Result<(
        mpsc::UnboundedSender<RelayFrame>,
        mpsc::UnboundedReceiver<RelayFrame>,
    )> {
        let (ws_stream, _) = connect_async(&self.server_url).await?;
        info!("Connected to relay server at {}", self.server_url);

        let (mut sender, mut receiver) = ws_stream.split();
        let (tx_in, mut rx_in) = mpsc::unbounded_channel::<RelayFrame>();
        let (tx_out, rx_out) = mpsc::unbounded_channel::<RelayFrame>();

        // Background write task
        tokio::spawn(async move {
            while let Some(frame) = rx_in.recv().await {
                if let Ok(text) = serde_json::to_string(&frame) {
                    if let Err(e) = sender.send(Message::Text(text.into())).await {
                        warn!("Failed to send frame to relay: {}", e);
                        break;
                    }
                }
            }
        });

        // Background read task
        let node_id_clone = node_id.to_string();
        tokio::spawn(async move {
            while let Some(msg) = receiver.next().await {
                match msg {
                    Ok(Message::Text(text)) => {
                        if let Ok(frame) = serde_json::from_str::<RelayFrame>(&text) {
                            let _ = tx_out.send(frame);
                        }
                    }
                    Ok(Message::Close(_)) => {
                        info!("Relay server closed connection.");
                        break;
                    }
                    Err(e) => {
                        error!("Error receiving from relay: {}", e);
                        break;
                    }
                    _ => {}
                }
            }
            warn!("Relay read loop for node {} terminated.", node_id_clone);
        });

        // Immediately send a Register frame upon connection
        let _ = tx_in.send(RelayFrame::Register {
            session_token: session_token.to_string(),
            node_id: node_id.to_string(),
        });

        Ok((tx_in, rx_out))
    }

    fn resolve_session_token(
        env_token: Option<String>,
        config_token: Option<String>,
    ) -> Option<String> {
        env_token
            .filter(|token| !token.trim().is_empty())
            .or_else(|| config_token.filter(|token| !token.trim().is_empty()))
    }
}

#[cfg(test)]
mod tests {
    use super::RelayClient;

    #[test]
    fn resolve_session_token_prefers_env() {
        let token = RelayClient::resolve_session_token(
            Some("env-token".into()),
            Some("config-token".into()),
        );
        assert_eq!(token.as_deref(), Some("env-token"));
    }

    #[test]
    fn resolve_session_token_uses_config_fallback() {
        let token = RelayClient::resolve_session_token(None, Some("config-token".into()));
        assert_eq!(token.as_deref(), Some("config-token"));
    }

    #[test]
    fn resolve_session_token_rejects_blank_values() {
        let token = RelayClient::resolve_session_token(Some("   ".into()), Some("".into()));
        assert!(token.is_none());
    }

    #[test]
    fn new_accepts_wss_url() {
        let client = RelayClient::new("wss://relay.hive.cloud/ws");
        assert!(client.is_ok());
        assert_eq!(client.unwrap().server_url(), "wss://relay.hive.cloud/ws");
    }

    #[test]
    fn new_accepts_ws_localhost() {
        assert!(RelayClient::new("ws://localhost:8080/ws").is_ok());
        assert!(RelayClient::new("ws://127.0.0.1:8080/ws").is_ok());
    }

    #[test]
    fn new_rejects_plain_ws() {
        let result = RelayClient::new("ws://relay.example.com/ws");
        assert!(result.is_err());
    }

    #[test]
    fn new_rejects_http() {
        let result = RelayClient::new("http://relay.example.com/ws");
        assert!(result.is_err());
    }
}
