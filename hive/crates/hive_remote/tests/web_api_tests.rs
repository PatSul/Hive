use hive_remote::daemon::{DaemonConfig, HiveDaemon};
use hive_remote::protocol::DaemonEvent;
use hive_remote::web_api::DaemonState;
use hive_remote::web_server::build_router;

use http::Request;
use http_body_util::BodyExt;
use std::sync::Arc;
use tempfile::tempdir;
use tokio::sync::RwLock;
use tower::ServiceExt;

/// Helper: create a DaemonState backed by a temp directory.
fn make_daemon() -> DaemonState {
    let dir = tempdir().unwrap();
    let config = DaemonConfig {
        data_dir: dir.path().to_path_buf(),
        ..DaemonConfig::default()
    };
    let daemon = HiveDaemon::new(config).unwrap();
    Arc::new(RwLock::new(daemon))
}

#[tokio::test]
async fn test_get_state_returns_snapshot() {
    let daemon = make_daemon();
    let app = build_router(daemon);

    let req = Request::builder()
        .uri("/api/state")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 200);

    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["active_panel"], "chat");
    assert!(json["agent_runs"].is_array());
    assert!(json["timestamp"].is_string());
}

#[tokio::test]
async fn test_post_chat_sends_message() {
    let daemon = make_daemon();
    let app = build_router(daemon.clone());

    let payload = serde_json::json!({
        "conversation_id": "conv-42",
        "content": "hello world",
        "model": "gpt-4"
    });

    let req = Request::builder()
        .uri("/api/chat")
        .method("POST")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(payload.to_string()))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 200);

    // Verify daemon state was updated
    let d = daemon.read().await;
    let snapshot = d.get_snapshot();
    assert_eq!(snapshot.active_conversation, Some("conv-42".into()));
}

#[tokio::test]
async fn test_get_panel_data() {
    let daemon = make_daemon();
    let app = build_router(daemon);

    let req = Request::builder()
        .uri("/api/panels/monitor")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 200);

    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["panel"], "monitor");
}

#[tokio::test]
async fn test_post_agent_action() {
    let daemon = make_daemon();
    let app = build_router(daemon.clone());

    let payload = serde_json::json!({
        "goal": "build auth system",
        "orchestration_mode": "hivemind"
    });

    let req = Request::builder()
        .uri("/api/agents")
        .method("POST")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(payload.to_string()))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 200);

    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "started");

    // Verify daemon state was updated
    let d = daemon.read().await;
    let snapshot = d.get_snapshot();
    assert_eq!(snapshot.agent_runs.len(), 1);
    assert_eq!(snapshot.agent_runs[0].goal, "build auth system");
}

#[tokio::test]
async fn test_websocket_receives_initial_snapshot() {
    use futures::stream::StreamExt;
    use tokio::net::TcpListener;

    let daemon = make_daemon();

    // Seed some state so the snapshot is non-trivial
    {
        let mut d = daemon.write().await;
        d.handle_event(DaemonEvent::SwitchPanel {
            panel: "agents".into(),
        })
        .await;
    }

    let app = build_router(daemon);

    // Bind to a random port
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    // Spawn the server
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    // Connect WebSocket client
    let url = format!("ws://{}/ws", addr);
    let (mut ws_stream, _) = tokio_tungstenite::connect_async(&url).await.unwrap();

    // First message should be the initial StateSnapshot
    let msg = ws_stream.next().await.unwrap().unwrap();
    let text = msg.into_text().unwrap();
    let event: DaemonEvent = serde_json::from_str(&text).unwrap();

    match event {
        DaemonEvent::StateSnapshot(snapshot) => {
            assert_eq!(snapshot.active_panel, "agents");
        }
        other => panic!("Expected StateSnapshot, got {:?}", other),
    }
}
