use hive_remote::daemon::{DaemonConfig, HiveDaemon};
use hive_remote::web_api::DaemonState;
use hive_remote::web_server::build_router;
use http::Request;
use http_body_util::BodyExt;
use std::sync::Arc;
use tempfile::tempdir;
use tokio::sync::RwLock;
use tower::ServiceExt;

fn make_daemon() -> DaemonState {
    let dir = tempdir().unwrap();
    let data_dir = dir.path().to_path_buf();
    std::mem::forget(dir);
    let config = DaemonConfig {
        config_root: Some(data_dir.join("config")),
        data_dir,
        ..DaemonConfig::default()
    };
    Arc::new(RwLock::new(HiveDaemon::new(config).unwrap()))
}

#[tokio::test]
async fn get_state_returns_shell_snapshot() {
    let app = build_router(make_daemon());

    let request = Request::builder()
        .uri("/api/state")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["active_destination"], "home");
    assert_eq!(json["active_panel"], "home");
    assert_eq!(json["observe_view"], "inbox");
    assert!(json["panel_registry"]["destinations"].is_array());
}

#[tokio::test]
async fn get_panel_home_returns_typed_payload() {
    let app = build_router(make_daemon());

    let request = Request::builder()
        .uri("/api/panels/home")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["panel"], "home");
    assert_eq!(json["data"]["kind"], "home");
}

#[tokio::test]
async fn build_core_panels_return_typed_payloads() {
    let app = build_router(make_daemon());

    for panel in ["history", "files", "specs", "agents", "git_ops", "terminal"] {
        let request = Request::builder()
            .uri(format!("/api/panels/{panel}"))
            .method("GET")
            .body(axum::body::Body::empty())
            .unwrap();

        let response = app.clone().oneshot(request).await.unwrap();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["panel"], panel);
        assert_eq!(json["data"]["kind"], panel);
    }
}

#[tokio::test]
async fn automate_and_assist_panels_return_typed_payloads() {
    let app = build_router(make_daemon());

    for panel in ["workflows", "channels", "network", "assistant"] {
        let request = Request::builder()
            .uri(format!("/api/panels/{panel}"))
            .method("GET")
            .body(axum::body::Body::empty())
            .unwrap();

        let response = app.clone().oneshot(request).await.unwrap();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["panel"], panel);
        assert_eq!(json["data"]["kind"], panel);
    }
}

#[tokio::test]
async fn utility_panels_return_typed_payloads() {
    let app = build_router(make_daemon());

    for panel in ["settings", "models", "routing", "skills", "launch", "help"] {
        let request = Request::builder()
            .uri(format!("/api/panels/{panel}"))
            .method("GET")
            .body(axum::body::Body::empty())
            .unwrap();

        let response = app.clone().oneshot(request).await.unwrap();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["panel"], panel);
        assert_eq!(json["data"]["kind"], panel);
    }
}

#[tokio::test]
async fn post_navigation_switches_destination_and_panel() {
    let app = build_router(make_daemon());

    let payload = serde_json::json!({
        "destination": "observe",
        "panel": "observe",
    });

    let request = Request::builder()
        .uri("/api/navigation")
        .method("POST")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(payload.to_string()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["status"], "ok");
    assert_eq!(json["snapshot"]["active_destination"], "observe");
    assert_eq!(json["snapshot"]["active_panel"], "observe");
}

#[tokio::test]
async fn post_home_launch_moves_into_chat() {
    let app = build_router(make_daemon());

    let payload = serde_json::json!({
        "template_id": "resume",
        "detail": "Finish the remote shell",
    });

    let request = Request::builder()
        .uri("/api/home/launch")
        .method("POST")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(payload.to_string()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert!(json["status"] == "launched" || json["status"] == "approval_required");
    assert_eq!(json["snapshot"]["active_destination"], "build");
    assert_eq!(json["snapshot"]["active_panel"], "chat");
}

#[tokio::test]
async fn post_chat_sets_active_conversation() {
    let daemon = make_daemon();
    let app = build_router(daemon.clone());

    let payload = serde_json::json!({
        "conversation_id": "conv-42",
        "content": "Inspect the shell state",
        "model": "auto",
    });

    let request = Request::builder()
        .uri("/api/chat")
        .method("POST")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(payload.to_string()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["status"], "streaming");
    assert_eq!(json["snapshot"]["active_conversation"], "conv-42");

    let snapshot = daemon.read().await.get_snapshot();
    assert_eq!(snapshot.active_conversation, Some("conv-42".into()));
}

#[tokio::test]
async fn approvals_can_be_resolved_over_http() {
    let app = build_router(make_daemon());

    let agent_payload = serde_json::json!({
        "goal": "Deploy the latest release",
        "orchestration_mode": "coordinator",
    });

    let request = Request::builder()
        .uri("/api/agents")
        .method("POST")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(agent_payload.to_string()))
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["status"], "approval_required");
    let request_id = json["request_id"].as_str().unwrap();

    let approval_payload = serde_json::json!({ "approved": true });
    let request = Request::builder()
        .uri(format!("/api/approvals/{request_id}/decision"))
        .method("POST")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(approval_payload.to_string()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["status"], "running");
    assert_eq!(json["approved"], true);
}

#[tokio::test]
async fn terminal_lifecycle_is_available_over_http() {
    let app = build_router(make_daemon());

    let start = Request::builder()
        .uri("/api/terminal/start")
        .method("POST")
        .body(axum::body::Body::empty())
        .unwrap();
    let response = app.clone().oneshot(start).await.unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["status"] == "started" || json["status"] == "running");

    let kill = Request::builder()
        .uri("/api/terminal/kill")
        .method("POST")
        .body(axum::body::Body::empty())
        .unwrap();
    let response = app.oneshot(kill).await.unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "ok");
}

#[tokio::test]
async fn workflow_run_endpoint_starts_automate_flow() {
    let app = build_router(make_daemon());

    let get_workflows = Request::builder()
        .uri("/api/panels/workflows")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();
    let response = app.clone().oneshot(get_workflows).await.unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let workflow_id = json["data"]["workflows"][0]["id"]
        .as_str()
        .expect("workflow id");

    let request = Request::builder()
        .uri("/api/workflows/run")
        .method("POST")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            serde_json::json!({ "workflow_id": workflow_id }).to_string(),
        ))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["status"], "started");
    assert_eq!(json["snapshot"]["active_destination"], "automate");
    assert_eq!(json["snapshot"]["active_panel"], "workflows");
}

#[tokio::test]
async fn channel_select_and_message_routes_update_channel_state() {
    let app = build_router(make_daemon());

    let get_channels = Request::builder()
        .uri("/api/panels/channels")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();
    let response = app.clone().oneshot(get_channels).await.unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let channel_id = json["data"]["channels"][0]["id"]
        .as_str()
        .expect("channel id");

    let select = Request::builder()
        .uri("/api/channels/select")
        .method("POST")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            serde_json::json!({ "channel_id": channel_id }).to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(select).await.unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "ok");
    assert_eq!(json["snapshot"]["active_destination"], "automate");
    assert_eq!(json["snapshot"]["active_panel"], "channels");

    let message = Request::builder()
        .uri("/api/channels/message")
        .method("POST")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            serde_json::json!({
                "channel_id": channel_id,
                "content": "Remote channel smoke test"
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(message).await.unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "ok");

    let refreshed = Request::builder()
        .uri("/api/panels/channels")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();
    let response = app.oneshot(refreshed).await.unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["data"]["selected_channel"]["id"], channel_id);
    assert!(json["data"]["selected_channel"]["messages"]
        .as_array()
        .unwrap()
        .iter()
        .any(|message| message["content"] == "Remote channel smoke test"));
}

#[tokio::test]
async fn utility_mutation_routes_update_panel_state() {
    let app = build_router(make_daemon());

    let update_setting = Request::builder()
        .uri("/api/settings/update")
        .method("POST")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            serde_json::json!({
                "setting": "privacy_mode",
                "value": true
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(update_setting).await.unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "ok");

    let get_settings = Request::builder()
        .uri("/api/panels/settings")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();
    let response = app.clone().oneshot(get_settings).await.unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["data"]["privacy_mode"], true);

    let update_ollama = Request::builder()
        .uri("/api/settings/text")
        .method("POST")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            serde_json::json!({
                "setting": "ollama_url",
                "value": "http://127.0.0.1:22434"
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(update_ollama).await.unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "ok");

    let get_settings = Request::builder()
        .uri("/api/panels/settings")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();
    let response = app.clone().oneshot(get_settings).await.unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["data"]["ollama_url"], "http://127.0.0.1:22434");

    let toggle_remote = Request::builder()
        .uri("/api/settings/update")
        .method("POST")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            serde_json::json!({
                "setting": "remote_enabled",
                "value": true
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(toggle_remote).await.unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "ok");

    let toggle_remote_auto_start = Request::builder()
        .uri("/api/settings/update")
        .method("POST")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            serde_json::json!({
                "setting": "remote_auto_start",
                "value": true
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(toggle_remote_auto_start).await.unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "ok");

    let get_launch = Request::builder()
        .uri("/api/panels/launch")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();
    let response = app.clone().oneshot(get_launch).await.unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["data"]["remote_enabled"], true);
    assert_eq!(json["data"]["remote_auto_start"], true);

    let update_cloud_api = Request::builder()
        .uri("/api/settings/text")
        .method("POST")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            serde_json::json!({
                "setting": "cloud_api_url",
                "value": "https://api.hive.example"
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(update_cloud_api).await.unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "ok");

    let update_cloud_relay = Request::builder()
        .uri("/api/settings/text")
        .method("POST")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            serde_json::json!({
                "setting": "cloud_relay_url",
                "value": "wss://relay.hive.example/ws"
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(update_cloud_relay).await.unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "ok");

    let update_cloud_tier = Request::builder()
        .uri("/api/settings/text")
        .method("POST")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            serde_json::json!({
                "setting": "cloud_tier",
                "value": "Pro"
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(update_cloud_tier).await.unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "ok");

    let get_launch = Request::builder()
        .uri("/api/panels/launch")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();
    let response = app.clone().oneshot(get_launch).await.unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["data"]["cloud_api_url"], "https://api.hive.example");
    assert_eq!(json["data"]["cloud_relay_url"], "wss://relay.hive.example/ws");
    assert_eq!(json["data"]["cloud_tier"], "Pro");

    let get_models = Request::builder()
        .uri("/api/panels/models")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();
    let response = app.clone().oneshot(get_models).await.unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let available_models = json["data"]["available_models"]
        .as_array()
        .expect("available models");
    let current_model = json["data"]["current_model"].as_str().expect("current model");
    let default_model = json["data"]["default_model"].as_str().expect("default model");
    let selected_model = available_models
        .iter()
        .filter_map(|model| model["id"].as_str())
        .find(|model| *model != default_model && *model != current_model)
        .or_else(|| {
            available_models
                .iter()
                .filter_map(|model| model["id"].as_str())
                .find(|model| *model != default_model)
        })
        .unwrap_or(default_model)
        .to_string();

    let set_current_model = Request::builder()
        .uri("/api/navigation")
        .method("POST")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            serde_json::json!({
                "model": selected_model
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(set_current_model).await.unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "ok");

    let get_models = Request::builder()
        .uri("/api/panels/models")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();
    let response = app.clone().oneshot(get_models).await.unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["data"]["current_model"], selected_model);

    let set_default_model = Request::builder()
        .uri("/api/models/default")
        .method("POST")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            serde_json::json!({
                "model": selected_model
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(set_default_model).await.unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "ok");

    let get_models = Request::builder()
        .uri("/api/panels/models")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();
    let response = app.clone().oneshot(get_models).await.unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["data"]["default_model"], selected_model);

    let update_provider_key = Request::builder()
        .uri("/api/providers/key")
        .method("POST")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            serde_json::json!({
                "provider": "openai",
                "key": "sk-openai-remote"
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(update_provider_key).await.unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "ok");

    let get_models = Request::builder()
        .uri("/api/panels/models")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();
    let response = app.clone().oneshot(get_models).await.unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["data"]["provider_credentials"]
        .as_array()
        .unwrap()
        .iter()
        .any(|provider| provider["id"] == "openai" && provider["has_key"] == true));

    let set_auto_routing = Request::builder()
        .uri("/api/routing/update")
        .method("POST")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            serde_json::json!({
                "enabled": false
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(set_auto_routing).await.unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "ok");

    let get_routing = Request::builder()
        .uri("/api/panels/routing")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();
    let response = app.clone().oneshot(get_routing).await.unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["data"]["auto_routing"], false);

    let add_project_model = Request::builder()
        .uri("/api/routing/project-models/add")
        .method("POST")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            serde_json::json!({
                "model": "gpt-5-mini"
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(add_project_model).await.unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "ok");

    let get_routing = Request::builder()
        .uri("/api/panels/routing")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();
    let response = app.clone().oneshot(get_routing).await.unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["data"]["project_models"]
        .as_array()
        .unwrap()
        .iter()
        .any(|model| model == "gpt-5-mini"));

    let remove_project_model = Request::builder()
        .uri("/api/routing/project-models/remove")
        .method("POST")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            serde_json::json!({
                "model": "gpt-5-mini"
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(remove_project_model).await.unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "ok");

    let get_routing = Request::builder()
        .uri("/api/panels/routing")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();
    let response = app.clone().oneshot(get_routing).await.unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(!json["data"]["project_models"]
        .as_array()
        .unwrap()
        .iter()
        .any(|model| model == "gpt-5-mini"));

    let get_skills = Request::builder()
        .uri("/api/panels/skills")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();
    let response = app.clone().oneshot(get_skills).await.unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let skills = json["data"]["skills"].as_array().expect("skills list");
    let first_skill = skills.first().expect("at least one skill");
    let skill_name = first_skill["name"].as_str().expect("skill name").to_string();
    let current_enabled = first_skill["enabled"].as_bool().expect("skill state");

    let toggle_skill = Request::builder()
        .uri("/api/skills/toggle")
        .method("POST")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            serde_json::json!({
                "name": skill_name,
                "enabled": !current_enabled
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(toggle_skill).await.unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "ok");

    let get_skills = Request::builder()
        .uri("/api/panels/skills")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();
    let response = app.clone().oneshot(get_skills).await.unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let updated_skill = json["data"]["skills"]
        .as_array()
        .unwrap()
        .iter()
        .find(|skill| skill["name"] == skill_name)
        .expect("updated skill present");
    assert_eq!(updated_skill["enabled"], serde_json::json!(!current_enabled));

    let install_skill = Request::builder()
        .uri("/api/skills/install")
        .method("POST")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            serde_json::json!({
                "name": "remote-custom",
                "description": "Remote-created custom skill",
                "instructions": "Inspect the active workspace and report the next highest-value task."
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(install_skill).await.unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "ok");

    let get_skills = Request::builder()
        .uri("/api/panels/skills")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();
    let response = app.clone().oneshot(get_skills).await.unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["data"]["skills"]
        .as_array()
        .unwrap()
        .iter()
        .any(|skill| skill["name"] == "remote-custom"));

    let remove_skill = Request::builder()
        .uri("/api/skills/remove")
        .method("POST")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            serde_json::json!({
                "name": "remote-custom"
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(remove_skill).await.unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "ok");

    let get_skills = Request::builder()
        .uri("/api/panels/skills")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();
    let response = app.oneshot(get_skills).await.unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(!json["data"]["skills"]
        .as_array()
        .unwrap()
        .iter()
        .any(|skill| skill["name"] == "remote-custom"));
}
