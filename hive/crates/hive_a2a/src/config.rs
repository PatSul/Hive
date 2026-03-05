//! A2A configuration types.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::A2aError;

/// Top-level A2A configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct A2aConfig {
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub client: ClientConfig,
    #[serde(default)]
    pub agents: Vec<RemoteAgentConfig>,
}

impl Default for A2aConfig {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            client: ClientConfig::default(),
            agents: Vec::new(),
        }
    }
}

impl A2aConfig {
    /// Load config from a TOML file, or create a default file if it doesn't exist.
    pub fn load_or_create(path: &Path) -> Result<Self, A2aError> {
        if path.exists() {
            let contents = std::fs::read_to_string(path).map_err(|e| {
                A2aError::Config(format!("Failed to read {}: {}", path.display(), e))
            })?;
            let config: A2aConfig = toml::from_str(&contents).map_err(|e| {
                A2aError::Config(format!("Failed to parse {}: {}", path.display(), e))
            })?;
            Ok(config)
        } else {
            let config = Self::default();
            let toml_str = toml::to_string_pretty(&config)
                .map_err(|e| A2aError::Config(format!("Failed to serialize config: {}", e)))?;
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    A2aError::Config(format!(
                        "Failed to create directory {}: {}",
                        parent.display(),
                        e
                    ))
                })?;
            }
            std::fs::write(path, &toml_str).map_err(|e| {
                A2aError::Config(format!("Failed to write {}: {}", path.display(), e))
            })?;
            Ok(config)
        }
    }

    /// Returns the bind address as "host:port".
    pub fn bind_addr(&self) -> String {
        format!("{}:{}", self.server.bind, self.server.port)
    }
}

/// Server-side configuration.
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
    #[serde(default = "default_max_concurrent_tasks")]
    pub max_concurrent_tasks: usize,
    #[serde(default = "default_rate_limit_rpm")]
    pub rate_limit_rpm: u32,
    #[serde(default)]
    pub defaults: ServerDefaults,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            bind: "127.0.0.1".to_string(),
            port: 7420,
            api_key: None,
            max_concurrent_tasks: 10,
            rate_limit_rpm: 60,
            defaults: ServerDefaults::default(),
        }
    }
}

/// Default limits for server tasks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerDefaults {
    #[serde(default = "default_max_budget_usd")]
    pub max_budget_usd: f64,
    #[serde(default = "default_max_time_seconds")]
    pub max_time_seconds: u64,
    #[serde(default = "default_skill")]
    pub default_skill: String,
}

impl Default for ServerDefaults {
    fn default() -> Self {
        Self {
            max_budget_usd: 1.0,
            max_time_seconds: 300,
            default_skill: "hivemind".to_string(),
        }
    }
}

/// Client-side configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientConfig {
    #[serde(default = "default_discovery_cache_ttl")]
    pub discovery_cache_ttl_seconds: u64,
    #[serde(default = "default_request_timeout")]
    pub request_timeout_seconds: u64,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            discovery_cache_ttl_seconds: 300,
            request_timeout_seconds: 60,
        }
    }
}

/// Configuration for a remote A2A agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteAgentConfig {
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub api_key: Option<String>,
}

// Serde default helper functions
fn default_true() -> bool {
    true
}
fn default_bind() -> String {
    "127.0.0.1".to_string()
}
fn default_port() -> u16 {
    7420
}
fn default_max_concurrent_tasks() -> usize {
    10
}
fn default_rate_limit_rpm() -> u32 {
    60
}
fn default_max_budget_usd() -> f64 {
    1.0
}
fn default_max_time_seconds() -> u64 {
    300
}
fn default_skill() -> String {
    "hivemind".to_string()
}
fn default_discovery_cache_ttl() -> u64 {
    300
}
fn default_request_timeout() -> u64 {
    60
}

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
        assert_eq!(config.server.defaults.max_budget_usd, 1.0);
        assert_eq!(config.server.defaults.max_time_seconds, 300);
        assert_eq!(config.server.defaults.default_skill, "hivemind");
        assert_eq!(config.client.discovery_cache_ttl_seconds, 300);
        assert_eq!(config.client.request_timeout_seconds, 60);
        assert!(config.agents.is_empty());
        assert!(config.server.api_key.is_none());
    }

    #[test]
    fn test_parse_toml_config() {
        let toml_str = r#"
[server]
enabled = false
bind = "0.0.0.0"
port = 9000
api_key = "secret-key-123"
max_concurrent_tasks = 20
rate_limit_rpm = 120

[server.defaults]
max_budget_usd = 5.0
max_time_seconds = 600
default_skill = "coordinator"

[client]
discovery_cache_ttl_seconds = 600
request_timeout_seconds = 120

[[agents]]
name = "remote-coder"
url = "https://remote.example.com/.well-known/agent.json"
api_key = "remote-key"

[[agents]]
name = "local-reviewer"
url = "http://localhost:8080/.well-known/agent.json"
"#;
        let config: A2aConfig = toml::from_str(toml_str).unwrap();
        assert!(!config.server.enabled);
        assert_eq!(config.server.bind, "0.0.0.0");
        assert_eq!(config.server.port, 9000);
        assert_eq!(config.server.api_key.as_deref(), Some("secret-key-123"));
        assert_eq!(config.server.max_concurrent_tasks, 20);
        assert_eq!(config.server.rate_limit_rpm, 120);
        assert_eq!(config.server.defaults.max_budget_usd, 5.0);
        assert_eq!(config.server.defaults.max_time_seconds, 600);
        assert_eq!(config.server.defaults.default_skill, "coordinator");
        assert_eq!(config.client.discovery_cache_ttl_seconds, 600);
        assert_eq!(config.client.request_timeout_seconds, 120);
        assert_eq!(config.agents.len(), 2);
        assert_eq!(config.agents[0].name, "remote-coder");
        assert_eq!(
            config.agents[0].url,
            "https://remote.example.com/.well-known/agent.json"
        );
        assert_eq!(config.agents[0].api_key.as_deref(), Some("remote-key"));
        assert_eq!(config.agents[1].name, "local-reviewer");
        assert!(config.agents[1].api_key.is_none());
    }

    #[test]
    fn test_config_load_creates_default() {
        let dir = std::env::temp_dir().join("hive_a2a_test_config");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("a2a.toml");

        let config = A2aConfig::load_or_create(&path).unwrap();
        assert_eq!(config.server.port, 7420);
        assert!(path.exists());

        // Loading from the created file should produce the same result
        let config2 = A2aConfig::load_or_create(&path).unwrap();
        assert_eq!(config2.server.port, 7420);
        assert_eq!(config2.server.bind, "127.0.0.1");

        // Clean up
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_bind_addr() {
        let config = A2aConfig::default();
        assert_eq!(config.bind_addr(), "127.0.0.1:7420");
    }

    #[test]
    fn test_bind_addr_custom() {
        let mut config = A2aConfig::default();
        config.server.bind = "0.0.0.0".to_string();
        config.server.port = 9000;
        assert_eq!(config.bind_addr(), "0.0.0.0:9000");
    }

    #[test]
    fn test_partial_toml_uses_defaults() {
        let toml_str = r#"
[server]
port = 8080
"#;
        let config: A2aConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.server.port, 8080);
        // All other fields should have defaults
        assert!(config.server.enabled);
        assert_eq!(config.server.bind, "127.0.0.1");
        assert_eq!(config.server.max_concurrent_tasks, 10);
        assert_eq!(config.client.discovery_cache_ttl_seconds, 300);
        assert!(config.agents.is_empty());
    }

    #[test]
    fn test_roundtrip_serialize() {
        let config = A2aConfig::default();
        let toml_str = toml::to_string_pretty(&config).unwrap();
        let config2: A2aConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(config2.server.port, config.server.port);
        assert_eq!(config2.server.bind, config.server.bind);
        assert_eq!(config2.server.enabled, config.server.enabled);
        assert_eq!(
            config2.client.request_timeout_seconds,
            config.client.request_timeout_seconds
        );
    }
}
