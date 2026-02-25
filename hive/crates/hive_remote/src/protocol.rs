use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Events exchanged between GUI, daemon, and web clients.
/// Serialized as JSON over local WebSocket or relay.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DaemonEvent {
    // Client -> Daemon
    SendMessage {
        conversation_id: String,
        content: String,
        model: String,
    },
    SwitchPanel {
        panel: String,
    },
    StartAgentTask {
        goal: String,
        orchestration_mode: String,
    },
    CancelAgentTask {
        run_id: String,
    },

    // Daemon -> Client
    StreamChunk {
        conversation_id: String,
        chunk: String,
    },
    StreamComplete {
        conversation_id: String,
        prompt_tokens: u32,
        completion_tokens: u32,
        cost_usd: Option<f64>,
    },
    AgentStatus {
        run_id: String,
        status: String,
        detail: String,
    },
    StateSnapshot(SessionSnapshot),
    PanelData {
        panel: String,
        data: serde_json::Value,
    },
    Error {
        code: u16,
        message: String,
    },

    // Bidirectional
    Ping,
    Pong,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSnapshot {
    pub active_conversation: Option<String>,
    pub active_panel: String,
    pub agent_runs: Vec<AgentRunSummary>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRunSummary {
    pub run_id: String,
    pub goal: String,
    pub status: String,
    pub cost_usd: f64,
    pub elapsed_ms: u64,
}
