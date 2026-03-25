use hive_learn::cortex::guardrails::GuardrailsEngine;
use hive_learn::cortex::meta_learner::MetaLearner;
use hive_learn::cortex::types::{Domain, Strategy, StrategyId};

// ── T044: Weight increase on success ────────────────────────────────────

#[test]
fn test_weight_increases_on_success() {
    let mut meta = MetaLearner::new();
    let initial = meta.get_weight(StrategyId::PromptMutation);
    assert!((initial - 0.5).abs() < f64::EPSILON);

    meta.record_success(StrategyId::PromptMutation, 0.1);

    let after = meta.get_weight(StrategyId::PromptMutation);
    // 0.5 * 1.1 = 0.55
    assert!(
        (after - 0.55).abs() < 1e-9,
        "weight should be 0.55 after one success, got {after}"
    );
}

#[test]
fn test_weight_capped_at_max() {
    let mut meta = MetaLearner::new();

    // Drive weight to max: 0.5 * 1.1^n -> eventually > 1.0
    for _ in 0..50 {
        meta.record_success(StrategyId::TierAdjustment, 0.05);
    }

    let weight = meta.get_weight(StrategyId::TierAdjustment);
    assert!(
        weight <= 1.0 + f64::EPSILON,
        "weight must never exceed 1.0, got {weight}"
    );
    assert!(
        (weight - 1.0).abs() < f64::EPSILON,
        "weight should be capped at exactly 1.0, got {weight}"
    );
}

// ── T044: Weight decrease on failure ────────────────────────────────────

#[test]
fn test_weight_decreases_on_failure() {
    let mut meta = MetaLearner::new();

    meta.record_failure(StrategyId::PatternInjection);

    let weight = meta.get_weight(StrategyId::PatternInjection);
    // 0.5 * 0.7 = 0.35
    assert!(
        (weight - 0.35).abs() < 1e-9,
        "weight should be 0.35 after one failure, got {weight}"
    );
}

#[test]
fn test_weight_floored_at_min() {
    let mut meta = MetaLearner::new();

    // Drive weight to minimum: 0.5 * 0.7^n -> approaches 0
    for _ in 0..100 {
        meta.record_failure(StrategyId::CrossPollination);
    }

    let weight = meta.get_weight(StrategyId::CrossPollination);
    assert!(
        weight >= 0.1 - f64::EPSILON,
        "weight must never go below 0.1, got {weight}"
    );
    assert!(
        (weight - 0.1).abs() < f64::EPSILON,
        "weight should be floored at exactly 0.1, got {weight}"
    );
}

// ── T044: avg_impact running average ────────────────────────────────────

#[test]
fn test_avg_impact_single_success() {
    let mut meta = MetaLearner::new();

    meta.record_success(StrategyId::PromptMutation, 0.3);

    let strategy = meta.get_strategy(StrategyId::PromptMutation).unwrap();
    assert!(
        (strategy.avg_impact - 0.3).abs() < 1e-9,
        "avg_impact should be 0.3 after a single success with delta=0.3, got {}",
        strategy.avg_impact
    );
}

#[test]
fn test_avg_impact_running_average() {
    let mut meta = MetaLearner::new();

    meta.record_success(StrategyId::PromptMutation, 0.2);
    meta.record_success(StrategyId::PromptMutation, 0.4);

    let strategy = meta.get_strategy(StrategyId::PromptMutation).unwrap();
    // Running average of [0.2, 0.4] = 0.3
    assert!(
        (strategy.avg_impact - 0.3).abs() < 1e-9,
        "avg_impact should be 0.3 (average of 0.2 and 0.4), got {}",
        strategy.avg_impact
    );
}

#[test]
fn test_avg_impact_three_values() {
    let mut meta = MetaLearner::new();

    meta.record_success(StrategyId::TierAdjustment, 0.1);
    meta.record_success(StrategyId::TierAdjustment, 0.2);
    meta.record_success(StrategyId::TierAdjustment, 0.6);

    let strategy = meta.get_strategy(StrategyId::TierAdjustment).unwrap();
    // Running average of [0.1, 0.2, 0.6] = 0.3
    assert!(
        (strategy.avg_impact - 0.3).abs() < 1e-9,
        "avg_impact should be 0.3 (average of 0.1, 0.2, 0.6), got {}",
        strategy.avg_impact
    );
}

// ── T044: should_trigger threshold adjustment ───────────────────────────

#[test]
fn test_should_trigger_default_weight() {
    let meta = MetaLearner::new();

    // Default weight is 0.5 -> threshold = base * (1.0 + (0.5 - 0.5)) = base * 1.0
    let threshold = meta.should_trigger(StrategyId::PromptMutation, 0.8);
    assert!(
        (threshold - 0.8).abs() < 1e-9,
        "at default weight 0.5, threshold should equal base, got {threshold}"
    );
}

#[test]
fn test_should_trigger_high_weight_lowers_threshold() {
    let mut meta = MetaLearner::new();

    // Drive weight up
    for _ in 0..10 {
        meta.record_success(StrategyId::PromptMutation, 0.1);
    }

    let weight = meta.get_weight(StrategyId::PromptMutation);
    assert!(weight > 0.5, "weight should be above 0.5 after successes");

    let threshold = meta.should_trigger(StrategyId::PromptMutation, 0.8);
    assert!(
        threshold < 0.8,
        "high weight should lower the threshold below base, got {threshold}"
    );
}

#[test]
fn test_should_trigger_low_weight_raises_threshold() {
    let mut meta = MetaLearner::new();

    // Drive weight down
    for _ in 0..5 {
        meta.record_failure(StrategyId::PatternInjection);
    }

    let weight = meta.get_weight(StrategyId::PatternInjection);
    assert!(weight < 0.5, "weight should be below 0.5 after failures");

    let threshold = meta.should_trigger(StrategyId::PatternInjection, 0.8);
    assert!(
        threshold > 0.8,
        "low weight should raise the threshold above base, got {threshold}"
    );
}

// ── T044: Stagnation detection ──────────────────────────────────────────

#[test]
fn test_review_all_detects_stagnation() {
    let mut meta = MetaLearner::new();

    // 11 failures, 1 success = 12 attempts, ~8.3% success rate (< 20%)
    for _ in 0..11 {
        meta.record_failure(StrategyId::CrossPollination);
    }
    meta.record_success(StrategyId::CrossPollination, 0.05);

    let flagged = meta.review_all();
    assert!(
        flagged
            .iter()
            .any(|(id, label)| *id == StrategyId::CrossPollination && *label == "stagnant"),
        "CrossPollination should be flagged as stagnant"
    );
}

#[test]
fn test_review_all_no_stagnation_when_healthy() {
    let mut meta = MetaLearner::new();

    // 8 successes, 4 failures = 12 attempts, ~67% success rate (> 20%)
    for _ in 0..8 {
        meta.record_success(StrategyId::PromptMutation, 0.1);
    }
    for _ in 0..4 {
        meta.record_failure(StrategyId::PromptMutation);
    }

    let flagged = meta.review_all();
    assert!(
        !flagged
            .iter()
            .any(|(id, _)| *id == StrategyId::PromptMutation),
        "PromptMutation should NOT be flagged when success rate is healthy"
    );
}

#[test]
fn test_review_all_no_stagnation_below_attempt_threshold() {
    let mut meta = MetaLearner::new();

    // Only 5 failures = 5 attempts (below threshold of 10)
    for _ in 0..5 {
        meta.record_failure(StrategyId::TierAdjustment);
    }

    let flagged = meta.review_all();
    assert!(
        !flagged
            .iter()
            .any(|(id, _)| *id == StrategyId::TierAdjustment),
        "should not flag strategies below the attempt threshold"
    );
}

// ── T044: load_from populates correctly ─────────────────────────────────

#[test]
fn test_load_from_populates_strategies() {
    let mut meta = MetaLearner::new();

    let custom_strategy = Strategy {
        id: StrategyId::PromptMutation,
        domain: Domain::Prompts,
        weight: 0.9,
        attempts: 50,
        successes: 40,
        failures: 10,
        avg_impact: 0.25,
        last_adjusted: 1700000000,
    };

    meta.load_from(vec![custom_strategy]);

    let loaded = meta.get_strategy(StrategyId::PromptMutation).unwrap();
    assert!((loaded.weight - 0.9).abs() < f64::EPSILON);
    assert_eq!(loaded.successes, 40);
    assert_eq!(loaded.failures, 10);
    assert!((loaded.avg_impact - 0.25).abs() < f64::EPSILON);
}

#[test]
fn test_load_from_replaces_defaults() {
    let mut meta = MetaLearner::new();

    // Before load: default weight is 0.5
    assert!((meta.get_weight(StrategyId::TierAdjustment) - 0.5).abs() < f64::EPSILON);

    let loaded_strategy = Strategy {
        id: StrategyId::TierAdjustment,
        domain: Domain::Routing,
        weight: 0.75,
        attempts: 20,
        successes: 15,
        failures: 5,
        avg_impact: 0.18,
        last_adjusted: 1700000000,
    };

    meta.load_from(vec![loaded_strategy]);

    assert!(
        (meta.get_weight(StrategyId::TierAdjustment) - 0.75).abs() < f64::EPSILON,
        "load_from should replace existing default strategy"
    );
}

// ── T044: to_save returns all strategies ────────────────────────────────

#[test]
fn test_to_save_returns_all() {
    let meta = MetaLearner::new();
    let to_save = meta.to_save();
    assert_eq!(to_save.len(), 4, "should return all 4 default strategies");
}

// ── T044: all_strategies returns all ────────────────────────────────────

#[test]
fn test_all_strategies_returns_four() {
    let meta = MetaLearner::new();
    let all = meta.all_strategies();
    assert_eq!(all.len(), 4, "should have 4 strategies by default");
}

// ── T042: GuardrailsEngine adjusted_threshold ───────────────────────────

#[test]
fn test_guardrails_adjusted_threshold_delegates() {
    let meta = MetaLearner::new();
    let engine = GuardrailsEngine::new();

    // Default weight = 0.5, so adjusted threshold = base * 1.0 = base
    let threshold = engine.adjusted_threshold(&meta, StrategyId::PromptMutation, 0.7);
    assert!(
        (threshold - 0.7).abs() < 1e-9,
        "adjusted_threshold should delegate to MetaLearner::should_trigger, got {threshold}"
    );
}

#[test]
fn test_guardrails_adjusted_threshold_reflects_weight() {
    let mut meta = MetaLearner::new();
    let engine = GuardrailsEngine::new();

    // Drive weight up
    for _ in 0..5 {
        meta.record_success(StrategyId::TierAdjustment, 0.1);
    }

    let threshold = engine.adjusted_threshold(&meta, StrategyId::TierAdjustment, 0.7);
    assert!(
        threshold < 0.7,
        "high-weight strategy should lower the threshold, got {threshold}"
    );
}
