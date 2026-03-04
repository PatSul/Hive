use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
    routing::get,
    Router,
};
use futures::{sink::SinkExt, stream::StreamExt};
use hive_remote::relay::RelayFrame;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, warn};

type Tx = mpsc::UnboundedSender<Message>;

#[derive(Default)]
struct AppState {
    rooms: RwLock<HashMap<String, RoomState>>,
}

struct RoomState {
    clients: HashMap<String, Tx>, // node_id -> sender
}

pub fn router() -> Router {
    let state = Arc::new(AppState::default());
    Router::new()
        .route("/ws", get(ws_handler))
        .with_state(state)
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<Arc<AppState>>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: Arc<AppState>) {
    let (mut sender, mut receiver) = socket.split();
    let (tx, mut rx) = mpsc::unbounded_channel();

    // Spawn a task to forward messages from our internal channel to the websocket
    let mut send_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if sender.send(msg).await.is_err() {
                break;
            }
        }
    });

    let mut current_node_id: Option<String> = None;
    let mut current_room_id: Option<String> = None;

    let mut recv_task = {
        let state = state.clone();
        tokio::spawn(async move {
            while let Some(Ok(msg)) = receiver.next().await {
                if let Message::Text(text) = msg {
                    match serde_json::from_str::<RelayFrame>(&text) {
                        Ok(frame) => match frame {
                            RelayFrame::Register { node_id, .. } => {
                                current_node_id = Some(node_id.clone());
                                debug!("Node registered: {}", node_id);
                            }
                            RelayFrame::JoinRoom { room_id, .. } | RelayFrame::CreateRoom { room_id, .. } => {
                                if let Some(ref node) = current_node_id {
                                    current_room_id = Some(room_id.clone());
                                    let mut rooms = state.rooms.write().await;
                                    let room = rooms.entry(room_id.clone()).or_insert_with(|| RoomState {
                                        clients: HashMap::new(),
                                    });
                                    room.clients.insert(node.clone(), tx.clone());
                                    info!("Node {} joined room {}", node, room_id);
                                } else {
                                    warn!("Received Join/Create room before register");
                                }
                            }
                            RelayFrame::Forward { to, payload } => {
                                if let (Some(room_id), Some(sender_node)) = (&current_room_id, &current_node_id) {
                                    let rooms = state.rooms.read().await;
                                    if let Some(room) = rooms.get(room_id) {
                                        let outbound = RelayFrame::Forward {
                                            to: to.clone(),
                                            payload,
                                        };
                                        let out_text = serde_json::to_string(&outbound).unwrap_or_default();
                                        
                                        if let Some(target) = to {
                                            if let Some(client_tx) = room.clients.get(&target) {
                                                let _ = client_tx.send(Message::Text(out_text.into()));
                                            }
                                        } else {
                                            // Broadcast to all except sender
                                            for (node, client_tx) in room.clients.iter() {
                                                if node != sender_node {
                                                    let _ = client_tx.send(Message::Text(out_text.clone().into()));
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            RelayFrame::Ping => {
                                let pong = serde_json::to_string(&RelayFrame::Pong).unwrap_or_default();
                                let _ = tx.send(Message::Text(pong.into()));
                            }
                            _ => debug!("Unhandled frame: {:?}", frame),
                        },
                        Err(e) => {
                            error!("Failed to parse RelayFrame: {}", e);
                            let error_frame = RelayFrame::Error {
                                code: 400,
                                message: format!("Parse error: {}", e),
                            };
                            let out = serde_json::to_string(&error_frame).unwrap_or_default();
                            let _ = tx.send(Message::Text(out.into()));
                        }
                    }
                }
            }
            
            // Cleanup on disconnect
            if let (Some(room_id), Some(node_id)) = (current_room_id, current_node_id) {
                let mut rooms = state.rooms.write().await;
                if let Some(room) = rooms.get_mut(&room_id) {
                    room.clients.remove(&node_id);
                    if room.clients.is_empty() {
                        rooms.remove(&room_id);
                    }
                }
            }
        })
    };

    tokio::select! {
        _ = (&mut send_task) => recv_task.abort(),
        _ = (&mut recv_task) => send_task.abort(),
    };
}
