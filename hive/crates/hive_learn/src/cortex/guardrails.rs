use chrono::Utc;

use super::LearningCortex;
use super::meta_learner::MetaLearner;
use super::types::{ChangeStatus, CortexChange, StrategyId, Tier};

/// Engine for tiered auto-apply safety.
///
/// Three tiers by blast radius:
/// - Green: routing adjustments (immediate, monitor 10 interactions)
/// - Yellow: prompt promotions (1-hour soak)
/// - Red: pattern injection, strategy weights (24-hour soak)
///
/// Enforces:
/// - Auto-apply must be globally enabled
/// - User must be idle (>30s)
/// - Max 3 auto-applied changes per 24 hours
/// - Quality regression triggers rollback
pub struct GuardrailsEngine;

/// Maximum number of auto-applied changes allowed in a 24-hour window.
const MAX_CHANGES_PER_24H: u32 = 3;

impl GuardrailsEngine {
    pub fn new() -> Self {
        Self
    }

    /// Check whether an auto-apply is allowed right now.
    ///
    /// Returns `Ok(true)` only when ALL of these hold:
    /// 1. Auto-apply is globally enabled
    /// 2. The user has been idle for at least `idle_threshold_secs`
    /// 3. Fewer than 3 changes have been applied in the last 24 hours
    pub fn can_auto_apply(&self, _tier: Tier, cortex: &LearningCortex) -> Result<bool, String> {
        // 1. Global toggle
        if !cortex.is_auto_apply_enabled() {
            return Ok(false);
        }

        // 2. Idle check
        if !cortex.is_idle() {
            return Ok(false);
        }

        // 3. Rate limit
        let recent = cortex.changes_last_24h()?;
        if recent >= MAX_CHANGES_PER_24H {
            return Ok(false);
        }

        Ok(true)
    }

    /// Determine whether a change should be rolled back based on quality regression.
    ///
    /// Compares `quality_before` against `current_quality`. If the drop exceeds
    /// the tier's rollback threshold, returns `true`.
    pub fn should_rollback(&self, change: &CortexChange, current_quality: f64) -> bool {
        let baseline = match change.quality_before {
            Some(q) => q,
            // No baseline recorded — cannot detect regression
            None => return false,
        };

        // If baseline is essentially zero, avoid false positives
        if baseline <= f64::EPSILON {
            return false;
        }

        let drop = baseline - current_quality;
        drop > change.tier.rollback_threshold()
    }

    /// Check whether a change's soak period has expired.
    pub fn soak_expired(&self, change: &CortexChange) -> bool {
        let now = Utc::now().timestamp();
        now >= change.soak_until
    }

    /// Review all soaking changes and decide their fate.
    ///
    /// Returns a list of `(change_id, new_status)` for changes that need updates:
    /// - Soak expired AND quality held (or improved) -> `Confirmed`
    /// - Quality regressed beyond threshold -> `RolledBack`
    /// - Still soaking -> skipped (not included in output)
    pub fn check_soaking_changes(
        &self,
        cortex: &LearningCortex,
    ) -> Vec<(String, ChangeStatus)> {
        let soaking = match cortex.load_soaking_changes() {
            Ok(changes) => changes,
            Err(_) => return Vec::new(),
        };

        let mut updates = Vec::new();

        for change in &soaking {
            let current_quality = change.quality_after.unwrap_or_else(|| {
                change.quality_before.unwrap_or(0.0)
            });

            if self.should_rollback(change, current_quality) {
                updates.push((change.change_id.clone(), ChangeStatus::RolledBack));
            } else if self.soak_expired(change) {
                updates.push((change.change_id.clone(), ChangeStatus::Confirmed));
            }
            // else: still soaking, skip
        }

        updates
    }

    /// Return a strategy-adjusted threshold for guardrail decisions.
    ///
    /// Delegates to `MetaLearner::should_trigger()` which adjusts the base
    /// threshold based on the strategy's accumulated trust weight.
    pub fn adjusted_threshold(
        &self,
        meta: &MetaLearner,
        strategy_id: StrategyId,
        base: f64,
    ) -> f64 {
        meta.should_trigger(strategy_id, base)
    }
}
