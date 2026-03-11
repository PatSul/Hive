//! Shared outbound A2A client service for app/UI/MCP integration.

use std::path::{Path, PathBuf};
use std::sync::RwLock;
use std::time::Duration;

use async_trait::async_trait;
use a2a_rs::{AgentCard, TaskState};
use hive_agents::integration_tools::{A2aAgentRecord, A2aTaskRecord, OutboundA2aService};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::client::DiscoveryCache;
use crate::config::{A2aConfig, RemoteAgentConfig};
use crate::error::A2aError;
use crate::remote_agent::RemoteAgent;

const AGENT_CARD_SUFFIX: &str = "/.well-known/agent-card.json";
const LEGACY_AGENT_CARD_SUFFIX: &str = "/.well-known/agent.json";

/// Snapshot of a configured remote agent, optionally enriched with a discovered card.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteAgentSummary {
    pub name: String,
    pub url: String,
    pub api_key_configured: bool,
    pub discovered: bool,
    pub card_name: Option<String>,
    pub description: Option<String>,
    pub version: Option<String>,
    pub skills: Vec<String>,
}

/// Result of running a prompt against a remote agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteAgentRunResult {
    pub agent_name: String,
    pub url: String,
    pub task_id: String,
    pub state: String,
    pub skill_id: Option<String>,
    pub output: String,
}

/// App-owned outbound A2A client service backed by `a2a.toml`.
pub struct A2aClientService {
    config_path: PathBuf,
    config: RwLock<A2aConfig>,
    cache: DiscoveryCache,
}

impl A2aClientService {
    /// Load config from disk or create a default one if missing.
    pub fn load_or_create(path: impl AsRef<Path>) -> Result<Self, A2aError> {
        let path = path.as_ref().to_path_buf();
        let config = A2aConfig::load_or_create(&path)?;
        // Validate each agent URL at load time
        for agent in &config.agents {
            crate::auth::validate_outbound_url(&agent.url).map_err(|e| {
                A2aError::Config(format!("Agent '{}' has invalid URL: {e}", agent.name))
            })?;
        }
        Ok(Self::with_config(path, config))
    }

    /// Build a service from an already-loaded config.
    pub fn with_config(path: impl Into<PathBuf>, config: A2aConfig) -> Self {
        let ttl = Duration::from_secs(config.client.discovery_cache_ttl_seconds.max(1));
        Self {
            config_path: path.into(),
            config: RwLock::new(config),
            cache: DiscoveryCache::new(ttl),
        }
    }

    /// Return the current config path on disk.
    pub fn config_path(&self) -> &Path {
        &self.config_path
    }

    /// Reload `a2a.toml` from disk.
    pub fn reload(&self) -> Result<(), A2aError> {
        let config = A2aConfig::load_or_create(&self.config_path)?;
        // Validate each agent URL on reload
        for agent in &config.agents {
            crate::auth::validate_outbound_url(&agent.url).map_err(|e| {
                A2aError::Config(format!("Agent '{}' has invalid URL: {e}", agent.name))
            })?;
        }
        let mut guard = self
            .config
            .write()
            .map_err(|_| A2aError::Config("A2A config lock poisoned".into()))?;
        *guard = config;
        Ok(())
    }

    /// Return the configured remote agents with any cached discovery data attached.
    pub fn list_agents(&self) -> Result<Vec<RemoteAgentSummary>, A2aError> {
        let guard = self
            .config
            .read()
            .map_err(|_| A2aError::Config("A2A config lock poisoned".into()))?;
        Ok(guard
            .agents
            .iter()
            .map(|agent| {
                let base_url = normalize_base_url(&agent.url);
                let card = self.cache.get(&base_url);
                summary_from_agent(agent, card.as_ref())
            })
            .collect())
    }

    /// Discover a configured agent's card and return the enriched summary.
    pub async fn discover_agent(&self, identifier: &str) -> Result<RemoteAgentSummary, A2aError> {
        let agent = self.find_agent(identifier)?;
        let base_url = normalize_base_url(&agent.url);
        let timeout_seconds = self.request_timeout_seconds()?;
        let card = tokio::time::timeout(
            Duration::from_secs(timeout_seconds),
            crate::discover_agent(&base_url, &self.cache),
        )
        .await
        .map_err(|_| A2aError::Timeout {
            seconds: timeout_seconds,
        })??;

        Ok(summary_from_agent(&agent, Some(&card)))
    }

    /// Run a text prompt against a configured remote agent.
    pub async fn run_task(
        &self,
        identifier: &str,
        prompt: &str,
        skill_id: Option<&str>,
    ) -> Result<RemoteAgentRunResult, A2aError> {
        let agent = self.find_agent(identifier)?;
        let base_url = normalize_base_url(&agent.url);
        let timeout_seconds = self.request_timeout_seconds()?;
        let card = tokio::time::timeout(
            Duration::from_secs(timeout_seconds),
            crate::discover_agent(&base_url, &self.cache),
        )
        .await
        .map_err(|_| A2aError::Timeout {
            seconds: timeout_seconds,
        })??;

        if let Some(skill_id) = skill_id {
            if !card.skills.iter().any(|skill| skill.id == skill_id) {
                return Err(A2aError::UnsupportedSkill(skill_id.to_string()));
            }
        }

        let remote = RemoteAgent::with_card(
            agent.name.clone(),
            base_url.clone(),
            card,
            agent.api_key.clone(),
        );
        let task_id = Uuid::new_v4().to_string();
        let task = tokio::time::timeout(
            Duration::from_secs(timeout_seconds),
            remote.send_task(prompt, skill_id, &task_id),
        )
        .await
        .map_err(|_| A2aError::Timeout {
            seconds: timeout_seconds,
        })??;

        Ok(RemoteAgentRunResult {
            agent_name: agent.name,
            url: base_url,
            task_id: task.id.clone(),
            state: task_state_label(task.status.state.clone()),
            skill_id: skill_id.map(ToOwned::to_owned),
            output: RemoteAgent::extract_output(&task),
        })
    }

    fn request_timeout_seconds(&self) -> Result<u64, A2aError> {
        let guard = self
            .config
            .read()
            .map_err(|_| A2aError::Config("A2A config lock poisoned".into()))?;
        Ok(guard.client.request_timeout_seconds.max(1))
    }

    fn find_agent(&self, identifier: &str) -> Result<RemoteAgentConfig, A2aError> {
        let guard = self
            .config
            .read()
            .map_err(|_| A2aError::Config("A2A config lock poisoned".into()))?;
        let identifier = identifier.trim();
        let normalized_identifier = normalize_base_url(identifier);
        guard
            .agents
            .iter()
            .find(|agent| {
                agent.name.eq_ignore_ascii_case(identifier)
                    || normalize_base_url(&agent.url).eq_ignore_ascii_case(&normalized_identifier)
            })
            .cloned()
            .ok_or_else(|| {
                A2aError::Config(format!("No configured remote A2A agent matches '{identifier}'"))
            })
    }
}

fn normalize_base_url(url: &str) -> String {
    let trimmed = url.trim().trim_end_matches('/');
    if let Some(stripped) = trimmed.strip_suffix(AGENT_CARD_SUFFIX) {
        stripped.to_string()
    } else if let Some(stripped) = trimmed.strip_suffix(LEGACY_AGENT_CARD_SUFFIX) {
        stripped.to_string()
    } else {
        trimmed.to_string()
    }
}

fn summary_from_agent(agent: &RemoteAgentConfig, card: Option<&AgentCard>) -> RemoteAgentSummary {
    RemoteAgentSummary {
        name: agent.name.clone(),
        url: normalize_base_url(&agent.url),
        api_key_configured: agent.api_key.as_ref().is_some_and(|key| !key.is_empty()),
        discovered: card.is_some(),
        card_name: card.map(|card| card.name.clone()),
        description: card.map(|card| card.description.clone()),
        version: card.map(|card| card.version.clone()),
        skills: card
            .map(|card| card.skills.iter().map(|skill| skill.id.clone()).collect())
            .unwrap_or_default(),
    }
}

fn task_state_label(state: TaskState) -> String {
    format!("{state:?}")
}

#[async_trait]
impl OutboundA2aService for A2aClientService {
    async fn list_agents(&self) -> Result<Vec<A2aAgentRecord>, String> {
        A2aClientService::list_agents(self)
            .map(|agents| agents.into_iter().map(Into::into).collect())
            .map_err(|e| e.to_string())
    }

    async fn discover_agent(&self, identifier: &str) -> Result<A2aAgentRecord, String> {
        A2aClientService::discover_agent(self, identifier)
            .await
            .map(Into::into)
            .map_err(|e| e.to_string())
    }

    async fn run_task(
        &self,
        identifier: &str,
        prompt: &str,
        skill_id: Option<&str>,
    ) -> Result<A2aTaskRecord, String> {
        A2aClientService::run_task(self, identifier, prompt, skill_id)
            .await
            .map(Into::into)
            .map_err(|e| e.to_string())
    }
}

impl From<RemoteAgentSummary> for A2aAgentRecord {
    fn from(value: RemoteAgentSummary) -> Self {
        Self {
            name: value.name,
            url: value.url,
            api_key_configured: value.api_key_configured,
            discovered: value.discovered,
            card_name: value.card_name,
            description: value.description,
            version: value.version,
            skills: value.skills,
        }
    }
}

impl From<RemoteAgentRunResult> for A2aTaskRecord {
    fn from(value: RemoteAgentRunResult) -> Self {
        Self {
            agent_name: value.agent_name,
            url: value.url,
            task_id: value.task_id,
            state: value.state,
            skill_id: value.skill_id,
            output: value.output,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use a2a_rs::{AgentCapabilities, AgentSkill, Artifact, Part, Task, TaskStatus};
    use axum::extract::State;
    use axum::routing::{get, post};
    use axum::{Json, Router};
    use serde_json::Value;
    use tokio::net::TcpListener;

    use super::*;

    #[derive(Clone)]
    struct TestState {
        card_hits: Arc<AtomicUsize>,
        base_url: String,
    }

    fn test_card(base_url: &str) -> AgentCard {
        AgentCard {
            name: "Remote Builder".into(),
            description: "Builds and reviews code".into(),
            url: base_url.into(),
            provider: None,
            version: "1.2.3".into(),
            documentation_url: None,
            capabilities: AgentCapabilities::default(),
            security_schemes: None,
            security: None,
            default_input_modes: vec!["text".into()],
            default_output_modes: vec!["text".into()],
            skills: vec![
                AgentSkill::new(
                    "build".into(),
                    "Build".into(),
                    "Build software".into(),
                    vec!["compile".into()],
                ),
                AgentSkill::new(
                    "review".into(),
                    "Review".into(),
                    "Review changes".into(),
                    vec!["review".into()],
                ),
            ],
            supports_authenticated_extended_card: None,
        }
    }

    async fn test_card_handler(
        State(state): State<TestState>,
    ) -> Json<AgentCard> {
        state.card_hits.fetch_add(1, Ordering::SeqCst);
        Json(test_card(&state.base_url))
    }

    async fn test_send_handler(Json(payload): Json<Value>) -> Json<Value> {
        let skill_id = payload["params"]["message"]["metadata"]["skill_id"]
            .as_str()
            .map(ToOwned::to_owned);
        let task = Task {
            id: "remote-task-1".into(),
            context_id: "ctx-1".into(),
            status: TaskStatus {
                state: TaskState::Completed,
                message: None,
                timestamp: None,
            },
            artifacts: Some(vec![Artifact {
                artifact_id: "artifact-1".into(),
                name: None,
                description: None,
                parts: vec![Part::text(format!(
                    "Ran {}",
                    skill_id.unwrap_or_else(|| "default".into())
                ))],
                metadata: None,
            }]),
            history: None,
            metadata: None,
            kind: "task".into(),
        };

        Json(serde_json::json!({
            "jsonrpc": "2.0",
            "id": "rpc-1",
            "result": task,
        }))
    }

    async fn start_test_server() -> (String, Arc<AtomicUsize>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let base_url = format!("http://{}", addr);
        let hits = Arc::new(AtomicUsize::new(0));
        let state = TestState {
            card_hits: Arc::clone(&hits),
            base_url: base_url.clone(),
        };
        let app = Router::new()
            .route("/.well-known/agent-card.json", get(test_card_handler))
            .route("/a2a", post(test_send_handler))
            .with_state(state);

        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        (base_url, hits)
    }

    fn write_test_config(path: &Path, base_url: &str) {
        let config = format!(
            r#"[server]
enabled = false

[client]
discovery_cache_ttl_seconds = 300
request_timeout_seconds = 5

[[agents]]
name = "remote-builder"
url = "{base_url}"
api_key = "secret"
"#
        );
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, config).unwrap();
    }

    fn temp_config_path() -> PathBuf {
        std::env::temp_dir().join(format!("hive_a2a_service_{}.toml", Uuid::new_v4()))
    }

    #[tokio::test]
    async fn config_driven_agent_loading() {
        let path = temp_config_path();
        write_test_config(&path, "http://127.0.0.1:9999");
        let service = A2aClientService::load_or_create(&path).unwrap();

        let agents = service.list_agents().unwrap();
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].name, "remote-builder");
        assert_eq!(agents[0].url, "http://127.0.0.1:9999");
        assert!(agents[0].api_key_configured);

        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn discovery_success_and_skill_extraction() {
        let (base_url, hits) = start_test_server().await;
        let path = temp_config_path();
        write_test_config(&path, &format!("{base_url}/.well-known/agent-card.json"));
        let service = A2aClientService::load_or_create(&path).unwrap();

        let summary = service.discover_agent("remote-builder").await.unwrap();
        assert!(summary.discovered);
        assert_eq!(summary.card_name.as_deref(), Some("Remote Builder"));
        assert_eq!(summary.skills, vec!["build", "review"]);

        let cached = service.discover_agent("remote-builder").await.unwrap();
        assert_eq!(cached.skills, vec!["build", "review"]);
        assert_eq!(hits.load(Ordering::SeqCst), 1);

        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn discovery_failure_propagates() {
        let path = temp_config_path();
        write_test_config(&path, "http://127.0.0.1:9");
        let service = A2aClientService::load_or_create(&path).unwrap();

        let err = service.discover_agent("remote-builder").await.unwrap_err();
        assert!(matches!(err, A2aError::Network(_)));

        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn task_execution_and_error_propagation() {
        let (base_url, _) = start_test_server().await;
        let path = temp_config_path();
        write_test_config(&path, &base_url);
        let service = A2aClientService::load_or_create(&path).unwrap();

        let result = service
            .run_task("remote-builder", "Build this project", Some("build"))
            .await
            .unwrap();
        assert_eq!(result.agent_name, "remote-builder");
        assert_eq!(result.skill_id.as_deref(), Some("build"));
        assert_eq!(result.state, "Completed");
        assert!(result.output.contains("Ran build"));

        let err = service
            .run_task("remote-builder", "Do something", Some("missing-skill"))
            .await
            .unwrap_err();
        assert!(matches!(err, A2aError::UnsupportedSkill(_)));

        let _ = std::fs::remove_file(path);
    }
}
