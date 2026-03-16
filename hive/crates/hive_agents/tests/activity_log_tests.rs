use hive_agents::activity::{ActivityEvent, PauseReason, FileOp};

#[test]
fn activity_event_serializes_to_json() {
    let event = ActivityEvent::AgentStarted {
        agent_id: "agent-1".into(),
        role: "Coder".into(),
        task_id: Some("task-42".into()),
    };
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("AgentStarted"));
    assert!(json.contains("agent-1"));
}

#[test]
fn activity_event_cost_incurred_round_trip() {
    let event = ActivityEvent::CostIncurred {
        agent_id: "agent-2".into(),
        model: "claude-sonnet-4-20250514".into(),
        input_tokens: 1000,
        output_tokens: 500,
        cost_usd: 0.012,
    };
    let json = serde_json::to_string(&event).unwrap();
    let parsed: ActivityEvent = serde_json::from_str(&json).unwrap();
    match parsed {
        ActivityEvent::CostIncurred { cost_usd, .. } => {
            assert!((cost_usd - 0.012).abs() < 0.001);
        }
        _ => panic!("wrong variant"),
    }
}

#[test]
fn pause_reason_variants() {
    let reasons = vec![
        PauseReason::BudgetExhausted,
        PauseReason::UserRequested,
        PauseReason::ApprovalTimeout,
        PauseReason::Error("test".into()),
    ];
    for reason in reasons {
        let json = serde_json::to_string(&reason).unwrap();
        let _: PauseReason = serde_json::from_str(&json).unwrap();
    }
}
