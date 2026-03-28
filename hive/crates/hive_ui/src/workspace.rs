use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::input::{InputEvent, InputState};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{error, info, warn};

use hive_agents::plugin_manager::PluginManager;
use hive_agents::plugin_types::{PluginPreview, PluginSource};
use hive_ai::providers::AiProvider;
use hive_ai::types::ChatRequest;
use hive_core::config::HiveConfig;
use hive_core::notifications::{AppNotification, NotificationType};
use hive_core::session::SessionState;
use hive_terminal::{InteractiveShell, ShellOutput};

use crate::chat_input::{ChatInputView, SubmitMessage};
use crate::chat_service::{ChatService, MessageRole, StreamCompleted};
use chrono::Utc;
use hive_ui_core::{
    // Globals
    AppA2aClient,
    AppActivityService,
    AppAgentNotifications,
    AppAiService,
    AppApprovalGate,
    AppAssistant,
    AppAutomation,
    AppAws,
    AppAzure,
    AppBitbucket,
    AppBrowser,
    AppChannels,
    AppConfig,
    AppContextEngine,
    AppContextSelection,
    AppCortexStatus,
    AppDatabase,
    AppDocker,
    AppDocsIndexer,
    AppFleetLearning,
    AppGcp,
    AppGitLab,
    AppHiveMemory,
    AppAirweave,
    AppHueClient,
    AppIntegrationDb,
    AppKnowledge,
    AppKnowledgeFiles,
    AppKubernetes,
    AppLearning,
    AppMarketplace,
    AppMcpServer,
    AppMessaging,
    AppNetwork,
    AppNotifications,
    AppOllamaManager,
    AppPersonas,
    AppProjectManagement,
    AppQuickIndex,
    AppRagService,
    AppReminderRx,
    AppRpcConfig,
    AppSecurity,
    AppSemanticSearch,
    AppShield,
    AppSkillManager,
    AppSpecs,
    AppTheme,
    AppTts,
    AppUpdater,
    AppVoiceAssistant,
    AppWallets,
    ContextSelectionState,
    // Types
    HiveTheme,
    Panel,
    ShellDestination,
    Sidebar,
};
// Re-export actions so hive_app can import from hive_ui::workspace::*
use crate::statusbar::{ConnectivityDisplay, StatusBar};
use crate::titlebar::Titlebar;
pub use hive_ui_core::{
    AccountConnectPlatform, AccountDisconnectPlatform, ActivityApprove, ActivityDeny,
    ActivityExpandEvent, ActivityExportCsv, ActivityRefresh, ActivitySetFilter,
    ActivitySetView,
    AgentsDiscoverRemoteAgent, AgentsRefreshRemoteAgents, AgentsReloadWorkflows,
    AgentsRunRemoteAgent, AgentsRunWorkflow, AgentsSelectRemoteAgent, AgentsSelectRemoteSkill,
    AppPluginManager, ApplyAllEdits, ApplyCodeBlock, ChannelSelect, CheckCalendar, CheckEmail,
    ChatReadAloud, ClearChat,
    ContextFormatChanged, CopyFullPrompt, CopyToClipboard, CostsClearHistory, CostsExportCsv,
    CostsResetToday, DailyBriefing, ExportConfig, ExportPrompt, FilesClearChecked, FilesCloseViewer,
    FilesDeleteEntry, FilesNavigateBack, FilesNavigateTo, FilesNewFile, FilesNewFolder,
    FilesOpenEntry, FilesRefresh, FilesSetSearchQuery, FilesToggleCheck, HistoryClearAll,
    HistoryClearAllCancel, HistoryClearAllConfirm, HistoryDeleteConversation,
    HistoryLoadConversation, HistoryRefresh, ImportConfig, KanbanAddTask, LogsClear, LogsSetFilter,
    LogsToggleAutoScroll, MonitorRefresh, NetworkRefresh, NewConversation, OllamaDeleteModel,
    OllamaPullModel, OpenWorkspaceDirectory, PluginImportCancel, PluginImportConfirm,
    PluginImportFromGitHub, PluginImportFromLocal, PluginImportFromUrl, PluginImportOpen,
    PluginImportToggleSkill, PluginRemove, PluginToggleExpand, PluginToggleSkill, PluginUpdate,
    PromptLibraryDelete, PromptLibraryLoad, PromptLibraryRefresh, PromptLibrarySaveCurrent,
    QuickStartOpenPanel, QuickStartRunProject, QuickStartSelectTemplate, RemoveRecentWorkspace,
    ReviewAiCommitMessage, ReviewBranchCreate, ReviewBranchDeleteNamed, ReviewBranchRefresh,
    ReviewBranchSetName, ReviewBranchSwitch, ReviewCommit, ReviewCommitWithMessage,
    ReviewDiscardAll, ReviewGitflowFinishNamed, ReviewGitflowInit, ReviewGitflowSetName,
    ReviewGitflowStart, ReviewLfsPull, ReviewLfsPush, ReviewLfsRefresh, ReviewLfsSetPattern,
    ReviewLfsTrack, ReviewLfsUntrack, ReviewPrAiGenerate, ReviewPrCreate, ReviewPrRefresh,
    ReviewPrSetBase, ReviewPrSetBody, ReviewPrSetTitle, ReviewPush, ReviewPushSetUpstream,
    ReviewSetCommitMessage, ReviewStageAll, ReviewSwitchTab, ReviewUnstageAll, RoutingAddRule,
    SettingsSave, SkillsAddSource, SkillsClearSearch, SkillsCreate, SkillsInstall, SkillsRefresh,
    SkillsRemove, SkillsRemoveSource, SkillsSetCategory, SkillsSetSearch, SkillsSetTab,
    SkillsToggle, SwitchToActivity, SwitchToAgents, SwitchToAssistant, SwitchToChannels,
    SwitchToChat, SwitchToCodeMap, SwitchToCosts, SwitchToFiles, SwitchToHelp, SwitchToHistory,
    SwitchToKanban, SwitchToLearning, SwitchToLogs, SwitchToModels, SwitchToMonitor,
    SwitchToNetwork, SwitchToPromptLibrary, SwitchToQuickStart, SwitchToReview, SwitchToRouting,
    SwitchToSettings, SwitchToShield, SwitchToSkills, SwitchToSpecs, SwitchToTerminal,
    SwitchToTokenLaunch, SwitchToWorkflows, SwitchToWorkspace, TerminalClear, TerminalKill,
    TerminalRestart, TerminalSubmitCommand, ThemeChanged, ToggleCommandPalette,
    ToggleDisclosure, TogglePinWorkspace, ToggleProjectDropdown, TokenLaunchCreateWallet,
    TokenLaunchDeploy, TokenLaunchImportWallet,
    TokenLaunchResetRpcConfig, TokenLaunchSaveRpcConfig, TokenLaunchSelectChain,
    TokenLaunchSelectWallet, TokenLaunchSetStep, ToolApprove, ToolReject, TriggerAppUpdate,
    VoiceProcessText, WorkflowBuilderDeleteNode, WorkflowBuilderLoadWorkflow, WorkflowBuilderRun,
    WorkflowBuilderSave,
};
use hive_ui_panels::panels::chat::{DisplayMessage, ToolCallDisplay};
use hive_ui_panels::panels::{
    agents::{AgentsPanel, AgentsPanelData},
    activity::{ActivityData, ActivityPanel, ObserveView, ObserveRuntimeData, ObserveAgentRow, ObserveRunRow, ObserveSafetyData, ObserveSafetyEvent, ObserveSpendData},
    assistant::{AssistantPanel, AssistantPanelData},
    channels::{ChannelCreated, ChannelMessageSent, ChannelsView},
    chat::{CachedChatData, ChatPanel},
    costs::{CostData, CostsPanel},
    files::{FilesData, FilesPanel},
    help::HelpPanel,
    history::{HistoryData, HistoryPanel},
    kanban::{KanbanData, KanbanPanel},
    learning::{LearningPanel, LearningPanelData},
    logs::{LogsData, LogsPanel},
    models_browser::{ModelsBrowserView, ProjectModelsChanged},
    monitor::{MonitorData, MonitorPanel, SystemResources},
    network::{NetworkPanel, NetworkPeerData, PeerDisplayInfo},
    quick_start::{QuickStartPanel, QuickStartPanelData, QuickStartTone},
    review::{
        AiCommitState, BranchEntry, GitOpsTab, LfsFileEntry, PrForm, PrSummary, ReviewData,
        ReviewPanel,
    },
    routing::{RoutingData, RoutingPanel},
    settings::{SettingsSaved, SettingsView},
    shield::{ShieldConfigChanged, ShieldPanelData, ShieldView},
    skills::{SkillsData, SkillsPanel},
    specs::{SpecPanelData, SpecsPanel},
    terminal::{TerminalData, TerminalPanel},
    token_launch::{TokenLaunchData, TokenLaunchInputs, TokenLaunchPanel},
    workflow_builder::{WorkflowBuilderView, WorkflowRunRequested, WorkflowSaved},
};

mod activity_actions;
mod account_actions;
mod agents_actions;
mod assistant_actions;
mod approval_actions;
mod assistant_refresh;
mod chat_actions;
mod chrome;
mod context_rail;
mod costs_actions;
mod data_refresh;
mod file_actions;
mod history_actions;
mod kanban_actions;
mod logs_actions;
mod monitor_actions;
mod navigation;
mod network_actions;
mod overlays;
mod panel_router;
mod plugin_actions;
mod prompt_library_actions;
mod project_context;
mod quick_start_actions;
mod review_actions;
mod routing_actions;
mod shield_actions;
mod sidebar_shell;
mod skills_actions;
mod settings_actions;
mod status_sync;
mod terminal_host;
mod token_launch_actions;
mod token_launch_support;
mod utility_actions;
mod workflow_actions;
mod workflow_planning;

// ---------------------------------------------------------------------------
// Workspace
// ---------------------------------------------------------------------------

/// Async helper: query HiveMemory + KnowledgeHub off the UI thread and inject
/// the results as system messages into a [`ChatRequest`].  Falls back silently
/// if either service is unavailable or returns an error.
async fn enrich_request_with_memory(
    request: &mut ChatRequest,
    hive_mem: &Option<std::sync::Arc<tokio::sync::Mutex<hive_ai::memory::HiveMemory>>>,
    knowledge_hub: &Option<std::sync::Arc<hive_integrations::knowledge::KnowledgeHub>>,
    query_text: &str,
) {
    let mut extra_context = String::new();
    let mut memory_ctx = String::new();

    // Query HiveMemory (LanceDB vector store)
    if let Some(ref hm) = *hive_mem {
        let mem = hm.lock().await;
        if let Ok(result) = mem.query(query_text, 5).await {
            for chunk in &result.chunks {
                extra_context.push_str(&format!(
                    "// From {}\n{}\n\n",
                    chunk.source_file, chunk.content
                ));
            }
            for mem_result in &result.memories {
                memory_ctx.push_str(&format!(
                    "- {} (importance: {:.0}, category: {})\n",
                    mem_result.content, mem_result.importance, mem_result.category
                ));
            }
        }
    }

    // Query KnowledgeHub (Obsidian, Notion, etc.)
    if let Some(ref kb) = *knowledge_hub {
        let kb_context = kb.get_context_all(query_text).await;
        if !kb_context.trim().is_empty() {
            extra_context.push_str("# Knowledge Base Context\n\n");
            extra_context.push_str(&kb_context);
            extra_context.push_str("\n\n");
        }
    }

    // Inject memory context as system messages
    if !memory_ctx.is_empty() {
        request.messages.insert(
            0,
            hive_ai::types::ChatMessage {
                role: hive_ai::types::MessageRole::System,
                content: format!("## Recalled Memories\n\n{}", memory_ctx),
                timestamp: chrono::Utc::now(),
                tool_call_id: None,
                tool_calls: None,
            },
        );
    }
    if !extra_context.is_empty() {
        request.messages.insert(
            0,
            hive_ai::types::ChatMessage {
                role: hive_ai::types::MessageRole::System,
                content: format!("## Additional Context\n\n{}", extra_context),
                timestamp: chrono::Utc::now(),
                tool_call_id: None,
                tool_calls: None,
            },
        );
    }
}

/// Root workspace layout: titlebar + sidebar + content + statusbar + chat input.
///
/// Commands sent from the UI thread to the background shell task.
enum TerminalCmd {
    /// Write a command string to shell stdin (newline appended automatically).
    Write(String),
    /// Kill the running shell process.
    Kill,
}

/// Owns the `Entity<ChatService>` and orchestrates the flow between the chat
/// input, AI service, and panel rendering.
pub struct HiveWorkspace {
    theme: HiveTheme,
    sidebar: Sidebar,
    status_bar: StatusBar,
    current_project_root: PathBuf,
    current_project_name: String,
    chat_input: Entity<ChatInputView>,
    quick_start_goal_input: Entity<InputState>,
    command_palette_input: Entity<InputState>,
    agents_remote_prompt_input: Entity<InputState>,
    chat_service: Entity<ChatService>,
    settings_view: Entity<SettingsView>,
    shield_view: Entity<ShieldView>,
    models_browser_view: Entity<ModelsBrowserView>,
    workflow_builder_view: Entity<WorkflowBuilderView>,
    channels_view: Entity<ChannelsView>,
    /// Focus handle for the workspace root div. Ensures that `dispatch_action`
    /// from child panels (Files, History, etc.) can bubble up to the root
    /// div's `.on_action()` handlers even when no input element is focused.
    focus_handle: FocusHandle,
    history_data: HistoryData,
    files_data: FilesData,
    code_map_data: hive_ui_panels::panels::code_map::CodeMapData,
    prompt_library_data: hive_ui_panels::panels::prompt_library::PromptLibraryData,
    quick_start_data: QuickStartPanelData,
    kanban_data: KanbanData,
    monitor_data: MonitorData,
    logs_data: LogsData,
    review_data: ReviewData,
    cost_data: CostData,
    routing_data: RoutingData,
    skills_data: SkillsData,
    token_launch_data: TokenLaunchData,
    token_launch_inputs: TokenLaunchInputs,
    specs_data: SpecPanelData,
    agents_data: AgentsPanelData,
    activity_data: ActivityData,
    shield_data: ShieldPanelData,
    learning_data: LearningPanelData,
    assistant_data: AssistantPanelData,
    network_peer_data: NetworkPeerData,
    terminal_data: TerminalData,
    terminal_input: Entity<InputState>,
    /// Channel to send commands to the background shell task.
    terminal_cmd_tx: Option<tokio::sync::mpsc::UnboundedSender<TerminalCmd>>,
    /// Background task running the interactive shell read loop.
    _terminal_task: Option<Task<()>>,
    /// In-flight stream spawn task (kept alive to prevent cancellation).
    _stream_task: Option<Task<()>>,
    /// Tracks whether session state needs to be persisted. Avoids writing
    /// `session.json` on every render frame -- only writes when state actually
    /// changed (panel switch, conversation load, stream finalization).
    session_dirty: bool,
    /// The conversation ID at the time of the last session save. Used to
    /// detect when a new conversation was auto-saved by `finalize_stream`.
    last_saved_conversation_id: Option<String>,
    /// Cached display data for the chat panel. Rebuilt only when the
    /// `ChatService` generation counter changes, avoiding per-frame string
    /// cloning and enabling markdown parse caching.
    cached_chat_data: CachedChatData,
    /// Timestamp of the last discovery scan (for 30s cadence).
    last_discovery_scan: Option<std::time::Instant>,
    /// Whether a discovery scan is currently in-flight.
    discovery_scan_pending: bool,
    /// Set to `true` by the background scan thread when done.
    discovery_done_flag: Option<Arc<std::sync::atomic::AtomicBool>>,
    /// Timestamp of the last network peer refresh (for 30s cadence).
    last_network_refresh: Option<std::time::Instant>,
    /// Recently used workspace roots, persisted to session and shown in the titlebar.
    recent_workspace_roots: Vec<PathBuf>,
    pinned_workspace_roots: Vec<PathBuf>,
    show_project_dropdown: bool,
    show_command_palette: bool,
    show_utility_drawer: bool,
    /// Last observed window size (width, height) in logical pixels.
    /// Updated on each render frame so `save_session` can persist it without
    /// needing a `&Window` reference.
    last_window_size: Option<[u32; 2]>,
    /// Holds the backend `PluginPreview` + `PluginSource` while the user is
    /// reviewing the import preview screen. Consumed by confirm handler.
    pending_plugin_preview: Option<(PluginPreview, PluginSource)>,
    /// File watcher for incremental RAG indexing. Dropped on project switch.
    _file_watcher: Option<hive_fs::FileWatcher>,
    /// Completed swarm task trees. Appended after `/swarm` execution and shown
    /// in the monitor panel's background tasks section alongside active runs.
    swarm_task_trees: Vec<hive_ui_panels::components::task_tree::TaskTreeState>,
    /// Active toast notifications shown as an overlay in the top-right corner.
    /// Each entry holds (notification_id, summary, toast_kind, created_at).
    toast_messages: Vec<(
        String,
        String,
        hive_ui_panels::components::toast::ToastKind,
        std::time::Instant,
    )>,
    /// IDs of notifications already surfaced as toasts, preventing duplicates.
    seen_notification_ids: HashSet<String>,
    /// Cached QuickIndex for the current project, available for ContextEngine.
    quick_index: Option<Arc<hive_ai::quick_index::QuickIndex>>,
    /// Background task rebuilding the QuickIndex after a project switch.
    _quick_index_task: Option<Task<()>>,
    /// Background task loading startup data (files, prompts, knowledge).
    _bootstrap_task: Option<Task<()>>,
}

const MAX_RECENT_WORKSPACES: usize = 8;
const MAX_PINNED_WORKSPACES: usize = 20;

struct StartupProjectSnapshot {
    files_data: FilesData,
    prompt_library_data: hive_ui_panels::panels::prompt_library::PromptLibraryData,
    knowledge_sources: Vec<hive_ai::knowledge_files::KnowledgeSource>,
}

impl HiveWorkspace {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        // Resolve and publish the theme BEFORE creating child views so they
        // can read AppTheme from the global during their constructors.
        let theme = {
            let theme_name = if cx.has_global::<AppConfig>() {
                cx.global::<AppConfig>().0.get().theme.clone()
            } else {
                "HiveCode Dark".to_string()
            };
            project_context::resolve_theme_by_name(&theme_name)
        };
        cx.set_global(AppTheme(theme.clone()));
        project_context::sync_gpui_theme(&theme, cx);

        // Read default model from config if available.
        let default_model = if cx.has_global::<AppConfig>() {
            cx.global::<AppConfig>().0.get().default_model.clone()
        } else {
            String::new()
        };

        let chat_service = cx.new(|_| ChatService::new(default_model.clone()));

        // Observe chat service — re-render whenever streaming state changes.
        cx.observe(&chat_service, |_this, _svc, cx| {
            cx.notify();
        })
        .detach();

        // Subscribe to stream completion events for learning instrumentation.
        cx.subscribe(&chat_service, |_this, svc, event: &StreamCompleted, cx| {
            if cx.has_global::<AppLearning>() {
                let learning = &cx.global::<AppLearning>().0;
                let record = hive_learn::OutcomeRecord {
                    conversation_id: svc.read(cx).conversation_id.clone().unwrap_or_default(),
                    message_id: uuid::Uuid::new_v4().to_string(),
                    model_id: event.model.clone(),
                    task_type: "chat".into(),
                    tier: "standard".into(),
                    persona: None,
                    outcome: hive_learn::Outcome::Accepted,
                    edit_distance: None,
                    follow_up_count: 0,
                    quality_score: 0.8, // default; refined by future edits/regeneration
                    cost: event.cost.unwrap_or(0.0),
                    latency_ms: 0,
                    timestamp: chrono::Utc::now().to_rfc3339(),
                };
                if let Err(e) = learning.on_outcome(&record) {
                    tracing::warn!("Learning: failed to record outcome: {e}");
                }
            }

            // Fleet learning: record pattern and update instance metrics.
            if cx.has_global::<AppFleetLearning>() {
                if let Ok(mut fleet) = cx.global::<AppFleetLearning>().0.lock() {
                    fleet.record_pattern(
                        hive_ai::fleet_learning::PatternType::PromptPattern,
                        &event.model,
                        0.8,
                    );
                    let tokens_total = event.tokens.map(|(i, o)| (i + o) as u64).unwrap_or(0);
                    fleet.update_instance_metrics(
                        "local",
                        1,
                        event.cost.unwrap_or(0.0),
                        tokens_total,
                    );
                }
            }

            // Context compaction: if the conversation has grown past the
            // compaction threshold, summarize older messages with a budget
            // AI call and replace them with the summary.
            if svc.read(cx).needs_compaction() {
                let compaction_msgs = svc.read(cx).messages_for_compaction();
                if !compaction_msgs.is_empty() {
                    let compaction_msgs_for_apply = compaction_msgs.clone();
                    let prompt = format!(
                        "Summarize the following conversation excerpt concisely. \
                         Preserve key facts, decisions, and user preferences. \
                         For each durable fact, prefix the line with one of: \
                         Preference: / Decision: / Pattern: / Fact:\n\n{}",
                        compaction_msgs
                            .iter()
                            .map(|m| format!("[{}]: {}", m.role, m.content))
                            .collect::<Vec<_>>()
                            .join("\n\n")
                    );
                    let summarize_messages = vec![hive_ai::types::ChatMessage {
                        role: hive_ai::types::MessageRole::User,
                        content: prompt,
                        timestamp: chrono::Utc::now(),
                        tool_call_id: None,
                        tool_calls: None,
                    }];

                    if cx.has_global::<AppAiService>() {
                        let provider = cx.global::<AppAiService>().0.first_provider();
                        if let Some(provider) = provider {
                            let svc_weak = svc.downgrade();
                            let request = hive_ai::types::ChatRequest {
                                messages: summarize_messages,
                                model: "auto".into(),
                                max_tokens: 1024_u32,
                                temperature: Some(0.3_f32),
                                system_prompt: None,
                                tools: None,
                                cache_system_prompt: false,
                            };
                            // Capture globals for fact extraction in the async block.
                            let learning_for_facts = cx.has_global::<AppLearning>()
                                .then(|| cx.global::<AppLearning>().0.clone());
                            let memory_for_facts = cx.has_global::<hive_ui_core::AppCollectiveMemory>()
                                .then(|| Arc::clone(&cx.global::<hive_ui_core::AppCollectiveMemory>().0));

                            cx.spawn(async move |_this, app: &mut gpui::AsyncApp| {
                                match provider.chat(&request).await {
                                    Ok(response) => {
                                        let summary = response.content.clone();
                                        let _ = svc_weak.update(app, |svc, _cx| {
                                            svc.apply_compaction(
                                                response.content,
                                                &compaction_msgs_for_apply,
                                            );
                                        });

                                        // Extract durable facts and persist them.
                                        let facts = hive_ai::extract_facts(&summary);
                                        for fact in &facts {
                                            match fact.category {
                                                hive_ai::FactCategory::Preference => {
                                                    if let Some(ref learning) = learning_for_facts {
                                                        let _ = learning.preference_model.observe(
                                                            &fact.content,
                                                            &fact.content,
                                                            0.7,
                                                        );
                                                    }
                                                }
                                                hive_ai::FactCategory::CodePattern => {
                                                    if let Some(ref mem) = memory_for_facts {
                                                        let entry = hive_agents::collective_memory::MemoryEntry::new(
                                                            hive_agents::collective_memory::MemoryCategory::CodePattern,
                                                            &fact.content,
                                                        );
                                                        let _ = mem.remember(&entry);
                                                    }
                                                }
                                                hive_ai::FactCategory::Decision | hive_ai::FactCategory::Fact => {
                                                    if let Some(ref mem) = memory_for_facts {
                                                        let entry = hive_agents::collective_memory::MemoryEntry::new(
                                                            hive_agents::collective_memory::MemoryCategory::General,
                                                            &fact.content,
                                                        );
                                                        let _ = mem.remember(&entry);
                                                    }
                                                }
                                            }
                                        }
                                        if !facts.is_empty() {
                                            tracing::info!(
                                                "Extracted {} facts from compaction summary",
                                                facts.len(),
                                            );
                                        }
                                    }
                                    Err(e) => {
                                        tracing::warn!(
                                            "Context compaction summarize failed: {e}"
                                        );
                                    }
                                }
                            })
                            .detach();
                        }
                    }
                }
            }
        })
        .detach();

        // Phase 3: Run memory maintenance on workspace startup.
        if cx.has_global::<hive_ui_core::AppCollectiveMemory>() {
            let mem = &cx.global::<hive_ui_core::AppCollectiveMemory>().0;
            match mem.maintenance(0.98, 0.1, 0.85) {
                Ok(report) => {
                    if report.decayed > 0 || report.pruned > 0 || report.deduplicated > 0 {
                        tracing::info!(
                            "Memory maintenance: {} decayed, {} pruned, {} deduplicated",
                            report.decayed, report.pruned, report.deduplicated
                        );
                    }
                }
                Err(e) => tracing::warn!("Memory maintenance failed: {e}"),
            }
        }

        // Build initial status bar from config + providers.
        let mut status_bar = StatusBar::new();
        if cx.has_global::<AppConfig>() {
            let config = cx.global::<AppConfig>().0.get();
            status_bar.current_model = if config.default_model.is_empty() {
                "Select Model".to_string()
            } else {
                config.default_model.clone()
            };
            status_bar.privacy_mode = config.privacy_mode;
        }
        if cx.has_global::<AppAiService>() {
            let providers = cx.global::<AppAiService>().0.available_providers();
            status_bar.connectivity = if providers.is_empty() {
                ConnectivityDisplay::Offline
            } else {
                ConnectivityDisplay::Online
            };
        }
        if cx.has_global::<AppCortexStatus>() {
            let cortex = cx.global::<AppCortexStatus>();
            status_bar.cortex_state = cortex.state.clone();
            status_bar.cortex_changes_applied = cortex.changes_applied;
            status_bar.cortex_auto_apply_enabled = cortex.auto_apply_enabled;
        }

        // -- Session recovery: restore last conversation + panel ----------------
        let session = SessionState::load().unwrap_or_default();
        let mut restored_panel = Panel::Chat;

        if let Some(ref conv_id) = session.active_conversation_id {
            let load_result = chat_service.update(cx, |svc, _cx| svc.load_conversation(conv_id));
            match load_result {
                Ok(()) => {
                    info!("Session recovery: loaded conversation {conv_id}");
                    restored_panel = Panel::from_stored(&session.active_panel);
                }
                Err(e) => {
                    warn!("Session recovery: failed to load conversation {conv_id}: {e}");
                    // Start fresh -- don't propagate the error.
                }
            }
        } else if !session.active_panel.is_empty() {
            // No conversation to restore, but the user may have been on a
            // non-Chat panel (e.g. Settings, Files).
            restored_panel = Panel::from_stored(&session.active_panel);
        }

        let mut sidebar = Sidebar::new();
        sidebar.active_panel = restored_panel;
        if let Some(destination) = restored_panel.shell_destination() {
            sidebar.active_destination = destination;
        }

        let project_root = project_context::resolve_project_root_from_session(&session);
        let recent_workspace_roots =
            project_context::load_recent_workspace_roots(&session, &project_root);
        let pinned_workspace_roots = project_context::load_pinned_workspace_roots(&session);
        let project_name = project_context::project_name_from_path(&project_root);
        let files_data = FilesData {
            current_path: project_root.clone(),
            entries: Vec::new(),
            search_query: String::new(),
            selected_file: None,
            breadcrumbs: Vec::new(),
            viewed_file_content: None,
            viewed_file_path: None,
            viewed_file_language: String::new(),
            viewed_file_size: 0,
            checked_files: HashSet::new(),
            semantic_results: Vec::new(),
        };
        let code_map_data = hive_ui_panels::panels::code_map::CodeMapData::default();
        let prompt_library_data =
            hive_ui_panels::panels::prompt_library::PromptLibraryData::default();
        status_bar.active_project = format!("{} [{}]", project_name, project_root.display());

        // Initialize context selection state (files checked in Files panel).
        cx.set_global(AppContextSelection(std::sync::Arc::new(
            std::sync::Mutex::new(ContextSelectionState::default()),
        )));

        // Create the interactive chat input entity.
        let chat_input = cx.new(|cx| ChatInputView::new(window, cx));

        // When the user submits a message, feed it into the send flow.
        cx.subscribe_in(
            &chat_input,
            window,
            |this, _view, event: &SubmitMessage, window, cx| {
                chat_actions::handle_send_text(
                    this,
                    event.text.clone(),
                    event.context_files.clone(),
                    window,
                    cx,
                );
            },
        )
        .detach();

        let quick_start_goal_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_placeholder(
                "Describe what Hive should improve, complete, fix, or ship in this project",
                window,
                cx,
            );
            state
        });

        let command_palette_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_placeholder("Jump to a panel, workspace, or Home mission", window, cx);
            state
        });

        cx.subscribe_in(
            &command_palette_input,
            window,
            |this, _view, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { .. } => {
                    overlays::handle_command_palette_submit(this, window, cx);
                }
                InputEvent::Change => cx.notify(),
                _ => {}
            },
        )
        .detach();

        let agents_remote_prompt_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_placeholder(
                "Ask the selected remote agent to review code, summarize docs, or handle a focused task",
                window,
                cx,
            );
            state
        });

        let terminal_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_placeholder("Type a command and press Enter...", window, cx);
            state
        });

        cx.subscribe_in(
            &terminal_input,
            window,
            |this, _view, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    terminal_host::handle_terminal_submit(
                        this,
                        &TerminalSubmitCommand,
                        window,
                        cx,
                    );
                }
            },
        )
        .detach();

        // Create the interactive settings view entity.
        let settings_view = cx.new(|cx| SettingsView::new(window, cx));

        // When settings are saved, persist to AppConfig.
        cx.subscribe_in(
            &settings_view,
            window,
            |this, _view, _event: &SettingsSaved, _window, cx| {
                settings_actions::handle_settings_save_from_view(this, cx);
            },
        )
        .detach();

        // Create the interactive shield view entity.
        let shield_view = cx.new(|cx| ShieldView::new(window, cx));

        // When shield config changes, persist to AppConfig and rebuild the shield.
        cx.subscribe_in(
            &shield_view,
            window,
            |this, _view, _event: &ShieldConfigChanged, _window, cx| {
                shield_actions::handle_shield_config_save(this, cx);
            },
        )
        .detach();

        // Create the model browser view entity.
        let project_models = if cx.has_global::<AppConfig>() {
            cx.global::<AppConfig>().0.get().project_models.clone()
        } else {
            Vec::new()
        };
        let models_browser_view = cx.new(|cx| ModelsBrowserView::new(project_models, window, cx));

        // Eagerly push API keys to the models browser so it can show static
        // registry entries even before the user explicitly switches to the panel.
        if cx.has_global::<AppConfig>() {
            let cfg = cx.global::<AppConfig>().0.get();
            let mut providers = HashSet::new();
            if cfg.openrouter_api_key.is_some() {
                providers.insert(hive_ai::types::ProviderType::OpenRouter);
            }
            if cfg.openai_api_key.is_some() {
                providers.insert(hive_ai::types::ProviderType::OpenAI);
            }
            if cfg.anthropic_api_key.is_some() {
                providers.insert(hive_ai::types::ProviderType::Anthropic);
            }
            if cfg.google_api_key.is_some() {
                providers.insert(hive_ai::types::ProviderType::Google);
            }
            if cfg.xai_api_key.is_some() {
                providers.insert(hive_ai::types::ProviderType::XAI);
            }
            if cfg.mistral_api_key.is_some() {
                providers.insert(hive_ai::types::ProviderType::Mistral);
            }
            if cfg.venice_api_key.is_some() {
                providers.insert(hive_ai::types::ProviderType::Venice);
            }
            if cfg.groq_api_key.is_some() {
                providers.insert(hive_ai::types::ProviderType::Groq);
            }
            if cfg.huggingface_api_key.is_some() {
                providers.insert(hive_ai::types::ProviderType::HuggingFace);
            }
            models_browser_view.update(cx, |browser, cx| {
                browser.set_enabled_providers(providers, cx);
                browser.set_openrouter_api_key(cfg.openrouter_api_key.clone(), cx);
                browser.set_openai_api_key(cfg.openai_api_key.clone(), cx);
                browser.set_anthropic_api_key(cfg.anthropic_api_key.clone(), cx);
                browser.set_google_api_key(cfg.google_api_key.clone(), cx);
                browser.set_groq_api_key(cfg.groq_api_key.clone(), cx);
                browser.set_huggingface_api_key(cfg.huggingface_api_key.clone(), cx);

                // If the user left the Models panel open last session, kick off
                // catalog fetches immediately so the panel isn't stuck on the
                // static-registry fallback.
                if restored_panel == Panel::Models {
                    browser.trigger_fetches(cx);
                }
            });
        }

        // When the user adds/removes models from the project list, persist to config
        // and push to settings model selector.
        cx.subscribe_in(
            &models_browser_view,
            window,
            |this, _view, event: &ProjectModelsChanged, _window, cx| {
                settings_actions::handle_project_models_changed(this, &event.0, cx);
            },
        )
        .detach();

        // Create the workflow builder view entity.
        let workflow_builder_view = cx.new(|cx| WorkflowBuilderView::new(window, cx));
        cx.subscribe_in(
            &workflow_builder_view,
            window,
            |this, _view, event: &WorkflowSaved, _window, cx| {
                workflow_actions::handle_workflow_saved(this, &event.0, cx);
            },
        )
        .detach();
        cx.subscribe_in(
            &workflow_builder_view,
            window,
            |this, _view, event: &WorkflowRunRequested, _window, cx| {
                workflow_actions::handle_workflow_run_requested(this, event.0.clone(), cx);
            },
        )
        .detach();

        // Create the channels view entity.
        let channels_view = cx.new(|cx| ChannelsView::new(window, cx));
        cx.subscribe_in(
            &channels_view,
            window,
            |this, _view, event: &ChannelMessageSent, _window, cx| {
                workflow_actions::handle_channel_message_sent(this, event, cx);
            },
        )
        .detach();

        // Handle new channel creation from the channels panel.
        cx.subscribe_in(
            &channels_view,
            window,
            |this, _view, event: &ChannelCreated, _window, cx| {
                info!("New channel created: {}", event.name);

                let mut new_id = String::new();
                if cx.has_global::<AppChannels>() {
                    // Derive a simple icon from the channel name.
                    let icon = "\u{1F4AC}"; // 💬
                    let description = format!("Custom channel: {}", event.name);
                    new_id = cx.global_mut::<AppChannels>().0.create_channel(
                        &event.name,
                        icon,
                        &description,
                        event.agents.clone(),
                    );
                }

                // Refresh the channels view to show the new channel and select it.
                workflow_actions::refresh_channels_view(this, cx);
                if !new_id.is_empty() {
                    this.channels_view.update(cx, |view, cx| {
                        view.select_channel(&new_id, cx);
                    });
                }
            },
        )
        .detach();

        let token_name_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_placeholder("e.g. My Awesome Token", window, cx);
            state
        });
        let token_symbol_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_placeholder("e.g. HIVE", window, cx);
            state
        });
        let total_supply_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_placeholder("e.g. 1000000000", window, cx);
            state
        });
        let decimals_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_placeholder("9", window, cx);
            state.set_value("9".to_string(), window, cx);
            state
        });
        let wallet_name_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_placeholder("Wallet name", window, cx);
            state
        });
        let wallet_secret_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_placeholder("Select a chain to configure wallet import", window, cx);
            state = state.masked(true);
            state
        });
        let rpc_url_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_placeholder("Select a chain to configure RPC", window, cx);
            state
        });

        for input in [
            &token_name_input,
            &token_symbol_input,
            &total_supply_input,
            &decimals_input,
            &wallet_name_input,
            &wallet_secret_input,
        ] {
            cx.subscribe_in(
                input,
                window,
                |this, _entity, event: &InputEvent, _window, cx| {
                    if matches!(event, InputEvent::Change | InputEvent::Blur) {
                        this.sync_token_launch_inputs_to_data(cx);
                        cx.notify();
                    }
                },
            )
            .detach();
        }

        let token_launch_inputs = TokenLaunchInputs {
            token_name: token_name_input,
            token_symbol: token_symbol_input,
            total_supply: total_supply_input,
            decimals: decimals_input,
            wallet_name: wallet_name_input,
            wallet_secret: wallet_secret_input,
            rpc_url: rpc_url_input,
        };

        // Focus handle for the workspace root — ensures dispatch_action works
        // from child panel click handlers even when no input is focused.
        let focus_handle = cx.focus_handle();

        let history_data = HistoryData::empty();
        let quick_start_data = QuickStartPanelData::empty();
        let kanban_data = KanbanData::default();
        let monitor_data = MonitorData::empty();
        let logs_data = LogsData::empty();
        let review_data = ReviewData::empty();
        let cost_data = CostData::empty();
        let routing_data = RoutingData::empty();
        let skills_data = SkillsData::empty();
        let token_launch_data = TokenLaunchData::new();
        let specs_data = SpecPanelData::empty();
        let agents_data = AgentsPanelData::empty();
        let shield_data = ShieldPanelData::empty();
        let learning_data = LearningPanelData::empty();
        let assistant_data = AssistantPanelData::empty();

        // Theme was already resolved and published at the top of new().
        let mut workspace = Self {
            theme,
            sidebar,
            status_bar,
            recent_workspace_roots,
            pinned_workspace_roots,
            show_project_dropdown: false,
            show_command_palette: false,
            show_utility_drawer: false,
            current_project_root: project_root,
            current_project_name: project_name,
            chat_input,
            quick_start_goal_input,
            command_palette_input,
            agents_remote_prompt_input,
            chat_service,
            settings_view,
            shield_view,
            models_browser_view,
            workflow_builder_view,
            channels_view,
            focus_handle,
            history_data,
            files_data,
            code_map_data,
            prompt_library_data,
            quick_start_data,
            kanban_data,
            monitor_data,
            logs_data,
            review_data,
            cost_data,
            routing_data,
            skills_data,
            token_launch_data,
            token_launch_inputs,
            specs_data,
            agents_data,
            activity_data: Default::default(),
            shield_data,
            learning_data,
            assistant_data,
            network_peer_data: NetworkPeerData::default(),
            terminal_data: TerminalData::empty(),
            terminal_input,
            terminal_cmd_tx: None,
            _terminal_task: None,
            _stream_task: None,
            session_dirty: false,
            last_saved_conversation_id: session.active_conversation_id.clone(),
            cached_chat_data: CachedChatData::new(),
            last_discovery_scan: None,
            discovery_scan_pending: false,
            discovery_done_flag: None,
            last_network_refresh: None,
            last_window_size: session.window_size,
            pending_plugin_preview: None,
            _file_watcher: None,
            swarm_task_trees: Vec::new(),
            toast_messages: Vec::new(),
            seen_notification_ids: HashSet::new(),
            quick_index: None,
            _quick_index_task: None,
            _bootstrap_task: None,
        };

        workspace.bootstrap_startup_snapshot(cx);
        workspace
    }

    fn bootstrap_startup_snapshot(&mut self, cx: &mut Context<Self>) {
        /// Maximum poll iterations before giving up (300 * 100ms = 30s).
        const MAX_POLL_ATTEMPTS: u32 = 300;

        let project_root = self.current_project_root.clone();
        let memory = cx
            .has_global::<hive_ui_core::AppCollectiveMemory>()
            .then(|| Arc::clone(&cx.global::<hive_ui_core::AppCollectiveMemory>().0));

        type BootstrapSlot = Arc<std::sync::Mutex<Option<Result<StartupProjectSnapshot, String>>>>;
        type QuickIndexSlot =
            Arc<std::sync::Mutex<Option<Result<Arc<hive_ai::quick_index::QuickIndex>, String>>>>;

        let startup_slot: BootstrapSlot = Arc::new(std::sync::Mutex::new(None));
        let startup_slot_for_thread = Arc::clone(&startup_slot);

        let quick_index_slot: QuickIndexSlot = Arc::new(std::sync::Mutex::new(None));
        let quick_index_slot_for_thread = Arc::clone(&quick_index_slot);

        if let Err(e) = std::thread::Builder::new()
            .name("hive-workspace-bootstrap".into())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let files_data = FilesData::from_path(&project_root);
                    let prompt_library_data =
                        hive_ui_panels::panels::prompt_library::PromptLibraryData::load();
                    let knowledge_sources = hive_ai::KnowledgeFileScanner::scan(&project_root);

                    if let Some(memory) = memory {
                        match memory.maintenance(0.98, 0.1, 0.85) {
                            Ok(report) => {
                                if report.pruned > 0 || report.deduplicated > 0 {
                                    tracing::info!(
                                        "Memory maintenance: decayed={}, pruned={}, deduplicated={}",
                                        report.decayed,
                                        report.pruned,
                                        report.deduplicated,
                                    );
                                }
                            }
                            Err(e) => tracing::warn!("Memory maintenance failed: {e}"),
                        }
                    }

                    StartupProjectSnapshot {
                        files_data,
                        prompt_library_data,
                        knowledge_sources,
                    }
                }));

                *startup_slot_for_thread
                    .lock()
                    .unwrap_or_else(|e| e.into_inner()) = Some(
                    result.map_err(|_| "bootstrap thread panicked".to_string()),
                );
            })
        {
            error!("Failed to spawn bootstrap thread: {e}");
        }

        let quick_index_root = self.current_project_root.clone();
        if let Err(e) = std::thread::Builder::new()
            .name("hive-workspace-quick-index".into())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    Arc::new(hive_ai::quick_index::QuickIndex::build(&quick_index_root))
                }));

                *quick_index_slot_for_thread
                    .lock()
                    .unwrap_or_else(|e| e.into_inner()) = Some(
                    result.map_err(|_| "quick-index thread panicked".to_string()),
                );
            })
        {
            error!("Failed to spawn quick-index thread: {e}");
        }

        let startup_slot_for_poll = Arc::clone(&startup_slot);
        self._bootstrap_task = Some(cx.spawn(
            async move |this: WeakEntity<HiveWorkspace>, app: &mut AsyncApp| {
                let mut attempts: u32 = 0;
                loop {
                    attempts += 1;
                    if attempts > MAX_POLL_ATTEMPTS {
                        tracing::error!(
                            "Bootstrap thread did not complete within {}s, giving up",
                            MAX_POLL_ATTEMPTS / 10
                        );
                        break;
                    }
                    if let Some(result) = startup_slot_for_poll
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .take()
                    {
                        match result {
                            Ok(snapshot) => {
                                let _ = this.update(app, |this, cx| {
                                    if !snapshot.knowledge_sources.is_empty() {
                                        info!(
                                            "Loaded {} project knowledge file(s) from {}",
                                            snapshot.knowledge_sources.len(),
                                            this.current_project_root.display()
                                        );
                                    }

                                    this.files_data = snapshot.files_data;
                                    this.prompt_library_data = snapshot.prompt_library_data;
                                    cx.set_global(AppKnowledgeFiles(snapshot.knowledge_sources));
                                    cx.notify();
                                });
                            }
                            Err(e) => {
                                tracing::error!("Bootstrap thread failed: {e}");
                            }
                        }
                        break;
                    }
                    app.background_executor()
                        .timer(std::time::Duration::from_millis(100))
                        .await;
                }
            },
        ));

        let quick_index_slot_for_poll = Arc::clone(&quick_index_slot);
        self._quick_index_task = Some(cx.spawn(
            async move |this: WeakEntity<HiveWorkspace>, app: &mut AsyncApp| {
                let mut attempts: u32 = 0;
                loop {
                    attempts += 1;
                    if attempts > MAX_POLL_ATTEMPTS {
                        tracing::error!(
                            "QuickIndex thread did not complete within {}s, giving up",
                            MAX_POLL_ATTEMPTS / 10
                        );
                        break;
                    }
                    if let Some(result) = quick_index_slot_for_poll
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .take()
                    {
                        match result {
                            Ok(quick_index) => {
                                let _ = this.update(app, |this, cx| {
                                    info!(
                                        "QuickIndex: {} files, {} symbols, {} deps in {:?}",
                                        quick_index.file_tree.total_files,
                                        quick_index.key_symbols.len(),
                                        quick_index.dependencies.len(),
                                        quick_index.indexed_at.elapsed()
                                    );

                                    this.quick_index = Some(quick_index.clone());
                                    this.code_map_data =
                                        hive_ui_panels::panels::code_map::CodeMapData::from_quick_index(
                                            &quick_index,
                                        );
                                    cx.set_global(AppQuickIndex(quick_index));
                                    project_context::schedule_background_project_indexing(cx);
                                    cx.notify();
                                });
                            }
                            Err(e) => {
                                tracing::error!("QuickIndex thread failed: {e}");
                            }
                        }
                        break;
                    }
                    app.background_executor()
                        .timer(std::time::Duration::from_millis(100))
                        .await;
                }
            },
        ));
    }

    pub fn set_active_panel(&mut self, panel: Panel) {
        self.sidebar.active_panel = panel;
        if let Some(destination) = panel.shell_destination() {
            self.sidebar.active_destination = destination;
        }
        self.session_dirty = true;
    }

    /// Override the version shown in the status bar (called from hive_app
    /// which has access to the git-based HIVE_VERSION).
    pub fn set_version(&mut self, version: String) {
        self.status_bar.version = version;
    }

    // -- History data --------------------------------------------------------

    /// Append a log entry to the in-memory list and persist it to the SQLite
    /// database.  This is the single entry-point for adding log entries so
    /// they survive application restarts.
    pub fn log(
        &mut self,
        level: hive_ui_panels::panels::logs::LogLevel,
        source: impl Into<String>,
        message: impl Into<String>,
        cx: &App,
    ) {
        let source = source.into();
        let message = message.into();
        if cx.has_global::<AppDatabase>() {
            let db = &cx.global::<AppDatabase>().0;
            if let Err(e) = db.save_log(level.as_str(), &source, &message) {
                warn!("Failed to persist log entry: {e}");
            }
        }
        self.logs_data.add_entry(level, source, message);
    }

    // -- Session persistence -------------------------------------------------

    pub fn load_history_data() -> HistoryData {
        data_refresh::load_history_data()
    }

    /// Persist the current session state (conversation ID, active panel) to
    /// `~/.hive/session.json`. This is lightweight -- just a small JSON write.
    /// Errors are logged but never propagated.
    pub fn save_session(&mut self, cx: &App) {
        let svc = self.chat_service.read(cx);
        let conv_id = svc.conversation_id().map(String::from);

        let state = SessionState {
            active_conversation_id: conv_id.clone(),
            active_panel: self.sidebar.active_panel.to_stored().to_string(),
            window_size: self.last_window_size,
            working_directory: Some(self.current_project_root.to_string_lossy().to_string()),
            recent_workspaces: self
                .recent_workspace_roots
                .iter()
                .map(|path| path.to_string_lossy().to_string())
                .collect(),
            pinned_workspaces: self
                .pinned_workspace_roots
                .iter()
                .map(|path| path.to_string_lossy().to_string())
                .collect(),
            open_files: Vec::new(),
            chat_draft: None,
        };

        if let Err(e) = state.save() {
            warn!("Failed to save session: {e}");
        }

        self.last_saved_conversation_id = conv_id;
        self.session_dirty = false;
    }

    // -- Send flow -----------------------------------------------------------


    // -- Rendering -----------------------------------------------------------

    fn render_active_panel(&mut self, cx: &mut Context<Self>) -> AnyElement {
        panel_router::render_active_panel(self, cx)
    }

    // -- Keyboard action handlers --------------------------------------------

    // -- Terminal panel handlers --------------------------------------------

    // -- Tool Approval handlers ---------------------------------------------

    // -- Network panel handlers ----------------------------------------------

    // -- Agents panel handlers -----------------------------------------------

    // Preserved for backwards compatibility — original system editor open logic
    // was replaced by the built-in file viewer above.
    #[allow(dead_code)]
    fn _handle_files_open_external_legacy(
        &mut self,
        file_path: &std::path::Path,
        cx: &mut Context<Self>,
    ) {
        // Open in default system editor, validating the launch command.
        let command_string = if cfg!(target_os = "windows") {
            format!("cmd /C start \"\" \"{}\"", file_path.to_string_lossy())
        } else if cfg!(target_os = "macos") {
            format!("open \"{}\"", file_path.to_string_lossy())
        } else {
            format!("xdg-open \"{}\"", file_path.to_string_lossy())
        };
        if cx.has_global::<AppSecurity>()
            && let Err(e) = cx.global::<AppSecurity>().0.check_command(&command_string)
        {
            error!("Files: blocked open command: {e}");
            self.push_notification(
                cx,
                NotificationType::Error,
                "Files",
                format!("Blocked file open command: {e}"),
            );
            return;
        }

        #[cfg(target_os = "windows")]
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", "", &file_path.to_string_lossy()])
            .spawn();
        #[cfg(target_os = "macos")]
        let _ = std::process::Command::new("open").arg(&file_path).spawn();
        #[cfg(target_os = "linux")]
        let _ = std::process::Command::new("xdg-open")
            .arg(&file_path)
            .spawn();

        cx.notify();
    }

    // -- History panel handlers ----------------------------------------------

    // -- Kanban panel handlers -----------------------------------------------

    // -- Logs panel handlers -------------------------------------------------

    fn push_notification(
        &self,
        cx: &mut Context<Self>,
        kind: NotificationType,
        title: &str,
        message: impl Into<String>,
    ) {
        if cx.has_global::<AppNotifications>() {
            cx.global_mut::<AppNotifications>()
                .0
                .push(AppNotification::new(kind, message).with_title(title));
        }
    }

    // -- Costs panel handlers ------------------------------------------------

    // -- PR operations -----------------------------------------------------------

    // -- Skills panel handlers -----------------------------------------------

    // -- Plugin action handlers -----------------------------------------------

    // -- Routing panel handlers ----------------------------------------------


    // -- Monitor panel handlers ----------------------------------------------

}

fn network_peer_status_label(state: &hive_network::PeerState) -> String {
    match state {
        hive_network::PeerState::Connected => "Connected",
        hive_network::PeerState::Connecting => "Connecting",
        hive_network::PeerState::Discovered => "Discovered",
        hive_network::PeerState::Disconnected => "Disconnected",
        hive_network::PeerState::Banned => "Banned",
    }
    .to_string()
}

fn network_peer_status_rank(status: &str) -> u8 {
    match status {
        "Connected" => 0,
        "Connecting" => 1,
        "Discovered" => 2,
        "Disconnected" => 3,
        "Banned" => 4,
        _ => 5,
    }
}

fn format_network_relative_time(dt: chrono::DateTime<Utc>) -> String {
    let duration = Utc::now().signed_duration_since(dt);
    let total_seconds = duration.num_seconds();

    if total_seconds < 60 {
        return "Just now".to_string();
    }

    let minutes = duration.num_minutes();
    if minutes == 1 {
        return "1 minute ago".to_string();
    }
    if minutes < 60 {
        return format!("{minutes} minutes ago");
    }

    let hours = duration.num_hours();
    if hours == 1 {
        return "1 hour ago".to_string();
    }
    if hours < 24 {
        return format!("{hours} hours ago");
    }

    let days = duration.num_days();
    if days == 1 {
        return "Yesterday".to_string();
    }
    if days < 7 {
        return format!("{days} days ago");
    }

    dt.format("%b %-d, %Y").to_string()
}

impl Render for HiveWorkspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Capture the current window size for session persistence.
        let bounds = window.bounds();
        let w = f32::from(bounds.size.width) as u32;
        let h = f32::from(bounds.size.height) as u32;
        if w > 0 && h > 0 {
            self.last_window_size = Some([w, h]);
        }

        status_sync::sync_status_bar(self, window, cx);

        let shell_header_data = chrome::build_shell_header_data(self, cx);

        // Auto-focus: when nothing is focused, give focus to the chat input on
        // the Chat panel or the workspace root on other panels. This ensures
        // typing goes straight into the input and dispatch_action() still works.
        chrome::apply_default_focus(self, window, cx);

        // Render the active panel first (may require &mut self for cache updates).
        let active_panel_el = self.render_active_panel(cx);

        let theme = &self.theme;
        let chat_input = self.chat_input.clone();

        div()
            .id("workspace-root")
            .track_focus(&self.focus_handle)
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.bg_primary)
            .text_color(theme.text_primary)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if event.keystroke.modifiers.secondary()
                    && event.keystroke.key.eq_ignore_ascii_case("k")
                {
                    cx.stop_propagation();
                    overlays::handle_toggle_command_palette(this, window, cx);
                    return;
                }

                if event.keystroke.key == "escape" && this.show_command_palette {
                    overlays::close_command_palette(this, window, cx);
                    return;
                }

                if event.keystroke.key == "escape" && this.show_utility_drawer {
                    this.show_utility_drawer = false;
                    cx.notify();
                    return;
                }

                if event.keystroke.key == "escape" && this.show_project_dropdown {
                    this.show_project_dropdown = false;
                    cx.notify();
                }
            }))
            // -- Action handlers for keyboard shortcuts -----------------------
            .on_action(cx.listener(chat_actions::handle_new_conversation))
            .on_action(cx.listener(chat_actions::handle_clear_chat))
            .on_action(cx.listener(navigation::handle_switch_to_chat))
            .on_action(cx.listener(navigation::handle_switch_to_quick_start))
            .on_action(cx.listener(navigation::handle_switch_to_history))
            .on_action(cx.listener(navigation::handle_switch_to_files))
            .on_action(cx.listener(navigation::handle_switch_to_kanban))
            .on_action(cx.listener(navigation::handle_switch_to_monitor))
            .on_action(cx.listener(navigation::handle_switch_to_activity))
            .on_action(cx.listener(navigation::handle_switch_to_logs))
            .on_action(cx.listener(navigation::handle_switch_to_costs))
            .on_action(cx.listener(navigation::handle_switch_to_review))
            .on_action(cx.listener(navigation::handle_switch_to_skills))
            .on_action(cx.listener(navigation::handle_switch_to_routing))
            .on_action(cx.listener(navigation::handle_switch_to_models))
            .on_action(cx.listener(navigation::handle_switch_to_token_launch))
            .on_action(cx.listener(navigation::handle_switch_to_specs))
            .on_action(cx.listener(navigation::handle_switch_to_agents))
            .on_action(cx.listener(navigation::handle_switch_to_workflows))
            .on_action(cx.listener(navigation::handle_switch_to_channels))
            .on_action(cx.listener(navigation::handle_switch_to_learning))
            .on_action(cx.listener(navigation::handle_switch_to_shield))
            .on_action(cx.listener(navigation::handle_switch_to_assistant))
            .on_action(cx.listener(navigation::handle_switch_to_settings))
            .on_action(cx.listener(navigation::handle_switch_to_help))
            .on_action(cx.listener(navigation::handle_switch_to_network))
            .on_action(cx.listener(navigation::handle_switch_to_terminal))
            .on_action(cx.listener(navigation::handle_switch_to_code_map))
            .on_action(cx.listener(navigation::handle_switch_to_prompt_library))
            .on_action(cx.listener(prompt_library_actions::handle_prompt_library_save_current))
            .on_action(cx.listener(prompt_library_actions::handle_prompt_library_refresh))
            .on_action(cx.listener(prompt_library_actions::handle_prompt_library_load))
            .on_action(cx.listener(prompt_library_actions::handle_prompt_library_delete))
            .on_action(cx.listener(network_actions::handle_network_refresh))
            .on_action(cx.listener(navigation::handle_open_workspace_directory))
            .on_action(cx.listener(navigation::handle_toggle_project_dropdown))
            .on_action(cx.listener(|this, _action: &ToggleCommandPalette, window, cx| {
                overlays::handle_toggle_command_palette(this, window, cx);
            }))
            .on_action(cx.listener(navigation::handle_switch_to_workspace_action))
            .on_action(cx.listener(navigation::handle_toggle_pin_workspace))
            .on_action(cx.listener(navigation::handle_remove_recent_workspace))
            // -- Panel action handlers -----------------------------------
            // Files
            .on_action(cx.listener(file_actions::handle_files_navigate_back))
            .on_action(cx.listener(file_actions::handle_files_navigate_to))
            .on_action(cx.listener(file_actions::handle_files_open_entry))
            .on_action(cx.listener(file_actions::handle_files_delete_entry))
            .on_action(cx.listener(file_actions::handle_files_refresh))
            .on_action(cx.listener(file_actions::handle_files_new_file))
            .on_action(cx.listener(file_actions::handle_files_new_folder))
            .on_action(cx.listener(file_actions::handle_files_close_viewer))
            .on_action(cx.listener(file_actions::handle_files_toggle_check))
            .on_action(cx.listener(file_actions::handle_files_clear_checked))
            .on_action(cx.listener(file_actions::handle_files_set_search_query))
            .on_action(cx.listener(chat_actions::handle_chat_read_aloud))
            // Apply mode + clipboard
            .on_action(cx.listener(chat_actions::handle_apply_code_block))
            .on_action(cx.listener(chat_actions::handle_apply_all_edits))
            .on_action(cx.listener(chat_actions::handle_copy_to_clipboard))
            .on_action(cx.listener(chat_actions::handle_copy_full_prompt))
            .on_action(cx.listener(chat_actions::handle_export_prompt))
            // History
            .on_action(cx.listener(history_actions::handle_history_load))
            .on_action(cx.listener(history_actions::handle_history_delete))
            .on_action(cx.listener(history_actions::handle_history_refresh))
            .on_action(cx.listener(history_actions::handle_history_clear_all))
            .on_action(cx.listener(history_actions::handle_history_clear_all_confirm))
            .on_action(cx.listener(history_actions::handle_history_clear_all_cancel))
            // Kanban
            .on_action(cx.listener(kanban_actions::handle_kanban_add_task))
            // Logs
            .on_action(cx.listener(logs_actions::handle_logs_clear))
            .on_action(cx.listener(logs_actions::handle_logs_set_filter))
            .on_action(cx.listener(logs_actions::handle_logs_toggle_auto_scroll))
            // Activity
            .on_action(cx.listener(activity_actions::handle_activity_refresh))
            .on_action(cx.listener(activity_actions::handle_activity_set_view))
            .on_action(cx.listener(activity_actions::handle_activity_approve))
            .on_action(cx.listener(activity_actions::handle_activity_deny))
            .on_action(cx.listener(activity_actions::handle_activity_expand_event))
            .on_action(cx.listener(activity_actions::handle_activity_export_csv))
            .on_action(cx.listener(activity_actions::handle_activity_set_filter))
            // Terminal
            .on_action(cx.listener(terminal_host::handle_terminal_clear))
            .on_action(cx.listener(terminal_host::handle_terminal_submit))
            .on_action(cx.listener(terminal_host::handle_terminal_kill))
            .on_action(cx.listener(terminal_host::handle_terminal_restart))
            // Tool approval
            .on_action(cx.listener(approval_actions::handle_tool_approve))
            .on_action(cx.listener(approval_actions::handle_tool_reject))
            // Costs
            .on_action(cx.listener(costs_actions::handle_costs_export_csv))
            .on_action(cx.listener(costs_actions::handle_costs_reset_today))
            .on_action(cx.listener(costs_actions::handle_costs_clear_history))
            // Review
            .on_action(cx.listener(Self::handle_review_stage_all))
            .on_action(cx.listener(Self::handle_review_unstage_all))
            .on_action(cx.listener(Self::handle_review_commit))
            .on_action(cx.listener(Self::handle_review_discard_all))
            // Git Ops
            .on_action(cx.listener(Self::handle_review_switch_tab))
            .on_action(cx.listener(Self::handle_review_ai_commit_message))
            .on_action(cx.listener(Self::handle_review_set_commit_message))
            .on_action(cx.listener(Self::handle_review_commit_with_message))
            .on_action(cx.listener(Self::handle_review_push))
            .on_action(cx.listener(Self::handle_review_push_set_upstream))
            .on_action(cx.listener(Self::handle_review_pr_refresh))
            .on_action(cx.listener(Self::handle_review_pr_ai_generate))
            .on_action(cx.listener(Self::handle_review_pr_create))
            .on_action(cx.listener(Self::handle_review_pr_set_title))
            .on_action(cx.listener(Self::handle_review_pr_set_body))
            .on_action(cx.listener(Self::handle_review_pr_set_base))
            .on_action(cx.listener(Self::handle_review_branch_refresh))
            .on_action(cx.listener(Self::handle_review_branch_create))
            .on_action(cx.listener(Self::handle_review_branch_switch))
            .on_action(cx.listener(Self::handle_review_branch_delete_named))
            .on_action(cx.listener(Self::handle_review_branch_set_name))
            .on_action(cx.listener(Self::handle_review_lfs_refresh))
            .on_action(cx.listener(Self::handle_review_lfs_track))
            .on_action(cx.listener(Self::handle_review_lfs_untrack))
            .on_action(cx.listener(Self::handle_review_lfs_set_pattern))
            .on_action(cx.listener(Self::handle_review_lfs_pull))
            .on_action(cx.listener(Self::handle_review_lfs_push))
            .on_action(cx.listener(Self::handle_review_gitflow_init))
            .on_action(cx.listener(Self::handle_review_gitflow_start))
            .on_action(cx.listener(Self::handle_review_gitflow_finish_named))
            .on_action(cx.listener(Self::handle_review_gitflow_set_name))
            // Skills / ClawdHub
            .on_action(cx.listener(skills_actions::handle_skills_refresh))
            .on_action(cx.listener(skills_actions::handle_skills_install))
            .on_action(cx.listener(skills_actions::handle_skills_remove))
            .on_action(cx.listener(skills_actions::handle_skills_toggle))
            .on_action(cx.listener(skills_actions::handle_skills_create))
            .on_action(cx.listener(skills_actions::handle_skills_add_source))
            .on_action(cx.listener(skills_actions::handle_skills_remove_source))
            .on_action(cx.listener(skills_actions::handle_skills_set_tab))
            .on_action(cx.listener(skills_actions::handle_skills_set_search))
            .on_action(cx.listener(skills_actions::handle_skills_set_category))
            .on_action(cx.listener(skills_actions::handle_skills_clear_search))
            // Plugins
            .on_action(cx.listener(plugin_actions::handle_plugin_import_open))
            .on_action(cx.listener(plugin_actions::handle_plugin_import_cancel))
            .on_action(cx.listener(plugin_actions::handle_plugin_import_from_github))
            .on_action(cx.listener(plugin_actions::handle_plugin_import_from_url))
            .on_action(cx.listener(plugin_actions::handle_plugin_import_from_local))
            .on_action(cx.listener(plugin_actions::handle_plugin_import_confirm))
            .on_action(cx.listener(plugin_actions::handle_plugin_import_toggle_skill))
            .on_action(cx.listener(plugin_actions::handle_plugin_remove))
            .on_action(cx.listener(plugin_actions::handle_plugin_update))
            .on_action(cx.listener(plugin_actions::handle_plugin_toggle_expand))
            .on_action(cx.listener(plugin_actions::handle_plugin_toggle_skill))
            // Routing
            .on_action(cx.listener(routing_actions::handle_routing_add_rule))
            // Token Launch
            .on_action(cx.listener(token_launch_actions::handle_token_launch_set_step))
            .on_action(cx.listener(token_launch_actions::handle_token_launch_select_chain))
            .on_action(cx.listener(token_launch_actions::handle_token_launch_select_wallet))
            .on_action(cx.listener(token_launch_actions::handle_token_launch_create_wallet))
            .on_action(cx.listener(token_launch_actions::handle_token_launch_import_wallet))
            .on_action(cx.listener(token_launch_actions::handle_token_launch_save_rpc_config))
            .on_action(cx.listener(token_launch_actions::handle_token_launch_reset_rpc_config))
            .on_action(cx.listener(token_launch_actions::handle_token_launch_deploy))
            // Settings
            .on_action(cx.listener(settings_actions::handle_settings_save))
            .on_action(cx.listener(settings_actions::handle_export_config))
            .on_action(cx.listener(settings_actions::handle_import_config))
            // Quick Start
            .on_action(cx.listener(quick_start_actions::handle_quick_start_select_template))
            .on_action(cx.listener(quick_start_actions::handle_quick_start_open_panel))
            .on_action(cx.listener(quick_start_actions::handle_quick_start_run_project))
            // Theme + context format
            .on_action(cx.listener(settings_actions::handle_theme_changed))
            .on_action(cx.listener(settings_actions::handle_context_format_changed))
            // Monitor
            .on_action(cx.listener(monitor_actions::handle_monitor_refresh))
            // Agents
            .on_action(cx.listener(agents_actions::handle_agents_refresh_remote_agents))
            .on_action(cx.listener(agents_actions::handle_agents_reload_workflows))
            .on_action(cx.listener(agents_actions::handle_agents_select_remote_agent))
            .on_action(cx.listener(agents_actions::handle_agents_select_remote_skill))
            .on_action(cx.listener(agents_actions::handle_agents_discover_remote_agent))
            .on_action(cx.listener(agents_actions::handle_agents_run_remote_agent))
            .on_action(cx.listener(agents_actions::handle_agents_run_workflow))
            .on_action(cx.listener(workflow_actions::handle_workflow_builder_load))
            // Connected Accounts
            .on_action(cx.listener(account_actions::handle_account_connect_platform))
            .on_action(cx.listener(account_actions::handle_account_disconnect_platform))
            // Assistant actions
            .on_action(cx.listener(assistant_actions::handle_daily_briefing))
            .on_action(cx.listener(assistant_actions::handle_check_email))
            .on_action(cx.listener(assistant_actions::handle_check_calendar))
            // Voice
            .on_action(cx.listener(utility_actions::handle_voice_process_text))
            // Auto-update
            .on_action(cx.listener(utility_actions::handle_trigger_app_update))
            // Ollama model management
            .on_action(cx.listener(utility_actions::handle_ollama_pull_model))
            .on_action(cx.listener(utility_actions::handle_ollama_delete_model))
            // Chat disclosure
            .on_action(cx.listener(utility_actions::handle_toggle_disclosure))
            // Titlebar
            .child(Titlebar::render(theme, window, &self.current_project_root))
            // Project dropdown backdrop (dismisses on click)
            .when(self.show_project_dropdown, |el| {
                el.child(chrome::render_project_dropdown_backdrop())
            })
            // Project dropdown overlay
            .when(self.show_project_dropdown, |el| {
                el.child(overlays::render_project_dropdown(self, cx))
            })
            .when(self.show_command_palette, |el| {
                el.child(overlays::render_command_palette(self, cx))
            })
            .when(self.show_utility_drawer, |el| {
                el.child(chrome::render_utility_drawer_backdrop(cx))
            })
            // Main content area: sidebar + panel
            .child(chrome::render_main_content(
                self,
                active_panel_el,
                shell_header_data,
                chat_input,
                cx,
            ))
            // Status bar
            .child(self.status_bar.render(theme))
            // Toast notification overlay (top-right corner)
            .when(!self.toast_messages.is_empty(), |el| {
                let toast_els: Vec<_> = self
                    .toast_messages
                    .iter()
                    .map(|(_id, msg, kind, _created)| {
                        hive_ui_panels::components::toast::render_toast(*kind, msg, theme)
                    })
                    .collect();
                el.child(
                    div()
                        .absolute()
                        .top(px(48.0))
                        .right(px(16.0))
                        .w(px(360.0))
                        .flex()
                        .flex_col()
                        .gap(theme.space_2)
                        .children(toast_els),
                )
            })
    }
}

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

/// Event emitted when clicking a sidebar panel.
#[derive(Debug, Clone)]
pub struct SwitchPanel(pub Panel);

impl EventEmitter<SwitchPanel> for HiveWorkspace {}

// ---------------------------------------------------------------------------
// Chat cache sync (bridges ChatService → CachedChatData across crate boundary)
// ---------------------------------------------------------------------------

fn sync_chat_cache(cache: &mut CachedChatData, svc: &ChatService) {
    let svc_gen = svc.generation();
    if svc_gen == cache.generation {
        return;
    }

    cache.display_messages.clear();
    cache.total_cost = 0.0;
    cache.total_tokens = 0;

    for msg in svc.messages() {
        if msg.role == crate::chat_service::MessageRole::Assistant && msg.content.is_empty() {
            continue;
        }
        let role = match msg.role {
            crate::chat_service::MessageRole::User => hive_ai::MessageRole::User,
            crate::chat_service::MessageRole::Assistant => hive_ai::MessageRole::Assistant,
            crate::chat_service::MessageRole::System => hive_ai::MessageRole::System,
            crate::chat_service::MessageRole::Error => hive_ai::MessageRole::Error,
            crate::chat_service::MessageRole::Tool => hive_ai::MessageRole::Tool,
        };
        let tool_calls = msg
            .tool_calls
            .as_ref()
            .map(|tcs| {
                tcs.iter()
                    .map(|tc| ToolCallDisplay {
                        name: tc.name.clone(),
                        args: serde_json::to_string_pretty(&tc.input)
                            .unwrap_or_else(|_| tc.input.to_string()),
                    })
                    .collect()
            })
            .unwrap_or_default();

        let display_msg = DisplayMessage {
            role,
            content: msg.content.clone(),
            thinking: None,
            model: msg.model.clone(),
            cost: msg.cost,
            tokens: msg.tokens.map(|(i, o)| (i + o) as u32),
            timestamp: msg.timestamp,
            show_thinking: false,
            tool_calls,
            tool_call_id: msg.tool_call_id.clone(),
            disclosure: Default::default(),
        };
        if let Some(c) = display_msg.cost {
            cache.total_cost += c;
        }
        if let Some(t) = display_msg.tokens {
            cache.total_tokens += t;
        }
        cache.display_messages.push(display_msg);
    }

    cache.generation = svc_gen;
}

// ---------------------------------------------------------------------------
// Standalone helpers
// ---------------------------------------------------------------------------

/// Parse a `ps -o etime=` elapsed time string into seconds.
///
/// Format variations: `MM:SS`, `HH:MM:SS`, `D-HH:MM:SS`.
fn parse_etime(s: &str) -> u64 {
    let s = s.trim();
    let (days, rest) = if let Some(idx) = s.find('-') {
        let d = s[..idx].parse::<u64>().unwrap_or(0);
        (d, &s[idx + 1..])
    } else {
        (0, s)
    };
    let parts: Vec<u64> = rest.split(':').filter_map(|p| p.parse().ok()).collect();
    let (hours, minutes, seconds) = match parts.len() {
        3 => (parts[0], parts[1], parts[2]),
        2 => (0, parts[0], parts[1]),
        1 => (0, 0, parts[0]),
        _ => (0, 0, 0),
    };
    days * 86400 + hours * 3600 + minutes * 60 + seconds
}

fn parse_github_owner_repo(url: &str) -> Option<(String, String)> {
    // HTTPS: https://github.com/owner/repo.git
    if let Some(rest) = url.strip_prefix("https://github.com/") {
        let parts: Vec<&str> = rest.trim_end_matches(".git").splitn(2, '/').collect();
        if parts.len() == 2 {
            return Some((parts[0].to_string(), parts[1].to_string()));
        }
    }
    // SSH: git@github.com:owner/repo.git
    if let Some(rest) = url.strip_prefix("git@github.com:") {
        let parts: Vec<&str> = rest.trim_end_matches(".git").splitn(2, '/').collect();
        if parts.len() == 2 {
            return Some((parts[0].to_string(), parts[1].to_string()));
        }
    }
    None
}
