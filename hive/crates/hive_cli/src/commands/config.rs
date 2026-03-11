//! hive config command handler.

use crate::ui;
use anyhow::{bail, Result};

pub async fn run(key: Option<String>, value: Option<String>) -> Result<()> {
    let mut config = hive_core::HiveConfig::load()?;
    match (key.as_deref(), value.as_deref()) {
        (None, _) => {
            ui::print_header("Hive Configuration");
            let json = serde_json::to_string_pretty(&config)?;
            println!("{}", json);
            println!();
            println!(
                "  Config file: {}",
                hive_core::HiveConfig::config_path()?.display()
            );
            println!();
        }
        (Some(k), None) => {
            let json = serde_json::to_value(&config)?;
            match json.get(k) {
                Some(v) => println!("  {} = {}", k, v),
                None => println!("  Unknown config key: {}", k),
            }
        }
        (Some(k), Some(v)) => {
            // Validate URL-type config keys before setting
            if matches!(k, "cloud_api_url" | "cloud_relay_url") {
                if !v.is_empty() && url::Url::parse(v).is_err() {
                    bail!("Invalid URL for {k}: {v}");
                }
            }

            let mut json = serde_json::to_value(&config)?;
            if let Some(obj) = json.as_object_mut() {
                if !obj.contains_key(k) {
                    bail!("Unknown config key: {}", k);
                }
                let existing = &obj[k];
                let new_value = if existing.is_boolean() {
                    match v.to_lowercase().as_str() {
                        "true" | "1" | "yes" => serde_json::Value::Bool(true),
                        "false" | "0" | "no" => serde_json::Value::Bool(false),
                        _ => bail!("Expected boolean for key: {}", k),
                    }
                } else if existing.is_u64() || existing.is_i64() {
                    let n: i64 = v.parse().map_err(|_| anyhow::anyhow!("Expected integer"))?;
                    serde_json::Value::Number(serde_json::Number::from(n))
                } else if existing.is_f64() {
                    let n: f64 = v.parse().map_err(|_| anyhow::anyhow!("Expected number"))?;
                    serde_json::json!(n)
                } else {
                    serde_json::Value::String(v.to_string())
                };
                obj.insert(k.to_string(), new_value);
                config = serde_json::from_value(json)?;
                config.save()?;
                println!("  Set {} = {}", k, v);
            }
        }
    }
    Ok(())
}
