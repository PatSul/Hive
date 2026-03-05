//! Full round-trip integration test:
//! 1. Start the A2A server on a random port
//! 2. Fetch the Agent Card
//! 3. Send a task message
//! 4. Verify the responses

use hive_a2a::config::A2aConfig;

/// Helper: spawn a real HTTP server on a random port and return the base URL.
async fn spawn_server(config: A2aConfig) -> String {
    let router = hive_a2a::server::build_router(config);
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

    // 1. Fetch Agent Card
    let card_url = format!("{}/.well-known/agent-card.json", base);
    let card_resp = client.get(&card_url).send().await.unwrap();
    assert_eq!(card_resp.status(), 200);

    let card: serde_json::Value = card_resp.json().await.unwrap();
    assert_eq!(card["name"], "Hive");
    assert_eq!(card["skills"].as_array().unwrap().len(), 4);
    assert!(card["capabilities"]["streaming"].as_bool().unwrap_or(false));

    // 2. Send a message (JSON-RPC)
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
                "kind": "message"
            }
        }
    });

    let resp = client.post(&a2a_url).json(&msg).send().await.unwrap();
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["jsonrpc"], "2.0");
    assert_eq!(body["id"], "test-1");
    // Should have a result (not an error)
    assert!(body.get("result").is_some(), "Expected result, got: {body}");
    // Result should contain a task in "working" state
    assert!(body["result"]["id"].is_string());
    assert_eq!(body["result"]["status"]["state"], "working");
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
        "params": {}
    });

    // Without key -> 401
    let resp = client.post(&a2a_url).json(&msg).send().await.unwrap();
    assert_eq!(resp.status(), 401);

    // With wrong key -> 401
    let resp = client
        .post(&a2a_url)
        .header("x-hive-key", "wrong")
        .json(&msg)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);

    // With correct key -> 200
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
    // Server returns HTTP 200 with a JSON-RPC error body for unknown methods
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body.get("error").is_some(), "Expected error, got: {body}");
    assert_eq!(body["error"]["code"], -32601);
    assert!(body["error"]["message"]
        .as_str()
        .unwrap()
        .contains("unknown/method"));
}

#[tokio::test]
async fn test_task_lookup_not_found() {
    let config = A2aConfig::default();
    let base = spawn_server(config).await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{}/a2a/tasks/nonexistent-id", base))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["error"]
        .as_str()
        .unwrap()
        .contains("nonexistent-id"));
}
