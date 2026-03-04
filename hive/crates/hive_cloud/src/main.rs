use axum::{routing::get, Router};
use std::net::SocketAddr;
use tracing::{info, Level};

mod auth;
mod billing;
mod relay;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .init();

    info!("Starting hive-cloud backend...");

    // Build our application with routes
    let app = Router::new()
        .route("/", get(|| async { "Hive Cloud API v1" }))
        .nest("/relay", relay::router());

    // Run it with hyper on localhost:3000
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    info!("listening on {}", addr);
    
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
