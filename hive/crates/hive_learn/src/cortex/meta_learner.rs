use super::types::{Strategy, StrategyId};
use std::collections::HashMap;

const WEIGHT_SUCCESS_MULTIPLIER: f64 = 1.1;
const WEIGHT_FAILURE_MULTIPLIER: f64 = 0.7;
const WEIGHT_MAX: f64 = 1.0;
const WEIGHT_MIN: f64 = 0.1;

/// Stagnation threshold: if a strategy has more than this many attempts
/// but its success rate is below `STAGNATION_SUCCESS_RATE`, flag it.
const STAGNATION_ATTEMPT_THRESHOLD: u32 = 10;
const STAGNATION_SUCCESS_RATE: f64 = 0.2;

/// Tracks which improvement strategies work and adjusts their parameters.
///
/// Does not improve tasks -- improves how the system improves.
/// Adjusts parameters (thresholds, frequencies), not code.
pub struct MetaLearner {
    strategies: HashMap<StrategyId, Strategy>,
}

impl MetaLearner {
    /// Initialize with default strategies for all four `StrategyId` variants.
    pub fn new() -> Self {
        let mut strategies = HashMap::new();
        for id in [
            StrategyId::PromptMutation,
            StrategyId::TierAdjustment,
            StrategyId::PatternInjection,
            StrategyId::CrossPollination,
        ] {
            strategies.insert(id, Strategy::new(id));
        }
        Self { strategies }
    }

    /// Record a successful outcome for a strategy.
    ///
    /// Increments the success counter, multiplies the weight by 1.1
    /// (capped at 1.0), and updates `avg_impact` as a running average.
    pub fn record_success(&mut self, id: StrategyId, quality_delta: f64) {
        if let Some(strategy) = self.strategies.get_mut(&id) {
            strategy.successes += 1;
            strategy.weight = (strategy.weight * WEIGHT_SUCCESS_MULTIPLIER).min(WEIGHT_MAX);
            strategy.last_adjusted = chrono::Utc::now().timestamp();

            // Running average: new_avg = old_avg + (new_value - old_avg) / n
            let n = strategy.successes as f64;
            strategy.avg_impact += (quality_delta - strategy.avg_impact) / n;
        }
    }

    /// Record a failed outcome for a strategy.
    ///
    /// Increments the failure counter and multiplies the weight by 0.7
    /// (floored at 0.1).
    pub fn record_failure(&mut self, id: StrategyId) {
        if let Some(strategy) = self.strategies.get_mut(&id) {
            strategy.failures += 1;
            strategy.weight = (strategy.weight * WEIGHT_FAILURE_MULTIPLIER).max(WEIGHT_MIN);
            strategy.last_adjusted = chrono::Utc::now().timestamp();
        }
    }

    /// Return the current weight for a strategy.
    pub fn get_weight(&self, id: StrategyId) -> f64 {
        self.strategies
            .get(&id)
            .map(|s| s.weight)
            .unwrap_or(0.5)
    }

    /// Return a reference to a strategy by id.
    pub fn get_strategy(&self, id: StrategyId) -> Option<&Strategy> {
        self.strategies.get(&id)
    }

    /// Return references to all strategies.
    pub fn all_strategies(&self) -> Vec<&Strategy> {
        self.strategies.values().collect()
    }

    /// Return an adjusted threshold for triggering a strategy.
    ///
    /// Formula: `base_threshold * (1.0 + (0.5 - weight))`
    ///
    /// - Low weight (untrusted strategy) raises the threshold, making it harder to trigger.
    /// - High weight (trusted strategy) lowers the threshold, making it easier to trigger.
    pub fn should_trigger(&self, id: StrategyId, base_threshold: f64) -> f64 {
        let weight = self.get_weight(id);
        base_threshold * (1.0 + (0.5 - weight))
    }

    /// Review all strategies for stagnation.
    ///
    /// A strategy is considered stagnant when it has more than 10 total
    /// attempts but a success rate below 20%.
    ///
    /// Returns a vec of `(StrategyId, "stagnant")` for flagged strategies.
    pub fn review_all(&self) -> Vec<(StrategyId, &str)> {
        let mut flagged = Vec::new();

        for (id, strategy) in &self.strategies {
            let total = strategy.successes + strategy.failures;
            if total > STAGNATION_ATTEMPT_THRESHOLD {
                let success_rate = strategy.successes as f64 / total as f64;
                if success_rate < STAGNATION_SUCCESS_RATE {
                    flagged.push((*id, "stagnant"));
                }
            }
        }

        flagged
    }

    /// Populate strategies from a database load, replacing existing entries.
    pub fn load_from(&mut self, strategies: Vec<Strategy>) {
        for strategy in strategies {
            self.strategies.insert(strategy.id, strategy);
        }
    }

    /// Return references to all strategies for persistence.
    pub fn to_save(&self) -> Vec<&Strategy> {
        self.strategies.values().collect()
    }
}
