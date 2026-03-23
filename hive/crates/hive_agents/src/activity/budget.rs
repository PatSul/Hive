use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use super::log::ActivityLog;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetConfig {
    pub global_daily_limit_usd: Option<f64>,
    pub global_monthly_limit_usd: Option<f64>,
    pub per_agent_limit_usd: Option<f64>,
    pub per_task_limit_usd: Option<f64>,
    pub warning_threshold_pct: f64,
    pub on_exhaust: ExhaustAction,
}

impl Default for BudgetConfig {
    fn default() -> Self {
        Self {
            global_daily_limit_usd: None,
            global_monthly_limit_usd: None,
            per_agent_limit_usd: None,
            per_task_limit_usd: None,
            warning_threshold_pct: 0.8,
            on_exhaust: ExhaustAction::Pause,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ExhaustAction {
    Pause,
    ApprovalRequired,
    WarnOnly,
}

#[derive(Debug, Clone)]
pub enum BudgetDecision {
    Proceed,
    Warning { usage_pct: f64, message: String },
    Blocked { reason: String },
}

pub struct BudgetEnforcer {
    config: BudgetConfig,
    log: Arc<ActivityLog>,
}

impl std::fmt::Debug for BudgetEnforcer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BudgetEnforcer")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl BudgetEnforcer {
    pub fn new(config: BudgetConfig, log: Arc<ActivityLog>) -> Self {
        Self { config, log }
    }

    pub fn check(&self, agent_id: &str, estimated_cost: f64) -> BudgetDecision {
        if let Some(daily_limit) = self.config.global_daily_limit_usd {
            let since = Utc::now() - Duration::hours(24);
            if let Ok(summary) = self.log.cost_summary(None, since) {
                let projected = summary.total_usd + estimated_cost;
                let usage_pct = summary.total_usd / daily_limit;

                if projected >= daily_limit {
                    return BudgetDecision::Blocked {
                        reason: format!(
                            "Daily budget ${:.2} would be exceeded (current: ${:.2}, estimated: ${:.2})",
                            daily_limit, summary.total_usd, estimated_cost
                        ),
                    };
                }

                if usage_pct >= self.config.warning_threshold_pct {
                    return BudgetDecision::Warning {
                        usage_pct,
                        message: format!(
                            "Daily budget at {:.0}% (${:.2} / ${:.2})",
                            usage_pct * 100.0,
                            summary.total_usd,
                            daily_limit
                        ),
                    };
                }
            }
        }

        if let Some(agent_limit) = self.config.per_agent_limit_usd {
            let since = Utc::now() - Duration::days(30);
            if let Ok(summary) = self.log.cost_summary(Some(agent_id), since) {
                let projected = summary.total_usd + estimated_cost;
                if projected >= agent_limit {
                    return BudgetDecision::Blocked {
                        reason: format!(
                            "Agent '{agent_id}' monthly budget ${agent_limit:.2} would be exceeded"
                        ),
                    };
                }
            }
        }

        BudgetDecision::Proceed
    }

    pub fn config(&self) -> &BudgetConfig {
        &self.config
    }

    pub fn set_config(&mut self, config: BudgetConfig) {
        self.config = config;
    }
}
