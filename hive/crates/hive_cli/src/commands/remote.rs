//! hive remote command handler.

use anyhow::Result;
use crate::ui;

pub async fn run() -> Result<()> {
    let config = hive_core::HiveConfig::load()?;
    ui::print_header("Remote Status");
    println!("  Enabled:    {}", config.remote_enabled);
    println!("  Local port: {}", config.remote_local_port);
    println!("  Web port:   {}", config.remote_web_port);
    println!("  Auto-start: {}", config.remote_auto_start);
    println!();
    if let Some(ref relay) = config.cloud_relay_url {
        println!("  Cloud Relay: {}", relay);
    } else {
        println!("  Cloud Relay: (not configured)");
    }
    if config.cloud_jwt.is_some() {
        println!("  Auth:        authenticated");
    } else {
        println!("  Auth:        not logged in");
    }
    println!();
    Ok(())
}
