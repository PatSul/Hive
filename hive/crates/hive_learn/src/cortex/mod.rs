pub mod bridge;
pub mod event_bus;
pub mod guardrails;
pub mod meta_learner;
pub mod types;

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use chrono::Utc;
use rusqlite::params;
use tracing::{error, info, warn};

use crate::storage::LearningStorage;
use event_bus::{CortexEvent, CortexEventSender};
use types::*;

/// Minimum number of quality samples before we consider degradation meaningful.
const MIN_QUALITY_SAMPLES: usize = 20;

/// Quality threshold below which a persona is considered degraded.
const QUALITY_DEGRADATION_THRESHOLD: f64 = 0.6;

/// The central nervous system for Hive's self-improvement.
///
/// Subscribes to events from hive_learn, hive_agents, and hive_remote.
/// Correlates them, decides on improvements, and auto-applies with guardrails.
pub struct LearningCortex {
    storage: Arc<LearningStorage>,
    event_tx: CortexEventSender,
    last_user_interaction: Arc<AtomicI64>,
    auto_apply_enabled: Arc<std::sync::atomic::AtomicBool>,
    idle_threshold_secs: i64,
    /// Tracks recent quality scores per persona for degradation detection.
    quality_buffer: HashMap<String, Vec<f64>>,
}

impl LearningCortex {
    pub fn new(storage: Arc<LearningStorage>, event_tx: CortexEventSender) -> Self {
        // Initialize cortex tables on construction
        if let Err(e) = Self::init_tables(&storage) {
            error!("Failed to initialize cortex tables: {e}");
        }

        Self {
            storage,
            event_tx,
            last_user_interaction: Arc::new(AtomicI64::new(Utc::now().timestamp())),
            auto_apply_enabled: Arc::new(std::sync::atomic::AtomicBool::new(true)),
            idle_threshold_secs: 30,
            quality_buffer: HashMap::new(),
        }
    }

    /// Called by ChatService and HiveDaemon on every user interaction.
    pub fn mark_active(&self) {
        self.last_user_interaction
            .store(Utc::now().timestamp(), Ordering::Relaxed);
    }

    /// Returns true if the user has been idle for at least `idle_threshold_secs`.
    pub fn is_idle(&self) -> bool {
        let last = self.last_user_interaction.load(Ordering::Relaxed);
        let now = Utc::now().timestamp();
        (now - last) >= self.idle_threshold_secs
    }

    /// Pause or resume auto-apply globally.
    pub fn set_auto_apply_enabled(&self, enabled: bool) {
        self.auto_apply_enabled
            .store(enabled, Ordering::Relaxed);
    }

    /// Check if auto-apply is enabled.
    pub fn is_auto_apply_enabled(&self) -> bool {
        self.auto_apply_enabled.load(Ordering::Relaxed)
    }

    /// Returns a clone of the last_user_interaction for sharing with other crates.
    pub fn interaction_tracker(&self) -> Arc<AtomicI64> {
        Arc::clone(&self.last_user_interaction)
    }

    /// Returns a clone of the event sender for sharing with producers.
    pub fn event_sender(&self) -> CortexEventSender {
        self.event_tx.clone()
    }

    /// Initialize cortex database tables.
    fn init_tables(storage: &LearningStorage) -> Result<(), String> {
        let conn = storage.conn_lock()?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS cortex_events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                event_type TEXT NOT NULL,
                payload TEXT NOT NULL,
                timestamp INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_cortex_events_timestamp ON cortex_events(timestamp);
            CREATE INDEX IF NOT EXISTS idx_cortex_events_type ON cortex_events(event_type);

            CREATE TABLE IF NOT EXISTS cortex_changes (
                change_id TEXT PRIMARY KEY,
                domain TEXT NOT NULL,
                tier TEXT NOT NULL,
                action TEXT NOT NULL,
                prior_state TEXT NOT NULL,
                applied_at INTEGER NOT NULL,
                soak_until INTEGER NOT NULL,
                status TEXT NOT NULL,
                quality_before REAL,
                quality_after REAL
            );
            CREATE INDEX IF NOT EXISTS idx_cortex_changes_status ON cortex_changes(status);

            CREATE TABLE IF NOT EXISTS cortex_strategies (
                strategy_id TEXT PRIMARY KEY,
                domain TEXT NOT NULL,
                weight REAL NOT NULL DEFAULT 0.5,
                attempts INTEGER NOT NULL DEFAULT 0,
                successes INTEGER NOT NULL DEFAULT 0,
                failures INTEGER NOT NULL DEFAULT 0,
                avg_impact REAL NOT NULL DEFAULT 0.0,
                last_adjusted INTEGER NOT NULL
            );"
        )
        .map_err(|e| format!("Failed to initialize cortex tables: {e}"))
    }

    /// Persist a CortexEvent to the database.
    pub fn persist_event(&self, event: &CortexEvent) -> Result<(), String> {
        let event_type = event.event_type();
        let payload = serde_json::to_string(event)
            .map_err(|e| format!("Failed to serialize event: {e}"))?;
        let timestamp = Utc::now().timestamp();

        let conn = self.storage.conn_lock()?;
        conn.execute(
            "INSERT INTO cortex_events (event_type, payload, timestamp) VALUES (?1, ?2, ?3)",
            params![event_type, payload, timestamp],
        )
        .map_err(|e| format!("Failed to persist event: {e}"))?;
        Ok(())
    }

    /// Insert a new change record.
    pub fn insert_change(&self, change: &CortexChange) -> Result<(), String> {
        let conn = self.storage.conn_lock()?;
        conn.execute(
            "INSERT INTO cortex_changes (change_id, domain, tier, action, prior_state, applied_at, soak_until, status, quality_before, quality_after)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                change.change_id,
                change.domain.as_str(),
                change.tier.as_str(),
                change.action,
                change.prior_state,
                change.applied_at,
                change.soak_until,
                change.status.as_str(),
                change.quality_before,
                change.quality_after,
            ],
        )
        .map_err(|e| format!("Failed to insert change: {e}"))?;
        Ok(())
    }

    /// Update the status of a change.
    pub fn update_change_status(
        &self,
        change_id: &str,
        status: ChangeStatus,
        quality_after: Option<f64>,
    ) -> Result<(), String> {
        let conn = self.storage.conn_lock()?;
        conn.execute(
            "UPDATE cortex_changes SET status = ?1, quality_after = ?2 WHERE change_id = ?3",
            params![status.as_str(), quality_after, change_id],
        )
        .map_err(|e| format!("Failed to update change status: {e}"))?;
        Ok(())
    }

    /// Load all changes currently in Soaking status.
    pub fn load_soaking_changes(&self) -> Result<Vec<CortexChange>, String> {
        let conn = self.storage.conn_lock()?;
        let mut stmt = conn
            .prepare(
                "SELECT change_id, domain, tier, action, prior_state, applied_at, soak_until, status, quality_before, quality_after
                 FROM cortex_changes WHERE status = 'soaking'"
            )
            .map_err(|e| format!("Failed to prepare soaking query: {e}"))?;

        let rows = stmt
            .query_map([], |row| {
                Ok(CortexChange {
                    change_id: row.get(0)?,
                    domain: serde_json::from_str(&format!("\"{}\"", row.get::<_, String>(1)?))
                        .unwrap_or(Domain::Routing),
                    tier: serde_json::from_str(&format!("\"{}\"", row.get::<_, String>(2)?))
                        .unwrap_or(Tier::Green),
                    action: row.get(3)?,
                    prior_state: row.get(4)?,
                    applied_at: row.get(5)?,
                    soak_until: row.get(6)?,
                    status: ChangeStatus::from_str(&row.get::<_, String>(7)?),
                    quality_before: row.get(8)?,
                    quality_after: row.get(9)?,
                })
            })
            .map_err(|e| format!("Failed to query soaking changes: {e}"))?;

        let mut changes = Vec::new();
        for row in rows {
            changes.push(row.map_err(|e| format!("Failed to read change row: {e}"))?);
        }
        Ok(changes)
    }

    /// Prune events older than 30 days. Returns count of deleted rows.
    pub fn prune_old_events(&self) -> Result<usize, String> {
        let cutoff = Utc::now().timestamp() - (30 * 24 * 3600);
        let conn = self.storage.conn_lock()?;
        conn.execute(
            "DELETE FROM cortex_events WHERE timestamp < ?1",
            params![cutoff],
        )
        .map_err(|e| format!("Failed to prune old events: {e}"))
    }

    /// Count changes applied in the last 24 hours.
    pub fn changes_last_24h(&self) -> Result<u32, String> {
        let cutoff = Utc::now().timestamp() - 86400;
        let conn = self.storage.conn_lock()?;
        let count: u32 = conn
            .query_row(
                "SELECT COUNT(*) FROM cortex_changes WHERE applied_at > ?1 AND status != 'rolled_back'",
                params![cutoff],
                |row| row.get(0),
            )
            .map_err(|e| format!("Failed to count recent changes: {e}"))?;
        Ok(count)
    }

    /// Save a strategy to the database.
    pub fn save_strategy(&self, strategy: &Strategy) -> Result<(), String> {
        let conn = self.storage.conn_lock()?;
        conn.execute(
            "INSERT OR REPLACE INTO cortex_strategies (strategy_id, domain, weight, attempts, successes, failures, avg_impact, last_adjusted)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                strategy.id.as_str(),
                strategy.domain.as_str(),
                strategy.weight,
                strategy.attempts,
                strategy.successes,
                strategy.failures,
                strategy.avg_impact,
                strategy.last_adjusted,
            ],
        )
        .map_err(|e| format!("Failed to save strategy: {e}"))?;
        Ok(())
    }

    /// Load all strategies from the database.
    pub fn load_strategies(&self) -> Result<Vec<Strategy>, String> {
        let conn = self.storage.conn_lock()?;
        let mut stmt = conn
            .prepare(
                "SELECT strategy_id, domain, weight, attempts, successes, failures, avg_impact, last_adjusted
                 FROM cortex_strategies"
            )
            .map_err(|e| format!("Failed to prepare strategies query: {e}"))?;

        let rows = stmt
            .query_map([], |row| {
                let id_str: String = row.get(0)?;
                let domain_str: String = row.get(1)?;
                Ok(Strategy {
                    id: serde_json::from_str(&format!("\"{id_str}\"")).unwrap_or(StrategyId::PromptMutation),
                    domain: serde_json::from_str(&format!("\"{domain_str}\"")).unwrap_or(Domain::Routing),
                    weight: row.get(2)?,
                    attempts: row.get(3)?,
                    successes: row.get(4)?,
                    failures: row.get(5)?,
                    avg_impact: row.get(6)?,
                    last_adjusted: row.get(7)?,
                })
            })
            .map_err(|e| format!("Failed to query strategies: {e}"))?;

        let mut strategies = Vec::new();
        for row in rows {
            strategies.push(row.map_err(|e| format!("Failed to read strategy row: {e}"))?);
        }
        Ok(strategies)
    }

    // ── Quality tracking (T020-T021) ────────────────────────────────────

    /// Record a quality score for a persona.
    ///
    /// Called after every AI interaction that has a persona tag.
    /// The buffer is kept in memory (not persisted) and is capped at 100
    /// entries per persona to bound memory usage.
    pub fn record_quality(&mut self, persona: &str, quality: f64) {
        let buf = self.quality_buffer.entry(persona.to_string()).or_default();
        buf.push(quality);
        // Cap at 100 to avoid unbounded growth
        if buf.len() > 100 {
            buf.drain(..buf.len() - 100);
        }
    }

    /// Check whether a specific persona is showing quality degradation.
    ///
    /// Returns `true` when:
    /// - At least `MIN_QUALITY_SAMPLES` (20) scores have been recorded
    /// - The average quality is below `QUALITY_DEGRADATION_THRESHOLD` (0.6)
    pub fn check_quality_degradation(&self, persona: &str) -> bool {
        let buf = match self.quality_buffer.get(persona) {
            Some(b) => b,
            None => return false,
        };

        if buf.len() < MIN_QUALITY_SAMPLES {
            return false;
        }

        let sum: f64 = buf.iter().sum();
        let avg = sum / buf.len() as f64;
        avg < QUALITY_DEGRADATION_THRESHOLD
    }

    /// Scan all tracked personas for quality degradation.
    ///
    /// Returns `Some(persona)` for the first persona that needs improvement,
    /// or `None` if all personas are healthy.
    ///
    /// Note: this only detects the need for AutoResearch -- actual spawning
    /// requires executor wiring in main.rs.
    pub fn should_trigger_autoresearch(&self) -> Option<String> {
        for (persona, _buf) in &self.quality_buffer {
            if self.check_quality_degradation(persona) {
                return Some(persona.clone());
            }
        }
        None
    }

    // ── Startup recovery (T024) ─────────────────────────────────────────

    /// Recover state after an app restart.
    ///
    /// Loads all changes that were still in `Soaking` status and checks
    /// whether their soak period has expired. Expired changes are marked
    /// as pending review (set back to `Soaking` with a note -- the next
    /// tick of the soak monitor will promote or rollback as appropriate).
    ///
    /// This ensures no change is silently forgotten if the app was closed
    /// during a soak period.
    pub fn startup_recovery(&self) -> Result<(), String> {
        let soaking = self.load_soaking_changes()?;
        let now = Utc::now().timestamp();
        let mut expired_count = 0u32;

        for change in &soaking {
            if now >= change.soak_until {
                expired_count += 1;
                // Log for observability -- the soak monitor will pick these up
                // on its next tick and confirm or rollback.
                info!(
                    "Startup recovery: change {} soak expired while offline (soak_until={}, now={})",
                    change.change_id, change.soak_until, now
                );
            }
        }

        if expired_count > 0 {
            warn!(
                "Startup recovery: {expired_count} soaking change(s) expired while app was offline"
            );
        } else if !soaking.is_empty() {
            info!(
                "Startup recovery: {} change(s) still soaking normally",
                soaking.len()
            );
        }

        Ok(())
    }
}
