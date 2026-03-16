use hive_agents::activity::rules::{ApprovalRule, RuleTrigger};
use hive_agents::activity::OperationType;

#[test]
fn rule_matches_shell_command_pattern() {
    let rule = ApprovalRule {
        name: "git-push".into(),
        enabled: true,
        trigger: RuleTrigger::CommandMatches { pattern: "git push*".into() },
        priority: 80,
    };
    let op = OperationType::ShellCommand("git push origin main".into());
    assert!(rule.matches(&op));
}

#[test]
fn rule_does_not_match_different_command() {
    let rule = ApprovalRule {
        name: "git-push".into(),
        enabled: true,
        trigger: RuleTrigger::CommandMatches { pattern: "git push*".into() },
        priority: 80,
    };
    let op = OperationType::ShellCommand("git status".into());
    assert!(!rule.matches(&op));
}

#[test]
fn rule_matches_cost_threshold() {
    let rule = ApprovalRule {
        name: "expensive".into(),
        enabled: true,
        trigger: RuleTrigger::CostExceeds { usd: 5.0 },
        priority: 90,
    };
    let op = OperationType::AiCall { model: "claude-opus-4-6".into(), estimated_cost: 7.50 };
    assert!(rule.matches(&op));
}

#[test]
fn rule_cost_under_threshold_no_match() {
    let rule = ApprovalRule {
        name: "expensive".into(),
        enabled: true,
        trigger: RuleTrigger::CostExceeds { usd: 5.0 },
        priority: 90,
    };
    let op = OperationType::AiCall { model: "claude-haiku".into(), estimated_cost: 0.50 };
    assert!(!rule.matches(&op));
}

#[test]
fn rule_matches_path_glob() {
    let rule = ApprovalRule {
        name: "protect-core".into(),
        enabled: true,
        trigger: RuleTrigger::PathMatches { glob: "src/core/**".into() },
        priority: 75,
    };
    let op = OperationType::FileModify {
        path: "src/core/config.rs".into(),
        scope: "1 file".into(),
    };
    assert!(rule.matches(&op));
}

#[test]
fn disabled_rule_never_matches() {
    let rule = ApprovalRule {
        name: "disabled".into(),
        enabled: false,
        trigger: RuleTrigger::Always,
        priority: 100,
    };
    let op = OperationType::ShellCommand("anything".into());
    assert!(!rule.matches(&op));
}

#[test]
fn rules_sorted_by_priority_descending() {
    let mut rules = vec![
        ApprovalRule { name: "low".into(), enabled: true, trigger: RuleTrigger::Always, priority: 10 },
        ApprovalRule { name: "high".into(), enabled: true, trigger: RuleTrigger::Always, priority: 100 },
        ApprovalRule { name: "mid".into(), enabled: true, trigger: RuleTrigger::Always, priority: 50 },
    ];
    rules.sort_by(|a, b| b.priority.cmp(&a.priority));
    assert_eq!(rules[0].name, "high");
    assert_eq!(rules[1].name, "mid");
    assert_eq!(rules[2].name, "low");
}

use hive_agents::activity::approval::{ApprovalGate, ApprovalDecision};

#[tokio::test]
async fn approval_gate_no_rules_match_proceeds() {
    let gate = ApprovalGate::new(vec![]);
    let result = gate.check_with_channel("agent-1", &OperationType::ShellCommand("ls".into()));
    assert!(result.is_none()); // None = no approval needed
}

#[tokio::test]
async fn approval_gate_rule_match_creates_request() {
    let rules = vec![
        ApprovalRule {
            name: "always".into(),
            enabled: true,
            trigger: RuleTrigger::Always,
            priority: 100,
        },
    ];
    let gate = ApprovalGate::new(rules);

    let pending = gate.check_sync("agent-1", &OperationType::ShellCommand("test".into()));
    assert!(pending.is_some());
    let request = pending.unwrap();
    assert_eq!(request.matched_rule, "always");

    gate.respond(&request.id, ApprovalDecision::Approved);
    assert_eq!(gate.pending_count(), 0);
}

#[tokio::test]
async fn approval_gate_respond_deny() {
    let rules = vec![
        ApprovalRule {
            name: "always".into(),
            enabled: true,
            trigger: RuleTrigger::Always,
            priority: 100,
        },
    ];
    let gate = ApprovalGate::new(rules);

    let pending = gate.check_sync("agent-1", &OperationType::ShellCommand("test".into()));
    assert!(pending.is_some());

    gate.respond(&pending.unwrap().id, ApprovalDecision::Denied { reason: Some("nope".into()) });
    assert_eq!(gate.pending_count(), 0);
}

use hive_agents::activity::notification::{NotificationService, NotificationKind};

#[test]
fn notification_service_push_and_read() {
    let svc = NotificationService::new();
    svc.push(NotificationKind::AgentCompleted, "Agent finished task");
    assert_eq!(svc.unread_count(), 1);

    let items = svc.all();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].summary, "Agent finished task");
}

#[test]
fn notification_mark_read() {
    let svc = NotificationService::new();
    svc.push(NotificationKind::BudgetWarning, "Budget at 80%");
    assert_eq!(svc.unread_count(), 1);

    let items = svc.all();
    svc.mark_read(&items[0].id);
    assert_eq!(svc.unread_count(), 0);
}

#[test]
fn notification_dismiss() {
    let svc = NotificationService::new();
    svc.push(NotificationKind::AgentCompleted, "Done");
    svc.push(NotificationKind::BudgetWarning, "Warning");
    assert_eq!(svc.all().len(), 2);

    let id = svc.all()[0].id.clone();
    svc.dismiss(&id);
    assert_eq!(svc.all().len(), 1);
}
