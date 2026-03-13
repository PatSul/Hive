use gpui::*;

// ---------------------------------------------------------------------------
// Zero-sized actions
// ---------------------------------------------------------------------------

actions!(
    hive_workspace,
    [
        ClearChat,
        NewConversation,
        // Panel switch actions
        SwitchToChat,
        SwitchToQuickStart,
        SwitchToHistory,
        SwitchToFiles,
        SwitchToKanban,
        SwitchToMonitor,
        SwitchToLogs,
        SwitchToCosts,
        SwitchToReview,
        SwitchToSkills,
        SwitchToRouting,
        SwitchToModels,
        SwitchToTokenLaunch,
        SwitchToSpecs,
        SwitchToAgents,
        SwitchToLearning,
        SwitchToShield,
        SwitchToAssistant,
        SwitchToSettings,
        SwitchToNetwork,
        SwitchToTerminal,
        SwitchToHelp,
        OpenWorkspaceDirectory,
        ToggleProjectDropdown,
        // Files panel
        FilesNavigateBack,
        FilesRefresh,
        FilesNewFile,
        FilesNewFolder,
        FilesCloseViewer,
        // History panel
        HistoryRefresh,
        HistoryClearAll,
        HistoryClearAllConfirm,
        HistoryClearAllCancel,
        // Kanban panel
        KanbanAddTask,
        // Logs panel
        LogsClear,
        LogsToggleAutoScroll,
        // Costs panel
        CostsExportCsv,
        CostsResetToday,
        CostsClearHistory,
        // Review panel
        ReviewStageAll,
        ReviewUnstageAll,
        ReviewCommit,
        ReviewDiscardAll,
        // Git Ops — expanded review panel
        ReviewAiCommitMessage,
        ReviewCommitWithMessage,
        ReviewPush,
        ReviewPushSetUpstream,
        ReviewPrRefresh,
        ReviewPrAiGenerate,
        ReviewPrCreate,
        ReviewBranchRefresh,
        ReviewBranchCreate,
        ReviewLfsRefresh,
        ReviewLfsTrack,
        ReviewLfsUntrack,
        ReviewLfsPull,
        ReviewLfsPush,
        ReviewGitflowInit,
        // Skills panel
        SkillsRefresh,
        SkillsClearSearch,
        // Plugin import actions
        PluginImportOpen,
        PluginImportCancel,
        PluginImportConfirm,
        // Routing panel
        RoutingAddRule,
        // Token Launch panel
        TokenLaunchCreateWallet,
        TokenLaunchImportWallet,
        TokenLaunchDeploy,
        TokenLaunchSaveRpcConfig,
        TokenLaunchResetRpcConfig,
        // Settings panel
        SettingsSave,
        ExportConfig,
        ImportConfig,
        // Monitor panel
        MonitorRefresh,
        // Network panel
        NetworkRefresh,
        // Terminal panel
        TerminalClear,
        TerminalSubmitCommand,
        TerminalKill,
        TerminalRestart,
        // Tool approval
        ToolApprove,
        ToolReject,
        // Agents panel
        AgentsRefreshRemoteAgents,
        AgentsReloadWorkflows,
        // Panel switch — new panels
        SwitchToWorkflows,
        SwitchToChannels,
        // Workflow builder
        WorkflowBuilderSave,
        WorkflowBuilderRun,
        WorkflowBuilderDeleteNode,
        // Auto-update
        TriggerAppUpdate,
    ]
);

// ---------------------------------------------------------------------------
// Data-carrying actions
// ---------------------------------------------------------------------------

/// Navigate to a specific directory in the Files panel.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct FilesNavigateTo {
    pub path: String,
}

/// Open a file by path.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct FilesOpenEntry {
    pub name: String,
    pub is_directory: bool,
}

/// Delete a file entry.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct FilesDeleteEntry {
    pub name: String,
}

/// Load a conversation by ID in the History panel.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct HistoryLoadConversation {
    pub conversation_id: String,
}

/// Delete a conversation by ID.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct HistoryDeleteConversation {
    pub conversation_id: String,
}

/// Set log filter level.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct LogsSetFilter {
    pub level: String,
}

/// Token Launch wizard: advance or go back a step.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct TokenLaunchSetStep {
    pub step: usize,
}

/// Token Launch: select a chain.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct TokenLaunchSelectChain {
    pub chain: String,
}

/// Token Launch: select one of the saved wallets for the active chain.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct TokenLaunchSelectWallet {
    pub wallet_id: String,
}

/// Load a specific workflow into the visual builder canvas.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct WorkflowBuilderLoadWorkflow {
    pub workflow_id: String,
}

/// Select a channel in the Channels panel.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct ChannelSelect {
    pub channel_id: String,
}

/// Initiate an OAuth connection for a specific platform.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct AccountConnectPlatform {
    pub platform: String,
}

/// Disconnect a connected account.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct AccountDisconnectPlatform {
    pub platform: String,
}

/// Run a specific automation workflow by ID from the Agents panel.
///
/// `instruction` is optional free-form text describing the task for this run.
/// When provided, the workflow runtime will be planned against that instruction
/// before execution.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct AgentsRunWorkflow {
    pub workflow_id: String,
    pub instruction: String,
    pub source: String,
    pub source_id: String,
}

/// Select the active remote A2A agent in the Agents panel.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct AgentsSelectRemoteAgent {
    pub agent_name: String,
}

/// Select or clear the skill used for the currently selected remote A2A agent.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct AgentsSelectRemoteSkill {
    pub agent_name: String,
    pub skill_id: Option<String>,
}

/// Discover the configured remote A2A agent and cache its agent card.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct AgentsDiscoverRemoteAgent {
    pub agent_name: String,
}

/// Run a prompt against a configured remote A2A agent.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct AgentsRunRemoteAgent {
    pub agent_name: String,
    pub prompt: String,
    pub skill_id: Option<String>,
}

/// Select the active Quick Start template.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct QuickStartSelectTemplate {
    pub template_id: String,
}

/// Open a panel from the Quick Start setup or next-step cards.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct QuickStartOpenPanel {
    pub panel: String,
}

/// Launch a guided Quick Start run for the active project.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct QuickStartRunProject {
    pub template_id: String,
    pub detail: String,
}

/// Switch to a specific tab within the Git Ops panel.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct ReviewSwitchTab {
    pub tab: String,
}

/// Set the commit message text.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct ReviewSetCommitMessage {
    pub message: String,
}

/// Switch to a specific branch.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct ReviewBranchSwitch {
    pub branch_name: String,
}

/// Delete a specific branch by name.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct ReviewBranchDeleteNamed {
    pub branch_name: String,
}

/// Set the new branch name input.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct ReviewBranchSetName {
    pub name: String,
}

/// Set PR title.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct ReviewPrSetTitle {
    pub title: String,
}

/// Set PR body.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct ReviewPrSetBody {
    pub body: String,
}

/// Set PR base branch.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct ReviewPrSetBase {
    pub base: String,
}

/// Start a gitflow feature/release/hotfix.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct ReviewGitflowStart {
    pub kind: String,
    pub name: String,
}

/// Finish a gitflow feature/release/hotfix.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct ReviewGitflowFinishNamed {
    pub kind: String,
    pub name: String,
}

/// Set gitflow new name input.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct ReviewGitflowSetName {
    pub name: String,
}

/// Set LFS track pattern input.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct ReviewLfsSetPattern {
    pub pattern: String,
}

// ---------------------------------------------------------------------------
// Skills / ClawdHub actions
// ---------------------------------------------------------------------------

/// Install a skill from the directory by its ID.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct SkillsInstall {
    pub skill_id: String,
}

/// Remove an installed skill by its ID.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct SkillsRemove {
    pub skill_id: String,
}

/// Toggle a skill between enabled/disabled by its ID.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct SkillsToggle {
    pub skill_id: String,
}

/// Switch the active theme by name.
///
/// Dispatched from the Settings theme picker. The workspace handler resolves
/// the name against built-in and custom themes and updates `self.theme` plus
/// the `AppTheme` global.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct ThemeChanged {
    pub theme_name: String,
}

/// Create a new custom skill from the Create tab form.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct SkillsCreate {
    pub name: String,
    pub description: String,
    pub instructions: String,
}

/// Add a remote skill source by URL.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct SkillsAddSource {
    pub url: String,
    pub name: String,
}

/// Remove a skill source by URL.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct SkillsRemoveSource {
    pub url: String,
}

/// Switch the active tab in the Skills panel.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct SkillsSetTab {
    pub tab: String,
}

/// Update the search query in the Skills panel.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct SkillsSetSearch {
    pub query: String,
}

/// Set the active category filter in the Skills directory.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct SkillsSetCategory {
    pub category: String,
}

// ---------------------------------------------------------------------------
// Plugin import actions
// ---------------------------------------------------------------------------

/// Import a plugin from a GitHub repository (owner/repo format).
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct PluginImportFromGitHub {
    pub owner_repo: String,
}

/// Import a plugin from a URL.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct PluginImportFromUrl {
    pub url: String,
}

/// Import a plugin from a local path.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct PluginImportFromLocal {
    pub path: String,
}

/// Toggle a skill checkbox in the import preview.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct PluginImportToggleSkill {
    pub index: usize,
}

/// Remove an installed plugin by ID.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct PluginRemove {
    pub plugin_id: String,
}

/// Update an installed plugin to latest version.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct PluginUpdate {
    pub plugin_id: String,
}

/// Toggle expand/collapse of an installed plugin group.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct PluginToggleExpand {
    pub plugin_id: String,
}

/// Toggle a skill within an installed plugin.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct PluginToggleSkill {
    pub plugin_id: String,
    pub skill_name: String,
}

// ---------------------------------------------------------------------------
// Project quick-switcher actions
// ---------------------------------------------------------------------------

/// Switch to a workspace by path.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct SwitchToWorkspace {
    pub path: String,
}

/// Toggle pin/unpin state for a workspace.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct TogglePinWorkspace {
    pub path: String,
}

/// Remove a workspace from the recent list.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct RemoveRecentWorkspace {
    pub path: String,
}
