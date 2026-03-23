use hive_remote::daemon::{DaemonConfig, HiveDaemon, PendingAction};
use hive_remote::protocol::{ObserveView, PanelPayload, ShellDestination};
use tempfile::tempdir;

fn make_daemon() -> HiveDaemon {
    let dir = tempdir().unwrap();
    let data_dir = dir.path().to_path_buf();
    std::mem::forget(dir);
    let config = DaemonConfig {
        config_root: Some(data_dir.join("config")),
        data_dir,
        ..DaemonConfig::default()
    };
    HiveDaemon::new(config).unwrap()
}

#[test]
fn daemon_starts_in_home_destination() {
    let daemon = make_daemon();
    let snapshot = daemon.get_snapshot();

    assert_eq!(snapshot.active_destination, ShellDestination::Home);
    assert_eq!(snapshot.active_panel, "home");
    assert_eq!(snapshot.observe_view, ObserveView::Inbox);
    assert!(snapshot.agent_runs.is_empty());
}

#[test]
fn switching_destination_picks_its_default_panel() {
    let mut daemon = make_daemon();

    daemon.switch_destination(ShellDestination::Observe);
    let snapshot = daemon.get_snapshot();
    assert_eq!(snapshot.active_destination, ShellDestination::Observe);
    assert_eq!(snapshot.active_panel, "observe");

    daemon.switch_destination(ShellDestination::Build);
    let snapshot = daemon.get_snapshot();
    assert_eq!(snapshot.active_panel, "chat");
}

#[test]
fn panel_response_returns_typed_panel_payloads() {
    let daemon = make_daemon();

    let home = daemon.panel_response("home").unwrap();
    assert_eq!(home.panel, "home");
    assert!(matches!(home.data, hive_remote::protocol::PanelPayload::Home(_)));

    let files = daemon.panel_response("files").unwrap();
    assert_eq!(files.panel, "files");
    assert!(matches!(
        files.data,
        hive_remote::protocol::PanelPayload::Files(_)
    ));

    let history = daemon.panel_response("history").unwrap();
    assert!(matches!(
        history.data,
        hive_remote::protocol::PanelPayload::History(_)
    ));

    let workflows = daemon.panel_response("workflows").unwrap();
    assert!(matches!(workflows.data, PanelPayload::Workflows(_)));

    let channels = daemon.panel_response("channels").unwrap();
    assert!(matches!(channels.data, PanelPayload::Channels(_)));

    let network = daemon.panel_response("network").unwrap();
    assert!(matches!(network.data, PanelPayload::Network(_)));

    let assistant = daemon.panel_response("assistant").unwrap();
    assert!(matches!(assistant.data, PanelPayload::Assistant(_)));

    let settings = daemon.panel_response("settings").unwrap();
    assert!(matches!(settings.data, PanelPayload::Settings(_)));

    let models = daemon.panel_response("models").unwrap();
    assert!(matches!(models.data, PanelPayload::Models(_)));

    let routing = daemon.panel_response("routing").unwrap();
    assert!(matches!(routing.data, PanelPayload::Routing(_)));

    let skills = daemon.panel_response("skills").unwrap();
    assert!(matches!(skills.data, PanelPayload::Skills(_)));

    let launch = daemon.panel_response("launch").unwrap();
    assert!(matches!(launch.data, PanelPayload::Launch(_)));

    let help = daemon.panel_response("help").unwrap();
    assert!(matches!(help.data, PanelPayload::Help(_)));
}

#[test]
fn launch_home_mission_moves_into_chat_flow() {
    let mut daemon = make_daemon();

    let disposition = daemon
        .launch_home_mission("resume".into(), "Continue the last task".into())
        .unwrap();

    let snapshot = daemon.get_snapshot();
    assert_eq!(snapshot.active_destination, ShellDestination::Build);
    assert_eq!(snapshot.active_panel, "chat");
    assert!(snapshot.active_conversation.is_some());
    assert!(matches!(
        disposition,
        hive_remote::daemon::SendDisposition::Stream { .. }
            | hive_remote::daemon::SendDisposition::ApprovalPending { .. }
    ));
}

#[test]
fn risky_agent_task_requires_approval_and_returns_pending_action_on_approval() {
    let mut daemon = make_daemon();

    let disposition = daemon
        .start_agent_task("Deploy the latest release".into(), "coordinator".into())
        .unwrap();

    let request_id = match disposition {
        hive_remote::daemon::AgentDisposition::ApprovalPending { request_id, .. } => request_id,
        other => panic!("Expected approval pending, got {other:?}"),
    };

    assert_eq!(daemon.get_snapshot().pending_approval_count, 1);

    let pending = daemon
        .apply_approval_decision(&request_id, true, None)
        .unwrap()
        .expect("approved request should yield pending action");

    match pending {
        PendingAction::Agent {
            goal,
            orchestration_mode,
            ..
        } => {
            assert_eq!(goal, "Deploy the latest release");
            assert_eq!(orchestration_mode, "coordinator");
        }
        other => panic!("Expected agent pending action, got {other:?}"),
    }
}

#[test]
fn journal_replay_restores_last_shell_state() {
    let dir = tempdir().unwrap();
    let config = DaemonConfig {
        data_dir: dir.path().to_path_buf(),
        config_root: Some(dir.path().join("config")),
        ..DaemonConfig::default()
    };

    {
        let mut daemon = HiveDaemon::new(config.clone()).unwrap();
        daemon.switch_destination(ShellDestination::Observe);
        daemon.set_observe_view(ObserveView::Safety);
        daemon.switch_panel("monitor");
    }

    let mut replayed = HiveDaemon::new(config).unwrap();
    replayed.replay_journal().unwrap();
    let snapshot = replayed.get_snapshot();

    assert_eq!(snapshot.active_destination, ShellDestination::Observe);
    assert_eq!(snapshot.observe_view, ObserveView::Safety);
    assert_eq!(snapshot.active_panel, "monitor");
}

#[test]
fn channel_actions_update_selected_channel_and_messages() {
    let mut daemon = make_daemon();

    daemon.select_channel("general").unwrap();
    daemon
        .send_channel_message("general", "Remote channel message")
        .unwrap();

    let response = daemon.panel_response("channels").unwrap();
    let PanelPayload::Channels(data) = response.data else {
        panic!("expected channels payload");
    };

    assert_eq!(data.selected_channel_id.as_deref(), Some("general"));
    let selected = data.selected_channel.expect("selected channel");
    assert_eq!(selected.id, "general");
    assert!(
        selected
            .messages
            .iter()
            .any(|message| message.content == "Remote channel message")
    );
}

#[test]
fn workflows_panel_exposes_available_workflows() {
    let daemon = make_daemon();

    let response = daemon.panel_response("workflows").unwrap();
    let PanelPayload::Workflows(data) = response.data else {
        panic!("expected workflows payload");
    };

    assert!(!data.workflows.is_empty());
    assert!(data
        .workflows
        .iter()
        .any(|workflow| workflow.id == "builtin:hive-dogfood-v1"));
}
