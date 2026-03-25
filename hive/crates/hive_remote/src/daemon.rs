use crate::protocol::{
    AgentRunSummary, DaemonEvent, ObserveView, PanelPayload, PanelResponse, SessionSnapshot,
    ShellDestination, WorkspaceSummary,
};
use crate::session::SessionJournal;
use anyhow::{Result, anyhow};
use chrono::Utc;
use hive_agents::{
    automation::{AutomationService, TriggerType, Workflow, WorkflowRunResult},
    skill_format::SkillLoader,
    skills::{SkillSource, SkillsRegistry},
    specs::{Spec, SpecSection},
    ActivityEntry, ActivityEvent, ActivityFilter, ActivityLog, ApprovalDecision, ApprovalGate,
    ApprovalRequest, ApprovalRule, OperationType, RuleTrigger,
};
use hive_assistant::AssistantService;
use hive_ai::service::{AiService, AiServiceConfig};
use hive_core::channels::{ChannelMessage, ChannelStore, MessageAuthor};
use hive_core::config::{AccountPlatform, ConfigManager, HiveConfig};
use hive_core::conversations::{Conversation, ConversationStore, StoredMessage, generate_title};
use hive_fs::{FileService, FileStatusType, GitService};
use hive_network::HiveNodeHandle;
use hive_terminal::{InteractiveShell, ShellOutput};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::{Mutex, broadcast};

pub const PANEL_HOME: &str = "home";
pub const PANEL_CHAT: &str = "chat";
pub const PANEL_HISTORY: &str = "history";
pub const PANEL_FILES: &str = "files";
pub const PANEL_CODE_MAP: &str = "code_map";
pub const PANEL_PROMPTS: &str = "prompts";
pub const PANEL_SPECS: &str = "specs";
pub const PANEL_AGENTS: &str = "agents";
pub const PANEL_GIT_OPS: &str = "git_ops";
pub const PANEL_TERMINAL: &str = "terminal";
pub const PANEL_WORKFLOWS: &str = "workflows";
pub const PANEL_CHANNELS: &str = "channels";
pub const PANEL_NETWORK: &str = "network";
pub const PANEL_ASSISTANT: &str = "assistant";
pub const PANEL_OBSERVE: &str = "observe";
pub const PANEL_MONITOR: &str = "monitor";
pub const PANEL_LOGS: &str = "logs";
pub const PANEL_COSTS: &str = "costs";
pub const PANEL_LEARNING: &str = "learning";
pub const PANEL_SHIELD: &str = "shield";
pub const PANEL_SETTINGS: &str = "settings";
pub const PANEL_MODELS: &str = "models";
pub const PANEL_ROUTING: &str = "routing";
pub const PANEL_SKILLS: &str = "skills";
pub const PANEL_LAUNCH: &str = "launch";
pub const PANEL_HELP: &str = "help";

#[derive(Debug, Clone)]
pub struct DaemonConfig {
    pub data_dir: PathBuf,
    pub config_root: Option<PathBuf>,
    pub local_port: u16,
    pub web_port: u16,
    pub shutdown_grace_secs: u64,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        let data_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".hive");
        Self {
            data_dir,
            config_root: None,
            local_port: 9480,
            web_port: 9481,
            shutdown_grace_secs: 30,
        }
    }
}

#[derive(Debug, Clone)]
pub enum PendingAction {
    Chat {
        conversation_id: String,
        content: String,
        model: String,
    },
    Agent {
        run_id: String,
        goal: String,
        orchestration_mode: String,
    },
}

#[derive(Debug, Clone)]
pub enum SendDisposition {
    Stream {
        conversation_id: String,
        model: String,
    },
    ApprovalPending {
        request_id: String,
    },
}

#[derive(Debug, Clone)]
pub enum AgentDisposition {
    Run {
        run_id: String,
        goal: String,
        orchestration_mode: String,
    },
    ApprovalPending {
        request_id: String,
        run_id: String,
    },
}

#[derive(Debug, Clone)]
pub struct WorkflowRunState {
    pub run_id: String,
    pub workflow_id: String,
    pub workflow_name: String,
    pub status: String,
    pub started_at: chrono::DateTime<Utc>,
    pub completed_at: Option<chrono::DateTime<Utc>>,
    pub steps_completed: usize,
    pub error: Option<String>,
}

pub struct HiveDaemon {
    config: DaemonConfig,
    journal: SessionJournal,
    active_destination: ShellDestination,
    active_panel: String,
    active_conversation: Option<String>,
    current_model: String,
    current_workspace: WorkspaceSummary,
    workspaces: Vec<WorkspaceSummary>,
    observe_view: ObserveView,
    is_streaming: bool,
    last_launch_status: Option<String>,
    shield_enabled: bool,
    agent_runs: Vec<AgentRunSummary>,
    files_current_path: PathBuf,
    selected_file: Option<PathBuf>,
    selected_spec: Option<PathBuf>,
    terminal_shell: Option<Arc<Mutex<InteractiveShell>>>,
    terminal_lines: Vec<crate::protocol::TerminalLineData>,
    terminal_last_exit_code: Option<i32>,
    terminal_reader_active: bool,
    automation_service: AutomationService,
    workflow_runs: Vec<WorkflowRunState>,
    channel_store: ChannelStore,
    selected_channel_id: Option<String>,
    assistant_service: Option<AssistantService>,
    connected_account_count: usize,
    network_handle: Option<Arc<HiveNodeHandle>>,
    pending_actions: HashMap<String, PendingAction>,
    conversation_store: ConversationStore,
    activity_log: Arc<ActivityLog>,
    approval_gate: Arc<ApprovalGate>,
    ai_service: Arc<Mutex<AiService>>,
    event_tx: broadcast::Sender<DaemonEvent>,
    cortex_event_tx: Option<hive_learn::cortex::event_bus::CortexEventSender>,
    interaction_tracker: Option<std::sync::Arc<std::sync::atomic::AtomicI64>>,
}

impl HiveDaemon {
    pub fn new(config: DaemonConfig) -> Result<Self> {
        Self::new_with_network(config, None)
    }

    pub fn new_with_network(
        config: DaemonConfig,
        network_handle: Option<Arc<HiveNodeHandle>>,
    ) -> Result<Self> {
        std::fs::create_dir_all(&config.data_dir)?;
        let journal = SessionJournal::new(&config.data_dir.join("session_journal.jsonl"))?;
        let (event_tx, _) = broadcast::channel(256);

        let current_dir = std::env::current_dir().unwrap_or_else(|_| config.data_dir.clone());
        let current_workspace = workspace_from_path(&current_dir, true, true);
        let config_manager = config_manager_for_daemon(&config).ok();
        let config_snapshot = config_manager.as_ref().map(|manager| manager.get().clone());
        let current_model = config_snapshot
            .as_ref()
            .map(|cfg| cfg.default_model.clone())
            .filter(|model| !model.trim().is_empty())
            .unwrap_or_else(|| "auto".into());

        let conversation_store = ConversationStore::new_at(config.data_dir.join("conversations"))?;
        let activity_log = Arc::new(ActivityLog::open(&config.data_dir.join("activity.db"))?);
        let mut automation_service = AutomationService::new();
        let _ = automation_service.initialize_workflows(&current_dir);
        let mut channel_store = ChannelStore::new();
        channel_store.ensure_default_channels();
        let selected_channel_id = channel_store
            .list_channels()
            .first()
            .map(|channel| channel.id.clone());
        let connected_account_count = config_snapshot
            .as_ref()
            .map(|cfg| cfg.connected_accounts.len())
            .unwrap_or(0);
        let assistant_service =
            build_assistant_service(config_manager.as_ref(), connected_account_count).ok().flatten();

        let mut daemon = Self {
            config,
            journal,
            active_destination: ShellDestination::Home,
            active_panel: PANEL_HOME.into(),
            active_conversation: latest_conversation_id(&conversation_store),
            current_model: current_model.clone(),
            current_workspace: current_workspace.clone(),
            workspaces: vec![current_workspace],
            observe_view: ObserveView::Inbox,
            is_streaming: false,
            last_launch_status: None,
            shield_enabled: config_snapshot
                .as_ref()
                .map(|cfg| cfg.shield_enabled)
                .unwrap_or(false),
            agent_runs: Vec::new(),
            files_current_path: current_dir,
            selected_file: None,
            selected_spec: None,
            terminal_shell: None,
            terminal_lines: Vec::new(),
            terminal_last_exit_code: None,
            terminal_reader_active: false,
            automation_service,
            workflow_runs: Vec::new(),
            channel_store,
            selected_channel_id,
            assistant_service,
            connected_account_count,
            network_handle,
            pending_actions: HashMap::new(),
            conversation_store,
            activity_log,
            approval_gate: Arc::new(ApprovalGate::new(default_remote_approval_rules())),
            ai_service: Arc::new(Mutex::new(AiService::new(ai_service_config(
                config_snapshot.as_ref(),
                &current_model,
            )))),
            event_tx,
            cortex_event_tx: None,
            interaction_tracker: None,
        };

        daemon.replay_journal()?;
        Ok(daemon)
    }

    pub fn get_snapshot(&self) -> SessionSnapshot {
        SessionSnapshot {
            active_conversation: self.active_conversation.clone(),
            active_destination: self.active_destination,
            active_panel: self.active_panel.clone(),
            current_workspace: self.current_workspace.clone(),
            current_model: self.current_model.clone(),
            pending_approval_count: self.pending_actions.len(),
            is_streaming: self.is_streaming,
            observe_view: self.observe_view,
            panel_registry: panel_registry(),
            agent_runs: self.agent_runs.clone(),
            timestamp: Utc::now(),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<DaemonEvent> {
        self.event_tx.subscribe()
    }

    pub fn broadcast_event(&self, event: DaemonEvent) {
        let _ = self.event_tx.send(event);
    }

    pub fn set_cortex_event_tx(&mut self, tx: hive_learn::cortex::event_bus::CortexEventSender) {
        self.cortex_event_tx = Some(tx);
    }

    pub fn set_interaction_tracker(&mut self, tracker: std::sync::Arc<std::sync::atomic::AtomicI64>) {
        self.interaction_tracker = Some(tracker);
    }

    pub fn config(&self) -> &DaemonConfig {
        &self.config
    }

    pub fn current_workspace_root(&self) -> PathBuf {
        self.workspace_root()
    }

    pub fn ai_service(&self) -> Arc<Mutex<AiService>> {
        Arc::clone(&self.ai_service)
    }

    pub fn begin_send_message(
        &mut self,
        conversation_id: String,
        content: String,
        model: String,
    ) -> Result<SendDisposition> {
        let effective_model = normalize_model(&model, &self.current_model);
        self.active_destination = ShellDestination::Build;
        self.active_panel = PANEL_CHAT.into();
        self.active_conversation = Some(conversation_id.clone());
        self.current_model = effective_model.clone();

        self.append_user_message(&conversation_id, &content, &effective_model)?;
        self.append_journal(&DaemonEvent::SendMessage {
            conversation_id: conversation_id.clone(),
            content: content.clone(),
            model: effective_model.clone(),
        });

        if let Some(operation) = classify_chat_operation(&content)
            && let Some(request) = self.approval_gate.check_sync("remote-chat", &operation)
        {
            self.pending_actions.insert(
                request.id.clone(),
                PendingAction::Chat {
                    conversation_id: conversation_id.clone(),
                    content,
                    model: effective_model,
                },
            );
            self.last_launch_status =
                Some("Awaiting approval before Hive can execute this request remotely.".into());
            self.record_activity(ActivityEvent::ApprovalRequested {
                request_id: request.id.clone(),
                agent_id: "remote-chat".into(),
                operation: request.operation_string(),
                context: request.context.clone(),
                rule: request.matched_rule.clone(),
            });
            self.broadcast_state_and_panels();
            return Ok(SendDisposition::ApprovalPending {
                request_id: request.id,
            });
        }

        self.is_streaming = true;
        self.broadcast_state_and_panels();
        Ok(SendDisposition::Stream {
            conversation_id,
            model: effective_model,
        })
    }

    pub fn launch_home_mission(
        &mut self,
        template_id: String,
        detail: String,
    ) -> Result<SendDisposition> {
        self.last_launch_status = Some(format!("Started '{}' from Home.", home_template_title(&template_id)));
        let prompt = build_home_prompt(&self.current_workspace, &template_id, &detail);
        self.begin_send_message(uuid::Uuid::new_v4().to_string(), prompt, self.current_model.clone())
    }

    pub fn resume_conversation(&mut self, conversation_id: &str) -> Result<()> {
        let conversation = self.conversation_store.load(conversation_id)?;
        self.active_conversation = Some(conversation.id);
        self.active_destination = ShellDestination::Build;
        self.active_panel = PANEL_CHAT.into();
        if !conversation.model.trim().is_empty() {
            self.current_model = conversation.model;
        }
        self.broadcast_state_and_panels();
        Ok(())
    }

    pub fn switch_panel(&mut self, panel: &str) {
        let normalized = canonical_panel_id(panel);
        self.active_panel = normalized.clone();
        if let Some(destination) = panel_destination(&normalized) {
            self.active_destination = destination;
        }
        self.append_journal(&DaemonEvent::SwitchPanel { panel: normalized });
        self.broadcast_state_and_panels();
    }

    pub fn switch_destination(&mut self, destination: ShellDestination) {
        self.active_destination = destination;
        if panel_destination(&self.active_panel) != Some(destination) {
            self.active_panel = default_panel_for_destination(destination).into();
        }
        self.append_journal(&DaemonEvent::SwitchDestination { destination });
        self.broadcast_state_and_panels();
    }

    pub fn set_model(&mut self, model: String) {
        self.current_model = normalize_model(&model, &self.current_model);
        self.append_journal(&DaemonEvent::SetModel {
            model: self.current_model.clone(),
        });
        self.broadcast_state_and_panels();
    }

    pub fn set_observe_view(&mut self, view: ObserveView) {
        self.observe_view = view;
        self.active_destination = ShellDestination::Observe;
        if panel_destination(&self.active_panel) != Some(ShellDestination::Observe) {
            self.active_panel = PANEL_OBSERVE.into();
        }
        self.append_journal(&DaemonEvent::SetObserveView { view });
        self.broadcast_state_and_panels();
    }

    pub fn switch_workspace(&mut self, workspace_path: String) {
        let path = PathBuf::from(workspace_path.clone());
        let next = workspace_from_path(&path, true, false);

        for workspace in &mut self.workspaces {
            workspace.is_current = workspace.path == next.path;
        }

        if let Some(existing) = self
            .workspaces
            .iter_mut()
            .find(|workspace| workspace.path == next.path)
        {
            existing.is_current = true;
            self.current_workspace = existing.clone();
        } else {
            let mut appended = next.clone();
            appended.is_current = true;
            self.workspaces.push(appended.clone());
            self.current_workspace = appended;
        }

        self.files_current_path = PathBuf::from(&self.current_workspace.path);
        self.selected_file = None;
        self.selected_spec = None;
        self.terminal_last_exit_code = None;
        self.automation_service = AutomationService::new();
        let _ = self
            .automation_service
            .initialize_workflows(&self.workspace_root());

        self.append_journal(&DaemonEvent::SwitchWorkspace { workspace_path });
        self.broadcast_state_and_panels();
    }

    pub fn navigate_files(&mut self, path: &str) -> Result<()> {
        let next = self.resolve_workspace_path(path, Some(&self.files_current_path))?;
        if !next.is_dir() {
            return Err(anyhow!("Path is not a directory: {}", next.display()));
        }
        self.active_destination = ShellDestination::Build;
        self.active_panel = PANEL_FILES.into();
        self.files_current_path = next;
        self.selected_file = None;
        self.broadcast_state_and_panels();
        Ok(())
    }

    pub fn open_file(&mut self, path: &str) -> Result<()> {
        let next = self.resolve_workspace_path(path, Some(&self.files_current_path))?;
        if next.is_dir() {
            self.files_current_path = next;
            self.selected_file = None;
        } else {
            self.selected_file = Some(next);
        }
        self.active_destination = ShellDestination::Build;
        self.active_panel = PANEL_FILES.into();
        self.broadcast_state_and_panels();
        Ok(())
    }

    pub fn select_spec(&mut self, path: &str) -> Result<()> {
        let next = self.resolve_workspace_path(path, Some(&self.workspace_root()))?;
        if !next.is_file() {
            return Err(anyhow!("Spec path is not a file: {}", next.display()));
        }
        self.active_destination = ShellDestination::Build;
        self.active_panel = PANEL_SPECS.into();
        self.selected_spec = Some(next);
        self.broadcast_state_and_panels();
        Ok(())
    }

    pub fn start_terminal(&mut self) -> Result<bool> {
        if self.terminal_shell.is_some() {
            self.active_destination = ShellDestination::Build;
            self.active_panel = PANEL_TERMINAL.into();
            self.broadcast_state_and_panels();
            return Ok(false);
        }

        let cwd = self.workspace_root();
        let shell = InteractiveShell::new(Some(&cwd))?;
        self.terminal_shell = Some(Arc::new(Mutex::new(shell)));
        self.terminal_last_exit_code = None;
        self.active_destination = ShellDestination::Build;
        self.active_panel = PANEL_TERMINAL.into();
        self.push_terminal_line("system", format!("Started shell in {}", cwd.display()));
        self.broadcast_state_and_panels();
        Ok(true)
    }

    pub fn terminal_shell(&self) -> Option<Arc<Mutex<InteractiveShell>>> {
        self.terminal_shell.as_ref().map(Arc::clone)
    }

    pub fn take_terminal_shell(&mut self) -> Option<Arc<Mutex<InteractiveShell>>> {
        self.terminal_reader_active = false;
        self.terminal_shell.take()
    }

    pub fn clear_terminal(&mut self) {
        self.terminal_lines.clear();
        self.terminal_last_exit_code = None;
        self.broadcast_state_and_panels();
    }

    pub fn ensure_terminal_reader(&mut self) -> bool {
        if self.terminal_reader_active {
            false
        } else {
            self.terminal_reader_active = true;
            true
        }
    }

    pub fn finish_terminal_reader(&mut self) {
        self.terminal_reader_active = false;
    }

    pub fn push_terminal_output(&mut self, output: ShellOutput) {
        match output {
            ShellOutput::Stdout(content) => self.push_terminal_line("stdout", content),
            ShellOutput::Stderr(content) => self.push_terminal_line("stderr", content),
            ShellOutput::Exit(code) => {
                self.terminal_last_exit_code = Some(code);
                self.push_terminal_line("system", format!("Shell exited with code {code}"));
                self.terminal_shell = None;
                self.terminal_reader_active = false;
            }
        }
    }

    pub fn start_workflow_run(&mut self, workflow_id: &str) -> Result<(String, Workflow, PathBuf)> {
        let workflow = self
            .automation_service
            .clone_workflow(workflow_id)
            .ok_or_else(|| anyhow!("Workflow '{}' not found", workflow_id))?;
        let run_id = uuid::Uuid::new_v4().to_string();
        self.workflow_runs.push(WorkflowRunState {
            run_id: run_id.clone(),
            workflow_id: workflow.id.clone(),
            workflow_name: workflow.name.clone(),
            status: "running".into(),
            started_at: Utc::now(),
            completed_at: None,
            steps_completed: 0,
            error: None,
        });
        self.active_destination = ShellDestination::Automate;
        self.active_panel = PANEL_WORKFLOWS.into();
        self.broadcast_state_and_panels();
        Ok((run_id, workflow, self.workspace_root()))
    }

    pub fn finish_workflow_run(
        &mut self,
        run_id: &str,
        result: Result<WorkflowRunResult, String>,
    ) -> Result<()> {
        if let Some(run) = self.workflow_runs.iter_mut().find(|run| run.run_id == run_id) {
            run.completed_at = Some(Utc::now());
            match result {
                Ok(result) => {
                    run.status = if result.success {
                        "completed".into()
                    } else {
                        "failed".into()
                    };
                    run.steps_completed = result.steps_completed;
                    run.error = result.error.clone();
                    let _ = self.automation_service.record_run(
                        &result.workflow_id,
                        result.success,
                        result.steps_completed,
                        result.error.clone(),
                    );
                }
                Err(error) => {
                    run.status = "failed".into();
                    run.error = Some(error.clone());
                    let _ = self.automation_service.record_run(
                        &run.workflow_id,
                        false,
                        run.steps_completed,
                        Some(error),
                    );
                }
            }
        }
        self.broadcast_state_and_panels();
        Ok(())
    }

    pub fn select_channel(&mut self, channel_id: &str) -> Result<()> {
        if self.channel_store.get_channel(channel_id).is_none() {
            return Err(anyhow!("Channel '{}' not found", channel_id));
        }
        self.selected_channel_id = Some(channel_id.to_string());
        self.active_destination = ShellDestination::Automate;
        self.active_panel = PANEL_CHANNELS.into();
        self.broadcast_state_and_panels();
        Ok(())
    }

    pub fn send_channel_message(&mut self, channel_id: &str, content: &str) -> Result<()> {
        if self.channel_store.get_channel(channel_id).is_none() {
            return Err(anyhow!("Channel '{}' not found", channel_id));
        }
        self.channel_store.add_message(
            channel_id,
            ChannelMessage {
                id: uuid::Uuid::new_v4().to_string(),
                author: MessageAuthor::User,
                content: content.to_string(),
                timestamp: Utc::now(),
                thread_id: None,
                model: None,
                cost: None,
            },
        );
        self.selected_channel_id = Some(channel_id.to_string());
        self.active_destination = ShellDestination::Automate;
        self.active_panel = PANEL_CHANNELS.into();
        self.broadcast_state_and_panels();
        Ok(())
    }

    pub fn decide_assistant_approval(&mut self, approval_id: &str, approved: bool) -> Result<()> {
        let assistant = self
            .assistant_service
            .as_ref()
            .ok_or_else(|| anyhow!("Assistant service is unavailable"))?;
        if approved {
            assistant
                .approval_service
                .approve(approval_id, "remote-user")
                .map_err(anyhow::Error::msg)?;
        } else {
            assistant
                .approval_service
                .reject(approval_id, "remote-user")
                .map_err(anyhow::Error::msg)?;
        }
        self.active_destination = ShellDestination::Assist;
        self.active_panel = PANEL_ASSISTANT.into();
        self.broadcast_state_and_panels();
        Ok(())
    }

    pub fn update_setting(&mut self, setting: &str, value: bool) -> Result<()> {
        match setting {
            "privacy_mode"
            | "shield_enabled"
            | "notifications_enabled"
            | "auto_update"
            | "remote_enabled"
            | "remote_auto_start" => {}
            _ => return Err(anyhow!("Unknown setting '{}'", setting)),
        }

        let manager = self.config_manager()?;
        manager.update(|config| match setting {
            "privacy_mode" => config.privacy_mode = value,
            "shield_enabled" => config.shield_enabled = value,
            "notifications_enabled" => config.notifications_enabled = value,
            "auto_update" => config.auto_update = value,
            "remote_enabled" => config.remote_enabled = value,
            "remote_auto_start" => config.remote_auto_start = value,
            _ => unreachable!(),
        })?;

        let config = manager.get();
        self.apply_config_snapshot(&config);
        self.active_panel = PANEL_SETTINGS.into();
        self.broadcast_state_and_panels();
        Ok(())
    }

    pub fn update_text_setting(&mut self, setting: &str, value: String) -> Result<()> {
        let trimmed = value.trim().to_string();
        let optional = if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.clone())
        };

        let manager = self.config_manager()?;
        match setting {
            "theme" => {
                if trimmed.is_empty() {
                    return Err(anyhow!("Theme cannot be empty"));
                }
                manager.update(|config| config.theme = trimmed.clone())?;
                self.active_panel = PANEL_SETTINGS.into();
            }
            "ollama_url" => {
                if trimmed.is_empty() {
                    return Err(anyhow!("Ollama URL cannot be empty"));
                }
                manager.update(|config| config.ollama_url = trimmed.clone())?;
                self.active_panel = PANEL_SETTINGS.into();
            }
            "lmstudio_url" => {
                if trimmed.is_empty() {
                    return Err(anyhow!("LM Studio URL cannot be empty"));
                }
                manager.update(|config| config.lmstudio_url = trimmed.clone())?;
                self.active_panel = PANEL_SETTINGS.into();
            }
            "litellm_url" => {
                manager.update(|config| config.litellm_url = optional.clone())?;
                self.active_panel = PANEL_SETTINGS.into();
            }
            "local_provider_url" => {
                manager.update(|config| config.local_provider_url = optional.clone())?;
                self.active_panel = PANEL_SETTINGS.into();
            }
            "cloud_api_url" => {
                manager.update(|config| config.cloud_api_url = optional.clone())?;
                self.active_panel = PANEL_LAUNCH.into();
            }
            "cloud_relay_url" => {
                manager.update(|config| config.cloud_relay_url = optional.clone())?;
                self.active_panel = PANEL_LAUNCH.into();
            }
            "cloud_tier" => {
                manager.update(|config| config.cloud_tier = optional.clone())?;
                self.active_panel = PANEL_LAUNCH.into();
            }
            _ => return Err(anyhow!("Unknown text setting '{}'", setting)),
        }

        let config = manager.get();
        self.apply_config_snapshot(&config);
        self.broadcast_state_and_panels();
        Ok(())
    }

    pub fn set_default_model(&mut self, model: &str) -> Result<()> {
        let manager = self.config_manager()?;
        manager.update(|config| config.default_model = model.to_string())?;

        let config = manager.get();
        self.current_model = normalize_model(model, &self.current_model);
        self.apply_config_snapshot(&config);
        self.active_panel = PANEL_MODELS.into();
        self.broadcast_state_and_panels();
        Ok(())
    }

    pub fn set_provider_key(&mut self, provider: &str, key: String) -> Result<()> {
        let manager = self.config_manager()?;
        let trimmed = key.trim().to_string();
        let next_key = if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        };

        manager.set_api_key(provider, next_key)?;

        let config = manager.get();
        self.apply_config_snapshot(&config);
        self.active_panel = PANEL_MODELS.into();
        self.broadcast_state_and_panels();
        Ok(())
    }

    pub fn set_auto_routing(&mut self, enabled: bool) -> Result<()> {
        let manager = self.config_manager()?;
        manager.update(|config| config.auto_routing = enabled)?;

        let config = manager.get();
        self.apply_config_snapshot(&config);
        self.active_panel = PANEL_ROUTING.into();
        self.broadcast_state_and_panels();
        Ok(())
    }

    pub fn add_project_model(&mut self, model: String) -> Result<()> {
        let trimmed = model.trim().to_string();
        if trimmed.is_empty() {
            return Err(anyhow!("Project model cannot be empty"));
        }

        let manager = self.config_manager()?;
        manager.update(|config| {
            if !config.project_models.iter().any(|entry| entry == &trimmed) {
                config.project_models.push(trimmed.clone());
            }
        })?;

        let config = manager.get();
        self.apply_config_snapshot(&config);
        self.active_panel = PANEL_ROUTING.into();
        self.broadcast_state_and_panels();
        Ok(())
    }

    pub fn remove_project_model(&mut self, model: &str) -> Result<()> {
        let trimmed = model.trim().to_string();
        if trimmed.is_empty() {
            return Err(anyhow!("Project model cannot be empty"));
        }

        let manager = self.config_manager()?;
        manager.update(|config| {
            config.project_models.retain(|entry| entry != &trimmed);
        })?;

        let config = manager.get();
        self.apply_config_snapshot(&config);
        self.active_panel = PANEL_ROUTING.into();
        self.broadcast_state_and_panels();
        Ok(())
    }

    pub fn set_skill_enabled(&mut self, name: &str, enabled: bool) -> Result<()> {
        let skills_dir = self.skills_dir();
        let mut registry = SkillsRegistry::with_loader(SkillLoader::new(skills_dir));
        let current = registry
            .list()
            .into_iter()
            .find(|skill| skill.name == name)
            .map(|skill| skill.enabled)
            .ok_or_else(|| anyhow!("Skill '{}' not found", name))?;

        if current != enabled {
            registry
                .toggle(name)
                .ok_or_else(|| anyhow!("Failed to toggle skill '{}'", name))?;
        }

        self.active_panel = PANEL_SKILLS.into();
        self.broadcast_state_and_panels();
        Ok(())
    }

    pub fn install_skill(
        &mut self,
        name: String,
        description: String,
        instructions: String,
    ) -> Result<()> {
        let name = name.trim().to_string();
        let description = description.trim().to_string();
        let instructions = instructions.trim().to_string();

        if name.is_empty() {
            return Err(anyhow!("Skill name cannot be empty"));
        }
        if description.is_empty() {
            return Err(anyhow!("Skill description cannot be empty"));
        }
        if instructions.is_empty() {
            return Err(anyhow!("Skill instructions cannot be empty"));
        }

        let skills_dir = self.skills_dir();
        let mut registry = SkillsRegistry::with_loader(SkillLoader::new(skills_dir));
        registry.install(name, description, instructions, SkillSource::Custom)?;

        self.active_panel = PANEL_SKILLS.into();
        self.broadcast_state_and_panels();
        Ok(())
    }

    pub fn remove_skill(&mut self, name: &str) -> Result<()> {
        let skills_dir = self.skills_dir();
        let mut registry = SkillsRegistry::with_loader(SkillLoader::new(skills_dir));
        let source = registry
            .list()
            .into_iter()
            .find(|skill| skill.name == name)
            .map(|skill| skill.source)
            .ok_or_else(|| anyhow!("Skill '{}' not found", name))?;

        if source == SkillSource::BuiltIn {
            return Err(anyhow!("Built-in skills cannot be removed"));
        }

        if !registry.uninstall(name) {
            return Err(anyhow!("Failed to remove skill '{}'", name));
        }

        self.active_panel = PANEL_SKILLS.into();
        self.broadcast_state_and_panels();
        Ok(())
    }

    pub fn start_agent_task(
        &mut self,
        goal: String,
        orchestration_mode: String,
    ) -> Result<AgentDisposition> {
        let run_id = uuid::Uuid::new_v4().to_string();
        let mut run = AgentRunSummary {
            run_id: run_id.clone(),
            goal: goal.clone(),
            status: "planning".into(),
            detail: format!("Preparing {} run", orchestration_mode),
            cost_usd: 0.0,
            elapsed_ms: 0,
        };

        self.append_journal(&DaemonEvent::StartAgentTask {
            goal: goal.clone(),
            orchestration_mode: orchestration_mode.clone(),
        });

        if let Some(operation) = classify_agent_operation(&goal)
            && let Some(request) = self.approval_gate.check_sync("remote-agent", &operation)
        {
            run.status = "pending_approval".into();
            run.detail = "Waiting for approval before execution".into();
            self.agent_runs.push(run);
            self.pending_actions.insert(
                request.id.clone(),
                PendingAction::Agent {
                    run_id: run_id.clone(),
                    goal: goal.clone(),
                    orchestration_mode: orchestration_mode.clone(),
                },
            );
            self.record_activity(ActivityEvent::ApprovalRequested {
                request_id: request.id.clone(),
                agent_id: "remote-agent".into(),
                operation: request.operation_string(),
                context: request.context.clone(),
                rule: request.matched_rule.clone(),
            });
            self.broadcast_state_and_panels();
            return Ok(AgentDisposition::ApprovalPending {
                request_id: request.id,
                run_id,
            });
        }

        self.agent_runs.push(run);
        self.record_activity(ActivityEvent::AgentStarted {
            agent_id: run_id.clone(),
            role: orchestration_mode.clone(),
            task_id: Some(run_id.clone()),
        });
        self.broadcast_state_and_panels();
        Ok(AgentDisposition::Run {
            run_id,
            goal,
            orchestration_mode,
        })
    }

    pub fn cancel_agent_task(&mut self, run_id: &str) {
        if let Some(run) = self.agent_runs.iter_mut().find(|run| run.run_id == run_id) {
            run.status = "cancelled".into();
            run.detail = "Cancelled from remote".into();
        }
        self.append_journal(&DaemonEvent::CancelAgentTask {
            run_id: run_id.to_string(),
        });
        self.broadcast_state_and_panels();
    }

    pub fn git_stage_all(&mut self) -> Result<usize> {
        let git = GitService::open(&self.workspace_root())?;
        let statuses = git.status()?;
        let paths: Vec<PathBuf> = statuses.into_iter().map(|status| status.path).collect();
        let path_refs: Vec<&Path> = paths.iter().map(PathBuf::as_path).collect();
        if !path_refs.is_empty() {
            git.stage(&path_refs)?;
        }
        self.active_destination = ShellDestination::Build;
        self.active_panel = PANEL_GIT_OPS.into();
        self.broadcast_state_and_panels();
        Ok(path_refs.len())
    }

    pub fn git_unstage_all(&mut self) -> Result<usize> {
        let git = GitService::open(&self.workspace_root())?;
        let statuses = git.status()?;
        let paths: Vec<PathBuf> = statuses.into_iter().map(|status| status.path).collect();
        let path_refs: Vec<&Path> = paths.iter().map(PathBuf::as_path).collect();
        if !path_refs.is_empty() {
            git.unstage(&path_refs)?;
        }
        self.active_destination = ShellDestination::Build;
        self.active_panel = PANEL_GIT_OPS.into();
        self.broadcast_state_and_panels();
        Ok(path_refs.len())
    }

    pub fn git_commit(&mut self, message: &str) -> Result<String> {
        if message.trim().is_empty() {
            return Err(anyhow!("Commit message cannot be empty"));
        }
        let git = GitService::open(&self.workspace_root())?;
        let hash = git.commit(message)?;
        self.active_destination = ShellDestination::Build;
        self.active_panel = PANEL_GIT_OPS.into();
        self.broadcast_state_and_panels();
        Ok(hash)
    }

    pub fn update_agent_status(
        &mut self,
        run_id: &str,
        status: &str,
        detail: impl Into<String>,
        elapsed_ms: u64,
        cost_usd: f64,
    ) {
        let detail = detail.into();
        if let Some(run) = self.agent_runs.iter_mut().find(|run| run.run_id == run_id) {
            run.status = status.into();
            run.detail = detail.clone();
            run.elapsed_ms = elapsed_ms;
            run.cost_usd = cost_usd;
        }

        match status {
            "running" => self.record_activity(ActivityEvent::TaskProgress {
                task_id: run_id.to_string(),
                progress: 0.5,
                message: detail.clone(),
            }),
            "completed" => self.record_activity(ActivityEvent::AgentCompleted {
                agent_id: run_id.to_string(),
                duration_ms: elapsed_ms,
                cost: cost_usd,
            }),
            "failed" => self.record_activity(ActivityEvent::AgentFailed {
                agent_id: run_id.to_string(),
                error: detail.clone(),
            }),
            _ => {}
        }

        // Publish learning cortex events for agent completion/failure
        if let Some(ref tx) = self.cortex_event_tx {
            match status {
                "completed" => {
                    let _ = tx.send(
                        hive_learn::cortex::event_bus::CortexEvent::OutcomeRecorded {
                            interaction_id: run_id.to_string(),
                            model: String::new(),
                            quality_score: 0.8,
                            outcome: "accepted".to_string(),
                        },
                    );
                }
                "failed" => {
                    let _ = tx.send(
                        hive_learn::cortex::event_bus::CortexEvent::OutcomeRecorded {
                            interaction_id: run_id.to_string(),
                            model: String::new(),
                            quality_score: 0.3,
                            outcome: "corrected".to_string(),
                        },
                    );
                }
                _ => {}
            }
        }

        let _ = self.event_tx.send(DaemonEvent::AgentStatus {
            run_id: run_id.to_string(),
            status: status.to_string(),
            detail,
        });
        self.broadcast_state_and_panels();
    }

    pub fn complete_stream(
        &mut self,
        conversation_id: &str,
        model: &str,
        content: &str,
        prompt_tokens: u32,
        completion_tokens: u32,
        cost_usd: Option<f64>,
    ) -> Result<()> {
        self.append_assistant_message(
            conversation_id,
            content,
            model,
            prompt_tokens,
            completion_tokens,
            cost_usd,
        )?;
        self.is_streaming = false;
        self.current_model = normalize_model(model, &self.current_model);
        if let Some(cost) = cost_usd {
            self.record_activity(ActivityEvent::CostIncurred {
                agent_id: "remote-chat".into(),
                model: model.into(),
                input_tokens: prompt_tokens,
                output_tokens: completion_tokens,
                cost_usd: cost,
            });
        }

        // Publish learning cortex event for remote chat completions
        if let Some(ref tx) = self.cortex_event_tx {
            let _ = tx.send(hive_learn::cortex::event_bus::CortexEvent::OutcomeRecorded {
                interaction_id: conversation_id.to_string(),
                model: model.to_string(),
                quality_score: 0.5, // Default unknown quality for remote
                outcome: "unknown".to_string(),
            });
        }

        self.broadcast_state_and_panels();
        Ok(())
    }

    pub fn resume_approved_chat_stream(&mut self, conversation_id: &str, model: &str) {
        self.active_destination = ShellDestination::Build;
        self.active_panel = PANEL_CHAT.into();
        self.active_conversation = Some(conversation_id.to_string());
        self.current_model = normalize_model(model, &self.current_model);
        self.is_streaming = true;
        self.broadcast_state_and_panels();
    }

    pub fn fail_stream(&mut self, conversation_id: &str, message: &str) -> Result<()> {
        self.append_error_message(conversation_id, message)?;
        self.is_streaming = false;
        self.broadcast_state_and_panels();
        Ok(())
    }

    pub fn apply_approval_decision(
        &mut self,
        request_id: &str,
        approved: bool,
        reason: Option<String>,
    ) -> Result<Option<PendingAction>> {
        if !self.pending_actions.contains_key(request_id) {
            return Err(anyhow!("Unknown approval request '{request_id}'"));
        }

        self.approval_gate.respond(
            request_id,
            if approved {
                ApprovalDecision::Approved
            } else {
                ApprovalDecision::Denied {
                    reason: reason.clone(),
                }
            },
        );

        let pending = self.pending_actions.remove(request_id);
        if approved {
            self.record_activity(ActivityEvent::ApprovalGranted {
                request_id: request_id.into(),
            });
        } else {
            self.record_activity(ActivityEvent::ApprovalDenied {
                request_id: request_id.into(),
                reason: reason.clone(),
            });
        }

        if let Some(PendingAction::Chat {
            conversation_id,
            content,
            ..
        }) = pending.as_ref()
            && !approved
        {
            let denial = match reason {
                Some(ref value) if !value.trim().is_empty() => {
                    format!("Remote approval denied: {}. Request: {}", value.trim(), content)
                }
                _ => format!("Remote approval denied for request: {}", content),
            };
            self.append_error_message(conversation_id, &denial)?;
        }

        if let Some(PendingAction::Agent { run_id, .. }) = pending.as_ref() && !approved {
            if let Some(run) = self.agent_runs.iter_mut().find(|run| run.run_id == *run_id) {
                run.status = "denied".into();
                run.detail = reason
                    .clone()
                    .unwrap_or_else(|| "Approval denied from remote".into());
            }
        }

        self.append_journal(&DaemonEvent::ApprovalDecision {
            request_id: request_id.to_string(),
            approved,
            reason,
        });
        self.broadcast_state_and_panels();
        Ok(pending)
    }

    pub fn panel_response(&self, panel_id: &str) -> Result<PanelResponse> {
        let panel = canonical_panel_id(panel_id);
        Ok(PanelResponse {
            panel: panel.clone(),
            data: self.panel_payload(&panel)?,
        })
    }

    pub async fn handle_event(&mut self, event: DaemonEvent) {
        // Mark interaction activity for the learning cortex
        if let Some(ref tracker) = self.interaction_tracker {
            tracker.store(
                chrono::Utc::now().timestamp(),
                std::sync::atomic::Ordering::Relaxed,
            );
        }

        self.append_journal(&event);
        match event {
            DaemonEvent::SwitchPanel { panel } => self.switch_panel(&panel),
            DaemonEvent::SwitchDestination { destination } => self.switch_destination(destination),
            DaemonEvent::SetModel { model } => self.set_model(model),
            DaemonEvent::SetObserveView { view } => self.set_observe_view(view),
            DaemonEvent::SwitchWorkspace { workspace_path } => self.switch_workspace(workspace_path),
            DaemonEvent::ResumeConversation { conversation_id } => {
                let _ = self.resume_conversation(&conversation_id);
            }
            DaemonEvent::CancelAgentTask { run_id } => self.cancel_agent_task(&run_id),
            DaemonEvent::ResponseFeedback {
                message_id,
                positive,
            } => {
                if let Some(ref tx) = self.cortex_event_tx {
                    let quality = if positive { 0.9 } else { 0.3 };
                    let outcome = if positive { "accepted" } else { "corrected" };
                    let _ = tx.send(
                        hive_learn::cortex::event_bus::CortexEvent::OutcomeRecorded {
                            interaction_id: message_id,
                            model: String::new(),
                            quality_score: quality,
                            outcome: outcome.to_string(),
                        },
                    );
                }
            }
            DaemonEvent::Ping => {
                let _ = self.event_tx.send(DaemonEvent::Pong);
            }
            _ => {}
        }
    }

    pub fn replay_journal(&mut self) -> Result<()> {
        let events = SessionJournal::replay(self.journal.path())?;
        for event in events {
            match event {
                DaemonEvent::SwitchPanel { panel } => {
                    self.active_panel = canonical_panel_id(&panel);
                    if let Some(destination) = panel_destination(&self.active_panel) {
                        self.active_destination = destination;
                    }
                }
                DaemonEvent::SwitchDestination { destination } => {
                    self.active_destination = destination;
                    self.active_panel = default_panel_for_destination(destination).into();
                }
                DaemonEvent::SendMessage {
                    conversation_id,
                    model,
                    ..
                } => {
                    self.active_conversation = Some(conversation_id);
                    self.current_model = normalize_model(&model, &self.current_model);
                }
                DaemonEvent::SetModel { model } => {
                    self.current_model = normalize_model(&model, &self.current_model);
                }
                DaemonEvent::SetObserveView { view } => {
                    self.observe_view = view;
                }
                DaemonEvent::SwitchWorkspace { workspace_path } => {
                    let workspace = workspace_from_path(Path::new(&workspace_path), true, false);
                    self.current_workspace = workspace.clone();
                    self.files_current_path = PathBuf::from(&workspace.path);
                    self.selected_file = None;
                    self.selected_spec = None;
                    if let Some(existing) = self
                        .workspaces
                        .iter_mut()
                        .find(|candidate| candidate.path == workspace.path)
                    {
                        existing.is_current = true;
                    } else {
                        self.workspaces.push(workspace);
                    }
                }
                _ => {}
            }
        }
        if !self.files_current_path.exists() {
            self.files_current_path = self.workspace_root();
        }
        Ok(())
    }

    pub fn ai_messages_for_conversation(
        &self,
        conversation_id: &str,
    ) -> Result<Vec<hive_ai::types::ChatMessage>> {
        let conversation = self.conversation_store.load(conversation_id)?;
        Ok(conversation
            .messages
            .into_iter()
            .filter_map(|message| {
                let role = match message.role.as_str() {
                    "user" => hive_ai::types::MessageRole::User,
                    "assistant" => hive_ai::types::MessageRole::Assistant,
                    "system" => hive_ai::types::MessageRole::System,
                    "tool" => hive_ai::types::MessageRole::Tool,
                    "error" => hive_ai::types::MessageRole::Error,
                    _ => return None,
                };
                Some(hive_ai::types::ChatMessage {
                    role,
                    content: message.content,
                    timestamp: message.timestamp,
                    tool_call_id: None,
                    tool_calls: None,
                })
            })
            .collect())
    }

    pub fn panel_payload(&self, panel: &str) -> Result<PanelPayload> {
        Ok(match canonical_panel_id(panel).as_str() {
            PANEL_HOME => PanelPayload::Home(self.home_panel_data()?),
            PANEL_OBSERVE => PanelPayload::Observe(self.observe_panel_data()?),
            PANEL_CHAT => PanelPayload::Chat(self.chat_panel_data()?),
            PANEL_HISTORY => PanelPayload::History(self.history_panel_data()),
            PANEL_FILES => PanelPayload::Files(self.files_panel_data()?),
            PANEL_SPECS => PanelPayload::Specs(self.specs_panel_data()?),
            PANEL_AGENTS => PanelPayload::Agents(self.agents_panel_data()),
            PANEL_GIT_OPS => PanelPayload::GitOps(self.git_ops_panel_data()),
            PANEL_TERMINAL => PanelPayload::Terminal(self.terminal_panel_data()),
            PANEL_WORKFLOWS => PanelPayload::Workflows(self.workflows_panel_data()),
            PANEL_CHANNELS => PanelPayload::Channels(self.channels_panel_data()),
            PANEL_NETWORK => PanelPayload::Network(self.network_panel_data()),
            PANEL_ASSISTANT => PanelPayload::Assistant(self.assistant_panel_data()),
            PANEL_SETTINGS => PanelPayload::Settings(self.settings_panel_data()),
            PANEL_MODELS => PanelPayload::Models(self.models_panel_data()),
            PANEL_ROUTING => PanelPayload::Routing(self.routing_panel_data()),
            PANEL_SKILLS => PanelPayload::Skills(self.skills_panel_data()),
            PANEL_LAUNCH => PanelPayload::Launch(self.launch_panel_data()),
            PANEL_HELP => PanelPayload::Help(self.help_panel_data()),
            other => PanelPayload::Handoff(crate::protocol::HandoffPanelData {
                panel: other.into(),
                title: handoff_title(other).into(),
                description: format!(
                    "{} is wired into the remote shell, but desktop parity for that surface lands in a later phase.",
                    handoff_title(other)
                ),
                action_label: "Open desktop app".into(),
            }),
        })
    }

    pub fn broadcast_state_and_panels(&self) {
        let _ = self.event_tx.send(DaemonEvent::StateSnapshot(self.get_snapshot()));
        for panel in [PANEL_HOME, PANEL_CHAT, PANEL_OBSERVE, self.active_panel.as_str()] {
            if let Ok(response) = self.panel_response(panel) {
                let _ = self.event_tx.send(DaemonEvent::PanelData {
                    panel: response.panel,
                    data: serde_json::to_value(response.data).unwrap_or_default(),
                });
            }
        }
    }

    fn append_journal(&mut self, event: &DaemonEvent) {
        if let Err(error) = self.journal.append(event) {
            tracing::error!("Failed to append daemon journal event: {error}");
        }
    }

    fn record_activity(&self, event: ActivityEvent) {
        if let Err(error) = self.activity_log.record(&event) {
            tracing::warn!("Failed to record activity event: {error}");
        }
    }

    fn append_user_message(
        &mut self,
        conversation_id: &str,
        content: &str,
        model: &str,
    ) -> Result<()> {
        let mut conversation = self.load_or_create_conversation(conversation_id, content, model)?;
        conversation.messages.push(StoredMessage {
            role: "user".into(),
            content: content.into(),
            timestamp: Utc::now(),
            model: None,
            cost: None,
            tokens: None,
            thinking: None,
            is_compacted: false,
            compacted_from: None,
        });
        conversation.updated_at = Utc::now();
        conversation.model = model.into();
        conversation.title = generate_title(&conversation.messages);
        self.conversation_store.save(&conversation)?;
        Ok(())
    }

    fn append_assistant_message(
        &mut self,
        conversation_id: &str,
        content: &str,
        model: &str,
        prompt_tokens: u32,
        completion_tokens: u32,
        cost_usd: Option<f64>,
    ) -> Result<()> {
        let mut conversation = self.load_or_create_conversation(conversation_id, content, model)?;
        conversation.messages.push(StoredMessage {
            role: "assistant".into(),
            content: content.into(),
            timestamp: Utc::now(),
            model: Some(model.into()),
            cost: cost_usd,
            tokens: Some(prompt_tokens + completion_tokens),
            thinking: None,
            is_compacted: false,
            compacted_from: None,
        });
        if let Some(cost) = cost_usd {
            conversation.total_cost += cost;
        }
        conversation.total_tokens += prompt_tokens + completion_tokens;
        conversation.updated_at = Utc::now();
        conversation.model = model.into();
        self.conversation_store.save(&conversation)?;
        Ok(())
    }

    fn append_error_message(&mut self, conversation_id: &str, message: &str) -> Result<()> {
        let mut conversation =
            self.load_or_create_conversation(conversation_id, message, &self.current_model)?;
        conversation.messages.push(StoredMessage {
            role: "error".into(),
            content: message.into(),
            timestamp: Utc::now(),
            model: None,
            cost: None,
            tokens: None,
            thinking: None,
            is_compacted: false,
            compacted_from: None,
        });
        conversation.updated_at = Utc::now();
        self.conversation_store.save(&conversation)?;
        Ok(())
    }

    fn load_or_create_conversation(
        &self,
        conversation_id: &str,
        seed_content: &str,
        model: &str,
    ) -> Result<Conversation> {
        match self.conversation_store.load(conversation_id) {
            Ok(conversation) => Ok(conversation),
            Err(_) => Ok(Conversation {
                id: conversation_id.into(),
                title: seed_content
                    .split_whitespace()
                    .take(7)
                    .collect::<Vec<_>>()
                    .join(" "),
                messages: Vec::new(),
                model: model.into(),
                total_cost: 0.0,
                total_tokens: 0,
                created_at: Utc::now(),
                updated_at: Utc::now(),
                parent_id: None,
                branch_point_index: None,
                branch_name: None,
            }),
        }
    }

    fn chat_panel_data(&self) -> Result<crate::protocol::ChatPanelData> {
        let (messages, total_cost) = if let Some(conversation_id) = self.active_conversation.as_deref()
        {
            if let Ok(conversation) = self.conversation_store.load(conversation_id) {
                (
                    conversation
                        .messages
                        .into_iter()
                        .map(remote_message_from_stored)
                        .collect(),
                    conversation.total_cost,
                )
            } else {
                (Vec::new(), 0.0)
            }
        } else {
            (Vec::new(), 0.0)
        };

        Ok(crate::protocol::ChatPanelData {
            conversation_id: self.active_conversation.clone(),
            current_model: self.current_model.clone(),
            is_streaming: self.is_streaming,
            total_cost,
            messages,
            conversations: self.conversation_summaries(),
            available_models: self.available_models(),
            pending_approvals: self.approval_cards(Some("chat")),
        })
    }

    fn history_panel_data(&self) -> crate::protocol::HistoryPanelData {
        crate::protocol::HistoryPanelData {
            active_conversation: self.active_conversation.clone(),
            conversations: self.conversation_summaries(),
        }
    }

    fn files_panel_data(&self) -> Result<crate::protocol::FilesPanelData> {
        let root = self.workspace_root();
        let current_path = if self.files_current_path.exists() {
            self.files_current_path.clone()
        } else {
            root.clone()
        };
        let entries = FileService::list_dir(&current_path)?
            .into_iter()
            .map(|entry| crate::protocol::FileEntryData {
                name: entry.name,
                path: entry.path.display().to_string(),
                is_dir: entry.is_dir,
                size: entry.size,
                modified: entry.modified.map(timestamp_from_system_time),
            })
            .collect();

        let (preview, preview_error) = match self.selected_file.as_ref() {
            Some(path) => match self.build_file_preview(path) {
                Ok(preview) => (Some(preview), None),
                Err(error) => (None, Some(error.to_string())),
            },
            None => (None, None),
        };

        Ok(crate::protocol::FilesPanelData {
            workspace_root: root.display().to_string(),
            current_path: current_path.display().to_string(),
            breadcrumbs: file_breadcrumbs(&root, &current_path),
            entries,
            preview,
            preview_error,
        })
    }

    fn specs_panel_data(&self) -> Result<crate::protocol::SpecsPanelData> {
        let root = self.workspace_root();
        let mut specs = list_spec_files(&root)?;
        specs.sort();

        let summaries: Vec<(crate::protocol::SpecSummaryData, PathBuf, Spec)> = specs
            .into_iter()
            .filter_map(|path| {
                let spec = load_spec_file(&path).ok()?;
                let summary = crate::protocol::SpecSummaryData {
                    id: spec.id.clone(),
                    path: path.display().to_string(),
                    title: spec.title.clone(),
                    description: spec.description.clone(),
                    status: spec_status_label(spec.status),
                    domain: spec.domain.clone(),
                    updated_at: spec.updated_at.to_rfc3339(),
                    completion_pct: spec.completion_pct(),
                    entry_count: spec.entry_count(),
                    checked_count: spec.checked_count(),
                };
                Some((summary, path, spec))
            })
            .collect();

        let selected_path = self
            .selected_spec
            .clone()
            .or_else(|| summaries.first().map(|(_, path, _)| path.clone()));
        let selected_spec = selected_path
            .as_ref()
            .and_then(|selected_path| summaries.iter().find(|(_, path, _)| path == selected_path))
            .map(|(summary, path, spec)| crate::protocol::SpecDetailData {
                id: summary.id.clone(),
                path: path.display().to_string(),
                title: spec.title.clone(),
                description: spec.description.clone(),
                status: spec_status_label(spec.status),
                domain: spec.domain.clone(),
                updated_at: spec.updated_at.to_rfc3339(),
                version: spec.version,
                completion_pct: spec.completion_pct(),
                sections: spec_sections(spec),
            });

        Ok(crate::protocol::SpecsPanelData {
            workspace_root: root.display().to_string(),
            selected_spec_id: selected_spec.as_ref().map(|spec| spec.id.clone()),
            specs: summaries.into_iter().map(|(summary, _, _)| summary).collect(),
            selected_spec,
        })
    }

    fn agents_panel_data(&self) -> crate::protocol::AgentsPanelData {
        let active_runs = self
            .agent_runs
            .iter()
            .filter(|run| matches!(run.status.as_str(), "planning" | "running" | "pending_approval"))
            .map(agent_run_card)
            .collect();
        let recent_runs = self
            .agent_runs
            .iter()
            .rev()
            .take(10)
            .map(agent_run_card)
            .collect();

        crate::protocol::AgentsPanelData {
            current_model: self.current_model.clone(),
            active_runs,
            recent_runs,
            pending_approvals: self.approval_cards(Some("observe")),
            orchestration_modes: vec![
                crate::protocol::AgentModeData {
                    id: "coordinator".into(),
                    label: "Coordinator".into(),
                    description: "Plan a multi-step run and delegate follow-through.".into(),
                },
                crate::protocol::AgentModeData {
                    id: "researcher".into(),
                    label: "Researcher".into(),
                    description: "Investigate code or behavior before proposing action.".into(),
                },
                crate::protocol::AgentModeData {
                    id: "fixer".into(),
                    label: "Fixer".into(),
                    description: "Take a focused repair task through completion.".into(),
                },
            ],
        }
    }

    fn git_ops_panel_data(&self) -> crate::protocol::GitOpsPanelData {
        let root = self.workspace_root();
        match GitService::open(&root) {
            Ok(git) => {
                let statuses = git.status().unwrap_or_default();
                let commits = git
                    .log(8)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|entry| crate::protocol::GitCommitData {
                        short_hash: entry.hash.chars().take(8).collect(),
                        hash: entry.hash,
                        message: entry.message,
                        author: entry.author,
                        timestamp: chrono::DateTime::<Utc>::from_timestamp(entry.timestamp, 0)
                            .map(|timestamp| timestamp.to_rfc3339())
                            .unwrap_or_else(|| entry.timestamp.to_string()),
                    })
                    .collect();

                crate::protocol::GitOpsPanelData {
                    repo_path: root.display().to_string(),
                    is_repo: true,
                    branch: git.current_branch().ok(),
                    dirty_count: statuses.len(),
                    files: statuses
                        .into_iter()
                        .map(|status| crate::protocol::GitFileData {
                            path: status.path.display().to_string(),
                            status: git_status_label(status.status),
                        })
                        .collect(),
                    commits,
                    diff: truncate_diff(&git.diff().unwrap_or_default()),
                    can_commit: true,
                    error: None,
                }
            }
            Err(error) => crate::protocol::GitOpsPanelData {
                repo_path: root.display().to_string(),
                is_repo: false,
                branch: None,
                dirty_count: 0,
                files: Vec::new(),
                commits: Vec::new(),
                diff: String::new(),
                can_commit: false,
                error: Some(error.to_string()),
            },
        }
    }

    fn terminal_panel_data(&self) -> crate::protocol::TerminalPanelData {
        crate::protocol::TerminalPanelData {
            cwd: self.workspace_root().display().to_string(),
            is_running: self.terminal_shell.is_some(),
            last_exit_code: self.terminal_last_exit_code,
            lines: self.terminal_lines.clone(),
        }
    }

    fn workflows_panel_data(&self) -> crate::protocol::WorkflowsPanelData {
        crate::protocol::WorkflowsPanelData {
            workspace_root: self.workspace_root().display().to_string(),
            source_dir: self
                .workspace_root()
                .join(hive_agents::automation::USER_WORKFLOW_DIR)
                .display()
                .to_string(),
            workflows: self
                .automation_service
                .list_workflows()
                .iter()
                .map(|workflow| crate::protocol::WorkflowSummaryData {
                    id: workflow.id.clone(),
                    name: workflow.name.clone(),
                    description: workflow.description.clone(),
                    status: workflow_status_label(workflow.status),
                    trigger: workflow_trigger_label(&workflow.trigger),
                    step_count: workflow.steps.len(),
                    run_count: workflow.run_count,
                    last_run: workflow.last_run.map(|value| value.to_rfc3339()),
                    is_builtin: workflow.id.starts_with("builtin:"),
                })
                .collect(),
            active_runs: self
                .workflow_runs
                .iter()
                .filter(|run| run.status == "running")
                .map(workflow_run_data)
                .collect(),
            recent_runs: self
                .workflow_runs
                .iter()
                .rev()
                .take(8)
                .map(workflow_run_data)
                .collect(),
        }
    }

    fn channels_panel_data(&self) -> crate::protocol::ChannelsPanelData {
        let channels: Vec<crate::protocol::ChannelSummaryData> = self
            .channel_store
            .list_channels()
            .iter()
            .map(|channel| crate::protocol::ChannelSummaryData {
                id: channel.id.clone(),
                name: channel.name.clone(),
                icon: channel.icon.clone(),
                description: channel.description.clone(),
                assigned_agents: channel.assigned_agents.clone(),
                message_count: channel.messages.len(),
                updated_at: channel.updated_at.to_rfc3339(),
            })
            .collect();

        let selected_channel = self
            .selected_channel_id
            .as_ref()
            .and_then(|channel_id| self.channel_store.get_channel(channel_id))
            .or_else(|| self.channel_store.list_channels().first())
            .map(|channel| crate::protocol::ChannelDetailData {
                id: channel.id.clone(),
                name: channel.name.clone(),
                icon: channel.icon.clone(),
                description: channel.description.clone(),
                assigned_agents: channel.assigned_agents.clone(),
                pinned_files: channel.pinned_files.clone(),
                messages: channel
                    .messages
                    .iter()
                    .map(channel_message_data)
                    .collect(),
            });

        crate::protocol::ChannelsPanelData {
            current_model: self.current_model.clone(),
            selected_channel_id: selected_channel.as_ref().map(|channel| channel.id.clone()),
            channels,
            selected_channel,
        }
    }

    fn network_panel_data(&self) -> crate::protocol::NetworkPanelData {
        if let Some(handle) = self.network_handle.as_ref() {
            let peers = handle.peers_snapshot();
            let peers: Vec<crate::protocol::NetworkPeerData> = peers
                .iter()
                .map(|peer| crate::protocol::NetworkPeerData {
                    name: peer.identity.name.clone(),
                    status: network_peer_status_label(&peer.state),
                    address: peer.addr.to_string(),
                    latency_ms: peer.latency_ms,
                    last_seen: relative_time(peer.last_seen),
                })
                .collect();
            let connected_count = peers.iter().filter(|peer| peer.status == "Connected").count();

            crate::protocol::NetworkPanelData {
                available: true,
                our_peer_id: handle.peer_id().to_string(),
                connected_count,
                total_count: peers.len(),
                peers,
                note: None,
            }
        } else {
            crate::protocol::NetworkPanelData {
                available: false,
                our_peer_id: String::new(),
                connected_count: 0,
                total_count: 0,
                peers: Vec::new(),
                note: Some(
                    "Network runtime was not attached to the remote daemon in this process."
                        .into(),
                ),
            }
        }
    }

    fn assistant_panel_data(&self) -> crate::protocol::AssistantPanelData {
        let Some(assistant) = self.assistant_service.as_ref() else {
            return crate::protocol::AssistantPanelData {
                connected_account_count: self.connected_account_count,
                briefing: None,
                events: Vec::new(),
                email_groups: Vec::new(),
                reminders: Vec::new(),
                approvals: Vec::new(),
                recent_actions: Vec::new(),
            };
        };

        let project_root = self.workspace_root();
        let briefing = assistant.daily_briefing_for_project(Some(&project_root));
        let approvals = assistant.approval_service.list_pending().unwrap_or_default();

        crate::protocol::AssistantPanelData {
            connected_account_count: self.connected_account_count,
            briefing: Some(crate::protocol::AssistantBriefingData {
                greeting: "Good morning!".into(),
                date: briefing.date.clone(),
                event_count: briefing.events.len(),
                unread_emails: briefing.email_summary.as_ref().map_or(0, |summary| summary.email_count),
                active_reminders: briefing.active_reminders.len(),
                top_priority: briefing.action_items.first().cloned(),
            }),
            events: briefing
                .events
                .iter()
                .map(|event| crate::protocol::AssistantEventData {
                    title: event.title.clone(),
                    time: event.start.clone(),
                    location: event.location.clone(),
                    is_conflict: false,
                })
                .collect(),
            email_groups: briefing
                .email_summary
                .into_iter()
                .map(|summary| crate::protocol::AssistantEmailGroupData {
                    provider: format!("{:?}", summary.provider),
                    previews: vec![crate::protocol::AssistantEmailPreviewData {
                        from: format!("{:?}", summary.provider),
                        subject: "Inbox summary".into(),
                        snippet: summary.summary,
                        time: summary.created_at,
                        important: false,
                    }],
                })
                .collect(),
            reminders: briefing
                .active_reminders
                .iter()
                .map(|reminder| crate::protocol::AssistantReminderData {
                    title: reminder.title.clone(),
                    due: match &reminder.trigger {
                        hive_assistant::ReminderTrigger::At(at) => at.format("%Y-%m-%d %H:%M").to_string(),
                        hive_assistant::ReminderTrigger::Recurring(expr) => {
                            format!("Recurring: {expr}")
                        }
                        hive_assistant::ReminderTrigger::OnEvent(event) => {
                            format!("On event: {event}")
                        }
                    },
                    is_overdue: matches!(
                        &reminder.trigger,
                        hive_assistant::ReminderTrigger::At(at) if *at <= Utc::now()
                    ),
                })
                .collect(),
            approvals: approvals
                .iter()
                .map(|approval| crate::protocol::AssistantApprovalData {
                    id: approval.id.clone(),
                    action: approval.action.clone(),
                    resource: approval.resource.clone(),
                    level: format!("{:?}", approval.level),
                    requested_by: approval.requested_by.clone(),
                    created_at: approval.created_at.clone(),
                })
                .collect(),
            recent_actions: briefing
                .action_items
                .iter()
                .take(6)
                .map(|item| crate::protocol::AssistantRecentActionData {
                    description: item.clone(),
                    timestamp: briefing.date.clone(),
                    action_type: "briefing".into(),
                })
                .collect(),
        }
    }

    fn settings_panel_data(&self) -> crate::protocol::SettingsPanelData {
        let config = self.config_snapshot();
        crate::protocol::SettingsPanelData {
            current_workspace: self.current_workspace.path.clone(),
            theme: config.theme,
            privacy_mode: config.privacy_mode,
            shield_enabled: config.shield_enabled,
            notifications_enabled: config.notifications_enabled,
            auto_update: config.auto_update,
            remote_enabled: config.remote_enabled,
            remote_auto_start: config.remote_auto_start,
            remote_local_port: config.remote_local_port,
            remote_web_port: config.remote_web_port,
            ollama_url: config.ollama_url,
            lmstudio_url: config.lmstudio_url,
            litellm_url: config.litellm_url,
            local_provider_url: config.local_provider_url,
            connected_account_count: config.connected_accounts.len(),
        }
    }

    fn models_panel_data(&self) -> crate::protocol::ModelsPanelData {
        let config = self.config_snapshot();
        let available_provider_types = self.available_provider_types();
        let configured_provider_types = configured_provider_types(&config);
        let mut available_models = Vec::new();
        append_model_options(&mut available_models, &available_provider_types, &self.current_model);

        crate::protocol::ModelsPanelData {
            current_model: self.current_model.clone(),
            default_model: config.default_model.clone(),
            auto_routing: config.auto_routing,
            project_models: config.project_models.clone(),
            available_models: dedupe_models(available_models),
            available_providers: provider_labels(&available_provider_types),
            configured_providers: provider_labels(&configured_provider_types),
            provider_credentials: provider_credentials(&config),
        }
    }

    fn routing_panel_data(&self) -> crate::protocol::RoutingPanelData {
        let config = self.config_snapshot();
        let available_providers = provider_labels(&self.available_provider_types());
        let strategy_summary = if config.auto_routing {
            if config.project_models.is_empty() {
                "Automatic routing is enabled and Hive will use the default fallback chain."
                    .into()
            } else {
                format!(
                    "Automatic routing is enabled and {} project models are pinned into the fallback chain.",
                    config.project_models.len()
                )
            }
        } else {
            format!(
                "Automatic routing is disabled. Hive will prefer the explicit default model '{}'.",
                config.default_model
            )
        };

        let mut notes = vec![format!("Current remote model: {}", self.current_model)];
        if config.project_models.is_empty() {
            notes.push("No project-specific model overrides are configured.".into());
        } else {
            notes.push("Project-specific model overrides are available for routing.".into());
        }
        if available_providers.is_empty() {
            notes.push("No providers are currently available to the remote shell.".into());
        } else {
            notes.push(format!(
                "{} providers are currently available to the remote shell.",
                available_providers.len()
            ));
        }

        crate::protocol::RoutingPanelData {
            auto_routing: config.auto_routing,
            default_model: config.default_model,
            strategy_summary,
            project_models: config.project_models,
            available_providers,
            notes,
        }
    }

    fn skills_panel_data(&self) -> crate::protocol::SkillsPanelData {
        let skills_dir = self.skills_dir();
        let registry = SkillsRegistry::with_loader(SkillLoader::new(skills_dir.clone()));
        let skills = registry.list();
        let total_skills = skills.len();
        let enabled_skills = skills.iter().filter(|skill| skill.enabled).count();
        let builtin_skills = skills
            .iter()
            .filter(|skill| skill.source == SkillSource::BuiltIn)
            .count();
        let community_skills = skills
            .iter()
            .filter(|skill| skill.source == SkillSource::Community)
            .count();
        let custom_skills = skills
            .iter()
            .filter(|skill| skill.source == SkillSource::Custom)
            .count();

        crate::protocol::SkillsPanelData {
            skills_dir: skills_dir.display().to_string(),
            total_skills,
            enabled_skills,
            builtin_skills,
            community_skills,
            custom_skills,
            skills: skills
                .into_iter()
                .take(24)
                .map(|skill| crate::protocol::SkillSummaryData {
                    name: skill.name.clone(),
                    description: skill.description.clone(),
                    source: format!("{:?}", skill.source),
                    enabled: skill.enabled,
                })
                .collect(),
        }
    }

    fn launch_panel_data(&self) -> crate::protocol::LaunchPanelData {
        let config = self.config_snapshot();
        crate::protocol::LaunchPanelData {
            remote_enabled: config.remote_enabled,
            remote_auto_start: config.remote_auto_start,
            local_api_port: config.remote_local_port,
            web_port: config.remote_web_port,
            local_api_url: format!("http://127.0.0.1:{}", config.remote_local_port),
            web_url: format!("http://127.0.0.1:{}", config.remote_web_port),
            cloud_api_url: config.cloud_api_url,
            cloud_relay_url: config.cloud_relay_url,
            cloud_tier: config.cloud_tier,
        }
    }

    fn help_panel_data(&self) -> crate::protocol::HelpPanelData {
        crate::protocol::HelpPanelData {
            version: env!("CARGO_PKG_VERSION").into(),
            docs: vec![
                crate::protocol::HelpLinkData {
                    title: "Home".into(),
                    detail: "Launch a mission, resume active work, or jump into approvals.".into(),
                },
                crate::protocol::HelpLinkData {
                    title: "Build".into(),
                    detail:
                        "Use Chat, History, Files, Specs, Agents, Git Ops, and Terminal for repo work."
                            .into(),
                },
                crate::protocol::HelpLinkData {
                    title: "Observe".into(),
                    detail:
                        "Use Observe to clear approvals, watch runtime health, and review spend or safety."
                            .into(),
                },
            ],
            quick_tips: vec![
                "Use Home to launch a mission, then switch to Chat or Observe as the run progresses."
                    .into(),
                "Approve risky writes from Observe or the Chat approval cards without opening desktop."
                    .into(),
                "Use the utility drawer for runtime setup, routing, skills, and remote launch details."
                    .into(),
            ],
            troubleshooting: vec![
                "If chat streaming does not start, verify at least one AI provider is configured on the paired machine."
                    .into(),
                "If Network shows unavailable, the remote daemon was started without the desktop network runtime."
                    .into(),
                "If a surface looks thinner than desktop, prefer the paired desktop app for advanced editing flows."
                    .into(),
            ],
        }
    }

    fn home_panel_data(&self) -> Result<crate::protocol::HomePanelData> {
        let providers_online = self.available_models().len() > 1;
        let launch_ready = providers_online && !self.current_model.trim().is_empty();
        let conversation_summaries = self.conversation_summaries();
        let pending_approval_count = self.pending_actions.len();

        let priorities = vec![
            if self.is_streaming {
                crate::protocol::HomePriorityCardData {
                    eyebrow: "Resume run".into(),
                    title: "Chat is actively streaming".into(),
                    detail: "Open Chat to monitor the current response and next action.".into(),
                    action_label: "Open Chat".into(),
                    action_panel: PANEL_CHAT.into(),
                    tone: "ready".into(),
                }
            } else if self.active_conversation.is_some() || !conversation_summaries.is_empty() {
                crate::protocol::HomePriorityCardData {
                    eyebrow: "Resume context".into(),
                    title: "Continue the latest conversation".into(),
                    detail: "The latest remote conversation is ready to resume.".into(),
                    action_label: "Resume Chat".into(),
                    action_panel: PANEL_CHAT.into(),
                    tone: "ready".into(),
                }
            } else {
                crate::protocol::HomePriorityCardData {
                    eyebrow: "Fresh mission".into(),
                    title: "Launch the next guided run".into(),
                    detail: "Choose a mission below and Hive will open Chat with the right kickoff prompt."
                        .into(),
                    action_label: "Open Launch".into(),
                    action_panel: PANEL_HOME.into(),
                    tone: if launch_ready {
                        "ready".into()
                    } else {
                        "action".into()
                    },
                }
            },
            if pending_approval_count > 0 {
                crate::protocol::HomePriorityCardData {
                    eyebrow: "Blocked work".into(),
                    title: format!("{pending_approval_count} approvals need review"),
                    detail: "Observe is the fastest way to unblock remote work before another run starts."
                        .into(),
                    action_label: "Open Observe".into(),
                    action_panel: PANEL_OBSERVE.into(),
                    tone: "action".into(),
                }
            } else {
                crate::protocol::HomePriorityCardData {
                    eyebrow: "Observe".into(),
                    title: "No approvals are blocking work".into(),
                    detail: "Use Observe to inspect recent runs, failures, spend, and safety posture."
                        .into(),
                    action_label: "Open Observe".into(),
                    action_panel: PANEL_OBSERVE.into(),
                    tone: "ready".into(),
                }
            },
        ];

        Ok(crate::protocol::HomePanelData {
            project_name: self.current_workspace.name.clone(),
            project_root: self.current_workspace.path.clone(),
            project_summary: format!(
                "Using {} as the active project context for remote work.",
                self.current_workspace.path
            ),
            current_model: self.current_model.clone(),
            pending_approval_count,
            launch_ready,
            launch_hint: if launch_ready {
                format!(
                    "Ready to launch a guided run in Chat with {}.",
                    self.current_model
                )
            } else {
                "Connect at least one model provider before launching a remote mission.".into()
            },
            last_launch_status: self.last_launch_status.clone(),
            templates: home_templates(),
            priorities,
            status_cards: vec![
                crate::protocol::HomeStatusCardData {
                    title: "Observe inbox".into(),
                    value: if pending_approval_count > 0 {
                        format!("{pending_approval_count} pending approvals")
                    } else {
                        "Quiet".into()
                    },
                    detail: if pending_approval_count > 0 {
                        "Resolve approvals and failed runs before shipping or launching more work."
                            .into()
                    } else {
                        "Approvals, failures, spend, and safety are currently stable.".into()
                    },
                    tone: if pending_approval_count > 0 {
                        "action".into()
                    } else {
                        "ready".into()
                    },
                    action_label: Some("Open Observe".into()),
                    action_panel: Some(PANEL_OBSERVE.into()),
                },
                crate::protocol::HomeStatusCardData {
                    title: "Model routing".into(),
                    value: self.current_model.clone(),
                    detail: if launch_ready {
                        "Home will use this model when it launches the next remote mission.".into()
                    } else {
                        "Connect a provider or pick a model before launching a remote mission.".into()
                    },
                    tone: if launch_ready {
                        "ready".into()
                    } else {
                        "action".into()
                    },
                    action_label: Some("Open Models".into()),
                    action_panel: Some(PANEL_MODELS.into()),
                },
                crate::protocol::HomeStatusCardData {
                    title: "Saved workspaces".into(),
                    value: format!("{} available", self.workspaces.len()),
                    detail: "Switch the active workspace without leaving the remote shell.".into(),
                    tone: "ready".into(),
                    action_label: Some("Open Files".into()),
                    action_panel: Some(PANEL_FILES.into()),
                },
            ],
            next_steps: vec![
                crate::protocol::HomeNextStepData {
                    title: "Observe".into(),
                    detail: "Review approvals, failures, cost, and safety before continuing a run.".into(),
                    action_label: "Open Observe".into(),
                    action_panel: PANEL_OBSERVE.into(),
                },
                crate::protocol::HomeNextStepData {
                    title: "Files".into(),
                    detail: "Review the active workspace before starting a mission.".into(),
                    action_label: "Open Files".into(),
                    action_panel: PANEL_FILES.into(),
                },
                crate::protocol::HomeNextStepData {
                    title: "History".into(),
                    detail: "Resume a prior conversation when the latest work already has useful context."
                        .into(),
                    action_label: "Open History".into(),
                    action_panel: PANEL_HISTORY.into(),
                },
            ],
            saved_workspaces: self.workspaces.clone(),
        })
    }

    fn observe_panel_data(&self) -> Result<crate::protocol::ObservePanelData> {
        let approvals = self.approval_cards(None);
        let recent_entries = self.activity_entries(18);
        let spend_today = self
            .activity_log
            .cost_summary(None, Utc::now() - chrono::Duration::days(1))
            .unwrap_or_default();
        let spend_all = self
            .activity_log
            .cost_summary(None, Utc::now() - chrono::Duration::days(3650))
            .unwrap_or_default();
        let available_models = self.available_models();
        let active_runs = self
            .agent_runs
            .iter()
            .filter(|run| matches!(run.status.as_str(), "planning" | "running" | "pending_approval"))
            .count();

        Ok(crate::protocol::ObservePanelData {
            current_view: self.observe_view,
            inbox: build_inbox_items(&approvals, &recent_entries),
            approvals: approvals.clone(),
            runtime: crate::protocol::ObserveRuntimeData {
                status_label: if self.is_streaming || active_runs > 0 {
                    "Active".into()
                } else {
                    "Quiet".into()
                },
                active_agents: active_runs,
                active_streams: usize::from(self.is_streaming),
                online_providers: available_models.len().saturating_sub(1),
                total_providers: available_models.len().saturating_sub(1),
                request_queue_length: self.pending_actions.len(),
                current_run_id: self.active_conversation.clone(),
                agents: self
                    .agent_runs
                    .iter()
                    .filter(|run| matches!(run.status.as_str(), "planning" | "running" | "pending_approval"))
                    .map(|run| crate::protocol::ObserveAgentRow {
                        role: "remote".into(),
                        status: run.status.clone(),
                        phase: run.detail.clone(),
                        model: self.current_model.clone(),
                        started_at: relative_timestamp_from_elapsed(run.elapsed_ms),
                    })
                    .collect(),
                recent_runs: self
                    .agent_runs
                    .iter()
                    .rev()
                    .take(8)
                    .map(|run| crate::protocol::ObserveRunRow {
                        id: run.run_id.clone(),
                        summary: run.goal.clone(),
                        status: run.status.clone(),
                        started_at: relative_timestamp_from_elapsed(run.elapsed_ms),
                        cost_usd: run.cost_usd,
                    })
                    .collect(),
            },
            spend: crate::protocol::ObserveSpendData {
                total_cost_usd: spend_all.total_usd,
                today_cost_usd: spend_today.total_usd,
                quality_score: quality_score(&self.agent_runs),
                quality_trend: if self.agent_runs.iter().any(|run| run.status == "failed") {
                    "Mixed".into()
                } else {
                    "Stable".into()
                },
                cost_efficiency: if spend_all.total_usd > 0.0 {
                    quality_score(&self.agent_runs) / spend_all.total_usd.max(0.01)
                } else {
                    quality_score(&self.agent_runs)
                },
                best_model: spend_all
                    .by_model
                    .iter()
                    .min_by(|left, right| left.1.total_cmp(&right.1))
                    .map(|entry| entry.0.clone()),
                worst_model: spend_all
                    .by_model
                    .iter()
                    .max_by(|left, right| left.1.total_cmp(&right.1))
                    .map(|entry| entry.0.clone()),
                weak_areas: weak_areas(&self.agent_runs, &spend_all),
            },
            safety: crate::protocol::ObserveSafetyData {
                shield_enabled: self.shield_enabled,
                pii_detections: 0,
                secrets_blocked: approvals
                    .iter()
                    .filter(|approval| approval.severity == "critical")
                    .count(),
                threats_caught: recent_entries
                    .iter()
                    .filter(|entry| matches!(entry.event_type.as_str(), "approval_denied" | "agent_failed" | "budget_exhausted"))
                    .count(),
                recent_events: recent_entries
                    .iter()
                    .filter_map(safety_event_from_entry)
                    .take(6)
                    .collect(),
            },
        })
    }

    fn conversation_summaries(&self) -> Vec<crate::protocol::ConversationSummaryData> {
        self.conversation_store
            .list_summaries()
            .unwrap_or_default()
            .into_iter()
            .take(12)
            .map(|summary| crate::protocol::ConversationSummaryData {
                id: summary.id,
                title: summary.title,
                preview: summary.preview,
                message_count: summary.message_count,
                total_cost: summary.total_cost,
                model: summary.model,
                updated_at: summary.updated_at.to_rfc3339(),
            })
            .collect()
    }

    fn activity_entries(&self, limit: usize) -> Vec<ActivityEntry> {
        self.activity_log
            .query(&ActivityFilter {
                limit,
                ..ActivityFilter::default()
            })
            .unwrap_or_default()
    }

    fn approval_cards(&self, source_filter: Option<&str>) -> Vec<crate::protocol::ApprovalCardData> {
        let mut requests = self.approval_gate.pending_requests();
        requests.sort_by(|left, right| left.timestamp.cmp(&right.timestamp));

        requests
            .into_iter()
            .filter_map(|request| {
                let action = self.pending_actions.get(&request.id)?;
                let source = match action {
                    PendingAction::Chat { .. } => "chat",
                    PendingAction::Agent { .. } => "observe",
                };
                if let Some(expected) = source_filter && source != expected {
                    return None;
                }
                Some(approval_card(&request, source, action))
            })
            .collect()
    }

    fn available_models(&self) -> Vec<crate::protocol::ModelOption> {
        let mut models = vec![crate::protocol::ModelOption {
            id: "auto".into(),
            label: "Auto".into(),
        }];
        let providers = self.available_provider_types();
        append_model_options(&mut models, &providers, self.current_model.as_str());
        dedupe_models(models)
    }

    fn available_provider_types(&self) -> Vec<hive_ai::types::ProviderType> {
        if let Ok(ai) = self.ai_service.try_lock() {
            let providers = ai.available_providers();
            if !providers.is_empty() {
                return providers;
            }
        }
        configured_provider_types(&self.config_snapshot())
    }

    fn apply_config_snapshot(&mut self, config: &HiveConfig) {
        self.connected_account_count = config.connected_accounts.len();
        self.shield_enabled = config.shield_enabled;
        if let Ok(mut ai) = self.ai_service.try_lock() {
            ai.update_config(ai_service_config(Some(config), &self.current_model));
        }
    }

    fn config_manager(&self) -> Result<ConfigManager> {
        config_manager_for_daemon(&self.config)
    }

    fn config_snapshot(&self) -> HiveConfig {
        self.config_manager()
            .ok()
            .map(|manager| manager.get())
            .unwrap_or_default()
    }

    fn skills_dir(&self) -> PathBuf {
        if let Some(root) = &self.config.config_root {
            root.join("skills")
        } else {
            HiveConfig::base_dir()
                .unwrap_or_else(|_| PathBuf::from(".hive"))
                .join("skills")
        }
    }

    fn workspace_root(&self) -> PathBuf {
        PathBuf::from(&self.current_workspace.path)
    }

    fn resolve_workspace_path(&self, raw_path: &str, fallback: Option<&Path>) -> Result<PathBuf> {
        let workspace_root = self.workspace_root();
        let root = workspace_root.canonicalize().unwrap_or(workspace_root.clone());
        let requested = if raw_path.trim().is_empty() {
            fallback
                .map(Path::to_path_buf)
                .unwrap_or_else(|| workspace_root.clone())
        } else {
            let path = PathBuf::from(raw_path);
            if path.is_absolute() {
                path
            } else {
                workspace_root.join(path)
            }
        };
        let resolved = requested
            .canonicalize()
            .map_err(|error| anyhow!("Failed to resolve path {}: {error}", requested.display()))?;
        if !resolved.starts_with(&root) {
            return Err(anyhow!(
                "Path must stay within the active workspace: {}",
                root.display()
            ));
        }
        Ok(resolved)
    }

    fn build_file_preview(&self, path: &Path) -> Result<crate::protocol::FilePreviewData> {
        let content = FileService::read_file(path)?;
        let stats = FileService::file_stats(path)?;
        Ok(crate::protocol::FilePreviewData {
            path: path.display().to_string(),
            content,
            size: stats.size,
            modified: stats.modified.map(timestamp_from_system_time),
        })
    }

    fn push_terminal_line(&mut self, stream: &str, content: String) {
        if content.is_empty() {
            return;
        }
        self.terminal_lines.push(crate::protocol::TerminalLineData {
            stream: stream.into(),
            content,
            timestamp: Utc::now().to_rfc3339(),
        });
        const MAX_TERMINAL_LINES: usize = 400;
        if self.terminal_lines.len() > MAX_TERMINAL_LINES {
            let drain = self.terminal_lines.len() - MAX_TERMINAL_LINES;
            self.terminal_lines.drain(0..drain);
        }
    }
}

fn ai_service_config(
    config: Option<&hive_core::config::HiveConfig>,
    fallback_model: &str,
) -> AiServiceConfig {
    match config {
        Some(cfg) => AiServiceConfig {
            anthropic_api_key: cfg.anthropic_api_key.clone(),
            openai_api_key: cfg.openai_api_key.clone(),
            openrouter_api_key: cfg.openrouter_api_key.clone(),
            google_api_key: cfg.google_api_key.clone(),
            groq_api_key: cfg.groq_api_key.clone(),
            huggingface_api_key: cfg.huggingface_api_key.clone(),
            xai_api_key: cfg.xai_api_key.clone(),
            mistral_api_key: cfg.mistral_api_key.clone(),
            venice_api_key: cfg.venice_api_key.clone(),
            litellm_url: cfg.litellm_url.clone(),
            litellm_api_key: cfg.litellm_api_key.clone(),
            ollama_url: cfg.ollama_url.clone(),
            lmstudio_url: cfg.lmstudio_url.clone(),
            local_provider_url: cfg.local_provider_url.clone(),
            kilo_url: Some(cfg.kilo_url.clone()),
            kilo_password: cfg.kilo_password.clone(),
            privacy_mode: cfg.privacy_mode,
            default_model: cfg.default_model.clone(),
            auto_routing: cfg.auto_routing,
        },
        None => AiServiceConfig {
            default_model: fallback_model.into(),
            ollama_url: "http://127.0.0.1:11434".into(),
            lmstudio_url: "http://127.0.0.1:1234".into(),
            auto_routing: true,
            ..AiServiceConfig::default()
        },
    }
}

fn default_remote_approval_rules() -> Vec<ApprovalRule> {
    let mut rules = ApprovalRule::defaults();
    rules.push(ApprovalRule {
        name: "remote-file-write".into(),
        enabled: true,
        trigger: RuleTrigger::PathMatches { glob: "*".into() },
        priority: 75,
    });
    rules.push(ApprovalRule {
        name: "remote-deploy".into(),
        enabled: true,
        trigger: RuleTrigger::CommandMatches {
            pattern: "deploy*".into(),
        },
        priority: 85,
    });
    rules
}

fn latest_conversation_id(store: &ConversationStore) -> Option<String> {
    store
        .list_summaries()
        .ok()
        .and_then(|summaries| summaries.into_iter().next())
        .map(|summary| summary.id)
}

fn workspace_from_path(path: &Path, is_current: bool, is_pinned: bool) -> WorkspaceSummary {
    WorkspaceSummary {
        name: path
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.trim().is_empty())
            .unwrap_or("Workspace")
            .to_string(),
        path: path.display().to_string(),
        is_current,
        is_pinned,
    }
}

fn canonical_panel_id(panel: &str) -> String {
    match panel.trim().to_lowercase().as_str() {
        "quickstart" | "home" => PANEL_HOME.into(),
        "activity" | "observe" => PANEL_OBSERVE.into(),
        "review" | "gitops" | "git_ops" => PANEL_GIT_OPS.into(),
        "promptlibrary" | "prompt_library" | "prompts" => PANEL_PROMPTS.into(),
        "codemap" | "code_map" => PANEL_CODE_MAP.into(),
        "tokenlaunch" | "token_launch" | "launch" => PANEL_LAUNCH.into(),
        other => other.into(),
    }
}

fn panel_destination(panel: &str) -> Option<ShellDestination> {
    match canonical_panel_id(panel).as_str() {
        PANEL_HOME => Some(ShellDestination::Home),
        PANEL_CHAT
        | PANEL_HISTORY
        | PANEL_FILES
        | PANEL_CODE_MAP
        | PANEL_PROMPTS
        | PANEL_SPECS
        | PANEL_AGENTS
        | PANEL_GIT_OPS
        | PANEL_TERMINAL => Some(ShellDestination::Build),
        PANEL_WORKFLOWS | PANEL_CHANNELS | PANEL_NETWORK => Some(ShellDestination::Automate),
        PANEL_ASSISTANT => Some(ShellDestination::Assist),
        PANEL_OBSERVE
        | PANEL_MONITOR
        | PANEL_LOGS
        | PANEL_COSTS
        | PANEL_LEARNING
        | PANEL_SHIELD => Some(ShellDestination::Observe),
        _ => None,
    }
}

fn default_panel_for_destination(destination: ShellDestination) -> &'static str {
    match destination {
        ShellDestination::Home => PANEL_HOME,
        ShellDestination::Build => PANEL_CHAT,
        ShellDestination::Automate => PANEL_WORKFLOWS,
        ShellDestination::Assist => PANEL_ASSISTANT,
        ShellDestination::Observe => PANEL_OBSERVE,
    }
}

fn panel_registry() -> crate::protocol::PanelRegistry {
    crate::protocol::PanelRegistry {
        destinations: vec![
            crate::protocol::DestinationPanels {
                destination: ShellDestination::Home,
                panels: vec![panel_meta(
                    PANEL_HOME,
                    "Home",
                    "Start work, launch missions, and resume context.",
                    true,
                )],
            },
            crate::protocol::DestinationPanels {
                destination: ShellDestination::Build,
                panels: vec![
                    panel_meta(PANEL_CHAT, "Chat", "Remote chat, approvals, and conversation resume.", true),
                    panel_meta(PANEL_FILES, "Files", "Browse workspace files.", true),
                    panel_meta(PANEL_HISTORY, "History", "Review prior conversations.", true),
                    panel_meta(PANEL_SPECS, "Specs", "Track implementation specs.", true),
                    panel_meta(PANEL_AGENTS, "Agents", "Monitor distributed runs.", true),
                    panel_meta(PANEL_GIT_OPS, "Git Ops", "Review branches, diffs, and shipping state.", true),
                    panel_meta(PANEL_TERMINAL, "Terminal", "Run terminal actions remotely.", true),
                ],
            },
            crate::protocol::DestinationPanels {
                destination: ShellDestination::Automate,
                panels: vec![
                    panel_meta(PANEL_WORKFLOWS, "Workflows", "Run and inspect workflow automations.", true),
                    panel_meta(PANEL_CHANNELS, "Channels", "Watch connected channels.", true),
                    panel_meta(PANEL_NETWORK, "Network", "Inspect networked execution paths.", true),
                ],
            },
            crate::protocol::DestinationPanels {
                destination: ShellDestination::Assist,
                panels: vec![panel_meta(
                    PANEL_ASSISTANT,
                    "Assistant",
                    "Handle reminders, briefings, and assistant tasks.",
                    true,
                )],
            },
            crate::protocol::DestinationPanels {
                destination: ShellDestination::Observe,
                panels: vec![
                    panel_meta(PANEL_OBSERVE, "Observe", "Approval inbox, runtime, spend, and safety.", true),
                    panel_meta(PANEL_MONITOR, "Monitor", "Runtime health and telemetry.", false),
                    panel_meta(PANEL_LOGS, "Logs", "Inspect runtime and agent logs.", false),
                    panel_meta(PANEL_COSTS, "Costs", "Review cost breakdowns and history.", false),
                    panel_meta(PANEL_LEARNING, "Learning", "Inspect learned preferences and outcomes.", false),
                    panel_meta(PANEL_SHIELD, "Shield", "Review safety posture and privacy controls.", false),
                ],
            },
        ],
        utility_panels: vec![
            utility_panel(PANEL_SETTINGS, "Settings", "Configure Hive providers and runtime defaults."),
            utility_panel(PANEL_MODELS, "Models", "Inspect and select available models."),
            utility_panel(PANEL_ROUTING, "Routing", "Review automatic model routing."),
            utility_panel(PANEL_SKILLS, "Skills", "Inspect installed skills."),
            utility_panel(PANEL_LAUNCH, "Launch", "Token and deployment launch utilities."),
            utility_panel(PANEL_HELP, "Help", "Read docs and troubleshooting help."),
        ],
    }
}

fn panel_meta(id: &str, label: &str, description: &str, supported: bool) -> crate::protocol::PanelMeta {
    crate::protocol::PanelMeta {
        id: id.into(),
        label: label.into(),
        description: description.into(),
        destination: panel_destination(id),
        supported,
        utility: false,
    }
}

fn utility_panel(id: &str, label: &str, description: &str) -> crate::protocol::PanelMeta {
    crate::protocol::PanelMeta {
        id: id.into(),
        label: label.into(),
        description: description.into(),
        destination: None,
        supported: true,
        utility: true,
    }
}

fn timestamp_from_system_time(value: std::time::SystemTime) -> String {
    chrono::DateTime::<Utc>::from(value).to_rfc3339()
}

fn file_breadcrumbs(root: &Path, current: &Path) -> Vec<crate::protocol::FileBreadcrumbData> {
    let root_label = root
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or("Workspace")
        .to_string();
    let mut breadcrumbs = vec![crate::protocol::FileBreadcrumbData {
        label: root_label,
        path: root.display().to_string(),
    }];

    if let Ok(relative) = current.strip_prefix(root) {
        let mut cursor = root.to_path_buf();
        for component in relative.components() {
            cursor.push(component.as_os_str());
            breadcrumbs.push(crate::protocol::FileBreadcrumbData {
                label: component.as_os_str().to_string_lossy().into_owned(),
                path: cursor.display().to_string(),
            });
        }
    }

    breadcrumbs
}

fn spec_sections(spec: &Spec) -> Vec<crate::protocol::SpecSectionData> {
    SpecSection::ALL
        .into_iter()
        .map(|section| crate::protocol::SpecSectionData {
            section: section.label().into(),
            entries: spec
                .sections
                .get(&section)
                .into_iter()
                .flatten()
                .map(|entry| crate::protocol::SpecEntryData {
                    id: entry.id.clone(),
                    title: entry.title.clone(),
                    content: entry.content.clone(),
                    status: spec_status_label(entry.status),
                    checked: entry.checked,
                })
                .collect(),
        })
        .collect()
}

fn spec_status_label(status: hive_agents::specs::SpecStatus) -> String {
    match status {
        hive_agents::specs::SpecStatus::Draft => "draft".into(),
        hive_agents::specs::SpecStatus::Active => "active".into(),
        hive_agents::specs::SpecStatus::Completed => "completed".into(),
        hive_agents::specs::SpecStatus::Archived => "archived".into(),
    }
}

fn agent_run_card(run: &AgentRunSummary) -> crate::protocol::AgentRunCardData {
    crate::protocol::AgentRunCardData {
        run_id: run.run_id.clone(),
        goal: run.goal.clone(),
        status: run.status.clone(),
        detail: run.detail.clone(),
        cost_usd: run.cost_usd,
        elapsed_ms: run.elapsed_ms,
    }
}

fn git_status_label(status: FileStatusType) -> String {
    match status {
        FileStatusType::Modified => "modified".into(),
        FileStatusType::Added => "added".into(),
        FileStatusType::Deleted => "deleted".into(),
        FileStatusType::Renamed => "renamed".into(),
        FileStatusType::Untracked => "untracked".into(),
    }
}

fn truncate_diff(diff: &str) -> String {
    const MAX_DIFF_CHARS: usize = 16_000;
    if diff.len() <= MAX_DIFF_CHARS {
        diff.into()
    } else {
        let mut truncated = diff.chars().take(MAX_DIFF_CHARS).collect::<String>();
        truncated.push_str("\n\n... diff truncated for remote view ...");
        truncated
    }
}

fn workflow_status_label(status: hive_agents::automation::WorkflowStatus) -> String {
    match status {
        hive_agents::automation::WorkflowStatus::Draft => "draft".into(),
        hive_agents::automation::WorkflowStatus::Active => "active".into(),
        hive_agents::automation::WorkflowStatus::Paused => "paused".into(),
        hive_agents::automation::WorkflowStatus::Completed => "completed".into(),
        hive_agents::automation::WorkflowStatus::Failed => "failed".into(),
    }
}

fn workflow_trigger_label(trigger: &TriggerType) -> String {
    match trigger {
        TriggerType::Schedule { .. } => "schedule".into(),
        TriggerType::FileChange { .. } => "file_change".into(),
        TriggerType::WebhookReceived { .. } => "webhook".into(),
        TriggerType::ManualTrigger => "manual".into(),
        TriggerType::OnMessage { .. } => "message".into(),
        TriggerType::OnError { .. } => "error".into(),
    }
}

fn workflow_run_data(run: &WorkflowRunState) -> crate::protocol::WorkflowRunData {
    crate::protocol::WorkflowRunData {
        run_id: run.run_id.clone(),
        workflow_id: run.workflow_id.clone(),
        workflow_name: run.workflow_name.clone(),
        status: run.status.clone(),
        started_at: run.started_at.to_rfc3339(),
        completed_at: run.completed_at.map(|value| value.to_rfc3339()),
        steps_completed: run.steps_completed,
        error: run.error.clone(),
    }
}

fn channel_message_data(message: &ChannelMessage) -> crate::protocol::ChannelMessageData {
    let (author_type, author_label) = match &message.author {
        MessageAuthor::User => ("user".to_string(), "You".to_string()),
        MessageAuthor::Agent { persona } => ("agent".to_string(), persona.clone()),
        MessageAuthor::System => ("system".to_string(), "System".to_string()),
    };
    crate::protocol::ChannelMessageData {
        id: message.id.clone(),
        author_type,
        author_label,
        content: message.content.clone(),
        timestamp: message.timestamp.to_rfc3339(),
        model: message.model.clone(),
        cost: message.cost,
    }
}

fn network_peer_status_label(state: &hive_network::PeerState) -> String {
    match state {
        hive_network::PeerState::Connected => "Connected".into(),
        hive_network::PeerState::Connecting => "Connecting".into(),
        hive_network::PeerState::Discovered => "Discovered".into(),
        hive_network::PeerState::Disconnected => "Disconnected".into(),
        hive_network::PeerState::Banned => "Banned".into(),
    }
}

fn relative_time(timestamp: chrono::DateTime<Utc>) -> String {
    let delta = Utc::now() - timestamp;
    if delta.num_seconds() < 60 {
        "Just now".into()
    } else if delta.num_minutes() < 60 {
        format!("{} min ago", delta.num_minutes())
    } else if delta.num_hours() < 24 {
        format!("{} hr ago", delta.num_hours())
    } else {
        format!("{} d ago", delta.num_days())
    }
}

fn build_assistant_service(
    config_manager: Option<&ConfigManager>,
    connected_account_count: usize,
) -> Result<Option<AssistantService>> {
    let assistant_db_path = hive_core::config::HiveConfig::base_dir()
        .unwrap_or_else(|_| PathBuf::from(".hive"))
        .join("assistant.db");
    let mut assistant = AssistantService::open(&assistant_db_path.display().to_string())
        .map_err(anyhow::Error::msg)?;

    if connected_account_count == 0 {
        return Ok(Some(assistant));
    }

    if let Some(manager) = config_manager {
        if let Some(token) = manager.get_oauth_token(AccountPlatform::Google) {
            assistant.set_gmail_token(token.access_token.clone());
            assistant.set_google_calendar_token(token.access_token);
        }
        if let Some(token) = manager.get_oauth_token(AccountPlatform::Microsoft) {
            assistant.set_outlook_token(token.access_token.clone());
            assistant.set_outlook_calendar_token(token.access_token);
        }
    }

    Ok(Some(assistant))
}

fn list_spec_files(base: &Path) -> Result<Vec<PathBuf>> {
    let specs_dir = base.join("specs");
    if !specs_dir.exists() {
        return Ok(Vec::new());
    }
    let mut files = Vec::new();
    collect_json_files(&specs_dir, &mut files)?;
    Ok(files)
}

fn collect_json_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_json_files(&path, out)?;
        } else if path.extension().is_some_and(|ext| ext == "json") {
            out.push(path);
        }
    }
    Ok(())
}

fn load_spec_file(path: &Path) -> Result<Spec> {
    let content = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str(&content)?)
}

fn remote_message_from_stored(message: StoredMessage) -> crate::protocol::RemoteChatMessage {
    crate::protocol::RemoteChatMessage {
        role: message.role,
        content: message.content,
        timestamp: message.timestamp.to_rfc3339(),
        model: message.model,
        cost: message.cost,
        tokens: message.tokens,
    }
}

fn normalize_model(model: &str, current_model: &str) -> String {
    if model.trim().is_empty() || model == "auto" {
        if current_model.trim().is_empty() {
            "auto".into()
        } else {
            current_model.into()
        }
    } else {
        model.into()
    }
}

fn config_manager_for_daemon(config: &DaemonConfig) -> Result<ConfigManager> {
    if let Some(root) = &config.config_root {
        ConfigManager::new_with_paths(root.join("config.json"), root.join("keys.enc"))
    } else {
        ConfigManager::new()
    }
}

fn configured_provider_types(config: &HiveConfig) -> Vec<hive_ai::types::ProviderType> {
    use hive_ai::types::ProviderType;

    let mut providers = Vec::new();

    if config.anthropic_api_key.as_ref().is_some_and(|key| !key.is_empty()) {
        providers.push(ProviderType::Anthropic);
    }
    if config.openai_api_key.as_ref().is_some_and(|key| !key.is_empty()) {
        providers.push(ProviderType::OpenAI);
    }
    if config.openrouter_api_key.as_ref().is_some_and(|key| !key.is_empty()) {
        providers.push(ProviderType::OpenRouter);
    }
    if config.google_api_key.as_ref().is_some_and(|key| !key.is_empty()) {
        providers.push(ProviderType::Google);
    }
    if config.groq_api_key.as_ref().is_some_and(|key| !key.is_empty()) {
        providers.push(ProviderType::Groq);
    }
    if config.litellm_url.as_ref().is_some_and(|url| !url.is_empty()) {
        providers.push(ProviderType::LiteLLM);
    }
    if config
        .huggingface_api_key
        .as_ref()
        .is_some_and(|key| !key.is_empty())
    {
        providers.push(ProviderType::HuggingFace);
    }
    if !config.ollama_url.trim().is_empty() {
        providers.push(ProviderType::Ollama);
    }
    if !config.lmstudio_url.trim().is_empty() {
        providers.push(ProviderType::LMStudio);
    }
    if config
        .local_provider_url
        .as_ref()
        .is_some_and(|url| !url.is_empty())
    {
        providers.push(ProviderType::GenericLocal);
    }
    if config.xai_api_key.as_ref().is_some_and(|key| !key.is_empty()) {
        providers.push(ProviderType::XAI);
    }
    if config
        .mistral_api_key
        .as_ref()
        .is_some_and(|key| !key.is_empty())
    {
        providers.push(ProviderType::Mistral);
    }
    if config.venice_api_key.as_ref().is_some_and(|key| !key.is_empty()) {
        providers.push(ProviderType::Venice);
    }
    if config.cloud_api_url.as_ref().is_some_and(|url| !url.is_empty()) {
        providers.push(ProviderType::HiveGateway);
    }

    providers.sort_by_key(|provider| provider.to_string());
    providers.dedup();
    providers
}

fn provider_labels(providers: &[hive_ai::types::ProviderType]) -> Vec<String> {
    providers
        .iter()
        .map(|provider| titleize_words(&provider.to_string()))
        .collect()
}

fn provider_credentials(config: &HiveConfig) -> Vec<crate::protocol::ProviderCredentialData> {
    [
        ("anthropic", "Anthropic", config.anthropic_api_key.is_some()),
        ("openai", "OpenAI", config.openai_api_key.is_some()),
        ("openrouter", "OpenRouter", config.openrouter_api_key.is_some()),
        ("google", "Google", config.google_api_key.is_some()),
        ("groq", "Groq", config.groq_api_key.is_some()),
        ("huggingface", "Hugging Face", config.huggingface_api_key.is_some()),
        ("litellm", "LiteLLM", config.litellm_api_key.is_some()),
        ("xai", "xAI", config.xai_api_key.is_some()),
        ("mistral", "Mistral", config.mistral_api_key.is_some()),
        ("venice", "Venice", config.venice_api_key.is_some()),
    ]
    .into_iter()
    .map(|(id, label, has_key)| crate::protocol::ProviderCredentialData {
        id: id.into(),
        label: label.into(),
        has_key,
    })
    .collect()
}

fn titleize_words(value: &str) -> String {
    value.replace('_', " ")
        .split_whitespace()
        .map(|segment| {
            let mut chars = segment.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_ascii_uppercase(), chars.as_str()),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn approval_card(
    request: &ApprovalRequest,
    source: &str,
    action: &PendingAction,
) -> crate::protocol::ApprovalCardData {
    let (title, detail, conversation_id) = match action {
        PendingAction::Chat {
            conversation_id,
            content,
            ..
        } => (
            format!("Approve remote chat action for {}", request.operation_string()),
            content.clone(),
            Some(conversation_id.clone()),
        ),
        PendingAction::Agent {
            goal,
            orchestration_mode,
            ..
        } => (
            format!("Approve {} agent task", orchestration_mode),
            goal.clone(),
            None,
        ),
    };

    crate::protocol::ApprovalCardData {
        id: request.id.clone(),
        source: source.into(),
        title,
        detail,
        severity: approval_severity(request),
        created_at: request.timestamp.to_rfc3339(),
        conversation_id,
    }
}

fn approval_severity(request: &ApprovalRequest) -> String {
    match &request.operation {
        OperationType::FileDelete(_) => "critical".into(),
        OperationType::ShellCommand(cmd) if cmd.contains("deploy") || cmd.contains("push") => {
            "high".into()
        }
        OperationType::FileModify { .. } => "high".into(),
        _ => "medium".into(),
    }
}

fn quality_score(runs: &[AgentRunSummary]) -> f64 {
    let finished = runs
        .iter()
        .filter(|run| matches!(run.status.as_str(), "completed" | "failed" | "denied"))
        .count();
    if finished == 0 {
        return 0.82;
    }
    let completed = runs
        .iter()
        .filter(|run| run.status == "completed")
        .count();
    completed as f64 / finished as f64
}

fn weak_areas(runs: &[AgentRunSummary], cost_summary: &hive_agents::CostSummary) -> Vec<String> {
    let mut weak_areas = Vec::new();
    if runs.iter().any(|run| run.status == "failed") {
        weak_areas.push("Run stability".into());
    }
    if runs.iter().any(|run| run.status == "pending_approval") {
        weak_areas.push("Approval latency".into());
    }
    if cost_summary.total_usd > 2.0 {
        weak_areas.push("Spend discipline".into());
    }
    if weak_areas.is_empty() {
        weak_areas.push("No major weak areas detected".into());
    }
    weak_areas
}

fn build_inbox_items(
    approvals: &[crate::protocol::ApprovalCardData],
    entries: &[ActivityEntry],
) -> Vec<crate::protocol::ObserveInboxItem> {
    let mut items: Vec<crate::protocol::ObserveInboxItem> = approvals
        .iter()
        .map(|approval| crate::protocol::ObserveInboxItem {
            id: approval.id.clone(),
            kind: "approval".into(),
            title: approval.title.clone(),
            detail: approval.detail.clone(),
            tone: approval.severity.clone(),
            timestamp: approval.created_at.clone(),
        })
        .collect();

    for entry in entries {
        if matches!(
            entry.event_type.as_str(),
            "agent_failed" | "task_failed" | "budget_exhausted" | "approval_denied"
        ) {
            items.push(crate::protocol::ObserveInboxItem {
                id: format!("event-{}", entry.id),
                kind: entry.event_type.clone(),
                title: entry.summary.clone(),
                detail: entry
                    .detail_json
                    .clone()
                    .unwrap_or_else(|| entry.summary.clone()),
                tone: if entry.event_type == "approval_denied" {
                    "high".into()
                } else {
                    "action".into()
                },
                timestamp: entry.timestamp.clone(),
            });
        }
    }

    items.sort_by(|left, right| right.timestamp.cmp(&left.timestamp));
    items.truncate(12);
    items
}

fn safety_event_from_entry(entry: &ActivityEntry) -> Option<crate::protocol::ObserveSafetyEvent> {
    let (event_type, severity) = match entry.event_type.as_str() {
        "approval_denied" => ("approval_denied", "high"),
        "agent_failed" => ("agent_failed", "high"),
        "budget_exhausted" => ("budget_exhausted", "medium"),
        "budget_warning" => ("budget_warning", "medium"),
        _ => return None,
    };

    Some(crate::protocol::ObserveSafetyEvent {
        timestamp: entry.timestamp.clone(),
        event_type: event_type.into(),
        severity: severity.into(),
        detail: entry.summary.clone(),
    })
}

fn relative_timestamp_from_elapsed(elapsed_ms: u64) -> String {
    if elapsed_ms == 0 {
        "just now".into()
    } else if elapsed_ms < 60_000 {
        format!("{}s ago", elapsed_ms / 1000)
    } else {
        format!("{}m ago", elapsed_ms / 60_000)
    }
}

fn handoff_title(panel: &str) -> &'static str {
    match panel {
        PANEL_FILES => "Files",
        PANEL_HISTORY => "History",
        PANEL_SPECS => "Specs",
        PANEL_AGENTS => "Agents",
        PANEL_GIT_OPS => "Git Ops",
        PANEL_TERMINAL => "Terminal",
        PANEL_WORKFLOWS => "Workflows",
        PANEL_CHANNELS => "Channels",
        PANEL_NETWORK => "Network",
        PANEL_ASSISTANT => "Assistant",
        PANEL_MONITOR => "Monitor",
        PANEL_LOGS => "Logs",
        PANEL_COSTS => "Costs",
        PANEL_LEARNING => "Learning",
        PANEL_SHIELD => "Shield",
        PANEL_SETTINGS => "Settings",
        PANEL_MODELS => "Models",
        PANEL_ROUTING => "Routing",
        PANEL_SKILLS => "Skills",
        PANEL_LAUNCH => "Launch",
        PANEL_HELP => "Help",
        _ => "Desktop Surface",
    }
}

fn append_model_options(
    models: &mut Vec<crate::protocol::ModelOption>,
    providers: &[hive_ai::types::ProviderType],
    current_model: &str,
) {
    if !current_model.trim().is_empty() && current_model != "auto" {
        models.push(crate::protocol::ModelOption {
            id: current_model.into(),
            label: labelize_model(current_model),
        });
    }

    for provider in providers {
        match provider {
            hive_ai::types::ProviderType::Anthropic => {
                models.push(crate::protocol::ModelOption {
                    id: "claude-sonnet-4-20250514".into(),
                    label: "Claude Sonnet 4".into(),
                });
                models.push(crate::protocol::ModelOption {
                    id: "claude-opus-4-20250514".into(),
                    label: "Claude Opus 4".into(),
                });
            }
            hive_ai::types::ProviderType::OpenAI => {
                models.push(crate::protocol::ModelOption {
                    id: "gpt-4o".into(),
                    label: "GPT-4o".into(),
                });
                models.push(crate::protocol::ModelOption {
                    id: "gpt-4o-mini".into(),
                    label: "GPT-4o mini".into(),
                });
            }
            hive_ai::types::ProviderType::Google => models.push(crate::protocol::ModelOption {
                id: "gemini-2.5-pro".into(),
                label: "Gemini 2.5 Pro".into(),
            }),
            hive_ai::types::ProviderType::Groq => models.push(crate::protocol::ModelOption {
                id: "llama-3.3-70b".into(),
                label: "Groq Llama 3.3 70B".into(),
            }),
            hive_ai::types::ProviderType::OpenRouter => models.push(crate::protocol::ModelOption {
                id: "openrouter/auto".into(),
                label: "OpenRouter Auto".into(),
            }),
            hive_ai::types::ProviderType::Ollama => models.push(crate::protocol::ModelOption {
                id: "ollama/default".into(),
                label: "Ollama Default".into(),
            }),
            hive_ai::types::ProviderType::LMStudio => models.push(crate::protocol::ModelOption {
                id: "lmstudio/default".into(),
                label: "LM Studio Default".into(),
            }),
            hive_ai::types::ProviderType::GenericLocal => {
                models.push(crate::protocol::ModelOption {
                    id: "local/default".into(),
                    label: "Local Provider".into(),
                });
            }
            _ => {}
        }
    }
}

fn dedupe_models(
    models: Vec<crate::protocol::ModelOption>,
) -> Vec<crate::protocol::ModelOption> {
    let mut seen = std::collections::HashSet::new();
    let mut deduped = Vec::new();
    for model in models {
        if seen.insert(model.id.clone()) {
            deduped.push(model);
        }
    }
    deduped
}

fn labelize_model(model: &str) -> String {
    model
        .replace('-', " ")
        .replace('/', " / ")
        .split_whitespace()
        .map(|segment| {
            let mut chars = segment.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_ascii_uppercase(), chars.as_str()),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn build_home_prompt(workspace: &WorkspaceSummary, template_id: &str, detail: &str) -> String {
    let user_focus = if detail.trim().is_empty() {
        "Use your judgment from the current repository state and start with the highest-impact opportunity."
            .to_string()
    } else {
        detail.trim().to_string()
    };

    format!(
        "You are kicking off work on the active project.\n\nProject: {}\nWorkspace root: {}\nMission: {}\nSpecific focus: {}\n\nExecution rules:\n1. Inspect the codebase and current git state before changing anything.\n2. Summarize the relevant context briefly.\n3. Produce a concise impact-ordered execution plan.\n4. Start the first concrete task immediately instead of stopping at analysis.\n5. Keep changes integrated with the existing modules, tabs, and shared services.\n\nMission details:\n{}",
        workspace.name,
        workspace.path,
        home_template_title(template_id),
        user_focus,
        home_template_instruction(template_id),
    )
}

fn home_templates() -> Vec<crate::protocol::HomeTemplateData> {
    vec![
        crate::protocol::HomeTemplateData {
            id: "dogfood".into(),
            title: "Improve This Codebase".into(),
            description: "Find the highest-leverage gaps in the current project and start closing them."
                .into(),
            outcome: "Best when you want Hive to improve the active product.".into(),
        },
        crate::protocol::HomeTemplateData {
            id: "feature".into(),
            title: "Ship A Feature".into(),
            description: "Trace the relevant code, define the change, implement it, and verify the result."
                .into(),
            outcome: "Best when you already know the product outcome you want.".into(),
        },
        crate::protocol::HomeTemplateData {
            id: "bug".into(),
            title: "Fix A Bug".into(),
            description: "Reproduce the problem, isolate root cause, patch it, and confirm the regression is closed."
                .into(),
            outcome: "Best when the project is blocked by a failure or broken workflow.".into(),
        },
        crate::protocol::HomeTemplateData {
            id: "understand".into(),
            title: "Understand The Project".into(),
            description: "Map the architecture, explain how the pieces fit, and identify the real risks."
                .into(),
            outcome: "Best when a human needs a clear read on the codebase before deciding.".into(),
        },
        crate::protocol::HomeTemplateData {
            id: "review".into(),
            title: "Review Current State".into(),
            description: "Inspect the working tree and call out the highest-risk issues and next actions."
                .into(),
            outcome: "Best when you want an informed starting point before more coding.".into(),
        },
    ]
}

fn home_template_title(template_id: &str) -> &'static str {
    match template_id {
        "feature" => "Ship A Feature",
        "bug" => "Fix A Bug",
        "understand" => "Understand The Project",
        "review" => "Review Current State",
        _ => "Improve This Codebase",
    }
}

fn home_template_instruction(template_id: &str) -> &'static str {
    match template_id {
        "feature" => {
            "Trace the feature area, identify the files and interfaces involved, implement the change end-to-end, and verify the result with the right checks."
        }
        "bug" => {
            "Reproduce the failure from repo context, isolate the root cause, implement a precise fix, and verify that the bug is actually closed."
        }
        "understand" => {
            "Build a practical map of the codebase, call out the major modules and dependencies, identify incomplete or risky seams, and recommend the next high-impact work."
        }
        "review" => {
            "Start with repository state and current changes, identify the most important risks or regressions, and recommend the next concrete actions to move the project forward."
        }
        _ => {
            "Treat this as a dogfooding and completion run: find the highest-impact gaps in the product, prioritize what will make the app more integrated and more usable, and begin closing those gaps."
        }
    }
}

fn classify_chat_operation(content: &str) -> Option<OperationType> {
    let lower = content.to_lowercase();
    if lower.contains("git push") || lower.contains("push to origin") {
        Some(OperationType::ShellCommand("git push origin HEAD".into()))
    } else if lower.contains("deploy") || lower.contains("publish") {
        Some(OperationType::ShellCommand("deploy remote release".into()))
    } else if lower.contains("delete ") || lower.contains("remove file") {
        Some(OperationType::FileDelete("workspace selection".into()))
    } else if lower.contains("write file")
        || lower.contains("modify file")
        || lower.contains("apply patch")
        || lower.contains("edit code")
        || lower.contains("refactor")
    {
        Some(OperationType::FileModify {
            path: "workspace".into(),
            scope: "12 files".into(),
        })
    } else {
        None
    }
}

fn classify_agent_operation(goal: &str) -> Option<OperationType> {
    let lower = goal.to_lowercase();
    if lower.contains("deploy") || lower.contains("release") {
        Some(OperationType::ShellCommand("deploy remote release".into()))
    } else if lower.contains("delete") || lower.contains("remove") {
        Some(OperationType::FileDelete("workspace".into()))
    } else if lower.contains("refactor") || lower.contains("apply") || lower.contains("write") {
        Some(OperationType::FileModify {
            path: "workspace".into(),
            scope: "12 files".into(),
        })
    } else {
        None
    }
}

trait ApprovalRequestExt {
    fn operation_string(&self) -> String;
}

impl ApprovalRequestExt for ApprovalRequest {
    fn operation_string(&self) -> String {
        match &self.operation {
            OperationType::ShellCommand(command) => command.clone(),
            OperationType::FileDelete(path) => format!("Delete {path}"),
            OperationType::FileModify { path, scope } => format!("Modify {path} ({scope})"),
            OperationType::GitPush { remote, branch } => format!("Push {branch} to {remote}"),
            OperationType::AiCall {
                model,
                estimated_cost,
            } => {
                format!("AI call on {model} (${estimated_cost:.2})")
            }
            OperationType::Custom(value) => value.clone(),
        }
    }
}
