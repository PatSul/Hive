//! Runtime routing policy.
//!
//! Parses the persisted [`hive_core::config::RoutingPolicy`] (primitive,
//! serde-friendly types) into a runtime form keyed by
//! [`CapabilityTaskType`] with typed [`ModelTier`] floors.
//!
//! The policy is **strictly additive**: an empty/default policy is inactive
//! ([`RuntimeRoutingPolicy::is_active`] returns `false`) and routing behaves
//! exactly as it did before policy support existed.

use std::collections::HashMap;

use tracing::warn;

use super::capability_router::CapabilityTaskType;
use crate::types::ModelTier;

/// Per-category allow-list + optional quality floor, in runtime form.
#[derive(Debug, Clone, Default)]
pub struct CategoryPool {
    /// Allowed model id substrings. Empty = all models allowed.
    pub allow: Vec<String>,
    /// Minimum tier. `None` = no floor.
    pub floor: Option<ModelTier>,
}

/// Runtime routing policy: parsed, validated, and ready for the router.
#[derive(Debug, Clone, Default)]
pub struct RuntimeRoutingPolicy {
    /// Per-category pools keyed by task type.
    pub categories: HashMap<CapabilityTaskType, CategoryPool>,
    /// Cost-aggressiveness in `[0.0, 1.0]` (0.0 = quality-max, 1.0 = thrift-max).
    pub cost_aggressiveness: f32,
    /// Optional escalation pool of model id substrings (reserved for future use).
    pub escalation_pool: Vec<String>,
}

/// Parse a floor string into a [`ModelTier`]. Returns `None` for an empty
/// string (meaning "no floor") and logs a warning for unknown values.
fn parse_floor(s: &str) -> Option<ModelTier> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return None;
    }
    match trimmed.to_lowercase().as_str() {
        "free" => Some(ModelTier::Free),
        "budget" => Some(ModelTier::Budget),
        "mid" => Some(ModelTier::Mid),
        "premium" => Some(ModelTier::Premium),
        other => {
            warn!(floor = other, "Unknown routing floor; ignoring");
            None
        }
    }
}

impl RuntimeRoutingPolicy {
    /// Build a runtime policy from the persisted config form.
    ///
    /// - Unknown category strings are skipped with a `warn!`.
    /// - Unknown floor strings are skipped (treated as no floor) with a `warn!`.
    /// - `cost_aggressiveness` is clamped to `[0.0, 1.0]`.
    pub fn from_config(cfg: &hive_core::config::RoutingPolicy) -> Self {
        let mut categories = HashMap::new();

        for cat in &cfg.categories {
            let Some(task) = CapabilityTaskType::from_display(&cat.category) else {
                warn!(category = %cat.category, "Unknown routing category; skipping");
                continue;
            };
            let pool = CategoryPool {
                allow: cat
                    .allow
                    .iter()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect(),
                floor: parse_floor(&cat.floor),
            };
            categories.insert(task, pool);
        }

        let cost_aggressiveness = cfg.cost_aggressiveness.clamp(0.0, 1.0);

        let escalation_pool = cfg
            .escalation_pool
            .iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        Self {
            categories,
            cost_aggressiveness,
            escalation_pool,
        }
    }

    /// Whether this policy changes routing at all. When `false`, the router
    /// must behave exactly as it did before policy support existed.
    pub fn is_active(&self) -> bool {
        !self.categories.is_empty() || self.cost_aggressiveness > 0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hive_core::config::{CategoryPolicy, RoutingPolicy};

    #[test]
    fn default_is_inactive() {
        let p = RuntimeRoutingPolicy::default();
        assert!(!p.is_active());
        assert!(p.categories.is_empty());
        assert_eq!(p.cost_aggressiveness, 0.0);
    }

    #[test]
    fn empty_config_is_inactive() {
        let p = RuntimeRoutingPolicy::from_config(&RoutingPolicy::default());
        assert!(!p.is_active());
    }

    #[test]
    fn from_config_parses_category_and_floor() {
        let cfg = RoutingPolicy {
            categories: vec![CategoryPolicy {
                category: "coding".into(),
                allow: vec!["glm-4.6".into(), "claude".into()],
                floor: "premium".into(),
            }],
            cost_aggressiveness: 0.5,
            escalation_pool: vec!["o3".into()],
        };
        let p = RuntimeRoutingPolicy::from_config(&cfg);
        assert!(p.is_active());
        let pool = p
            .categories
            .get(&CapabilityTaskType::Coding)
            .expect("coding pool present");
        assert_eq!(pool.allow, vec!["glm-4.6".to_string(), "claude".to_string()]);
        assert_eq!(pool.floor, Some(ModelTier::Premium));
        assert_eq!(p.cost_aggressiveness, 0.5);
        assert_eq!(p.escalation_pool, vec!["o3".to_string()]);
    }

    #[test]
    fn from_config_skips_unknown_category() {
        let cfg = RoutingPolicy {
            categories: vec![
                CategoryPolicy {
                    category: "not-a-real-category".into(),
                    allow: vec![],
                    floor: String::new(),
                },
                CategoryPolicy {
                    category: "math".into(),
                    allow: vec![],
                    floor: String::new(),
                },
            ],
            cost_aggressiveness: 0.0,
            escalation_pool: vec![],
        };
        let p = RuntimeRoutingPolicy::from_config(&cfg);
        // Only the valid "math" category survives.
        assert_eq!(p.categories.len(), 1);
        assert!(p.categories.contains_key(&CapabilityTaskType::Math));
    }

    #[test]
    fn from_config_skips_unknown_floor() {
        let cfg = RoutingPolicy {
            categories: vec![CategoryPolicy {
                category: "coding".into(),
                allow: vec![],
                floor: "ultra-premium".into(),
            }],
            cost_aggressiveness: 0.0,
            escalation_pool: vec![],
        };
        let p = RuntimeRoutingPolicy::from_config(&cfg);
        let pool = p.categories.get(&CapabilityTaskType::Coding).unwrap();
        assert_eq!(pool.floor, None, "unknown floor should be ignored");
    }

    #[test]
    fn from_config_clamps_cost_aggressiveness() {
        let high = RoutingPolicy {
            cost_aggressiveness: 5.0,
            ..Default::default()
        };
        assert_eq!(
            RuntimeRoutingPolicy::from_config(&high).cost_aggressiveness,
            1.0
        );

        let low = RoutingPolicy {
            cost_aggressiveness: -2.0,
            ..Default::default()
        };
        assert_eq!(
            RuntimeRoutingPolicy::from_config(&low).cost_aggressiveness,
            0.0
        );
    }

    #[test]
    fn case_insensitive_category_and_floor() {
        let cfg = RoutingPolicy {
            categories: vec![CategoryPolicy {
                category: "  Creative Writing ".into(),
                allow: vec![],
                floor: "  MID  ".into(),
            }],
            cost_aggressiveness: 0.0,
            escalation_pool: vec![],
        };
        let p = RuntimeRoutingPolicy::from_config(&cfg);
        let pool = p
            .categories
            .get(&CapabilityTaskType::CreativeWriting)
            .expect("creative writing parsed case-insensitively");
        assert_eq!(pool.floor, Some(ModelTier::Mid));
    }
}
