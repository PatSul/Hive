// Phase 3: Terminal execution, local AI detection

pub mod browser;
pub mod cli;
pub mod docker;
pub mod executor;
pub mod local_ai;
pub mod sandbox;
pub mod shell;

pub use cli::{CheckStatus, CliCommand, CliOutput, CliService, CommandArg, DoctorCheck};
pub use docker::{
    Container, ContainerConfig, ContainerStatus, DockerSandbox, ExecResult, ResourceLimits,
    VolumeMount,
};
pub use executor::{CommandExecutor, CommandOutput};
pub use sandbox::{AgentSandbox, SandboxConfig, SharedSandbox, shared_sandbox};
pub use local_ai::{LocalAiDetector, LocalProviderInfo, OllamaManager, OllamaModelInfo, PullProgress};
pub use shell::{InteractiveShell, ShellOutput};
