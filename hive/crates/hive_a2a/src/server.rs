//! HTTP server — Axum-based A2A endpoint for Hive.
//!
//! Provides:
//! - `GET /.well-known/agent-card.json` — Agent Card discovery
//! - `POST /a2a` — JSON-RPC message handler (`message/send`)
//! - `GET /a2a/tasks/{task_id}` — Task status lookup
//! - `GET /a2a/tasks/{task_id}/events` — SSE task status updates
//!
//! The server validates `X-Hive-Key` when an API key is configured.

use std::collections::HashMap;
use std::convert::Infallible;
use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};

use a2a_rs::{Message, MessageSendParams, Task, TaskState, TaskStatusUpdateEvent};
use axum::extract::{Path, State};
use axum::http::HeaderName;
use axum::http::{HeaderMap, Method, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Json};
use axum::routing::{get, post};
use axum::Router;
use futures::stream::{self, BoxStream};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::net::TcpListener;
use tokio::sync::{broadcast, Mutex, Semaphore};
use tower_http::cors::{Any, CorsLayer};

use hive_agents::hivemind::AiExecutor;

use crate::agent_card::build_hive_agent_card;
use crate::auth::validate_api_key_optional;
use crate::config::A2aConfig;
use crate::error::A2aError;
use crate::task_handler::HiveTaskHandler;

// ---------------------------------------------------------------------------
// Shared state
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct RateLimitState {
    window_started: Instant,
    requests: u32,
}

/// Blocking adapter so the Axum router can call into `HiveTaskHandler`
/// without requiring its async futures to be `Send`.
pub trait TaskHandlerAdapter: Send + Sync {
    fn handle_message_blocking(&self, task_id: String, message: Message) -> Result<Task, A2aError>;
    fn get_task_blocking(&self, task_id: &str) -> Result<Task, A2aError>;
    fn subscribe_blocking(
        &self,
        task_id: &str,
    ) -> Result<broadcast::Receiver<TaskStatusUpdateEvent>, A2aError>;
    /// Remove a task from the active tasks map (used for deferred purge).
    fn remove_task_blocking(&self, task_id: &str);
}

impl<E: AiExecutor + 'static> TaskHandlerAdapter for HiveTaskHandler<E> {
    fn handle_message_blocking(&self, task_id: String, message: Message) -> Result<Task, A2aError> {
        self.block_on(self.handle_message(&task_id, &message))
    }

    fn get_task_blocking(&self, task_id: &str) -> Result<Task, A2aError> {
        self.block_on(self.get_task(task_id))
    }

    fn subscribe_blocking(
        &self,
        task_id: &str,
    ) -> Result<broadcast::Receiver<TaskStatusUpdateEvent>, A2aError> {
        self.block_on(self.subscribe(task_id))
    }

    fn remove_task_blocking(&self, task_id: &str) {
        self.block_on(async {
            self.remove_task(task_id).await;
            Ok::<(), A2aError>(())
        })
        .ok();
    }
}

impl<E: AiExecutor + 'static> HiveTaskHandler<E> {
    /// Run a non-`Send` future to completion on a fresh single-use runtime.
    ///
    /// Creating a current-thread runtime per call is intentional: the
    /// `AiExecutor` future is not `Send`, so it cannot be driven by a shared
    /// multi-thread runtime, and a current-thread runtime cannot be called from
    /// multiple `spawn_blocking` threads concurrently. The construction cost
    /// (~20 us) is negligible relative to AI round-trip latency.
    fn block_on<F, T>(&self, future: F) -> Result<T, A2aError>
    where
        F: Future<Output = Result<T, A2aError>>,
    {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| A2aError::Config(format!("Failed to create task runtime: {e}")))?;
        rt.block_on(future)
    }
}

/// Shared server state passed to all Axum handlers.
#[derive(Clone)]
pub struct AppState {
    pub config: A2aConfig,
    pub task_handler: Option<Arc<dyn TaskHandlerAdapter>>,
    rate_limits: Arc<Mutex<HashMap<String, RateLimitState>>>,
    concurrent_tasks: Arc<Semaphore>,
}

// ---------------------------------------------------------------------------
// JSON-RPC types (minimal)
// ---------------------------------------------------------------------------

/// A minimal JSON-RPC 2.0 request envelope.
#[derive(Debug, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: Value,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

/// A JSON-RPC 2.0 success response.
#[derive(Debug, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Value,
    pub result: Value,
}

/// A JSON-RPC 2.0 error response.
#[derive(Debug, Serialize)]
pub struct JsonRpcErrorResponse {
    pub jsonrpc: String,
    pub id: Value,
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

/// Build the Axum router with discovery-only routes.
pub fn build_router(config: A2aConfig) -> Router {
    build_router_with_handler(config, None)
}

/// Build the Axum router with an optional live task handler.
pub fn build_router_with_handler(
    config: A2aConfig,
    task_handler: Option<Arc<dyn TaskHandlerAdapter>>,
) -> Router {
    let state = AppState {
        concurrent_tasks: Arc::new(Semaphore::new(config.server.max_concurrent_tasks.max(1))),
        rate_limits: Arc::new(Mutex::new(HashMap::new())),
        task_handler,
        config,
    };

    Router::new()
        .route("/.well-known/agent-card.json", get(agent_card_handler))
        .route("/a2a", post(send_message_handler))
        .route("/a2a/tasks/:task_id", get(get_task_handler))
        .route("/a2a/tasks/:task_id/events", get(task_events_handler))
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_headers([
                    HeaderName::from_static("content-type"),
                    HeaderName::from_static("x-hive-key"),
                ])
                .allow_methods([Method::GET, Method::POST]),
        )
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// `GET /.well-known/agent-card.json`
async fn agent_card_handler(State(state): State<AppState>) -> impl IntoResponse {
    let card = build_hive_agent_card(&state.config);
    Json(card)
}

/// `POST /a2a`
async fn send_message_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(rpc_request): Json<JsonRpcRequest>,
) -> impl IntoResponse {
    if let Err(message) = validate_request_auth(&state, &headers) {
        return rpc_error(StatusCode::UNAUTHORIZED, rpc_request.id, -32001, message);
    }

    if rpc_request.jsonrpc != "2.0" {
        return rpc_error(
            StatusCode::BAD_REQUEST,
            rpc_request.id,
            -32600,
            "Invalid JSON-RPC version",
        );
    }

    if let Err(message) = enforce_rate_limit(&state, &headers).await {
        return rpc_error(
            StatusCode::TOO_MANY_REQUESTS,
            rpc_request.id,
            -32029,
            message,
        );
    }

    match rpc_request.method.as_str() {
        "message/send" => {
            let params: MessageSendParams = match serde_json::from_value(rpc_request.params) {
                Ok(params) => params,
                Err(e) => {
                    return rpc_error(
                        StatusCode::BAD_REQUEST,
                        rpc_request.id,
                        -32602,
                        format!("Invalid message/send params: {e}"),
                    );
                }
            };

            let Some(task_handler) = state.task_handler.clone() else {
                return rpc_error(
                    StatusCode::SERVICE_UNAVAILABLE,
                    rpc_request.id,
                    -32000,
                    "Task execution is not configured for this server",
                );
            };

            let permit = match state.concurrent_tasks.clone().try_acquire_owned() {
                Ok(permit) => permit,
                Err(_) => {
                    return rpc_error(
                        StatusCode::TOO_MANY_REQUESTS,
                        rpc_request.id,
                        -32002,
                        "Max concurrent tasks reached",
                    );
                }
            };

            let task_id = params
                .message
                .task_id
                .clone()
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
            let message = params.message;

            let purge_handler = task_handler.clone();
            let purge_task_id = task_id.clone();
            let result = tokio::task::spawn_blocking(move || {
                let _permit = permit;
                task_handler.handle_message_blocking(task_id, message)
            })
            .await;

            match result {
                Ok(Ok(ref task)) if task_is_final(&task.status.state) => {
                    // Schedule removal of completed/failed task after 5 minutes
                    // to prevent unbounded memory growth in active_tasks.
                    tokio::spawn(async move {
                        tokio::time::sleep(Duration::from_secs(300)).await;
                        tokio::task::spawn_blocking(move || {
                            purge_handler.remove_task_blocking(&purge_task_id);
                        })
                        .await
                        .ok();
                    });
                    rpc_success(
                        rpc_request.id,
                        serde_json::to_value(task).unwrap_or_default(),
                    )
                }
                Ok(Ok(task)) => rpc_success(
                    rpc_request.id,
                    serde_json::to_value(task).unwrap_or_default(),
                ),
                Ok(Err(error)) => {
                    let (status, code) = match error {
                        A2aError::TaskNotFound(_) => (StatusCode::NOT_FOUND, -32004),
                        A2aError::UnsupportedSkill(_) | A2aError::Bridge(_) => {
                            (StatusCode::BAD_REQUEST, -32602)
                        }
                        _ => (StatusCode::INTERNAL_SERVER_ERROR, -32010),
                    };
                    rpc_error(status, rpc_request.id, code, error.to_string())
                }
                Err(e) => rpc_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    rpc_request.id,
                    -32011,
                    format!("Task execution thread failed: {e}"),
                ),
            }
        }
        _ => {
            let method_preview: String = rpc_request.method.chars().take(64).collect();
            rpc_error(
                StatusCode::OK,
                rpc_request.id,
                -32601,
                format!("Method not found: {}", method_preview),
            )
        }
    }
}

/// `GET /a2a/tasks/{task_id}`
async fn get_task_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(task_id): Path<String>,
) -> impl IntoResponse {
    if let Err(message) = validate_request_auth(&state, &headers) {
        return json_error(StatusCode::UNAUTHORIZED, message);
    }

    let Some(task_handler) = state.task_handler.clone() else {
        return json_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "Task execution is not configured for this server",
        );
    };

    match tokio::task::spawn_blocking(move || task_handler.get_task_blocking(&task_id)).await {
        Ok(Ok(task)) => (
            StatusCode::OK,
            Json(serde_json::to_value(task).unwrap_or_default()),
        ),
        Ok(Err(A2aError::TaskNotFound(message))) => json_error(StatusCode::NOT_FOUND, message),
        Ok(Err(error)) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
        Err(e) => json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Task lookup thread failed: {e}"),
        ),
    }
}

/// `GET /a2a/tasks/{task_id}/events`
async fn task_events_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(task_id): Path<String>,
) -> Result<Sse<BoxStream<'static, Result<Event, Infallible>>>, impl IntoResponse> {
    if let Err(message) = validate_request_auth(&state, &headers) {
        return Err(json_error(StatusCode::UNAUTHORIZED, message));
    }

    let Some(task_handler) = state.task_handler.clone() else {
        return Err(json_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "Task execution is not configured for this server",
        ));
    };

    let lookup_handler = task_handler.clone();
    let lookup_task_id = task_id.clone();
    let task = match tokio::task::spawn_blocking(move || {
        lookup_handler.get_task_blocking(&lookup_task_id)
    })
    .await
    {
        Ok(Ok(task)) => task,
        Ok(Err(A2aError::TaskNotFound(message))) => {
            return Err(json_error(StatusCode::NOT_FOUND, message));
        }
        Ok(Err(error)) => {
            return Err(json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                error.to_string(),
            ));
        }
        Err(e) => {
            return Err(json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Task lookup thread failed: {e}"),
            ));
        }
    };

    if task_is_final(&task.status.state) {
        let snapshot = serde_json::to_string(&status_update_from_task(&task)).unwrap_or_default();
        let stream = stream::once(async move {
            Ok::<Event, Infallible>(Event::default().event("status").data(snapshot))
        })
        .boxed();
        return Ok(Sse::new(stream).keep_alive(KeepAlive::default()));
    }

    let subscribe_handler = task_handler.clone();
    let subscribe_task_id = task_id.clone();
    let receiver = match tokio::task::spawn_blocking(move || {
        subscribe_handler.subscribe_blocking(&subscribe_task_id)
    })
    .await
    {
        Ok(Ok(receiver)) => receiver,
        Ok(Err(A2aError::TaskNotFound(message))) => {
            return Err(json_error(StatusCode::NOT_FOUND, message));
        }
        Ok(Err(error)) => {
            return Err(json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                error.to_string(),
            ));
        }
        Err(e) => {
            return Err(json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Task subscription thread failed: {e}"),
            ));
        }
    };

    let stream = stream::unfold((receiver, false), |(mut receiver, done)| async move {
        if done {
            return None;
        }

        match tokio::time::timeout(Duration::from_secs(300), receiver.recv()).await {
            Ok(Ok(update)) => {
                let payload = serde_json::to_string(&update).unwrap_or_default();
                let next_done = update.final_;
                Some((
                    Ok::<Event, Infallible>(Event::default().event("status").data(payload)),
                    (receiver, next_done),
                ))
            }
            Ok(Err(_)) | Err(_) => None,
        }
    })
    .boxed();

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

// ---------------------------------------------------------------------------
// Server startup
// ---------------------------------------------------------------------------

/// Start the discovery-only A2A HTTP server.
pub async fn start_server(config: A2aConfig) -> Result<(), A2aError> {
    start_server_with_handler(config, None).await
}

/// Start the A2A HTTP server with a live task handler.
pub async fn start_server_with_handler(
    config: A2aConfig,
    task_handler: Option<Arc<dyn TaskHandlerAdapter>>,
) -> Result<(), A2aError> {
    if !config.server.enabled {
        return Ok(());
    }

    let addr = config.bind_addr();
    let router = build_router_with_handler(config, task_handler);

    let listener = TcpListener::bind(&addr)
        .await
        .map_err(|e| A2aError::Network(format!("Failed to bind to {}: {}", addr, e)))?;

    axum::serve(listener, router)
        .await
        .map_err(|e| A2aError::Network(format!("Server error: {}", e)))?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn validate_request_auth(state: &AppState, headers: &HeaderMap) -> Result<(), String> {
    let provided_key = headers.get("x-hive-key").and_then(|v| v.to_str().ok());
    let expected_key = state.config.server.api_key.as_deref();

    validate_api_key_optional(provided_key, expected_key)
        .map_err(|_| "Authentication failed".into())
}

async fn enforce_rate_limit(state: &AppState, headers: &HeaderMap) -> Result<(), String> {
    let key = headers
        .get("x-hive-key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("anonymous")
        .to_string();

    let mut limits = state.rate_limits.lock().await;
    let limiter = limits.entry(key).or_insert_with(|| RateLimitState {
        window_started: Instant::now(),
        requests: 0,
    });

    if limiter.window_started.elapsed() >= Duration::from_secs(60) {
        limiter.window_started = Instant::now();
        limiter.requests = 0;
    }

    if limiter.requests >= state.config.server.rate_limit_rpm {
        return Err(format!(
            "Rate limit exceeded ({} requests/minute)",
            state.config.server.rate_limit_rpm
        ));
    }

    limiter.requests += 1;
    Ok(())
}

fn status_update_from_task(task: &Task) -> TaskStatusUpdateEvent {
    TaskStatusUpdateEvent {
        task_id: task.id.clone(),
        context_id: task.context_id.clone(),
        kind: "status-update".into(),
        status: task.status.clone(),
        final_: task_is_final(&task.status.state),
        metadata: None,
    }
}

fn task_is_final(state: &TaskState) -> bool {
    !matches!(state, TaskState::Working | TaskState::Submitted)
}

fn rpc_success(id: Value, result: Value) -> (StatusCode, Json<Value>) {
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": result,
        })),
    )
}

fn rpc_error(
    status: StatusCode,
    id: Value,
    code: i32,
    message: impl Into<String>,
) -> (StatusCode, Json<Value>) {
    (
        status,
        Json(serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {
                "code": code,
                "message": message.into(),
            },
        })),
    )
}

fn json_error(status: StatusCode, message: impl Into<String>) -> (StatusCode, Json<Value>) {
    (
        status,
        Json(serde_json::json!({
            "error": message.into(),
        })),
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use hive_ai::types::{ChatRequest, ChatResponse, FinishReason, TokenUsage};
    use http::Request;
    use tower::ServiceExt; // for oneshot

    use crate::config::ServerDefaults;

    struct MockExecutor;

    impl AiExecutor for MockExecutor {
        async fn execute(&self, request: &ChatRequest) -> Result<ChatResponse, String> {
            Ok(ChatResponse {
                content: format!("mock: {}", request.messages.len()),
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

    fn message_send_body(id: &str, task_id: &str) -> Value {
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "message/send",
            "params": {
                "message": {
                    "role": "user",
                    "parts": [{"kind": "text", "text": "hello"}],
                    "messageId": "m1",
                    "taskId": task_id,
                    "kind": "message"
                },
                "configuration": {
                    "acceptedOutputModes": ["text"],
                    "blocking": true
                }
            }
        })
    }

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
        let card: Value = serde_json::from_slice(&body).unwrap();
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
        let card: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(card["capabilities"]["streaming"], true);
        assert_eq!(card["capabilities"]["stateTransitionHistory"], true);
    }

    #[tokio::test]
    async fn test_send_message_requires_task_handler() {
        let config = A2aConfig::default();
        let app = build_router(config);

        let req = Request::builder()
            .method("POST")
            .uri("/a2a")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_string(&message_send_body("1", "task-1")).unwrap(),
            ))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn test_send_message_with_handler_completes_task() {
        let config = A2aConfig::default();
        let app = build_router_with_handler(config, Some(task_handler()));

        let req = Request::builder()
            .method("POST")
            .uri("/a2a")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_string(&message_send_body("1", "task-1")).unwrap(),
            ))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["jsonrpc"], "2.0");
        assert_eq!(json["result"]["id"], "task-1");
        assert_eq!(json["result"]["status"]["state"], "completed");
    }

    #[tokio::test]
    async fn test_send_message_auth_required_when_configured() {
        let mut config = A2aConfig::default();
        config.server.api_key = Some("secret".into());
        let app = build_router_with_handler(config, Some(task_handler()));

        let req = Request::builder()
            .method("POST")
            .uri("/a2a")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_string(&message_send_body("1", "task-1")).unwrap(),
            ))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_send_message_auth_succeeds_with_correct_key() {
        let mut config = A2aConfig::default();
        config.server.api_key = Some("secret".into());
        let app = build_router_with_handler(config, Some(task_handler()));

        let req = Request::builder()
            .method("POST")
            .uri("/a2a")
            .header("content-type", "application/json")
            .header("x-hive-key", "secret")
            .body(Body::from(
                serde_json::to_string(&message_send_body("2", "task-2")).unwrap(),
            ))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_send_message_unknown_method() {
        let config = A2aConfig::default();
        let app = build_router_with_handler(config, Some(task_handler()));

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
        assert_eq!(resp.status(), StatusCode::OK);

        let resp_body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: Value = serde_json::from_slice(&resp_body).unwrap();
        assert_eq!(json["error"]["code"], -32601);
        assert!(json["error"]["message"]
            .as_str()
            .unwrap()
            .contains("tasks/bogus"));
    }

    #[tokio::test]
    async fn test_get_task_returns_completed_task() {
        let config = A2aConfig::default();
        let app = build_router_with_handler(config, Some(task_handler()));

        let send_req = Request::builder()
            .method("POST")
            .uri("/a2a")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_string(&message_send_body("4", "task-lookup")).unwrap(),
            ))
            .unwrap();
        let _ = app.clone().oneshot(send_req).await.unwrap();

        let get_req = Request::builder()
            .uri("/a2a/tasks/task-lookup")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(get_req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["id"], "task-lookup");
        assert_eq!(json["status"]["state"], "completed");
    }

    #[tokio::test]
    async fn test_get_task_not_found() {
        let config = A2aConfig::default();
        let app = build_router_with_handler(config, Some(task_handler()));

        let req = Request::builder()
            .uri("/a2a/tasks/nonexistent")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_task_events_returns_snapshot_for_completed_task() {
        let config = A2aConfig::default();
        let app = build_router_with_handler(config, Some(task_handler()));

        let send_req = Request::builder()
            .method("POST")
            .uri("/a2a")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_string(&message_send_body("5", "task-events")).unwrap(),
            ))
            .unwrap();
        let _ = app.clone().oneshot(send_req).await.unwrap();

        let events_req = Request::builder()
            .uri("/a2a/tasks/task-events/events")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(events_req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let text = String::from_utf8(body.to_vec()).unwrap();
        assert!(text.contains("task-events"));
        assert!(text.contains("completed"));
    }

    #[tokio::test]
    async fn test_rate_limit_enforced() {
        let mut config = A2aConfig::default();
        config.server.rate_limit_rpm = 1;
        let app = build_router_with_handler(config, Some(task_handler()));

        let first_req = Request::builder()
            .method("POST")
            .uri("/a2a")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_string(&message_send_body("6", "task-rate-1")).unwrap(),
            ))
            .unwrap();
        let first_resp = app.clone().oneshot(first_req).await.unwrap();
        assert_eq!(first_resp.status(), StatusCode::OK);

        let second_req = Request::builder()
            .method("POST")
            .uri("/a2a")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_string(&message_send_body("7", "task-rate-2")).unwrap(),
            ))
            .unwrap();
        let second_resp = app.oneshot(second_req).await.unwrap();
        assert_eq!(second_resp.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    #[tokio::test]
    async fn test_start_server_disabled() {
        let mut config = A2aConfig::default();
        config.server.enabled = false;
        let result = start_server(config).await;
        assert!(result.is_ok());
    }
}
