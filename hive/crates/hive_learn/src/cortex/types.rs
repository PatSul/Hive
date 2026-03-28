use serde::{Deserialize, Serialize};

/// What kind of improvement the Cortex can apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Domain {
    Routing,
    Prompts,
    Patterns,
    SwarmConfig,
}

impl Domain {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Routing => "routing",
            Self::Prompts => "prompts",
            Self::Patterns => "patterns",
            Self::SwarmConfig => "swarm_config",
        }
    }
}

/// Identifies a specific improvement strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StrategyId {
    PromptMutation,
    TierAdjustment,
    PatternInjection,
    CrossPollination,
}

impl StrategyId {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PromptMutation => "prompt_mutation",
            Self::TierAdjustment => "tier_adjustment",
            Self::PatternInjection => "pattern_injection",
            Self::CrossPollination => "cross_pollination",
        }
    }

    pub fn default_domain(&self) -> Domain {
        match self {
            Self::PromptMutation => Domain::Prompts,
            Self::TierAdjustment => Domain::Routing,
            Self::PatternInjection => Domain::Patterns,
            Self::CrossPollination => Domain::SwarmConfig,
        }
    }
}

/// Mirrors hive_agents::collective_memory::MemoryCategory without importing it.
/// Conversion happens in CortexBridgeImpl (hive_app).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CortexMemoryCategory {
    SuccessPattern,
    FailurePattern,
    ModelInsight,
    ConflictResolution,
    CodePattern,
    UserPreference,
    General,
}

impl CortexMemoryCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SuccessPattern => "success_pattern",
            Self::FailurePattern => "failure_pattern",
            Self::ModelInsight => "model_insight",
            Self::ConflictResolution => "conflict_resolution",
            Self::CodePattern => "code_pattern",
            Self::UserPreference => "user_preference",
            Self::General => "general",
        }
    }
}

/// A cross-system insight translated between learning systems.
/// Uses only primitive types — no cross-crate imports.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgedMemoryEntry {
    pub category: String,
    pub content: String,
    /// Maps to MemoryEntry.relevance_score.
    pub relevance_score: f64,
    /// Converted from MemoryEntry.created_at (RFC 3339) by CortexBridgeImpl.
    pub timestamp_epoch: i64,
}

/// Persisted event log row for Cortex observability queries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CortexEventRecord {
    pub id: i64,
    pub event_type: String,
    pub payload: String,
    pub timestamp: i64,
}

/// Auto-apply safety tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    /// Routing adjustments — apply immediately, monitor 10 interactions.
    Green,
    /// Prompt promotions — 1-hour soak period.
    Yellow,
    /// Pattern injection, strategy weight changes — 24-hour soak period.
    Red,
}

impl Tier {
    pub fn soak_duration_secs(&self) -> i64 {
        match self {
            Self::Green => 0,     // Immediate, but monitored
            Self::Yellow => 3600, // 1 hour
            Self::Red => 86400,   // 24 hours
        }
    }

    pub fn rollback_threshold(&self) -> f64 {
        match self {
            Self::Green => 0.0,   // Any degradation in next 10 interactions
            Self::Yellow => 0.15, // Quality drops > 15%
            Self::Red => 0.10,    // Quality drops > 10%
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Green => "green",
            Self::Yellow => "yellow",
            Self::Red => "red",
        }
    }
}

/// Status of an applied change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeStatus {
    Soaking,
    Confirmed,
    RolledBack,
}

impl ChangeStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Soaking => "soaking",
            Self::Confirmed => "confirmed",
            Self::RolledBack => "rolled_back",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "soaking" => Self::Soaking,
            "confirmed" => Self::Confirmed,
            "rolled_back" => Self::RolledBack,
            _ => Self::Soaking,
        }
    }
}

/// A meta-learning record tracking strategy effectiveness.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Strategy {
    pub id: StrategyId,
    pub domain: Domain,
    /// 0.0-1.0, how much to trust this strategy. Default: 0.5.
    pub weight: f64,
    pub attempts: u32,
    /// Times the improvement survived soak period.
    pub successes: u32,
    /// Times the improvement was rolled back.
    pub failures: u32,
    /// Average quality delta when successful.
    pub avg_impact: f64,
    /// Unix epoch seconds.
    pub last_adjusted: i64,
}

impl Strategy {
    pub fn new(id: StrategyId) -> Self {
        Self {
            domain: id.default_domain(),
            id,
            weight: 0.5,
            attempts: 0,
            successes: 0,
            failures: 0,
            avg_impact: 0.0,
            last_adjusted: chrono::Utc::now().timestamp(),
        }
    }
}

/// A record of an applied improvement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CortexChange {
    pub change_id: String,
    pub domain: Domain,
    pub tier: Tier,
    /// JSON: what was changed.
    pub action: String,
    /// JSON: snapshot for rollback.
    pub prior_state: String,
    /// Unix epoch seconds.
    pub applied_at: i64,
    /// Unix epoch seconds.
    pub soak_until: i64,
    pub status: ChangeStatus,
    pub quality_before: Option<f64>,
    pub quality_after: Option<f64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tier_soak_durations() {
        assert_eq!(Tier::Green.soak_duration_secs(), 0);
        assert_eq!(Tier::Yellow.soak_duration_secs(), 3600);
        assert_eq!(Tier::Red.soak_duration_secs(), 86400);
    }

    #[test]
    fn test_strategy_defaults() {
        let s = Strategy::new(StrategyId::PromptMutation);
        assert_eq!(s.domain, Domain::Prompts);
        assert!((s.weight - 0.5).abs() < f64::EPSILON);
        assert_eq!(s.attempts, 0);
    }

    #[test]
    fn test_domain_serde_roundtrip() {
        for domain in [
            Domain::Routing,
            Domain::Prompts,
            Domain::Patterns,
            Domain::SwarmConfig,
        ] {
            let json = serde_json::to_string(&domain).unwrap();
            let parsed: Domain = serde_json::from_str(&json).unwrap();
            assert_eq!(domain, parsed);
        }
    }

    #[test]
    fn test_strategy_id_serde_roundtrip() {
        for id in [
            StrategyId::PromptMutation,
            StrategyId::TierAdjustment,
            StrategyId::PatternInjection,
            StrategyId::CrossPollination,
        ] {
            let json = serde_json::to_string(&id).unwrap();
            let parsed: StrategyId = serde_json::from_str(&json).unwrap();
            assert_eq!(id, parsed);
        }
    }

    #[test]
    fn test_change_status_from_str() {
        assert_eq!(ChangeStatus::from_str("soaking"), ChangeStatus::Soaking);
        assert_eq!(ChangeStatus::from_str("confirmed"), ChangeStatus::Confirmed);
        assert_eq!(
            ChangeStatus::from_str("rolled_back"),
            ChangeStatus::RolledBack
        );
        assert_eq!(ChangeStatus::from_str("unknown"), ChangeStatus::Soaking);
    }
}
