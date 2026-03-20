//! hive sync command handlers.

use crate::api::CloudClient;
use crate::ui;
use anyhow::{Context, Result};

pub async fn push(key: &str, file_path: &str) -> Result<()> {
    let canonical = std::path::Path::new(file_path)
        .canonicalize()
        .with_context(|| format!("Invalid path: {file_path}"))?;
    let path_str = canonical.to_string_lossy().to_lowercase();
    for seg in &[
        ".ssh",
        ".aws",
        ".gnupg",
        ".config/gcloud",
        ".config\\gcloud",
    ] {
        if path_str.contains(seg) {
            anyhow::bail!("Access to sensitive path blocked: {seg}");
        }
    }

    let config = hive_core::HiveConfig::load()?;
    let client = CloudClient::new(config.cloud_api_url.as_deref(), config.cloud_jwt.as_deref());
    let data = std::fs::read(&canonical)
        .with_context(|| format!("Failed to read file: {}", canonical.display()))?;
    println!(
        "  Pushing {} ({} bytes) as \"{}\"...",
        canonical.display(),
        data.len(),
        key
    );
    client.sync_push(key, &data).await?;
    println!("  Done. Blob uploaded successfully.");
    Ok(())
}

pub async fn pull(key: &str, file_path: &str) -> Result<()> {
    // For pull, the file may not exist yet, so canonicalize the parent directory
    let path = std::path::Path::new(file_path);
    let parent = path.parent().unwrap_or_else(|| std::path::Path::new("."));
    let canonical_parent = parent
        .canonicalize()
        .with_context(|| format!("Invalid parent directory: {}", parent.display()))?;
    let canonical = canonical_parent.join(
        path.file_name()
            .ok_or_else(|| anyhow::anyhow!("Invalid file path: {file_path}"))?,
    );
    let path_str = canonical.to_string_lossy().to_lowercase();
    for seg in &[
        ".ssh",
        ".aws",
        ".gnupg",
        ".config/gcloud",
        ".config\\gcloud",
    ] {
        if path_str.contains(seg) {
            anyhow::bail!("Access to sensitive path blocked: {seg}");
        }
    }

    let config = hive_core::HiveConfig::load()?;
    let client = CloudClient::new(config.cloud_api_url.as_deref(), config.cloud_jwt.as_deref());
    println!("  Pulling \"{}\"...", key);
    let data = client.sync_pull(key).await?;
    std::fs::write(&canonical, &data)
        .with_context(|| format!("Failed to write file: {}", canonical.display()))?;
    println!(
        "  Done. Saved {} bytes to {}",
        data.len(),
        canonical.display()
    );
    Ok(())
}

pub async fn status() -> Result<()> {
    let config = hive_core::HiveConfig::load()?;
    let client = CloudClient::new(config.cloud_api_url.as_deref(), config.cloud_jwt.as_deref());
    ui::print_header("Sync Status");
    match client.sync_manifest().await {
        Ok(manifest) => {
            let total_mb = manifest.total_size_bytes as f64 / (1024.0 * 1024.0);
            let limit_mb = manifest.storage_limit_bytes as f64 / (1024.0 * 1024.0);
            println!("  Storage: {:.2} MB / {:.2} MB", total_mb, limit_mb);
            println!("  Blobs:   {}", manifest.blobs.len());
            println!();
            if manifest.blobs.is_empty() {
                println!("  No blobs stored.");
            } else {
                println!("  {:<30} {:>10} {:<20}", "KEY", "SIZE", "UPDATED");
                println!("  {}", "-".repeat(62));
                for blob in &manifest.blobs {
                    let size = format_bytes(blob.size_bytes);
                    println!("  {:<30} {:>10} {:<20}", blob.key, size, blob.updated_at);
                }
            }
            println!();
        }
        Err(e) => println!("  Failed to get sync status: {}", e),
    }
    Ok(())
}

fn format_bytes(bytes: i64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}
