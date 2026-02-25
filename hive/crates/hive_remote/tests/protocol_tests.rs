use chrono::Utc;
use hive_remote::protocol::{AgentRunSummary, DaemonEvent, SessionSnapshot};

// ---------------------------------------------------------------------------
// 1. Serialization roundtrip for each variant of DaemonEvent
// ---------------------------------------------------------------------------

#[test]
fn roundtrip_send_message() {
    let event = DaemonEvent::SendMessage {
        conversation_id: "conv-1".into(),
        content: "Hello world".into(),
        model: "gpt-4".into(),
    };
    let json = serde_json::to_string(&event).unwrap();
    let back: DaemonEvent = serde_json::from_str(&json).unwrap();
    match back {
        DaemonEvent::SendMessage {
            conversation_id,
            content,
            model,
        } => {
            assert_eq!(conversation_id, "conv-1");
            assert_eq!(content, "Hello world");
            assert_eq!(model, "gpt-4");
        }
        other => panic!("Expected SendMessage, got {:?}", other),
    }
}

#[test]
fn roundtrip_switch_panel() {
    let event = DaemonEvent::SwitchPanel {
        panel: "settings".into(),
    };
    let json = serde_json::to_string(&event).unwrap();
    let back: DaemonEvent = serde_json::from_str(&json).unwrap();
    match back {
        DaemonEvent::SwitchPanel { panel } => assert_eq!(panel, "settings"),
        other => panic!("Expected SwitchPanel, got {:?}", other),
    }
}

#[test]
fn roundtrip_start_agent_task() {
    let event = DaemonEvent::StartAgentTask {
        goal: "fix bug #42".into(),
        orchestration_mode: "coordinator".into(),
    };
    let json = serde_json::to_string(&event).unwrap();
    let back: DaemonEvent = serde_json::from_str(&json).unwrap();
    match back {
        DaemonEvent::StartAgentTask {
            goal,
            orchestration_mode,
        } => {
            assert_eq!(goal, "fix bug #42");
            assert_eq!(orchestration_mode, "coordinator");
        }
        other => panic!("Expected StartAgentTask, got {:?}", other),
    }
}

#[test]
fn roundtrip_cancel_agent_task() {
    let event = DaemonEvent::CancelAgentTask {
        run_id: "run-abc".into(),
    };
    let json = serde_json::to_string(&event).unwrap();
    let back: DaemonEvent = serde_json::from_str(&json).unwrap();
    match back {
        DaemonEvent::CancelAgentTask { run_id } => assert_eq!(run_id, "run-abc"),
        other => panic!("Expected CancelAgentTask, got {:?}", other),
    }
}

#[test]
fn roundtrip_stream_chunk() {
    let event = DaemonEvent::StreamChunk {
        conversation_id: "conv-2".into(),
        chunk: "partial response".into(),
    };
    let json = serde_json::to_string(&event).unwrap();
    let back: DaemonEvent = serde_json::from_str(&json).unwrap();
    match back {
        DaemonEvent::StreamChunk {
            conversation_id,
            chunk,
        } => {
            assert_eq!(conversation_id, "conv-2");
            assert_eq!(chunk, "partial response");
        }
        other => panic!("Expected StreamChunk, got {:?}", other),
    }
}

#[test]
fn roundtrip_stream_complete() {
    let event = DaemonEvent::StreamComplete {
        conversation_id: "conv-2".into(),
        prompt_tokens: 100,
        completion_tokens: 50,
        cost_usd: Some(0.0025),
    };
    let json = serde_json::to_string(&event).unwrap();
    let back: DaemonEvent = serde_json::from_str(&json).unwrap();
    match back {
        DaemonEvent::StreamComplete {
            conversation_id,
            prompt_tokens,
            completion_tokens,
            cost_usd,
        } => {
            assert_eq!(conversation_id, "conv-2");
            assert_eq!(prompt_tokens, 100);
            assert_eq!(completion_tokens, 50);
            assert_eq!(cost_usd, Some(0.0025));
        }
        other => panic!("Expected StreamComplete, got {:?}", other),
    }
}

#[test]
fn roundtrip_stream_complete_no_cost() {
    let event = DaemonEvent::StreamComplete {
        conversation_id: "conv-3".into(),
        prompt_tokens: 10,
        completion_tokens: 5,
        cost_usd: None,
    };
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("\"cost_usd\":null"));
    let back: DaemonEvent = serde_json::from_str(&json).unwrap();
    match back {
        DaemonEvent::StreamComplete { cost_usd, .. } => assert_eq!(cost_usd, None),
        other => panic!("Expected StreamComplete, got {:?}", other),
    }
}

#[test]
fn roundtrip_agent_status() {
    let event = DaemonEvent::AgentStatus {
        run_id: "run-1".into(),
        status: "running".into(),
        detail: "Step 3 of 5".into(),
    };
    let json = serde_json::to_string(&event).unwrap();
    let back: DaemonEvent = serde_json::from_str(&json).unwrap();
    match back {
        DaemonEvent::AgentStatus {
            run_id,
            status,
            detail,
        } => {
            assert_eq!(run_id, "run-1");
            assert_eq!(status, "running");
            assert_eq!(detail, "Step 3 of 5");
        }
        other => panic!("Expected AgentStatus, got {:?}", other),
    }
}

#[test]
fn roundtrip_state_snapshot() {
    let snapshot = SessionSnapshot {
        active_conversation: Some("conv-1".into()),
        active_panel: "chat".into(),
        agent_runs: vec![],
        timestamp: Utc::now(),
    };
    let event = DaemonEvent::StateSnapshot(snapshot);
    let json = serde_json::to_string(&event).unwrap();
    let back: DaemonEvent = serde_json::from_str(&json).unwrap();
    match back {
        DaemonEvent::StateSnapshot(s) => {
            assert_eq!(s.active_conversation, Some("conv-1".into()));
            assert_eq!(s.active_panel, "chat");
            assert!(s.agent_runs.is_empty());
        }
        other => panic!("Expected StateSnapshot, got {:?}", other),
    }
}

#[test]
fn roundtrip_panel_data() {
    let data = serde_json::json!({"models": ["gpt-4", "claude-3"], "count": 2});
    let event = DaemonEvent::PanelData {
        panel: "models_browser".into(),
        data: data.clone(),
    };
    let json = serde_json::to_string(&event).unwrap();
    let back: DaemonEvent = serde_json::from_str(&json).unwrap();
    match back {
        DaemonEvent::PanelData { panel, data: d } => {
            assert_eq!(panel, "models_browser");
            assert_eq!(d, data);
        }
        other => panic!("Expected PanelData, got {:?}", other),
    }
}

#[test]
fn roundtrip_error() {
    let event = DaemonEvent::Error {
        code: 404,
        message: "Not found".into(),
    };
    let json = serde_json::to_string(&event).unwrap();
    let back: DaemonEvent = serde_json::from_str(&json).unwrap();
    match back {
        DaemonEvent::Error { code, message } => {
            assert_eq!(code, 404);
            assert_eq!(message, "Not found");
        }
        other => panic!("Expected Error, got {:?}", other),
    }
}

#[test]
fn roundtrip_ping() {
    let event = DaemonEvent::Ping;
    let json = serde_json::to_string(&event).unwrap();
    let back: DaemonEvent = serde_json::from_str(&json).unwrap();
    assert!(matches!(back, DaemonEvent::Ping));
}

#[test]
fn roundtrip_pong() {
    let event = DaemonEvent::Pong;
    let json = serde_json::to_string(&event).unwrap();
    let back: DaemonEvent = serde_json::from_str(&json).unwrap();
    assert!(matches!(back, DaemonEvent::Pong));
}

// ---------------------------------------------------------------------------
// 2. SessionSnapshot serialization
// ---------------------------------------------------------------------------

#[test]
fn session_snapshot_serialization() {
    let ts = Utc::now();
    let snapshot = SessionSnapshot {
        active_conversation: None,
        active_panel: "agents".into(),
        agent_runs: vec![AgentRunSummary {
            run_id: "run-x".into(),
            goal: "deploy app".into(),
            status: "completed".into(),
            cost_usd: 0.15,
            elapsed_ms: 45000,
        }],
        timestamp: ts,
    };
    let json = serde_json::to_string(&snapshot).unwrap();
    let back: SessionSnapshot = serde_json::from_str(&json).unwrap();
    assert_eq!(back.active_conversation, None);
    assert_eq!(back.active_panel, "agents");
    assert_eq!(back.agent_runs.len(), 1);
    assert_eq!(back.agent_runs[0].run_id, "run-x");
    assert_eq!(back.timestamp, ts);
}

#[test]
fn session_snapshot_with_no_agent_runs() {
    let snapshot = SessionSnapshot {
        active_conversation: Some("conv-z".into()),
        active_panel: "chat".into(),
        agent_runs: vec![],
        timestamp: Utc::now(),
    };
    let json = serde_json::to_string(&snapshot).unwrap();
    assert!(json.contains("\"agent_runs\":[]"));
    let back: SessionSnapshot = serde_json::from_str(&json).unwrap();
    assert!(back.agent_runs.is_empty());
}

// ---------------------------------------------------------------------------
// 3. AgentRunSummary serialization
// ---------------------------------------------------------------------------

#[test]
fn agent_run_summary_serialization() {
    let summary = AgentRunSummary {
        run_id: "run-42".into(),
        goal: "refactor module".into(),
        status: "in_progress".into(),
        cost_usd: 0.032,
        elapsed_ms: 12345,
    };
    let json = serde_json::to_string(&summary).unwrap();
    let back: AgentRunSummary = serde_json::from_str(&json).unwrap();
    assert_eq!(back.run_id, "run-42");
    assert_eq!(back.goal, "refactor module");
    assert_eq!(back.status, "in_progress");
    assert!((back.cost_usd - 0.032).abs() < f64::EPSILON);
    assert_eq!(back.elapsed_ms, 12345);
}

#[test]
fn agent_run_summary_zero_cost() {
    let summary = AgentRunSummary {
        run_id: "run-0".into(),
        goal: "test".into(),
        status: "pending".into(),
        cost_usd: 0.0,
        elapsed_ms: 0,
    };
    let json = serde_json::to_string(&summary).unwrap();
    let back: AgentRunSummary = serde_json::from_str(&json).unwrap();
    assert_eq!(back.cost_usd, 0.0);
    assert_eq!(back.elapsed_ms, 0);
}

// ---------------------------------------------------------------------------
// 4. All variants serialize with correct "type" tag
// ---------------------------------------------------------------------------

#[test]
fn all_variants_have_correct_type_tag() {
    let variants: Vec<(&str, DaemonEvent)> = vec![
        (
            "send_message",
            DaemonEvent::SendMessage {
                conversation_id: "c".into(),
                content: "x".into(),
                model: "m".into(),
            },
        ),
        (
            "switch_panel",
            DaemonEvent::SwitchPanel {
                panel: "p".into(),
            },
        ),
        (
            "start_agent_task",
            DaemonEvent::StartAgentTask {
                goal: "g".into(),
                orchestration_mode: "o".into(),
            },
        ),
        (
            "cancel_agent_task",
            DaemonEvent::CancelAgentTask {
                run_id: "r".into(),
            },
        ),
        (
            "stream_chunk",
            DaemonEvent::StreamChunk {
                conversation_id: "c".into(),
                chunk: "ch".into(),
            },
        ),
        (
            "stream_complete",
            DaemonEvent::StreamComplete {
                conversation_id: "c".into(),
                prompt_tokens: 1,
                completion_tokens: 1,
                cost_usd: None,
            },
        ),
        (
            "agent_status",
            DaemonEvent::AgentStatus {
                run_id: "r".into(),
                status: "s".into(),
                detail: "d".into(),
            },
        ),
        (
            "state_snapshot",
            DaemonEvent::StateSnapshot(SessionSnapshot {
                active_conversation: None,
                active_panel: "chat".into(),
                agent_runs: vec![],
                timestamp: Utc::now(),
            }),
        ),
        (
            "panel_data",
            DaemonEvent::PanelData {
                panel: "p".into(),
                data: serde_json::Value::Null,
            },
        ),
        (
            "error",
            DaemonEvent::Error {
                code: 500,
                message: "err".into(),
            },
        ),
        ("ping", DaemonEvent::Ping),
        ("pong", DaemonEvent::Pong),
    ];

    for (expected_tag, event) in &variants {
        let json = serde_json::to_string(event).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        let actual_tag = parsed["type"].as_str().unwrap_or("MISSING");
        assert_eq!(
            actual_tag, *expected_tag,
            "Variant {:?} should have type tag '{}', got '{}'",
            event, expected_tag, actual_tag
        );
    }
}

#[test]
fn all_variants_roundtrip_from_json_string() {
    // Ensure every variant can survive a full JSON roundtrip via string
    let variants: Vec<DaemonEvent> = vec![
        DaemonEvent::SendMessage {
            conversation_id: "c1".into(),
            content: "hello".into(),
            model: "claude-3".into(),
        },
        DaemonEvent::SwitchPanel {
            panel: "chat".into(),
        },
        DaemonEvent::StartAgentTask {
            goal: "build feature".into(),
            orchestration_mode: "hivemind".into(),
        },
        DaemonEvent::CancelAgentTask {
            run_id: "run-99".into(),
        },
        DaemonEvent::StreamChunk {
            conversation_id: "c2".into(),
            chunk: "data".into(),
        },
        DaemonEvent::StreamComplete {
            conversation_id: "c2".into(),
            prompt_tokens: 500,
            completion_tokens: 200,
            cost_usd: Some(0.01),
        },
        DaemonEvent::AgentStatus {
            run_id: "run-1".into(),
            status: "done".into(),
            detail: "finished".into(),
        },
        DaemonEvent::StateSnapshot(SessionSnapshot {
            active_conversation: Some("conv".into()),
            active_panel: "files".into(),
            agent_runs: vec![AgentRunSummary {
                run_id: "r1".into(),
                goal: "test".into(),
                status: "ok".into(),
                cost_usd: 0.0,
                elapsed_ms: 100,
            }],
            timestamp: Utc::now(),
        }),
        DaemonEvent::PanelData {
            panel: "settings".into(),
            data: serde_json::json!({"key": "value"}),
        },
        DaemonEvent::Error {
            code: 403,
            message: "Forbidden".into(),
        },
        DaemonEvent::Ping,
        DaemonEvent::Pong,
    ];

    for event in &variants {
        let json = serde_json::to_string(event).unwrap();
        let back: DaemonEvent = serde_json::from_str(&json).expect(
            &format!("Failed to deserialize variant: {}", json),
        );
        // Verify roundtrip by re-serializing and comparing JSON
        let json2 = serde_json::to_string(&back).unwrap();
        assert_eq!(json, json2, "Roundtrip mismatch for {:?}", event);
    }
}

// ---------------------------------------------------------------------------
// 5. Edge cases
// ---------------------------------------------------------------------------

#[test]
fn deserialize_unknown_type_tag_fails() {
    let json = r#"{"type":"unknown_variant","data":123}"#;
    let result = serde_json::from_str::<DaemonEvent>(json);
    assert!(result.is_err(), "Unknown type tag should fail to deserialize");
}

#[test]
fn send_message_with_unicode_content() {
    let event = DaemonEvent::SendMessage {
        conversation_id: "conv-u".into(),
        content: "Hello \u{1F600} \u{4F60}\u{597D}".into(),
        model: "gpt-4".into(),
    };
    let json = serde_json::to_string(&event).unwrap();
    let back: DaemonEvent = serde_json::from_str(&json).unwrap();
    match back {
        DaemonEvent::SendMessage { content, .. } => {
            assert!(content.contains('\u{1F600}'));
            assert!(content.contains("\u{4F60}\u{597D}"));
        }
        other => panic!("Expected SendMessage, got {:?}", other),
    }
}

#[test]
fn panel_data_with_nested_json() {
    let nested = serde_json::json!({
        "level1": {
            "level2": {
                "level3": [1, 2, 3]
            }
        }
    });
    let event = DaemonEvent::PanelData {
        panel: "deep".into(),
        data: nested.clone(),
    };
    let json = serde_json::to_string(&event).unwrap();
    let back: DaemonEvent = serde_json::from_str(&json).unwrap();
    match back {
        DaemonEvent::PanelData { data, .. } => assert_eq!(data, nested),
        other => panic!("Expected PanelData, got {:?}", other),
    }
}

#[test]
fn session_snapshot_with_multiple_agent_runs() {
    let snapshot = SessionSnapshot {
        active_conversation: Some("conv-multi".into()),
        active_panel: "agents".into(),
        agent_runs: vec![
            AgentRunSummary {
                run_id: "run-a".into(),
                goal: "task a".into(),
                status: "completed".into(),
                cost_usd: 0.05,
                elapsed_ms: 10000,
            },
            AgentRunSummary {
                run_id: "run-b".into(),
                goal: "task b".into(),
                status: "running".into(),
                cost_usd: 0.02,
                elapsed_ms: 5000,
            },
            AgentRunSummary {
                run_id: "run-c".into(),
                goal: "task c".into(),
                status: "failed".into(),
                cost_usd: 0.01,
                elapsed_ms: 2000,
            },
        ],
        timestamp: Utc::now(),
    };
    let json = serde_json::to_string(&snapshot).unwrap();
    let back: SessionSnapshot = serde_json::from_str(&json).unwrap();
    assert_eq!(back.agent_runs.len(), 3);
    assert_eq!(back.agent_runs[0].run_id, "run-a");
    assert_eq!(back.agent_runs[1].status, "running");
    assert_eq!(back.agent_runs[2].goal, "task c");
}
