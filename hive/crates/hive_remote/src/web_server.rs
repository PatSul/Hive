use crate::web_api::{self, DaemonState};
use axum::Router;
use axum::routing::{any, get, post};

// ---------------------------------------------------------------------------
// Static asset handlers — serve the embedded web UI
// ---------------------------------------------------------------------------

/// GET / -- serve the main HTML page.
pub async fn serve_index() -> axum::response::Html<&'static str> {
    axum::response::Html(include_str!("../web/index.html"))
}

/// GET /style.css -- serve the stylesheet.
pub async fn serve_css() -> (
    [(axum::http::header::HeaderName, &'static str); 1],
    &'static str,
) {
    (
        [(axum::http::header::CONTENT_TYPE, "text/css")],
        include_str!("../web/style.css"),
    )
}

/// GET /app.js -- serve the client-side JavaScript.
pub async fn serve_js() -> (
    [(axum::http::header::HeaderName, &'static str); 1],
    &'static str,
) {
    (
        [(axum::http::header::CONTENT_TYPE, "application/javascript")],
        include_str!("../web/app.js"),
    )
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

pub fn build_router(daemon: DaemonState) -> Router {
    Router::new()
        .route("/", get(serve_index))
        .route("/style.css", get(serve_css))
        .route("/app.js", get(serve_js))
        .route("/api/state", get(web_api::get_state))
        .route("/api/chat", post(web_api::send_message))
        .route("/api/navigation", post(web_api::navigate_shell))
        .route("/api/home/launch", post(web_api::launch_home_mission))
        .route("/api/workspaces/switch", post(web_api::switch_workspace))
        .route("/api/conversations/resume", post(web_api::resume_conversation))
        .route(
            "/api/approvals/{request_id}/decision",
            post(web_api::approval_decision),
        )
        .route("/api/panels/{panel_id}", get(web_api::get_panel))
        .route("/api/agents", post(web_api::agent_action))
        .route("/api/agents/{run_id}/cancel", post(web_api::agent_cancel))
        .route("/api/files/navigate", post(web_api::file_navigate))
        .route("/api/files/open", post(web_api::file_open))
        .route("/api/specs/select", post(web_api::spec_select))
        .route("/api/git/stage-all", post(web_api::git_stage_all))
        .route("/api/git/unstage-all", post(web_api::git_unstage_all))
        .route("/api/git/commit", post(web_api::git_commit))
        .route("/api/terminal/start", post(web_api::terminal_start))
        .route("/api/terminal/send", post(web_api::terminal_send))
        .route("/api/terminal/clear", post(web_api::terminal_clear))
        .route("/api/terminal/kill", post(web_api::terminal_kill))
        .route("/api/workflows/run", post(web_api::workflow_run))
        .route("/api/channels/select", post(web_api::channel_select))
        .route("/api/channels/message", post(web_api::channel_message))
        .route(
            "/api/assistant/approvals/{approval_id}/decision",
            post(web_api::assistant_decision),
        )
        .route("/api/settings/update", post(web_api::settings_update))
        .route("/api/settings/text", post(web_api::settings_text))
        .route("/api/models/default", post(web_api::models_default))
        .route("/api/routing/update", post(web_api::routing_update))
        .route("/api/providers/key", post(web_api::provider_key_update))
        .route(
            "/api/routing/project-models/add",
            post(web_api::routing_project_model_add),
        )
        .route(
            "/api/routing/project-models/remove",
            post(web_api::routing_project_model_remove),
        )
        .route("/api/skills/toggle", post(web_api::skills_toggle))
        .route("/api/skills/install", post(web_api::skills_install))
        .route("/api/skills/remove", post(web_api::skills_remove))
        .route("/ws", any(web_api::websocket_handler))
        .with_state(daemon)
}
