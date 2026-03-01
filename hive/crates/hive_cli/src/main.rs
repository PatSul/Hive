//! Hive AI terminal client.
#![allow(dead_code)]
//!
//! A Ratatui-based CLI for interacting with Hive Cloud services: login,
//! chat with AI models, sync data, and manage configuration.

mod api;
mod app;
mod commands;
mod ui;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "hive", version, about = "Hive AI terminal client")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Authenticate with Hive Cloud
    Login,
    /// Show account info, tier, usage, and sync status
    Status,
    /// Interactive TUI chat with AI
    Chat {
        /// Model to use (e.g. gpt-4o, claude-3-sonnet)
        #[arg(short, long)]
        model: Option<String>,
    },
    /// Cloud sync operations
    Sync {
        #[command(subcommand)]
        action: SyncAction,
    },
    /// Show remote connection status
    Remote,
    /// Show or edit config values
    Config {
        /// Config key to get or set
        key: Option<String>,
        /// Value to set (omit to read)
        value: Option<String>,
    },
    /// List available AI models
    Models,
}

#[derive(Subcommand)]
enum SyncAction {
    /// Push a blob to cloud storage
    Push {
        /// Blob key
        key: String,
        /// Path to file to upload
        file: String,
    },
    /// Pull a blob from cloud storage
    Pull {
        /// Blob key
        key: String,
        /// Path to save downloaded file
        file: String,
    },
    /// Show sync manifest (all stored blobs)
    Status,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Login => commands::login::run().await,
        Commands::Status => commands::status::run().await,
        Commands::Chat { model } => commands::chat::run(model).await,
        Commands::Sync { action } => match action {
            SyncAction::Push { key, file } => commands::sync::push(&key, &file).await,
            SyncAction::Pull { key, file } => commands::sync::pull(&key, &file).await,
            SyncAction::Status => commands::sync::status().await,
        },
        Commands::Remote => commands::remote::run().await,
        Commands::Config { key, value } => commands::config::run(key, value).await,
        Commands::Models => commands::models::run().await,
    }
}
