use crate::relay::RelayFrame;
use futures::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use tracing::{error, info, warn};

/// A simple relay client for connecting to the hive-cloud WebSocket hub.
pub struct RelayClient {
    pub server_url: String,
}

impl RelayClient {
    pub fn new(url: &str) -> Self {
        Self {
            server_url: url.to_string(),
        }
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
            session_token: "TODO_JWT".into(),
            node_id: node_id.to_string(),
        });

        Ok((tx_in, rx_out))
    }
}
