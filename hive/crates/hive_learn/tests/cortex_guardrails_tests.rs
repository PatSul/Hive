use std::sync::Arc;

use hive_learn::cortex::event_bus::create_event_bus;
use hive_learn::cortex::guardrails::GuardrailsEngine;
use hive_learn::cortex::types::{ChangeStatus, CortexChange, Domain, Tier};
use hive_learn::cortex::LearningCortex;
use hive_learn::storage::LearningStorage;

/// Helper: build a LearningCortex backed by an in-memory database.
fn make_cortex() -> LearningCortex {
    let storage = Arc::new(LearningStorage::in_memory().unwrap());
    let (tx, _rx) = create_event_bus();
    LearningCortex::new(storage, tx)
}

/// Helper: build a CortexChange with sensible defaults.
fn make_change(tier: Tier, quality_before: Option<f64>, soak_until: i64) -> CortexChange {
    CortexChange {
        change_id: format!("change-{}", uuid::Uuid::new_v4()),
        domain: Domain::Prompts,
        tier,
        action: "{}".to_string(),
        prior_state: "{}".to_string(),
        applied_at: chrono::Utc::now().timestamp(),
        soak_until,
        status: ChangeStatus::Soaking,
        quality_before,
        quality_after: None,
    }
}

// ── T018: can_auto_apply tests ──────────────────────────────────────────

#[test]
fn test_can_auto_apply_false_when_disabled() {
    let cortex = make_cortex();
    cortex.set_auto_apply_enabled(false);

    let engine = GuardrailsEngine::new();
    let result = engine.can_auto_apply(Tier::Green, &cortex).unwrap();
    assert!(
        !result,
        "auto-apply should be blocked when globally disabled"
    );
}

#[test]
fn test_can_auto_apply_false_when_user_active() {
    let cortex = make_cortex();
    // User just interacted, so is_idle() returns false
    cortex.mark_active();

    let engine = GuardrailsEngine::new();
    let result = engine.can_auto_apply(Tier::Green, &cortex).unwrap();
    assert!(!result, "auto-apply should be blocked when user is active");
}

#[test]
fn test_can_auto_apply_false_when_3_changes_in_24h() {
    let cortex = make_cortex();
    // Make the user appear idle by not calling mark_active (timestamp stays at construction time,
    // but we need the idle check to pass). We force idle by setting the interaction to the past.
    force_idle(&cortex);

    // Insert 3 recent changes
    for i in 0..3 {
        let mut change = make_change(Tier::Green, Some(0.8), future_timestamp(3600));
        change.change_id = format!("rate-limit-{i}");
        change.status = ChangeStatus::Confirmed;
        cortex.insert_change(&change).unwrap();
    }

    let engine = GuardrailsEngine::new();
    let result = engine.can_auto_apply(Tier::Yellow, &cortex).unwrap();
    assert!(!result, "auto-apply should be blocked at 3 changes in 24h");
}

#[test]
fn test_can_auto_apply_true_when_all_conditions_met() {
    let cortex = make_cortex();
    force_idle(&cortex);

    let engine = GuardrailsEngine::new();
    let result = engine.can_auto_apply(Tier::Green, &cortex).unwrap();
    assert!(
        result,
        "auto-apply should be allowed when all conditions pass"
    );
}

// ── T018: should_rollback tests ─────────────────────────────────────────

#[test]
fn test_should_rollback_true_when_quality_degrades_green() {
    let engine = GuardrailsEngine::new();
    // Green tier threshold is 0.0 -- any degradation should trigger rollback
    let change = make_change(Tier::Green, Some(0.8), future_timestamp(3600));

    assert!(
        engine.should_rollback(&change, 0.79),
        "Green tier should rollback on any quality drop"
    );
}

#[test]
fn test_should_rollback_true_when_quality_degrades_yellow() {
    let engine = GuardrailsEngine::new();
    // Yellow tier threshold is 0.15
    let change = make_change(Tier::Yellow, Some(0.8), future_timestamp(3600));

    // 0.8 - 0.6 = 0.2 > 0.15 threshold
    assert!(
        engine.should_rollback(&change, 0.6),
        "Yellow tier should rollback when drop > 15%"
    );
}

#[test]
fn test_should_rollback_false_when_quality_held() {
    let engine = GuardrailsEngine::new();
    let change = make_change(Tier::Yellow, Some(0.8), future_timestamp(3600));

    // 0.8 - 0.75 = 0.05 < 0.15 threshold
    assert!(
        !engine.should_rollback(&change, 0.75),
        "should not rollback when quality drop is within threshold"
    );
}

#[test]
fn test_should_rollback_false_when_no_baseline() {
    let engine = GuardrailsEngine::new();
    let change = make_change(Tier::Red, None, future_timestamp(3600));

    assert!(
        !engine.should_rollback(&change, 0.5),
        "should not rollback when no baseline quality was recorded"
    );
}

// ── T018: soak_expired tests ────────────────────────────────────────────

#[test]
fn test_soak_expired_true_when_time_passed() {
    let engine = GuardrailsEngine::new();
    // soak_until is in the past
    let change = make_change(Tier::Yellow, Some(0.8), past_timestamp(3600));

    assert!(
        engine.soak_expired(&change),
        "soak should be expired when soak_until is in the past"
    );
}

#[test]
fn test_soak_expired_false_when_still_soaking() {
    let engine = GuardrailsEngine::new();
    let change = make_change(Tier::Yellow, Some(0.8), future_timestamp(3600));

    assert!(
        !engine.soak_expired(&change),
        "soak should not be expired when soak_until is in the future"
    );
}

// ── T022-T023: check_soaking_changes tests ──────────────────────────────

#[test]
fn test_check_soaking_confirms_expired_healthy_change() {
    let cortex = make_cortex();
    let engine = GuardrailsEngine::new();

    // Insert a soaking change whose soak period has expired, with good quality
    let mut change = make_change(Tier::Yellow, Some(0.8), past_timestamp(3600));
    change.quality_after = Some(0.85); // quality held or improved
    cortex.insert_change(&change).unwrap();

    let updates = engine.check_soaking_changes(&cortex);
    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0].0, change.change_id);
    assert_eq!(updates[0].1, ChangeStatus::Confirmed);
}

#[test]
fn test_check_soaking_rolls_back_degraded_change() {
    let cortex = make_cortex();
    let engine = GuardrailsEngine::new();

    // Insert a soaking change where quality has regressed badly
    // Yellow threshold is 0.15 -- so 0.8 - 0.5 = 0.3 > 0.15
    let mut change = make_change(Tier::Yellow, Some(0.8), future_timestamp(3600));
    change.quality_after = Some(0.5);
    cortex.insert_change(&change).unwrap();

    let updates = engine.check_soaking_changes(&cortex);
    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0].0, change.change_id);
    assert_eq!(updates[0].1, ChangeStatus::RolledBack);
}

#[test]
fn test_check_soaking_skips_still_soaking_healthy() {
    let cortex = make_cortex();
    let engine = GuardrailsEngine::new();

    // Soak not expired, quality is fine
    let mut change = make_change(Tier::Yellow, Some(0.8), future_timestamp(3600));
    change.quality_after = Some(0.78); // 0.8 - 0.78 = 0.02 < 0.15
    cortex.insert_change(&change).unwrap();

    let updates = engine.check_soaking_changes(&cortex);
    assert!(
        updates.is_empty(),
        "should skip changes that are still soaking and healthy"
    );
}

// ── T020-T021: quality degradation detection tests ──────────────────────

#[test]
fn test_quality_degradation_detected() {
    let mut cortex = make_cortex();

    // Record 25 low-quality scores
    for _ in 0..25 {
        cortex.record_quality("test-persona", 0.4);
    }

    assert!(
        cortex.check_quality_degradation("test-persona"),
        "should detect degradation when avg < 0.6 over 20+ samples"
    );
}

#[test]
fn test_no_degradation_when_quality_healthy() {
    let mut cortex = make_cortex();

    for _ in 0..25 {
        cortex.record_quality("good-persona", 0.85);
    }

    assert!(
        !cortex.check_quality_degradation("good-persona"),
        "should not detect degradation when quality is healthy"
    );
}

#[test]
fn test_no_degradation_with_insufficient_samples() {
    let mut cortex = make_cortex();

    // Only 10 samples -- below the 20-sample minimum
    for _ in 0..10 {
        cortex.record_quality("new-persona", 0.3);
    }

    assert!(
        !cortex.check_quality_degradation("new-persona"),
        "should not detect degradation with fewer than 20 samples"
    );
}

#[test]
fn test_no_degradation_for_unknown_persona() {
    let cortex = make_cortex();

    assert!(
        !cortex.check_quality_degradation("nonexistent"),
        "should not detect degradation for unknown persona"
    );
}

#[test]
fn test_should_trigger_autoresearch_returns_degraded_persona() {
    let mut cortex = make_cortex();

    // One healthy, one degraded
    for _ in 0..25 {
        cortex.record_quality("healthy", 0.9);
        cortex.record_quality("struggling", 0.3);
    }

    let result = cortex.should_trigger_autoresearch();
    assert!(
        result.is_some(),
        "should find a persona needing improvement"
    );
    // The degraded persona should be the one returned
    assert_eq!(result.unwrap(), "struggling");
}

#[test]
fn test_should_trigger_autoresearch_returns_none_when_healthy() {
    let mut cortex = make_cortex();

    for _ in 0..25 {
        cortex.record_quality("persona-a", 0.9);
        cortex.record_quality("persona-b", 0.85);
    }

    assert!(
        cortex.should_trigger_autoresearch().is_none(),
        "should return None when all personas are healthy"
    );
}

// ── T024: startup recovery test ─────────────────────────────────────────

#[test]
fn test_startup_recovery_succeeds_with_no_changes() {
    let cortex = make_cortex();
    cortex.startup_recovery().unwrap();
}

#[test]
fn test_startup_recovery_with_expired_soaking_changes() {
    let cortex = make_cortex();

    // Insert a change whose soak expired while "offline"
    let change = make_change(Tier::Yellow, Some(0.8), past_timestamp(7200));
    cortex.insert_change(&change).unwrap();

    // Should complete without error
    cortex.startup_recovery().unwrap();

    // The change should still be in soaking status -- the monitor picks it up next
    let soaking = cortex.load_soaking_changes().unwrap();
    assert_eq!(soaking.len(), 1);
}

// ── Helpers ─────────────────────────────────────────────────────────────

/// Force the cortex to appear idle by setting last interaction far in the past.
fn force_idle(cortex: &LearningCortex) {
    use std::sync::atomic::Ordering;
    let past = chrono::Utc::now().timestamp() - 120; // 2 minutes ago
    cortex.interaction_tracker().store(past, Ordering::Relaxed);
}

fn future_timestamp(secs: i64) -> i64 {
    chrono::Utc::now().timestamp() + secs
}

fn past_timestamp(secs: i64) -> i64 {
    chrono::Utc::now().timestamp() - secs
}
