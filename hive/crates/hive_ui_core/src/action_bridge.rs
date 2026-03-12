//! MCP ↔ GPUI action bridge.
//!
//! Exposes every workspace action as an MCP tool so external agents can drive
//! the UI programmatically.  The bridge uses an `mpsc` channel: MCP tool
//! handlers (which run on arbitrary threads) send [`UiActionRequest`]s, and a
//! main-thread polling loop dispatches them as GPUI actions.

use hive_agents::mcp_client::McpTool;
use serde_json::{json, Value};

// ---------------------------------------------------------------------------
// Channel message
// ---------------------------------------------------------------------------

/// A request sent from an MCP tool handler to the main GPUI thread.
pub struct UiActionRequest {
    pub action_name: String,
    pub params: Value,
    pub response_tx: std::sync::mpsc::Sender<Result<Value, String>>,
}

// ---------------------------------------------------------------------------
// Security gate
// ---------------------------------------------------------------------------

const BLOCKED_ACTIONS: &[&str] = &["quit"];

pub fn is_action_allowed(name: &str) -> bool {
    !BLOCKED_ACTIONS.contains(&name)
}

// ---------------------------------------------------------------------------
// JSON param helpers
// ---------------------------------------------------------------------------

fn string_param(params: &Value, field: &str) -> Result<String, String> {
    params
        .get(field)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| format!("missing required string field: {field}"))
}

fn usize_param(params: &Value, field: &str) -> Result<usize, String> {
    params
        .get(field)
        .and_then(|v| v.as_u64())
        .map(|n| n as usize)
        .ok_or_else(|| format!("missing required integer field: {field}"))
}

fn bool_param(params: &Value, field: &str) -> Result<bool, String> {
    params
        .get(field)
        .and_then(|v| v.as_bool())
        .ok_or_else(|| format!("missing required boolean field: {field}"))
}

fn opt_string_param(params: &Value, field: &str) -> Option<String> {
    params
        .get(field)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

fn schema_for(type_str: &str) -> Value {
    match type_str.trim() {
        "String" => json!({"type": "string"}),
        "usize" => json!({"type": "integer", "minimum": 0}),
        "bool" => json!({"type": "boolean"}),
        s if s.contains("Option") => json!({"type": ["string", "null"]}),
        _ => json!({"type": "string"}),
    }
}

// ---------------------------------------------------------------------------
// Macro
// ---------------------------------------------------------------------------

/// Types are wrapped in `()` so each is a single `tt` token, avoiding
/// ambiguity in nested repetitions.  E.g. `field: (String)` or
/// `field: (Option<String>)`.
macro_rules! register_ui_actions {
    // -- Tool defs -------------------------------------------------------------
    (@tool $name:expr, $desc:expr,) => {
        McpTool {
            name: format!("ui.{}", $name),
            description: $desc.to_string(),
            input_schema: json!({"type": "object", "properties": {}, "required": []}),
        }
    };

    (@tool $name:expr, $desc:expr, $( $field:ident : $fty:tt ),+ ) => {
        McpTool {
            name: format!("ui.{}", $name),
            description: $desc.to_string(),
            input_schema: {
                let mut props = serde_json::Map::new();
                let mut required = Vec::<&str>::new();
                $(
                    props.insert(stringify!($field).to_string(), schema_for(stringify!($fty)));
                    if !stringify!($fty).contains("Option") {
                        required.push(stringify!($field));
                    }
                )+
                json!({"type": "object", "properties": Value::Object(props), "required": required})
            },
        }
    };

    // -- Field extraction sub-rules -------------------------------------------
    (@extract $p:ident, $f:ident, (String))          => { string_param(&$p, stringify!($f))? };
    (@extract $p:ident, $f:ident, (usize))           => { usize_param(&$p, stringify!($f))? };
    (@extract $p:ident, $f:ident, (bool))             => { bool_param(&$p, stringify!($f))? };
    (@extract $p:ident, $f:ident, (Option<String>))  => { opt_string_param(&$p, stringify!($f)) };

    // -- Action construction ---------------------------------------------------
    (@make_one $action:ident, $params:ident) => {{
        let _ = &$params;
        Ok(Box::new(crate::actions::$action) as Box<dyn gpui::Action>)
    }};

    (@make_one $action:ident, $params:ident, $( $field:ident : $fty:tt ),+ ) => {{
        Ok(Box::new(crate::actions::$action {
            $( $field: register_ui_actions!(@extract $params, $field, $fty), )+
        }) as Box<dyn gpui::Action>)
    }};

    // -- Entry point -----------------------------------------------------------
    (
        $(
            $name:expr => $action:ident
                $({ $( $field:ident : $fty:tt ),+ $(,)? })?
            , $desc:expr
        );+ $(;)?
    ) => {
        pub fn make_action(name: &str, params: Value) -> Result<Box<dyn gpui::Action>, String> {
            match name {
                $( $name => {
                    register_ui_actions!(@make_one $action, params $(, $( $field : $fty ),+ )? )
                }),+,
                other => Err(format!("Unknown UI action: {other}")),
            }
        }

        pub fn ui_action_tools() -> Vec<McpTool> {
            vec![$(
                register_ui_actions!(@tool $name, $desc, $( $( $field : $fty ),+ )? )
            ),+]
        }
    };
}

// ---------------------------------------------------------------------------
// Registry — every workspace action
// ---------------------------------------------------------------------------

register_ui_actions! {
    // Chat
    "clear_chat"            => ClearChat,           "Clear the chat history";
    "new_conversation"      => NewConversation,     "Start a new conversation";

    // Panel switches
    "switch_to_chat"        => SwitchToChat,        "Switch to Chat panel";
    "switch_to_quick_start" => SwitchToQuickStart,  "Switch to Quick Start panel";
    "switch_to_history"     => SwitchToHistory,     "Switch to History panel";
    "switch_to_files"       => SwitchToFiles,       "Switch to Files panel";
    "switch_to_kanban"      => SwitchToKanban,      "Switch to Kanban panel";
    "switch_to_monitor"     => SwitchToMonitor,     "Switch to Monitor panel";
    "switch_to_logs"        => SwitchToLogs,        "Switch to Logs panel";
    "switch_to_costs"       => SwitchToCosts,       "Switch to Costs panel";
    "switch_to_review"      => SwitchToReview,      "Switch to Review panel";
    "switch_to_skills"      => SwitchToSkills,      "Switch to Skills panel";
    "switch_to_routing"     => SwitchToRouting,     "Switch to Routing panel";
    "switch_to_models"      => SwitchToModels,      "Switch to Models panel";
    "switch_to_token_launch"=> SwitchToTokenLaunch, "Switch to Token Launch panel";
    "switch_to_specs"       => SwitchToSpecs,       "Switch to Specs panel";
    "switch_to_agents"      => SwitchToAgents,      "Switch to Agents panel";
    "switch_to_learning"    => SwitchToLearning,    "Switch to Learning panel";
    "switch_to_shield"      => SwitchToShield,      "Switch to Shield panel";
    "switch_to_assistant"   => SwitchToAssistant,   "Switch to Assistant panel";
    "switch_to_settings"    => SwitchToSettings,    "Switch to Settings panel";
    "switch_to_network"     => SwitchToNetwork,     "Switch to Network panel";
    "switch_to_terminal"    => SwitchToTerminal,    "Switch to Terminal panel";
    "switch_to_help"        => SwitchToHelp,        "Switch to Help panel";
    "switch_to_workflows"   => SwitchToWorkflows,   "Switch to Workflows panel";
    "switch_to_channels"    => SwitchToChannels,    "Switch to Channels panel";

    // Files panel
    "open_workspace_directory" => OpenWorkspaceDirectory, "Open the workspace directory in the OS file manager";
    "files_navigate_back"   => FilesNavigateBack,   "Navigate back in the Files panel";
    "files_refresh"         => FilesRefresh,         "Refresh the Files panel";
    "files_new_file"        => FilesNewFile,         "Create a new file";
    "files_new_folder"      => FilesNewFolder,       "Create a new folder";
    "files_close_viewer"    => FilesCloseViewer,     "Close the file viewer";
    "files_navigate_to"     => FilesNavigateTo { path: (String) }, "Navigate to a directory path";
    "files_open_entry"      => FilesOpenEntry { name: (String), is_directory: (bool) }, "Open a file or directory entry";
    "files_delete_entry"    => FilesDeleteEntry { name: (String) }, "Delete a file entry";

    // History panel
    "history_refresh"       => HistoryRefresh,       "Refresh conversation history";
    "history_clear_all"     => HistoryClearAll,      "Clear all conversation history";
    "history_clear_all_confirm" => HistoryClearAllConfirm, "Confirm clearing all history";
    "history_clear_all_cancel"  => HistoryClearAllCancel,  "Cancel clearing all history";
    "history_load_conversation" => HistoryLoadConversation { conversation_id: (String) }, "Load a conversation by ID";
    "history_delete_conversation" => HistoryDeleteConversation { conversation_id: (String) }, "Delete a conversation by ID";

    // Kanban
    "kanban_add_task"       => KanbanAddTask,        "Add a new Kanban task";

    // Logs
    "logs_clear"            => LogsClear,            "Clear log output";
    "logs_toggle_auto_scroll" => LogsToggleAutoScroll, "Toggle auto-scroll in logs";
    "logs_set_filter"       => LogsSetFilter { level: (String) }, "Set the log filter level";

    // Costs
    "costs_export_csv"      => CostsExportCsv,       "Export costs as CSV";
    "costs_reset_today"     => CostsResetToday,       "Reset today's costs";
    "costs_clear_history"   => CostsClearHistory,     "Clear cost history";

    // Review / Git Ops
    "review_stage_all"      => ReviewStageAll,        "Stage all changes";
    "review_unstage_all"    => ReviewUnstageAll,      "Unstage all changes";
    "review_commit"         => ReviewCommit,           "Commit staged changes";
    "review_discard_all"    => ReviewDiscardAll,       "Discard all changes";
    "review_ai_commit_message" => ReviewAiCommitMessage, "Generate AI commit message";
    "review_commit_with_message" => ReviewCommitWithMessage, "Commit with current message";
    "review_push"           => ReviewPush,             "Push to remote";
    "review_push_set_upstream" => ReviewPushSetUpstream, "Push and set upstream";
    "review_pr_refresh"     => ReviewPrRefresh,        "Refresh PR status";
    "review_pr_ai_generate" => ReviewPrAiGenerate,     "AI-generate PR description";
    "review_pr_create"      => ReviewPrCreate,         "Create a pull request";
    "review_branch_refresh" => ReviewBranchRefresh,    "Refresh branch list";
    "review_branch_create"  => ReviewBranchCreate,     "Create a new branch";
    "review_lfs_refresh"    => ReviewLfsRefresh,       "Refresh LFS status";
    "review_lfs_track"      => ReviewLfsTrack,         "Track files with LFS";
    "review_lfs_untrack"    => ReviewLfsUntrack,       "Untrack files from LFS";
    "review_lfs_pull"       => ReviewLfsPull,          "Pull LFS objects";
    "review_lfs_push"       => ReviewLfsPush,          "Push LFS objects";
    "review_gitflow_init"   => ReviewGitflowInit,      "Initialize gitflow";
    "review_switch_tab"     => ReviewSwitchTab { tab: (String) }, "Switch to a review tab";
    "review_set_commit_message" => ReviewSetCommitMessage { message: (String) }, "Set commit message text";
    "review_branch_switch"  => ReviewBranchSwitch { branch_name: (String) }, "Switch to a branch";
    "review_branch_delete_named" => ReviewBranchDeleteNamed { branch_name: (String) }, "Delete a branch by name";
    "review_branch_set_name" => ReviewBranchSetName { name: (String) }, "Set new branch name";
    "review_pr_set_title"   => ReviewPrSetTitle { title: (String) }, "Set PR title";
    "review_pr_set_body"    => ReviewPrSetBody { body: (String) }, "Set PR body";
    "review_pr_set_base"    => ReviewPrSetBase { base: (String) }, "Set PR base branch";
    "review_gitflow_start"  => ReviewGitflowStart { kind: (String), name: (String) }, "Start a gitflow branch";
    "review_gitflow_finish_named" => ReviewGitflowFinishNamed { kind: (String), name: (String) }, "Finish a gitflow branch";
    "review_gitflow_set_name" => ReviewGitflowSetName { name: (String) }, "Set gitflow branch name";
    "review_lfs_set_pattern" => ReviewLfsSetPattern { pattern: (String) }, "Set LFS track pattern";

    // Skills
    "skills_refresh"        => SkillsRefresh,          "Refresh skills list";
    "skills_clear_search"   => SkillsClearSearch,      "Clear skills search query";
    "skills_install"        => SkillsInstall { skill_id: (String) }, "Install a skill by ID";
    "skills_remove"         => SkillsRemove { skill_id: (String) }, "Remove a skill by ID";
    "skills_toggle"         => SkillsToggle { skill_id: (String) }, "Toggle a skill on/off";
    "skills_create"         => SkillsCreate { name: (String), description: (String), instructions: (String) }, "Create a custom skill";
    "skills_add_source"     => SkillsAddSource { url: (String), name: (String) }, "Add a skill source";
    "skills_remove_source"  => SkillsRemoveSource { url: (String) }, "Remove a skill source";
    "skills_set_tab"        => SkillsSetTab { tab: (String) }, "Switch skills panel tab";
    "skills_set_search"     => SkillsSetSearch { query: (String) }, "Set skills search query";
    "skills_set_category"   => SkillsSetCategory { category: (String) }, "Set skills category filter";

    // Plugin import
    "plugin_import_open"    => PluginImportOpen,       "Open plugin import dialog";
    "plugin_import_cancel"  => PluginImportCancel,     "Cancel plugin import";
    "plugin_import_confirm" => PluginImportConfirm,    "Confirm plugin import";
    "plugin_import_from_github" => PluginImportFromGitHub { owner_repo: (String) }, "Import plugin from GitHub (owner/repo)";
    "plugin_import_from_url" => PluginImportFromUrl { url: (String) }, "Import plugin from URL";
    "plugin_import_from_local" => PluginImportFromLocal { path: (String) }, "Import plugin from local path";
    "plugin_import_toggle_skill" => PluginImportToggleSkill { index: (usize) }, "Toggle skill in import preview";
    "plugin_remove"         => PluginRemove { plugin_id: (String) }, "Remove an installed plugin";
    "plugin_update"         => PluginUpdate { plugin_id: (String) }, "Update a plugin to latest version";
    "plugin_toggle_expand"  => PluginToggleExpand { plugin_id: (String) }, "Toggle plugin group expand/collapse";
    "plugin_toggle_skill"   => PluginToggleSkill { plugin_id: (String), skill_name: (String) }, "Toggle a skill within a plugin";

    // Routing
    "routing_add_rule"      => RoutingAddRule,          "Add a routing rule";

    // Token Launch
    "token_launch_create_wallet" => TokenLaunchCreateWallet, "Create a new wallet";
    "token_launch_import_wallet" => TokenLaunchImportWallet, "Import an existing wallet";
    "token_launch_deploy"   => TokenLaunchDeploy,       "Deploy the token";
    "token_launch_save_rpc_config" => TokenLaunchSaveRpcConfig, "Save RPC configuration";
    "token_launch_reset_rpc_config" => TokenLaunchResetRpcConfig, "Reset RPC configuration";
    "token_launch_set_step" => TokenLaunchSetStep { step: (usize) }, "Set the wizard step";
    "token_launch_select_chain" => TokenLaunchSelectChain { chain: (String) }, "Select a blockchain";
    "token_launch_select_wallet" => TokenLaunchSelectWallet { wallet_id: (String) }, "Select a wallet";

    // Settings
    "settings_save"         => SettingsSave,            "Save settings";
    "export_config"         => ExportConfig,            "Export configuration";
    "import_config"         => ImportConfig,            "Import configuration";
    "theme_changed"         => ThemeChanged { theme_name: (String) }, "Switch the active theme";

    // Monitor
    "monitor_refresh"       => MonitorRefresh,          "Refresh monitor panel";

    // Network
    "network_refresh"       => NetworkRefresh,          "Refresh network panel";

    // Terminal
    "terminal_clear"        => TerminalClear,           "Clear terminal output";
    "terminal_submit_command" => TerminalSubmitCommand,  "Submit terminal command";
    "terminal_kill"         => TerminalKill,             "Kill terminal process";
    "terminal_restart"      => TerminalRestart,          "Restart terminal";

    // Tool approval
    "tool_approve"          => ToolApprove,              "Approve a pending tool call";
    "tool_reject"           => ToolReject,               "Reject a pending tool call";

    // Agents
    "agents_refresh_remote_agents" => AgentsRefreshRemoteAgents, "Refresh remote agent list";
    "agents_reload_workflows" => AgentsReloadWorkflows,   "Reload workflows";
    "agents_run_workflow"   => AgentsRunWorkflow { workflow_id: (String), instruction: (String), source: (String), source_id: (String) }, "Run a workflow";
    "agents_select_remote_agent" => AgentsSelectRemoteAgent { agent_name: (String) }, "Select a remote agent";
    "agents_select_remote_skill" => AgentsSelectRemoteSkill { agent_name: (String), skill_id: (Option<String>) }, "Select a remote agent skill";
    "agents_discover_remote_agent" => AgentsDiscoverRemoteAgent { agent_name: (String) }, "Discover a remote agent";
    "agents_run_remote_agent" => AgentsRunRemoteAgent { agent_name: (String), prompt: (String), skill_id: (Option<String>) }, "Run a remote agent";

    // Workflow builder
    "workflow_builder_save" => WorkflowBuilderSave,      "Save workflow";
    "workflow_builder_run"  => WorkflowBuilderRun,       "Run workflow";
    "workflow_builder_delete_node" => WorkflowBuilderDeleteNode, "Delete a workflow node";
    "workflow_builder_load_workflow" => WorkflowBuilderLoadWorkflow { workflow_id: (String) }, "Load a workflow";

    // Quick Start
    "quick_start_select_template" => QuickStartSelectTemplate { template_id: (String) }, "Select a Quick Start template";
    "quick_start_open_panel" => QuickStartOpenPanel { panel: (String) }, "Open a panel from Quick Start";
    "quick_start_run_project" => QuickStartRunProject { template_id: (String), detail: (String) }, "Run a Quick Start project";

    // Channels
    "channel_select"        => ChannelSelect { channel_id: (String) }, "Select a channel";

    // Account
    "account_connect_platform" => AccountConnectPlatform { platform: (String) }, "Connect an account platform";
    "account_disconnect_platform" => AccountDisconnectPlatform { platform: (String) }, "Disconnect an account platform";

    // Auto-update
    "trigger_app_update"    => TriggerAppUpdate,         "Trigger an application update"
}
