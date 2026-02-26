use crate::web_api::{self, DaemonState};
use axum::routing::{any, get, post};
use axum::Router;

// ---------------------------------------------------------------------------
// Static asset handlers — serve the embedded web UI
// ---------------------------------------------------------------------------

/// GET / -- serve the main HTML page.
pub async fn serve_index() -> axum::response::Html<&'static str> {
    axum::response::Html(include_str!("../web/index.html"))
}

/// GET /style.css -- serve the stylesheet.
pub async fn serve_css() -> ([(axum::http::header::HeaderName, &'static str); 1], &'static str) {
    (
        [(axum::http::header::CONTENT_TYPE, "text/css")],
        include_str!("../web/style.css"),
    )
}

/// GET /app.js -- serve the client-side JavaScript.
pub async fn serve_js() -> ([(axum::http::header::HeaderName, &'static str); 1], &'static str) {
    (
        [(axum::http::header::CONTENT_TYPE, "application/javascript")],
        include_str!("../web/app.js"),
    )
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

/// Build the axum router with all API, WebSocket, and static asset routes.
///
/// Routes:
/// - GET  /                   -- embedded web UI (index.html)
/// - GET  /style.css          -- embedded stylesheet
/// - GET  /app.js             -- embedded client JavaScript
/// - GET  /api/state          -- current session snapshot
/// - POST /api/chat           -- send a chat message
/// - GET  /api/panels/{id}    -- panel-specific data
/// - POST /api/agents         -- start/cancel agent tasks
/// - GET  /ws                 -- WebSocket event stream
pub fn build_router(daemon: DaemonState) -> Router {
    Router::new()
        .route("/", get(serve_index))
        .route("/style.css", get(serve_css))
        .route("/app.js", get(serve_js))
        .route("/api/state", get(web_api::get_state))
        .route("/api/chat", post(web_api::send_message))
        .route("/api/panels/{panel_id}", get(web_api::get_panel))
        .route("/api/agents", post(web_api::agent_action))
        .route("/ws", any(web_api::websocket_handler))
        .with_state(daemon)
}
