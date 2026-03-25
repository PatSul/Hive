use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Events exchanged between the daemon and remote clients.
///
/// Existing event variants remain intact; new variants extend the contract so
/// the web client can drive the shell, Home, Observe, and approval workflows.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DaemonEvent {
    // Client -> Daemon
    SendMessage {
        conversation_id: String,
        content: String,
        model: String,
    },
    SwitchPanel {
        panel: String,
    },
    SwitchDestination {
        destination: ShellDestination,
    },
    SetModel {
        model: String,
    },
    SetObserveView {
        view: ObserveView,
    },
    SwitchWorkspace {
        workspace_path: String,
    },
    LaunchHomeMission {
        template_id: String,
        detail: String,
    },
    ResumeConversation {
        conversation_id: String,
    },
    ApprovalDecision {
        request_id: String,
        approved: bool,
        reason: Option<String>,
    },
    StartAgentTask {
        goal: String,
        orchestration_mode: String,
    },
    CancelAgentTask {
        run_id: String,
    },
    ResponseFeedback {
        message_id: String,
        positive: bool,
    },

    // Daemon -> Client
    StreamChunk {
        conversation_id: String,
        chunk: String,
    },
    StreamComplete {
        conversation_id: String,
        prompt_tokens: u32,
        completion_tokens: u32,
        cost_usd: Option<f64>,
    },
    AgentStatus {
        run_id: String,
        status: String,
        detail: String,
    },
    StateSnapshot(SessionSnapshot),
    PanelData {
        panel: String,
        data: serde_json::Value,
    },
    Error {
        code: u16,
        message: String,
    },

    // Bidirectional
    Ping,
    Pong,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ShellDestination {
    Home,
    Build,
    Automate,
    Assist,
    Observe,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ObserveView {
    Inbox,
    Runtime,
    Spend,
    Safety,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceSummary {
    pub name: String,
    pub path: String,
    pub is_current: bool,
    pub is_pinned: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PanelMeta {
    pub id: String,
    pub label: String,
    pub description: String,
    pub destination: Option<ShellDestination>,
    pub supported: bool,
    pub utility: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DestinationPanels {
    pub destination: ShellDestination,
    pub panels: Vec<PanelMeta>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PanelRegistry {
    pub destinations: Vec<DestinationPanels>,
    pub utility_panels: Vec<PanelMeta>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSnapshot {
    pub active_conversation: Option<String>,
    pub active_destination: ShellDestination,
    pub active_panel: String,
    pub current_workspace: WorkspaceSummary,
    pub current_model: String,
    pub pending_approval_count: usize,
    pub is_streaming: bool,
    pub observe_view: ObserveView,
    pub panel_registry: PanelRegistry,
    pub agent_runs: Vec<AgentRunSummary>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentRunSummary {
    pub run_id: String,
    pub goal: String,
    pub status: String,
    pub detail: String,
    pub cost_usd: f64,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HomeTemplateData {
    pub id: String,
    pub title: String,
    pub description: String,
    pub outcome: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HomePriorityCardData {
    pub eyebrow: String,
    pub title: String,
    pub detail: String,
    pub action_label: String,
    pub action_panel: String,
    pub tone: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HomeStatusCardData {
    pub title: String,
    pub value: String,
    pub detail: String,
    pub tone: String,
    pub action_label: Option<String>,
    pub action_panel: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HomeNextStepData {
    pub title: String,
    pub detail: String,
    pub action_label: String,
    pub action_panel: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ApprovalCardData {
    pub id: String,
    pub source: String,
    pub title: String,
    pub detail: String,
    pub severity: String,
    pub created_at: String,
    pub conversation_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HomePanelData {
    pub project_name: String,
    pub project_root: String,
    pub project_summary: String,
    pub current_model: String,
    pub pending_approval_count: usize,
    pub launch_ready: bool,
    pub launch_hint: String,
    pub last_launch_status: Option<String>,
    pub templates: Vec<HomeTemplateData>,
    pub priorities: Vec<HomePriorityCardData>,
    pub status_cards: Vec<HomeStatusCardData>,
    pub next_steps: Vec<HomeNextStepData>,
    pub saved_workspaces: Vec<WorkspaceSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ObserveInboxItem {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub detail: String,
    pub tone: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ObserveAgentRow {
    pub role: String,
    pub status: String,
    pub phase: String,
    pub model: String,
    pub started_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ObserveRunRow {
    pub id: String,
    pub summary: String,
    pub status: String,
    pub started_at: String,
    pub cost_usd: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ObserveRuntimeData {
    pub status_label: String,
    pub active_agents: usize,
    pub active_streams: usize,
    pub online_providers: usize,
    pub total_providers: usize,
    pub request_queue_length: usize,
    pub current_run_id: Option<String>,
    pub agents: Vec<ObserveAgentRow>,
    pub recent_runs: Vec<ObserveRunRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ObserveSpendData {
    pub total_cost_usd: f64,
    pub today_cost_usd: f64,
    pub quality_score: f64,
    pub quality_trend: String,
    pub cost_efficiency: f64,
    pub best_model: Option<String>,
    pub worst_model: Option<String>,
    pub weak_areas: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ObserveSafetyEvent {
    pub timestamp: String,
    pub event_type: String,
    pub severity: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ObserveSafetyData {
    pub shield_enabled: bool,
    pub pii_detections: usize,
    pub secrets_blocked: usize,
    pub threats_caught: usize,
    pub recent_events: Vec<ObserveSafetyEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ObservePanelData {
    pub current_view: ObserveView,
    pub inbox: Vec<ObserveInboxItem>,
    pub approvals: Vec<ApprovalCardData>,
    pub runtime: ObserveRuntimeData,
    pub spend: ObserveSpendData,
    pub safety: ObserveSafetyData,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelOption {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConversationSummaryData {
    pub id: String,
    pub title: String,
    pub preview: String,
    pub message_count: usize,
    pub total_cost: f64,
    pub model: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RemoteChatMessage {
    pub role: String,
    pub content: String,
    pub timestamp: String,
    pub model: Option<String>,
    pub cost: Option<f64>,
    pub tokens: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatPanelData {
    pub conversation_id: Option<String>,
    pub current_model: String,
    pub is_streaming: bool,
    pub total_cost: f64,
    pub messages: Vec<RemoteChatMessage>,
    pub conversations: Vec<ConversationSummaryData>,
    pub available_models: Vec<ModelOption>,
    pub pending_approvals: Vec<ApprovalCardData>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HistoryPanelData {
    pub active_conversation: Option<String>,
    pub conversations: Vec<ConversationSummaryData>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileBreadcrumbData {
    pub label: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileEntryData {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FilePreviewData {
    pub path: String,
    pub content: String,
    pub size: u64,
    pub modified: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FilesPanelData {
    pub workspace_root: String,
    pub current_path: String,
    pub breadcrumbs: Vec<FileBreadcrumbData>,
    pub entries: Vec<FileEntryData>,
    pub preview: Option<FilePreviewData>,
    pub preview_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SpecEntryData {
    pub id: String,
    pub title: String,
    pub content: String,
    pub status: String,
    pub checked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SpecSectionData {
    pub section: String,
    pub entries: Vec<SpecEntryData>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SpecSummaryData {
    pub id: String,
    pub path: String,
    pub title: String,
    pub description: String,
    pub status: String,
    pub domain: Option<String>,
    pub updated_at: String,
    pub completion_pct: f32,
    pub entry_count: usize,
    pub checked_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SpecDetailData {
    pub id: String,
    pub path: String,
    pub title: String,
    pub description: String,
    pub status: String,
    pub domain: Option<String>,
    pub updated_at: String,
    pub version: u32,
    pub completion_pct: f32,
    pub sections: Vec<SpecSectionData>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SpecsPanelData {
    pub workspace_root: String,
    pub selected_spec_id: Option<String>,
    pub specs: Vec<SpecSummaryData>,
    pub selected_spec: Option<SpecDetailData>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentModeData {
    pub id: String,
    pub label: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentRunCardData {
    pub run_id: String,
    pub goal: String,
    pub status: String,
    pub detail: String,
    pub cost_usd: f64,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentsPanelData {
    pub current_model: String,
    pub active_runs: Vec<AgentRunCardData>,
    pub recent_runs: Vec<AgentRunCardData>,
    pub pending_approvals: Vec<ApprovalCardData>,
    pub orchestration_modes: Vec<AgentModeData>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GitFileData {
    pub path: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GitCommitData {
    pub hash: String,
    pub short_hash: String,
    pub message: String,
    pub author: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GitOpsPanelData {
    pub repo_path: String,
    pub is_repo: bool,
    pub branch: Option<String>,
    pub dirty_count: usize,
    pub files: Vec<GitFileData>,
    pub commits: Vec<GitCommitData>,
    pub diff: String,
    pub can_commit: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TerminalLineData {
    pub stream: String,
    pub content: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TerminalPanelData {
    pub cwd: String,
    pub is_running: bool,
    pub last_exit_code: Option<i32>,
    pub lines: Vec<TerminalLineData>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowSummaryData {
    pub id: String,
    pub name: String,
    pub description: String,
    pub status: String,
    pub trigger: String,
    pub step_count: usize,
    pub run_count: u32,
    pub last_run: Option<String>,
    pub is_builtin: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowRunData {
    pub run_id: String,
    pub workflow_id: String,
    pub workflow_name: String,
    pub status: String,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub steps_completed: usize,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowsPanelData {
    pub workspace_root: String,
    pub source_dir: String,
    pub workflows: Vec<WorkflowSummaryData>,
    pub active_runs: Vec<WorkflowRunData>,
    pub recent_runs: Vec<WorkflowRunData>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChannelSummaryData {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub description: String,
    pub assigned_agents: Vec<String>,
    pub message_count: usize,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChannelMessageData {
    pub id: String,
    pub author_type: String,
    pub author_label: String,
    pub content: String,
    pub timestamp: String,
    pub model: Option<String>,
    pub cost: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChannelDetailData {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub description: String,
    pub assigned_agents: Vec<String>,
    pub pinned_files: Vec<String>,
    pub messages: Vec<ChannelMessageData>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChannelsPanelData {
    pub current_model: String,
    pub selected_channel_id: Option<String>,
    pub channels: Vec<ChannelSummaryData>,
    pub selected_channel: Option<ChannelDetailData>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NetworkPeerData {
    pub name: String,
    pub status: String,
    pub address: String,
    pub latency_ms: Option<u64>,
    pub last_seen: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NetworkPanelData {
    pub available: bool,
    pub our_peer_id: String,
    pub connected_count: usize,
    pub total_count: usize,
    pub peers: Vec<NetworkPeerData>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AssistantBriefingData {
    pub greeting: String,
    pub date: String,
    pub event_count: usize,
    pub unread_emails: usize,
    pub active_reminders: usize,
    pub top_priority: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AssistantEventData {
    pub title: String,
    pub time: String,
    pub location: Option<String>,
    pub is_conflict: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AssistantEmailPreviewData {
    pub from: String,
    pub subject: String,
    pub snippet: String,
    pub time: String,
    pub important: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AssistantEmailGroupData {
    pub provider: String,
    pub previews: Vec<AssistantEmailPreviewData>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AssistantReminderData {
    pub title: String,
    pub due: String,
    pub is_overdue: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AssistantApprovalData {
    pub id: String,
    pub action: String,
    pub resource: String,
    pub level: String,
    pub requested_by: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AssistantRecentActionData {
    pub description: String,
    pub timestamp: String,
    pub action_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AssistantPanelData {
    pub connected_account_count: usize,
    pub briefing: Option<AssistantBriefingData>,
    pub events: Vec<AssistantEventData>,
    pub email_groups: Vec<AssistantEmailGroupData>,
    pub reminders: Vec<AssistantReminderData>,
    pub approvals: Vec<AssistantApprovalData>,
    pub recent_actions: Vec<AssistantRecentActionData>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SettingsPanelData {
    pub current_workspace: String,
    pub theme: String,
    pub privacy_mode: bool,
    pub shield_enabled: bool,
    pub notifications_enabled: bool,
    pub auto_update: bool,
    pub remote_enabled: bool,
    pub remote_auto_start: bool,
    pub remote_local_port: u16,
    pub remote_web_port: u16,
    pub ollama_url: String,
    pub lmstudio_url: String,
    pub litellm_url: Option<String>,
    pub local_provider_url: Option<String>,
    pub connected_account_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelsPanelData {
    pub current_model: String,
    pub default_model: String,
    pub auto_routing: bool,
    pub project_models: Vec<String>,
    pub available_models: Vec<ModelOption>,
    pub available_providers: Vec<String>,
    pub configured_providers: Vec<String>,
    pub provider_credentials: Vec<ProviderCredentialData>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RoutingPanelData {
    pub auto_routing: bool,
    pub default_model: String,
    pub strategy_summary: String,
    pub project_models: Vec<String>,
    pub available_providers: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderCredentialData {
    pub id: String,
    pub label: String,
    pub has_key: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SkillSummaryData {
    pub name: String,
    pub description: String,
    pub source: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SkillsPanelData {
    pub skills_dir: String,
    pub total_skills: usize,
    pub enabled_skills: usize,
    pub builtin_skills: usize,
    pub community_skills: usize,
    pub custom_skills: usize,
    pub skills: Vec<SkillSummaryData>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LaunchPanelData {
    pub remote_enabled: bool,
    pub remote_auto_start: bool,
    pub local_api_port: u16,
    pub web_port: u16,
    pub local_api_url: String,
    pub web_url: String,
    pub cloud_api_url: Option<String>,
    pub cloud_relay_url: Option<String>,
    pub cloud_tier: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HelpLinkData {
    pub title: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HelpPanelData {
    pub version: String,
    pub docs: Vec<HelpLinkData>,
    pub quick_tips: Vec<String>,
    pub troubleshooting: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HandoffPanelData {
    pub panel: String,
    pub title: String,
    pub description: String,
    pub action_label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PanelPayload {
    Home(HomePanelData),
    Observe(ObservePanelData),
    Chat(ChatPanelData),
    History(HistoryPanelData),
    Files(FilesPanelData),
    Specs(SpecsPanelData),
    Agents(AgentsPanelData),
    GitOps(GitOpsPanelData),
    Terminal(TerminalPanelData),
    Workflows(WorkflowsPanelData),
    Channels(ChannelsPanelData),
    Network(NetworkPanelData),
    Assistant(AssistantPanelData),
    Settings(SettingsPanelData),
    Models(ModelsPanelData),
    Routing(RoutingPanelData),
    Skills(SkillsPanelData),
    Launch(LaunchPanelData),
    Help(HelpPanelData),
    Handoff(HandoffPanelData),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PanelResponse {
    pub panel: String,
    pub data: PanelPayload,
}
