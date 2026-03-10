//! hive status command.

use crate::api::CloudClient;
use crate::ui;
use anyhow::Result;

pub async fn run() -> Result<()> {
    let config = hive_core::HiveConfig::load()?;
    let client = CloudClient::new(config.cloud_api_url.as_deref(), config.cloud_jwt.as_deref());
    ui::print_header("Hive Status");
    match client.get_account().await {
        Ok(acct) => {
            println!("  Account:  {}", acct.email);
            if let Some(name) = &acct.display_name {
                println!("  Name:     {}", name);
            }
            println!("  Tier:     {}", acct.tier);
            println!("  ID:       {}", acct.id);
            if let Some(exp) = &acct.subscription_expires_at {
                println!("  Expires:  {}", exp);
            }
            println!("  Created:  {}", acct.created_at);
        }
        Err(e) => println!("  Could not fetch account info: {}", e),
    }
    println!();
    match client.get_usage().await {
        Ok(usage) => {
            let budget = usage.token_budget_cents as f64 / 100.0;
            let used = usage.token_used_cents as f64 / 100.0;
            let remaining = usage.token_remaining_cents as f64 / 100.0;
            println!("  Budget:    ${:.2}", budget);
            println!("  Used:      ${:.2} ({:.1}%)", used, usage.usage_percent);
            println!("  Remaining: ${:.2}", remaining);
            println!("  Resets:    {}", usage.budget_reset_at);
        }
        Err(e) => println!("  Could not fetch usage: {}", e),
    }
    println!();
    println!("  Config:");
    println!(
        "    Cloud API:   {}",
        config.cloud_api_url.as_deref().unwrap_or("(default)")
    );
    println!(
        "    Cloud Relay: {}",
        config.cloud_relay_url.as_deref().unwrap_or("(not set)")
    );
    println!(
        "    JWT:         {}",
        if config.cloud_jwt.is_some() {
            "set"
        } else {
            "not set"
        }
    );
    println!("    Model:       {}", config.default_model);
    println!();
    Ok(())
}
