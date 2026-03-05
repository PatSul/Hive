//! hive_a2a — A2A (Agent-to-Agent) protocol support for Hive.
//!
//! Enables bidirectional interoperability with external AI agents:
//! - Server: Exposes HiveMind/Coordinator/Queen as A2A skills
//! - Client: Discovers and delegates tasks to external A2A agents

pub mod agent_card;
pub mod auth;
pub mod bridge;
pub mod client;
pub mod config;
pub mod error;
pub mod remote_agent;
pub mod server;
pub mod streaming;
pub mod task_handler;

// Re-exports for convenience
pub use client::{discover_agent, DiscoveryCache};
pub use config::A2aConfig;
pub use error::A2aError;
pub use remote_agent::RemoteAgent;
pub use server::start_server;
