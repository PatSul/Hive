//! Bridge between Kilo's `/mcp` endpoint and Hive's tool-use layer.
//!
//! Kilo manages a set of Model Context Protocol (MCP) servers on the user's
//! behalf.  Rather than having Hive connect to those MCP servers directly,
//! `KiloMcpBridge` queries Kilo for the list of available tools and delegates
//! tool execution back through Kilo — keeping all MCP lifecycle management in
//! one place.
//!
//! # Usage
//!
//! ```ignore
//! let bridge = KiloMcpBridge::new(kilo_client.clone());
//! let tools: Vec<ToolDefinition> = bridge.available_tool_definitions().await?;
//! // Register `tools` with hive_agents::tool_use for the current HiveMind run.
//! ```
//!
//! # Integration pattern
//!
//! Hive's `tool_use` module currently expects `Vec<ToolDefinition>` (from
//! `hive_ai::types`).  `KiloMcpBridge` converts `KiloMcpTool` → `ToolDefinition`
//! with all MCP tools namespaced as `kilo_mcp::{server}::{tool}` to avoid
//! collisions with Hive's native tools.

use std::sync::Arc;

use serde_json::json;
use tracing::{debug, warn};

use hive_ai::types::ToolDefinition;

use crate::client::{KiloClient, KiloMcpServer, KiloMcpTool};
use crate::error::{KiloError, KiloResult};

// ---------------------------------------------------------------------------
// KiloMcpBridge
// ---------------------------------------------------------------------------

/// Bridges Kilo-managed MCP servers into Hive's tool registry.
pub struct KiloMcpBridge {
    client: Arc<KiloClient>,
}

impl KiloMcpBridge {
    /// Create a bridge backed by the given [`KiloClient`].
    pub fn new(client: Arc<KiloClient>) -> Self {
        Self { client }
    }

    // -----------------------------------------------------------------------
    // Tool discovery
    // -----------------------------------------------------------------------

    /// Fetch all MCP servers and their tools from Kilo.
    pub async fn list_servers(&self) -> KiloResult<Vec<KiloMcpServer>> {
        self.client.list_mcp_servers().await
    }

    /// Return all available tools across all Kilo-managed MCP servers as
    /// Hive [`ToolDefinition`]s, namespaced as `kilo_mcp::{server}::{tool}`.
    pub async fn available_tool_definitions(&self) -> KiloResult<Vec<ToolDefinition>> {
        let servers = self.list_servers().await?;
        let mut defs: Vec<ToolDefinition> = Vec::new();

        for server in &servers {
            for tool in &server.tools {
                defs.push(self.to_tool_definition(&server.name, tool));
            }
        }

        debug!("KiloMcpBridge: {} tools discovered", defs.len());
        Ok(defs)
    }

    /// Convert a single MCP tool into a Hive `ToolDefinition`.
    ///
    /// The tool name is namespaced as `kilo_mcp::{server_name}::{tool_name}` to
    /// avoid collisions with Hive's built-in tools.
    fn to_tool_definition(&self, server_name: &str, tool: &KiloMcpTool) -> ToolDefinition {
        // Sanitise server/tool names for use as a composite tool ID.
        let safe_server = server_name.replace(['/', '-', ' '], "_");
        let safe_tool = tool.name.replace(['/', '-', ' '], "_");
        let name = format!("kilo_mcp__{safe_server}__{safe_tool}");

        let description = tool
            .description
            .clone()
            .unwrap_or_else(|| format!("MCP tool '{}' from server '{}'", tool.name, server_name));

        let input_schema = tool.input_schema.clone().unwrap_or_else(|| {
            json!({
                "type": "object",
                "properties": {},
                "required": []
            })
        });

        ToolDefinition {
            name,
            description,
            input_schema,
        }
    }

    // -----------------------------------------------------------------------
    // Tool execution
    // -----------------------------------------------------------------------

    /// Execute a namespaced Kilo MCP tool by calling back through the Kilo
    /// session.
    ///
    /// The `tool_name` must be in the `kilo_mcp::{server}::{tool}` format
    /// produced by [`available_tool_definitions`].
    ///
    /// # Arguments
    ///
    /// * `session_id` — The Kilo session within which the tool call should be
    ///   executed.  Pass the ID of the current HiveMind session.
    /// * `tool_name` — Namespaced tool name (e.g. `kilo_mcp__filesystem__read_file`).
    /// * `input` — Tool input as a JSON object.
    pub async fn call_tool(
        &self,
        session_id: &str,
        tool_name: &str,
        input: serde_json::Value,
    ) -> KiloResult<serde_json::Value> {
        // Decode the namespaced name → (server_name, tool_name).
        let (server_name, bare_tool_name) = parse_namespaced_tool(tool_name)
            .ok_or_else(|| KiloError::Other(format!("Invalid Kilo MCP tool name: {tool_name}")))?;

        // Construct a synthetic tool-call message and send it to the session.
        // Kilo will route the call to the appropriate MCP server.
        let call_id = uuid::Uuid::new_v4().to_string();
        let payload = serde_json::json!({
            "type": "tool_call",
            "id": call_id,
            "server": server_name,
            "name": bare_tool_name,
            "input": input,
        });

        let url = format!(
            "{}/session/{session_id}/tool",
            self.client.config().base_url
        );
        let resp = reqwest::Client::new()
            .post(&url)
            .json(&payload)
            .send()
            .await
            .map_err(KiloError::Network)?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(KiloError::api(status, body));
        }

        Ok(resp.json().await.unwrap_or(serde_json::Value::Null))
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parse a namespaced tool name `kilo_mcp__{server}__{tool}` into
/// `(server, tool)` components.  Returns `None` if the format is unrecognised.
fn parse_namespaced_tool(name: &str) -> Option<(String, String)> {
    let rest = name.strip_prefix("kilo_mcp__")?;
    let mut parts = rest.splitn(2, "__");
    let server = parts.next()?.replace('_', "-");
    let tool = parts.next()?.replace('_', "-");
    Some((server, tool))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::KiloMcpTool;

    #[test]
    fn to_tool_definition_namespaces_correctly() {
        let client = Arc::new(KiloClient::new(Default::default()));
        let bridge = KiloMcpBridge::new(client);
        let tool = KiloMcpTool {
            name: "read_file".into(),
            description: Some("Read a file".into()),
            input_schema: None,
        };
        let def = bridge.to_tool_definition("filesystem", &tool);
        assert_eq!(def.name, "kilo_mcp__filesystem__read_file");
        assert_eq!(def.description, "Read a file");
    }

    #[test]
    fn parse_namespaced_tool_roundtrip() {
        let (server, tool) =
            parse_namespaced_tool("kilo_mcp__filesystem__read_file").unwrap();
        // Underscores are re-mapped to hyphens in the parse path.
        assert_eq!(server, "filesystem");
        assert_eq!(tool, "read-file");
    }

    #[test]
    fn parse_namespaced_tool_invalid_returns_none() {
        assert!(parse_namespaced_tool("not_a_kilo_tool").is_none());
        assert!(parse_namespaced_tool("kilo_mcp__only_one_segment").is_none());
    }
}
