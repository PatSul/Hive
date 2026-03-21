use anyhow::Result;
use hive_remote::daemon::{DaemonConfig, HiveDaemon};
use hive_remote::web_server::build_router;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

#[tokio::main]
async fn main() -> Result<()> {
    let root = std::env::var("HIVE_REMOTE_PREVIEW_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(".hive-preview"));
    let data_dir = root.join("data");
    let config_root = root.join("config");
    let web_port = std::env::var("HIVE_REMOTE_PREVIEW_PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(9491);

    std::fs::create_dir_all(&data_dir)?;
    std::fs::create_dir_all(&config_root)?;

    let daemon = HiveDaemon::new(DaemonConfig {
        data_dir,
        config_root: Some(config_root),
        local_port: 9480,
        web_port,
        shutdown_grace_secs: 30,
    })?;
    let router = build_router(Arc::new(RwLock::new(daemon)));
    let addr = format!("127.0.0.1:{web_port}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    println!("HIVE_REMOTE_PREVIEW_URL=http://{addr}");
    axum::serve(listener, router).await?;
    Ok(())
}
