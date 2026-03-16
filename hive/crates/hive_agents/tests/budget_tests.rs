use hive_agents::activity::budget::{BudgetConfig, BudgetDecision, BudgetEnforcer, ExhaustAction};
use hive_agents::activity::log::ActivityLog;
use hive_agents::activity::{ActivityEvent, ActivityService};
use std::sync::Arc;

fn test_enforcer(daily_limit: f64) -> (Arc<ActivityLog>, BudgetEnforcer) {
    let log = Arc::new(ActivityLog::open_in_memory().unwrap());
    let config = BudgetConfig {
        global_daily_limit_usd: Some(daily_limit),
        global_monthly_limit_usd: None,
        per_agent_limit_usd: None,
        per_task_limit_usd: None,
        warning_threshold_pct: 0.8,
        on_exhaust: ExhaustAction::Pause,
    };
    let enforcer = BudgetEnforcer::new(config, log.clone());
    (log, enforcer)
}

#[test]
fn budget_proceed_when_under_limit() {
    let (log, enforcer) = test_enforcer(10.0);
    let decision = enforcer.check("agent-1", 0.5);
    assert!(matches!(decision, BudgetDecision::Proceed));
}

#[test]
fn budget_warning_at_threshold() {
    let (log, enforcer) = test_enforcer(10.0);
    for _ in 0..85 {
        log.record(&ActivityEvent::CostIncurred {
            agent_id: "agent-1".into(),
            model: "test".into(),
            input_tokens: 100,
            output_tokens: 50,
            cost_usd: 0.1,
        }).unwrap();
    }
    let decision = enforcer.check("agent-1", 0.5);
    assert!(matches!(decision, BudgetDecision::Warning { .. }));
}

#[test]
fn budget_blocked_at_limit() {
    let (log, enforcer) = test_enforcer(1.0);
    for _ in 0..15 {
        log.record(&ActivityEvent::CostIncurred {
            agent_id: "agent-1".into(),
            model: "test".into(),
            input_tokens: 100,
            output_tokens: 50,
            cost_usd: 0.1,
        }).unwrap();
    }
    let decision = enforcer.check("agent-1", 0.5);
    assert!(matches!(decision, BudgetDecision::Blocked { .. }));
}

#[test]
fn budget_no_limit_always_proceeds() {
    let log = Arc::new(ActivityLog::open_in_memory().unwrap());
    let config = BudgetConfig::default();
    let enforcer = BudgetEnforcer::new(config, log);
    let decision = enforcer.check("agent-1", 100.0);
    assert!(matches!(decision, BudgetDecision::Proceed));
}
