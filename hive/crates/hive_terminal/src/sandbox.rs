//! AgentSandbox — high-level wrapper around `DockerSandbox` that provides
//! a ready-to-use isolated execution environment for AI agents.
//!
//! Features:
//! - Single-container lifecycle (start → exec → stop)
//! - Workspace bind-mount for file exchange
//! - Resource limits (memory, CPU, timeout)
//! - Network isolation by default
//! - Snapshot support for reproducibility

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tracing::{debug, warn};

use crate::docker::{ContainerConfig, DockerSandbox, ExecResult, ResourceLimits, VolumeMount};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for an agent sandbox environment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    /// Docker image to use (default: `"hive-sandbox:latest"`).
    pub image: String,
    /// Memory limit in megabytes (default: 512).
    pub memory_mb: u64,
    /// CPU core limit (default: 1.0).
    pub cpu_cores: f64,
    /// Per-command timeout in seconds (default: 300).
    pub timeout_secs: u64,
    /// Whether networking is enabled inside the container (default: false).
    pub network_enabled: bool,
    /// Keep the container alive between exec calls within a session (default: true).
    pub persist_between_calls: bool,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            image: "hive-sandbox:latest".into(),
            memory_mb: 512,
            cpu_cores: 1.0,
            timeout_secs: 300,
            network_enabled: false,
            persist_between_calls: true,
        }
    }
}

// ---------------------------------------------------------------------------
// AgentSandbox
// ---------------------------------------------------------------------------

/// A high-level sandbox that wraps `DockerSandbox` to provide a managed
/// execution environment for AI agents.
///
/// Typical lifecycle:
/// ```text
///   let sandbox = AgentSandbox::new(workspace, config);
///   sandbox.start()?;          // create & start container
///   sandbox.exec("ls -la")?;   // run commands inside
///   sandbox.stop()?;           // tear down
/// ```
pub struct AgentSandbox {
    docker: DockerSandbox,
    container_id: Option<String>,
    workspace_mount: PathBuf,
    config: SandboxConfig,
}

impl AgentSandbox {
    /// Create a new sandbox. The `workspace` directory will be bind-mounted
    /// into the container at `/workspace`.
    pub fn new(workspace: impl Into<PathBuf>, config: SandboxConfig) -> Self {
        Self {
            docker: DockerSandbox::new(),
            container_id: None,
            workspace_mount: workspace.into(),
            config,
        }
    }

    /// Create a sandbox with a pre-built `DockerSandbox` instance (useful for
    /// testing with simulation mode).
    pub fn with_docker(
        docker: DockerSandbox,
        workspace: impl Into<PathBuf>,
        config: SandboxConfig,
    ) -> Self {
        Self {
            docker,
            container_id: None,
            workspace_mount: workspace.into(),
            config,
        }
    }

    /// Whether this sandbox is in Docker simulation mode.
    pub fn is_simulation(&self) -> bool {
        self.docker.is_simulation()
    }

    /// Whether the container is currently running.
    pub fn is_running(&self) -> bool {
        self.container_id.is_some()
    }

    /// Return the container ID if started.
    pub fn container_id(&self) -> Option<&str> {
        self.container_id.as_deref()
    }

    /// Start the sandbox by creating and starting a container.
    pub fn start(&mut self) -> Result<()> {
        if self.container_id.is_some() {
            bail!("Sandbox already started");
        }

        let workspace_str = self.workspace_mount.to_string_lossy().to_string();

        let container_config = ContainerConfig {
            image: self.config.image.clone(),
            name: None,
            env_vars: Default::default(),
            volumes: vec![VolumeMount {
                host_path: workspace_str,
                container_path: "/workspace".into(),
                read_only: false,
            }],
            resource_limits: ResourceLimits {
                memory_mb: Some(self.config.memory_mb),
                cpu_cores: Some(self.config.cpu_cores),
                disk_mb: None,
                timeout_secs: Some(self.config.timeout_secs),
            },
            working_dir: Some("/workspace".into()),
            network_enabled: self.config.network_enabled,
        };

        let id = self.docker.create_container(container_config)?;
        self.docker.start_container(&id)?;
        debug!(container_id = %id, "sandbox started");
        self.container_id = Some(id);
        Ok(())
    }

    /// Execute a command inside the running container.
    pub fn exec(&self, command: &str) -> Result<ExecResult> {
        let id = self
            .container_id
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Sandbox not started"))?;

        self.docker.exec_in_container(id, command)
    }

    /// Stop and remove the container.
    pub fn stop(&mut self) -> Result<()> {
        if let Some(ref id) = self.container_id {
            // Best-effort stop + remove.
            if let Err(e) = self.docker.stop_container(id) {
                warn!(error = %e, "failed to stop sandbox container");
            }
            if let Err(e) = self.docker.remove_container(id) {
                warn!(error = %e, "failed to remove sandbox container");
            }
            debug!(container_id = %id, "sandbox stopped");
        }
        self.container_id = None;
        Ok(())
    }

    /// Commit the current container state as a Docker image for
    /// reproducibility.
    pub fn snapshot(&self, tag: &str) -> Result<String> {
        let id = self
            .container_id
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Sandbox not started"))?;

        let output = std::process::Command::new("docker")
            .args(["commit", id, tag])
            .output()
            .map_err(|e| anyhow::anyhow!("Failed to run docker commit: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("docker commit failed: {stderr}");
        }

        let image_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
        debug!(tag, image_id = %image_id, "sandbox snapshot created");
        Ok(image_id)
    }

    /// Get the workspace path.
    pub fn workspace(&self) -> &Path {
        &self.workspace_mount
    }

    /// Get the sandbox configuration.
    pub fn config(&self) -> &SandboxConfig {
        &self.config
    }
}

impl Drop for AgentSandbox {
    fn drop(&mut self) {
        if self.container_id.is_some() {
            if let Err(e) = self.stop() {
                warn!(error = %e, "failed to cleanup sandbox on drop");
            }
        }
    }
}

/// Thread-safe shared sandbox reference.
pub type SharedSandbox = Arc<Mutex<AgentSandbox>>;

/// Create a new `SharedSandbox`.
pub fn shared_sandbox(sandbox: AgentSandbox) -> SharedSandbox {
    Arc::new(Mutex::new(sandbox))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::docker::DockerSandbox;

    fn simulated_sandbox() -> AgentSandbox {
        let docker = DockerSandbox::new_simulated();
        AgentSandbox::with_docker(docker, "/tmp/test-workspace", SandboxConfig::default())
    }

    #[test]
    fn default_config() {
        let config = SandboxConfig::default();
        assert_eq!(config.image, "hive-sandbox:latest");
        assert_eq!(config.memory_mb, 512);
        assert!((config.cpu_cores - 1.0).abs() < f64::EPSILON);
        assert_eq!(config.timeout_secs, 300);
        assert!(!config.network_enabled);
        assert!(config.persist_between_calls);
    }

    #[test]
    fn start_and_exec_simulated() {
        let mut sandbox = simulated_sandbox();
        assert!(!sandbox.is_running());

        sandbox.start().unwrap();
        assert!(sandbox.is_running());
        assert!(sandbox.container_id().is_some());

        let result = sandbox.exec("echo hello").unwrap();
        assert_eq!(result.exit_code, 0);

        sandbox.stop().unwrap();
        assert!(!sandbox.is_running());
    }

    #[test]
    fn double_start_errors() {
        let mut sandbox = simulated_sandbox();
        sandbox.start().unwrap();
        assert!(sandbox.start().is_err());
    }

    #[test]
    fn exec_without_start_errors() {
        let sandbox = simulated_sandbox();
        assert!(sandbox.exec("echo hello").is_err());
    }

    #[test]
    fn stop_without_start_is_ok() {
        let mut sandbox = simulated_sandbox();
        assert!(sandbox.stop().is_ok());
    }

    #[test]
    fn is_simulation() {
        let sandbox = simulated_sandbox();
        assert!(sandbox.is_simulation());
    }

    #[test]
    fn workspace_path() {
        let sandbox = simulated_sandbox();
        assert_eq!(sandbox.workspace(), Path::new("/tmp/test-workspace"));
    }

    #[test]
    fn config_accessors() {
        let config = SandboxConfig {
            memory_mb: 1024,
            network_enabled: true,
            ..Default::default()
        };
        let sandbox = AgentSandbox::with_docker(DockerSandbox::new_simulated(), "/tmp/ws", config);
        assert_eq!(sandbox.config().memory_mb, 1024);
        assert!(sandbox.config().network_enabled);
    }

    #[test]
    fn shared_sandbox_thread_safety() {
        let sandbox = simulated_sandbox();
        let shared = shared_sandbox(sandbox);
        let handle = {
            let shared = Arc::clone(&shared);
            std::thread::spawn(move || {
                let mut sb = shared.lock().unwrap();
                sb.start().unwrap();
                assert!(sb.is_running());
            })
        };
        handle.join().unwrap();
        let sb = shared.lock().unwrap();
        assert!(sb.is_running());
    }

    #[test]
    fn drop_stops_container() {
        let mut sandbox = simulated_sandbox();
        sandbox.start().unwrap();
        let id = sandbox.container_id().unwrap().to_string();
        assert!(!id.is_empty());
        // Drop should clean up without panicking.
        drop(sandbox);
    }

    #[test]
    fn config_serialization() {
        let config = SandboxConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let restored: SandboxConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.image, config.image);
        assert_eq!(restored.memory_mb, config.memory_mb);
    }
}
