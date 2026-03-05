//! HTTP server — Axum-based A2A endpoint for Hive.
//!
//! Provides:
//! - `GET /.well-known/agent-card.json` — Agent Card discovery
//! - `POST /a2a` — JSON-RPC message handler (message/send)
//! - `GET /a2a/tasks/{task_id}` — Task status lookup
//!
//! The server validates `X-Hive-Key` when an API key is configured.

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Json};
use axum::routing::{get, post};
use axum::Router;
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;

use crate::agent_card::build_hive_agent_card;
use crate::auth::validate_api_key_optional;
use crate::config::A2aConfig;
use crate::error::A2aError;

// ---------------------------------------------------------------------------
// Shared state
// ---------------------------------------------------------------------------

/// Shared server state passed to all Axum handlers.
///
/// NOTE: We cannot include `HiveTaskHandler` here because it is generic
/// over `E: AiExecutor`. For now, the server handles agent card discovery,
/// authentication, and basic JSON-RPC routing. Full task execution
/// integration requires the caller to wire the handler externally.
#[derive(Clone)]
pub struct AppState {
    pub config: A2aConfig,
}

// ---------------------------------------------------------------------------
// JSON-RPC types (minimal)
// ---------------------------------------------------------------------------

/// A minimal JSON-RPC 2.0 request envelope.
#[derive(Debug, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: serde_json::Value,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

/// A JSON-RPC 2.0 success response.
#[derive(Debug, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: serde_json::Value,
    pub result: serde_json::Value,
}

/// A JSON-RPC 2.0 error response.
#[derive(Debug, Serialize)]
pub struct JsonRpcErrorResponse {
    pub jsonrpc: String,
    pub id: serde_json::Value,
    pub error: JsonRpcError,
}

/// A JSON-RPC 2.0 error object.
#[derive(Debug, Serialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

/// Build the Axum router with all A2A routes.
pub fn build_router(config: A2aConfig) -> Router {
    Router::new()
        .route("/.well-known/agent-card.json", get(agent_card_handler))
        .route("/a2a", post(send_message_handler))
        .route("/a2a/tasks/:task_id", get(get_task_handler))
        .with_state(AppState { config })
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// `GET /.well-known/agent-card.json`
///
/// Returns the Hive Agent Card as JSON, describing the agent's identity,
/// capabilities, and available skills.
async fn agent_card_handler(State(state): State<AppState>) -> impl IntoResponse {
    let card = build_hive_agent_card(&state.config);
    Json(card)
}

/// `POST /a2a`
///
/// Accepts a JSON-RPC 2.0 request. Currently supports:
/// - `message/send` — Validates auth, parses the request, and returns an
///   acknowledgement with a task ID in "working" status.
///
/// When no API key is configured, all requests are accepted.
/// When an API key is configured, the `X-Hive-Key` header must match.
async fn send_message_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(rpc_request): Json<JsonRpcRequest>,
) -> impl IntoResponse {
    // --- Auth check ---
    let provided_key = headers
        .get("x-hive-key")
        .and_then(|v| v.to_str().ok());
    let expected_key = state.config.server.api_key.as_deref();

    if let Err(_e) = validate_api_key_optional(provided_key, expected_key) {
        let err_resp = JsonRpcErrorResponse {
            jsonrpc: "2.0".into(),
            id: rpc_request.id,
            error: JsonRpcError {
                code: -32001,
                message: "Authentication failed".into(),
            },
        };
        return (StatusCode::UNAUTHORIZED, Json(serde_json::to_value(err_resp).unwrap()));
    }

    // --- Validate JSON-RPC version ---
    if rpc_request.jsonrpc != "2.0" {
        let err_resp = JsonRpcErrorResponse {
            jsonrpc: "2.0".into(),
            id: rpc_request.id,
            error: JsonRpcError {
                code: -32600,
                message: "Invalid JSON-RPC version".into(),
            },
        };
        return (StatusCode::BAD_REQUEST, Json(serde_json::to_value(err_resp).unwrap()));
    }

    // --- Dispatch by method ---
    match rpc_request.method.as_str() {
        "message/send" => {
            // Generate a task ID for the acknowledgement.
            let task_id = uuid::Uuid::new_v4().to_string();
            let context_id = uuid::Uuid::new_v4().to_string();

            let result = serde_json::json!({
                "id": task_id,
                "contextId": context_id,
                "status": {
                    "state": "working",
                    "message": null,
                    "timestamp": chrono::Utc::now().to_rfc3339()
                },
                "kind": "task"
            });

            let response = JsonRpcResponse {
                jsonrpc: "2.0".into(),
                id: rpc_request.id,
                result,
            };

            (StatusCode::OK, Json(serde_json::to_value(response).unwrap()))
        }
        _ => {
            let err_resp = JsonRpcErrorResponse {
                jsonrpc: "2.0".into(),
                id: rpc_request.id,
                error: JsonRpcError {
                    code: -32601,
                    message: format!("Method not found: {}", rpc_request.method),
                },
            };
            (StatusCode::OK, Json(serde_json::to_value(err_resp).unwrap()))
        }
    }
}

/// `GET /a2a/tasks/{task_id}`
///
/// Retrieves task status by ID. Currently returns 404 for all requests
/// because tasks are not yet stored in server state (requires
/// `HiveTaskHandler` wiring, which is generic over `AiExecutor`).
async fn get_task_handler(
    Path(task_id): Path<String>,
) -> impl IntoResponse {
    let body = serde_json::json!({
        "error": format!("Task not found: {}", task_id)
    });
    (StatusCode::NOT_FOUND, Json(body))
}

// ---------------------------------------------------------------------------
// Server startup
// ---------------------------------------------------------------------------

/// Start the A2A HTTP server.
///
/// Binds to the address specified in `config` and serves the A2A router.
/// Returns immediately with `Ok(())` if the server is disabled in config.
///
/// This function blocks until the server is shut down (e.g. via signal).
pub async fn start_server(config: A2aConfig) -> Result<(), A2aError> {
    if !config.server.enabled {
        return Ok(());
    }

    let addr = config.bind_addr();
    let router = build_router(config);

    let listener = TcpListener::bind(&addr)
        .await
        .map_err(|e| A2aError::Network(format!("Failed to bind to {}: {}", addr, e)))?;

    axum::serve(listener, router)
        .await
        .map_err(|e| A2aError::Network(format!("Server error: {}", e)))?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use http::Request;
    use tower::ServiceExt; // for oneshot

    #[tokio::test]
    async fn test_agent_card_endpoint() {
        let config = A2aConfig::default();
        let app = build_router(config);

        let req = Request::builder()
            .uri("/.well-known/agent-card.json")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), 200);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let card: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(card["name"], "Hive");
        assert_eq!(card["skills"].as_array().unwrap().len(), 4);
    }

    #[tokio::test]
    async fn test_agent_card_has_capabilities() {
        let config = A2aConfig::default();
        let app = build_router(config);

        let req = Request::builder()
            .uri("/.well-known/agent-card.json")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let card: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(card["capabilities"]["streaming"], true);
        assert_eq!(card["capabilities"]["stateTransitionHistory"], true);
    }

    #[tokio::test]
    async fn test_send_message_no_auth_when_unconfigured() {
        let config = A2aConfig::default(); // no api_key
        let app = build_router(config);

        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": "1",
            "method": "message/send",
            "params": {
                "message": {
                    "role": "user",
                    "parts": [{"kind": "text", "text": "hello"}],
                    "messageId": "m1",
                    "kind": "message"
                }
            }
        });

        let req = Request::builder()
            .method("POST")
            .uri("/a2a")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), 200);

        let resp_body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&resp_body).unwrap();
        assert_eq!(json["jsonrpc"], "2.0");
        assert!(json["result"]["id"].is_string());
        assert_eq!(json["result"]["status"]["state"], "working");
    }

    #[tokio::test]
    async fn test_send_message_auth_required_when_configured() {
        let mut config = A2aConfig::default();
        config.server.api_key = Some("secret".into());
        let app = build_router(config);

        // No X-Hive-Key header
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": "1",
            "method": "message/send",
            "params": {}
        });

        let req = Request::builder()
            .method("POST")
            .uri("/a2a")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), 401);
    }

    #[tokio::test]
    async fn test_send_message_auth_succeeds_with_correct_key() {
        let mut config = A2aConfig::default();
        config.server.api_key = Some("secret".into());
        let app = build_router(config);

        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": "2",
            "method": "message/send",
            "params": {}
        });

        let req = Request::builder()
            .method("POST")
            .uri("/a2a")
            .header("content-type", "application/json")
            .header("x-hive-key", "secret")
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), 200);
    }

    #[tokio::test]
    async fn test_send_message_unknown_method() {
        let config = A2aConfig::default();
        let app = build_router(config);

        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": "3",
            "method": "tasks/bogus",
            "params": {}
        });

        let req = Request::builder()
            .method("POST")
            .uri("/a2a")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), 200);

        let resp_body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&resp_body).unwrap();
        assert_eq!(json["error"]["code"], -32601);
        assert!(json["error"]["message"]
            .as_str()
            .unwrap()
            .contains("tasks/bogus"));
    }

    #[tokio::test]
    async fn test_get_task_not_found() {
        let config = A2aConfig::default();
        let app = build_router(config);

        let req = Request::builder()
            .uri("/a2a/tasks/nonexistent")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), 404);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["error"]
            .as_str()
            .unwrap()
            .contains("nonexistent"));
    }

    #[tokio::test]
    async fn test_start_server_disabled() {
        let mut config = A2aConfig::default();
        config.server.enabled = false;
        // Should return Ok immediately without binding.
        let result = start_server(config).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_send_message_response_has_context_id() {
        let config = A2aConfig::default();
        let app = build_router(config);

        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": "4",
            "method": "message/send",
            "params": {}
        });

        let req = Request::builder()
            .method("POST")
            .uri("/a2a")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        let resp_body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&resp_body).unwrap();
        assert!(json["result"]["contextId"].is_string());
        assert_eq!(json["result"]["kind"], "task");
    }
}
