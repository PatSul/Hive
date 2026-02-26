use crate::protocol::{AgentRunSummary, DaemonEvent, SessionSnapshot};
use crate::session::SessionJournal;
use anyhow::Result;
use chrono::Utc;
use std::path::PathBuf;
use tokio::sync::broadcast;

/// Configuration for the HiveDaemon background service.
#[derive(Debug, Clone)]
pub struct DaemonConfig {
    pub data_dir: PathBuf,
    pub local_port: u16,
    pub web_port: u16,
    pub shutdown_grace_secs: u64,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        let data_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".hive");
        Self {
            data_dir,
            local_port: 9480,
            web_port: 9481,
            shutdown_grace_secs: 30,
        }
    }
}

/// Core background service that owns state and broadcasts events.
///
/// The daemon holds the canonical session state (active panel, conversation,
/// agent runs) and persists events via `SessionJournal`. All connected clients
/// (GUI + web) receive events through a broadcast channel.
pub struct HiveDaemon {
    config: DaemonConfig,
    journal: SessionJournal,
    active_panel: String,
    active_conversation: Option<String>,
    agent_runs: Vec<AgentRunSummary>,
    event_tx: broadcast::Sender<DaemonEvent>,
}

impl HiveDaemon {
    /// Create a new daemon with the given configuration.
    ///
    /// Opens (or creates) the session journal at `<data_dir>/session_journal.jsonl`
    /// and initialises the broadcast channel with capacity 256.
    pub fn new(config: DaemonConfig) -> Result<Self> {
        let journal_path = config.data_dir.join("session_journal.jsonl");
        let journal = SessionJournal::new(&journal_path)?;
        let (event_tx, _) = broadcast::channel(256);
        Ok(Self {
            config,
            journal,
            active_panel: "chat".into(),
            active_conversation: None,
            agent_runs: vec![],
            event_tx,
        })
    }

    /// Return a point-in-time snapshot of the daemon's session state.
    pub fn get_snapshot(&self) -> SessionSnapshot {
        SessionSnapshot {
            active_conversation: self.active_conversation.clone(),
            active_panel: self.active_panel.clone(),
            agent_runs: self.agent_runs.clone(),
            timestamp: Utc::now(),
        }
    }

    /// Subscribe to the event broadcast channel.
    pub fn subscribe(&self) -> broadcast::Receiver<DaemonEvent> {
        self.event_tx.subscribe()
    }

    /// Process an incoming event: journal it, update state, then broadcast.
    pub async fn handle_event(&mut self, event: DaemonEvent) {
        // Journal the event
        if let Err(e) = self.journal.append(&event) {
            tracing::error!("Failed to journal event: {}", e);
        }

        match &event {
            DaemonEvent::SwitchPanel { panel } => {
                self.active_panel = panel.clone();
            }
            DaemonEvent::SendMessage {
                conversation_id, ..
            } => {
                self.active_conversation = Some(conversation_id.clone());
            }
            DaemonEvent::StartAgentTask {
                goal,
                orchestration_mode: _,
            } => {
                let run_id = uuid::Uuid::new_v4().to_string();
                self.agent_runs.push(AgentRunSummary {
                    run_id,
                    goal: goal.clone(),
                    status: "planning".into(),
                    cost_usd: 0.0,
                    elapsed_ms: 0,
                });
            }
            DaemonEvent::CancelAgentTask { run_id } => {
                if let Some(run) = self.agent_runs.iter_mut().find(|r| r.run_id == *run_id) {
                    run.status = "cancelled".into();
                }
            }
            _ => {}
        }

        // Broadcast to all subscribers (GUI + web clients)
        let _ = self.event_tx.send(event);
    }

    /// Replay the journal to reconstruct daemon state from a previous session.
    pub async fn replay_journal(&mut self) -> Result<()> {
        let events = SessionJournal::replay(self.journal.path())?;
        for event in events {
            match &event {
                DaemonEvent::SwitchPanel { panel } => {
                    self.active_panel = panel.clone();
                }
                DaemonEvent::SendMessage {
                    conversation_id, ..
                } => {
                    self.active_conversation = Some(conversation_id.clone());
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Access the daemon configuration.
    pub fn config(&self) -> &DaemonConfig {
        &self.config
    }
}
