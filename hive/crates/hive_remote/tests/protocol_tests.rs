use chrono::Utc;
use hive_remote::protocol::{
    AgentRunSummary, AgentsPanelData, AssistantApprovalData, AssistantBriefingData,
    AssistantEmailGroupData, AssistantEmailPreviewData, AssistantEventData, AssistantPanelData,
    AssistantRecentActionData, AssistantReminderData, ChannelsPanelData, ChatPanelData,
    ChannelDetailData, ChannelMessageData, ChannelSummaryData, ConversationSummaryData,
    DaemonEvent, DestinationPanels, FilesPanelData, GitOpsPanelData, HandoffPanelData,
    HelpLinkData, HelpPanelData, HistoryPanelData, HomePanelData, LaunchPanelData,
    ModelsPanelData, ModelOption, NetworkPanelData, NetworkPeerData, ObservePanelData,
    ObserveRuntimeData, ObserveSafetyData, ObserveSpendData, ObserveView, PanelMeta,
    PanelPayload, PanelRegistry, PanelResponse, ProviderCredentialData, RoutingPanelData, SessionSnapshot,
    SettingsPanelData, ShellDestination, SkillSummaryData, SkillsPanelData, SpecsPanelData,
    TerminalPanelData, WorkflowRunData, WorkflowsPanelData, WorkflowSummaryData, WorkspaceSummary,
};

fn sample_registry() -> PanelRegistry {
    PanelRegistry {
        destinations: vec![DestinationPanels {
            destination: ShellDestination::Home,
            panels: vec![PanelMeta {
                id: "home".into(),
                label: "Home".into(),
                description: "Mission control".into(),
                destination: Some(ShellDestination::Home),
                supported: true,
                utility: false,
            }],
        }],
        utility_panels: vec![PanelMeta {
            id: "settings".into(),
            label: "Settings".into(),
            description: "Configuration".into(),
            destination: None,
            supported: false,
            utility: true,
        }],
    }
}

fn sample_workspace() -> WorkspaceSummary {
    WorkspaceSummary {
        name: "AIrglowStudio".into(),
        path: "H:/WORK/AG/AIrglowStudio".into(),
        is_current: true,
        is_pinned: true,
    }
}

#[test]
fn daemon_event_roundtrip_for_shell_variants() {
    let events = vec![
        DaemonEvent::SwitchDestination {
            destination: ShellDestination::Observe,
        },
        DaemonEvent::SetObserveView {
            view: ObserveView::Safety,
        },
        DaemonEvent::LaunchHomeMission {
            template_id: "resume".into(),
            detail: "Pick up the last task".into(),
        },
        DaemonEvent::ApprovalDecision {
            request_id: "approval-1".into(),
            approved: true,
            reason: None,
        },
    ];

    for event in events {
        let json = serde_json::to_string(&event).unwrap();
        let parsed: DaemonEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(json, serde_json::to_string(&parsed).unwrap());
    }
}

#[test]
fn session_snapshot_serialization_includes_shell_state() {
    let snapshot = SessionSnapshot {
        active_conversation: Some("conv-1".into()),
        active_destination: ShellDestination::Build,
        active_panel: "chat".into(),
        current_workspace: sample_workspace(),
        current_model: "auto".into(),
        pending_approval_count: 2,
        is_streaming: true,
        observe_view: ObserveView::Inbox,
        panel_registry: sample_registry(),
        agent_runs: vec![AgentRunSummary {
            run_id: "run-1".into(),
            goal: "Fix the remote shell".into(),
            status: "running".into(),
            detail: "Writing the final cards".into(),
            cost_usd: 0.03,
            elapsed_ms: 1_250,
        }],
        timestamp: Utc::now(),
    };

    let json = serde_json::to_string(&snapshot).unwrap();
    let parsed: SessionSnapshot = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.active_destination, ShellDestination::Build);
    assert_eq!(parsed.active_panel, "chat");
    assert_eq!(parsed.current_workspace.name, "AIrglowStudio");
    assert_eq!(parsed.pending_approval_count, 2);
    assert!(parsed.is_streaming);
    assert_eq!(parsed.panel_registry.utility_panels.len(), 1);
}

#[test]
fn home_panel_payload_roundtrip() {
    let payload = PanelPayload::Home(HomePanelData {
        project_name: "Hive".into(),
        project_root: "H:/WORK/AG/AIrglowStudio".into(),
        project_summary: "Remote mission control".into(),
        current_model: "auto".into(),
        pending_approval_count: 1,
        launch_ready: true,
        launch_hint: "Describe the next mission".into(),
        last_launch_status: Some("Started from Home".into()),
        templates: vec![],
        priorities: vec![],
        status_cards: vec![],
        next_steps: vec![],
        saved_workspaces: vec![sample_workspace()],
    });

    let json = serde_json::to_string(&payload).unwrap();
    let parsed: PanelPayload = serde_json::from_str(&json).unwrap();
    match parsed {
        PanelPayload::Home(data) => {
            assert_eq!(data.project_name, "Hive");
            assert_eq!(data.saved_workspaces.len(), 1);
        }
        other => panic!("Expected home payload, got {other:?}"),
    }
}

#[test]
fn observe_panel_payload_roundtrip() {
    let payload = PanelPayload::Observe(ObservePanelData {
        current_view: ObserveView::Runtime,
        inbox: vec![],
        approvals: vec![],
        runtime: ObserveRuntimeData {
            status_label: "Healthy".into(),
            active_agents: 2,
            active_streams: 1,
            online_providers: 1,
            total_providers: 3,
            request_queue_length: 0,
            current_run_id: Some("run-1".into()),
            agents: vec![],
            recent_runs: vec![],
        },
        spend: ObserveSpendData {
            total_cost_usd: 0.14,
            today_cost_usd: 0.06,
            quality_score: 0.87,
            quality_trend: "Up".into(),
            cost_efficiency: 0.79,
            best_model: Some("auto".into()),
            worst_model: None,
            weak_areas: vec!["spec quality".into()],
        },
        safety: ObserveSafetyData {
            shield_enabled: true,
            pii_detections: 1,
            secrets_blocked: 2,
            threats_caught: 3,
            recent_events: vec![],
        },
    });

    let json = serde_json::to_string(&payload).unwrap();
    let parsed: PanelPayload = serde_json::from_str(&json).unwrap();
    match parsed {
        PanelPayload::Observe(data) => {
            assert_eq!(data.current_view, ObserveView::Runtime);
            assert_eq!(data.runtime.active_agents, 2);
            assert_eq!(data.spend.weak_areas, vec!["spec quality"]);
        }
        other => panic!("Expected observe payload, got {other:?}"),
    }
}

#[test]
fn chat_panel_and_handoff_payloads_roundtrip() {
    let chat = PanelPayload::Chat(ChatPanelData {
        conversation_id: Some("conv-1".into()),
        current_model: "auto".into(),
        is_streaming: false,
        total_cost: 0.02,
        messages: vec![],
        conversations: vec![ConversationSummaryData {
            id: "conv-1".into(),
            title: "Remote parity".into(),
            preview: "Finish the shell".into(),
            message_count: 3,
            total_cost: 0.02,
            model: "auto".into(),
            updated_at: "2026-03-19T12:00:00Z".into(),
        }],
        available_models: vec![ModelOption {
            id: "auto".into(),
            label: "Auto".into(),
        }],
        pending_approvals: vec![],
    });
    let handoff = PanelPayload::Handoff(HandoffPanelData {
        panel: "files".into(),
        title: "Files".into(),
        description: "Desktop later".into(),
        action_label: "Open desktop app".into(),
    });

    for payload in [chat, handoff] {
        let json = serde_json::to_string(&payload).unwrap();
        let parsed: PanelPayload = serde_json::from_str(&json).unwrap();
        assert_eq!(json, serde_json::to_string(&parsed).unwrap());
    }
}

#[test]
fn build_core_payloads_roundtrip() {
    let payloads = vec![
        PanelPayload::History(HistoryPanelData {
            active_conversation: Some("conv-1".into()),
            conversations: vec![],
        }),
        PanelPayload::Files(FilesPanelData {
            workspace_root: "H:/WORK/AG/AIrglowStudio".into(),
            current_path: "H:/WORK/AG/AIrglowStudio".into(),
            breadcrumbs: vec![],
            entries: vec![],
            preview: None,
            preview_error: None,
        }),
        PanelPayload::Specs(SpecsPanelData {
            workspace_root: "H:/WORK/AG/AIrglowStudio".into(),
            selected_spec_id: None,
            specs: vec![],
            selected_spec: None,
        }),
        PanelPayload::Agents(AgentsPanelData {
            current_model: "auto".into(),
            active_runs: vec![],
            recent_runs: vec![],
            pending_approvals: vec![],
            orchestration_modes: vec![],
        }),
        PanelPayload::GitOps(GitOpsPanelData {
            repo_path: "H:/WORK/AG/AIrglowStudio".into(),
            is_repo: true,
            branch: Some("main".into()),
            dirty_count: 0,
            files: vec![],
            commits: vec![],
            diff: String::new(),
            can_commit: true,
            error: None,
        }),
        PanelPayload::Terminal(TerminalPanelData {
            cwd: "H:/WORK/AG/AIrglowStudio".into(),
            is_running: true,
            last_exit_code: None,
            lines: vec![],
        }),
    ];

    for payload in payloads {
        let json = serde_json::to_string(&payload).unwrap();
        let parsed: PanelPayload = serde_json::from_str(&json).unwrap();
        assert_eq!(json, serde_json::to_string(&parsed).unwrap());
    }
}

#[test]
fn automate_and_assist_payloads_roundtrip() {
    let payloads = vec![
        PanelPayload::Workflows(WorkflowsPanelData {
            workspace_root: "H:/WORK/AG/AIrglowStudio".into(),
            source_dir: "H:/WORK/AG/AIrglowStudio/.hive/workflows".into(),
            workflows: vec![WorkflowSummaryData {
                id: "builtin:hive-dogfood-v1".into(),
                name: "Local Build Check".into(),
                description: "Built-in workflow".into(),
                status: "active".into(),
                trigger: "manual".into(),
                step_count: 4,
                run_count: 1,
                last_run: Some("2026-03-20T12:00:00Z".into()),
                is_builtin: true,
            }],
            active_runs: vec![WorkflowRunData {
                run_id: "workflow-run-1".into(),
                workflow_id: "builtin:hive-dogfood-v1".into(),
                workflow_name: "Local Build Check".into(),
                status: "running".into(),
                started_at: "2026-03-20T12:01:00Z".into(),
                completed_at: None,
                steps_completed: 1,
                error: None,
            }],
            recent_runs: vec![],
        }),
        PanelPayload::Channels(ChannelsPanelData {
            current_model: "auto".into(),
            selected_channel_id: Some("general".into()),
            channels: vec![ChannelSummaryData {
                id: "general".into(),
                name: "General".into(),
                icon: "#".into(),
                description: "Shared workspace coordination".into(),
                assigned_agents: vec!["Coordinator".into()],
                message_count: 1,
                updated_at: "2026-03-20T12:02:00Z".into(),
            }],
            selected_channel: Some(ChannelDetailData {
                id: "general".into(),
                name: "General".into(),
                icon: "#".into(),
                description: "Shared workspace coordination".into(),
                assigned_agents: vec!["Coordinator".into()],
                pinned_files: vec!["README.md".into()],
                messages: vec![ChannelMessageData {
                    id: "msg-1".into(),
                    author_type: "user".into(),
                    author_label: "You".into(),
                    content: "Remote channel check".into(),
                    timestamp: "2026-03-20T12:02:00Z".into(),
                    model: None,
                    cost: None,
                }],
            }),
        }),
        PanelPayload::Network(NetworkPanelData {
            available: true,
            our_peer_id: "peer-self".into(),
            connected_count: 1,
            total_count: 2,
            peers: vec![NetworkPeerData {
                name: "Laptop".into(),
                status: "Connected".into(),
                address: "/ip4/127.0.0.1/tcp/9000".into(),
                latency_ms: Some(18),
                last_seen: "Just now".into(),
            }],
            note: None,
        }),
        PanelPayload::Assistant(AssistantPanelData {
            connected_account_count: 2,
            briefing: Some(AssistantBriefingData {
                greeting: "Good morning!".into(),
                date: "2026-03-20".into(),
                event_count: 2,
                unread_emails: 4,
                active_reminders: 1,
                top_priority: Some("Reply to vendor".into()),
            }),
            events: vec![AssistantEventData {
                title: "Product sync".into(),
                time: "09:30".into(),
                location: Some("Remote".into()),
                is_conflict: false,
            }],
            email_groups: vec![AssistantEmailGroupData {
                provider: "Gmail".into(),
                previews: vec![AssistantEmailPreviewData {
                    from: "Gmail".into(),
                    subject: "Inbox summary".into(),
                    snippet: "Four unread messages".into(),
                    time: "2026-03-20T08:00:00Z".into(),
                    important: false,
                }],
            }],
            reminders: vec![AssistantReminderData {
                title: "Ship remote parity".into(),
                due: "2026-03-20 14:00".into(),
                is_overdue: false,
            }],
            approvals: vec![AssistantApprovalData {
                id: "approval-1".into(),
                action: "send_email".into(),
                resource: "customer@example.com".into(),
                level: "High".into(),
                requested_by: "assistant".into(),
                created_at: "2026-03-20T08:30:00Z".into(),
            }],
            recent_actions: vec![AssistantRecentActionData {
                description: "Prepared the daily briefing".into(),
                timestamp: "2026-03-20T08:31:00Z".into(),
                action_type: "briefing".into(),
            }],
        }),
    ];

    for payload in payloads {
        let json = serde_json::to_string(&payload).unwrap();
        let parsed: PanelPayload = serde_json::from_str(&json).unwrap();
        assert_eq!(json, serde_json::to_string(&parsed).unwrap());
    }
}

#[test]
fn utility_payloads_roundtrip() {
    let payloads = vec![
        PanelPayload::Settings(SettingsPanelData {
            current_workspace: "H:/WORK/AG/AIrglowStudio".into(),
            theme: "hive-light".into(),
            privacy_mode: false,
            shield_enabled: true,
            notifications_enabled: true,
            auto_update: true,
            remote_enabled: true,
            remote_auto_start: true,
            remote_local_port: 9480,
            remote_web_port: 9481,
            ollama_url: "http://127.0.0.1:11434".into(),
            lmstudio_url: "http://127.0.0.1:1234".into(),
            litellm_url: Some("https://litellm.example/v1".into()),
            local_provider_url: Some("https://models.example/v1".into()),
            connected_account_count: 2,
        }),
        PanelPayload::Models(ModelsPanelData {
            current_model: "auto".into(),
            default_model: "gpt-4o-mini".into(),
            auto_routing: true,
            project_models: vec!["claude-sonnet-4-20250514".into()],
            available_models: vec![ModelOption {
                id: "auto".into(),
                label: "Auto".into(),
            }],
            available_providers: vec!["Openai".into()],
            configured_providers: vec!["Openai".into(), "Ollama".into()],
            provider_credentials: vec![ProviderCredentialData {
                id: "openai".into(),
                label: "OpenAI".into(),
                has_key: true,
            }],
        }),
        PanelPayload::Routing(RoutingPanelData {
            auto_routing: true,
            default_model: "gpt-4o-mini".into(),
            strategy_summary: "Remote routing summary".into(),
            project_models: vec!["claude-sonnet-4-20250514".into()],
            available_providers: vec!["Openai".into()],
            notes: vec!["Current remote model: auto".into()],
        }),
        PanelPayload::Skills(SkillsPanelData {
            skills_dir: "H:/.hive/skills".into(),
            total_skills: 3,
            enabled_skills: 2,
            builtin_skills: 1,
            community_skills: 1,
            custom_skills: 1,
            skills: vec![SkillSummaryData {
                name: "code-review".into(),
                description: "Review code".into(),
                source: "BuiltIn".into(),
                enabled: true,
            }],
        }),
        PanelPayload::Launch(LaunchPanelData {
            remote_enabled: true,
            remote_auto_start: true,
            local_api_port: 9480,
            web_port: 9481,
            local_api_url: "http://127.0.0.1:9480".into(),
            web_url: "http://127.0.0.1:9481".into(),
            cloud_api_url: None,
            cloud_relay_url: None,
            cloud_tier: Some("Pro".into()),
        }),
        PanelPayload::Help(HelpPanelData {
            version: "0.3.30".into(),
            docs: vec![HelpLinkData {
                title: "Home".into(),
                detail: "Use Home to launch work.".into(),
            }],
            quick_tips: vec!["Tip".into()],
            troubleshooting: vec!["Check provider config".into()],
        }),
    ];

    for payload in payloads {
        let json = serde_json::to_string(&payload).unwrap();
        let parsed: PanelPayload = serde_json::from_str(&json).unwrap();
        assert_eq!(json, serde_json::to_string(&parsed).unwrap());
    }
}

#[test]
fn panel_response_roundtrip() {
    let response = PanelResponse {
        panel: "home".into(),
        data: PanelPayload::Handoff(HandoffPanelData {
            panel: "history".into(),
            title: "History".into(),
            description: "Remote handoff".into(),
            action_label: "Open desktop".into(),
        }),
    };

    let json = serde_json::to_string(&response).unwrap();
    let parsed: PanelResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.panel, "home");
    assert!(matches!(parsed.data, PanelPayload::Handoff(_)));
}
