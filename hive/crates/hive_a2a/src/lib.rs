//! hive_a2a — A2A (Agent-to-Agent) protocol support for Hive.
//!
//! Enables bidirectional interoperability with external AI agents:
//! - Server: Exposes HiveMind/Coordinator/Queen as A2A skills
//! - Client: Discovers and delegates tasks to external A2A agents

pub mod config;
pub mod error;

pub mod auth;

// Modules added in subsequent tasks:
pub mod agent_card;
pub mod bridge;
pub mod client;
pub mod remote_agent;
pub mod task_handler;
// pub mod streaming;
// pub mod server;
