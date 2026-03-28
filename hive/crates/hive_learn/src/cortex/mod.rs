pub mod bridge;
pub mod event_bus;
pub mod guardrails;
pub mod meta_learner;
pub mod types;

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::time::Duration;

use chrono::Utc;
use rusqlite::params;
use tracing::{error, info, warn};

use crate::storage::LearningStorage;
use crate::types::LearningLogEntry;
use bridge::{BridgeAction, BridgeProcessor, CortexBridge};
use event_bus::{CortexEvent, CortexEventReceiver, CortexEventSender};
use guardrails::GuardrailsEngine;
use meta_learner::MetaLearner;
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
    meta_learner: MetaLearner,
    /// Tracks recent quality scores per persona for degradation detection.
    quality_buffer: HashMap<String, Vec<f64>>,
}

impl LearningCortex {
    pub fn new(storage: Arc<LearningStorage>, event_tx: CortexEventSender) -> Self {
        // Initialize cortex tables on construction
        if let Err(e) = Self::init_tables(&storage) {
            error!("Failed to initialize cortex tables: {e}");
        }

        let meta_learner = Self::load_meta_learner_from_storage(&storage).unwrap_or_else(|e| {
            warn!("Failed to load strategy weights from storage, using defaults: {e}");
            MetaLearner::new()
        });

        let cortex = Self {
            storage,
            event_tx,
            last_user_interaction: Arc::new(AtomicI64::new(Utc::now().timestamp())),
            auto_apply_enabled: Arc::new(std::sync::atomic::AtomicBool::new(true)),
            idle_threshold_secs: 30,
            meta_learner,
            quality_buffer: HashMap::new(),
        };

        if let Err(e) = cortex.save_meta_learner() {
            warn!("Failed to persist initial strategy weights: {e}");
        }

        cortex
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
        self.auto_apply_enabled.store(enabled, Ordering::Relaxed);
    }

    /// Returns the shared auto-apply flag so UI/config code can update it.
    pub fn auto_apply_handle(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.auto_apply_enabled)
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

    /// Load strategy weights from persistence or return defaults.
    fn load_strategies_from_storage(storage: &LearningStorage) -> Result<Vec<Strategy>, String> {
        let conn = storage.conn_lock()?;
        let mut stmt = conn
            .prepare(
                "SELECT strategy_id, domain, weight, attempts, successes, failures, avg_impact, last_adjusted
                 FROM cortex_strategies",
            )
            .map_err(|e| format!("Failed to prepare strategies query: {e}"))?;

        let rows = stmt
            .query_map([], |row| {
                let id_str: String = row.get(0)?;
                let domain_str: String = row.get(1)?;
                Ok(Strategy {
                    id: serde_json::from_str(&format!("\"{id_str}\""))
                        .unwrap_or(StrategyId::PromptMutation),
                    domain: serde_json::from_str(&format!("\"{domain_str}\""))
                        .unwrap_or(Domain::Routing),
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

    fn load_meta_learner_from_storage(storage: &LearningStorage) -> Result<MetaLearner, String> {
        let mut meta = MetaLearner::new();
        meta.load_from(Self::load_strategies_from_storage(storage)?);
        Ok(meta)
    }

    /// Persist the current meta-learner strategy set to SQLite.
    pub fn save_meta_learner(&self) -> Result<(), String> {
        for strategy in self.meta_learner.to_save() {
            self.save_strategy(strategy)?;
        }
        Ok(())
    }

    /// Reload strategy weights from SQLite, replacing the in-memory state.
    pub fn reload_meta_learner(&mut self) -> Result<(), String> {
        self.meta_learner = Self::load_meta_learner_from_storage(&self.storage)?;
        Ok(())
    }

    /// Access the current weight for a strategy.
    pub fn strategy_weight(&self, strategy: StrategyId) -> f64 {
        self.meta_learner.get_weight(strategy)
    }

    /// Apply a successful strategy outcome and persist it.
    pub fn record_strategy_success(
        &mut self,
        strategy: StrategyId,
        quality_delta: f64,
    ) -> Result<(), String> {
        self.meta_learner.record_success(strategy, quality_delta);
        self.save_meta_learner()
    }

    /// Apply a failed strategy outcome and persist it.
    pub fn record_strategy_failure(&mut self, strategy: StrategyId) -> Result<(), String> {
        self.meta_learner.record_failure(strategy);
        self.save_meta_learner()
    }

    /// Override a strategy weight and persist it.
    pub fn set_strategy_weight(&mut self, strategy: StrategyId, weight: f64) -> Result<(), String> {
        self.meta_learner.set_weight(strategy, weight);
        self.save_meta_learner()
    }

    fn strategy_for_domain(domain: Domain) -> StrategyId {
        match domain {
            Domain::Prompts => StrategyId::PromptMutation,
            Domain::Routing => StrategyId::TierAdjustment,
            Domain::Patterns => StrategyId::PatternInjection,
            Domain::SwarmConfig => StrategyId::CrossPollination,
        }
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
            );",
        )
        .map_err(|e| format!("Failed to initialize cortex tables: {e}"))
    }

    /// Persist a CortexEvent to the database.
    pub fn persist_event(&self, event: &CortexEvent) -> Result<(), String> {
        let event_type = event.event_type();
        let payload =
            serde_json::to_string(event).map_err(|e| format!("Failed to serialize event: {e}"))?;
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
        Self::load_strategies_from_storage(&self.storage)
    }

    /// Load a single change by id.
    pub fn load_change(&self, change_id: &str) -> Result<Option<CortexChange>, String> {
        let conn = self.storage.conn_lock()?;
        let mut stmt = conn
            .prepare(
                "SELECT change_id, domain, tier, action, prior_state, applied_at, soak_until, status, quality_before, quality_after
                 FROM cortex_changes WHERE change_id = ?1",
            )
            .map_err(|e| format!("Failed to prepare change lookup: {e}"))?;

        match stmt.query_row(params![change_id], |row| {
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
        }) {
            Ok(change) => Ok(Some(change)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(format!("Failed to load change: {e}")),
        }
    }

    /// Load the most recent changes, regardless of status.
    pub fn load_recent_changes(&self, limit: usize) -> Result<Vec<CortexChange>, String> {
        let conn = self.storage.conn_lock()?;
        let mut stmt = conn
            .prepare(
                "SELECT change_id, domain, tier, action, prior_state, applied_at, soak_until, status, quality_before, quality_after
                 FROM cortex_changes ORDER BY applied_at DESC LIMIT ?1",
            )
            .map_err(|e| format!("Failed to prepare recent changes query: {e}"))?;

        let rows = stmt
            .query_map(params![limit as i64], |row| {
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
            .map_err(|e| format!("Failed to query recent changes: {e}"))?;

        let mut changes = Vec::new();
        for row in rows {
            changes.push(row.map_err(|e| format!("Failed to read change row: {e}"))?);
        }
        Ok(changes)
    }

    /// Load changes filtered by status.
    pub fn load_changes_by_status(
        &self,
        status: ChangeStatus,
        limit: usize,
    ) -> Result<Vec<CortexChange>, String> {
        let conn = self.storage.conn_lock()?;
        let mut stmt = conn
            .prepare(
                "SELECT change_id, domain, tier, action, prior_state, applied_at, soak_until, status, quality_before, quality_after
                 FROM cortex_changes WHERE status = ?1 ORDER BY applied_at DESC LIMIT ?2",
            )
            .map_err(|e| format!("Failed to prepare status query: {e}"))?;

        let rows = stmt
            .query_map(params![status.as_str(), limit as i64], |row| {
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
            .map_err(|e| format!("Failed to query changes by status: {e}"))?;

        let mut changes = Vec::new();
        for row in rows {
            changes.push(row.map_err(|e| format!("Failed to read change row: {e}"))?);
        }
        Ok(changes)
    }

    /// Load the most recent event records.
    pub fn load_events(&self, limit: usize) -> Result<Vec<CortexEventRecord>, String> {
        let conn = self.storage.conn_lock()?;
        let mut stmt = conn
            .prepare(
                "SELECT id, event_type, payload, timestamp
                 FROM cortex_events ORDER BY timestamp DESC, id DESC LIMIT ?1",
            )
            .map_err(|e| format!("Failed to prepare event query: {e}"))?;

        let rows = stmt
            .query_map(params![limit as i64], |row| {
                Ok(CortexEventRecord {
                    id: row.get(0)?,
                    event_type: row.get(1)?,
                    payload: row.get(2)?,
                    timestamp: row.get(3)?,
                })
            })
            .map_err(|e| format!("Failed to query events: {e}"))?;

        let mut events = Vec::new();
        for row in rows {
            events.push(row.map_err(|e| format!("Failed to read event row: {e}"))?);
        }
        Ok(events)
    }

    /// Load recent events filtered by type.
    pub fn load_events_by_type(
        &self,
        event_type: &str,
        limit: usize,
    ) -> Result<Vec<CortexEventRecord>, String> {
        let conn = self.storage.conn_lock()?;
        let mut stmt = conn
            .prepare(
                "SELECT id, event_type, payload, timestamp
                 FROM cortex_events WHERE event_type = ?1 ORDER BY timestamp DESC, id DESC LIMIT ?2",
            )
            .map_err(|e| format!("Failed to prepare typed event query: {e}"))?;

        let rows = stmt
            .query_map(params![event_type, limit as i64], |row| {
                Ok(CortexEventRecord {
                    id: row.get(0)?,
                    event_type: row.get(1)?,
                    payload: row.get(2)?,
                    timestamp: row.get(3)?,
                })
            })
            .map_err(|e| format!("Failed to query typed events: {e}"))?;

        let mut events = Vec::new();
        for row in rows {
            events.push(row.map_err(|e| format!("Failed to read event row: {e}"))?);
        }
        Ok(events)
    }

    /// Load events inside a timestamp window.
    pub fn load_events_between(
        &self,
        since: i64,
        until: i64,
        limit: usize,
    ) -> Result<Vec<CortexEventRecord>, String> {
        let conn = self.storage.conn_lock()?;
        let mut stmt = conn
            .prepare(
                "SELECT id, event_type, payload, timestamp
                 FROM cortex_events
                 WHERE timestamp >= ?1 AND timestamp <= ?2
                 ORDER BY timestamp DESC, id DESC LIMIT ?3",
            )
            .map_err(|e| format!("Failed to prepare event range query: {e}"))?;

        let rows = stmt
            .query_map(params![since, until, limit as i64], |row| {
                Ok(CortexEventRecord {
                    id: row.get(0)?,
                    event_type: row.get(1)?,
                    payload: row.get(2)?,
                    timestamp: row.get(3)?,
                })
            })
            .map_err(|e| format!("Failed to query event range: {e}"))?;

        let mut events = Vec::new();
        for row in rows {
            events.push(row.map_err(|e| format!("Failed to read event row: {e}"))?);
        }
        Ok(events)
    }

    // ── Quality tracking (T020-T021) ────────────────────────────────────

    /// Record a quality score for a learning subject.
    ///
    /// Called after every AI interaction. The subject key is usually a persona
    /// when available; otherwise the runtime falls back to a stable model key.
    /// The buffer is kept in memory (not persisted) and is capped at 100
    /// entries per subject to bound memory usage.
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

    /// Update the latest soaking prompt change for a persona with fresh quality.
    pub fn record_prompt_soak_feedback(&self, persona: &str, quality: f64) -> Result<(), String> {
        #[derive(serde::Deserialize)]
        struct PromptAction {
            persona: String,
        }

        let latest = self
            .load_changes_by_status(ChangeStatus::Soaking, 50)?
            .into_iter()
            .find(|change| {
                change.domain == Domain::Prompts
                    && serde_json::from_str::<PromptAction>(&change.action)
                        .map(|action| action.persona == persona)
                        .unwrap_or(false)
            });

        let Some(change) = latest else {
            return Ok(());
        };

        let next_quality = match change.quality_after {
            Some(previous) => (previous + quality) / 2.0,
            None => quality,
        };

        self.update_change_status(&change.change_id, ChangeStatus::Soaking, Some(next_quality))
    }

    /// Apply the stored rollback action for a change when possible.
    pub fn rollback_change(&self, change: &CortexChange) -> Result<(), String> {
        #[derive(serde::Deserialize)]
        struct PromptPriorState {
            persona: String,
            to_version: u32,
        }

        match change.domain {
            Domain::Prompts => {
                let prior: PromptPriorState = serde_json::from_str(&change.prior_state)
                    .map_err(|e| format!("Failed to parse prompt rollback state: {e}"))?;
                let evolver = crate::prompt_evolver::PromptEvolver::new(Arc::clone(&self.storage));
                evolver.rollback(&prior.persona, prior.to_version)
            }
            _ => Ok(()),
        }
    }
}

/// Callback used by the Cortex runtime to kick off an AutoResearch run.
///
/// The runtime only knows the subject key that regressed. App-side wiring can
/// adapt that subject into the richer executor/suite context it has available.
pub trait AutoresearchTrigger: Send + Sync {
    fn trigger(&self, subject: &str) -> Result<(), String>;
}

/// Background runtime that subscribes to the cortex event bus and processes
/// events sequentially on a Tokio task.
pub struct LearningCortexRuntime {
    cortex: LearningCortex,
    receiver: CortexEventReceiver,
    bridge_processor: Option<BridgeProcessor>,
    autoresearch_trigger: Option<Arc<dyn AutoresearchTrigger>>,
    pending_autoresearch: VecDeque<String>,
}

impl LearningCortexRuntime {
    pub fn new(cortex: LearningCortex, receiver: CortexEventReceiver) -> Self {
        Self {
            cortex,
            receiver,
            bridge_processor: None,
            autoresearch_trigger: None,
            pending_autoresearch: VecDeque::new(),
        }
    }

    pub fn with_bridge(mut self, bridge: Arc<dyn CortexBridge>) -> Self {
        self.bridge_processor = Some(BridgeProcessor::new(bridge));
        self
    }

    pub fn with_autoresearch_trigger(mut self, trigger: Arc<dyn AutoresearchTrigger>) -> Self {
        self.autoresearch_trigger = Some(trigger);
        self
    }

    pub fn cortex(&self) -> &LearningCortex {
        &self.cortex
    }

    pub fn into_cortex(self) -> LearningCortex {
        self.cortex
    }

    pub fn spawn(mut self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            self.run().await;
        })
    }

    async fn run(&mut self) {
        let mut soak_timer = tokio::time::interval(Duration::from_secs(60));
        soak_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut prune_timer = tokio::time::interval(Duration::from_secs(24 * 60 * 60));
        prune_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        // Run one soak pass immediately so restart recovery does not wait for
        // the first timer tick.
        if let Err(e) = self.cortex.prune_old_events() {
            warn!(error = %e, "Failed to prune stale cortex events on startup");
        }
        self.process_soaking_changes();
        self.flush_autoresearch_queue();

        loop {
            tokio::select! {
                received = self.receiver.recv() => {
                    match received {
                        Ok(event) => self.handle_event(event).await,
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            warn!(dropped = n, "Cortex event bus lagged");
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            info!("Cortex event bus closed");
                            break;
                        }
                    }
                }
                _ = soak_timer.tick() => {
                    self.process_soaking_changes();
                    self.flush_autoresearch_queue();
                }
                _ = prune_timer.tick() => {
                    if let Err(e) = self.cortex.prune_old_events() {
                        warn!(error = %e, "Failed to prune stale cortex events");
                    }
                }
            }
        }
    }

    async fn handle_event(&mut self, event: CortexEvent) {
        if let Err(e) = self.cortex.persist_event(&event) {
            warn!(error = %e, "Failed to persist cortex event");
        }

        match event {
            CortexEvent::OutcomeRecorded {
                model,
                persona,
                quality_score,
                ..
            } => {
                let subject = persona
                    .as_deref()
                    .map(Self::subject_key_for_persona)
                    .unwrap_or_else(|| self.subject_key_for_model(&model));
                self.cortex.record_quality(&subject, quality_score);
                if let Some(persona) = persona.as_deref()
                    && let Err(e) = self.cortex.record_prompt_soak_feedback(persona, quality_score)
                {
                    warn!(error = %e, persona, "Failed to record prompt soak feedback");
                }
                if let Some(degraded) = self.cortex.should_trigger_autoresearch() {
                    self.queue_autoresearch(degraded.clone());
                    info!(subject = %degraded, "Queued autoresearch after quality regression");
                }
            }
            CortexEvent::RoutingDecision {
                tier,
                quality_result,
                ..
            } => {
                if let Some(quality) = quality_result {
                    let strategy = StrategyId::TierAdjustment;
                    let delta = if quality >= 0.6 { quality } else { -quality };
                    let outcome = if quality >= 0.6 {
                        self.cortex.record_strategy_success(strategy, delta)
                    } else {
                        self.cortex.record_strategy_failure(strategy)
                    };
                    if let Err(e) = outcome {
                        warn!(error = %e, tier, "Failed to update routing strategy weight");
                    }
                }
            }
            CortexEvent::PromptVersionCreated {
                persona,
                version,
                avg_quality,
            } => {
                if let Some(processor) = self.bridge_processor.as_mut() {
                    let action = processor.process_individual_to_collective(
                        &CortexEvent::PromptVersionCreated {
                            persona: persona.clone(),
                            version,
                            avg_quality,
                        },
                    );
                    self.handle_bridge_action(action).await;
                }
            }
            CortexEvent::PatternExtracted {
                pattern_id,
                language,
                category,
                quality,
            } => {
                if let Some(processor) = self.bridge_processor.as_mut() {
                    let action = processor.process_individual_to_collective(
                        &CortexEvent::PatternExtracted {
                            pattern_id: pattern_id.clone(),
                            language: language.clone(),
                            category: category.clone(),
                            quality,
                        },
                    );
                    self.handle_bridge_action(action).await;
                }
            }
            CortexEvent::SelfEvalCompleted {
                overall_quality, ..
            } => {
                if overall_quality < QUALITY_DEGRADATION_THRESHOLD {
                    self.queue_autoresearch("self_eval".to_string());
                }
            }
            CortexEvent::SwarmCompleted { success, .. } => {
                if success {
                    self.cortex
                        .storage
                        .log_learning(&LearningLogEntry {
                            id: 0,
                            event_type: "cortex_swarm_completed".into(),
                            description: "Swarm run completed successfully".into(),
                            details: String::new(),
                            reversible: false,
                            timestamp: Utc::now().to_rfc3339(),
                        })
                        .ok();
                }
            }
            CortexEvent::CollectiveMemoryEntry {
                category,
                content,
                relevance_score,
            } => {
                if let Some(processor) = self.bridge_processor.as_mut() {
                    let entry = BridgedMemoryEntry {
                        category: category.as_str().to_string(),
                        content,
                        relevance_score,
                        timestamp_epoch: Utc::now().timestamp(),
                    };
                    let action = processor.process_collective_to_individual(&entry);
                    self.handle_bridge_action(action).await;
                }
            }
            CortexEvent::QueenPlanGenerated { .. } => {}
            CortexEvent::SkillEvalCompleted {
                skill_id,
                pass_rate,
                iteration,
            } => {
                if pass_rate < QUALITY_DEGRADATION_THRESHOLD {
                    self.queue_autoresearch(format!("skill:{skill_id}"));
                }
                self.cortex
                    .storage
                    .log_learning(&LearningLogEntry {
                        id: 0,
                        event_type: "cortex_skill_eval_completed".into(),
                        description: format!("Skill eval completed for {skill_id} (iteration {iteration})"),
                        details: format!("{{\"skill_id\":\"{skill_id}\",\"pass_rate\":{pass_rate},\"iteration\":{iteration}}}"),
                        reversible: false,
                        timestamp: Utc::now().to_rfc3339(),
                    })
                    .ok();
            }
            CortexEvent::PromptMutated {
                skill_id,
                old_pass_rate,
                new_pass_rate,
                promoted,
            } => {
                let strategy = StrategyId::PromptMutation;
                let result = if promoted && new_pass_rate >= old_pass_rate {
                    self.cortex
                        .record_strategy_success(strategy, new_pass_rate - old_pass_rate)
                } else {
                    self.cortex.record_strategy_failure(strategy)
                };
                if let Err(e) = result {
                    warn!(error = %e, skill_id = %skill_id, "Failed to update prompt mutation strategy");
                }
            }
            CortexEvent::ImprovementApplied {
                domain,
                action,
                expected_impact,
            } => {
                let strategy = Self::strategy_for_domain(domain);
                if let Err(e) = self
                    .cortex
                    .record_strategy_success(strategy, expected_impact)
                {
                    warn!(error = %e, %action, "Failed to record improvement success");
                }
            }
            CortexEvent::ImprovementRolledBack {
                domain,
                action,
                reason,
            } => {
                let strategy = Self::strategy_for_domain(domain);
                if let Err(e) = self.cortex.record_strategy_failure(strategy) {
                    warn!(error = %e, %action, "Failed to record improvement rollback");
                }
                self.cortex
                    .storage
                    .log_learning(&LearningLogEntry {
                        id: 0,
                        event_type: "cortex_improvement_rolled_back".into(),
                        description: format!("Rolled back {action}"),
                        details: reason,
                        reversible: false,
                        timestamp: Utc::now().to_rfc3339(),
                    })
                    .ok();
            }
            CortexEvent::StrategyWeightAdjusted {
                strategy,
                old_weight: _,
                new_weight,
            } => {
                if let Err(e) = self.cortex.set_strategy_weight(strategy, new_weight) {
                    warn!(error = %e, "Failed to persist strategy weight adjustment");
                }
            }
        }

        self.flush_autoresearch_queue();
    }

    async fn handle_bridge_action(&mut self, action: BridgeAction) {
        match action {
            BridgeAction::SuggestPromptRefinement { persona, evidence } => {
                let _ = self.cortex.storage.log_learning(&LearningLogEntry {
                    id: 0,
                    event_type: "cortex_bridge_prompt_refinement".into(),
                    description: format!("Bridge suggested prompt refinement for '{persona}'"),
                    details: evidence,
                    reversible: false,
                    timestamp: Utc::now().to_rfc3339(),
                });
            }
            BridgeAction::SuggestRoutingAdjustment { task_type, insight } => {
                let _ = self.cortex.storage.log_learning(&LearningLogEntry {
                    id: 0,
                    event_type: "cortex_bridge_routing_adjustment".into(),
                    description: format!("Bridge suggested routing adjustment for '{task_type}'"),
                    details: insight,
                    reversible: false,
                    timestamp: Utc::now().to_rfc3339(),
                });
            }
            BridgeAction::WriteToCollective { category, content } => {
                if let Some(processor) = self.bridge_processor.as_mut() {
                    if let Err(e) = processor.execute_write(&category, &content) {
                        warn!(error = %e, "Failed to write bridged insight to collective memory");
                    }
                }
            }
            BridgeAction::Noop => {}
        }
    }

    fn queue_autoresearch(&mut self, subject: String) {
        if self
            .pending_autoresearch
            .iter()
            .any(|existing| existing == &subject)
        {
            return;
        }
        if self.pending_autoresearch.len() >= 10 {
            self.pending_autoresearch.pop_front();
        }
        self.pending_autoresearch.push_back(subject);
    }

    fn flush_autoresearch_queue(&mut self) {
        let Some(trigger) = self.autoresearch_trigger.as_ref().map(Arc::clone) else {
            return;
        };
        let guardrails = GuardrailsEngine::new();

        match guardrails.can_auto_apply(Tier::Yellow, &self.cortex) {
            Ok(true) => {}
            Ok(false) => return,
            Err(e) => {
                warn!(error = %e, "Failed to evaluate autoresearch guardrails");
                return;
            }
        }

        while let Some(subject) = self.pending_autoresearch.pop_front() {
            if let Err(e) = trigger.trigger(&subject) {
                warn!(error = %e, subject = %subject, "AutoResearch trigger failed");
                self.pending_autoresearch.push_front(subject);
                break;
            }
        }
    }

    fn subject_key_for_model(&self, model: &str) -> String {
        if model.trim().is_empty() {
            "model:unknown".to_string()
        } else {
            format!("model:{model}")
        }
    }

    fn subject_key_for_persona(persona: &str) -> String {
        if persona.trim().is_empty() {
            "persona:unknown".to_string()
        } else {
            format!("persona:{persona}")
        }
    }

    fn strategy_for_domain(domain: Domain) -> StrategyId {
        LearningCortex::strategy_for_domain(domain)
    }

    fn process_soaking_changes(&mut self) {
        let guardrails = GuardrailsEngine::new();
        let updates = match guardrails.check_soaking_changes(&self.cortex) {
            updates => updates,
        };

        for (change_id, status) in updates {
            let Some(change) = self.cortex.load_change(&change_id).ok().flatten() else {
                continue;
            };

            let current_quality = change
                .quality_after
                .unwrap_or_else(|| change.quality_before.unwrap_or(0.0));

            if let Err(e) =
                self.cortex
                    .update_change_status(&change_id, status, Some(current_quality))
            {
                warn!(error = %e, change_id = %change_id, "Failed to update soaked change");
                continue;
            }

            let impact = current_quality - change.quality_before.unwrap_or(current_quality);
            match status {
                ChangeStatus::Confirmed => {
                    let _ = self.cortex.event_tx.send(CortexEvent::ImprovementApplied {
                        domain: change.domain,
                        action: change.action.clone(),
                        expected_impact: impact,
                    });
                    let _ = self.cortex.storage.log_learning(&LearningLogEntry {
                        id: 0,
                        event_type: "cortex_change_confirmed".into(),
                        description: format!("Confirmed change {change_id}"),
                        details: change.prior_state.clone(),
                        reversible: false,
                        timestamp: Utc::now().to_rfc3339(),
                    });
                }
                ChangeStatus::RolledBack => {
                    if let Err(e) = self.cortex.rollback_change(&change) {
                        warn!(error = %e, change_id = %change_id, "Failed to restore rolled back change");
                    }
                    let _ = self
                        .cortex
                        .event_tx
                        .send(CortexEvent::ImprovementRolledBack {
                            domain: change.domain,
                            action: change.action.clone(),
                            reason: format!(
                                "Quality regressed from {:.3} to {:.3}",
                                change.quality_before.unwrap_or(current_quality),
                                current_quality
                            ),
                        });
                    let _ = self.cortex.storage.log_learning(&LearningLogEntry {
                        id: 0,
                        event_type: "cortex_change_rolled_back".into(),
                        description: format!("Rolled back change {change_id}"),
                        details: change.prior_state.clone(),
                        reversible: false,
                        timestamp: Utc::now().to_rfc3339(),
                    });
                }
                ChangeStatus::Soaking => {}
            }
        }
    }
}
