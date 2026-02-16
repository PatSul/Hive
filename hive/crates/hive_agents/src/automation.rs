//! Automation Workflows — define, manage, and simulate event-driven workflows.
//!
//! Mirrors the Electron app's `automation-service.ts` with trigger-based
//! workflows containing conditional steps, lifecycle management, simulated
//! execution, and run-history tracking.

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::{debug, warn};
use uuid::Uuid;
use std::time::Duration;

use hive_core::channels::{ChannelMessage, MessageAuthor};
use hive_core::config::HiveConfig;
use hive_core::kanban::{KanbanBoard, Priority};
use hive_core::notifications::{AppNotification, NotificationType};
use hive_terminal::executor::CommandExecutor;

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// The event that initiates a workflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum TriggerType {
    Schedule { cron: String },
    FileChange { path: String },
    WebhookReceived { event: String },
    ManualTrigger,
    OnMessage { pattern: String },
    OnError { source: String },
}

/// Comparison operators for workflow step conditions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConditionOp {
    Equals,
    NotEquals,
    Contains,
    GreaterThan,
    LessThan,
    Matches,
}

/// The action a workflow step performs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum ActionType {
    RunCommand {
        command: String,
    },
    SendMessage {
        channel: String,
        content: String,
    },
    CallApi {
        url: String,
        method: String,
    },
    CreateTask {
        title: String,
    },
    SendNotification {
        title: String,
        body: String,
    },
    ExecuteSkill {
        skill_trigger: String,
        input: String,
    },
}

/// Lifecycle status of a workflow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowStatus {
    Draft,
    Active,
    Paused,
    Completed,
    Failed,
}

// ---------------------------------------------------------------------------
// Data Types
// ---------------------------------------------------------------------------

/// A predicate that must be satisfied before a step executes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Condition {
    pub field: String,
    pub operator: ConditionOp,
    pub value: String,
}

/// A single step within a workflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowStep {
    pub id: String,
    pub name: String,
    pub action: ActionType,
    pub conditions: Vec<Condition>,
    pub timeout_secs: Option<u64>,
    pub retry_count: u32,
}

/// A complete automation workflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workflow {
    pub id: String,
    pub name: String,
    pub description: String,
    pub trigger: TriggerType,
    pub steps: Vec<WorkflowStep>,
    pub status: WorkflowStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_run: Option<DateTime<Utc>>,
    pub run_count: u32,
}

/// The result of executing (or simulating) a workflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowRunResult {
    pub workflow_id: String,
    pub started_at: DateTime<Utc>,
    pub completed_at: DateTime<Utc>,
    pub success: bool,
    pub steps_completed: usize,
    pub error: Option<String>,
}

/// Stable ID for the built-in dogfood workflow.
pub const BUILTIN_DOGFOOD_WORKFLOW_ID: &str = "builtin:hive-dogfood-v1";

/// Workspace-relative directory for user-defined workflows.
pub const USER_WORKFLOW_DIR: &str = ".hive/workflows";

/// Minimal JSON shape for user-defined workflows.
#[derive(Debug, Clone, Deserialize)]
pub struct WorkflowTemplate {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub trigger: Option<TriggerType>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub steps: Vec<WorkflowStepTemplate>,
}

/// Minimal JSON shape for user-defined workflow steps.
#[derive(Debug, Clone, Deserialize)]
pub struct WorkflowStepTemplate {
    pub name: String,
    pub action: ActionType,
    #[serde(default)]
    pub conditions: Vec<Condition>,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    #[serde(default)]
    pub retry_count: u32,
}

/// Result of loading user workflow files.
#[derive(Debug, Clone, Default)]
pub struct WorkflowLoadReport {
    pub source_dir: PathBuf,
    pub loaded: usize,
    pub failed: usize,
    pub skipped: usize,
    pub errors: Vec<String>,
}

fn default_true() -> bool {
    true
}

// ---------------------------------------------------------------------------
// AutomationService
// ---------------------------------------------------------------------------

/// In-memory service for creating, managing, and simulating automation workflows.
#[derive(Serialize, Deserialize)]
pub struct AutomationService {
    workflows: Vec<Workflow>,
    run_history: Vec<WorkflowRunResult>,
}

impl AutomationService {
    /// Create a new automation service with no workflows.
    pub fn new() -> Self {
        Self {
            workflows: Vec::new(),
            run_history: Vec::new(),
        }
    }

    /// Create a new workflow in `Draft` status.
    pub fn create_workflow(
        &mut self,
        name: &str,
        description: &str,
        trigger: TriggerType,
    ) -> Workflow {
        let now = Utc::now();
        let workflow = Workflow {
            id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            description: description.to_string(),
            trigger,
            steps: Vec::new(),
            status: WorkflowStatus::Draft,
            created_at: now,
            updated_at: now,
            last_run: None,
            run_count: 0,
        };
        debug!(name, id = %workflow.id, "Created workflow");
        self.workflows.push(workflow.clone());
        workflow
    }

    /// Ensure that built-in workflows are present.
    pub fn ensure_builtin_workflows(&mut self) {
        if self
            .workflows
            .iter()
            .any(|wf| wf.id == BUILTIN_DOGFOOD_WORKFLOW_ID)
        {
            return;
        }

        let now = Utc::now();
        let workflow = Workflow {
            id: BUILTIN_DOGFOOD_WORKFLOW_ID.to_string(),
            name: "Local Build Check".to_string(),
            description:
                "Run a local validation loop: quick build check, core tests, and git state checks."
                    .to_string(),
            trigger: TriggerType::ManualTrigger,
            steps: vec![
                WorkflowStep {
                    id: "builtin:hive-dogfood-v1:step-1".to_string(),
                    name: "Cargo check".to_string(),
                    action: ActionType::RunCommand {
                        command: "cargo check --quiet".to_string(),
                    },
                    conditions: Vec::new(),
                    timeout_secs: Some(900),
                    retry_count: 0,
                },
                WorkflowStep {
                    id: "builtin:hive-dogfood-v1:step-2".to_string(),
                    name: "Run tests (hive_app)".to_string(),
                    action: ActionType::RunCommand {
                        command: "cargo test --quiet -p hive_app".to_string(),
                    },
                    conditions: Vec::new(),
                    timeout_secs: Some(1200),
                    retry_count: 0,
                },
                WorkflowStep {
                    id: "builtin:hive-dogfood-v1:step-3".to_string(),
                    name: "Git status".to_string(),
                    action: ActionType::RunCommand {
                        command: "git status --short".to_string(),
                    },
                    conditions: Vec::new(),
                    timeout_secs: Some(120),
                    retry_count: 0,
                },
                WorkflowStep {
                    id: "builtin:hive-dogfood-v1:step-4".to_string(),
                    name: "Git diff stat".to_string(),
                    action: ActionType::RunCommand {
                        command: "git diff --stat".to_string(),
                    },
                    conditions: Vec::new(),
                    timeout_secs: Some(120),
                    retry_count: 0,
                },
            ],
            status: WorkflowStatus::Active,
            created_at: now,
            updated_at: now,
            last_run: None,
            run_count: 0,
        };

        self.workflows.push(workflow);

        // -- Additional built-in workflow templates --

        self.ensure_template("builtin:build-test-v1", "Build & Test",
            "Full build + test pipeline: cargo check, build release, run all tests, notify.",
            vec![
                ("Check", "cargo check --quiet"),
                ("Build", "cargo build --release --quiet"),
                ("Test", "cargo test --quiet"),
            ],
        );

        self.ensure_template("builtin:code-review-v1", "Code Review",
            "AI-powered code review: investigate changes, critique code, notify results.",
            vec![
                ("Investigate changes", "git diff --stat"),
                ("Run clippy", "cargo clippy --quiet -- -W clippy::all"),
                ("Run tests", "cargo test --quiet"),
            ],
        );

        self.ensure_template("builtin:debug-issue-v1", "Debug Issue",
            "Debug workflow: reproduce issue, investigate root cause, implement fix, verify.",
            vec![
                ("Reproduce", "cargo test --quiet 2>&1 || true"),
                ("Investigate", "git log --oneline -10"),
                ("Verify", "cargo test --quiet"),
            ],
        );

        self.ensure_template("builtin:deploy-v1", "Deploy",
            "Deploy pipeline: test, build release, deploy, notify team.",
            vec![
                ("Test", "cargo test --quiet"),
                ("Build release", "cargo build --release --quiet"),
                ("Check binary", "ls -la target/release/"),
            ],
        );

        self.ensure_template("builtin:research-impl-v1", "Research & Implement",
            "Research-first workflow: investigate approaches, implement solution, verify, review.",
            vec![
                ("Investigate", "git log --oneline -20"),
                ("Implement", "cargo check --quiet"),
                ("Verify", "cargo test --quiet"),
                ("Review", "cargo clippy --quiet -- -W clippy::all"),
            ],
        );

        self.ensure_template("builtin:full-pipeline-v1", "Full Pipeline",
            "End-to-end pipeline: investigate, implement, verify, critique, debug, notify.",
            vec![
                ("Investigate", "git status --short"),
                ("Build", "cargo check --quiet"),
                ("Test", "cargo test --quiet"),
                ("Lint", "cargo clippy --quiet -- -W clippy::all"),
                ("Verify", "cargo test --quiet"),
            ],
        );
    }

    /// Internal helper: ensure a template workflow exists by ID.
    fn ensure_template(&mut self, id: &str, name: &str, desc: &str, steps: Vec<(&str, &str)>) {
        if self.workflows.iter().any(|wf| wf.id == id) {
            return;
        }
        let now = Utc::now();
        let wf_steps: Vec<WorkflowStep> = steps
            .iter()
            .enumerate()
            .map(|(i, (step_name, cmd))| WorkflowStep {
                id: format!("{}:step-{}", id, i + 1),
                name: step_name.to_string(),
                action: ActionType::RunCommand {
                    command: cmd.to_string(),
                },
                conditions: Vec::new(),
                timeout_secs: Some(900),
                retry_count: 0,
            })
            .collect();

        self.workflows.push(Workflow {
            id: id.to_string(),
            name: name.to_string(),
            description: desc.to_string(),
            trigger: TriggerType::ManualTrigger,
            steps: wf_steps,
            status: WorkflowStatus::Active,
            created_at: now,
            updated_at: now,
            last_run: None,
            run_count: 0,
        });
    }

    /// Replace file-based workflows by reading JSON templates from
    /// `<workspace>/.hive/workflows/*.json`.
    pub fn reload_user_workflows(&mut self, workspace_root: &Path) -> WorkflowLoadReport {
        let source_dir = workspace_root.join(USER_WORKFLOW_DIR);
        let mut report = WorkflowLoadReport {
            source_dir: source_dir.clone(),
            ..WorkflowLoadReport::default()
        };

        // Keep built-in and runtime-created workflows, replace only file imports.
        self.workflows.retain(|wf| !wf.id.starts_with("file:"));

        if !source_dir.exists() {
            return report;
        }

        let entries = match std::fs::read_dir(&source_dir) {
            Ok(entries) => entries,
            Err(e) => {
                report.failed += 1;
                report
                    .errors
                    .push(format!("{}: failed to read directory: {e}", source_dir.display()));
                return report;
            }
        };

        let mut paths: Vec<PathBuf> = entries
            .filter_map(|entry| entry.ok().map(|e| e.path()))
            .filter(|path| path.is_file())
            .collect();
        paths.sort();

        for path in paths {
            let is_json = path
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("json"));
            if !is_json {
                report.skipped += 1;
                continue;
            }

            let result = Self::parse_workflow_template(&path).and_then(|template| {
                Self::validate_workflow_template(&template)?;
                self.install_template_from_template(&path, template)
            });

            match result {
                Ok(()) => report.loaded += 1,
                Err(e) => {
                    report.failed += 1;
                    report.errors.push(format!("{}: {e}", path.display()));
                }
            }
        }

        report
    }

    /// Convenience helper for startup: install built-ins and load user files.
    pub fn initialize_workflows(&mut self, workspace_root: &Path) -> WorkflowLoadReport {
        self.ensure_builtin_workflows();
        let _ = std::fs::create_dir_all(workspace_root.join(USER_WORKFLOW_DIR));
        self.reload_user_workflows(workspace_root)
    }

    /// Clone a workflow for execution on a background thread.
    pub fn clone_workflow(&self, workflow_id: &str) -> Option<Workflow> {
        self.get_workflow(workflow_id).cloned()
    }

    /// Add a step to an existing workflow.
    pub fn add_step(
        &mut self,
        workflow_id: &str,
        name: &str,
        action: ActionType,
    ) -> Result<WorkflowStep> {
        self.add_step_with_conditions(workflow_id, name, action, Vec::new())
    }

    /// Add a step with conditions to an existing workflow.
    pub fn add_step_with_conditions(
        &mut self,
        workflow_id: &str,
        name: &str,
        action: ActionType,
        conditions: Vec<Condition>,
    ) -> Result<WorkflowStep> {
        let workflow = self
            .workflows
            .iter_mut()
            .find(|w| w.id == workflow_id)
            .ok_or_else(|| anyhow::anyhow!("Workflow '{}' not found", workflow_id))?;

        let step = WorkflowStep {
            id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            action,
            conditions,
            timeout_secs: None,
            retry_count: 0,
        };

        workflow.steps.push(step.clone());
        workflow.updated_at = Utc::now();
        debug!(workflow_id, step_name = name, "Added step to workflow");
        Ok(step)
    }

    /// Activate a workflow so it can be triggered.
    pub fn activate_workflow(&mut self, id: &str) -> Result<()> {
        let workflow = self
            .workflows
            .iter_mut()
            .find(|w| w.id == id)
            .ok_or_else(|| anyhow::anyhow!("Workflow '{}' not found", id))?;

        workflow.status = WorkflowStatus::Active;
        workflow.updated_at = Utc::now();
        debug!(id, "Activated workflow");
        Ok(())
    }

    /// Pause an active workflow.
    pub fn pause_workflow(&mut self, id: &str) -> Result<()> {
        let workflow = self
            .workflows
            .iter_mut()
            .find(|w| w.id == id)
            .ok_or_else(|| anyhow::anyhow!("Workflow '{}' not found", id))?;

        workflow.status = WorkflowStatus::Paused;
        workflow.updated_at = Utc::now();
        debug!(id, "Paused workflow");
        Ok(())
    }

    /// Delete a workflow by ID.
    pub fn delete_workflow(&mut self, id: &str) -> Result<()> {
        let before = self.workflows.len();
        self.workflows.retain(|w| w.id != id);
        if self.workflows.len() == before {
            bail!("Workflow '{}' not found", id);
        }
        debug!(id, "Deleted workflow");
        Ok(())
    }

    /// Look up a workflow by ID.
    pub fn get_workflow(&self, id: &str) -> Option<&Workflow> {
        self.workflows.iter().find(|w| w.id == id)
    }

    /// Return all workflows.
    pub fn list_workflows(&self) -> &[Workflow] {
        &self.workflows
    }

    /// Return only workflows with `Active` status.
    pub fn list_active_workflows(&self) -> Vec<&Workflow> {
        self.workflows
            .iter()
            .filter(|w| w.status == WorkflowStatus::Active)
            .collect()
    }

    /// Simulate executing a workflow. All steps are "run" in order and a
    /// `WorkflowRunResult` is produced. The workflow's `run_count` and
    /// `last_run` are updated.
    pub fn simulate_run(&mut self, workflow_id: &str) -> Result<WorkflowRunResult> {
        let workflow = self
            .workflows
            .iter_mut()
            .find(|w| w.id == workflow_id)
            .ok_or_else(|| anyhow::anyhow!("Workflow '{}' not found", workflow_id))?;

        if workflow.status != WorkflowStatus::Active {
            bail!(
                "Cannot run workflow '{}': status is {:?}, expected Active",
                workflow_id,
                workflow.status
            );
        }

        let started_at = Utc::now();
        let steps_completed = workflow.steps.len();

        workflow.run_count += 1;
        workflow.last_run = Some(Utc::now());
        workflow.updated_at = Utc::now();

        let result = WorkflowRunResult {
            workflow_id: workflow_id.to_string(),
            started_at,
            completed_at: Utc::now(),
            success: true,
            steps_completed,
            error: None,
        };

        self.run_history.push(result.clone());
        debug!(workflow_id, steps_completed, "Simulated workflow run");
        Ok(result)
    }

    /// Record an external run result for a workflow.
    pub fn record_run(
        &mut self,
        workflow_id: &str,
        success: bool,
        steps_completed: usize,
        error: Option<String>,
    ) -> Result<WorkflowRunResult> {
        let workflow = self
            .workflows
            .iter_mut()
            .find(|w| w.id == workflow_id)
            .ok_or_else(|| anyhow::anyhow!("Workflow '{}' not found", workflow_id))?;

        workflow.run_count += 1;
        workflow.last_run = Some(Utc::now());
        workflow.updated_at = Utc::now();

        if !success {
            workflow.status = WorkflowStatus::Failed;
        }

        let result = WorkflowRunResult {
            workflow_id: workflow_id.to_string(),
            started_at: Utc::now(),
            completed_at: Utc::now(),
            success,
            steps_completed,
            error,
        };

        self.run_history.push(result.clone());
        debug!(
            workflow_id,
            success, steps_completed, "Recorded workflow run"
        );
        Ok(result)
    }

    /// Return the most recent `limit` run results for a workflow (newest last).
    pub fn get_run_history(&self, workflow_id: &str, limit: usize) -> Vec<&WorkflowRunResult> {
        let all: Vec<&WorkflowRunResult> = self
            .run_history
            .iter()
            .filter(|r| r.workflow_id == workflow_id)
            .collect();
        let start = all.len().saturating_sub(limit);
        all[start..].to_vec()
    }

    /// Return all run results across workflows (oldest first).
    pub fn list_run_history(&self) -> &[WorkflowRunResult] {
        &self.run_history
    }

    /// Execute a workflow, handling all supported action types.
    ///
    /// This is intentionally a blocking call, suitable for running on a
    /// background thread. Commands are validated by the SecurityGateway
    /// inside `CommandExecutor`.
    ///
    /// Supported actions:
    /// - `RunCommand` — execute a shell command via `CommandExecutor`
    /// - `SendMessage` — write a message to a channel JSON file under `~/.hive/channels/`
    /// - `CallApi` — make an HTTP request via `reqwest`
    /// - `CreateTask` — add a task to the kanban board at `~/.hive/kanban.json`
    /// - `SendNotification` — write a notification JSON file under `~/.hive/notifications/`
    /// - `ExecuteSkill` — store a pending skill execution request under `~/.hive/pending_skills/`
    pub fn execute_workflow_blocking(
        workflow: &Workflow,
        working_dir: PathBuf,
    ) -> Result<WorkflowRunResult> {
        let started_at = Utc::now();

        // Run tokio-based process execution on an isolated runtime to avoid
        // assuming anything about the UI executor.
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .context("Failed to create tokio runtime for workflow execution")?;

        let executor = CommandExecutor::new(working_dir)?;

        let mut steps_completed = 0usize;
        let mut success = true;
        let mut error: Option<String> = None;

        for step in &workflow.steps {
            let step_result = match &step.action {
                ActionType::RunCommand { command } => {
                    let timeout = Duration::from_secs(step.timeout_secs.unwrap_or(30));
                    let result = rt.block_on(executor.execute_with_timeout(command, timeout));

                    match result {
                        Ok(output) if output.exit_code == 0 => Ok(()),
                        Ok(output) => {
                            let stderr = output.stderr.trim();
                            Err(if stderr.is_empty() {
                                format!(
                                    "Command failed (exit={}): {}",
                                    output.exit_code, command
                                )
                            } else {
                                format!(
                                    "Command failed (exit={}): {}\n{}",
                                    output.exit_code, command, stderr
                                )
                            })
                        }
                        Err(e) => Err(format!("Command failed: {command}\n{e}")),
                    }
                }

                ActionType::SendMessage {
                    channel,
                    content,
                } => {
                    Self::execute_send_message(channel, content)
                }

                ActionType::CallApi {
                    url,
                    method,
                } => {
                    rt.block_on(Self::execute_call_api(url, method))
                }

                ActionType::CreateTask { title } => {
                    Self::execute_create_task(title)
                }

                ActionType::SendNotification {
                    title,
                    body,
                } => {
                    Self::execute_send_notification(title, body)
                }

                ActionType::ExecuteSkill {
                    skill_trigger,
                    input,
                } => {
                    Self::execute_skill(skill_trigger, input)
                }
            };

            match step_result {
                Ok(()) => {
                    steps_completed += 1;
                }
                Err(e) => {
                    warn!(
                        workflow_id = %workflow.id,
                        step_name = %step.name,
                        "Step failed: {e}"
                    );
                    success = false;
                    error = Some(e);
                    break;
                }
            }
        }

        Ok(WorkflowRunResult {
            workflow_id: workflow.id.clone(),
            started_at,
            completed_at: Utc::now(),
            success,
            steps_completed,
            error,
        })
    }

    /// Deprecated alias for `execute_workflow_blocking`.
    #[deprecated(note = "Use execute_workflow_blocking instead")]
    pub fn execute_run_commands_blocking(
        workflow: &Workflow,
        working_dir: PathBuf,
    ) -> Result<WorkflowRunResult> {
        Self::execute_workflow_blocking(workflow, working_dir)
    }

    // -----------------------------------------------------------------------
    // Action handlers (private)
    // -----------------------------------------------------------------------

    /// Send a message to a channel by writing to `~/.hive/channels/{channel}.json`.
    fn execute_send_message(channel: &str, content: &str) -> std::result::Result<(), String> {
        let base_dir = match HiveConfig::base_dir() {
            Ok(d) => d,
            Err(e) => {
                warn!("Cannot resolve ~/.hive directory for SendMessage: {e}");
                return Ok(()); // fire-and-forget
            }
        };

        let channels_dir = base_dir.join("channels");
        let _ = std::fs::create_dir_all(&channels_dir);

        // Try to find the channel file by name. Channel IDs can be the name
        // itself (e.g. "general") or a UUID. We search for a matching file.
        let channel_file = channels_dir.join(format!("{channel}.json"));

        let message = ChannelMessage {
            id: Uuid::new_v4().to_string(),
            author: MessageAuthor::System,
            content: content.to_string(),
            timestamp: Utc::now(),
            thread_id: None,
            model: None,
            cost: None,
        };

        if channel_file.exists() {
            // Load existing channel, append message, save back
            match std::fs::read_to_string(&channel_file) {
                Ok(json) => {
                    if let Ok(mut ch) = serde_json::from_str::<hive_core::channels::AgentChannel>(&json) {
                        ch.messages.push(message);
                        ch.updated_at = Utc::now();
                        let updated = serde_json::to_string_pretty(&ch)
                            .map_err(|e| format!("Failed to serialize channel: {e}"))?;
                        std::fs::write(&channel_file, updated)
                            .map_err(|e| format!("Failed to write channel file: {e}"))?;
                    } else {
                        warn!("Could not parse channel file {}", channel_file.display());
                    }
                }
                Err(e) => {
                    warn!("Could not read channel file: {e}");
                }
            }
        } else {
            // Create a minimal channel message file for pickup
            let msg_json = serde_json::to_string_pretty(&message)
                .map_err(|e| format!("Failed to serialize message: {e}"))?;
            let pending_dir = channels_dir.join("pending");
            let _ = std::fs::create_dir_all(&pending_dir);
            let pending_file = pending_dir.join(format!("{channel}_{}.json", Uuid::new_v4()));
            std::fs::write(&pending_file, msg_json)
                .map_err(|e| format!("Failed to write pending message: {e}"))?;
        }

        debug!(channel, "SendMessage action completed");
        Ok(())
    }

    /// Make an HTTP request via reqwest (async, called within the runtime).
    async fn execute_call_api(url: &str, method: &str) -> std::result::Result<(), String> {
        let client = reqwest::Client::new();

        let request = match method.to_uppercase().as_str() {
            "GET" => client.get(url),
            "POST" => client.post(url),
            "PUT" => client.put(url),
            "DELETE" => client.delete(url),
            "PATCH" => client.patch(url),
            "HEAD" => client.head(url),
            other => return Err(format!("Unsupported HTTP method: {other}")),
        };

        match request.send().await {
            Ok(response) => {
                let status = response.status();
                debug!(url, %status, "CallApi action completed");
                if status.is_client_error() || status.is_server_error() {
                    Err(format!("HTTP request to {url} returned status {status}"))
                } else {
                    Ok(())
                }
            }
            Err(e) => {
                Err(format!("HTTP request to {url} failed: {e}"))
            }
        }
    }

    /// Create a task on the kanban board at `~/.hive/kanban.json`.
    fn execute_create_task(title: &str) -> std::result::Result<(), String> {
        let base_dir = match HiveConfig::base_dir() {
            Ok(d) => d,
            Err(e) => {
                warn!("Cannot resolve ~/.hive directory for CreateTask: {e}");
                return Ok(()); // fire-and-forget
            }
        };

        let kanban_path = base_dir.join("kanban.json");
        let mut board = KanbanBoard::load_from_file(&kanban_path)
            .unwrap_or_else(|_| KanbanBoard::new());

        board.add_task(title, None, Priority::Medium);

        board
            .save_to_file(&kanban_path)
            .map_err(|e| format!("Failed to save kanban board: {e}"))?;

        debug!(title, "CreateTask action completed");
        Ok(())
    }

    /// Send a notification by writing to `~/.hive/notifications/`.
    fn execute_send_notification(title: &str, body: &str) -> std::result::Result<(), String> {
        let base_dir = match HiveConfig::base_dir() {
            Ok(d) => d,
            Err(e) => {
                warn!("Cannot resolve ~/.hive directory for SendNotification: {e}");
                return Ok(()); // fire-and-forget
            }
        };

        // Persist notification as a JSON file for the app to pick up
        let notifications_dir = base_dir.join("notifications");
        let _ = std::fs::create_dir_all(&notifications_dir);

        let notification = AppNotification::new(NotificationType::Info, body)
            .with_title(title);
        let json = serde_json::to_string_pretty(&notification)
            .map_err(|e| format!("Failed to serialize notification: {e}"))?;
        let file_path = notifications_dir.join(format!("{}.json", notification.id));
        std::fs::write(&file_path, json)
            .map_err(|e| format!("Failed to write notification file: {e}"))?;

        debug!(title, "SendNotification action completed");
        Ok(())
    }

    /// Execute a skill by storing a pending request under `~/.hive/pending_skills/`.
    ///
    /// Since `SkillsRegistry` may not be accessible in this blocking context,
    /// we write the request as a JSON file that can be picked up by the main
    /// application loop.
    fn execute_skill(skill_trigger: &str, input: &str) -> std::result::Result<(), String> {
        let base_dir = match HiveConfig::base_dir() {
            Ok(d) => d,
            Err(e) => {
                warn!("Cannot resolve ~/.hive directory for ExecuteSkill: {e}");
                return Ok(()); // fire-and-forget
            }
        };

        let pending_dir = base_dir.join("pending_skills");
        let _ = std::fs::create_dir_all(&pending_dir);

        let request_id = Uuid::new_v4().to_string();
        let request = serde_json::json!({
            "id": request_id,
            "skill_trigger": skill_trigger,
            "input": input,
            "created_at": Utc::now().to_rfc3339(),
            "status": "pending"
        });

        let json = serde_json::to_string_pretty(&request)
            .map_err(|e| format!("Failed to serialize skill request: {e}"))?;
        let file_path = pending_dir.join(format!("{request_id}.json"));
        std::fs::write(&file_path, json)
            .map_err(|e| format!("Failed to write pending skill file: {e}"))?;

        debug!(skill_trigger, "ExecuteSkill action queued for pickup");
        Ok(())
    }

    /// Return the total number of workflows.
    pub fn workflow_count(&self) -> usize {
        self.workflows.len()
    }

    /// Return the number of active workflows.
    pub fn active_count(&self) -> usize {
        self.workflows
            .iter()
            .filter(|w| w.status == WorkflowStatus::Active)
            .count()
    }

    // -----------------------------------------------------------------------
    // Persistence
    // -----------------------------------------------------------------------

    /// Persist the automation service to a JSON file.
    pub fn save_to_file(&self, path: &Path) -> Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Load an automation service from a JSON file. Returns an empty service
    /// if the file does not exist.
    pub fn load_from_file(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::new());
        }
        let json = std::fs::read_to_string(path)?;
        Ok(serde_json::from_str(&json)?)
    }

    /// Evaluate a single condition against an actual value.
    ///
    /// For `GreaterThan` and `LessThan`, both values are parsed as `f64`.
    /// For `Matches`, the condition value is compiled as a regex.
    pub fn check_condition(condition: &Condition, actual_value: &str) -> bool {
        match condition.operator {
            ConditionOp::Equals => actual_value == condition.value,
            ConditionOp::NotEquals => actual_value != condition.value,
            ConditionOp::Contains => actual_value.contains(&condition.value),
            ConditionOp::GreaterThan => {
                let actual = actual_value.parse::<f64>().unwrap_or(f64::NAN);
                let expected = condition.value.parse::<f64>().unwrap_or(f64::NAN);
                actual > expected
            }
            ConditionOp::LessThan => {
                let actual = actual_value.parse::<f64>().unwrap_or(f64::NAN);
                let expected = condition.value.parse::<f64>().unwrap_or(f64::NAN);
                actual < expected
            }
            ConditionOp::Matches => Regex::new(&condition.value)
                .map(|re| re.is_match(actual_value))
                .unwrap_or(false),
        }
    }

    fn parse_workflow_template(path: &Path) -> Result<WorkflowTemplate> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read workflow file {}", path.display()))?;
        let template: WorkflowTemplate = serde_json::from_str(&raw)
            .with_context(|| "invalid workflow JSON (expected WorkflowTemplate schema)")?;
        Ok(template)
    }

    fn validate_workflow_template(template: &WorkflowTemplate) -> Result<()> {
        if template.name.trim().is_empty() {
            bail!("workflow name must not be empty");
        }
        if template.steps.is_empty() {
            bail!("workflow must contain at least one step");
        }

        for (idx, step) in template.steps.iter().enumerate() {
            if step.name.trim().is_empty() {
                bail!("step #{} has an empty name", idx + 1);
            }

            match &step.action {
                ActionType::RunCommand { command } => {
                    if command.trim().is_empty() {
                        bail!("step '{}' has an empty command", step.name);
                    }
                    if command.contains('\n') || command.contains('\r') {
                        bail!(
                            "step '{}' has a multiline command; use a single command line",
                            step.name
                        );
                    }
                }
                ActionType::SendMessage { channel, content } => {
                    if channel.trim().is_empty() {
                        bail!("step '{}' has an empty channel", step.name);
                    }
                    if content.trim().is_empty() {
                        bail!("step '{}' has empty message content", step.name);
                    }
                }
                ActionType::CallApi { url, method } => {
                    if url.trim().is_empty() {
                        bail!("step '{}' has an empty URL", step.name);
                    }
                    let valid_methods = ["GET", "POST", "PUT", "DELETE", "PATCH", "HEAD"];
                    if !valid_methods.contains(&method.to_uppercase().as_str()) {
                        bail!(
                            "step '{}' has unsupported HTTP method: {}",
                            step.name, method
                        );
                    }
                }
                ActionType::CreateTask { title } => {
                    if title.trim().is_empty() {
                        bail!("step '{}' has an empty task title", step.name);
                    }
                }
                ActionType::SendNotification { title, body } => {
                    if title.trim().is_empty() && body.trim().is_empty() {
                        bail!(
                            "step '{}' has both empty notification title and body",
                            step.name
                        );
                    }
                }
                ActionType::ExecuteSkill { skill_trigger, .. } => {
                    if skill_trigger.trim().is_empty() {
                        bail!("step '{}' has an empty skill trigger", step.name);
                    }
                }
            }
        }

        Ok(())
    }

    fn install_template_from_template(
        &mut self,
        path: &Path,
        template: WorkflowTemplate,
    ) -> Result<()> {
        let file_stem = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .ok_or_else(|| anyhow::anyhow!("invalid workflow file name"))?;
        let workflow_id = format!("file:{}", Self::sanitize_identifier(file_stem));
        let now = Utc::now();

        let mut steps = Vec::with_capacity(template.steps.len());
        for (idx, step) in template.steps.iter().enumerate() {
            steps.push(WorkflowStep {
                id: format!("{workflow_id}:step-{}", idx + 1),
                name: step.name.clone(),
                action: step.action.clone(),
                conditions: step.conditions.clone(),
                timeout_secs: step.timeout_secs,
                retry_count: step.retry_count,
            });
        }

        let workflow = Workflow {
            id: workflow_id.clone(),
            name: template.name,
            description: template.description,
            trigger: template.trigger.unwrap_or(TriggerType::ManualTrigger),
            steps,
            status: if template.enabled {
                WorkflowStatus::Active
            } else {
                WorkflowStatus::Draft
            },
            created_at: now,
            updated_at: now,
            last_run: None,
            run_count: 0,
        };

        self.workflows.push(workflow);
        Ok(())
    }

    fn sanitize_identifier(raw: &str) -> String {
        let mut out = String::with_capacity(raw.len());
        for ch in raw.chars() {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                out.push(ch.to_ascii_lowercase());
            } else {
                out.push('-');
            }
        }
        while out.contains("--") {
            out = out.replace("--", "-");
        }
        out.trim_matches('-').to_string()
    }
}

impl Default for AutomationService {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- helpers ------------------------------------------------------------

    fn make_service_with_active_workflow() -> (AutomationService, String) {
        let mut svc = AutomationService::new();
        let wf = svc.create_workflow(
            "Deploy Pipeline",
            "Automated deployment",
            TriggerType::ManualTrigger,
        );
        let id = wf.id.clone();
        svc.add_step(
            &id,
            "Build",
            ActionType::RunCommand {
                command: "cargo build --release".into(),
            },
        )
        .unwrap();
        svc.activate_workflow(&id).unwrap();
        (svc, id)
    }

    // -- creation -----------------------------------------------------------

    #[test]
    fn create_workflow_defaults_to_draft() {
        let mut svc = AutomationService::new();
        let wf = svc.create_workflow(
            "Test Workflow",
            "A test",
            TriggerType::Schedule {
                cron: "0 * * * *".into(),
            },
        );

        assert_eq!(wf.name, "Test Workflow");
        assert_eq!(wf.description, "A test");
        assert_eq!(wf.status, WorkflowStatus::Draft);
        assert!(wf.steps.is_empty());
        assert_eq!(wf.run_count, 0);
        assert!(wf.last_run.is_none());
        assert_eq!(svc.workflow_count(), 1);
    }

    #[test]
    fn create_multiple_workflows() {
        let mut svc = AutomationService::new();
        svc.create_workflow("A", "first", TriggerType::ManualTrigger);
        svc.create_workflow("B", "second", TriggerType::ManualTrigger);
        svc.create_workflow("C", "third", TriggerType::ManualTrigger);

        assert_eq!(svc.workflow_count(), 3);
        assert_eq!(svc.list_workflows().len(), 3);
    }

    // -- steps --------------------------------------------------------------

    #[test]
    fn add_step_to_workflow() {
        let mut svc = AutomationService::new();
        let wf = svc.create_workflow("Build", "CI", TriggerType::ManualTrigger);

        let step = svc
            .add_step(
                &wf.id,
                "Compile",
                ActionType::RunCommand {
                    command: "make build".into(),
                },
            )
            .unwrap();

        assert_eq!(step.name, "Compile");
        assert!(step.conditions.is_empty());
        assert_eq!(step.retry_count, 0);
        assert!(step.timeout_secs.is_none());

        let updated = svc.get_workflow(&wf.id).unwrap();
        assert_eq!(updated.steps.len(), 1);
    }

    #[test]
    fn add_step_with_conditions() {
        let mut svc = AutomationService::new();
        let wf = svc.create_workflow("Conditional", "Test", TriggerType::ManualTrigger);

        let conditions = vec![Condition {
            field: "branch".into(),
            operator: ConditionOp::Equals,
            value: "main".into(),
        }];

        let step = svc
            .add_step_with_conditions(
                &wf.id,
                "Deploy to prod",
                ActionType::RunCommand {
                    command: "deploy.sh".into(),
                },
                conditions,
            )
            .unwrap();

        assert_eq!(step.conditions.len(), 1);
        assert_eq!(step.conditions[0].field, "branch");
    }

    #[test]
    fn add_step_to_nonexistent_workflow_fails() {
        let mut svc = AutomationService::new();
        let result = svc.add_step(
            "no-such-id",
            "Step",
            ActionType::RunCommand {
                command: "echo".into(),
            },
        );
        assert!(result.is_err());
    }

    // -- lifecycle ----------------------------------------------------------

    #[test]
    fn activate_and_pause_workflow() {
        let mut svc = AutomationService::new();
        let wf = svc.create_workflow("Lifecycle", "Test", TriggerType::ManualTrigger);

        assert_eq!(wf.status, WorkflowStatus::Draft);

        svc.activate_workflow(&wf.id).unwrap();
        assert_eq!(
            svc.get_workflow(&wf.id).unwrap().status,
            WorkflowStatus::Active
        );

        svc.pause_workflow(&wf.id).unwrap();
        assert_eq!(
            svc.get_workflow(&wf.id).unwrap().status,
            WorkflowStatus::Paused
        );
    }

    #[test]
    fn activate_nonexistent_workflow_fails() {
        let mut svc = AutomationService::new();
        assert!(svc.activate_workflow("ghost").is_err());
    }

    #[test]
    fn pause_nonexistent_workflow_fails() {
        let mut svc = AutomationService::new();
        assert!(svc.pause_workflow("ghost").is_err());
    }

    // -- delete -------------------------------------------------------------

    #[test]
    fn delete_workflow_removes_it() {
        let mut svc = AutomationService::new();
        let wf = svc.create_workflow("Ephemeral", "Will be deleted", TriggerType::ManualTrigger);

        assert_eq!(svc.workflow_count(), 1);
        svc.delete_workflow(&wf.id).unwrap();
        assert_eq!(svc.workflow_count(), 0);
    }

    #[test]
    fn delete_nonexistent_workflow_fails() {
        let mut svc = AutomationService::new();
        assert!(svc.delete_workflow("no-such-id").is_err());
    }

    // -- lookup -------------------------------------------------------------

    #[test]
    fn get_workflow_returns_none_for_unknown() {
        let svc = AutomationService::new();
        assert!(svc.get_workflow("unknown").is_none());
    }

    #[test]
    fn list_active_workflows_filters_correctly() {
        let mut svc = AutomationService::new();
        let wf1 = svc.create_workflow("Active1", "a", TriggerType::ManualTrigger);
        let wf2 = svc.create_workflow("Active2", "b", TriggerType::ManualTrigger);
        let _wf3 = svc.create_workflow("Draft", "c", TriggerType::ManualTrigger);

        svc.activate_workflow(&wf1.id).unwrap();
        svc.activate_workflow(&wf2.id).unwrap();

        assert_eq!(svc.active_count(), 2);
        assert_eq!(svc.list_active_workflows().len(), 2);
    }

    // -- simulate run -------------------------------------------------------

    #[test]
    fn simulate_run_succeeds_for_active_workflow() {
        let (mut svc, id) = make_service_with_active_workflow();

        let result = svc.simulate_run(&id).unwrap();
        assert!(result.success);
        assert_eq!(result.steps_completed, 1);
        assert!(result.error.is_none());

        let wf = svc.get_workflow(&id).unwrap();
        assert_eq!(wf.run_count, 1);
        assert!(wf.last_run.is_some());
    }

    #[test]
    fn simulate_run_fails_for_draft_workflow() {
        let mut svc = AutomationService::new();
        let wf = svc.create_workflow("Draft", "Not active", TriggerType::ManualTrigger);

        let result = svc.simulate_run(&wf.id);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Active"));
    }

    #[test]
    fn simulate_run_nonexistent_fails() {
        let mut svc = AutomationService::new();
        assert!(svc.simulate_run("no-such-id").is_err());
    }

    // -- record run ---------------------------------------------------------

    #[test]
    fn record_successful_run() {
        let (mut svc, id) = make_service_with_active_workflow();

        let result = svc.record_run(&id, true, 1, None).unwrap();
        assert!(result.success);
        assert_eq!(result.steps_completed, 1);
        assert!(result.error.is_none());

        let wf = svc.get_workflow(&id).unwrap();
        assert_eq!(wf.run_count, 1);
        assert_eq!(wf.status, WorkflowStatus::Active);
    }

    #[test]
    fn record_failed_run_sets_status() {
        let (mut svc, id) = make_service_with_active_workflow();

        let result = svc
            .record_run(&id, false, 0, Some("Timeout".into()))
            .unwrap();
        assert!(!result.success);
        assert_eq!(result.error.as_deref(), Some("Timeout"));

        let wf = svc.get_workflow(&id).unwrap();
        assert_eq!(wf.status, WorkflowStatus::Failed);
    }

    // -- run history --------------------------------------------------------

    #[test]
    fn get_run_history_returns_limited_results() {
        let (mut svc, id) = make_service_with_active_workflow();

        for _ in 0..5 {
            svc.simulate_run(&id).unwrap();
        }

        let history = svc.get_run_history(&id, 3);
        assert_eq!(history.len(), 3);

        let all = svc.get_run_history(&id, 100);
        assert_eq!(all.len(), 5);
    }

    #[test]
    fn get_run_history_empty_for_unknown_workflow() {
        let svc = AutomationService::new();
        let history = svc.get_run_history("unknown", 10);
        assert!(history.is_empty());
    }

    // -- conditions ---------------------------------------------------------

    #[test]
    fn check_condition_equals() {
        let cond = Condition {
            field: "status".into(),
            operator: ConditionOp::Equals,
            value: "ready".into(),
        };
        assert!(AutomationService::check_condition(&cond, "ready"));
        assert!(!AutomationService::check_condition(&cond, "pending"));
    }

    #[test]
    fn check_condition_not_equals() {
        let cond = Condition {
            field: "env".into(),
            operator: ConditionOp::NotEquals,
            value: "production".into(),
        };
        assert!(AutomationService::check_condition(&cond, "staging"));
        assert!(!AutomationService::check_condition(&cond, "production"));
    }

    #[test]
    fn check_condition_contains() {
        let cond = Condition {
            field: "message".into(),
            operator: ConditionOp::Contains,
            value: "error".into(),
        };
        assert!(AutomationService::check_condition(
            &cond,
            "An error occurred"
        ));
        assert!(!AutomationService::check_condition(&cond, "All good"));
    }

    #[test]
    fn check_condition_greater_than() {
        let cond = Condition {
            field: "score".into(),
            operator: ConditionOp::GreaterThan,
            value: "50".into(),
        };
        assert!(AutomationService::check_condition(&cond, "75"));
        assert!(!AutomationService::check_condition(&cond, "25"));
        assert!(!AutomationService::check_condition(&cond, "50"));
    }

    #[test]
    fn check_condition_less_than() {
        let cond = Condition {
            field: "latency".into(),
            operator: ConditionOp::LessThan,
            value: "100".into(),
        };
        assert!(AutomationService::check_condition(&cond, "42"));
        assert!(!AutomationService::check_condition(&cond, "200"));
        assert!(!AutomationService::check_condition(&cond, "100"));
    }

    #[test]
    fn check_condition_matches_regex() {
        let cond = Condition {
            field: "version".into(),
            operator: ConditionOp::Matches,
            value: r"^v\d+\.\d+\.\d+$".into(),
        };
        assert!(AutomationService::check_condition(&cond, "v1.2.3"));
        assert!(!AutomationService::check_condition(&cond, "1.2.3"));
        assert!(!AutomationService::check_condition(&cond, "v1.2"));
    }

    #[test]
    fn check_condition_matches_invalid_regex_returns_false() {
        let cond = Condition {
            field: "x".into(),
            operator: ConditionOp::Matches,
            value: r"[invalid".into(),
        };
        assert!(!AutomationService::check_condition(&cond, "anything"));
    }

    #[test]
    fn check_condition_greater_than_non_numeric_returns_false() {
        let cond = Condition {
            field: "x".into(),
            operator: ConditionOp::GreaterThan,
            value: "50".into(),
        };
        assert!(!AutomationService::check_condition(&cond, "not-a-number"));
    }

    // -- serde round trip ---------------------------------------------------

    #[test]
    fn workflow_serde_round_trip() {
        let mut svc = AutomationService::new();
        let wf = svc.create_workflow(
            "Serde Test",
            "Testing serialization",
            TriggerType::FileChange {
                path: "/src".into(),
            },
        );
        svc.add_step_with_conditions(
            &wf.id,
            "Notify",
            ActionType::SendNotification {
                title: "Changed".into(),
                body: "File changed".into(),
            },
            vec![Condition {
                field: "ext".into(),
                operator: ConditionOp::Equals,
                value: ".rs".into(),
            }],
        )
        .unwrap();

        let workflow = svc.get_workflow(&wf.id).unwrap();
        let json = serde_json::to_string_pretty(workflow).unwrap();
        let parsed: Workflow = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.name, "Serde Test");
        assert_eq!(parsed.steps.len(), 1);
        assert_eq!(parsed.steps[0].conditions.len(), 1);
    }

    #[test]
    fn run_result_serde_round_trip() {
        let result = WorkflowRunResult {
            workflow_id: "test-wf".into(),
            started_at: Utc::now(),
            completed_at: Utc::now(),
            success: true,
            steps_completed: 3,
            error: None,
        };
        let json = serde_json::to_string(&result).unwrap();
        let parsed: WorkflowRunResult = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.workflow_id, "test-wf");
        assert!(parsed.success);
        assert_eq!(parsed.steps_completed, 3);
    }

    // -- counts -------------------------------------------------------------

    #[test]
    fn workflow_count_and_active_count() {
        let mut svc = AutomationService::new();
        assert_eq!(svc.workflow_count(), 0);
        assert_eq!(svc.active_count(), 0);

        let wf1 = svc.create_workflow("A", "", TriggerType::ManualTrigger);
        let wf2 = svc.create_workflow("B", "", TriggerType::ManualTrigger);
        svc.create_workflow("C", "", TriggerType::ManualTrigger);

        svc.activate_workflow(&wf1.id).unwrap();
        svc.activate_workflow(&wf2.id).unwrap();

        assert_eq!(svc.workflow_count(), 3);
        assert_eq!(svc.active_count(), 2);
    }

    // -- trigger types ------------------------------------------------------

    #[test]
    fn all_trigger_types_serialize() {
        let triggers = vec![
            TriggerType::Schedule {
                cron: "* * * * *".into(),
            },
            TriggerType::FileChange {
                path: "/tmp".into(),
            },
            TriggerType::WebhookReceived {
                event: "push".into(),
            },
            TriggerType::ManualTrigger,
            TriggerType::OnMessage {
                pattern: "deploy".into(),
            },
            TriggerType::OnError {
                source: "build".into(),
            },
        ];

        for trigger in &triggers {
            let json = serde_json::to_string(trigger).unwrap();
            assert!(!json.is_empty());
        }
    }

    // -- action types -------------------------------------------------------

    #[test]
    fn all_action_types_serialize() {
        let actions = vec![
            ActionType::RunCommand {
                command: "ls".into(),
            },
            ActionType::SendMessage {
                channel: "#general".into(),
                content: "Hello".into(),
            },
            ActionType::CallApi {
                url: "https://api.example.com".into(),
                method: "POST".into(),
            },
            ActionType::CreateTask {
                title: "Fix bug".into(),
            },
            ActionType::SendNotification {
                title: "Alert".into(),
                body: "Something happened".into(),
            },
            ActionType::ExecuteSkill {
                skill_trigger: "/test".into(),
                input: "run all".into(),
            },
        ];

        for action in &actions {
            let json = serde_json::to_string(action).unwrap();
            assert!(!json.is_empty());
        }
    }

    // -- default impl -------------------------------------------------------

    #[test]
    fn default_creates_empty_service() {
        let svc = AutomationService::default();
        assert_eq!(svc.workflow_count(), 0);
        assert_eq!(svc.active_count(), 0);
    }

    #[test]
    fn ensure_builtin_workflows_is_idempotent() {
        let mut svc = AutomationService::new();
        svc.ensure_builtin_workflows();
        svc.ensure_builtin_workflows();

        let builtins = svc
            .list_workflows()
            .iter()
            .filter(|wf| wf.id == BUILTIN_DOGFOOD_WORKFLOW_ID)
            .count();
        assert_eq!(builtins, 1);
    }

    #[test]
    fn reload_user_workflows_loads_json_templates() {
        let tmp = tempfile::tempdir().unwrap();
        let workflows_dir = tmp.path().join(USER_WORKFLOW_DIR);
        std::fs::create_dir_all(&workflows_dir).unwrap();

        let json = r#"{
  "name": "Local QA",
  "description": "Run smoke checks",
  "enabled": true,
  "steps": [
    {
      "name": "Check",
      "action": { "type": "run_command", "command": "cargo check --quiet" }
    }
  ]
}"#;
        std::fs::write(workflows_dir.join("qa.json"), json).unwrap();

        let mut svc = AutomationService::new();
        svc.ensure_builtin_workflows();
        let report = svc.reload_user_workflows(tmp.path());

        assert_eq!(report.loaded, 1);
        assert_eq!(report.failed, 0);
        assert!(
            svc.list_workflows().iter().any(|wf| wf.id == "file:qa"),
            "expected file-based workflow id"
        );
    }

    #[test]
    fn reload_user_workflows_loads_call_api_template() {
        let tmp = tempfile::tempdir().unwrap();
        let workflows_dir = tmp.path().join(USER_WORKFLOW_DIR);
        std::fs::create_dir_all(&workflows_dir).unwrap();

        let json = r#"{
  "name": "API Workflow",
  "steps": [
    {
      "name": "Call local service",
      "action": { "type": "call_api", "url": "http://127.0.0.1/api", "method": "POST" }
    }
  ]
}"#;
        std::fs::write(workflows_dir.join("api.json"), json).unwrap();

        let mut svc = AutomationService::new();
        let report = svc.reload_user_workflows(tmp.path());
        assert_eq!(report.loaded, 1);
        assert_eq!(report.failed, 0);
        assert!(
            svc.list_workflows().iter().any(|wf| wf.id == "file:api"),
            "expected file-based workflow id"
        );
    }

    #[test]
    fn reload_user_workflows_rejects_invalid_action_fields() {
        let tmp = tempfile::tempdir().unwrap();
        let workflows_dir = tmp.path().join(USER_WORKFLOW_DIR);
        std::fs::create_dir_all(&workflows_dir).unwrap();

        let json = r#"{
  "name": "Bad Method",
  "steps": [
    {
      "name": "Call with invalid method",
      "action": { "type": "call_api", "url": "http://example.com/api", "method": "INVALID" }
    }
  ]
}"#;
        std::fs::write(workflows_dir.join("bad.json"), json).unwrap();

        let mut svc = AutomationService::new();
        let report = svc.reload_user_workflows(tmp.path());
        assert_eq!(report.loaded, 0);
        assert_eq!(report.failed, 1);
        assert!(
            report.errors[0].contains("unsupported HTTP method"),
            "unexpected error: {}",
            report.errors[0]
        );
    }

    // -- file persistence ---------------------------------------------------

    #[test]
    fn save_and_load_file_round_trip() {
        let dir = std::env::temp_dir().join("hive-automation-test");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("automation.json");

        let (mut svc, id) = make_service_with_active_workflow();
        svc.simulate_run(&id).unwrap();

        svc.save_to_file(&path).unwrap();
        let loaded = AutomationService::load_from_file(&path).unwrap();

        assert_eq!(loaded.workflow_count(), 1);
        let wf = loaded.get_workflow(&id).unwrap();
        assert_eq!(wf.name, "Deploy Pipeline");
        assert_eq!(wf.status, WorkflowStatus::Active);
        assert_eq!(wf.run_count, 1);
        assert!(wf.last_run.is_some());
        assert_eq!(wf.steps.len(), 1);

        let history = loaded.list_run_history();
        assert_eq!(history.len(), 1);
        assert!(history[0].success);

        // Clean up
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn load_missing_file_returns_empty_service() {
        let path = std::env::temp_dir().join("nonexistent-hive-automation.json");
        let svc = AutomationService::load_from_file(&path).unwrap();
        assert_eq!(svc.workflow_count(), 0);
    }
}
