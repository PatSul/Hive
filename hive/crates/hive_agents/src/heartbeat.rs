//! Heartbeat Service — monitor agent liveness via periodic heartbeats.
//!
//! Each agent records a heartbeat with its current status and optional task.
//! The service tracks these in memory and can identify agents that have gone
//! silent (exceeded the configurable timeout).

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

// ---------------------------------------------------------------------------
// Data Types
// ---------------------------------------------------------------------------

/// A single heartbeat record for an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentHeartbeat {
    pub agent_id: String,
    pub last_beat: DateTime<Utc>,
    pub status: String,
    pub current_task: Option<String>,
}

// ---------------------------------------------------------------------------
// Heartbeat Service
// ---------------------------------------------------------------------------

/// In-memory heartbeat tracker for agents.
///
/// Agents call [`beat`] periodically. The service considers an agent "dead"
/// if it has not sent a heartbeat within `timeout_secs` seconds.
#[derive(Serialize, Deserialize)]
pub struct HeartbeatService {
    heartbeats: HashMap<String, AgentHeartbeat>,
    timeout_secs: u64,
}

impl HeartbeatService {
    /// Create a new heartbeat service with the given timeout in seconds.
    pub fn new(timeout_secs: u64) -> Self {
        Self {
            heartbeats: HashMap::new(),
            timeout_secs,
        }
    }

    /// Record a heartbeat for an agent.
    pub fn beat(
        &mut self,
        agent_id: impl Into<String>,
        status: impl Into<String>,
        current_task: Option<String>,
    ) {
        let agent_id = agent_id.into();
        let heartbeat = AgentHeartbeat {
            agent_id: agent_id.clone(),
            last_beat: Utc::now(),
            status: status.into(),
            current_task,
        };
        self.heartbeats.insert(agent_id, heartbeat);
    }

    /// Check whether an agent has sent a heartbeat within the timeout window.
    pub fn is_alive(&self, agent_id: &str) -> bool {
        match self.heartbeats.get(agent_id) {
            Some(hb) => {
                let elapsed = Utc::now().signed_duration_since(hb.last_beat).num_seconds();
                elapsed < self.timeout_secs as i64
            }
            None => false,
        }
    }

    /// Return the IDs of all agents whose last heartbeat exceeds the timeout.
    pub fn dead_agents(&self) -> Vec<String> {
        let now = Utc::now();
        self.heartbeats
            .values()
            .filter(|hb| {
                let elapsed = now.signed_duration_since(hb.last_beat).num_seconds();
                elapsed >= self.timeout_secs as i64
            })
            .map(|hb| hb.agent_id.clone())
            .collect()
    }

    /// Return references to all current heartbeat records.
    pub fn all_heartbeats(&self) -> Vec<&AgentHeartbeat> {
        self.heartbeats.values().collect()
    }

    /// Remove an agent's heartbeat record entirely.
    pub fn remove(&mut self, agent_id: &str) {
        self.heartbeats.remove(agent_id);
    }

    /// Return the number of tracked agents.
    pub fn count(&self) -> usize {
        self.heartbeats.len()
    }

    /// Return the configured timeout in seconds.
    pub fn timeout_secs(&self) -> u64 {
        self.timeout_secs
    }

    // -----------------------------------------------------------------------
    // Persistence
    // -----------------------------------------------------------------------

    /// Persist the heartbeat service to a JSON file.
    pub fn save_to_file(&self, path: &Path) -> Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Load a heartbeat service from a JSON file. Returns an empty service
    /// with the given timeout if the file does not exist.
    pub fn load_from_file(path: &Path, default_timeout_secs: u64) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::new(default_timeout_secs));
        }
        let json = std::fs::read_to_string(path)?;
        Ok(serde_json::from_str(&json)?)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn new_service_has_no_heartbeats() {
        let svc = HeartbeatService::new(60);
        assert_eq!(svc.count(), 0);
        assert!(svc.all_heartbeats().is_empty());
        assert_eq!(svc.timeout_secs(), 60);
    }

    #[test]
    fn beat_registers_agent() {
        let mut svc = HeartbeatService::new(60);
        svc.beat("agent-1", "active", Some("task-A".into()));

        assert_eq!(svc.count(), 1);
        assert!(svc.is_alive("agent-1"));

        let hbs = svc.all_heartbeats();
        assert_eq!(hbs.len(), 1);
        assert_eq!(hbs[0].agent_id, "agent-1");
        assert_eq!(hbs[0].status, "active");
        assert_eq!(hbs[0].current_task.as_deref(), Some("task-A"));
    }

    #[test]
    fn beat_updates_existing_agent() {
        let mut svc = HeartbeatService::new(60);
        svc.beat("agent-1", "idle", None);
        svc.beat("agent-1", "working", Some("task-B".into()));

        assert_eq!(svc.count(), 1);
        let hbs = svc.all_heartbeats();
        assert_eq!(hbs[0].status, "working");
        assert_eq!(hbs[0].current_task.as_deref(), Some("task-B"));
    }

    #[test]
    fn is_alive_returns_false_for_unknown_agent() {
        let svc = HeartbeatService::new(60);
        assert!(!svc.is_alive("unknown"));
    }

    #[test]
    fn is_alive_returns_true_for_recent_heartbeat() {
        let mut svc = HeartbeatService::new(60);
        svc.beat("agent-1", "active", None);
        assert!(svc.is_alive("agent-1"));
    }

    #[test]
    fn dead_agents_detects_timed_out_agents() {
        let mut svc = HeartbeatService::new(30);

        // Manually insert a heartbeat with an old timestamp.
        svc.heartbeats.insert(
            "stale-agent".into(),
            AgentHeartbeat {
                agent_id: "stale-agent".into(),
                last_beat: Utc::now() - Duration::seconds(120),
                status: "active".into(),
                current_task: None,
            },
        );

        // Insert a fresh heartbeat.
        svc.beat("fresh-agent", "active", None);

        let dead = svc.dead_agents();
        assert_eq!(dead.len(), 1);
        assert_eq!(dead[0], "stale-agent");
    }

    #[test]
    fn dead_agents_empty_when_all_alive() {
        let mut svc = HeartbeatService::new(300);
        svc.beat("a", "active", None);
        svc.beat("b", "idle", None);

        let dead = svc.dead_agents();
        assert!(dead.is_empty());
    }

    #[test]
    fn remove_deletes_agent_record() {
        let mut svc = HeartbeatService::new(60);
        svc.beat("agent-1", "active", None);
        svc.beat("agent-2", "idle", None);
        assert_eq!(svc.count(), 2);

        svc.remove("agent-1");
        assert_eq!(svc.count(), 1);
        assert!(!svc.is_alive("agent-1"));
        assert!(svc.is_alive("agent-2"));
    }

    #[test]
    fn remove_nonexistent_is_noop() {
        let mut svc = HeartbeatService::new(60);
        svc.beat("agent-1", "active", None);

        svc.remove("ghost");
        assert_eq!(svc.count(), 1);
    }

    #[test]
    fn multiple_agents_tracked_independently() {
        let mut svc = HeartbeatService::new(60);
        svc.beat("alpha", "active", Some("task-1".into()));
        svc.beat("bravo", "idle", None);
        svc.beat("charlie", "working", Some("task-2".into()));

        assert_eq!(svc.count(), 3);
        assert!(svc.is_alive("alpha"));
        assert!(svc.is_alive("bravo"));
        assert!(svc.is_alive("charlie"));

        let hbs = svc.all_heartbeats();
        assert_eq!(hbs.len(), 3);
    }

    #[test]
    fn heartbeat_serde_round_trip() {
        let hb = AgentHeartbeat {
            agent_id: "test-agent".into(),
            last_beat: Utc::now(),
            status: "active".into(),
            current_task: Some("important-task".into()),
        };
        let json = serde_json::to_string(&hb).unwrap();
        let parsed: AgentHeartbeat = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.agent_id, "test-agent");
        assert_eq!(parsed.status, "active");
        assert_eq!(parsed.current_task.as_deref(), Some("important-task"));
    }

    #[test]
    fn heartbeat_with_none_task() {
        let mut svc = HeartbeatService::new(60);
        svc.beat("agent-no-task", "idle", None);

        let hbs = svc.all_heartbeats();
        let hb = hbs.iter().find(|h| h.agent_id == "agent-no-task").unwrap();
        assert!(hb.current_task.is_none());
    }

    #[test]
    fn save_and_load_file_round_trip() {
        let dir = std::env::temp_dir().join("hive-heartbeat-test");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("heartbeats.json");

        let mut svc = HeartbeatService::new(120);
        svc.beat("alpha", "active", Some("task-A".into()));
        svc.beat("bravo", "idle", None);
        svc.beat("charlie", "working", Some("task-C".into()));

        svc.save_to_file(&path).unwrap();
        let loaded = HeartbeatService::load_from_file(&path, 120).unwrap();

        assert_eq!(loaded.count(), 3);
        assert_eq!(loaded.timeout_secs(), 120);
        assert!(loaded.is_alive("alpha"));
        assert!(loaded.is_alive("bravo"));
        assert!(loaded.is_alive("charlie"));

        let hbs = loaded.all_heartbeats();
        let alpha = hbs.iter().find(|h| h.agent_id == "alpha").unwrap();
        assert_eq!(alpha.status, "active");
        assert_eq!(alpha.current_task.as_deref(), Some("task-A"));

        // Clean up
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn load_missing_file_returns_empty_service() {
        let path = std::env::temp_dir().join("nonexistent-hive-heartbeats.json");
        let svc = HeartbeatService::load_from_file(&path, 60).unwrap();
        assert_eq!(svc.count(), 0);
        assert_eq!(svc.timeout_secs(), 60);
    }
}
