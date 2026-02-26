use serde::{Deserialize, Serialize};

/// A frame exchanged between client and relay server over WebSocket.
///
/// Serialized with a `"type"` tag in snake_case for easy JavaScript interop.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RelayFrame {
    /// Register this node with the relay server.
    Register {
        session_token: String,
        node_id: String,
    },
    /// Authenticate a previously paired device.
    Authenticate {
        pairing_token: String,
    },
    /// Create a new encrypted room.
    CreateRoom {
        room_id: String,
        encryption_key_fingerprint: String,
    },
    /// Join an existing room using a pairing token.
    JoinRoom {
        room_id: String,
        pairing_token: String,
    },
    /// Leave the current room.
    LeaveRoom,
    /// Forward an encrypted payload to another participant (or broadcast).
    Forward {
        to: Option<String>,
        payload: EncryptedEnvelope,
    },
    /// Heartbeat request.
    Ping,
    /// Heartbeat response.
    Pong,
    /// An error reported by the relay.
    Error {
        code: u16,
        message: String,
    },
}

/// An encrypted payload transported inside a [`RelayFrame::Forward`].
///
/// The nonce and ciphertext are produced by [`SessionKeys::encrypt`](crate::pairing::SessionKeys::encrypt).
/// The `sender_fingerprint` allows the receiver to look up which paired
/// device produced this message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedEnvelope {
    pub nonce: [u8; 12],
    pub ciphertext: Vec<u8>,
    pub sender_fingerprint: String,
}

/// Configuration for the relay subsystem.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayConfig {
    /// Whether relay functionality is enabled at all.
    pub relay_enabled: bool,
    /// Optional WAN relay URL (e.g. `wss://relay.hive.example/ws`).
    pub wan_relay_url: Option<String>,
    /// Operating mode for this node.
    pub relay_mode: RelayMode,
    /// Port for LAN relay discovery / direct connections.
    pub lan_relay_port: u16,
}

impl Default for RelayConfig {
    fn default() -> Self {
        Self {
            relay_enabled: true,
            wan_relay_url: None,
            relay_mode: RelayMode::Client,
            lan_relay_port: 9482,
        }
    }
}

/// Whether this node acts as a relay client, server, or both.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RelayMode {
    Client,
    Server,
    Both,
}
