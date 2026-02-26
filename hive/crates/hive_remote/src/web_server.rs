use crate::web_api::{self, DaemonState};
use axum::routing::{any, get, post};
use axum::Router;

/// Build the axum router with all API and WebSocket routes.
///
/// Routes:
/// - GET  /api/state          -- current session snapshot
/// - POST /api/chat           -- send a chat message
/// - GET  /api/panels/{id}    -- panel-specific data
/// - POST /api/agents         -- start/cancel agent tasks
/// - GET  /ws                 -- WebSocket event stream
pub fn build_router(daemon: DaemonState) -> Router {
    Router::new()
        .route("/api/state", get(web_api::get_state))
        .route("/api/chat", post(web_api::send_message))
        .route("/api/panels/{panel_id}", get(web_api::get_panel))
        .route("/api/agents", post(web_api::agent_action))
        .route("/ws", any(web_api::websocket_handler))
        .with_state(daemon)
}
