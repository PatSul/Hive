use serde::{Deserialize, Serialize};

/// Configuration for the autoresearch improvement loop.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoResearchConfig {
    // -- Loop bounds --
    /// Maximum number of mutation iterations. Default: 10.
    pub max_iterations: u32,
    /// How many times to run the skill per eval (averaged). Default: 3.
    pub eval_samples_per_iteration: u32,
    /// Stop after this many consecutive iterations with no improvement. Default: 3.
    pub plateau_threshold: u32,

    // -- Quality gates --
    /// New prompt must beat current best by at least this much. Default: 0.05.
    pub min_improvement_threshold: f64,
    /// New prompt must have at least this pass rate to replace active. Default: 0.4.
    pub min_pass_rate_to_replace: f64,
    /// Stop immediately if pass rate reaches 1.0. Default: true.
    pub perfect_score_early_stop: bool,

    // -- Model overrides --
    /// Model for eval judging. None = skill's own model.
    pub eval_model: Option<String>,
    /// Model for prompt mutation. None = skill's own model.
    pub mutation_model: Option<String>,
    /// Model for skill execution during eval. None = skill's own model.
    pub skill_execution_model: Option<String>,

    // -- Safety --
    /// Maximum character length for mutated prompts. Default: 2000.
    pub max_prompt_length: usize,
    /// Optional USD budget cap. None = unlimited.
    pub cost_budget: Option<f64>,
    /// USD cost per token for budget tracking. Default: 0.000003.
    pub cost_per_token: f64,
    /// Run injection scan on every mutated prompt. Default: true.
    pub require_security_scan: bool,
}

impl Default for AutoResearchConfig {
    fn default() -> Self {
        Self {
            max_iterations: 10,
            eval_samples_per_iteration: 3,
            plateau_threshold: 3,
            min_improvement_threshold: 0.05,
            min_pass_rate_to_replace: 0.4,
            perfect_score_early_stop: true,
            eval_model: None,
            mutation_model: None,
            skill_execution_model: None,
            max_prompt_length: 2000,
            cost_budget: None,
            cost_per_token: 0.000003,
            require_security_scan: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_values() {
        let config = AutoResearchConfig::default();
        assert_eq!(config.max_iterations, 10);
        assert_eq!(config.eval_samples_per_iteration, 3);
        assert_eq!(config.plateau_threshold, 3);
        assert!((config.min_improvement_threshold - 0.05).abs() < f64::EPSILON);
        assert!((config.min_pass_rate_to_replace - 0.4).abs() < f64::EPSILON);
        assert!(config.perfect_score_early_stop);
        assert!(config.eval_model.is_none());
        assert!(config.mutation_model.is_none());
        assert!(config.skill_execution_model.is_none());
        assert_eq!(config.max_prompt_length, 2000);
        assert!(config.cost_budget.is_none());
        assert!((config.cost_per_token - 0.000003).abs() < f64::EPSILON);
        assert!(config.require_security_scan);
    }

    #[test]
    fn test_serde_roundtrip() {
        let config = AutoResearchConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let parsed: AutoResearchConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.max_iterations, config.max_iterations);
        assert_eq!(parsed.max_prompt_length, config.max_prompt_length);
        assert!((parsed.cost_per_token - config.cost_per_token).abs() < f64::EPSILON);
    }

    #[test]
    fn test_config_with_overrides() {
        let config = AutoResearchConfig {
            max_iterations: 20,
            eval_model: Some("claude-3-haiku".into()),
            cost_budget: Some(1.0),
            ..Default::default()
        };
        assert_eq!(config.max_iterations, 20);
        assert_eq!(config.eval_model, Some("claude-3-haiku".into()));
        assert_eq!(config.cost_budget, Some(1.0));
        assert_eq!(config.eval_samples_per_iteration, 3);
    }
}
