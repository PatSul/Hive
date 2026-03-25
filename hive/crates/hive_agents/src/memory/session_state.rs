//! L1 HOT: Session state with WAL persistence.
//!
//! Holds the ephemeral, in-memory state for a single agent session.
//! State can be flushed to a write-ahead log (WAL) file for crash recovery
//! and restored on startup.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

use super::TargetLayer;
use crate::collective_memory::MemoryCategory;

/// A recorded decision made during the session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decision {
    /// What was decided.
    pub content: String,
    /// When the decision was made.
    pub timestamp: DateTime<Utc>,
    /// Optional reasoning behind the decision.
    pub rationale: Option<String>,
}

/// Metadata about an entity encountered during the session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityInfo {
    /// Display name of the entity.
    pub name: String,
    /// Kind of entity: `"file"`, `"function"`, `"concept"`, `"person"`, etc.
    pub entity_type: String,
    /// When this entity was first observed in the session.
    pub first_seen: DateTime<Utc>,
    /// How many times this entity has been referenced.
    pub mentions: u32,
}

/// A memory write that has been queued but not yet flushed to a target layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingWrite {
    /// The content to persist.
    pub content: String,
    /// Which memory category this belongs to.
    pub category: MemoryCategory,
    /// Importance score (0.0 .. 1.0 typical, but unclamped).
    pub importance: f32,
    /// Which storage layer to write to.
    pub target_layer: TargetLayer,
}

/// In-memory session state for a single agent session (L1 HOT layer).
///
/// This struct is the primary working memory during a session. It tracks
/// the active context window, decisions made, entities encountered, and
/// any pending writes that should be flushed to lower memory layers.
///
/// The WAL (write-ahead log) methods allow crash-safe persistence: state
/// is serialized to a temporary file and atomically renamed into place.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionState {
    /// Unique identifier for this session.
    pub session_id: String,
    /// Current active context items (e.g. file paths, topics).
    pub active_context: Vec<String>,
    /// The task currently being worked on, if any.
    pub current_task: Option<String>,
    /// Chronological log of decisions made during the session.
    pub decisions_log: Vec<Decision>,
    /// Cache of entities encountered, keyed by entity name.
    pub entity_cache: HashMap<String, EntityInfo>,
    /// Writes queued for persistence to lower memory layers.
    pub pending_memory_writes: Vec<PendingWrite>,
    /// When this session was created.
    pub created_at: DateTime<Utc>,
    /// When the last mutation occurred.
    pub last_activity: DateTime<Utc>,
}

impl SessionState {
    /// Create a fresh session state with the given ID.
    pub fn new(session_id: String) -> Self {
        let now = Utc::now();
        Self {
            session_id,
            active_context: Vec::new(),
            current_task: None,
            decisions_log: Vec::new(),
            entity_cache: HashMap::new(),
            pending_memory_writes: Vec::new(),
            created_at: now,
            last_activity: now,
        }
    }

    /// Record a decision with optional rationale.
    pub fn log_decision(&mut self, content: &str, rationale: Option<&str>) {
        self.decisions_log.push(Decision {
            content: content.to_string(),
            timestamp: Utc::now(),
            rationale: rationale.map(|r| r.to_string()),
        });
        self.last_activity = Utc::now();
    }

    /// Get or create an entity entry and increment its mention count.
    ///
    /// If the entity does not exist yet, it is created with `mentions = 1`.
    /// If it already exists, `mentions` is incremented by one.
    /// Returns a mutable reference to the entity info.
    pub fn touch_entity(&mut self, name: &str, entity_type: &str) -> &mut EntityInfo {
        let now = Utc::now();
        let entry = self
            .entity_cache
            .entry(name.to_string())
            .and_modify(|info| {
                info.mentions += 1;
            })
            .or_insert_with(|| EntityInfo {
                name: name.to_string(),
                entity_type: entity_type.to_string(),
                first_seen: now,
                mentions: 1,
            });
        self.last_activity = Utc::now();
        entry
    }

    /// Replace the entire active context.
    pub fn set_context(&mut self, context: Vec<String>) {
        self.active_context = context;
        self.last_activity = Utc::now();
    }

    /// Append a single item to the active context.
    pub fn add_context(&mut self, item: String) {
        self.active_context.push(item);
        self.last_activity = Utc::now();
    }

    /// Set (or clear) the current task.
    pub fn set_task(&mut self, task: Option<String>) {
        self.current_task = task;
        self.last_activity = Utc::now();
    }

    /// Queue a memory write for later flush to a target layer.
    pub fn queue_write(
        &mut self,
        content: String,
        category: MemoryCategory,
        importance: f32,
        target: TargetLayer,
    ) {
        self.pending_memory_writes.push(PendingWrite {
            content,
            category,
            importance,
            target_layer: target,
        });
    }

    /// Drain all pending memory writes, returning them and clearing the queue.
    pub fn drain_pending(&mut self) -> Vec<PendingWrite> {
        std::mem::take(&mut self.pending_memory_writes)
    }

    /// Check whether this session has been idle longer than `threshold`.
    pub fn is_idle(&self, threshold: std::time::Duration) -> bool {
        let elapsed = Utc::now()
            .signed_duration_since(self.last_activity)
            .to_std()
            .unwrap_or(std::time::Duration::ZERO);
        elapsed >= threshold
    }

    /// Atomically persist session state to a WAL file.
    ///
    /// Writes to a `.tmp` sibling first, then renames into place for atomicity.
    pub fn flush_to_wal(&self, path: &Path) -> Result<(), String> {
        let json =
            serde_json::to_string_pretty(self).map_err(|e| format!("serialize error: {e}"))?;

        let tmp_path = path.with_extension("tmp");

        // Ensure parent directory exists.
        if let Some(parent) = tmp_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("failed to create WAL directory: {e}"))?;
        }

        std::fs::write(&tmp_path, json.as_bytes())
            .map_err(|e| format!("failed to write WAL tmp: {e}"))?;

        std::fs::rename(&tmp_path, path).map_err(|e| format!("failed to rename WAL: {e}"))?;

        Ok(())
    }

    /// Recover session state from a WAL file.
    ///
    /// Returns `Ok(None)` if the file does not exist.
    /// Returns `Err` if the file exists but cannot be parsed.
    pub fn recover_from_wal(path: &Path) -> Result<Option<Self>, String> {
        if !path.exists() {
            return Ok(None);
        }

        let data =
            std::fs::read_to_string(path).map_err(|e| format!("failed to read WAL: {e}"))?;

        let state: Self =
            serde_json::from_str(&data).map_err(|e| format!("failed to parse WAL: {e}"))?;

        Ok(Some(state))
    }

    /// Delete the WAL file if it exists.
    pub fn clear_wal(path: &Path) -> Result<(), String> {
        if path.exists() {
            std::fs::remove_file(path).map_err(|e| format!("failed to remove WAL: {e}"))?;
        }
        Ok(())
    }
}
