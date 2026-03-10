use axum::{Router, routing::get};
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::{Level, info};

mod admin;
mod auth;
#[cfg(test)]
mod billing;
mod relay;

fn build_app() -> Router {
    let relay_service = Arc::new(relay::RelayService::default());
    let admin_state = Arc::new(admin::AdminState::new(relay_service.clone()));

    Router::new()
        .route("/", get(|| async { "Hive Cloud API v1" }))
        .nest("/relay", relay::router(relay_service))
        .nest("/admin", admin::router(admin_state))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    info!("Starting hive-cloud backend...");

    let app = build_app();

    // Run it with hyper on localhost:3000
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    info!("listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
