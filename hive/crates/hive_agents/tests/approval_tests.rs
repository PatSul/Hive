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
