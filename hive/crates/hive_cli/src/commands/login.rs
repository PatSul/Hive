//! hive login command.

use anyhow::Result;
use std::io::{self, Write};
use crate::api::CloudClient;
use crate::ui;

pub async fn run() -> Result<()> {
    ui::print_login_banner();
    print!("  Email: ");
    io::stdout().flush()?;
    let mut email = String::new();
    io::stdin().read_line(&mut email)?;
    let email = email.trim();
    if email.is_empty() {
        println!("  Aborted: no email provided.");
        return Ok(());
    }
    println!("  Authenticating...");
    let config = hive_core::HiveConfig::load()?;
    let client = CloudClient::new(config.cloud_api_url.as_deref(), None);
    match client.login(email).await {
        Ok(tokens) => {
            let mut config = hive_core::HiveConfig::load()?;
            config.cloud_jwt = Some(tokens.access_token);
            config.save()?;
            println!("  Login successful! JWT saved to ~/.hive/config.json");
            println!("  Token expires in {} seconds.", tokens.expires_in);
            println!();
        }
        Err(e) => {
            println!("  Login failed: {}", e);
            println!();
            println!("  Make sure the Hive Cloud server is running and reachable.");
        }
    }
    Ok(())
}
