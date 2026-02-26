use crate::daemon::HiveDaemon;
use crate::protocol::DaemonEvent;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json};
use futures::stream::StreamExt;
use futures::SinkExt;
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Shared daemon state wrapped for concurrent access by axum handlers.
pub type DaemonState = Arc<RwLock<HiveDaemon>>;

/// GET /api/state -- returns current SessionSnapshot as JSON.
pub async fn get_state(State(daemon): State<DaemonState>) -> Json<serde_json::Value> {
    let daemon = daemon.read().await;
    let snapshot = daemon.get_snapshot();
    Json(serde_json::to_value(snapshot).unwrap_or_default())
}

/// Request body for POST /api/chat.
#[derive(Deserialize)]
pub struct ChatRequest {
    pub conversation_id: String,
    pub content: String,
    pub model: String,
}

/// POST /api/chat -- send a chat message to the daemon.
pub async fn send_message(
    State(daemon): State<DaemonState>,
    Json(req): Json<ChatRequest>,
) -> StatusCode {
    let mut daemon = daemon.write().await;
    daemon
        .handle_event(DaemonEvent::SendMessage {
            conversation_id: req.conversation_id,
            content: req.content,
            model: req.model,
        })
        .await;
    StatusCode::OK
}

/// GET /api/panels/{panel_id} -- get panel-specific data.
pub async fn get_panel(
    State(_daemon): State<DaemonState>,
    axum::extract::Path(panel_id): axum::extract::Path<String>,
) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "panel": panel_id, "data": {} }))
}

/// Request body for POST /api/agents.
#[derive(Deserialize)]
pub struct AgentRequest {
    pub goal: String,
    pub orchestration_mode: String,
}

/// POST /api/agents -- start an agent task.
pub async fn agent_action(
    State(daemon): State<DaemonState>,
    Json(req): Json<AgentRequest>,
) -> Json<serde_json::Value> {
    let mut daemon = daemon.write().await;
    daemon
        .handle_event(DaemonEvent::StartAgentTask {
            goal: req.goal,
            orchestration_mode: req.orchestration_mode,
        })
        .await;
    Json(serde_json::json!({"status": "started"}))
}

/// GET /ws -- WebSocket upgrade handler for the event stream.
pub async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(daemon): State<DaemonState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_websocket(socket, daemon))
}

/// Bidirectional WebSocket handler: forwards daemon events to the client,
/// and dispatches client messages to the daemon.
async fn handle_websocket(socket: WebSocket, daemon: DaemonState) {
    let (mut sender, mut receiver) = socket.split();

    // Send initial state snapshot
    {
        let d = daemon.read().await;
        let snapshot = d.get_snapshot();
        let event = DaemonEvent::StateSnapshot(snapshot);
        if let Ok(json) = serde_json::to_string(&event) {
            let _ = sender.send(Message::Text(json.into())).await;
        }
    }

    // Subscribe to daemon events
    let mut rx = {
        let d = daemon.read().await;
        d.subscribe()
    };

    // Forward daemon events to WebSocket
    let send_task = tokio::spawn(async move {
        while let Ok(event) = rx.recv().await {
            if let Ok(json) = serde_json::to_string(&event) {
                if sender.send(Message::Text(json.into())).await.is_err() {
                    break;
                }
            }
        }
    });

    // Receive client messages and dispatch to daemon
    let daemon_clone = daemon.clone();
    let recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            if let Message::Text(text) = msg {
                if let Ok(event) = serde_json::from_str::<DaemonEvent>(&text) {
                    let mut d = daemon_clone.write().await;
                    d.handle_event(event).await;
                }
            }
        }
    });

    tokio::select! {
        _ = send_task => {},
        _ = recv_task => {},
    }
}
