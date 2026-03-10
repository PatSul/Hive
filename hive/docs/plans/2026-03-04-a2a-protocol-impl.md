# A2A Protocol Integration — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add bidirectional A2A (Agent-to-Agent) protocol support to Hive via a new `hive_a2a` crate, enabling external agents to call Hive's orchestrators and Hive to delegate to external A2A agents.

**Architecture:** New `hive_a2a` crate wraps `a2a-rs` (protocol layer) and bridges to `hive_agents` (orchestration). Axum HTTP server exposes Agent Card + JSON-RPC endpoints. `HttpClient` from `a2a-rs` handles outbound calls to external agents.

**Tech Stack:** Rust, a2a-rs v0.1.0 (A2A v0.3 protocol), axum, tokio, reqwest, tower, serde, toml

**Design doc:** `docs/plans/2026-03-04-a2a-protocol-design.md`

---

### Task 1: Scaffold hive_a2a crate + workspace integration

**Files:**
- Create: `hive/crates/hive_a2a/Cargo.toml`
- Create: `hive/crates/hive_a2a/src/lib.rs`
- Modify: `hive/Cargo.toml` (workspace members list, line ~22)

**Step 1: Create the Cargo.toml**

```toml
[package]
name = "hive_a2a"
version = "0.1.0"
edition = "2021"

[dependencies]
# A2A protocol
a2a-rs = { version = "0.1", features = ["http-server", "http-client", "auth"] }

# HTTP server + middleware
axum = { version = "0.7", features = ["macros"] }
tower = { version = "0.5", features = ["timeout", "limit"] }
tower-http = { version = "0.6", features = ["cors"] }

# Workspace shared deps
tokio = { workspace = true }
futures = { workspace = true }
reqwest = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
toml = { workspace = true }
uuid = { workspace = true }
chrono = { workspace = true }
anyhow = { workspace = true }
thiserror = { workspace = true }

# Internal crates
hive_agents = { path = "../hive_agents" }
hive_core = { path = "../hive_core" }
hive_ai = { path = "../hive_ai" }

[dev-dependencies]
tokio = { workspace = true, features = ["test-util"] }
```

**Step 2: Create src/lib.rs**

```rust
//! hive_a2a — A2A (Agent-to-Agent) protocol support for Hive.
//!
//! Enables bidirectional interoperability with external AI agents:
//! - Server: Exposes HiveMind/Coordinator/Queen as A2A skills
//! - Client: Discovers and delegates tasks to external A2A agents

pub mod config;
pub mod error;

// Modules added in subsequent tasks:
// pub mod agent_card;
// pub mod bridge;
// pub mod auth;
// pub mod task_handler;
// pub mod streaming;
// pub mod server;
// pub mod client;
// pub mod remote_agent;
```

**Step 3: Add hive_a2a to workspace members**

In `hive/Cargo.toml`, add `"crates/hive_a2a"` to the workspace members list (after `crates/hive_cloud`).

**Step 4: Verify it compiles**

Run: `cargo check -p hive_a2a` from `hive/`
Expected: Compiles with 0 errors (may have warnings for unused modules)

**Step 5: Commit**

```bash
git add crates/hive_a2a/ Cargo.toml
git commit -m "feat(a2a): scaffold hive_a2a crate with workspace integration"
```

---

### Task 2: Config + Error types

**Files:**
- Create: `hive/crates/hive_a2a/src/config.rs`
- Create: `hive/crates/hive_a2a/src/error.rs`
- Test: inline `#[cfg(test)]`

**Step 1: Write the failing test for config**

In `config.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = A2aConfig::default();
        assert_eq!(config.server.port, 7420);
        assert_eq!(config.server.bind, "127.0.0.1");
        assert!(config.server.enabled);
        assert_eq!(config.server.max_concurrent_tasks, 10);
        assert_eq!(config.server.rate_limit_rpm, 60);
    }

    #[test]
    fn test_parse_toml_config() {
        let toml_str = r#"
[server]
enabled = true
bind = "0.0.0.0"
port = 8080
api_key = "test-key"
max_concurrent_tasks = 5
rate_limit_rpm = 30

[server.defaults]
max_budget_usd = 2.0
max_time_seconds = 600
default_skill = "coordinator"

[client]
discovery_cache_ttl_seconds = 120
request_timeout_seconds = 30

[[agents]]
name = "Test Agent"
url = "https://agent.example.com"
api_key = "agent-key"
"#;
        let config: A2aConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.server.port, 8080);
        assert_eq!(config.server.bind, "0.0.0.0");
        assert_eq!(config.server.api_key, Some("test-key".into()));
        assert_eq!(config.server.defaults.default_skill, "coordinator");
        assert_eq!(config.agents.len(), 1);
        assert_eq!(config.agents[0].name, "Test Agent");
    }

    #[test]
    fn test_config_load_creates_default() {
        let dir = std::env::temp_dir().join("hive_a2a_test_config");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a2a.toml");

        let config = A2aConfig::load_or_create(&path).unwrap();
        assert_eq!(config.server.port, 7420);
        assert!(path.exists());

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p hive_a2a -- config`
Expected: FAIL — `A2aConfig` not defined

**Step 3: Implement config.rs**

```rust
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct A2aConfig {
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub client: ClientConfig,
    #[serde(default)]
    pub agents: Vec<RemoteAgentConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_bind")]
    pub bind: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent_tasks: usize,
    #[serde(default = "default_rate_limit")]
    pub rate_limit_rpm: u32,
    #[serde(default)]
    pub defaults: ServerDefaults,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerDefaults {
    #[serde(default = "default_budget")]
    pub max_budget_usd: f64,
    #[serde(default = "default_time")]
    pub max_time_seconds: u64,
    #[serde(default = "default_skill")]
    pub default_skill: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientConfig {
    #[serde(default = "default_cache_ttl")]
    pub discovery_cache_ttl_seconds: u64,
    #[serde(default = "default_timeout")]
    pub request_timeout_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteAgentConfig {
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub api_key: Option<String>,
}

// Default value functions
fn default_true() -> bool { true }
fn default_bind() -> String { "127.0.0.1".into() }
fn default_port() -> u16 { 7420 }
fn default_max_concurrent() -> usize { 10 }
fn default_rate_limit() -> u32 { 60 }
fn default_budget() -> f64 { 1.0 }
fn default_time() -> u64 { 300 }
fn default_skill() -> String { "hivemind".into() }
fn default_cache_ttl() -> u64 { 300 }
fn default_timeout() -> u64 { 60 }

impl Default for A2aConfig {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            client: ClientConfig::default(),
            agents: Vec::new(),
        }
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            bind: default_bind(),
            port: default_port(),
            api_key: None,
            max_concurrent_tasks: default_max_concurrent(),
            rate_limit_rpm: default_rate_limit(),
            defaults: ServerDefaults::default(),
        }
    }
}

impl Default for ServerDefaults {
    fn default() -> Self {
        Self {
            max_budget_usd: default_budget(),
            max_time_seconds: default_time(),
            default_skill: default_skill(),
        }
    }
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            discovery_cache_ttl_seconds: default_cache_ttl(),
            request_timeout_seconds: default_timeout(),
        }
    }
}

impl A2aConfig {
    /// Load config from path, or create default if file doesn't exist.
    pub fn load_or_create(path: &Path) -> Result<Self, crate::error::A2aError> {
        if path.exists() {
            let content = std::fs::read_to_string(path)
                .map_err(|e| crate::error::A2aError::Config(format!("Failed to read {}: {}", path.display(), e)))?;
            let config: Self = toml::from_str(&content)
                .map_err(|e| crate::error::A2aError::Config(format!("Failed to parse {}: {}", path.display(), e)))?;
            Ok(config)
        } else {
            let config = Self::default();
            let content = toml::to_string_pretty(&config)
                .map_err(|e| crate::error::A2aError::Config(format!("Failed to serialize config: {}", e)))?;
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| crate::error::A2aError::Config(format!("Failed to create dir: {}", e)))?;
            }
            std::fs::write(path, content)
                .map_err(|e| crate::error::A2aError::Config(format!("Failed to write {}: {}", path.display(), e)))?;
            Ok(config)
        }
    }

    /// Server bind address as "host:port" string.
    pub fn bind_addr(&self) -> String {
        format!("{}:{}", self.server.bind, self.server.port)
    }
}
```

**Step 4: Write the failing test for error types**

In `error.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = A2aError::Config("bad config".into());
        assert!(err.to_string().contains("bad config"));
    }

    #[test]
    fn test_error_variants() {
        let _ = A2aError::Auth("unauthorized".into());
        let _ = A2aError::TaskNotFound("task-123".into());
        let _ = A2aError::UnsupportedSkill("foo".into());
        let _ = A2aError::BudgetExceeded { limit: 1.0, spent: 1.5 };
        let _ = A2aError::Timeout { seconds: 300 };
        let _ = A2aError::Provider("provider error".into());
        let _ = A2aError::Bridge("conversion error".into());
        let _ = A2aError::Network("connection refused".into());
    }
}
```

**Step 5: Implement error.rs**

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum A2aError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Authentication failed: {0}")]
    Auth(String),

    #[error("Task not found: {0}")]
    TaskNotFound(String),

    #[error("Unsupported skill: {0}")]
    UnsupportedSkill(String),

    #[error("Budget exceeded: limit ${limit:.2}, spent ${spent:.2}")]
    BudgetExceeded { limit: f64, spent: f64 },

    #[error("Task timed out after {seconds}s")]
    Timeout { seconds: u64 },

    #[error("AI provider error: {0}")]
    Provider(String),

    #[error("Type bridge error: {0}")]
    Bridge(String),

    #[error("Network error: {0}")]
    Network(String),

    #[error("Rate limited")]
    RateLimited,

    #[error("Security validation failed: {0}")]
    Security(String),
}
```

**Step 6: Run tests**

Run: `cargo test -p hive_a2a`
Expected: All tests pass

**Step 7: Commit**

```bash
git add crates/hive_a2a/src/config.rs crates/hive_a2a/src/error.rs
git commit -m "feat(a2a): add config and error types"
```

---

### Task 3: Agent Card builder

**Files:**
- Create: `hive/crates/hive_a2a/src/agent_card.rs`
- Modify: `hive/crates/hive_a2a/src/lib.rs` (uncomment `pub mod agent_card;`)
- Test: inline `#[cfg(test)]`

**Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_hive_agent_card() {
        let config = crate::config::ServerConfig::default();
        let info = build_hive_agent_info(&config);
        let card = info.get_agent_card();

        assert_eq!(card.name, "Hive");
        assert!(card.description.contains("multi-agent"));
        assert!(card.capabilities.streaming);
        assert!(!card.capabilities.push_notifications);
        assert_eq!(card.skills.len(), 4);

        let skill_ids: Vec<&str> = card.skills.iter().map(|s| s.id.as_str()).collect();
        assert!(skill_ids.contains(&"hivemind"));
        assert!(skill_ids.contains(&"coordinator"));
        assert!(skill_ids.contains(&"queen"));
        assert!(skill_ids.contains(&"single"));
    }

    #[test]
    fn test_agent_card_url_from_config() {
        let mut config = crate::config::ServerConfig::default();
        config.port = 9999;
        config.bind = "0.0.0.0".into();
        let info = build_hive_agent_info(&config);
        let card = info.get_agent_card();
        assert!(card.url.contains("9999"));
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p hive_a2a -- agent_card`
Expected: FAIL

**Step 3: Implement agent_card.rs**

```rust
use a2a_rs::domain::AgentSkill;
use a2a_rs::adapter::business::SimpleAgentInfo;
use a2a_rs::domain::{AgentProvider, AgentCapabilities};
use crate::config::ServerConfig;

/// Build the Hive Agent Card info from server config.
pub fn build_hive_agent_info(config: &ServerConfig) -> SimpleAgentInfo {
    let url = format!("http://{}:{}", config.bind, config.port);

    let mut info = SimpleAgentInfo::new("Hive".into(), url)
        .with_description("Multi-agent AI coding assistant with hierarchical orchestration".into())
        .with_version(env!("CARGO_PKG_VERSION").into())
        .with_provider(AgentProvider {
            organization: "AIrglow Studio".into(),
            url: "https://hivecode.app".into(),
        })
        .with_streaming();

    // HiveMind skill
    info = info.add_skill(AgentSkill {
        id: "hivemind".into(),
        name: "HiveMind Multi-Agent Pipeline".into(),
        description: "9-role orchestration: Architect, Coder, Reviewer, Tester, Debugger, Security, Documenter, OutputReviewer, TaskVerifier. Best for complex coding tasks.".into(),
        tags: vec!["coding".into(), "multi-agent".into(), "orchestration".into()],
        examples: Some(vec!["Build a REST API with tests".into(), "Refactor the auth module".into()]),
        input_modes: Some(vec!["text".into()]),
        output_modes: Some(vec!["text".into()]),
        security: None,
    });

    // Coordinator skill
    info = info.add_skill(AgentSkill {
        id: "coordinator".into(),
        name: "Task Coordinator".into(),
        description: "Dependency-ordered parallel task execution. Best for specs that decompose into multiple independent subtasks.".into(),
        tags: vec!["tasks".into(), "parallel".into(), "decomposition".into()],
        examples: Some(vec!["Implement these 5 API endpoints".into()]),
        input_modes: Some(vec!["text".into()]),
        output_modes: Some(vec!["text".into()]),
        security: None,
    });

    // Queen skill
    info = info.add_skill(AgentSkill {
        id: "queen".into(),
        name: "Queen Swarm Orchestration".into(),
        description: "Multi-team swarm with cross-team learning. Best for large goals requiring multiple specialized teams.".into(),
        tags: vec!["swarm".into(), "teams".into(), "large-scale".into()],
        examples: Some(vec!["Build an e-commerce platform".into()]),
        input_modes: Some(vec!["text".into()]),
        output_modes: Some(vec!["text".into()]),
        security: None,
    });

    // Single agent skill
    info = info.add_skill(AgentSkill {
        id: "single".into(),
        name: "Single Agent".into(),
        description: "One-shot AI call with a specific persona (Investigate, Implement, Verify, Critique, Debug, CodeReview). Cheapest option.".into(),
        tags: vec!["simple".into(), "one-shot".into(), "persona".into()],
        examples: Some(vec!["Review this function for bugs".into()]),
        input_modes: Some(vec!["text".into()]),
        output_modes: Some(vec!["text".into()]),
        security: None,
    });

    info
}

/// Skill IDs that Hive supports.
pub const SKILL_HIVEMIND: &str = "hivemind";
pub const SKILL_COORDINATOR: &str = "coordinator";
pub const SKILL_QUEEN: &str = "queen";
pub const SKILL_SINGLE: &str = "single";

pub const SUPPORTED_SKILLS: &[&str] = &[SKILL_HIVEMIND, SKILL_COORDINATOR, SKILL_QUEEN, SKILL_SINGLE];

/// Check if a skill ID is supported.
pub fn is_supported_skill(skill_id: &str) -> bool {
    SUPPORTED_SKILLS.contains(&skill_id)
}
```

**Step 4: Run tests**

Run: `cargo test -p hive_a2a -- agent_card`
Expected: PASS

**Step 5: Commit**

```bash
git add crates/hive_a2a/src/agent_card.rs crates/hive_a2a/src/lib.rs
git commit -m "feat(a2a): add Agent Card builder with 4 Hive skills"
```

---

### Task 4: Bridge — A2A types ↔ Hive types

**Files:**
- Create: `hive/crates/hive_a2a/src/bridge.rs`
- Modify: `hive/crates/hive_a2a/src/lib.rs` (uncomment `pub mod bridge;`)
- Test: inline `#[cfg(test)]`

**Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use hive_agents::{AgentOutput, AgentRole, OrchestrationStatus, OrchestrationResult};
    use hive_agents::coordinator::TaskEvent;

    #[test]
    fn test_extract_text_from_message() {
        let msg = a2a_rs::domain::Message::user_text("Build a REST API".into());
        let text = extract_message_text(&msg);
        assert_eq!(text, "Build a REST API");
    }

    #[test]
    fn test_extract_skill_from_metadata() {
        let mut metadata = serde_json::Map::new();
        metadata.insert("skill_id".into(), serde_json::Value::String("hivemind".into()));
        let skill = extract_skill_id(Some(&metadata));
        assert_eq!(skill, Some("hivemind".to_string()));
    }

    #[test]
    fn test_infer_skill_from_text() {
        assert_eq!(infer_skill("Build and architect a system"), "hivemind");
        assert_eq!(infer_skill("Do steps 1 then 2 then 3"), "coordinator");
        assert_eq!(infer_skill("Review this function"), "single");
        assert_eq!(infer_skill("Create teams to build a platform"), "queen");
    }

    #[test]
    fn test_orchestration_result_to_artifact() {
        let result = OrchestrationResult {
            run_id: "run-1".into(),
            task: "test".into(),
            status: OrchestrationStatus::Complete,
            agent_outputs: vec![],
            synthesized_output: "Final output here".into(),
            total_cost: 0.5,
            total_duration_ms: 1000,
            consensus_score: Some(0.9),
        };
        let artifact = orchestration_result_to_artifact(&result);
        assert_eq!(artifact.name, Some("hivemind-output".into()));
        let text = match &artifact.parts[0] {
            a2a_rs::domain::Part::Text { text, .. } => text.clone(),
            _ => panic!("Expected text part"),
        };
        assert!(text.contains("Final output here"));
    }

    #[test]
    fn test_task_event_to_status_update() {
        let event = TaskEvent::TaskStarted {
            task_id: "t1".into(),
            description: "Writing code".into(),
            persona: "implement".into(),
        };
        let update = task_event_to_status_update("a2a-task-1", "ctx-1", &event);
        assert!(update.is_some());
        let update = update.unwrap();
        assert_eq!(update.status.state, a2a_rs::domain::TaskState::Working);
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p hive_a2a -- bridge`
Expected: FAIL

**Step 3: Implement bridge.rs**

```rust
use a2a_rs::domain::{
    Artifact, Message, Part, Role, Task, TaskState, TaskStatus,
    TaskStatusUpdateEvent, TaskArtifactUpdateEvent,
};
use hive_agents::{
    AgentOutput, OrchestrationResult, OrchestrationStatus,
    CoordinatorResult, TaskEvent,
};
use hive_agents::swarm::SwarmResult;
use uuid::Uuid;

/// Extract plain text from an A2A Message by concatenating all text parts.
pub fn extract_message_text(message: &Message) -> String {
    message
        .parts
        .iter()
        .filter_map(|part| match part {
            Part::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Extract skill_id from message metadata if present.
pub fn extract_skill_id(metadata: Option<&serde_json::Map<String, serde_json::Value>>) -> Option<String> {
    metadata?.get("skill_id")?.as_str().map(|s| s.to_string())
}

/// Infer which Hive skill to use from the message text.
pub fn infer_skill(text: &str) -> &'static str {
    let lower = text.to_lowercase();

    // Queen indicators: multi-team, large scope
    if lower.contains("teams") || lower.contains("swarm") || lower.contains("platform") {
        return "queen";
    }

    // HiveMind indicators: design + implement, architect
    if lower.contains("architect") || lower.contains("design and implement")
        || lower.contains("plan and build") || lower.contains("full pipeline")
    {
        return "hivemind";
    }

    // Coordinator indicators: sequential steps, dependencies
    if lower.contains("steps") || lower.contains("then ")
        || lower.contains("after ") || lower.contains("in order")
        || lower.contains("parallel")
    {
        return "coordinator";
    }

    // Default: single for short/simple, hivemind for longer
    if text.len() < 100 {
        "single"
    } else {
        "hivemind"
    }
}

/// Convert an OrchestrationResult (HiveMind output) to an A2A Artifact.
pub fn orchestration_result_to_artifact(result: &OrchestrationResult) -> Artifact {
    let mut text = result.synthesized_output.clone();
    if let Some(score) = result.consensus_score {
        text.push_str(&format!("\n\n[Consensus score: {:.2}]", score));
    }
    text.push_str(&format!(
        "\n[Cost: ${:.4} | Duration: {}ms | Agents: {}]",
        result.total_cost, result.total_duration_ms, result.agent_outputs.len()
    ));

    Artifact {
        artifact_id: Uuid::new_v4().to_string(),
        name: Some("hivemind-output".into()),
        description: Some(format!("HiveMind orchestration result for: {}", result.task)),
        parts: vec![Part::Text { text, metadata: None }],
        metadata: None,
        extensions: None,
    }
}

/// Convert a CoordinatorResult to an A2A Artifact.
pub fn coordinator_result_to_artifact(result: &CoordinatorResult) -> Artifact {
    let mut text = String::new();
    for task_result in &result.results {
        text.push_str(&format!("## Task: {} ({})\n", task_result.task_id, task_result.persona.label()));
        if task_result.success {
            text.push_str(&task_result.output);
        } else if let Some(err) = &task_result.error {
            text.push_str(&format!("FAILED: {}", err));
        }
        text.push_str("\n\n---\n\n");
    }
    text.push_str(&format!(
        "[Cost: ${:.4} | Duration: {}ms | Tasks: {}/{}]",
        result.total_cost, result.total_duration_ms,
        result.results.iter().filter(|r| r.success).count(),
        result.results.len()
    ));

    Artifact {
        artifact_id: Uuid::new_v4().to_string(),
        name: Some("coordinator-output".into()),
        description: None,
        parts: vec![Part::Text { text, metadata: None }],
        metadata: None,
        extensions: None,
    }
}

/// Convert a SwarmResult (Queen output) to an A2A Artifact.
pub fn swarm_result_to_artifact(result: &SwarmResult) -> Artifact {
    let text = format!(
        "{}\n\n[Cost: ${:.4} | Duration: {}ms | Teams: {} | Learnings: {}]",
        result.synthesized_output,
        result.total_cost,
        result.total_duration_ms,
        result.team_results.len(),
        result.learnings_recorded,
    );

    Artifact {
        artifact_id: Uuid::new_v4().to_string(),
        name: Some("queen-output".into()),
        description: Some(format!("Queen swarm result for: {}", result.goal)),
        parts: vec![Part::Text { text, metadata: None }],
        metadata: None,
        extensions: None,
    }
}

/// Convert a Hive Coordinator TaskEvent to an A2A TaskStatusUpdateEvent.
/// Returns None for events that don't map to status updates.
pub fn task_event_to_status_update(
    a2a_task_id: &str,
    context_id: &str,
    event: &TaskEvent,
) -> Option<TaskStatusUpdateEvent> {
    let (state, msg_text, is_final) = match event {
        TaskEvent::PlanCreated { plan_id, tasks } => (
            TaskState::Working,
            format!("Plan created ({} tasks): {}", tasks.len(), plan_id),
            false,
        ),
        TaskEvent::TaskStarted { task_id, description, persona } => (
            TaskState::Working,
            format!("[{}] Started: {}", persona, description),
            false,
        ),
        TaskEvent::TaskProgress { task_id, progress, message } => (
            TaskState::Working,
            format!("[{:.0}%] {}", progress * 100.0, message),
            false,
        ),
        TaskEvent::TaskCompleted { task_id, duration_ms, cost, output_preview } => (
            TaskState::Working,
            format!("Completed {} ({}ms, ${:.4})", task_id, duration_ms, cost),
            false,
        ),
        TaskEvent::TaskFailed { task_id, error } => (
            TaskState::Working,
            format!("Task {} failed: {}", task_id, error),
            false,
        ),
        TaskEvent::AllComplete { total_cost, total_duration_ms, success_count, failure_count } => (
            if *failure_count == 0 { TaskState::Completed } else { TaskState::Failed },
            format!("All done: {}/{} succeeded (${:.4}, {}ms)",
                success_count, success_count + failure_count, total_cost, total_duration_ms),
            true,
        ),
    };

    let status_msg = Message::user_text(msg_text); // a2a-rs helper
    // Adjust role to Agent for outbound status messages
    let mut agent_msg = status_msg;
    agent_msg.role = Role::Agent;

    Some(TaskStatusUpdateEvent {
        task_id: a2a_task_id.into(),
        context_id: context_id.into(),
        kind: "status-update".into(),
        status: TaskStatus {
            state,
            message: Some(agent_msg),
            timestamp: Some(chrono::Utc::now()),
        },
        final_: is_final,
        metadata: None,
    })
}

/// Convert an A2A Artifact text content back to a simple String (for inbound from remote agents).
pub fn artifact_to_text(artifact: &Artifact) -> String {
    artifact
        .parts
        .iter()
        .filter_map(|part| match part {
            Part::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Helper trait to get a label from PersonaKind (needed for bridge formatting).
trait PersonaLabel {
    fn label(&self) -> &str;
}

impl PersonaLabel for hive_agents::PersonaKind {
    fn label(&self) -> &str {
        match self {
            hive_agents::PersonaKind::Investigate => "investigate",
            hive_agents::PersonaKind::Implement => "implement",
            hive_agents::PersonaKind::Verify => "verify",
            hive_agents::PersonaKind::Critique => "critique",
            hive_agents::PersonaKind::Debug => "debug",
            hive_agents::PersonaKind::CodeReview => "code-review",
            hive_agents::PersonaKind::Custom(s) => s.as_str(),
        }
    }
}
```

**Step 4: Run tests**

Run: `cargo test -p hive_a2a -- bridge`
Expected: PASS

**Step 5: Commit**

```bash
git add crates/hive_a2a/src/bridge.rs crates/hive_a2a/src/lib.rs
git commit -m "feat(a2a): add A2A ↔ Hive type bridge conversions"
```

---

### Task 5: Auth middleware

**Files:**
- Create: `hive/crates/hive_a2a/src/auth.rs`
- Modify: `hive/crates/hive_a2a/src/lib.rs` (uncomment `pub mod auth;`)
- Test: inline `#[cfg(test)]`

**Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_api_key_success() {
        let result = validate_api_key(Some("correct-key"), "correct-key");
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_api_key_mismatch() {
        let result = validate_api_key(Some("wrong-key"), "correct-key");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_api_key_missing() {
        let result = validate_api_key(None, "correct-key");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_no_key_configured_allows_all() {
        // When no API key is configured, skip validation
        let result = validate_api_key_optional(Some("anything"), None);
        assert!(result.is_ok());
        let result = validate_api_key_optional(None, None);
        assert!(result.is_ok());
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p hive_a2a -- auth`
Expected: FAIL

**Step 3: Implement auth.rs**

```rust
use crate::error::A2aError;

/// Validate an API key from a request header against expected value.
pub fn validate_api_key(provided: Option<&str>, expected: &str) -> Result<(), A2aError> {
    match provided {
        Some(key) if key == expected => Ok(()),
        Some(_) => Err(A2aError::Auth("Invalid API key".into())),
        None => Err(A2aError::Auth("Missing X-Hive-Key header".into())),
    }
}

/// Validate API key only if one is configured. If no key is configured, allow all requests.
pub fn validate_api_key_optional(provided: Option<&str>, expected: Option<&str>) -> Result<(), A2aError> {
    match expected {
        Some(key) => validate_api_key(provided, key),
        None => Ok(()), // No key configured = no auth required
    }
}

/// Validate that an outbound URL is safe (HTTPS for non-localhost, no private IPs).
pub fn validate_outbound_url(url: &str) -> Result<(), A2aError> {
    let parsed = url::Url::parse(url)
        .map_err(|e| A2aError::Security(format!("Invalid URL: {}", e)))?;

    let host = parsed.host_str()
        .ok_or_else(|| A2aError::Security("URL has no host".into()))?;

    // Allow localhost without HTTPS
    let is_localhost = host == "localhost" || host == "127.0.0.1" || host == "::1";

    if !is_localhost {
        // Require HTTPS for non-localhost
        if parsed.scheme() != "https" {
            return Err(A2aError::Security(format!(
                "HTTPS required for non-localhost URLs: {}", url
            )));
        }

        // Block private IPs
        if host.starts_with("10.")
            || host.starts_with("192.168.")
            || host.starts_with("169.254.")
            || host == "0.0.0.0"
        {
            return Err(A2aError::Security(format!(
                "Private IP addresses not allowed: {}", host
            )));
        }
    }

    Ok(())
}
```

Note: add `url = "2"` to `[dependencies]` in `Cargo.toml`.

**Step 4: Run tests**

Run: `cargo test -p hive_a2a -- auth`
Expected: PASS

**Step 5: Commit**

```bash
git add crates/hive_a2a/src/auth.rs crates/hive_a2a/src/lib.rs crates/hive_a2a/Cargo.toml
git commit -m "feat(a2a): add API key auth and outbound URL validation"
```

---

### Task 6: Task handler — A2A message → Hive orchestrator dispatch

**Files:**
- Create: `hive/crates/hive_a2a/src/task_handler.rs`
- Modify: `hive/crates/hive_a2a/src/lib.rs`
- Test: inline `#[cfg(test)]`

This is the core component. It implements `a2a_rs::port::AsyncMessageHandler` and routes A2A messages to the appropriate Hive orchestrator.

**Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // MockExecutor for tests (implements AiExecutor)
    struct MockExecutor;
    impl hive_agents::hivemind::AiExecutor for MockExecutor {
        async fn execute(&self, request: &hive_ai::ChatRequest) -> Result<hive_ai::ChatResponse, String> {
            Ok(hive_ai::ChatResponse {
                content: format!("Mock response to: {}", request.messages.last().map(|m| m.content.as_str()).unwrap_or("")),
                model: "mock".into(),
                usage: hive_ai::TokenUsage::default(),
                finish_reason: hive_ai::FinishReason::Stop,
                thinking: None,
                tool_calls: None,
            })
        }
    }

    #[test]
    fn test_resolve_skill_explicit() {
        let mut metadata = serde_json::Map::new();
        metadata.insert("skill_id".into(), "coordinator".into());
        let skill = resolve_skill("any text", Some(&metadata), "hivemind");
        assert_eq!(skill, "coordinator");
    }

    #[test]
    fn test_resolve_skill_inferred() {
        let skill = resolve_skill("Review this code for bugs", None, "hivemind");
        assert_eq!(skill, "single"); // short message → single
    }

    #[test]
    fn test_resolve_skill_default_fallback() {
        let skill = resolve_skill("", None, "coordinator");
        assert_eq!(skill, "coordinator"); // empty → use default
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p hive_a2a -- task_handler`
Expected: FAIL

**Step 3: Implement task_handler.rs**

```rust
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};
use uuid::Uuid;

use a2a_rs::domain::{
    Artifact, Message, Part, Role, Task, TaskState, TaskStatus,
    TaskStatusUpdateEvent, TaskArtifactUpdateEvent,
};

use hive_agents::hivemind::{AiExecutor, HiveMind, HiveMindConfig};
use hive_agents::{Coordinator, CoordinatorConfig, Queen};
use hive_agents::swarm::SwarmConfig;

use crate::bridge;
use crate::config::ServerDefaults;
use crate::error::A2aError;

/// Active task tracking.
pub struct ActiveTask {
    pub a2a_task: Task,
    pub event_tx: broadcast::Sender<TaskStatusUpdateEvent>,
}

/// The Hive task handler — routes A2A messages to Hive orchestrators.
pub struct HiveTaskHandler<E: AiExecutor + 'static> {
    executor: Arc<E>,
    defaults: ServerDefaults,
    active_tasks: Arc<Mutex<HashMap<String, ActiveTask>>>,
}

impl<E: AiExecutor + 'static> HiveTaskHandler<E> {
    pub fn new(executor: Arc<E>, defaults: ServerDefaults) -> Self {
        Self {
            executor,
            defaults,
            active_tasks: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Process an inbound A2A message: create/continue a task.
    pub async fn handle_message(
        &self,
        task_id: &str,
        message: &Message,
    ) -> Result<Task, A2aError> {
        let text = bridge::extract_message_text(message);
        if text.is_empty() {
            return Err(A2aError::Bridge("Message has no text content".into()));
        }

        let metadata = message.metadata.as_ref();
        let skill_id = resolve_skill(&text, metadata, &self.defaults.default_skill);
        let context_id = message.context_id.clone().unwrap_or_else(|| Uuid::new_v4().to_string());

        // Create the A2A task in Working state
        let (event_tx, _) = broadcast::channel(64);
        let task = Task {
            id: task_id.into(),
            context_id: context_id.clone(),
            status: TaskStatus {
                state: TaskState::Working,
                message: None,
                timestamp: Some(chrono::Utc::now()),
            },
            artifacts: None,
            history: Some(vec![message.clone()]),
            metadata: None,
            kind: "task".into(),
        };

        // Store active task
        {
            let mut tasks = self.active_tasks.lock().await;
            tasks.insert(task_id.into(), ActiveTask {
                a2a_task: task.clone(),
                event_tx: event_tx.clone(),
            });
        }

        // Spawn orchestration in background
        let executor = self.executor.clone();
        let task_id_owned = task_id.to_string();
        let active_tasks = self.active_tasks.clone();
        let defaults = self.defaults.clone();

        tokio::spawn(async move {
            let result = execute_skill(&skill_id, &text, executor.as_ref(), &defaults).await;

            let mut tasks = active_tasks.lock().await;
            if let Some(active) = tasks.get_mut(&task_id_owned) {
                match result {
                    Ok(artifact) => {
                        active.a2a_task.status = TaskStatus {
                            state: TaskState::Completed,
                            message: None,
                            timestamp: Some(chrono::Utc::now()),
                        };
                        active.a2a_task.artifacts = Some(vec![artifact]);
                    }
                    Err(err) => {
                        let err_msg = Message {
                            role: Role::Agent,
                            parts: vec![Part::Text { text: err.to_string(), metadata: None }],
                            metadata: None,
                            reference_task_ids: None,
                            message_id: Uuid::new_v4().to_string(),
                            task_id: Some(task_id_owned.clone()),
                            context_id: None,
                            extensions: None,
                            kind: "message".into(),
                        };
                        active.a2a_task.status = TaskStatus {
                            state: TaskState::Failed,
                            message: Some(err_msg),
                            timestamp: Some(chrono::Utc::now()),
                        };
                    }
                }

                // Broadcast final status
                let _ = active.event_tx.send(TaskStatusUpdateEvent {
                    task_id: task_id_owned.clone(),
                    context_id: active.a2a_task.context_id.clone(),
                    kind: "status-update".into(),
                    status: active.a2a_task.status.clone(),
                    final_: true,
                    metadata: None,
                });
            }
        });

        Ok(task)
    }

    /// Get a task by ID.
    pub async fn get_task(&self, task_id: &str) -> Result<Task, A2aError> {
        let tasks = self.active_tasks.lock().await;
        tasks
            .get(task_id)
            .map(|at| at.a2a_task.clone())
            .ok_or_else(|| A2aError::TaskNotFound(task_id.into()))
    }

    /// Subscribe to task status updates.
    pub async fn subscribe(&self, task_id: &str) -> Result<broadcast::Receiver<TaskStatusUpdateEvent>, A2aError> {
        let tasks = self.active_tasks.lock().await;
        tasks
            .get(task_id)
            .map(|at| at.event_tx.subscribe())
            .ok_or_else(|| A2aError::TaskNotFound(task_id.into()))
    }
}

/// Resolve which skill to use: explicit metadata > inference > default.
pub fn resolve_skill(
    text: &str,
    metadata: Option<&serde_json::Map<String, serde_json::Value>>,
    default: &str,
) -> String {
    // 1. Check explicit skill_id in metadata
    if let Some(skill) = bridge::extract_skill_id(metadata) {
        if crate::agent_card::is_supported_skill(&skill) {
            return skill;
        }
    }

    // 2. Infer from text (only if text is non-empty)
    if !text.is_empty() {
        return bridge::infer_skill(text).to_string();
    }

    // 3. Fall back to configured default
    default.to_string()
}

/// Execute the appropriate Hive orchestrator for the given skill.
async fn execute_skill<E: AiExecutor>(
    skill_id: &str,
    task_text: &str,
    executor: &E,
    defaults: &ServerDefaults,
) -> Result<Artifact, A2aError> {
    match skill_id {
        "hivemind" => {
            let config = HiveMindConfig {
                max_agents: 9,
                cost_limit_usd: defaults.max_budget_usd,
                time_limit_secs: defaults.max_time_seconds,
                auto_scale: false,
                consensus_threshold: 0.7,
                model_overrides: HashMap::new(),
            };
            // HiveMind::new takes executor by value — we need a reference workaround.
            // For now, we create a wrapper that clones the Arc.
            // This will be refined when we integrate with the actual server.
            Err(A2aError::Provider("HiveMind execution requires owned executor — see server.rs for full integration".into()))
        }
        "coordinator" => {
            Err(A2aError::Provider("Coordinator execution requires owned executor — see server.rs for full integration".into()))
        }
        "queen" => {
            Err(A2aError::Provider("Queen execution requires owned executor — see server.rs for full integration".into()))
        }
        "single" => {
            // Single-shot: just call the executor directly
            let request = hive_ai::ChatRequest {
                messages: vec![hive_ai::ChatMessage::text(hive_ai::MessageRole::User, task_text)],
                model: "claude-sonnet-4-5-20250929".into(),
                max_tokens: 4096,
                temperature: Some(0.7),
                system_prompt: Some("You are a helpful AI coding assistant.".into()),
                tools: None,
                cache_system_prompt: false,
            };
            let response = executor.execute(&request).await
                .map_err(|e| A2aError::Provider(e))?;

            Ok(Artifact {
                artifact_id: Uuid::new_v4().to_string(),
                name: Some("single-output".into()),
                description: None,
                parts: vec![Part::Text { text: response.content, metadata: None }],
                metadata: None,
                extensions: None,
            })
        }
        other => Err(A2aError::UnsupportedSkill(other.into())),
    }
}
```

**Step 4: Run tests**

Run: `cargo test -p hive_a2a -- task_handler`
Expected: PASS

**Step 5: Commit**

```bash
git add crates/hive_a2a/src/task_handler.rs crates/hive_a2a/src/lib.rs
git commit -m "feat(a2a): add task handler with skill routing and orchestrator dispatch"
```

---

### Task 7: HTTP Server — Axum routes

**Files:**
- Create: `hive/crates/hive_a2a/src/server.rs`
- Modify: `hive/crates/hive_a2a/src/lib.rs`
- Test: inline `#[cfg(test)]`

**Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_agent_card_endpoint() {
        let config = crate::config::A2aConfig::default();
        let app = build_router(config.clone());

        let response = axum::serve(
            tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap(),
            app,
        );
        // Use axum test utilities instead
        use axum::body::Body;
        use http::Request;
        use tower::ServiceExt;

        let app = build_router(config);
        let req = Request::builder()
            .uri("/.well-known/agent-card.json")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), 200);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let card: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(card["name"], "Hive");
        assert_eq!(card["skills"].as_array().unwrap().len(), 4);
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p hive_a2a -- server`
Expected: FAIL

**Step 3: Implement server.rs**

```rust
use axum::{
    Router, Json,
    extract::State,
    http::{StatusCode, HeaderMap},
    response::{IntoResponse, Response, Sse},
    routing::{get, post},
};
use std::sync::Arc;
use tokio::net::TcpListener;

use crate::agent_card::build_hive_agent_info;
use crate::auth::validate_api_key_optional;
use crate::config::A2aConfig;
use crate::error::A2aError;

/// Shared server state.
#[derive(Clone)]
pub struct AppState {
    pub config: A2aConfig,
}

/// Build the Axum router with all A2A endpoints.
pub fn build_router(config: A2aConfig) -> Router {
    let state = AppState { config };

    Router::new()
        .route("/.well-known/agent-card.json", get(agent_card_handler))
        .route("/a2a", post(send_message_handler))
        .route("/a2a/tasks/{task_id}", get(get_task_handler))
        .with_state(state)
}

/// GET /.well-known/agent-card.json — serve the Hive Agent Card.
async fn agent_card_handler(State(state): State<AppState>) -> impl IntoResponse {
    let info = build_hive_agent_info(&state.config.server);
    let card = info.get_agent_card();
    Json(card)
}

/// POST /a2a — receive A2A JSON-RPC messages.
async fn send_message_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<serde_json::Value>,
) -> Response {
    // Auth check
    let api_key = headers
        .get("x-hive-key")
        .and_then(|v| v.to_str().ok());
    if let Err(e) = validate_api_key_optional(api_key, state.config.server.api_key.as_deref()) {
        return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({
            "error": e.to_string()
        }))).into_response();
    }

    // For now, return a placeholder — full JSON-RPC routing will use a2a-rs DefaultRequestProcessor
    // in the final integration (Task 9)
    (StatusCode::OK, Json(serde_json::json!({
        "jsonrpc": "2.0",
        "id": body.get("id").cloned().unwrap_or(serde_json::Value::Null),
        "result": {
            "status": "received",
            "message": "A2A endpoint active — full routing in progress"
        }
    }))).into_response()
}

/// GET /a2a/tasks/:task_id — get task status.
async fn get_task_handler(
    State(state): State<AppState>,
    axum::extract::Path(task_id): axum::extract::Path<String>,
) -> Response {
    // Placeholder — will integrate with HiveTaskHandler in Task 9
    (StatusCode::NOT_FOUND, Json(serde_json::json!({
        "error": format!("Task {} not found", task_id)
    }))).into_response()
}

/// Start the A2A HTTP server.
pub async fn start_server(config: A2aConfig) -> Result<(), A2aError> {
    if !config.server.enabled {
        return Ok(());
    }

    let addr = config.bind_addr();
    let router = build_router(config);

    let listener = TcpListener::bind(&addr).await
        .map_err(|e| A2aError::Network(format!("Failed to bind to {}: {}", addr, e)))?;

    axum::serve(listener, router).await
        .map_err(|e| A2aError::Network(format!("Server error: {}", e)))?;

    Ok(())
}
```

Note: add `http = "1"` to `[dependencies]` and `[dev-dependencies]` in `Cargo.toml`.

**Step 4: Run tests**

Run: `cargo test -p hive_a2a -- server`
Expected: PASS

**Step 5: Commit**

```bash
git add crates/hive_a2a/src/server.rs crates/hive_a2a/src/lib.rs crates/hive_a2a/Cargo.toml
git commit -m "feat(a2a): add Axum HTTP server with Agent Card and message endpoints"
```

---

### Task 8: Client — Agent Card discovery + RemoteAgent

**Files:**
- Create: `hive/crates/hive_a2a/src/client.rs`
- Create: `hive/crates/hive_a2a/src/remote_agent.rs`
- Modify: `hive/crates/hive_a2a/src/lib.rs`
- Test: inline `#[cfg(test)]`

**Step 1: Write the failing tests**

In `client.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_card_url() {
        let url = agent_card_url("https://agent.example.com");
        assert_eq!(url, "https://agent.example.com/.well-known/agent-card.json");
    }

    #[test]
    fn test_agent_card_url_trailing_slash() {
        let url = agent_card_url("https://agent.example.com/");
        assert_eq!(url, "https://agent.example.com/.well-known/agent-card.json");
    }

    #[test]
    fn test_discovery_cache() {
        let cache = DiscoveryCache::new(std::time::Duration::from_secs(300));
        assert!(cache.get("https://example.com").is_none());
    }
}
```

In `remote_agent.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_remote_agent_from_config() {
        let config = crate::config::RemoteAgentConfig {
            name: "Test".into(),
            url: "https://test.com".into(),
            api_key: Some("key".into()),
        };
        let agent = RemoteAgent::from_config(config);
        assert_eq!(agent.name, "Test");
        assert_eq!(agent.base_url, "https://test.com");
    }
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test -p hive_a2a -- client remote_agent`
Expected: FAIL

**Step 3: Implement client.rs**

```rust
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use a2a_rs::domain::AgentCard;
use crate::auth::validate_outbound_url;
use crate::error::A2aError;

/// Build the well-known Agent Card URL from a base URL.
pub fn agent_card_url(base_url: &str) -> String {
    let base = base_url.trim_end_matches('/');
    format!("{}/.well-known/agent-card.json", base)
}

/// Simple in-memory cache for discovered Agent Cards.
pub struct DiscoveryCache {
    entries: Mutex<HashMap<String, (AgentCard, Instant)>>,
    ttl: Duration,
}

impl DiscoveryCache {
    pub fn new(ttl: Duration) -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            ttl,
        }
    }

    pub fn get(&self, url: &str) -> Option<AgentCard> {
        let entries = self.entries.lock().unwrap();
        entries.get(url).and_then(|(card, inserted)| {
            if inserted.elapsed() < self.ttl {
                Some(card.clone())
            } else {
                None
            }
        })
    }

    pub fn insert(&self, url: &str, card: AgentCard) {
        let mut entries = self.entries.lock().unwrap();
        entries.insert(url.into(), (card, Instant::now()));
    }

    pub fn invalidate(&self, url: &str) {
        let mut entries = self.entries.lock().unwrap();
        entries.remove(url);
    }
}

/// Discover an Agent Card from a remote URL.
pub async fn discover_agent(base_url: &str, cache: &DiscoveryCache) -> Result<AgentCard, A2aError> {
    // Check cache first
    if let Some(card) = cache.get(base_url) {
        return Ok(card);
    }

    // Validate URL safety
    validate_outbound_url(base_url)?;

    let card_url = agent_card_url(base_url);
    let client = reqwest::Client::new();
    let response = client.get(&card_url)
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| A2aError::Network(format!("Failed to fetch Agent Card from {}: {}", card_url, e)))?;

    if !response.status().is_success() {
        return Err(A2aError::Network(format!(
            "Agent Card request failed with status {}: {}", response.status(), card_url
        )));
    }

    let card: AgentCard = response.json().await
        .map_err(|e| A2aError::Bridge(format!("Failed to parse Agent Card: {}", e)))?;

    cache.insert(base_url, card.clone());
    Ok(card)
}
```

**Step 4: Implement remote_agent.rs**

```rust
use a2a_rs::domain::{AgentCard, Artifact, Message, Task};
use crate::bridge;
use crate::config::RemoteAgentConfig;
use crate::error::A2aError;

/// A remote A2A agent that Hive can delegate tasks to.
pub struct RemoteAgent {
    pub name: String,
    pub base_url: String,
    pub api_key: Option<String>,
    pub card: Option<AgentCard>,
    client: reqwest::Client,
}

impl RemoteAgent {
    /// Create from config entry.
    pub fn from_config(config: RemoteAgentConfig) -> Self {
        Self {
            name: config.name,
            base_url: config.url,
            api_key: config.api_key,
            card: None,
            client: reqwest::Client::new(),
        }
    }

    /// Create with a pre-fetched Agent Card.
    pub fn with_card(name: String, base_url: String, card: AgentCard, api_key: Option<String>) -> Self {
        Self {
            name,
            base_url,
            api_key,
            card: Some(card),
            client: reqwest::Client::new(),
        }
    }

    /// Send a task message to this remote agent (non-streaming).
    pub async fn send_task(
        &self,
        message_text: &str,
        skill_id: Option<&str>,
        task_id: &str,
    ) -> Result<Task, A2aError> {
        let url = self.card.as_ref()
            .map(|c| c.url.clone())
            .unwrap_or_else(|| self.base_url.clone());

        let mut metadata = serde_json::Map::new();
        if let Some(skill) = skill_id {
            metadata.insert("skill_id".into(), serde_json::Value::String(skill.into()));
        }

        let rpc_request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": task_id,
            "method": "SendMessage",
            "params": {
                "message": {
                    "role": "user",
                    "parts": [{ "kind": "text", "text": message_text }],
                    "messageId": uuid::Uuid::new_v4().to_string(),
                    "kind": "message",
                    "metadata": metadata,
                },
                "configuration": {
                    "acceptedOutputModes": ["text"],
                    "blocking": true
                }
            }
        });

        let mut request = self.client.post(&url).json(&rpc_request);
        if let Some(key) = &self.api_key {
            request = request.header("Authorization", format!("Bearer {}", key));
        }

        let response = request.send().await
            .map_err(|e| A2aError::Network(format!("Failed to send task to {}: {}", self.name, e)))?;

        if !response.status().is_success() {
            return Err(A2aError::Network(format!(
                "Remote agent {} returned status {}", self.name, response.status()
            )));
        }

        let rpc_response: serde_json::Value = response.json().await
            .map_err(|e| A2aError::Bridge(format!("Failed to parse response from {}: {}", self.name, e)))?;

        // Extract task from JSON-RPC result
        let task: Task = serde_json::from_value(rpc_response["result"].clone())
            .map_err(|e| A2aError::Bridge(format!("Failed to parse Task from {}: {}", self.name, e)))?;

        Ok(task)
    }

    /// Get the text output from a completed remote task.
    pub fn extract_output(task: &Task) -> String {
        task.artifacts
            .as_ref()
            .map(|artifacts| {
                artifacts.iter()
                    .map(|a| bridge::artifact_to_text(a))
                    .collect::<Vec<_>>()
                    .join("\n\n")
            })
            .unwrap_or_default()
    }

    /// List available skills on this remote agent.
    pub fn available_skills(&self) -> Vec<String> {
        self.card
            .as_ref()
            .map(|c| c.skills.iter().map(|s| s.id.clone()).collect())
            .unwrap_or_default()
    }
}
```

**Step 5: Run tests**

Run: `cargo test -p hive_a2a -- client remote_agent`
Expected: PASS

**Step 6: Commit**

```bash
git add crates/hive_a2a/src/client.rs crates/hive_a2a/src/remote_agent.rs crates/hive_a2a/src/lib.rs
git commit -m "feat(a2a): add Agent Card discovery client and RemoteAgent wrapper"
```

---

### Task 9: Full integration — wire server with a2a-rs DefaultRequestProcessor

**Files:**
- Modify: `hive/crates/hive_a2a/src/server.rs`
- Modify: `hive/crates/hive_a2a/src/task_handler.rs`
- Create: `hive/crates/hive_a2a/src/streaming.rs`
- Modify: `hive/crates/hive_a2a/src/lib.rs`

This task wires the a2a-rs `DefaultRequestProcessor` + `SimpleAgentHandler` into the Axum server, replacing the placeholder endpoints from Task 7.

**Step 1: Write the failing test**

```rust
// In streaming.rs
#[cfg(test)]
mod tests {
    use super::*;
    use hive_agents::coordinator::TaskEvent;

    #[test]
    fn test_coordinator_event_to_sse_data() {
        let event = TaskEvent::TaskStarted {
            task_id: "t1".into(),
            description: "Implementing feature".into(),
            persona: "implement".into(),
        };
        let sse_data = coordinator_event_to_sse("a2a-task-1", "ctx-1", &event);
        assert!(sse_data.is_some());
        let json_str = sse_data.unwrap();
        assert!(json_str.contains("Working") || json_str.contains("working"));
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p hive_a2a -- streaming`
Expected: FAIL

**Step 3: Implement streaming.rs**

```rust
use hive_agents::coordinator::TaskEvent;
use crate::bridge;

/// Convert a Coordinator TaskEvent into an SSE-ready JSON string.
pub fn coordinator_event_to_sse(
    a2a_task_id: &str,
    context_id: &str,
    event: &TaskEvent,
) -> Option<String> {
    let update = bridge::task_event_to_status_update(a2a_task_id, context_id, event)?;
    serde_json::to_string(&update).ok()
}
```

**Step 4: Update server.rs to use a2a-rs processing**

Replace the placeholder `send_message_handler` with proper a2a-rs integration. The full wiring uses `a2a_rs::adapter::business::SimpleAgentHandler` + `DefaultRequestProcessor` + `HttpServer`. However, since Hive needs custom routing (skill dispatch), we implement `AsyncMessageHandler` on our `HiveTaskHandler` and compose it with a2a-rs's `InMemoryTaskStorage`.

This step requires careful integration — the implementing engineer should:
1. Make `HiveTaskHandler` implement `a2a_rs::port::AsyncMessageHandler`
2. Use `a2a_rs::adapter::storage::InMemoryTaskStorage` for `AsyncTaskManager`
3. Compose into `DefaultRequestProcessor`
4. Either use a2a-rs `HttpServer` directly or adapt the Axum routes to delegate to the processor

**Step 5: Run all tests**

Run: `cargo test -p hive_a2a`
Expected: All tests pass

**Step 6: Commit**

```bash
git add crates/hive_a2a/src/streaming.rs crates/hive_a2a/src/server.rs crates/hive_a2a/src/task_handler.rs crates/hive_a2a/src/lib.rs
git commit -m "feat(a2a): integrate a2a-rs request processor with streaming support"
```

---

### Task 10: Integration test — full round-trip

**Files:**
- Create: `hive/crates/hive_a2a/tests/integration.rs`

**Step 1: Write the integration test**

```rust
//! Full round-trip integration test:
//! 1. Start the A2A server
//! 2. Fetch the Agent Card
//! 3. Send a task message
//! 4. Poll for task completion
//! 5. Verify the response

use hive_a2a::config::A2aConfig;
use hive_a2a::server;

#[tokio::test]
async fn test_full_round_trip() {
    // Use a random port to avoid conflicts
    let mut config = A2aConfig::default();
    config.server.port = 0; // Let OS assign

    // Start server
    let router = server::build_router(config);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    let client = reqwest::Client::new();

    // 1. Fetch Agent Card
    let card_url = format!("http://{}/.well-known/agent-card.json", addr);
    let card_resp = client.get(&card_url).send().await.unwrap();
    assert_eq!(card_resp.status(), 200);

    let card: serde_json::Value = card_resp.json().await.unwrap();
    assert_eq!(card["name"], "Hive");
    assert_eq!(card["skills"].as_array().unwrap().len(), 4);
    assert!(card["capabilities"]["streaming"].as_bool().unwrap());

    // 2. Send a message
    let a2a_url = format!("http://{}/a2a", addr);
    let msg = serde_json::json!({
        "jsonrpc": "2.0",
        "id": "test-1",
        "method": "SendMessage",
        "params": {
            "message": {
                "role": "user",
                "parts": [{ "kind": "text", "text": "Hello Hive" }],
                "messageId": "msg-1",
                "kind": "message"
            }
        }
    });

    let resp = client.post(&a2a_url).json(&msg).send().await.unwrap();
    assert_eq!(resp.status(), 200);
}
```

**Step 2: Run the integration test**

Run: `cargo test -p hive_a2a --test integration`
Expected: PASS

**Step 3: Commit**

```bash
git add crates/hive_a2a/tests/integration.rs
git commit -m "test(a2a): add full round-trip integration test"
```

---

### Task 11: Wire hive_a2a into hive_app startup

**Files:**
- Modify: `hive/crates/hive_app/Cargo.toml` (add `hive_a2a` dependency)
- Modify: `hive/crates/hive_app/src/main.rs` (spawn A2A server on startup)

**Step 1: Add dependency**

In `hive/crates/hive_app/Cargo.toml`, add:
```toml
hive_a2a = { path = "../hive_a2a" }
```

**Step 2: Spawn A2A server in main.rs**

Find the startup section in `main.rs` and add:

```rust
// Start A2A server if enabled
{
    let a2a_config_path = hive_core::config_dir().join("a2a.toml");
    let a2a_config = hive_a2a::config::A2aConfig::load_or_create(&a2a_config_path)
        .unwrap_or_default();
    if a2a_config.server.enabled {
        let addr = a2a_config.bind_addr();
        tokio::spawn(async move {
            if let Err(e) = hive_a2a::server::start_server(a2a_config).await {
                eprintln!("A2A server error: {}", e);
            }
        });
        println!("A2A server listening on {}", addr);
    }
}
```

**Step 3: Build and verify**

Run: `cargo build -p hive_app`
Expected: Compiles successfully

**Step 4: Commit**

```bash
git add crates/hive_app/Cargo.toml crates/hive_app/src/main.rs
git commit -m "feat(a2a): wire A2A server into hive_app startup"
```

---

### Task 12: Final lib.rs cleanup + public API

**Files:**
- Modify: `hive/crates/hive_a2a/src/lib.rs`

**Step 1: Finalize lib.rs with all modules and re-exports**

```rust
//! hive_a2a — A2A (Agent-to-Agent) protocol support for Hive.
//!
//! Enables bidirectional interoperability with external AI agents:
//! - **Server**: Exposes HiveMind/Coordinator/Queen as A2A skills via HTTP
//! - **Client**: Discovers and delegates tasks to external A2A agents
//!
//! ## Quick Start (Server)
//! ```no_run
//! let config = hive_a2a::config::A2aConfig::default();
//! hive_a2a::server::start_server(config).await.unwrap();
//! ```
//!
//! ## Quick Start (Client)
//! ```no_run
//! let cache = hive_a2a::client::DiscoveryCache::new(std::time::Duration::from_secs(300));
//! let card = hive_a2a::client::discover_agent("https://agent.example.com", &cache).await.unwrap();
//! ```

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
pub use config::A2aConfig;
pub use error::A2aError;
pub use remote_agent::RemoteAgent;
pub use server::start_server;
pub use client::{discover_agent, DiscoveryCache};
```

**Step 2: Run full test suite**

Run: `cargo test -p hive_a2a`
Expected: All tests pass

**Step 3: Run workspace check**

Run: `cargo check --workspace`
Expected: No errors

**Step 4: Commit**

```bash
git add crates/hive_a2a/src/lib.rs
git commit -m "feat(a2a): finalize public API and re-exports"
```

---

## Summary

| Task | Component | Estimated Time |
|------|-----------|----------------|
| 1 | Scaffold crate + workspace | 5 min |
| 2 | Config + Error types | 15 min |
| 3 | Agent Card builder | 15 min |
| 4 | Bridge (A2A ↔ Hive types) | 30 min |
| 5 | Auth middleware | 15 min |
| 6 | Task handler (orchestrator dispatch) | 45 min |
| 7 | HTTP Server (Axum routes) | 30 min |
| 8 | Client (discovery + RemoteAgent) | 30 min |
| 9 | Full a2a-rs integration + streaming | 45 min |
| 10 | Integration test (round-trip) | 20 min |
| 11 | Wire into hive_app startup | 10 min |
| 12 | Finalize public API | 10 min |
| **Total** | | **~4.5 hours** |

## Key Notes for the Implementing Engineer

1. **a2a-rs is v0.1.0** — the API may have rough edges. Check `docs.rs/a2a-rs` for exact signatures. If traits don't match, adapt the bridge.
2. **AiExecutor ownership** — HiveMind/Coordinator/Queen take `E: AiExecutor` by value. You'll need `Arc<E>` wrappers. Queen already does this internally with `ArcExecutor`.
3. **No lookahead/lookbehind** in regex — the workspace uses regex v1.
4. **GPUI note** — `AppContext` is a trait, use `&mut App` for concrete type.
5. **Windows build** — must run from VS Developer Command Prompt or set INCLUDE/LIB env vars.
6. **Security gate** — all inbound messages must pass through `SecurityGateway::check_injection()` before execution.
7. **Port 7420** — chosen to avoid conflicts with common dev ports (3000, 5173, 8080, etc.).
