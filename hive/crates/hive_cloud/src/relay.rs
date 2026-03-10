use axum::{
    Router,
    extract::{
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::IntoResponse,
    routing::get,
};
use chrono::{DateTime, Utc};
use futures::{sink::SinkExt, stream::StreamExt};
use hive_remote::relay::RelayFrame;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::{RwLock, mpsc};
use tracing::{debug, error, info, warn};

type Tx = mpsc::UnboundedSender<Message>;

#[derive(Default)]
pub struct RelayService {
    rooms: RwLock<HashMap<String, RoomState>>,
}

struct RoomState {
    clients: HashMap<String, Tx>, // node_id -> sender
    created_at: DateTime<Utc>,
    bytes_transferred: u64,
}

#[derive(Debug, Clone)]
pub struct RelayRoomSnapshot {
    pub room_id: String,
    pub participants: u32,
    pub created_at: DateTime<Utc>,
    pub bytes_transferred: u64,
}

#[derive(Debug, Clone)]
pub struct RelaySnapshot {
    pub active_rooms: u64,
    pub connected_devices: u64,
    pub rooms: Vec<RelayRoomSnapshot>,
}

pub fn router(state: Arc<RelayService>) -> Router {
    Router::new()
        .route("/ws", get(ws_handler))
        .with_state(state)
}

impl RelayService {
    pub async fn snapshot(&self) -> RelaySnapshot {
        let rooms = self.rooms.read().await;
        let mut room_snapshots = rooms
            .iter()
            .map(|(room_id, room)| RelayRoomSnapshot {
                room_id: room_id.clone(),
                participants: room.clients.len() as u32,
                created_at: room.created_at,
                bytes_transferred: room.bytes_transferred,
            })
            .collect::<Vec<_>>();
        room_snapshots.sort_by(|left, right| left.room_id.cmp(&right.room_id));

        RelaySnapshot {
            active_rooms: room_snapshots.len() as u64,
            connected_devices: room_snapshots
                .iter()
                .map(|room| u64::from(room.participants))
                .sum(),
            rooms: room_snapshots,
        }
    }

    #[cfg(test)]
    pub async fn seed_room_for_test(
        &self,
        room_id: &str,
        participants: &[&str],
        bytes_transferred: u64,
    ) {
        let clients = participants
            .iter()
            .map(|participant| {
                let (tx, _) = mpsc::unbounded_channel();
                ((*participant).to_string(), tx)
            })
            .collect();

        self.rooms.write().await.insert(
            room_id.to_string(),
            RoomState {
                clients,
                created_at: Utc::now(),
                bytes_transferred,
            },
        );
    }
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<RelayService>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: Arc<RelayService>) {
    let (mut sender, mut receiver) = socket.split();
    let (tx, mut rx) = mpsc::unbounded_channel();
    let tx_for_recv = tx.clone();

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
                            RelayFrame::JoinRoom { room_id, .. }
                            | RelayFrame::CreateRoom { room_id, .. } => {
                                if let Some(ref node) = current_node_id {
                                    current_room_id = Some(room_id.clone());
                                    let mut rooms = state.rooms.write().await;
                                    let room =
                                        rooms.entry(room_id.clone()).or_insert_with(|| RoomState {
                                            clients: HashMap::new(),
                                            created_at: Utc::now(),
                                            bytes_transferred: 0,
                                        });
                                    room.clients.insert(node.clone(), tx_for_recv.clone());
                                    info!("Node {} joined room {}", node, room_id);
                                } else {
                                    warn!("Received Join/Create room before register");
                                }
                            }
                            RelayFrame::Forward { to, payload } => {
                                if let (Some(room_id), Some(sender_node)) =
                                    (&current_room_id, &current_node_id)
                                {
                                    let outbound = RelayFrame::Forward {
                                        to: to.clone(),
                                        payload,
                                    };
                                    let out_text =
                                        serde_json::to_string(&outbound).unwrap_or_default();
                                    let message_bytes = out_text.len() as u64;
                                    let mut rooms = state.rooms.write().await;
                                    if let Some(room) = rooms.get_mut(room_id) {
                                        let mut delivered = 0_u64;
                                        if let Some(target) = to {
                                            if let Some(client_tx) = room.clients.get(&target) {
                                                let _ =
                                                    client_tx.send(Message::Text(out_text.into()));
                                                delivered = 1;
                                            }
                                        } else {
                                            // Broadcast to all except sender
                                            for (node, client_tx) in room.clients.iter() {
                                                if node != sender_node {
                                                    let _ = client_tx.send(Message::Text(
                                                        out_text.clone().into(),
                                                    ));
                                                    delivered += 1;
                                                }
                                            }
                                        }
                                        room.bytes_transferred =
                                            room.bytes_transferred.saturating_add(
                                                message_bytes.saturating_mul(delivered),
                                            );
                                    }
                                }
                            }
                            RelayFrame::LeaveRoom => {
                                if let (Some(room_id), Some(node_id)) =
                                    (current_room_id.take(), current_node_id.as_ref())
                                {
                                    remove_node_from_room(&state, &room_id, node_id).await;
                                }
                            }
                            RelayFrame::Ping => {
                                let pong =
                                    serde_json::to_string(&RelayFrame::Pong).unwrap_or_default();
                                let _ = tx_for_recv.send(Message::Text(pong.into()));
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
                            let _ = tx_for_recv.send(Message::Text(out.into()));
                        }
                    }
                }
            }

            // Cleanup on disconnect
            if let (Some(room_id), Some(node_id)) = (current_room_id, current_node_id.as_ref()) {
                remove_node_from_room(&state, &room_id, node_id).await;
            }
        })
    };

    tokio::select! {
        _ = (&mut send_task) => recv_task.abort(),
        _ = (&mut recv_task) => send_task.abort(),
    };
}

async fn remove_node_from_room(state: &RelayService, room_id: &str, node_id: &str) {
    let mut rooms = state.rooms.write().await;
    if let Some(room) = rooms.get_mut(room_id) {
        room.clients.remove(node_id);
        if room.clients.is_empty() {
            rooms.remove(room_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn snapshot_reports_rooms_participants_and_bytes() {
        let service = RelayService::default();
        let (tx_one, _) = mpsc::unbounded_channel();
        let (tx_two, _) = mpsc::unbounded_channel();

        {
            let mut rooms = service.rooms.write().await;
            rooms.insert(
                "room-1".into(),
                RoomState {
                    clients: HashMap::from([("node-a".into(), tx_one), ("node-b".into(), tx_two)]),
                    created_at: Utc::now(),
                    bytes_transferred: 512,
                },
            );
        }

        let snapshot = service.snapshot().await;
        assert_eq!(snapshot.active_rooms, 1);
        assert_eq!(snapshot.connected_devices, 2);
        assert_eq!(snapshot.rooms.len(), 1);
        assert_eq!(snapshot.rooms[0].room_id, "room-1");
        assert_eq!(snapshot.rooms[0].participants, 2);
        assert_eq!(snapshot.rooms[0].bytes_transferred, 512);
    }
}
