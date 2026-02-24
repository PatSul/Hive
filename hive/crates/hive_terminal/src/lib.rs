// Phase 3: Terminal execution, local AI detection

pub mod browser;
pub mod cli;
pub mod docker;
pub mod executor;
pub mod local_ai;
pub mod sandbox;
pub mod shell;

pub use browser::{
    ActionResult, BrowserAction, BrowserAutomation, BrowserInstance, BrowserPool,
    BrowserPoolConfig, CdpBrowserManager, CdpConnection, CdpPageInfo,
};
pub use cli::{CheckStatus, CliCommand, CliOutput, CliService, CommandArg, DoctorCheck};
pub use docker::{
    Container, ContainerConfig, ContainerStatus, DockerSandbox, ExecResult, ResourceLimits,
    VolumeMount,
};
pub use executor::{CommandExecutor, CommandOutput};
pub use local_ai::{OllamaManager, OllamaModelInfo, PullProgress};
pub use sandbox::{AgentSandbox, SandboxConfig, SharedSandbox, shared_sandbox};
pub use shell::{InteractiveShell, ShellOutput};
