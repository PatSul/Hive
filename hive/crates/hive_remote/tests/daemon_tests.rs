use hive_remote::daemon::{DaemonConfig, HiveDaemon};
use hive_remote::protocol::DaemonEvent;
use tempfile::tempdir;

#[tokio::test]
async fn test_daemon_config_defaults() {
    let config = DaemonConfig::default();
    assert_eq!(config.local_port, 9480);
    assert_eq!(config.web_port, 9481);
    assert_eq!(config.shutdown_grace_secs, 30);
}

#[tokio::test]
async fn test_daemon_initial_snapshot() {
    let dir = tempdir().unwrap();
    let config = DaemonConfig {
        data_dir: dir.path().to_path_buf(),
        ..DaemonConfig::default()
    };
    let daemon = HiveDaemon::new(config).unwrap();
    let snapshot = daemon.get_snapshot();
    assert_eq!(snapshot.active_panel, "chat");
    assert!(snapshot.agent_runs.is_empty());
    assert!(snapshot.active_conversation.is_none());
}

#[tokio::test]
async fn test_daemon_handles_switch_panel() {
    let dir = tempdir().unwrap();
    let config = DaemonConfig {
        data_dir: dir.path().to_path_buf(),
        ..DaemonConfig::default()
    };
    let mut daemon = HiveDaemon::new(config).unwrap();
    daemon
        .handle_event(DaemonEvent::SwitchPanel {
            panel: "agents".into(),
        })
        .await;
    let snapshot = daemon.get_snapshot();
    assert_eq!(snapshot.active_panel, "agents");
}

#[tokio::test]
async fn test_daemon_handles_send_message() {
    let dir = tempdir().unwrap();
    let config = DaemonConfig {
        data_dir: dir.path().to_path_buf(),
        ..DaemonConfig::default()
    };
    let mut daemon = HiveDaemon::new(config).unwrap();
    daemon
        .handle_event(DaemonEvent::SendMessage {
            conversation_id: "conv-1".into(),
            content: "hello".into(),
            model: "test".into(),
        })
        .await;
    let snapshot = daemon.get_snapshot();
    assert_eq!(snapshot.active_conversation, Some("conv-1".into()));
}

#[tokio::test]
async fn test_daemon_handles_agent_task() {
    let dir = tempdir().unwrap();
    let config = DaemonConfig {
        data_dir: dir.path().to_path_buf(),
        ..DaemonConfig::default()
    };
    let mut daemon = HiveDaemon::new(config).unwrap();
    daemon
        .handle_event(DaemonEvent::StartAgentTask {
            goal: "build auth".into(),
            orchestration_mode: "hivemind".into(),
        })
        .await;
    let snapshot = daemon.get_snapshot();
    assert_eq!(snapshot.agent_runs.len(), 1);
    assert_eq!(snapshot.agent_runs[0].goal, "build auth");
    assert_eq!(snapshot.agent_runs[0].status, "planning");
}

#[tokio::test]
async fn test_daemon_handles_cancel_agent_task() {
    let dir = tempdir().unwrap();
    let config = DaemonConfig {
        data_dir: dir.path().to_path_buf(),
        ..DaemonConfig::default()
    };
    let mut daemon = HiveDaemon::new(config).unwrap();

    // Start a task first
    daemon
        .handle_event(DaemonEvent::StartAgentTask {
            goal: "build auth".into(),
            orchestration_mode: "hivemind".into(),
        })
        .await;

    let run_id = daemon.get_snapshot().agent_runs[0].run_id.clone();

    // Cancel it
    daemon
        .handle_event(DaemonEvent::CancelAgentTask {
            run_id: run_id.clone(),
        })
        .await;

    let snapshot = daemon.get_snapshot();
    assert_eq!(snapshot.agent_runs[0].status, "cancelled");
}

#[tokio::test]
async fn test_daemon_journal_replay() {
    let dir = tempdir().unwrap();
    let config = DaemonConfig {
        data_dir: dir.path().to_path_buf(),
        ..DaemonConfig::default()
    };

    // First session
    {
        let mut daemon = HiveDaemon::new(config.clone()).unwrap();
        daemon
            .handle_event(DaemonEvent::SwitchPanel {
                panel: "agents".into(),
            })
            .await;
        daemon
            .handle_event(DaemonEvent::SwitchPanel {
                panel: "monitor".into(),
            })
            .await;
    }

    // Second session -- replay
    {
        let mut daemon = HiveDaemon::new(config).unwrap();
        daemon.replay_journal().await.unwrap();
        let snapshot = daemon.get_snapshot();
        assert_eq!(snapshot.active_panel, "monitor");
    }
}

#[tokio::test]
async fn test_daemon_broadcast_events() {
    let dir = tempdir().unwrap();
    let config = DaemonConfig {
        data_dir: dir.path().to_path_buf(),
        ..DaemonConfig::default()
    };
    let mut daemon = HiveDaemon::new(config).unwrap();

    let mut rx = daemon.subscribe();

    daemon.handle_event(DaemonEvent::Ping).await;

    let received = rx.try_recv();
    assert!(received.is_ok());
}
