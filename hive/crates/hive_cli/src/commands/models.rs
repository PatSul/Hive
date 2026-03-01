//! hive models command handler.

use anyhow::Result;
use crate::api::CloudClient;
use crate::ui;

pub async fn run() -> Result<()> {
    let config = hive_core::HiveConfig::load()?;
    let client = CloudClient::new(config.cloud_api_url.as_deref(), config.cloud_jwt.as_deref());
    ui::print_header("Available Models");
    match client.list_models().await {
        Ok(models) => {
            println!("  {:<20} {:<25} {:<10} {:>8} {:>8} {:>9}", "ID", "NAME", "PROVIDER", "IN $/M", "OUT $/M", "CTX");
            println!("  {}", "-".repeat(82));
            for m in &models {
                let avail = if m.available { "" } else { "(unavailable)" };
                println!("  {:<20} {:<25} {:<10} {:>8.2} {:>8.2} {:>8}K {}", m.id, m.name, m.provider, m.input_price_per_mtok, m.output_price_per_mtok, m.context_window / 1000, avail);
            }
            println!();
            println!("  {} models total", models.len());
        }
        Err(e) => {
            println!("  Failed to fetch models: {}", e);
            println!();
            println!("  Local default model: {}", config.default_model);
        }
    }
    println!();
    Ok(())
}
