use hive_agents::activity::log::{ActivityFilter, ActivityLog};
use hive_agents::activity::{ActivityEvent, ActivityService, PauseReason};

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

#[tokio::test]
async fn activity_service_emits_and_receives_events() {
    let service = ActivityService::new_bus_only();
    let mut rx = service.subscribe();

    service.emit(ActivityEvent::AgentStarted {
        agent_id: "test-agent".into(),
        role: "Coder".into(),
        task_id: None,
    });

    let event = rx.recv().await.unwrap();
    assert_eq!(event.event_type(), "agent_started");
    assert_eq!(event.agent_id(), Some("test-agent"));
}

#[tokio::test]
async fn activity_service_multiple_subscribers() {
    let service = ActivityService::new_bus_only();
    let mut rx1 = service.subscribe();
    let mut rx2 = service.subscribe();

    service.emit(ActivityEvent::CostIncurred {
        agent_id: "a".into(),
        model: "test".into(),
        input_tokens: 100,
        output_tokens: 50,
        cost_usd: 0.01,
    });

    let e1 = rx1.recv().await.unwrap();
    let e2 = rx2.recv().await.unwrap();
    assert_eq!(e1.event_type(), "cost_incurred");
    assert_eq!(e2.event_type(), "cost_incurred");
}

#[test]
fn activity_log_insert_and_query() {
    let log = ActivityLog::open_in_memory().unwrap();

    let event = ActivityEvent::CostIncurred {
        agent_id: "agent-1".into(),
        model: "claude-sonnet-4-20250514".into(),
        input_tokens: 1000,
        output_tokens: 500,
        cost_usd: 0.015,
    };
    log.record(&event).unwrap();

    let entries = log.query(&ActivityFilter::default()).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].event_type, "cost_incurred");
    assert!((entries[0].cost_usd - 0.015).abs() < 0.001);
    assert!(entries[0].summary.contains("agent-1"));
}

#[test]
fn activity_log_filter_by_category() {
    let log = ActivityLog::open_in_memory().unwrap();

    log.record(&ActivityEvent::AgentStarted {
        agent_id: "a".into(),
        role: "Coder".into(),
        task_id: None,
    })
    .unwrap();
    log.record(&ActivityEvent::CostIncurred {
        agent_id: "a".into(),
        model: "m".into(),
        input_tokens: 100,
        output_tokens: 50,
        cost_usd: 0.01,
    })
    .unwrap();

    let filter = ActivityFilter {
        categories: Some(vec!["cost".into()]),
        ..Default::default()
    };
    let entries = log.query(&filter).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].category, "cost");
}

#[test]
fn activity_log_cost_summary() {
    let log = ActivityLog::open_in_memory().unwrap();

    for i in 0..5 {
        log.record(&ActivityEvent::CostIncurred {
            agent_id: if i < 3 { "a".into() } else { "b".into() },
            model: "claude-sonnet-4-20250514".into(),
            input_tokens: 1000,
            output_tokens: 500,
            cost_usd: 1.0,
        })
        .unwrap();
    }

    let summary = log
        .cost_summary(None, chrono::Utc::now() - chrono::Duration::hours(1))
        .unwrap();
    assert!((summary.total_usd - 5.0).abs() < 0.01);
    assert_eq!(summary.request_count, 5);
    assert_eq!(summary.by_agent.len(), 2);
}

#[tokio::test]
async fn activity_service_with_log_persists_events() {
    let log = std::sync::Arc::new(ActivityLog::open_in_memory().unwrap());
    let service = ActivityService::new_with_log(log.clone());

    service.emit(ActivityEvent::AgentStarted {
        agent_id: "test".into(),
        role: "Coder".into(),
        task_id: None,
    });

    // Give the listener task a moment to process
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let entries = log.query(&ActivityFilter::default()).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].event_type, "agent_started");
}
