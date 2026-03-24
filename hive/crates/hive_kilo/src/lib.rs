//! `hive_kilo` — Kilo.ai coding-agent integration for Hive.
//!
//! Kilo is an open-source (Apache 2.0) AI coding agent that exposes a local
//! HTTP REST API on `localhost:4096`.  This crate bridges Kilo into Hive's
//! multi-layer AI architecture:
//!
//! | Layer | Type | Plugs into |
//! |---|---|---|
//! | Provider | [`provider::KiloAiProvider`] | `hive_ai::service::AiService` |
//! | Executor | [`executor::KiloAiExecutor`] | `hive_agents::hivemind::HiveMind` |
//! | MCP bridge | [`mcp_bridge::KiloMcpBridge`] | `hive_agents::tool_use` |
//! | PTY bridge | [`pty_bridge::KiloPtyBridge`] | `hive_terminal` (future) |
//!
//! # Quick start
//!
//! **Register the provider** from `hive_app` (avoids circular crate deps):
//!
//! ```ignore
//! use std::sync::Arc;
//! use hive_ai::types::ProviderType;
//! use hive_kilo::provider::KiloAiProvider;
//!
//! // In AiService setup (hive_app wiring):
//! let kilo = Arc::new(KiloAiProvider::new(
//!     Some("http://localhost:4096"),
//!     None, // no password
//! ));
//! ai_service.register_external_provider(ProviderType::Kilo, kilo);
//! ```
//!
//! **Use the executor** for multi-role HiveMind tasks:
//!
//! ```ignore
//! use hive_kilo::{config::KiloConfig, executor::KiloAiExecutor};
//! use hive_agents::hivemind::HiveMind;
//!
//! let exec = KiloAiExecutor::new(KiloConfig::default());
//! let hivemind = HiveMind::new(config, exec);
//! ```
//!
//! # Dependency note
//!
//! `hive_kilo` depends on `hive_ai` for the `AiProvider` trait and on
//! `hive_agents` for `AiExecutor`.  **`hive_ai` must NOT depend on `hive_kilo`**
//! to avoid a circular dependency.  Provider registration always happens in
//! `hive_app`.

pub mod client;
pub mod config;
pub mod error;
pub mod events;
pub mod executor;
pub mod mcp_bridge;
pub mod provider;
pub mod pty_bridge;
pub mod session;

// Convenience re-exports.
pub use client::KiloClient;
pub use config::KiloConfig;
pub use error::{KiloError, KiloResult};
pub use executor::KiloAiExecutor;
pub use mcp_bridge::KiloMcpBridge;
pub use provider::KiloAiProvider;
pub use pty_bridge::KiloPtyBridge;
pub use session::KiloSession;
