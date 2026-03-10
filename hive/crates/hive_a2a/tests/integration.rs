//! Full round-trip integration tests for the live A2A server path.

use std::sync::Arc;
use std::time::Duration;

use hive_a2a::config::{A2aConfig, ServerDefaults};
use hive_a2a::{discover_agent, DiscoveryCache, HiveTaskHandler, RemoteAgent, TaskHandlerAdapter};
use hive_agents::hivemind::AiExecutor;
use hive_ai::types::{ChatRequest, ChatResponse, FinishReason, TokenUsage};

struct MockExecutor;

impl AiExecutor for MockExecutor {
    async fn execute(&self, request: &ChatRequest) -> Result<ChatResponse, String> {
        Ok(ChatResponse {
            content: format!("integration: {}", request.messages.len()),
            model: request.model.clone(),
            usage: TokenUsage {
                prompt_tokens: 1,
                completion_tokens: 1,
                total_tokens: 2,
                cache_creation_input_tokens: None,
                cache_read_input_tokens: None,
            },
            finish_reason: FinishReason::Stop,
            thinking: None,
            tool_calls: None,
        })
    }
}

fn task_handler() -> Arc<dyn TaskHandlerAdapter> {
    Arc::new(HiveTaskHandler::new(
        Arc::new(MockExecutor),
        ServerDefaults::default(),
    ))
}

/// Helper: spawn a real HTTP server on a random port and return the base URL.
async fn spawn_server(config: A2aConfig) -> String {
    let router = hive_a2a::server::build_router_with_handler(config, Some(task_handler()));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    format!("http://{}", addr)
}

#[tokio::test]
async fn test_full_round_trip() {
    let config = A2aConfig::default();
    let base = spawn_server(config).await;
    let client = reqwest::Client::new();

    let card_url = format!("{}/.well-known/agent-card.json", base);
    let card_resp = client.get(&card_url).send().await.unwrap();
    assert_eq!(card_resp.status(), 200);

    let card: serde_json::Value = card_resp.json().await.unwrap();
    assert_eq!(card["name"], "Hive");
    assert_eq!(card["skills"].as_array().unwrap().len(), 4);
    assert!(card["capabilities"]["streaming"].as_bool().unwrap_or(false));

    let a2a_url = format!("{}/a2a", base);
    let msg = serde_json::json!({
        "jsonrpc": "2.0",
        "id": "test-1",
        "method": "message/send",
        "params": {
            "message": {
                "role": "user",
                "parts": [{ "kind": "text", "text": "Hello Hive" }],
                "messageId": "msg-1",
                "taskId": "integration-task-1",
                "kind": "message"
            },
            "configuration": {
                "acceptedOutputModes": ["text"],
                "blocking": true
            }
        }
    });

    let resp = client.post(&a2a_url).json(&msg).send().await.unwrap();
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["jsonrpc"], "2.0");
    assert_eq!(body["id"], "test-1");
    assert!(body.get("result").is_some(), "Expected result, got: {body}");
    assert_eq!(body["result"]["id"], "integration-task-1");
    assert_eq!(body["result"]["status"]["state"], "completed");
    assert_eq!(body["result"]["kind"], "task");
}

#[tokio::test]
async fn test_auth_enforcement() {
    let mut config = A2aConfig::default();
    config.server.api_key = Some("test-secret".into());
    let base = spawn_server(config).await;
    let client = reqwest::Client::new();

    let a2a_url = format!("{}/a2a", base);
    let msg = serde_json::json!({
        "jsonrpc": "2.0",
        "id": "1",
        "method": "message/send",
        "params": {
            "message": {
                "role": "user",
                "parts": [{ "kind": "text", "text": "auth" }],
                "messageId": "msg-auth",
                "taskId": "auth-task",
                "kind": "message"
            }
        }
    });

    let resp = client.post(&a2a_url).json(&msg).send().await.unwrap();
    assert_eq!(resp.status(), 401);

    let resp = client
        .post(&a2a_url)
        .header("x-hive-key", "wrong")
        .json(&msg)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);

    let resp = client
        .post(&a2a_url)
        .header("x-hive-key", "test-secret")
        .json(&msg)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn test_unknown_method_returns_error() {
    let config = A2aConfig::default();
    let base = spawn_server(config).await;
    let client = reqwest::Client::new();

    let a2a_url = format!("{}/a2a", base);
    let msg = serde_json::json!({
        "jsonrpc": "2.0",
        "id": "1",
        "method": "unknown/method",
        "params": {}
    });

    let resp = client.post(&a2a_url).json(&msg).send().await.unwrap();
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body.get("error").is_some(), "Expected error, got: {body}");
    assert_eq!(body["error"]["code"], -32601);
}

#[tokio::test]
async fn test_task_lookup_returns_completed_task() {
    let config = A2aConfig::default();
    let base = spawn_server(config).await;
    let client = reqwest::Client::new();

    let send_resp = client
        .post(format!("{}/a2a", base))
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": "task-lookup",
            "method": "message/send",
            "params": {
                "message": {
                    "role": "user",
                    "parts": [{ "kind": "text", "text": "lookup" }],
                    "messageId": "msg-lookup",
                    "taskId": "lookup-task",
                    "kind": "message"
                }
            }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(send_resp.status(), 200);

    let resp = client
        .get(format!("{}/a2a/tasks/lookup-task", base))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["id"], "lookup-task");
    assert_eq!(body["status"]["state"], "completed");
}

#[tokio::test]
async fn test_remote_agent_send_task_hits_a2a_endpoint() {
    let config = A2aConfig::default();
    let base = spawn_server(config).await;

    let agent = RemoteAgent::with_card(
        "Hive".into(),
        base.clone(),
        reqwest::get(format!("{}/.well-known/agent-card.json", base))
            .await
            .unwrap()
            .json()
            .await
            .unwrap(),
        None,
    );

    let task = agent
        .send_task("Run the integration flow", Some("single"), "remote-task-1")
        .await
        .unwrap();

    assert_eq!(task.id, "remote-task-1");
    assert_eq!(task.status.state, a2a_rs::TaskState::Completed);
}

#[tokio::test]
async fn test_discover_agent_fetches_live_agent_card() {
    let config = A2aConfig::default();
    let base = spawn_server(config).await;
    let cache = DiscoveryCache::new(Duration::from_secs(60));

    let card = discover_agent(&base, &cache).await.unwrap();

    assert_eq!(card.name, "Hive");
    assert!(card.capabilities.streaming);
    assert_eq!(cache.get(&base).unwrap().name, "Hive");
}

#[tokio::test]
async fn test_task_events_endpoint_streams_completed_snapshot_over_http() {
    let config = A2aConfig::default();
    let base = spawn_server(config).await;
    let client = reqwest::Client::new();

    let send_resp = client
        .post(format!("{}/a2a", base))
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": "task-events-http",
            "method": "message/send",
            "params": {
                "message": {
                    "role": "user",
                    "parts": [{ "kind": "text", "text": "events please" }],
                    "messageId": "msg-events-http",
                    "taskId": "events-http-task",
                    "kind": "message"
                }
            }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(send_resp.status(), 200);

    let body = client
        .get(format!("{}/a2a/tasks/events-http-task/events", base))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    assert!(body.contains("events-http-task"));
    assert!(body.contains("completed"));
}
