use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::input::{InputEvent, InputState};
use gpui_component::{Icon, IconName, Sizable as _};
use gpui_component::scroll::ScrollableElement;
use gpui_component::theme::Theme as GpuiTheme;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{debug, error, info, warn};

use hive_ai::providers::AiProvider;
use hive_ai::speculative::{self, SpeculativeConfig};
use hive_ai::types::{ChatRequest, ToolDefinition as AiToolDefinition};
use hive_core::config::HiveConfig;
use hive_core::notifications::{AppNotification, NotificationType};
use hive_core::session::SessionState;
use hive_core::theme_manager::ThemeManager;
use hive_agents::plugin_manager::PluginManager;
use hive_agents::plugin_types::{PluginPreview, PluginSource};
use hive_assistant::ReminderTrigger;
use hive_terminal::{InteractiveShell, ShellOutput};

use crate::chat_input::{ChatInputView, SubmitMessage};
use crate::chat_service::{ChatService, MessageRole, StreamCompleted};
use chrono::Utc;
use hive_ui_core::{
    // Globals
    AppA2aClient, AppAiService, AppAssistant, AppAutomation, AppAws, AppAzure, AppBrowser,
    AppChannels, AppConfig, AppDatabase, AppDocker, AppDocsIndexer, AppFleetLearning,
    AppGcp, AppHueClient, AppIntegrationDb, AppKnowledge, AppKnowledgeFiles, AppHiveMemory, AppKubernetes,
    AppLearning, AppMarketplace, AppMcpServer, AppMessaging, AppNetwork, AppNotifications,
    AppOllamaManager, AppPersonas, AppProjectManagement, AppQuickIndex, AppReminderRx,
    AppContextSelection, AppSkillManager, AppRagService, AppContextEngine, AppRpcConfig, AppSecurity,
    ContextSelectionState,
    AppSemanticSearch, AppShield, AppSpecs, AppTheme, AppTts, AppUpdater,
    AppVoiceAssistant, AppWallets,
    // Types
    HiveTheme, Panel, Sidebar,
};
// Re-export actions so hive_app can import from hive_ui::workspace::*
pub use hive_ui_core::{
    ClearChat, NewConversation,
    SwitchToChat, SwitchToQuickStart, SwitchToHistory, SwitchToFiles, SwitchToCodeMap,
    SwitchToPromptLibrary, SwitchToKanban, SwitchToMonitor,
    SwitchToActivity, SwitchToLogs, SwitchToCosts, SwitchToReview, SwitchToSkills, SwitchToRouting,
    SwitchToModels, SwitchToTokenLaunch, SwitchToSpecs, SwitchToAgents, SwitchToLearning,
    SwitchToShield, SwitchToAssistant, SwitchToSettings, SwitchToNetwork, SwitchToTerminal, SwitchToHelp,
    OpenWorkspaceDirectory,
    ToggleProjectDropdown,
    SwitchToWorkspace, TogglePinWorkspace, RemoveRecentWorkspace,
    FilesClearChecked, FilesNavigateBack, FilesRefresh, FilesNewFile, FilesNewFolder,
    FilesCloseViewer, FilesNavigateTo, FilesOpenEntry, FilesDeleteEntry, FilesToggleCheck,
    HistoryRefresh, HistoryLoadConversation, HistoryDeleteConversation,
    HistoryClearAll, HistoryClearAllConfirm, HistoryClearAllCancel,
    KanbanAddTask, LogsClear, LogsToggleAutoScroll, LogsSetFilter,
    CostsExportCsv, CostsResetToday, CostsClearHistory,
    ReviewStageAll, ReviewUnstageAll, ReviewCommit, ReviewDiscardAll,
    ReviewAiCommitMessage, ReviewBranchCreate, ReviewBranchDeleteNamed, ReviewBranchRefresh,
    ReviewBranchSetName, ReviewBranchSwitch, ReviewCommitWithMessage,
    ReviewGitflowFinishNamed, ReviewGitflowInit, ReviewGitflowSetName, ReviewGitflowStart,
    ReviewLfsPull, ReviewLfsPush, ReviewLfsRefresh, ReviewLfsSetPattern, ReviewLfsTrack,
    ReviewLfsUntrack, ReviewPrAiGenerate, ReviewPrCreate, ReviewPrRefresh, ReviewPrSetBase,
    ReviewPrSetBody, ReviewPrSetTitle, ReviewPush, ReviewPushSetUpstream,
    ReviewSetCommitMessage, ReviewSwitchTab,
    SkillsRefresh, SkillsClearSearch, SkillsInstall, SkillsRemove, SkillsToggle,
    SkillsCreate, SkillsAddSource, SkillsRemoveSource, SkillsSetTab, SkillsSetSearch,
    SkillsSetCategory,
    PluginImportOpen, PluginImportCancel, PluginImportFromGitHub,
    PluginImportFromUrl, PluginImportFromLocal, PluginImportConfirm,
    PluginImportToggleSkill, PluginRemove, PluginUpdate, PluginToggleExpand,
    PluginToggleSkill, AppPluginManager,
    RoutingAddRule, TokenLaunchCreateWallet, TokenLaunchDeploy, TokenLaunchImportWallet,
    TokenLaunchResetRpcConfig, TokenLaunchSaveRpcConfig, TokenLaunchSetStep,
    TokenLaunchSelectChain, TokenLaunchSelectWallet,
    SettingsSave, ExportConfig, ImportConfig,
    MonitorRefresh, NetworkRefresh,
    TerminalClear, TerminalSubmitCommand, TerminalKill, TerminalRestart,
    ToolApprove, ToolReject,
    AgentsDiscoverRemoteAgent, AgentsRefreshRemoteAgents,
    AgentsReloadWorkflows, AgentsRunRemoteAgent, AgentsRunWorkflow, AgentsSelectRemoteAgent,
    AgentsSelectRemoteSkill,
    QuickStartOpenPanel, QuickStartRunProject, QuickStartSelectTemplate,
    SwitchToWorkflows, SwitchToChannels,
    WorkflowBuilderSave, WorkflowBuilderRun, WorkflowBuilderDeleteNode,
    WorkflowBuilderLoadWorkflow, ChannelSelect,
    AccountConnectPlatform, AccountDisconnectPlatform,
    TriggerAppUpdate, ThemeChanged, ContextFormatChanged,
    ApplyCodeBlock, ApplyAllEdits, CopyToClipboard, CopyFullPrompt, ExportPrompt,
    PromptLibrarySaveCurrent, PromptLibraryRefresh, PromptLibraryLoad, PromptLibraryDelete,
    VoiceProcessText,
    OllamaPullModel, OllamaDeleteModel,
};
use hive_ui_panels::panels::chat::{DisplayMessage, ToolCallDisplay};
use hive_ui_panels::panels::{
    agents::{AgentsPanel, AgentsPanelData},
    assistant::{AssistantPanel, AssistantPanelData},
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
    quick_start::{
        QuickStartNextStepDisplay, QuickStartPanel, QuickStartPanelData,
        QuickStartSetupDisplay, QuickStartTemplateDisplay, QuickStartTone,
    },
    review::{AiCommitState, BranchEntry, GitOpsTab, LfsFileEntry, PrForm, PrSummary, ReviewData, ReviewPanel},
    routing::{RoutingData, RoutingPanel},
    settings::{SettingsSaved, SettingsView},
    shield::{ShieldConfigChanged, ShieldPanelData, ShieldView},
    skills::{SkillsData, SkillsPanel},
    specs::{SpecPanelData, SpecsPanel},
    terminal::{TerminalData, TerminalPanel},
    token_launch::{TokenLaunchData, TokenLaunchInputs, TokenLaunchPanel},
    workflow_builder::{WorkflowBuilderView, WorkflowSaved, WorkflowRunRequested},
    channels::{ChannelsView, ChannelCreated, ChannelMessageSent},
};
use crate::statusbar::{ConnectivityDisplay, StatusBar};
use crate::titlebar::Titlebar;

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
        request.messages.insert(0, hive_ai::types::ChatMessage {
            role: hive_ai::types::MessageRole::System,
            content: format!("## Recalled Memories\n\n{}", memory_ctx),
            timestamp: chrono::Utc::now(),
            tool_call_id: None,
            tool_calls: None,
        });
    }
    if !extra_context.is_empty() {
        request.messages.insert(0, hive_ai::types::ChatMessage {
            role: hive_ai::types::MessageRole::System,
            content: format!("## Additional Context\n\n{}", extra_context),
            timestamp: chrono::Utc::now(),
            tool_call_id: None,
            tool_calls: None,
        });
    }
}

/// Helper types for background assistant data fetching.
#[derive(Debug)]
struct EmailPreviewData {
    from: String,
    subject: String,
    snippet: String,
    time: String,
    important: bool,
}

#[derive(Debug)]
struct EventData {
    title: String,
    time: String,
    location: Option<String>,
}

#[derive(Debug)]
enum AssistantFetchResult {
    Emails {
        provider: String,
        previews: Vec<EmailPreviewData>,
    },
    Events(Vec<EventData>),
    RecentActions(Vec<String>),
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
    activity_data: hive_ui_panels::panels::activity::ActivityData,
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
}

const MAX_RECENT_WORKSPACES: usize = 8;
const MAX_PINNED_WORKSPACES: usize = 20;

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
            Self::resolve_theme_by_name(&theme_name)
        };
        cx.set_global(AppTheme(theme.clone()));
        Self::sync_gpui_theme(&theme, cx);

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
        })
        .detach();

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

        let project_root = Self::resolve_project_root_from_session(&session);
        let recent_workspace_roots = Self::load_recent_workspace_roots(&session, &project_root);
        let pinned_workspace_roots = Self::load_pinned_workspace_roots(&session);
        let project_name = Self::project_name_from_path(&project_root);
        let files_data = FilesData::from_path(&project_root);
        let code_map_data = hive_ui_panels::panels::code_map::build_code_map_data(cx);
        let prompt_library_data = hive_ui_panels::panels::prompt_library::PromptLibraryData::load();
        status_bar.active_project = format!(
            "{} [{}]",
            project_name,
            project_root.display()
        );

        // Scan for project knowledge files (HIVE.md, README.md, etc.) and
        // store them as a global for injection into the AI context window.
        let knowledge_sources = hive_ai::KnowledgeFileScanner::scan(&project_root);
        if !knowledge_sources.is_empty() {
            info!(
                "Loaded {} project knowledge file(s) from {}",
                knowledge_sources.len(),
                project_root.display()
            );
        }
        cx.set_global(AppKnowledgeFiles(knowledge_sources));

        // Build the fast-path project index (<3s) for immediate AI context.
        let quick_index = hive_ai::quick_index::QuickIndex::build(&project_root);
        info!(
            "QuickIndex: {} files, {} symbols, {} deps in {:?}",
            quick_index.file_tree.total_files,
            quick_index.key_symbols.len(),
            quick_index.dependencies.len(),
            quick_index.indexed_at.elapsed()
        );
        cx.set_global(AppQuickIndex(std::sync::Arc::new(quick_index)));

        // Initialize context selection state (files checked in Files panel).
        cx.set_global(AppContextSelection(
            std::sync::Arc::new(std::sync::Mutex::new(ContextSelectionState::default())),
        ));

        // Create the interactive chat input entity.
        let chat_input = cx.new(|cx| ChatInputView::new(window, cx));

        // When the user submits a message, feed it into the send flow.
        cx.subscribe_in(
            &chat_input,
            window,
            |this, _view, event: &SubmitMessage, window, cx| {
                this.handle_send_text(event.text.clone(), event.context_files.clone(), window, cx);
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
                    this.handle_terminal_submit(
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
                this.handle_settings_save_from_view(cx);
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
                this.handle_shield_config_save(cx);
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
                this.handle_project_models_changed(&event.0, cx);
            },
        )
        .detach();

        // Create the workflow builder view entity.
        let workflow_builder_view = cx.new(|cx| WorkflowBuilderView::new(window, cx));
        cx.subscribe_in(
            &workflow_builder_view,
            window,
            |this, _view, event: &WorkflowSaved, _window, cx| {
                info!("Workflow saved: {}", event.0);
                this.refresh_agents_data(cx);
            },
        )
        .detach();
        cx.subscribe_in(
            &workflow_builder_view,
            window,
            |this, _view, event: &WorkflowRunRequested, _window, cx| {
                info!("Workflow run requested: {}", event.0);
                this.handle_workflow_builder_run(event.0.clone(), cx);
            },
        )
        .detach();

        // Create the channels view entity.
        let channels_view = cx.new(|cx| ChannelsView::new(window, cx));
        cx.subscribe_in(
            &channels_view,
            window,
            |this, _view, event: &ChannelMessageSent, _window, cx| {
                info!("Channel message sent in {}: {}", event.channel_id, event.content);

                // Persist user message to the channel store.
                if cx.has_global::<AppChannels>() {
                    let user_msg = hive_core::channels::ChannelMessage {
                        id: uuid::Uuid::new_v4().to_string(),
                        author: hive_core::channels::MessageAuthor::User,
                        content: event.content.clone(),
                        timestamp: chrono::Utc::now(),
                        thread_id: None,
                        model: None,
                        cost: None,
                    };
                    cx.global_mut::<AppChannels>().0.add_message(&event.channel_id, user_msg);
                }

                this.handle_channel_agent_responses(
                    event.channel_id.clone(),
                    event.content.clone(),
                    event.assigned_agents.clone(),
                    cx,
                );
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
                    new_id = cx
                        .global_mut::<AppChannels>()
                        .0
                        .create_channel(&event.name, icon, &description, event.agents.clone());
                }

                // Refresh the channels view to show the new channel and select it.
                this.refresh_channels_view(cx);
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
        let quick_start_data = Self::build_quick_start_data(
            &project_root,
            &project_name,
            &chat_service,
            "dogfood",
            None,
            cx,
        );
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
        Self {
            theme,
            sidebar,
            status_bar,
            recent_workspace_roots,
            pinned_workspace_roots,
            show_project_dropdown: false,
            current_project_root: project_root,
            current_project_name: project_name,
            chat_input,
            quick_start_goal_input,
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
        }
    }

    /// Resolve a `HiveTheme` from a theme name string.
    ///
    /// * `"dark"` / `"light"` map to the built-in constructors.
    /// * Any other value is matched (case-insensitive) against the
    ///   `ThemeManager::builtin_themes()` catalog and custom themes on disk.
    /// * Falls back to `HiveTheme::dark()` if no match is found.
    fn resolve_theme_by_name(name: &str) -> HiveTheme {
        let lower = name.to_lowercase();
        let mut theme = match lower.as_str() {
            "dark" | "hivecode dark" => HiveTheme::dark(),
            "light" | "hivecode light" => HiveTheme::light(),
            _ => {
                // Search built-in themes first.
                for def in ThemeManager::builtin_themes() {
                    if def.name.to_lowercase() == lower {
                        return HiveTheme::from_definition(&def);
                    }
                }
                // Try loading from custom themes on disk.
                if let Ok(mgr) = ThemeManager::new() {
                    for def in mgr.list_custom_themes() {
                        if def.name.to_lowercase() == lower {
                            return HiveTheme::from_definition(&def);
                        }
                    }
                }
                // Fallback
                HiveTheme::dark()
            }
        };
        // Always enforce text/bg contrast regardless of theme source.
        theme.ensure_contrast();
        theme
    }

    /// Sync HiveTheme text/background colors into gpui-component's Theme global
    /// so that built-in components (Input, etc.) render with correct colors.
    fn sync_gpui_theme(theme: &HiveTheme, cx: &mut App) {
        if cx.has_global::<GpuiTheme>() {
            let gpui_theme = GpuiTheme::global_mut(cx);
            gpui_theme.foreground = theme.text_primary;
            gpui_theme.muted_foreground = theme.text_muted;
            gpui_theme.background = theme.bg_primary;
            gpui_theme.input = theme.bg_surface;
        }
    }

    fn resolve_project_root_from_session(session: &SessionState) -> PathBuf {
        let fallback = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let requested = session
            .working_directory
            .as_deref()
            .map(PathBuf::from)
            .unwrap_or_else(|| fallback.clone());

        let requested = if requested.exists() { requested } else { fallback };
        Self::discover_project_root(&requested)
    }

    fn load_recent_workspace_roots(
        session: &SessionState,
        current_project_root: &Path,
    ) -> Vec<PathBuf> {
        let mut recents = Vec::new();
        let current_root = Self::discover_project_root(current_project_root);
        recents.push(current_root);

        for path in &session.recent_workspaces {
            let path = PathBuf::from(path);
            if !path.exists() {
                continue;
            }

            let root = Self::discover_project_root(&path);
            if !recents.contains(&root) {
                recents.push(root);
            }
        }

        recents.truncate(MAX_RECENT_WORKSPACES);
        recents
    }

    fn load_pinned_workspace_roots(session: &SessionState) -> Vec<PathBuf> {
        session
            .pinned_workspaces
            .iter()
            .filter_map(|p| {
                let path = PathBuf::from(p);
                if path.exists() { Some(path) } else { None }
            })
            .take(MAX_PINNED_WORKSPACES)
            .collect()
    }

    fn record_recent_workspace(&mut self, workspace_root: &Path, cx: &mut Context<Self>) {
        if !workspace_root.exists() {
            return;
        }

        let project_root = Self::discover_project_root(workspace_root);
        let mut changed = false;

        if let Some(existing) = self
            .recent_workspace_roots
            .iter()
            .position(|path| path == &project_root)
        {
            if existing == 0 {
                return;
            }
            self.recent_workspace_roots.remove(existing);
            changed = true;
        }

        if !self.recent_workspace_roots.contains(&project_root) {
            changed = true;
        }

        if !changed {
            return;
        }

        self.recent_workspace_roots.insert(0, project_root);
        self.recent_workspace_roots
            .truncate(MAX_RECENT_WORKSPACES);

        self.session_dirty = true;
        self.save_session(cx);
        cx.notify();
    }

    fn discover_project_root(path: &Path) -> PathBuf {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let mut current = canonical.as_path();

        while let Some(parent) = current.parent() {
            if current.join(".git").exists() {
                return current.to_path_buf();
            }
            current = parent;
        }

        if canonical.join(".git").exists() {
            return canonical;
        }

        canonical
    }

    fn project_name_from_path(path: &Path) -> String {
        path.file_name()
            .unwrap_or(path.as_os_str())
            .to_string_lossy()
            .to_string()
    }

    fn project_label(&self) -> String {
        format!("{} [{}]", self.current_project_name, self.current_project_root.display())
    }

    fn start_background_project_indexing(&self, cx: &mut Context<Self>) {
        let hive_mem = cx.has_global::<AppHiveMemory>()
            .then(|| cx.global::<AppHiveMemory>().0.clone());
        let rag_service = cx.has_global::<AppRagService>()
            .then(|| cx.global::<AppRagService>().0.clone());

        if hive_mem.is_none() && rag_service.is_none() {
            return;
        }

        let project_root = self.current_project_root.clone();
        std::thread::Builder::new()
            .name("hive-indexer".into())
            .spawn(move || {
                let entries = hive_ai::memory::BackgroundIndexer::collect_indexable_files(
                    &project_root,
                );
                let path_str = project_root.to_string_lossy().to_string();
                let indexed_files: Vec<(String, String)> = entries
                    .iter()
                    .filter_map(|entry_path| {
                        std::fs::read_to_string(entry_path).ok().map(|content| {
                            let rel = entry_path
                                .strip_prefix(&project_root)
                                .unwrap_or(entry_path)
                                .to_string_lossy()
                                .to_string();
                            (rel, content)
                        })
                    })
                    .collect();

                if let Some(rag_service) = rag_service
                    && let Ok(mut rag) = rag_service.lock()
                {
                    rag.clear_index();
                    for (rel, content) in &indexed_files {
                        rag.index_file(rel, content);
                    }
                    info!(
                        "RAG indexer: indexed {}/{} files from {}",
                        indexed_files.len(),
                        entries.len(),
                        path_str
                    );
                }

                if let Some(hive_mem) = hive_mem {
                    let rt = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build();
                    if let Ok(rt) = rt {
                        rt.block_on(async {
                            let mem = hive_mem.lock().await;
                            let mut count = 0usize;
                            for (rel, content) in &indexed_files {
                                if mem.index_file(rel, content).await.is_ok() {
                                    count += 1;
                                }
                            }
                            info!(
                                "Background indexer: indexed {count}/{} files from {}",
                                entries.len(),
                                path_str
                            );
                        });
                    }
                }
            })
            .ok();
    }

    fn apply_project_context(&mut self, cwd: &Path, cx: &mut Context<Self>) {
        let project_root = Self::discover_project_root(cwd);
        if project_root != self.current_project_root {
            self.current_project_root = project_root;
            self.current_project_name = Self::project_name_from_path(&self.current_project_root);
            self.status_bar.active_project = self.project_label();
            self.session_dirty = true;
            self.save_session(cx);

            // Re-scan knowledge files for the new project root.
            let knowledge_sources = hive_ai::KnowledgeFileScanner::scan(&self.current_project_root);
            if !knowledge_sources.is_empty() {
                info!(
                    "Re-scanned {} project knowledge file(s) for {}",
                    knowledge_sources.len(),
                    self.current_project_root.display()
                );
            }
            cx.set_global(AppKnowledgeFiles(knowledge_sources));

            // Rebuild the fast-path project index for the new project root.
            let quick_index = hive_ai::quick_index::QuickIndex::build(&self.current_project_root);
            info!(
                "QuickIndex rebuilt: {} files, {} symbols, {} deps",
                quick_index.file_tree.total_files,
                quick_index.key_symbols.len(),
                quick_index.dependencies.len()
            );
            cx.set_global(AppQuickIndex(std::sync::Arc::new(quick_index)));
            self.start_background_project_indexing(cx);

            // Start incremental file watcher for RAG indexing.
            let rag_for_watcher = cx.has_global::<AppRagService>()
                .then(|| cx.global::<AppRagService>().0.clone());
            if let Some(rag_svc) = rag_for_watcher {
                let project_root = self.current_project_root.clone();
                match hive_fs::FileWatcher::new(&self.current_project_root, move |event| {
                    let path = match &event {
                        hive_fs::WatchEvent::Created(p) | hive_fs::WatchEvent::Modified(p) => Some(p.clone()),
                        hive_fs::WatchEvent::Renamed { to, .. } => Some(to.clone()),
                        hive_fs::WatchEvent::Deleted(_) => None,
                    };
                    if let Some(path) = path {
                        // Only index files with common code extensions.
                        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                        let indexable = matches!(
                            ext,
                            "rs" | "py" | "js" | "ts" | "tsx" | "jsx" | "go" | "java"
                            | "c" | "cpp" | "h" | "hpp" | "rb" | "swift" | "kt"
                            | "md" | "txt" | "toml" | "yaml" | "yml" | "json"
                        );
                        if indexable {
                            if let Ok(content) = std::fs::read_to_string(&path) {
                                let rel = path
                                    .strip_prefix(&project_root)
                                    .unwrap_or(&path)
                                    .to_string_lossy()
                                    .to_string();
                                if let Ok(mut rag) = rag_svc.lock() {
                                    rag.index_file(&rel, &content);
                                    tracing::debug!("RAG watcher: indexed {rel}");
                                }
                            }
                        }
                    }
                }) {
                    Ok(watcher) => {
                        self._file_watcher = Some(watcher);
                        info!("RAG file watcher started for {}", self.current_project_root.display());
                    }
                    Err(e) => {
                        warn!("RAG file watcher failed to start: {e}");
                        self._file_watcher = None;
                    }
                }
            }

            self.refresh_quick_start_data(cx);

            cx.notify();
        } else if self.current_project_name.is_empty() {
            self.current_project_name = Self::project_name_from_path(&self.current_project_root);
            self.status_bar.active_project = self.project_label();
            self.refresh_quick_start_data(cx);
            cx.notify();
        }

        let project_root = self.current_project_root.clone();
        self.record_recent_workspace(&project_root, cx);
    }

    fn switch_to_workspace(&mut self, workspace_path: PathBuf, cx: &mut Context<Self>) {
        if !workspace_path.exists() {
            return;
        }
        self.apply_project_context(&workspace_path, cx);
        self.files_data = FilesData::from_path(&self.current_project_root);
        self.switch_to_panel(Panel::Files, cx);
    }

    pub fn set_active_panel(&mut self, panel: Panel) {
        self.sidebar.active_panel = panel;
        self.session_dirty = true;
    }

    /// Override the version shown in the status bar (called from hive_app
    /// which has access to the git-based HIVE_VERSION).
    pub fn set_version(&mut self, version: String) {
        self.status_bar.version = version;
    }

    // -- History data --------------------------------------------------------

    pub fn refresh_history(&mut self) {
        self.history_data = Self::load_history_data();
    }

    fn refresh_learning_data(&mut self, cx: &App) {
        use hive_ui_panels::panels::learning::*;

        if !cx.has_global::<AppLearning>() {
            return;
        }
        let learning = &cx.global::<AppLearning>().0;

        let log_entries = learning
            .learning_log(20)
            .unwrap_or_default()
            .into_iter()
            .map(|e| LogEntryDisplay {
                event_type: e.event_type,
                description: e.description,
                timestamp: e.timestamp,
            })
            .collect();

        let preferences = learning
            .all_preferences()
            .unwrap_or_default()
            .into_iter()
            .map(|(key, value, confidence)| PreferenceDisplay {
                key,
                value,
                confidence,
            })
            .collect();

        let routing_insights = learning
            .routing_learner
            .current_adjustments()
            .into_iter()
            .map(|adj| RoutingInsightDisplay {
                task_type: adj.task_type,
                from_tier: adj.from_tier,
                to_tier: adj.to_tier,
                confidence: adj.confidence,
            })
            .collect();

        let eval = learning.self_evaluator.evaluate().ok();

        self.learning_data = LearningPanelData {
            metrics: QualityMetrics {
                overall_quality: eval.as_ref().map_or(0.0, |e| e.overall_quality),
                trend: eval
                    .as_ref()
                    .map_or("Stable".into(), |e| format!("{:?}", e.trend)),
                total_interactions: learning.interaction_count(),
                correction_rate: eval.as_ref().map_or(0.0, |e| e.correction_rate),
                regeneration_rate: eval.as_ref().map_or(0.0, |e| e.regeneration_rate),
                cost_efficiency: eval.as_ref().map_or(0.0, |e| e.cost_per_quality_point),
            },
            log_entries,
            preferences,
            prompt_suggestions: Vec::new(),
            routing_insights,
            weak_areas: eval.as_ref().map_or(Vec::new(), |e| e.weak_areas.clone()),
            best_model: eval.as_ref().and_then(|e| e.best_model.clone()),
            worst_model: eval.as_ref().and_then(|e| e.worst_model.clone()),
        };
    }

    fn refresh_shield_data(&mut self, cx: &mut Context<Self>) {
        if cx.has_global::<AppShield>() {
            let shield = &cx.global::<AppShield>().0;
            self.shield_data.enabled = true;
            self.shield_data.pii_detections = shield.pii_detection_count();
            self.shield_data.secrets_blocked = shield.secrets_blocked_count();
            self.shield_data.threats_caught = shield.threats_caught_count();
        }
        // Populate interactive toggle states from config.
        if cx.has_global::<AppConfig>() {
            let cfg = cx.global::<AppConfig>().0.get();
            self.shield_data.shield_enabled = cfg.shield_enabled;
            self.shield_data.secret_scan_enabled = cfg.shield.enable_secret_scan;
            self.shield_data.vulnerability_check_enabled = cfg.shield.enable_vulnerability_check;
            self.shield_data.pii_detection_enabled = cfg.shield.enable_pii_detection;
            self.shield_data.user_rules = cfg.shield.user_rules.clone();
        }
        // Push data to the ShieldView entity.
        self.shield_view.update(cx, |view, _cx| {
            view.update_from_data(&self.shield_data);
        });
    }

    /// Handle shield config changes from the ShieldView panel.
    fn handle_shield_config_save(&mut self, cx: &mut Context<Self>) {
        let snapshot = self.shield_view.read(cx).collect_shield_config();

        if cx.has_global::<AppConfig>() {
            let _ = cx.global::<AppConfig>().0.update(|cfg| {
                cfg.shield_enabled = snapshot.shield_enabled;
                cfg.shield.enable_secret_scan = snapshot.secret_scan_enabled;
                cfg.shield.enable_vulnerability_check = snapshot.vulnerability_check_enabled;
                cfg.shield.enable_pii_detection = snapshot.pii_detection_enabled;
                cfg.shield.user_rules = snapshot.user_rules.clone();
            });
        }

        // Rebuild the live HiveShield with the new config.
        if cx.has_global::<AppConfig>() {
            let cfg = cx.global::<AppConfig>().0.get();
            let new_shield =
                std::sync::Arc::new(hive_shield::HiveShield::new(cfg.shield.clone()));
            cx.set_global(AppShield(new_shield));
        }
    }

    fn refresh_routing_data(&mut self, cx: &App) {
        if cx.has_global::<AppAiService>() {
            self.routing_data = RoutingData::from_router(cx.global::<AppAiService>().0.router());
        }
    }

    /// Populate the monitor panel with real system metrics and provider status.
    ///
    /// Reads CPU, memory, and disk stats via macOS-compatible commands (`sysctl`,
    /// `ps`, `df`) and falls back to zero values when a metric cannot be read.
    /// Provider status is derived from the current `AppConfig` API key fields.
    fn refresh_monitor_data(&mut self, cx: &App) {
        use hive_ui_panels::panels::monitor::ProviderStatus;

        // -- System resources --------------------------------------------------
        let resources = self.gather_system_resources();
        self.monitor_data.resources = resources;

        // -- Provider status ---------------------------------------------------
        if cx.has_global::<AppConfig>() {
            let config = cx.global::<AppConfig>().0.get();
            let mut providers: Vec<ProviderStatus> = Vec::new();

            let has_anthropic = config.anthropic_api_key.as_ref().is_some_and(|k| !k.is_empty());
            providers.push(ProviderStatus::new("Anthropic", has_anthropic, if has_anthropic { Some(0) } else { None }));

            let has_openai = config.openai_api_key.as_ref().is_some_and(|k| !k.is_empty());
            providers.push(ProviderStatus::new("OpenAI", has_openai, if has_openai { Some(0) } else { None }));

            let has_google = config.google_api_key.as_ref().is_some_and(|k| !k.is_empty());
            providers.push(ProviderStatus::new("Google Gemini", has_google, if has_google { Some(0) } else { None }));

            let has_openrouter = config.openrouter_api_key.as_ref().is_some_and(|k| !k.is_empty());
            providers.push(ProviderStatus::new("OpenRouter", has_openrouter, if has_openrouter { Some(0) } else { None }));

            let has_groq = config.groq_api_key.as_ref().is_some_and(|k| !k.is_empty());
            providers.push(ProviderStatus::new("Groq", has_groq, if has_groq { Some(0) } else { None }));

            let has_ollama = !config.ollama_url.is_empty();
            providers.push(ProviderStatus::new("Ollama (local)", has_ollama, if has_ollama { Some(0) } else { None }));

            let has_lmstudio = !config.lmstudio_url.is_empty();
            providers.push(ProviderStatus::new("LM Studio", has_lmstudio, if has_lmstudio { Some(0) } else { None }));

            if config.local_provider_url.as_ref().is_some_and(|url| !url.is_empty()) {
                providers.push(ProviderStatus::new("Custom Local", true, Some(0)));
            }

            self.monitor_data.providers = providers;
        }

        // -- Background tasks from active agent runs + completed swarms --------
        self.monitor_data.background_tasks = self
            .agents_data
            .active_runs
            .iter()
            .filter(|r| r.is_active() && r.has_task_detail())
            .map(|r| {
                use hive_ui_panels::components::task_tree::TaskTreeState;
                TaskTreeState {
                    title: r.spec_title.clone(),
                    plan_id: r.id.clone(),
                    tasks: r.tasks.clone(),
                    collapsed: false,
                    total_cost: r.cost,
                    elapsed_ms: 0,
                }
            })
            .collect::<Vec<_>>();
        // Append completed swarm task trees so they remain visible in the
        // monitor panel after execution finishes.
        self.monitor_data
            .background_tasks
            .extend(self.swarm_task_trees.iter().cloned());

        // -- Uptime (seconds since process start) -----------------------------
        if let Ok(output) = std::process::Command::new("ps")
            .args(["-o", "etime=", "-p", &std::process::id().to_string()])
            .output()
            && output.status.success()
        {
            let raw = String::from_utf8_lossy(&output.stdout).trim().to_string();
            self.monitor_data.uptime_secs = parse_etime(&raw);
        }
    }

    /// Refresh the logs panel.  On first visit (when the in-memory log list is
    /// empty), loads persisted entries from the SQLite database so logs survive
    /// app restarts.  Subsequent visits are no-ops because new entries are
    /// already pushed in real-time via `logs_data.add_entry()`.
    fn refresh_logs_data(&mut self, cx: &App) {
        if !self.logs_data.entries.is_empty() {
            return;
        }
        if !cx.has_global::<AppDatabase>() {
            return;
        }
        let db = &cx.global::<AppDatabase>().0;
        match db.recent_logs(500, 0) {
            Ok(rows) => {
                use hive_ui_panels::panels::logs::LogLevel;
                // Rows come newest-first from DB; reverse so oldest is first in the vec.
                for row in rows.into_iter().rev() {
                    let level = LogLevel::from_str_lossy(&row.level);
                    self.logs_data.add_entry(level, row.source, row.message);
                }
            }
            Err(e) => {
                warn!("Failed to load persisted logs: {e}");
            }
        }
    }

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

    /// Load Kanban board state from `~/.hive/kanban.json`.  If the file does
    /// not exist or cannot be parsed the board starts empty.
    fn refresh_kanban_data(&mut self) {
        let path = match hive_core::config::HiveConfig::base_dir() {
            Ok(d) => d.join("kanban.json"),
            Err(_) => return,
        };
        if !path.exists() {
            return;
        }
        match std::fs::read_to_string(&path) {
            Ok(json) => match serde_json::from_str::<hive_ui_panels::panels::kanban::KanbanData>(&json) {
                Ok(data) => {
                    self.kanban_data = data;
                }
                Err(e) => {
                    warn!("Failed to parse kanban.json: {e}");
                }
            },
            Err(e) => {
                warn!("Failed to read kanban.json: {e}");
            }
        }
    }

    /// Persist Kanban board state to `~/.hive/kanban.json`.
    fn save_kanban_data(&self) {
        let path = match hive_core::config::HiveConfig::base_dir() {
            Ok(d) => d.join("kanban.json"),
            Err(e) => {
                warn!("Cannot save kanban: {e}");
                return;
            }
        };
        match serde_json::to_string_pretty(&self.kanban_data) {
            Ok(json) => {
                if let Err(e) = std::fs::write(&path, json) {
                    warn!("Failed to write kanban.json: {e}");
                }
            }
            Err(e) => {
                warn!("Failed to serialize kanban data: {e}");
            }
        }
    }

    fn refresh_skills_data(&mut self, cx: &App) {
        use hive_ui_panels::panels::skills::{
            DirectorySkill, InstalledSkill as UiSkill, SkillCategory as UiCat, SkillSource as UiSource,
        };

        let mut installed = Vec::new();

        // Built-in skills from the registry.
        if cx.has_global::<hive_ui_core::AppSkills>() {
            for skill in cx.global::<hive_ui_core::AppSkills>().0.list() {
                installed.push(UiSkill {
                    id: format!("builtin:{}", skill.name),
                    name: skill.name.clone(),
                    description: skill.description.clone(),
                    version: "built-in".into(),
                    enabled: skill.enabled,
                    integrity_hash: skill.integrity_hash.clone(),
                });
            }
        }

        // User-created skills from SkillManager (file-based).
        if cx.has_global::<AppSkillManager>() {
            let mgr = &cx.global::<AppSkillManager>().0;
            if let Ok(user_skills) = mgr.list() {
                for skill in user_skills {
                    installed.push(UiSkill {
                        id: format!("user:{}", skill.name),
                        name: skill.name.clone(),
                        description: skill.description.clone(),
                        version: "custom".into(),
                        enabled: skill.enabled,
                        integrity_hash: String::new(),
                    });
                }
            }
        }

        // Marketplace-installed skills.
        let mut installed_triggers: Vec<String> = Vec::new();
        if cx.has_global::<AppMarketplace>() {
            let mp = &cx.global::<AppMarketplace>().0;
            for skill in mp.list_installed() {
                installed.push(UiSkill {
                    id: skill.id.clone(),
                    name: skill.name.clone(),
                    description: skill.description.clone(),
                    version: skill.installed_at.format("%Y-%m-%d").to_string(),
                    enabled: skill.enabled,
                    integrity_hash: skill.integrity_hash.clone(),
                });
                installed_triggers.push(skill.trigger.clone());
            }

            // Populate sources from marketplace.
            self.skills_data.sources = mp
                .list_sources()
                .iter()
                .map(|s| UiSource {
                    url: s.url.clone(),
                    name: s.name.clone(),
                    skill_count: 0, // count not tracked per-source yet
                })
                .collect();
        }

        // Populate installed plugins for the UI.
        if cx.has_global::<AppMarketplace>() {
            let mp = &cx.global::<AppMarketplace>().0;
            self.skills_data.installed_plugins = mp.installed_plugins()
                .iter()
                .map(|p| {
                    use hive_ui_panels::panels::skills::{UiInstalledPlugin, UiPluginSkill};
                    UiInstalledPlugin {
                        id: p.id.clone(),
                        name: p.name.clone(),
                        version: p.version.clone(),
                        author: p.author.name.clone(),
                        description: p.description.clone(),
                        skills: p.skills.iter().map(|s| UiPluginSkill {
                            name: s.name.clone(),
                            description: s.description.clone(),
                            enabled: s.enabled,
                        }).collect(),
                        expanded: false,
                        update_available: None,
                    }
                })
                .collect();
        }

        // Populate directory from all connected skill sources.
        let catalog = hive_agents::skill_marketplace::SkillMarketplace::default_directory();
        let mut directory = Vec::new();
        for (idx, available) in catalog.iter().enumerate() {
            use hive_agents::skill_marketplace::SkillCategory as MpCat;
            let ui_category = match available.category {
                MpCat::CodeGeneration => UiCat::CodeQuality,
                MpCat::Documentation => UiCat::Documentation,
                MpCat::Testing => UiCat::Testing,
                MpCat::Security => UiCat::Security,
                MpCat::Refactoring => UiCat::Productivity,
                MpCat::Analysis => UiCat::Other,
                MpCat::Communication => UiCat::Productivity,
                MpCat::Custom => UiCat::Other,
            };
            // Derive author from the repo URL domain.
            let author = if available.repo_url.contains("anthropic.com") {
                "Anthropic"
            } else if available.repo_url.contains("openai.com") {
                "OpenAI"
            } else if available.repo_url.contains("google.dev") {
                "Google"
            } else if available.repo_url.contains("hive-community") {
                "Community"
            } else {
                "ClawdHub"
            };
            let is_installed = installed_triggers.contains(&available.trigger);
            directory.push(DirectorySkill {
                id: available.name.clone(),
                name: available.name.clone(),
                description: available.description.clone(),
                author: author.to_string(),
                version: "1.0.0".to_string(),
                downloads: (12_400 - idx * 300).max(800),
                rating: 4.8 - (idx as f32 * 0.03),
                category: ui_category,
                installed: is_installed,
            });
        }
        self.skills_data.directory = directory;
        self.skills_data.installed = installed;

        // Add default skill sources if none are configured.
        if self.skills_data.sources.is_empty() {
            let clawdhub_count = catalog.iter().filter(|s| s.repo_url.contains("clawdhub.hive.dev")).count();
            let anthropic_count = catalog.iter().filter(|s| s.repo_url.contains("anthropic.com")).count();
            let openai_count = catalog.iter().filter(|s| s.repo_url.contains("openai.com")).count();
            let google_count = catalog.iter().filter(|s| s.repo_url.contains("google.dev")).count();
            let community_count = catalog.iter().filter(|s| s.repo_url.contains("hive-community")).count();
            self.skills_data.sources.extend([
                UiSource {
                    url: "https://clawdhub.hive.dev/registry".into(),
                    name: "ClawdHub".into(),
                    skill_count: clawdhub_count,
                },
                UiSource {
                    url: "https://skills.anthropic.com".into(),
                    name: "Anthropic Official".into(),
                    skill_count: anthropic_count,
                },
                UiSource {
                    url: "https://skills.openai.com".into(),
                    name: "OpenAI Official".into(),
                    skill_count: openai_count,
                },
                UiSource {
                    url: "https://skills.google.dev".into(),
                    name: "Google Official".into(),
                    skill_count: google_count,
                },
                UiSource {
                    url: "https://github.com/hive-community/skills".into(),
                    name: "Community".into(),
                    skill_count: community_count,
                },
            ]);
        }
    }

    /// Trigger a background version check for installed plugins (throttled to 1hr).
    fn trigger_plugin_version_check(&mut self, cx: &mut Context<Self>) {
        use hive_agents::plugin_types::PluginCache;

        if !cx.has_global::<AppPluginManager>() || !cx.has_global::<AppMarketplace>() {
            return;
        }

        let pm = cx.global::<AppPluginManager>().0.clone();
        let plugins: Vec<_> = cx.global::<AppMarketplace>().0.installed_plugins().to_vec();

        if plugins.is_empty() {
            return;
        }

        // Load cache from disk.
        let cache_path = dirs::home_dir()
            .unwrap_or_default()
            .join(".hive")
            .join("plugin_cache.json");
        let mut cache = if cache_path.exists() {
            std::fs::read_to_string(&cache_path)
                .ok()
                .and_then(|s| serde_json::from_str::<PluginCache>(&s).ok())
                .unwrap_or_default()
        } else {
            PluginCache::default()
        };

        // Check if throttle period has elapsed (1 hour).
        if let Some(last) = cache.last_checked {
            if (chrono::Utc::now() - last).num_seconds() < 3600 {
                // Still within throttle window — skip network check but apply
                // cached update info to the UI.
                for ui_plugin in &mut self.skills_data.installed_plugins {
                    if let Some(cached) = cache.versions.get(&ui_plugin.id) {
                        if cached.latest_version != ui_plugin.version {
                            ui_plugin.update_available = Some(cached.latest_version.clone());
                        }
                    }
                }
                cx.notify();
                return;
            }
        }

        let result_flag: Arc<std::sync::Mutex<Option<(Vec<hive_agents::plugin_types::UpdateAvailable>, PluginCache)>>> =
            Arc::new(std::sync::Mutex::new(None));
        let result_for_thread = Arc::clone(&result_flag);
        let cache_path_for_thread = cache_path.clone();

        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build();
            if let Ok(rt) = rt {
                let updates = rt.block_on(pm.check_for_updates(&plugins, &mut cache));
                // Save updated cache to disk.
                if let Ok(json) = serde_json::to_string_pretty(&cache) {
                    let _ = std::fs::write(&cache_path_for_thread, json);
                }
                *result_for_thread.lock().unwrap_or_else(|e| e.into_inner()) =
                    Some((updates, cache));
            }
        });

        let result_for_ui = Arc::clone(&result_flag);
        cx.spawn(async move |this: WeakEntity<HiveWorkspace>, app: &mut AsyncApp| {
            loop {
                if let Some((updates, _cache)) = result_for_ui.lock().unwrap_or_else(|e| e.into_inner()).take() {
                    if !updates.is_empty() {
                        let _ = this.update(app, |this, cx| {
                            for update in &updates {
                                if let Some(ui_plugin) = this.skills_data.installed_plugins
                                    .iter_mut()
                                    .find(|p| p.id == update.plugin_id)
                                {
                                    ui_plugin.update_available = Some(update.latest_version.clone());
                                }
                            }
                            cx.notify();
                        });
                    }
                    break;
                }
                app.background_executor()
                    .timer(std::time::Duration::from_millis(500))
                    .await;
            }
        })
        .detach();
    }

    fn refresh_agents_data(&mut self, cx: &App) {
        use hive_ui_panels::panels::agents::{
            PersonaDisplay, RemoteAgentDisplay, RunDisplay, WorkflowDisplay,
        };

        if cx.has_global::<AppPersonas>() {
            let registry = &cx.global::<AppPersonas>().0;
            self.agents_data.personas = registry
                .all()
                .into_iter()
                .map(|p| PersonaDisplay {
                    name: p.name.clone(),
                    kind: format!("{:?}", p.kind),
                    description: p.description.clone(),
                    model_tier: format!("{:?}", p.model_tier),
                    active: false,
                })
                .collect();
        }

        self.agents_data.remote_agents.clear();
        if cx.has_global::<AppA2aClient>() {
            let client = &cx.global::<AppA2aClient>().0;
            if let Err(e) = client.reload() {
                warn!("Agents: failed to reload A2A config: {e}");
            }
            match client.list_agents() {
                Ok(remote_agents) => {
                    self.agents_data.remote_hint = Some(format!(
                        "{} configured remote agent(s) from {}",
                        remote_agents.len(),
                        client.config_path().display()
                    ));
                    self.agents_data.remote_agents = remote_agents
                        .into_iter()
                        .map(|agent| {
                            let description = agent
                                .description
                                .unwrap_or_else(|| format!("Remote A2A agent at {}", agent.url));
                            RemoteAgentDisplay {
                                name: agent.name,
                                url: agent.url,
                                description,
                                discovered: agent.discovered,
                                api_key_configured: agent.api_key_configured,
                                version: agent.version,
                                skills: agent.skills,
                            }
                        })
                        .collect();
                }
                Err(e) => {
                    self.agents_data.remote_hint = Some("Remote A2A config unavailable".into());
                    warn!("Agents: failed to list A2A agents: {e}");
                }
            }
        } else {
            self.agents_data.remote_hint = Some("A2A client unavailable".into());
        }

        if self.agents_data.remote_agents.is_empty() {
            self.agents_data.selected_remote_agent = None;
            self.agents_data.selected_remote_skill = None;
        } else {
            let selected_is_valid = self.agents_data.selected_remote_agent.as_ref().is_some_and(
                |selected| {
                    self.agents_data
                        .remote_agents
                        .iter()
                        .any(|agent| agent.name == *selected)
                },
            );
            if !selected_is_valid {
                self.agents_data.selected_remote_agent = self
                    .agents_data
                    .remote_agents
                    .first()
                    .map(|agent| agent.name.clone());
            }
            if let Some(selected_agent) = self.agents_data.selected_remote_agent.as_ref()
                && let Some(agent) = self
                    .agents_data
                    .remote_agents
                    .iter()
                    .find(|agent| agent.name == *selected_agent)
            {
                let skill_is_valid = self.agents_data.selected_remote_skill.as_ref().is_none_or(
                    |skill| agent.skills.iter().any(|candidate| candidate == skill),
                );
                if !skill_is_valid {
                    self.agents_data.selected_remote_skill = None;
                }
            }
            self.agents_data.personas.extend(
                self.agents_data
                    .remote_agents
                    .iter()
                    .map(|agent| PersonaDisplay {
                        name: agent.name.clone(),
                        kind: "remote_a2a".into(),
                        description: agent.description.clone(),
                        model_tier: "Remote".into(),
                        active: true,
                    }),
            );
        }

        if cx.has_global::<AppAutomation>() {
            let automation = &cx.global::<AppAutomation>().0;

            self.agents_data.workflows = automation
                .list_workflows()
                .iter()
                .map(|wf| WorkflowDisplay {
                    id: wf.id.clone(),
                    name: wf.name.clone(),
                    description: wf.description.clone(),
                    commands: Self::workflow_command_preview(wf),
                    source: if wf.id.starts_with("builtin:") {
                        "Built-in".into()
                    } else if wf.id.starts_with("file:") {
                        "User file".into()
                    } else {
                        "Runtime".into()
                    },
                    status: format!("{:?}", wf.status),
                    trigger: Self::trigger_label(&wf.trigger),
                    steps: wf.steps.len(),
                    run_count: wf.run_count as usize,
                    last_run: wf
                        .last_run
                        .as_ref()
                        .map(|ts: &chrono::DateTime<chrono::Utc>| {
                            ts.format("%Y-%m-%d %H:%M").to_string()
                        }),
                })
                .collect();

            self.agents_data.active_runs = automation
                .list_workflows()
                .iter()
                .filter(|wf| {
                    matches!(
                        wf.status,
                        hive_agents::automation::WorkflowStatus::Active
                            | hive_agents::automation::WorkflowStatus::Draft
                    )
                })
                .map(|wf| RunDisplay {
                    id: wf.id.clone(),
                    spec_title: wf.name.clone(),
                    status: format!("{:?}", wf.status),
                    progress: if wf.steps.is_empty() { 0.0 } else { 1.0 },
                    tasks_done: wf.steps.len(),
                    tasks_total: wf.steps.len(),
                    cost: 0.0,
                    elapsed: wf
                        .last_run
                        .as_ref()
                        .map(|_| "recent".to_string())
                        .unwrap_or_else(|| "-".to_string()),
                    tasks: vec![],
                    disclosure: Default::default(),
                })
                .collect();

            self.agents_data.run_history = automation
                .list_run_history()
                .iter()
                .rev()
                .take(8)
                .filter_map(|run| {
                    let workflow = automation.get_workflow(&run.workflow_id)?;
                    Some(RunDisplay {
                        id: run.workflow_id.clone(),
                        spec_title: workflow.name.clone(),
                        status: if run.success {
                            "Complete".into()
                        } else {
                            "Failed".into()
                        },
                        progress: if run.success { 1.0 } else { 0.0 },
                        tasks_done: run.steps_completed,
                        tasks_total: workflow.steps.len(),
                        cost: 0.0,
                        elapsed: format!(
                            "{}s",
                            (run.completed_at - run.started_at).num_seconds().max(0)
                        ),
                        tasks: vec![],
                        disclosure: Default::default(),
                    })
                })
                .collect();

            self.agents_data.workflow_source_dir = hive_agents::USER_WORKFLOW_DIR.to_string();
            self.agents_data.workflow_hint = Some(format!(
                "{} workflows loaded ({} active)",
                automation.workflow_count(),
                automation.active_count()
            ));
        }
    }

    fn workflow_command_preview(
        workflow: &hive_agents::automation::Workflow,
    ) -> Vec<String> {
        workflow
            .steps
            .iter()
            .filter_map(|step| match &step.action {
                hive_agents::automation::ActionType::RunCommand { command } => {
                    Some(command.to_string())
                }
                _ => None,
            })
            .collect()
    }

    fn trigger_label(trigger: &hive_agents::automation::TriggerType) -> String {
        match trigger {
            hive_agents::automation::TriggerType::ManualTrigger => "Manual".into(),
            hive_agents::automation::TriggerType::Schedule { cron } => {
                format!("Schedule ({cron})")
            }
            hive_agents::automation::TriggerType::FileChange { path } => {
                format!("File Change ({path})")
            }
            hive_agents::automation::TriggerType::WebhookReceived { event } => {
                format!("Webhook ({event})")
            }
            hive_agents::automation::TriggerType::OnMessage { pattern } => {
                format!("Message ({pattern})")
            }
            hive_agents::automation::TriggerType::OnError { source } => {
                format!("Error ({source})")
            }
        }
    }

    fn refresh_specs_data(&mut self, cx: &App) {
        use hive_ui_panels::panels::specs::SpecSummary;

        if cx.has_global::<AppSpecs>() {
            let manager = &cx.global::<AppSpecs>().0;
            self.specs_data.specs = manager
                .specs
                .values()
                .map(|s| SpecSummary {
                    id: s.id.clone(),
                    title: s.title.clone(),
                    status: format!("{:?}", s.status),
                    entries_total: s.entry_count(),
                    entries_checked: s.checked_count(),
                    updated_at: s.updated_at.format("%Y-%m-%d %H:%M").to_string(),
                })
                .collect();
        }
    }

    fn refresh_assistant_data(&mut self, cx: &App) {
        use hive_ui_panels::panels::assistant::{ActiveReminder, BriefingSummary, PendingApproval};

        if cx.has_global::<AppAssistant>() {
            let svc = &cx.global::<AppAssistant>().0;
            let briefing = svc.daily_briefing_for_project(Some(&self.current_project_root));

            self.assistant_data.briefing = Some(BriefingSummary {
                greeting: "Good morning!".into(),
                date: briefing.date.clone(),
                event_count: briefing.events.len(),
                unread_emails: briefing.email_summary.as_ref().map_or(0, |d| d.email_count),
                active_reminders: briefing.active_reminders.len(),
                top_priority: briefing.action_items.first().cloned(),
            });

            self.assistant_data.reminders = briefing
                .active_reminders
                .iter()
                .map(|r| ActiveReminder {
                    title: r.title.clone(),
                    due: match &r.trigger {
                        ReminderTrigger::At(at) => at.format("%Y-%m-%d %H:%M").to_string(),
                        ReminderTrigger::Recurring(expr) => {
                            format!("Recurring: {expr}")
                        }
                        ReminderTrigger::OnEvent(event) => {
                            format!("On event: {event}")
                        }
                    },
                    is_overdue: matches!(&r.trigger, ReminderTrigger::At(at) if *at <= Utc::now()),
                })
                .collect();

            if let Ok(pending) = svc.approval_service.list_pending() {
                self.assistant_data.approvals = pending
                    .iter()
                    .map(|a| PendingApproval {
                        id: a.id.clone(),
                        action: a.action.clone(),
                        resource: a.resource.clone(),
                        level: format!("{:?}", a.level),
                        requested_by: a.requested_by.clone(),
                        created_at: a.created_at.clone(),
                    })
                    .collect();
            }
        }
    }

    /// Kick off background async fetches to populate the assistant panel with
    /// real data from connected accounts (Gmail, Calendar, GitHub, etc.).
    ///
    /// Routes Gmail and Calendar fetches through `hive_assistant`'s
    /// `EmailService` / `CalendarService` so the logic lives in one place
    /// rather than duplicating the `hive_integrations` calls here.
    fn refresh_assistant_connected_data(&mut self, cx: &mut Context<Self>) {
        use hive_assistant::calendar::CalendarService;
        use hive_assistant::email::EmailService;
        use hive_core::config::AccountPlatform;
        use hive_ui_panels::panels::assistant::{
            EmailGroup, EmailPreview, UpcomingEvent,
        };

        if !cx.has_global::<AppConfig>() {
            return;
        }

        let config = cx.global::<AppConfig>().0.get();
        let connected = config.connected_accounts.clone();
        if connected.is_empty() {
            return;
        }

        // Gather OAuth tokens for connected platforms
        let mut tokens: Vec<(AccountPlatform, String)> = Vec::new();
        let config_mgr = &cx.global::<AppConfig>().0;
        for account in &connected {
            if let Some(token_data) = config_mgr.get_oauth_token(account.platform) {
                tokens.push((account.platform, token_data.access_token.clone()));
            }
        }

        if tokens.is_empty() {
            return;
        }

        // Keep the global AssistantService in sync with the latest tokens.
        if cx.has_global::<AppAssistant>() {
            let svc = &mut cx.global_mut::<AppAssistant>().0;
            for (platform, token) in &tokens {
                match platform {
                    AccountPlatform::Google => {
                        svc.set_gmail_token(token.clone());
                        svc.set_google_calendar_token(token.clone());
                    }
                    AccountPlatform::Microsoft => {
                        svc.set_outlook_token(token.clone());
                        svc.set_outlook_calendar_token(token.clone());
                    }
                    _ => {}
                }
            }
        }

        // Build lightweight service instances with current tokens for the
        // background thread.  These are cheap to create and avoid sending the
        // full AssistantService (which holds a database handle) across threads.
        let mut gmail_token: Option<String> = None;
        let mut outlook_token: Option<String> = None;
        let mut github_tokens: Vec<String> = Vec::new();

        for (platform, token) in &tokens {
            match platform {
                AccountPlatform::Google => gmail_token = Some(token.clone()),
                AccountPlatform::Microsoft => outlook_token = Some(token.clone()),
                AccountPlatform::GitHub => github_tokens.push(token.clone()),
                _ => {}
            }
        }

        let email_svc = EmailService::with_tokens(gmail_token.clone(), outlook_token.clone());
        let calendar_svc = CalendarService::with_tokens(gmail_token, outlook_token);

        // Spawn background thread with tokio runtime for async fetches
        let (tx, rx) = std::sync::mpsc::channel::<AssistantFetchResult>();

        std::thread::spawn(move || {
            let rt = match tokio::runtime::Runtime::new() {
                Ok(rt) => rt,
                Err(e) => {
                    warn!("Assistant: failed to create tokio runtime: {e}");
                    return;
                }
            };

            // Enter the runtime so that EmailService/CalendarService can
            // obtain a tokio Handle via Handle::try_current().
            let _guard = rt.enter();

            // --- Gmail (via EmailService) ---
            if let Ok(emails) = email_svc.fetch_gmail_inbox() {
                if !emails.is_empty() {
                    let previews: Vec<EmailPreviewData> = emails
                        .iter()
                        .map(|e| EmailPreviewData {
                            from: e.from.clone(),
                            subject: e.subject.clone(),
                            snippet: e.body.chars().take(120).collect(),
                            time: e.timestamp.clone(),
                            important: e.important,
                        })
                        .collect();
                    let _ = tx.send(AssistantFetchResult::Emails {
                        provider: "Gmail".into(),
                        previews,
                    });
                }
            }

            // --- Google Calendar + Outlook Calendar (via CalendarService) ---
            if let Ok(events) = calendar_svc.today_events() {
                if !events.is_empty() {
                    let upcoming: Vec<EventData> = events
                        .iter()
                        .map(|e| EventData {
                            title: e.title.clone(),
                            time: e.start.clone(),
                            location: e.location.clone(),
                        })
                        .collect();
                    let _ = tx.send(AssistantFetchResult::Events(upcoming));
                }
            }

            // --- Outlook email (via EmailService) ---
            if let Ok(emails) = email_svc.fetch_outlook_inbox() {
                if !emails.is_empty() {
                    let previews: Vec<EmailPreviewData> = emails
                        .iter()
                        .map(|e| EmailPreviewData {
                            from: e.from.clone(),
                            subject: e.subject.clone(),
                            snippet: e.body.chars().take(120).collect(),
                            time: e.timestamp.clone(),
                            important: e.important,
                        })
                        .collect();
                    let _ = tx.send(AssistantFetchResult::Emails {
                        provider: "Outlook".into(),
                        previews,
                    });
                }
            }

            // --- GitHub (still via hive_integrations directly -- not part of
            //     AssistantService) ---
            for token in &github_tokens {
                let client = match hive_integrations::GitHubClient::new(token.clone()) {
                    Ok(c) => c,
                    Err(_) => continue,
                };
                if let Ok(repos) = rt.block_on(client.list_repos())
                    && let Some(arr) = repos.as_array()
                {
                    let descriptions: Vec<String> = arr
                        .iter()
                        .take(5)
                        .filter_map(|r| {
                            let name = r.get("full_name")?.as_str()?;
                            Some(format!("Activity on {name}"))
                        })
                        .collect();
                    let _ = tx.send(AssistantFetchResult::RecentActions(descriptions));
                }
            }
        });

        // Poll for results from background thread via timer
        cx.spawn(async move |entity: WeakEntity<HiveWorkspace>, async_cx: &mut AsyncApp| {
            // Give background tasks time to complete
            Timer::after(std::time::Duration::from_secs(3)).await;
            let mut email_groups: Vec<(String, Vec<EmailPreviewData>)> = Vec::new();
            let mut events: Vec<EventData> = Vec::new();
            let mut actions: Vec<String> = Vec::new();

            while let Ok(result) = rx.try_recv() {
                match result {
                    AssistantFetchResult::Emails { provider, previews } => {
                        email_groups.push((provider, previews));
                    }
                    AssistantFetchResult::Events(evts) => {
                        events.extend(evts);
                    }
                    AssistantFetchResult::RecentActions(acts) => {
                        actions.extend(acts);
                    }
                }
            }

            {
                let _ = entity.update(async_cx, |ws: &mut HiveWorkspace, cx: &mut Context<HiveWorkspace>| {
                    // Apply email groups
                    for (provider, previews) in &email_groups {
                        ws.assistant_data.email_groups.push(EmailGroup {
                            provider: provider.clone(),
                            previews: previews
                                .iter()
                                .map(|p| EmailPreview {
                                    from: p.from.clone(),
                                    subject: p.subject.clone(),
                                    snippet: p.snippet.clone(),
                                    time: p.time.clone(),
                                    important: p.important,
                                })
                                .collect(),
                        });
                    }

                    // Apply events
                    for evt in &events {
                        ws.assistant_data.events.push(UpcomingEvent {
                            title: evt.title.clone(),
                            time: evt.time.clone(),
                            location: evt.location.clone(),
                            is_conflict: false,
                        });
                    }

                    // Apply recent actions
                    for act in &actions {
                        ws.assistant_data.recent_actions.push(
                            hive_ui_panels::panels::assistant::RecentAction {
                                description: act.clone(),
                                timestamp: "Now".into(),
                                action_type: "github".into(),
                            },
                        );
                    }

                    // Update briefing counts
                    if let Some(ref mut briefing) = ws.assistant_data.briefing {
                        let total_emails: usize =
                            email_groups.iter().map(|(_, p)| p.len()).sum();
                        briefing.unread_emails = total_emails;
                        briefing.event_count = ws.assistant_data.events.len();
                    }

                    cx.notify();
                });
            }
        })
        .detach();
    }

    fn refresh_workflow_builder(&mut self, cx: &mut Context<Self>) {
        use hive_ui_panels::panels::workflow_builder::WorkflowListEntry;

        if cx.has_global::<AppAutomation>() {
            let automation = &cx.global::<AppAutomation>().0;
            let workflows = automation.list_workflows();
            let entries: Vec<WorkflowListEntry> = workflows
                .iter()
                .map(|wf| WorkflowListEntry {
                    id: wf.id.clone(),
                    name: wf.name.clone(),
                    is_builtin: wf.id.starts_with("builtin:"),
                    status: format!("{:?}", wf.status),
                })
                .collect();

            self.workflow_builder_view.update(cx, |view, cx| {
                view.refresh_workflow_list(entries, cx);
            });
        }
    }

    fn refresh_channels_view(&mut self, cx: &mut Context<Self>) {
        if cx.has_global::<AppChannels>() {
            // Extract channel list data first to avoid borrow conflict.
            let channel_data: Vec<_> = cx
                .global::<AppChannels>()
                .0
                .list_channels()
                .iter()
                .map(|c| (
                    c.id.clone(),
                    c.name.clone(),
                    c.icon.clone(),
                    c.description.clone(),
                    c.messages.len(),
                    c.assigned_agents.clone(),
                ))
                .collect();

            self.channels_view.update(cx, |view, cx| {
                view.refresh_from_data(channel_data, cx);
            });
        }
    }

    fn refresh_cost_data(&mut self, cx: &App) {
        self.cost_data = if cx.has_global::<AppAiService>() {
            CostData::from_tracker(cx.global::<AppAiService>().0.cost_tracker())
        } else {
            CostData::empty()
        };
    }

    pub fn load_history_data() -> HistoryData {
        match hive_core::ConversationStore::new() {
            Ok(store) => {
                let summaries = store.list_summaries().unwrap_or_default();
                HistoryData::from_summaries(summaries)
            }
            Err(_) => HistoryData::empty(),
        }
    }

    // -- Session persistence -------------------------------------------------

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

    /// Initiate sending a user message and streaming the AI response.
    ///
    /// Called when `ChatInputView` emits `SubmitMessage`. The input has
    /// already been cleared by the view before this is invoked.
    ///
    /// 1. Records the text in `ChatService`.
    /// 2. Extracts the provider + request from the `AppAiService` global.
    /// 3. Spawns an async task that calls `provider.stream_chat()` and feeds
    ///    the resulting receiver back into `ChatService::attach_stream`.
    fn handle_send_text(&mut self, text: String, context_files: Vec<std::path::PathBuf>, window: &mut Window, cx: &mut Context<Self>) {
        if text.trim().is_empty() {
            return;
        }

        let model = self.chat_service.read(cx).current_model().to_string();

        // Shield: scan outgoing text before sending to AI.
        // Check if the shield is enabled in config.
        let shield_enabled = if cx.has_global::<AppConfig>() {
            cx.global::<AppConfig>().0.get().shield_enabled
        } else {
            true // default to enabled if no config
        };

        let send_text = if shield_enabled && cx.has_global::<AppShield>() {
            let shield = &cx.global::<AppShield>().0;
            let result = shield.process_outgoing(&text, &model);
            match result.action {
                hive_shield::ShieldAction::Allow => text,
                hive_shield::ShieldAction::CloakAndAllow(ref cloaked) => {
                    info!("Shield: PII cloaked in outgoing message");
                    cloaked.text.clone()
                }
                hive_shield::ShieldAction::Block(ref reason) => {
                    warn!("Shield: blocked outgoing message: {reason}");
                    self.chat_service.update(cx, |svc, cx| {
                        svc.set_error(format!("Message blocked by privacy shield: {reason}"), cx);
                    });
                    return;
                }
                hive_shield::ShieldAction::Warn(ref warning) => {
                    warn!("Shield: warning on outgoing message: {warning}");
                    text
                }
            }
        } else {
            text
        };

        // --- /swarm command intercept ------------------------------------------
        // `/swarm <goal>` dispatches the Queen meta-coordinator instead of the
        // normal chat flow.
        if send_text.trim().starts_with("/swarm ") {
            let goal = send_text.trim().strip_prefix("/swarm ").unwrap_or("").trim().to_string();
            if goal.is_empty() {
                self.chat_service.update(cx, |svc, cx| {
                    svc.set_error("Usage: /swarm <goal description>".to_string(), cx);
                });
                return;
            }

            // Record user message + create placeholder.
            self.chat_service.update(cx, |svc, cx| {
                svc.send_message(send_text, &model, cx);
            });

            // Resolve the AI provider to bridge into AiExecutor.
            let provider: Option<Arc<dyn AiProvider>> = if cx.has_global::<AppAiService>() {
                cx.global::<AppAiService>().0.first_provider()
            } else {
                None
            };
            let Some(provider) = provider else {
                self.chat_service.update(cx, |svc, cx| {
                    svc.set_error("No AI provider available for swarm execution", cx);
                });
                return;
            };

            // Optionally attach collective memory.
            let memory = if cx.has_global::<hive_ui_core::AppCollectiveMemory>() {
                Some(Arc::clone(&cx.global::<hive_ui_core::AppCollectiveMemory>().0))
            } else {
                None
            };

            // Optionally attach RAG service for pipeline context curation.
            let rag_service = cx.has_global::<AppRagService>()
                .then(|| cx.global::<AppRagService>().0.clone());

            let model_for_exec = model.clone();
            let chat_svc = self.chat_service.downgrade();
            cx.spawn(async move |_this, app: &mut AsyncApp| {
                // Bridge: wrap the provider as an AiExecutor.
                struct ProviderExecutor {
                    provider: Arc<dyn AiProvider>,
                    model: String,
                }
                impl hive_agents::AiExecutor for ProviderExecutor {
                    async fn execute(
                        &self,
                        request: &hive_ai::types::ChatRequest,
                    ) -> Result<hive_ai::types::ChatResponse, String> {
                        self.provider.chat(request).await.map_err(|e| e.to_string())
                    }
                }

                let executor = Arc::new(ProviderExecutor {
                    provider,
                    model: model_for_exec.clone(),
                });

                let mut queen =
                    hive_agents::Queen::new(hive_agents::swarm::SwarmConfig::default(), executor);
                if let Some(mem) = memory {
                    queen = queen.with_memory(mem);
                }
                if let Some(rag) = rag_service.clone() {
                    queen = queen.with_rag(rag);
                }

                let result_text = match queen.execute(&goal).await {
                    Ok(result) => {
                        // Convert team results into a TaskTreeState for the
                        // monitor panel's background tasks section.
                        use hive_ui_panels::components::task_tree::{
                            TaskDisplay, TaskDisplayStatus, TaskTreeState,
                        };
                        let tasks: Vec<TaskDisplay> = result
                            .team_results
                            .iter()
                            .map(|tr| {
                                let status = match tr.status {
                                    hive_agents::swarm::TeamStatus::Completed => {
                                        TaskDisplayStatus::Completed
                                    }
                                    hive_agents::swarm::TeamStatus::Failed => {
                                        TaskDisplayStatus::Failed(
                                            tr.error.clone().unwrap_or_default(),
                                        )
                                    }
                                    hive_agents::swarm::TeamStatus::Running => {
                                        TaskDisplayStatus::Running
                                    }
                                    _ => TaskDisplayStatus::Pending,
                                };
                                TaskDisplay {
                                    id: tr.team_id.clone(),
                                    description: tr.team_name.clone(),
                                    persona: "Swarm".into(),
                                    status,
                                    duration_ms: Some(tr.duration_ms),
                                    cost: Some(tr.cost),
                                    output_preview: tr.inner.as_ref().map(|i| {
                                        let s = match i {
                                            hive_agents::swarm::InnerResult::Native {
                                                content, ..
                                            }
                                            | hive_agents::swarm::InnerResult::SingleShot {
                                                content, ..
                                            } => content.clone(),
                                            _ => String::new(),
                                        };
                                        s.chars().take(200).collect()
                                    }),
                                    expanded: false,
                                    model_override: None,
                                }
                            })
                            .collect();
                        let tree = TaskTreeState {
                            title: format!("Swarm: {}", &result.goal),
                            plan_id: result.run_id.clone(),
                            tasks,
                            collapsed: false,
                            total_cost: result.total_cost,
                            elapsed_ms: result.total_duration_ms,
                        };
                        let _ = _this.update(app, |ws, _cx| {
                            ws.swarm_task_trees.push(tree);
                        });

                        format!(
                            "## Swarm Result\n\n\
                             **Goal:** {}\n\
                             **Status:** {:?}\n\
                             **Teams:** {}\n\
                             **Cost:** ${:.4}\n\
                             **Duration:** {}ms\n\n\
                             ---\n\n{}",
                            result.goal,
                            result.status,
                            result.team_results.len(),
                            result.total_cost,
                            result.total_duration_ms,
                            result.synthesized_output,
                        )
                    }
                    Err(e) => format!("Swarm execution failed: {e}"),
                };

                // Finalize the placeholder assistant message.
                let _ = app.update(|cx| {
                    if let Some(svc) = chat_svc.upgrade() {
                        svc.update(cx, |svc, _cx| {
                            let idx = svc.messages.len().saturating_sub(1);
                            svc.finalize_stream(idx, &result_text, &model_for_exec, None);
                        });
                    }
                });
            })
            .detach();

            return;
        }

        // Save the user text for RAG query before it is consumed by send_message.
        let user_query_text = send_text.clone();

        // 1. Record user message + create placeholder assistant message.
        self.chat_service.update(cx, |svc, cx| {
            svc.send_message(send_text, &model, cx);
        });

        // 2. Build the AI wire-format messages.
        let ai_messages = self.chat_service.read(cx).build_ai_messages();

        // 2b. Query RAG and SemanticSearch for relevant context and compile via ContextEngine
        let ai_messages = {
            let mut all_context = String::new();

            // Pull from RAG document chunks
            if cx.has_global::<AppRagService>() {
                if let Ok(rag_svc) = cx.global::<AppRagService>().0.lock() {
                    let rag_query = hive_ai::RagQuery {
                        query: user_query_text.clone(),
                        max_results: 10,
                        min_similarity: 0.1,
                    };
                    if let Ok(result) = rag_svc.query(&rag_query) {
                        if !result.context.is_empty() {
                            all_context.push_str(&result.context);
                            all_context.push_str("\n\n");
                        }
                    }
                }
            }

            if cx.has_global::<AppSemanticSearch>() {
                let mut candidate_paths = Vec::new();

                if cx.has_global::<AppQuickIndex>() {
                    let quick_index = &cx.global::<AppQuickIndex>().0;
                    let mut seen = std::collections::HashSet::new();
                    for symbol in quick_index.key_symbols.iter().take(32) {
                        let path = quick_index.project_root.join(&symbol.file);
                        if seen.insert(path.clone()) {
                            candidate_paths.push(path);
                        }
                    }
                }

                if candidate_paths.is_empty() {
                    candidate_paths.push(self.current_project_root.clone());
                }

                let candidate_refs: Vec<&std::path::Path> =
                    candidate_paths.iter().map(|path| path.as_path()).collect();

                if let Ok(mut semantic_search) = cx.global::<AppSemanticSearch>().0.lock() {
                    let results = semantic_search.search_with_context(
                        &user_query_text,
                        &candidate_refs,
                        5,
                        1,
                    );

                    if !results.is_empty() {
                        let semantic_context = results
                            .iter()
                            .map(|result| {
                                format!(
                                    "--- {}:{} ---\n{}\n{}\n{}",
                                    result.file_path,
                                    result.line_number,
                                    result.context_before,
                                    result.content,
                                    result.context_after
                                )
                            })
                            .collect::<Vec<_>>()
                            .join("\n\n");

                        all_context.push_str("## Semantic Search Matches\n\n");
                        all_context.push_str(&semantic_context);
                        all_context.push_str("\n\n");
                    }
                }
            }

            // HiveMemory + KnowledgeHub are async — queried in the spawn
            // blocks below. memory_context stays empty here; the real
            // enrichment happens off the UI thread via enrich_request().
            let memory_context = String::new();

            // For now, we seed the ContextEngine with whatever RAG found, plus we can index the current directory.
            if cx.has_global::<AppContextEngine>() {
                if let Ok(mut ctx_engine) = cx.global::<AppContextEngine>().0.lock() {
                    // Seed the engine with the retrieved context so TF-IDF
                    // curation can blend RAG and semantic-search matches.
                    if !all_context.is_empty() {
                        ctx_engine.add_file("retrieved_context.txt", &all_context);
                    }

                    // Seed engine with project knowledge files so they
                    // participate in TF-IDF scoring alongside RAG results.
                    if cx.has_global::<AppKnowledgeFiles>() {
                        for ks in &cx.global::<AppKnowledgeFiles>().0 {
                            let label = ks.path.file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_else(|| "knowledge".to_string());
                            ctx_engine.add_project_knowledge(&label, &ks.content);
                        }
                    }

                    // Also attempt to add semantic search history? Or use it directly.
                    // For the sake of the wiring task, we use ContextEngine to curate.
                    let budget = hive_ai::context_engine::ContextBudget {
                        max_tokens: 4000,
                        max_sources: 10,
                        reserved_tokens: 0,
                    };
                    let curated = ctx_engine.curate(&user_query_text, &budget);
                    
                    all_context.clear();
                    for source in curated.sources {
                        all_context.push_str(&source.content);
                        all_context.push_str("\n\n");
                    }
                }
            }

            let mut augmented = ai_messages.clone();

            // Inject project knowledge files (HIVE.md, README.md, etc.) as the
            // highest-priority system context. Re-scan on each message for freshness.
            {
                let fresh_sources = hive_ai::KnowledgeFileScanner::scan(&self.current_project_root);
                let knowledge_text = hive_ai::KnowledgeFileScanner::format_for_context(&fresh_sources);

                // Update the global so other systems see the latest state.
                cx.set_global(AppKnowledgeFiles(fresh_sources));

                if !knowledge_text.trim().is_empty() {
                    let kf_idx = augmented
                        .iter()
                        .position(|m| m.role != hive_ai::types::MessageRole::System)
                        .unwrap_or(0);
                    augmented.insert(
                        kf_idx,
                        hive_ai::types::ChatMessage {
                            role: hive_ai::types::MessageRole::System,
                            content: knowledge_text,
                            timestamp: chrono::Utc::now(),
                            tool_call_id: None,
                            tool_calls: None,
                        },
                    );
                }
            }

            // Determine context format for AI prompt encoding.
            let ctx_format = if cx.has_global::<AppConfig>() {
                hive_ai::ContextFormat::from_config_str(
                    &cx.global::<AppConfig>().0.get().context_format,
                )
            } else {
                hive_ai::ContextFormat::Markdown
            };

            // Inject fast-path project index as lightweight project context.
            // This gives the AI immediate awareness of the project structure,
            // key symbols, dependencies, and recent git activity -- available
            // even before the deeper RAG index has populated.
            if cx.has_global::<AppQuickIndex>() {
                let quick_ctx = match ctx_format {
                    hive_ai::ContextFormat::Toon => {
                        cx.global::<AppQuickIndex>().0.to_context_string_toon()
                    }
                    hive_ai::ContextFormat::Xml => {
                        cx.global::<AppQuickIndex>().0.to_context_string_xml()
                    }
                    _ => cx.global::<AppQuickIndex>().0.to_context_string(),
                };
                if !quick_ctx.trim().is_empty() {
                    let qi_idx = augmented
                        .iter()
                        .position(|m| m.role != hive_ai::types::MessageRole::System)
                        .unwrap_or(0);
                    augmented.insert(
                        qi_idx,
                        hive_ai::types::ChatMessage {
                            role: hive_ai::types::MessageRole::System,
                            content: quick_ctx,
                            timestamp: chrono::Utc::now(),
                            tool_call_id: None,
                            tool_calls: None,
                        },
                    );
                }
            }

            let insert_idx = augmented
                .iter()
                .position(|m| m.role != hive_ai::types::MessageRole::System)
                .unwrap_or(0);

            // Inject recalled memories as a dedicated system message
            if !memory_context.trim().is_empty() {
                augmented.insert(
                    insert_idx,
                    hive_ai::types::ChatMessage {
                        role: hive_ai::types::MessageRole::System,
                        content: format!(
                            "# Recalled Memories\n\nRelevant context from previous conversations:\n{}",
                            memory_context
                        ),
                        timestamp: chrono::Utc::now(),
                        tool_call_id: None,
                        tool_calls: None,
                    },
                );
            }

            // Inject retrieved code context
            if !all_context.trim().is_empty() {
                let ctx_idx = augmented
                    .iter()
                    .position(|m| m.role != hive_ai::types::MessageRole::System)
                    .unwrap_or(0);
                augmented.insert(
                    ctx_idx,
                    hive_ai::types::ChatMessage {
                        role: hive_ai::types::MessageRole::System,
                        content: format!("# Retrieved Context\n\n{}", all_context),
                        timestamp: chrono::Utc::now(),
                        tool_call_id: None,
                        tool_calls: None,
                    },
                );
            }

            // Inject user-selected context files (checked in Files panel).
            if !context_files.is_empty() {
                let use_xml = ctx_format == hive_ai::ContextFormat::Xml;
                let mut ctx_block = if use_xml {
                    String::from("<context_files>\n")
                } else {
                    String::from("# Selected Context Files\n\n")
                };
                for path in &context_files {
                    let rel = path
                        .strip_prefix(&self.current_project_root)
                        .unwrap_or(path);
                    let content = std::fs::read_to_string(path).unwrap_or_default();
                    let ext = path
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("");
                    let tokens = content.len().div_ceil(4);
                    if use_xml {
                        ctx_block.push_str(&format!(
                            "<file path=\"{}\" tokens=\"{}\"><![CDATA[{}]]></file>\n",
                            rel.display(),
                            tokens,
                            content
                        ));
                    } else {
                        ctx_block.push_str(&format!(
                            "## {}\n```{}\n{}\n```\n\n",
                            rel.display(),
                            ext,
                            content
                        ));
                    }
                }
                if use_xml {
                    ctx_block.push_str("</context_files>");
                }
                let cf_idx = augmented
                    .iter()
                    .position(|m| m.role != hive_ai::types::MessageRole::System)
                    .unwrap_or(0);
                augmented.insert(
                    cf_idx,
                    hive_ai::types::ChatMessage {
                        role: hive_ai::types::MessageRole::System,
                        content: ctx_block,
                        timestamp: chrono::Utc::now(),
                        tool_call_id: None,
                        tool_calls: None,
                    },
                );
            }

            augmented
        };

        // 2c. Check for /command skill activation and inject instructions
        let ai_messages = {
            let mut msgs = ai_messages;
            let trimmed_query = user_query_text.trim();
            if trimmed_query.starts_with('/') {
                let cmd_name = trimmed_query[1..]
                    .split_whitespace()
                    .next()
                    .unwrap_or("");
                let mut skill_instructions: Option<String> = None;

                // Check built-in skills registry
                if cx.has_global::<hive_ui_core::AppSkills>() {
                    if let Ok(instructions) = cx.global::<hive_ui_core::AppSkills>().0.dispatch(cmd_name)
                    {
                        skill_instructions = Some(instructions.to_string());
                    }
                }
                // Check user-created skills (file-based)
                if skill_instructions.is_none() && cx.has_global::<AppSkillManager>() {
                    if let Ok(Some(skill)) = cx.global::<AppSkillManager>().0.get(cmd_name) {
                        if skill.enabled {
                            skill_instructions = Some(skill.instructions.clone());
                        }
                    }
                }

                if let Some(instructions) = skill_instructions {
                    let insert_idx = msgs
                        .iter()
                        .position(|m| m.role != hive_ai::types::MessageRole::System)
                        .unwrap_or(0);
                    msgs.insert(
                        insert_idx,
                        hive_ai::types::ChatMessage {
                            role: hive_ai::types::MessageRole::System,
                            content: format!(
                                "# Active Skill: /{}\n\n{}",
                                cmd_name, instructions
                            ),
                            timestamp: chrono::Utc::now(),
                            tool_call_id: None,
                            tool_calls: None,
                        },
                    );
                }
            }
            msgs
        };

        // 3. Build tool definitions from built-in + MCP integration tools.
        let agent_defs = hive_agents::tool_use::builtin_tool_definitions();
        let mut tool_defs: Vec<AiToolDefinition> = agent_defs
            .into_iter()
            .map(|d| AiToolDefinition {
                name: d.name,
                description: d.description,
                input_schema: d.input_schema,
            })
            .collect();

        // Include MCP integration tools (messaging, project mgmt, browser, etc.)
        if cx.has_global::<AppMcpServer>() {
            let mcp = &cx.global::<AppMcpServer>().0;
            for tool in mcp.list_tools() {
                // Skip builtins already included to avoid duplicates.
                if tool_defs.iter().any(|t| t.name == tool.name) {
                    continue;
                }
                tool_defs.push(AiToolDefinition {
                    name: tool.name.clone(),
                    description: tool.description.clone(),
                    input_schema: tool.input_schema.clone(),
                });
            }
        }

        // 4a. Build system prompt from learned preferences (if any).
        let mut system_prompt = if cx.has_global::<AppLearning>() {
            let learning = &cx.global::<AppLearning>().0;
            match learning.preference_model.prompt_addendum() {
                Ok(addendum) if !addendum.is_empty() => {
                    info!("Injecting learned preferences into system prompt");
                    Some(addendum)
                }
                _ => None,
            }
        } else {
            None
        };

        // When XML context format is active, instruct the AI to use <edit> tags.
        let ctx_format_for_prompt = if cx.has_global::<AppConfig>() {
            hive_ai::ContextFormat::from_config_str(
                &cx.global::<AppConfig>().0.get().context_format,
            )
        } else {
            hive_ai::ContextFormat::Markdown
        };
        if ctx_format_for_prompt == hive_ai::ContextFormat::Xml {
            let xml_instruction = "\n\nWhen suggesting code changes, wrap each file edit in an XML tag: <edit path=\"relative/path\" lang=\"language\">new file content</edit>";
            system_prompt = Some(
                system_prompt
                    .map(|s| s + xml_instruction)
                    .unwrap_or_else(|| xml_instruction.to_string()),
            );
        }

        // 4b. Check if speculative decoding is enabled.
        let spec_config = if cx.has_global::<AppConfig>() {
            let cfg = cx.global::<AppConfig>().0.get();
            SpeculativeConfig {
                enabled: cfg.speculative_decoding,
                draft_model: cfg.speculative_draft_model.clone(),
                show_metrics: cfg.speculative_show_metrics,
            }
        } else {
            SpeculativeConfig::default()
        };

        // 4c. Extract provider + request from the global (sync — no await).
        //     If speculative decoding is enabled, also prepare the draft stream.
        let use_speculative = spec_config.enabled
            && cx.has_global::<AppAiService>()
            && cx.global::<AppAiService>().0.prepare_speculative_stream(
                ai_messages.clone(),
                &model,
                system_prompt.clone(),
                Some(tool_defs.clone()),
                &spec_config,
            ).is_some();

        let stream_setup: Option<(Arc<dyn AiProvider>, ChatRequest)> = if cx
            .has_global::<AppAiService>()
        {
            cx.global::<AppAiService>()
                .0
                .prepare_stream(ai_messages.clone(), &model, system_prompt.clone(), Some(tool_defs.clone()))
        } else {
            None
        };

        let Some((provider, request)) = stream_setup else {
            self.chat_service.update(cx, |svc, cx| {
                svc.set_error(
                    "No AI providers configured. Check Settings \u{2192} API Keys.",
                    cx,
                );
            });
            return;
        };

        // 5. Spawn async: call provider.stream_chat, then attach with tool loop.
        let chat_svc = self.chat_service.downgrade();
        let model_for_attach = model.clone();
        let provider_for_loop = provider.clone();
        let request_for_loop = request.clone();

        // Clone async-capable globals for capture by the spawn blocks.
        let hive_mem_for_async: Option<std::sync::Arc<tokio::sync::Mutex<hive_ai::memory::HiveMemory>>> =
            if cx.has_global::<AppHiveMemory>() {
                Some(cx.global::<AppHiveMemory>().0.clone())
            } else {
                None
            };
        let knowledge_hub_for_async: Option<std::sync::Arc<hive_integrations::knowledge::KnowledgeHub>> =
            if cx.has_global::<AppKnowledge>() {
                let kb = cx.global::<AppKnowledge>().0.clone();
                if kb.provider_count() > 0 { Some(kb) } else { None }
            } else {
                None
            };
        let query_for_memory = user_query_text.clone();

        let task = if use_speculative {
            // Speculative decoding path: dual-stream from draft + primary.
            let speculative_setup = cx.global::<AppAiService>().0.prepare_speculative_stream(
                ai_messages,
                &model,
                system_prompt,
                Some(tool_defs),
                &spec_config,
            );

            if let Some((draft_provider, mut draft_request, primary_provider, mut primary_request)) = speculative_setup {
                let spec_config_clone = spec_config.clone();
                let hm = hive_mem_for_async.clone();
                let kb = knowledge_hub_for_async.clone();
                let qm = query_for_memory.clone();
                cx.spawn(async move |_this, app: &mut AsyncApp| {
                    // Enrich both draft and primary requests with memory/knowledge.
                    enrich_request_with_memory(&mut draft_request, &hm, &kb, &qm).await;
                    enrich_request_with_memory(&mut primary_request, &hm, &kb, &qm).await;

                    match speculative::speculative_stream(
                        draft_provider,
                        draft_request,
                        primary_provider.clone(),
                        primary_request.clone(),
                        spec_config_clone,
                    ).await {
                        Ok(mut spec_rx) => {
                            // Convert speculative chunks into regular StreamChunk stream.
                            // Draft chunks get a "[speculating] " visual prefix.
                            // When primary starts, we send a reset-content signal.
                            let (tx, rx) = tokio::sync::mpsc::channel(256);
                            let _model_for_metrics = model_for_attach.clone();

                            tokio::spawn(async move {
                                let mut in_draft_phase = true;
                                while let Some(spec_chunk) = spec_rx.recv().await {
                                    if spec_chunk.is_draft {
                                        // Forward draft content as-is (UI can style it)
                                        let _ = tx.send(spec_chunk.chunk).await;
                                    } else {
                                        if in_draft_phase {
                                            // Transition: send a special "reset" chunk
                                            // The content field carries a marker the UI can detect
                                            let _ = tx.send(hive_ai::types::StreamChunk {
                                                content: "\n\n---\n\n".to_string(),
                                                done: false,
                                                thinking: None,
                                                usage: None,
                                                tool_calls: None,
                                                stop_reason: None,
                                            }).await;
                                            in_draft_phase = false;
                                        }

                                        // Append metrics info to the final chunk if available
                                        let mut chunk = spec_chunk.chunk;
                                        if let Some(metrics) = spec_chunk.metrics {
                                            if chunk.done {
                                                let metrics_text = format!(
                                                    "\n\n> Speculative decoding saved ~{}ms | Draft: {} ({}ms) | Primary: {} ({}ms)",
                                                    metrics.time_saved_ms,
                                                    metrics.draft_model,
                                                    metrics.draft_first_token_ms,
                                                    metrics.primary_model,
                                                    metrics.primary_first_token_ms,
                                                );
                                                chunk.content.push_str(&metrics_text);
                                            }
                                        }
                                        let _ = tx.send(chunk).await;
                                    }
                                }
                            });

                            let _ = chat_svc.update(app, |svc, cx| {
                                svc.attach_tool_stream(
                                    rx,
                                    model_for_attach,
                                    primary_provider,
                                    primary_request,
                                    cx,
                                );
                            });
                        }
                        Err(e) => {
                            error!("Speculative stream error: {e}");
                            // Fall back to normal stream (already enriched via
                            // the same hm/kb/qm captured by this spawn block).
                            let mut fallback_req = request.clone();
                            enrich_request_with_memory(&mut fallback_req, &hm, &kb, &qm).await;
                            match provider.stream_chat(&fallback_req).await {
                                Ok(rx) => {
                                    let _ = chat_svc.update(app, |svc, cx| {
                                        svc.attach_tool_stream(rx, model_for_attach, provider_for_loop, request_for_loop, cx);
                                    });
                                }
                                Err(e2) => {
                                    let _ = chat_svc.update(app, |svc, cx| {
                                        svc.set_error(format!("AI request failed: {e2}"), cx);
                                    });
                                }
                            }
                        }
                    }
                })
            } else {
                // Speculative setup failed, fall back to normal
                let hm = hive_mem_for_async.clone();
                let kb = knowledge_hub_for_async.clone();
                let qm = query_for_memory.clone();
                cx.spawn(async move |_this, app: &mut AsyncApp| {
                    let mut enriched_request = request.clone();
                    enrich_request_with_memory(&mut enriched_request, &hm, &kb, &qm).await;
                    match provider.stream_chat(&enriched_request).await {
                        Ok(rx) => {
                            let _ = chat_svc.update(app, |svc, cx| {
                                svc.attach_tool_stream(rx, model_for_attach, provider_for_loop, request_for_loop, cx);
                            });
                        }
                        Err(e) => {
                            error!("Stream error: {e}");
                            let _ = chat_svc.update(app, |svc, cx| {
                                svc.set_error(format!("AI request failed: {e}"), cx);
                            });
                        }
                    }
                })
            }
        } else {
            // Normal (non-speculative) path
            cx.spawn(async move |_this, app: &mut AsyncApp| {
                let mut enriched_request = request.clone();
                enrich_request_with_memory(
                    &mut enriched_request,
                    &hive_mem_for_async,
                    &knowledge_hub_for_async,
                    &query_for_memory,
                ).await;
                match provider.stream_chat(&enriched_request).await {
                    Ok(rx) => {
                        let _ = chat_svc.update(app, |svc, cx| {
                            svc.attach_tool_stream(
                                rx,
                                model_for_attach,
                                provider_for_loop,
                                request_for_loop,
                                cx,
                            );
                        });
                    }
                    Err(e) => {
                        error!("Stream error: {e}");
                        let _ = chat_svc.update(app, |svc, cx| {
                            svc.set_error(format!("AI request failed: {e}"), cx);
                        });
                    }
                }
            })
        };

        self._stream_task = Some(task);
        self.chat_input.update(cx, |input, cx| {
            input.set_sending(true, window, cx);
        });

        info!("Send initiated (model={})", model);
        cx.notify();
    }

    /// Sync status bar with current chat service state.
    /// NOTE: This runs on every render frame — must be cheap. No file I/O here.
    fn sync_status_bar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Read all state from the chat service first, then release the borrow.
        let (model, is_streaming, total, current_conv_id) = {
            let svc = self.chat_service.read(cx);
            let model = svc.current_model().to_string();
            let streaming = svc.is_streaming();
            let total: f64 = svc.messages().iter().filter_map(|m| m.cost).sum();
            let conv_id = svc.conversation_id().map(String::from);
            (model, streaming, total, conv_id)
        };

        self.status_bar.active_project = self.project_label();

        self.status_bar.current_model = if model.is_empty() {
            "Select Model".to_string()
        } else {
            model
        };
        self.status_bar.total_cost = total;

        // Sync the chat input disabled state with streaming status.
        self.chat_input.update(cx, |input, cx| {
            input.set_sending(is_streaming, window, cx);
        });

        // Detect conversation ID changes (e.g. after stream finalization
        // auto-saves a conversation and assigns an ID for the first time).
        if current_conv_id != self.last_saved_conversation_id {
            self.session_dirty = true;
            // Save session on actual state change — not every frame.
            self.save_session(cx);
        }

        // -- Auto-update: check if the updater has found a newer version --
        if cx.has_global::<AppUpdater>() {
            let info = cx.global::<AppUpdater>().0.available_update();
            self.status_bar.update_available = info.map(|i| i.version);
        }

        // -- Discovery: periodic scan + connectivity update --
        self.maybe_trigger_discovery_scan(cx);
        self.sync_connectivity(cx);

        // -- Drain triggered reminders from the tick driver --
        if cx.has_global::<AppReminderRx>() {
            let rx_arc = Arc::clone(&cx.global::<AppReminderRx>().0);
            let mut pending = Vec::new();
            if let Ok(rx) = rx_arc.lock() {
                while let Ok(reminders) = rx.try_recv() {
                    pending.extend(reminders);
                }
            }
            for reminder in &pending {
                info!("UI received reminder: {}", reminder.title);
                if cx.has_global::<AppNotifications>() {
                    cx.global_mut::<AppNotifications>().0.push(
                        AppNotification::new(
                            NotificationType::Info,
                            format!("Reminder: {}", reminder.title),
                        )
                        .with_title("Reminder"),
                    );
                }
            }
        }

        // -- Auto-refresh network peer data every 30 seconds when panel is active --
        if self.sidebar.active_panel == Panel::Network {
            let should_refresh = match self.last_network_refresh {
                None => true,
                Some(t) => t.elapsed() >= std::time::Duration::from_secs(30),
            };
            if should_refresh {
                self.last_network_refresh = Some(std::time::Instant::now());
                self.refresh_network_peer_data(cx);
            }
        }
    }

    /// Trigger a discovery scan every 30 seconds (non-blocking).
    ///
    /// Runs the actual HTTP probing on a background OS thread with its own Tokio
    /// runtime (reqwest requires Tokio, but GPUI uses a smol-based executor).
    /// On the next `sync_status_bar()` tick the completion flag is checked and
    /// the UI is updated with any newly discovered models.
    fn maybe_trigger_discovery_scan(&mut self, cx: &mut Context<Self>) {
        // Check if a previous scan just finished.
        if self.discovery_scan_pending {
            if let Some(flag) = &self.discovery_done_flag
                && flag.load(std::sync::atomic::Ordering::Acquire)
            {
                self.discovery_scan_pending = false;
                self.discovery_done_flag = None;
                // Refresh UI with discovered models.
                if cx.has_global::<AppAiService>()
                    && let Some(d) = cx.global::<AppAiService>().0.discovery()
                {
                    let models = d.snapshot().all_models();
                    self.settings_view.update(cx, |settings, cx| {
                        settings.refresh_local_models(models.clone(), cx);
                    });
                    self.models_browser_view.update(cx, |browser, cx| {
                        browser.set_local_models(models, cx);
                    });
                }
                cx.notify();
            }
            return;
        }

        let should_scan = match self.last_discovery_scan {
            None => true,
            Some(t) => t.elapsed() >= std::time::Duration::from_secs(30),
        };
        if !should_scan {
            return;
        }

        let discovery = if cx.has_global::<AppAiService>() {
            cx.global::<AppAiService>().0.discovery().cloned()
        } else {
            None
        };

        let Some(discovery) = discovery else { return };

        self.discovery_scan_pending = true;
        self.last_discovery_scan = Some(std::time::Instant::now());

        let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
        self.discovery_done_flag = Some(Arc::clone(&done));

        std::thread::spawn(move || {
            discovery.scan_all_blocking();
            done.store(true, std::sync::atomic::Ordering::Release);
        });
    }

    /// Update status bar connectivity based on registered + discovered providers.
    fn sync_connectivity(&mut self, cx: &App) {
        if !cx.has_global::<AppAiService>() {
            return;
        }
        let ai = &cx.global::<AppAiService>().0;
        let has_cloud = ai.available_providers().iter().any(|p| {
            matches!(
                p,
                hive_ai::types::ProviderType::Anthropic
                    | hive_ai::types::ProviderType::OpenAI
                    | hive_ai::types::ProviderType::OpenRouter
                    | hive_ai::types::ProviderType::Google
                    | hive_ai::types::ProviderType::Groq
                    | hive_ai::types::ProviderType::HuggingFace
            )
        });
        let has_local = ai
            .discovery()
            .map(|d| d.snapshot().any_online())
            .unwrap_or(false);

        self.status_bar.connectivity = match (has_cloud, has_local) {
            (true, _) => ConnectivityDisplay::Online,
            (false, true) => ConnectivityDisplay::LocalOnly,
            (false, false) => ConnectivityDisplay::Offline,
        };
    }

    // -- Rendering -----------------------------------------------------------

    fn render_active_panel(&mut self, cx: &mut Context<Self>) -> AnyElement {
        if self.sidebar.active_panel == Panel::Chat {
            return self.render_chat_cached(cx);
        }
        let theme = &self.theme;
        match self.sidebar.active_panel {
            Panel::Chat => unreachable!(),
            Panel::QuickStart => QuickStartPanel::render(
                &self.quick_start_data,
                &self.quick_start_goal_input,
                theme,
            )
            .into_any_element(),
            Panel::History => HistoryPanel::render(&self.history_data, theme).into_any_element(),
            Panel::Files => FilesPanel::render(&self.files_data, theme).into_any_element(),
            Panel::CodeMap => {
                hive_ui_panels::panels::code_map::render_code_map(&self.code_map_data, theme)
                    .into_any_element()
            }
            Panel::PromptLibrary => {
                hive_ui_panels::panels::prompt_library::render_prompt_library(
                    &self.prompt_library_data,
                    theme,
                )
                .into_any_element()
            }
            Panel::Kanban => KanbanPanel::render(&self.kanban_data, theme).into_any_element(),
            Panel::Monitor => MonitorPanel::render(&self.monitor_data, theme).into_any_element(),
            Panel::Activity => {
                hive_ui_panels::panels::activity::ActivityPanel::render(&self.activity_data, theme)
                    .into_any_element()
            }
            Panel::Logs => LogsPanel::render(&self.logs_data, theme).into_any_element(),
            Panel::Costs => CostsPanel::render(&self.cost_data, theme).into_any_element(),
            Panel::Review => ReviewPanel::render(&self.review_data, theme).into_any_element(),
            Panel::Skills => SkillsPanel::render(&self.skills_data, theme).into_any_element(),
            Panel::Routing => RoutingPanel::render(&self.routing_data, theme).into_any_element(),
            Panel::Workflows => self.workflow_builder_view.clone().into_any_element(),
            Panel::Channels => self.channels_view.clone().into_any_element(),
            Panel::Models => self.models_browser_view.clone().into_any_element(),
            Panel::TokenLaunch => TokenLaunchPanel::render(
                &self.token_launch_data,
                &self.token_launch_inputs,
                theme,
            )
            .into_any_element(),
            Panel::Specs => SpecsPanel::render(&self.specs_data, theme).into_any_element(),
            Panel::Agents => AgentsPanel::render(
                &self.agents_data,
                &self.agents_remote_prompt_input,
                theme,
            )
            .into_any_element(),
            Panel::Shield => self.shield_view.clone().into_any_element(),
            Panel::Learning => LearningPanel::render(&self.learning_data, theme).into_any_element(),
            Panel::Assistant => {
                AssistantPanel::render(&self.assistant_data, theme).into_any_element()
            }
            Panel::Settings => self.settings_view.clone().into_any_element(),
            Panel::Help => HelpPanel::render(theme).into_any_element(),
            Panel::Network => {
                NetworkPanel::render(&self.network_peer_data, theme).into_any_element()
            }
            Panel::Terminal => {
                div()
                    .flex()
                    .flex_col()
                    .size_full()
                    .child(TerminalPanel::render(&self.terminal_data, theme))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .px(theme.space_4)
                            .py(theme.space_2)
                            .border_t_1()
                            .border_color(theme.border)
                            .bg(theme.bg_secondary)
                            .gap(theme.space_2)
                            .child(
                                div()
                                    .text_size(theme.font_size_sm)
                                    .text_color(theme.accent_cyan)
                                    .font_family("Consolas, Menlo, monospace")
                                    .child("$"),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .child(self.terminal_input.clone()),
                            ),
                    )
                    .into_any_element()
            }
        }
    }

    /// Render the chat panel using cached display data.
    ///
    /// Syncs `CachedChatData` from `ChatService` only when the generation
    /// counter has changed, then renders from the cached `DisplayMessage`
    /// vec and pre-parsed markdown IR.
    fn render_chat_cached(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let svc = self.chat_service.read(cx);

        // Rebuild display messages only when the service has mutated.
        sync_chat_cache(&mut self.cached_chat_data, svc);

        let streaming_content = svc.streaming_content().to_string();
        let is_streaming = svc.is_streaming();
        let current_model = svc.current_model().to_string();
        let pending_approval = svc.pending_approval.clone();

        let chat_element = ChatPanel::render_cached(
            &mut self.cached_chat_data,
            &streaming_content,
            is_streaming,
            &current_model,
            &self.theme,
        );

        // If there's a pending tool approval, overlay the approval card.
        if let Some(approval) = pending_approval {
            let theme = &self.theme;
            div()
                .flex()
                .flex_col()
                .size_full()
                .child(chat_element)
                .child(Self::render_approval_card(&approval, theme))
                .into_any_element()
        } else {
            chat_element
        }
    }

    /// Render the tool approval card shown when write_file needs user consent.
    fn render_approval_card(
        approval: &crate::chat_service::PendingToolApproval,
        theme: &HiveTheme,
    ) -> impl IntoElement {
        use hive_ui_panels::components::diff_viewer::render_diff;
        use hive_ui_panels::components::code_block::render_code_block;

        let is_new_file = approval.old_content.is_none();
        let file_size = approval.new_content.len();

        // Detect language from extension.
        let lang = approval.file_path.rsplit('.').next()
            .map(|ext| match ext {
                "rs" => "Rust", "ts" | "tsx" => "TypeScript", "js" | "jsx" => "JavaScript",
                "py" => "Python", "toml" => "TOML", "json" => "JSON", "md" => "Markdown",
                "html" => "HTML", "css" => "CSS", "yaml" | "yml" => "YAML",
                _ => "text",
            })
            .unwrap_or("text");

        let diff_or_code: AnyElement = if is_new_file {
            render_code_block(&approval.new_content, lang, theme).into_any_element()
        } else {
            render_diff(&approval.diff_lines, theme).into_any_element()
        };

        div()
            .id("tool-approval-card")
            .mx(theme.space_4)
            .mb(theme.space_4)
            .rounded(theme.radius_md)
            .border_1()
            .border_color(theme.accent_yellow)
            .bg(theme.bg_secondary)
            .overflow_hidden()
            // Header
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .px(theme.space_4)
                    .py(theme.space_2)
                    .bg(theme.bg_tertiary)
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(theme.space_2)
                            .child(
                                div()
                                    .text_size(theme.font_size_sm)
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(theme.accent_yellow)
                                    .child(if is_new_file { "Create file" } else { "Modify file" }),
                            )
                            .child(
                                div()
                                    .text_size(theme.font_size_sm)
                                    .text_color(theme.text_primary)
                                    .child(approval.file_path.clone()),
                            )
                            .child(
                                div()
                                    .text_size(theme.font_size_xs)
                                    .text_color(theme.text_muted)
                                    .child(format!("{} bytes", file_size)),
                            ),
                    )
                    .child(
                        div()
                            .text_size(theme.font_size_xs)
                            .text_color(theme.text_muted)
                            .child(lang),
                    ),
            )
            // Diff/code content (scrollable, max height)
            .child(
                div()
                    .max_h(px(300.0))
                    .overflow_y_scrollbar()
                    .child(diff_or_code),
            )
            // Action buttons
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_end()
                    .gap(theme.space_2)
                    .px(theme.space_4)
                    .py(theme.space_2)
                    .border_t_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .id("tool-reject")
                            .cursor_pointer()
                            .px(theme.space_4)
                            .py(theme.space_1)
                            .rounded(theme.radius_sm)
                            .bg(theme.accent_red)
                            .text_size(theme.font_size_sm)
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(gpui::white())
                            .hover(|s| s.opacity(0.8))
                            .on_mouse_down(MouseButton::Left, |_, window, cx| {
                                window.dispatch_action(Box::new(ToolReject), cx);
                            })
                            .child("Reject"),
                    )
                    .child(
                        div()
                            .id("tool-approve")
                            .cursor_pointer()
                            .px(theme.space_4)
                            .py(theme.space_1)
                            .rounded(theme.radius_sm)
                            .bg(theme.accent_green)
                            .text_size(theme.font_size_sm)
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(gpui::white())
                            .hover(|s| s.opacity(0.8))
                            .on_mouse_down(MouseButton::Left, |_, window, cx| {
                                window.dispatch_action(Box::new(ToolApprove), cx);
                            })
                            .child("Approve"),
                    ),
            )
    }

    // -- Keyboard action handlers --------------------------------------------

    fn handle_new_conversation(
        &mut self,
        _action: &NewConversation,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        info!("NewConversation action triggered");
        self.chat_service.update(cx, |svc, _cx| {
            svc.new_conversation();
        });
        self.cached_chat_data.markdown_cache.clear();
        self.refresh_history();
        self.sidebar.active_panel = Panel::Chat;
        self.session_dirty = true;
        cx.notify();
    }

    fn handle_clear_chat(
        &mut self,
        _action: &ClearChat,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        info!("ClearChat action triggered");
        self.chat_service.update(cx, |svc, _cx| {
            svc.clear();
        });
        self.cached_chat_data.markdown_cache.clear();
        cx.notify();
    }

    fn switch_to_panel(&mut self, panel: Panel, cx: &mut Context<Self>) {
        info!("SwitchToPanel action: {:?}", panel);
        self.sidebar.active_panel = panel;

        // Lazy-load data for panels that need it on first visit.
        match panel {
            Panel::QuickStart => {
                self.refresh_quick_start_data(cx);
            }
            Panel::History if self.history_data.conversations.is_empty() => {
                self.history_data = Self::load_history_data();
            }
            Panel::Files if self.files_data.entries.is_empty() => {
                self.files_data = FilesData::from_path(&self.files_data.current_path.clone());
            }
            Panel::Review => {
                self.review_data = ReviewData::from_git(&self.current_project_root);
            }
            Panel::Costs => {
                self.refresh_cost_data(cx);
            }
            Panel::Learning => {
                self.refresh_learning_data(cx);
            }
            Panel::Shield => {
                self.refresh_shield_data(cx);
            }
            Panel::Routing => {
                self.refresh_routing_data(cx);
            }
            Panel::Workflows => {
                self.refresh_workflow_builder(cx);
            }
            Panel::Channels => {
                self.refresh_channels_view(cx);
            }
            Panel::Models => {
                self.push_keys_to_models_browser(cx);
                self.models_browser_view.update(cx, |browser, cx| {
                    browser.trigger_fetches(cx);
                });
            }
            Panel::Skills => {
                self.refresh_skills_data(cx);
                self.trigger_plugin_version_check(cx);
            }
            Panel::Agents => {
                self.refresh_agents_data(cx);
            }
            Panel::Specs => {
                self.refresh_specs_data(cx);
            }
            Panel::Assistant => {
                self.refresh_assistant_data(cx);
                self.refresh_assistant_connected_data(cx);
            }
            Panel::Monitor => {
                self.refresh_monitor_data(cx);
            }
            Panel::Network => {
                self.refresh_network_peer_data(cx);
            }
            Panel::Logs => {
                self.refresh_logs_data(cx);
            }
            Panel::Kanban => {
                self.refresh_kanban_data();
            }
            Panel::Terminal => {
                self.ensure_terminal_shell(cx);
            }
            _ => {}
        }

        // Save session immediately (this is an action handler, not render path).
        self.save_session(cx);
        cx.notify();
    }

    fn handle_switch_to_chat(
        &mut self,
        _action: &SwitchToChat,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.switch_to_panel(Panel::Chat, cx);
        // Focus the chat text input so the user can start typing immediately.
        let fh = self.chat_input.read(cx).input_focus_handle();
        window.focus(&fh);
    }

    fn handle_switch_to_quick_start(
        &mut self,
        _action: &SwitchToQuickStart,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.switch_to_panel(Panel::QuickStart, cx);
    }

    fn handle_switch_to_history(
        &mut self,
        _action: &SwitchToHistory,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.switch_to_panel(Panel::History, cx);
    }

    fn handle_switch_to_files(
        &mut self,
        _action: &SwitchToFiles,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.switch_to_panel(Panel::Files, cx);
    }

    fn handle_switch_to_code_map(
        &mut self,
        _action: &SwitchToCodeMap,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Refresh code map data from QuickIndex before showing
        self.code_map_data = hive_ui_panels::panels::code_map::build_code_map_data(cx);
        self.switch_to_panel(Panel::CodeMap, cx);
    }

    fn handle_switch_to_prompt_library(
        &mut self,
        _action: &SwitchToPromptLibrary,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.prompt_library_data.refresh();
        self.switch_to_panel(Panel::PromptLibrary, cx);
    }

    fn handle_prompt_library_save_current(
        &mut self,
        _action: &PromptLibrarySaveCurrent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use hive_agents::prompt_template;

        // Get current chat input text
        let instruction = self.chat_input.read(cx).current_text(cx);
        if instruction.trim().is_empty() {
            return;
        }

        // Collect checked context files
        let context_files: Vec<String> = self
            .files_data
            .checked_files
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect();

        let mut template = prompt_template::PromptTemplate::new(
            format!("Prompt {}", chrono::Utc::now().format("%Y-%m-%d %H:%M")),
            String::new(),
            instruction,
        );
        template.context_files = context_files;

        if let Err(e) = prompt_template::save_template(&template) {
            error!("Failed to save prompt template: {e}");
        } else {
            info!("Saved prompt template: {}", template.name);
            self.prompt_library_data.refresh();
            cx.notify();
        }
    }

    fn handle_prompt_library_refresh(
        &mut self,
        _action: &PromptLibraryRefresh,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.prompt_library_data.refresh();
        cx.notify();
    }

    fn handle_prompt_library_load(
        &mut self,
        action: &PromptLibraryLoad,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use hive_agents::prompt_template;

        match prompt_template::load_template(&action.prompt_id) {
            Ok(template) => {
                // Pre-fill instruction in chat input
                self.chat_input.update(cx, |input, cx| {
                    input.set_text(&template.instruction, window, cx);
                });

                // Auto-check context files
                for file in &template.context_files {
                    // Security: block absolute paths and directory traversal.
                    if std::path::Path::new(file).is_absolute() || file.contains("..") {
                        tracing::warn!("Skipping unsafe context file path: {file}");
                        continue;
                    }
                    let path = std::path::PathBuf::from(file);
                    if !self.files_data.checked_files.contains(&path) {
                        self.files_data.checked_files.insert(path);
                    }
                }

                // Switch to chat panel
                self.switch_to_panel(Panel::Chat, cx);
                info!("Loaded prompt template: {}", template.name);
            }
            Err(e) => {
                error!("Failed to load prompt template: {e}");
            }
        }
    }

    fn handle_prompt_library_delete(
        &mut self,
        action: &PromptLibraryDelete,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use hive_agents::prompt_template;

        if let Err(e) = prompt_template::delete_template(&action.prompt_id) {
            error!("Failed to delete prompt template: {e}");
        } else {
            self.prompt_library_data.refresh();
            cx.notify();
        }
    }

    fn handle_open_workspace_directory(
        &mut self,
        _action: &OpenWorkspaceDirectory,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: None,
        });

        cx.spawn(async move |this, app: &mut AsyncApp| {
            if let Ok(Ok(Some(paths))) = receiver.await
                && let Some(path) = paths.first()
            {
                let workspace_path = path.to_path_buf();
                let _ = this.update(app, move |this, cx| {
                    this.switch_to_workspace(workspace_path, cx);
                });
            }
        })
        .detach();
    }

    fn handle_toggle_project_dropdown(
        &mut self,
        _action: &ToggleProjectDropdown,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show_project_dropdown = !self.show_project_dropdown;
        cx.notify();
    }

    fn handle_switch_to_workspace_action(
        &mut self,
        action: &SwitchToWorkspace,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show_project_dropdown = false;
        let path = PathBuf::from(&action.path);
        if !path.exists() {
            // Remove stale path from both lists
            self.recent_workspace_roots.retain(|p| p != &path);
            self.pinned_workspace_roots.retain(|p| p != &path);
            self.session_dirty = true;
            self.save_session(cx);
            if cx.has_global::<AppNotifications>() {
                cx.global_mut::<AppNotifications>().0.push(
                    AppNotification::new(
                        NotificationType::Warning,
                        "Project folder not found",
                    ),
                );
            }
            cx.notify();
            return;
        }
        self.switch_to_workspace(path, cx);
    }

    fn handle_toggle_pin_workspace(
        &mut self,
        action: &TogglePinWorkspace,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let path = PathBuf::from(&action.path);
        if let Some(idx) = self.pinned_workspace_roots.iter().position(|p| p == &path) {
            self.pinned_workspace_roots.remove(idx);
        } else {
            self.pinned_workspace_roots.push(path);
            self.pinned_workspace_roots.truncate(MAX_PINNED_WORKSPACES);
        }
        self.session_dirty = true;
        self.save_session(cx);
        cx.notify();
    }

    fn handle_remove_recent_workspace(
        &mut self,
        action: &RemoveRecentWorkspace,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let path = PathBuf::from(&action.path);
        // No-op for active workspace
        if path == self.current_project_root {
            return;
        }
        self.recent_workspace_roots.retain(|p| p != &path);
        self.pinned_workspace_roots.retain(|p| p != &path);
        self.session_dirty = true;
        self.save_session(cx);
        cx.notify();
    }

    fn handle_switch_to_kanban(
        &mut self,
        _action: &SwitchToKanban,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.switch_to_panel(Panel::Kanban, cx);
    }

    fn handle_switch_to_monitor(
        &mut self,
        _action: &SwitchToMonitor,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.switch_to_panel(Panel::Monitor, cx);
    }

    fn handle_switch_to_activity(
        &mut self,
        _action: &SwitchToActivity,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.switch_to_panel(Panel::Activity, cx);
    }

    fn handle_switch_to_logs(
        &mut self,
        _action: &SwitchToLogs,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.switch_to_panel(Panel::Logs, cx);
    }

    fn handle_switch_to_costs(
        &mut self,
        _action: &SwitchToCosts,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.switch_to_panel(Panel::Costs, cx);
    }

    fn handle_switch_to_review(
        &mut self,
        _action: &SwitchToReview,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.switch_to_panel(Panel::Review, cx);
    }

    fn handle_switch_to_skills(
        &mut self,
        _action: &SwitchToSkills,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.switch_to_panel(Panel::Skills, cx);
    }

    fn handle_switch_to_routing(
        &mut self,
        _action: &SwitchToRouting,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.switch_to_panel(Panel::Routing, cx);
    }

    fn handle_switch_to_models(
        &mut self,
        _action: &SwitchToModels,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.switch_to_panel(Panel::Models, cx);
    }

    fn handle_switch_to_token_launch(
        &mut self,
        _action: &SwitchToTokenLaunch,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.switch_to_panel(Panel::TokenLaunch, cx);
    }

    fn handle_switch_to_specs(
        &mut self,
        _action: &SwitchToSpecs,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.switch_to_panel(Panel::Specs, cx);
    }

    fn handle_switch_to_agents(
        &mut self,
        _action: &SwitchToAgents,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.switch_to_panel(Panel::Agents, cx);
    }

    fn handle_switch_to_workflows(
        &mut self,
        _action: &SwitchToWorkflows,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.switch_to_panel(Panel::Workflows, cx);
    }

    fn handle_switch_to_channels(
        &mut self,
        _action: &SwitchToChannels,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.switch_to_panel(Panel::Channels, cx);
    }

    fn handle_switch_to_learning(
        &mut self,
        _action: &SwitchToLearning,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.switch_to_panel(Panel::Learning, cx);
    }

    fn handle_switch_to_shield(
        &mut self,
        _action: &SwitchToShield,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.switch_to_panel(Panel::Shield, cx);
    }

    fn handle_switch_to_assistant(
        &mut self,
        _action: &SwitchToAssistant,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.switch_to_panel(Panel::Assistant, cx);
    }

    fn handle_switch_to_settings(
        &mut self,
        _action: &SwitchToSettings,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.switch_to_panel(Panel::Settings, cx);
    }

    fn handle_switch_to_help(
        &mut self,
        _action: &SwitchToHelp,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.switch_to_panel(Panel::Help, cx);
    }

    fn handle_switch_to_network(
        &mut self,
        _action: &SwitchToNetwork,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.switch_to_panel(Panel::Network, cx);
    }

    fn handle_switch_to_terminal(
        &mut self,
        _action: &SwitchToTerminal,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.switch_to_panel(Panel::Terminal, cx);
        self.ensure_terminal_shell(cx);
    }

    // -- Terminal panel handlers --------------------------------------------

    /// Spawn the background shell if it isn't running yet.
    fn ensure_terminal_shell(&mut self, cx: &mut Context<Self>) {
        if self.terminal_cmd_tx.is_some() {
            return; // already running
        }

        let cwd = PathBuf::from(&self.terminal_data.cwd);
        let (cmd_tx, mut cmd_rx) = tokio::sync::mpsc::unbounded_channel::<TerminalCmd>();
        self.terminal_cmd_tx = Some(cmd_tx);
        self.terminal_data.is_running = true;
        self.terminal_data.push_system("Shell starting...");

        let task = cx.spawn(async move |this: WeakEntity<Self>, app: &mut AsyncApp| {
            // Spawn the shell on the async executor.
            let mut shell = match InteractiveShell::new(Some(&cwd)) {
                Ok(s) => s,
                Err(e) => {
                    let msg = format!("Failed to start shell: {e}");
                    let _ = this.update(app, |ws, cx| {
                        ws.terminal_data.push_system(&msg);
                        ws.terminal_data.is_running = false;
                        ws.terminal_cmd_tx = None;
                        cx.notify();
                    });
                    return;
                }
            };

            let _ = this.update(app, |ws, cx| {
                ws.terminal_data.push_system("Shell ready.");
                cx.notify();
            });

            // Main loop: poll shell output and command channel concurrently.
            loop {
                tokio::select! {
                    output = shell.read_async() => {
                        match output {
                            Some(ShellOutput::Stdout(line)) => {
                                let _ = this.update(app, |ws, cx| {
                                    ws.terminal_data.push_line(
                                        hive_ui_panels::panels::terminal::TerminalLineKind::Stdout,
                                        line,
                                    );
                                    cx.notify();
                                });
                            }
                            Some(ShellOutput::Stderr(line)) => {
                                let _ = this.update(app, |ws, cx| {
                                    ws.terminal_data.push_line(
                                        hive_ui_panels::panels::terminal::TerminalLineKind::Stderr,
                                        line,
                                    );
                                    cx.notify();
                                });
                            }
                            Some(ShellOutput::Exit(code)) => {
                                let msg = format!("Shell exited with code {code}");
                                let _ = this.update(app, |ws, cx| {
                                    ws.terminal_data.push_system(&msg);
                                    ws.terminal_data.is_running = false;
                                    ws.terminal_cmd_tx = None;
                                    cx.notify();
                                });
                                return;
                            }
                            None => {
                                // Channel closed — shell is gone.
                                let _ = this.update(app, |ws, cx| {
                                    ws.terminal_data.push_system("Shell disconnected.");
                                    ws.terminal_data.is_running = false;
                                    ws.terminal_cmd_tx = None;
                                    cx.notify();
                                });
                                return;
                            }
                        }
                    }
                    cmd = cmd_rx.recv() => {
                        match cmd {
                            Some(TerminalCmd::Write(text)) => {
                                if let Err(e) = shell.write(&format!("{text}\n")).await {
                                    let msg = format!("Write error: {e}");
                                    let _ = this.update(app, |ws, cx| {
                                        ws.terminal_data.push_system(&msg);
                                        cx.notify();
                                    });
                                }
                            }
                            Some(TerminalCmd::Kill) => {
                                let _ = shell.kill().await;
                                let _ = this.update(app, |ws, cx| {
                                    ws.terminal_data.push_system("Shell killed.");
                                    ws.terminal_data.is_running = false;
                                    ws.terminal_cmd_tx = None;
                                    cx.notify();
                                });
                                return;
                            }
                            None => {
                                // Command channel dropped — clean up.
                                let _ = shell.kill().await;
                                return;
                            }
                        }
                    }
                }
            }
        });
        self._terminal_task = Some(task);
        cx.notify();
    }

    fn handle_terminal_clear(
        &mut self,
        _action: &TerminalClear,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.terminal_data.lines.clear();
        cx.notify();
    }

    fn handle_terminal_submit(
        &mut self,
        _action: &TerminalSubmitCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let cmd = self.terminal_input.read(cx).text().to_string();
        let cmd = cmd.trim().to_string();
        if cmd.is_empty() {
            return;
        }
        // Clear input.
        self.terminal_input.update(cx, |input, cx| {
            input.set_value("", window, cx);
        });
        // Echo the command as a Stdin line.
        self.terminal_data.push_line(
            hive_ui_panels::panels::terminal::TerminalLineKind::Stdin,
            cmd.clone(),
        );

        // Send to background shell.
        if let Some(tx) = &self.terminal_cmd_tx {
            let _ = tx.send(TerminalCmd::Write(cmd));
        } else {
            self.terminal_data.push_system("No shell running. Restarting...");
            self.ensure_terminal_shell(cx);
        }
        cx.notify();
    }

    fn handle_terminal_kill(
        &mut self,
        _action: &TerminalKill,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(tx) = self.terminal_cmd_tx.take() {
            let _ = tx.send(TerminalCmd::Kill);
        }
        self._terminal_task = None;
        self.terminal_data.is_running = false;
        cx.notify();
    }

    fn handle_terminal_restart(
        &mut self,
        _action: &TerminalRestart,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Kill existing shell.
        if let Some(tx) = self.terminal_cmd_tx.take() {
            let _ = tx.send(TerminalCmd::Kill);
        }
        self._terminal_task = None;
        self.terminal_data.is_running = false;
        self.terminal_data.push_system("Restarting shell...");
        // Spawn new one.
        self.ensure_terminal_shell(cx);
    }

    // -- Tool Approval handlers ---------------------------------------------

    fn handle_tool_approve(
        &mut self,
        _action: &ToolApprove,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.chat_service.update(cx, |svc, cx| {
            svc.resolve_approval(true, cx);
        });
        cx.notify();
    }

    fn handle_tool_reject(
        &mut self,
        _action: &ToolReject,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.chat_service.update(cx, |svc, cx| {
            svc.resolve_approval(false, cx);
        });
        cx.notify();
    }

    // -- Quick Start panel handlers -----------------------------------------

    fn refresh_quick_start_data(&mut self, cx: &mut Context<Self>) {
        let selected_template = self.quick_start_data.selected_template.clone();
        let last_launch_status = self.quick_start_data.last_launch_status.clone();
        self.quick_start_data = Self::build_quick_start_data(
            &self.current_project_root,
            &self.current_project_name,
            &self.chat_service,
            &selected_template,
            last_launch_status,
            cx,
        );
    }

    fn build_quick_start_data(
        project_root: &Path,
        project_name: &str,
        chat_service: &Entity<ChatService>,
        selected_template: &str,
        last_launch_status: Option<String>,
        cx: &App,
    ) -> QuickStartPanelData {
        let templates = quick_start_templates();
        let selected_template = templates
            .iter()
            .find(|template| template.id == selected_template)
            .map(|template| template.id.clone())
            .unwrap_or_else(|| "dogfood".into());

        let current_model = chat_service.read(cx).current_model().to_string();
        let has_selected_model = !current_model.trim().is_empty();

        let (has_cloud_runtime, has_local_runtime) = if cx.has_global::<AppAiService>() {
            let ai = &cx.global::<AppAiService>().0;
            let has_cloud = ai.available_providers().iter().any(|provider| {
                matches!(
                    provider,
                    hive_ai::types::ProviderType::Anthropic
                        | hive_ai::types::ProviderType::OpenAI
                        | hive_ai::types::ProviderType::OpenRouter
                        | hive_ai::types::ProviderType::Google
                        | hive_ai::types::ProviderType::Groq
                        | hive_ai::types::ProviderType::HuggingFace
                        | hive_ai::types::ProviderType::XAI
                        | hive_ai::types::ProviderType::Mistral
                        | hive_ai::types::ProviderType::Venice
                )
            });
            let has_local = ai
                .discovery()
                .map(|discovery| discovery.snapshot().any_online())
                .unwrap_or(false);
            (has_cloud, has_local)
        } else {
            (false, false)
        };
        let has_ai_runtime = has_cloud_runtime || has_local_runtime;

        let knowledge_files = if cx.has_global::<AppKnowledgeFiles>() {
            cx.global::<AppKnowledgeFiles>().0.len()
        } else {
            0
        };

        let remote_agents = if cx.has_global::<AppA2aClient>() {
            cx.global::<AppA2aClient>()
                .0
                .list_agents()
                .map(|agents| agents.len())
                .unwrap_or(0)
        } else {
            0
        };

        let (project_summary, total_files, key_symbols, dependencies) =
            if cx.has_global::<AppQuickIndex>() {
                let quick_index = cx.global::<AppQuickIndex>().0.clone();
                let summary = if quick_index.file_tree.summary.trim().is_empty() {
                    format!("Using {} as the active project root.", project_root.display())
                } else {
                    quick_index.file_tree.summary.clone()
                };
                (
                    summary,
                    quick_index.file_tree.total_files,
                    quick_index.key_symbols.len(),
                    quick_index.dependencies.len(),
                )
            } else {
                (
                    format!("Using {} as the active project root.", project_root.display()),
                    0,
                    0,
                    0,
                )
            };

        let launch_ready = has_ai_runtime && has_selected_model;
        let launch_hint = if !has_ai_runtime {
            "Connect at least one cloud or local model runtime in Settings before starting a guided run."
                .into()
        } else if !has_selected_model {
            "Choose a default model in Settings so Quick Start knows where to launch the project run."
                .into()
        } else {
            format!(
                "Ready to launch a fresh project run in Chat with {}.",
                current_model
            )
        };

        let setup = vec![
            QuickStartSetupDisplay {
                title: "Project context".into(),
                detail: format!(
                    "Workspace root: {}. Knowledge files loaded: {}.",
                    project_root.display(),
                    knowledge_files
                ),
                status_label: "Ready".into(),
                tone: QuickStartTone::Ready,
                action_label: Some("Open Files".into()),
                action_panel: Some(Panel::Files.to_stored().into()),
            },
            QuickStartSetupDisplay {
                title: "AI runtime".into(),
                detail: if has_ai_runtime {
                    format!(
                        "Cloud runtime: {}. Local runtime: {}.",
                        if has_cloud_runtime {
                            "connected"
                        } else {
                            "not connected"
                        },
                        if has_local_runtime { "online" } else { "offline" }
                    )
                } else {
                    "No cloud or local models are available yet.".into()
                },
                status_label: if has_ai_runtime {
                    "Connected".into()
                } else {
                    "Needs setup".into()
                },
                tone: if has_ai_runtime {
                    QuickStartTone::Ready
                } else {
                    QuickStartTone::Action
                },
                action_label: Some(if has_ai_runtime {
                    "Review Settings".into()
                } else {
                    "Connect Models".into()
                }),
                action_panel: Some(Panel::Settings.to_stored().into()),
            },
            QuickStartSetupDisplay {
                title: "Default model".into(),
                detail: if has_selected_model {
                    format!("Current launch model: {}.", current_model)
                } else {
                    "No default model is selected for Chat yet.".into()
                },
                status_label: if has_selected_model {
                    "Selected".into()
                } else {
                    "Choose one".into()
                },
                tone: if has_selected_model {
                    QuickStartTone::Ready
                } else {
                    QuickStartTone::Action
                },
                action_label: Some("Open Settings".into()),
                action_panel: Some(Panel::Settings.to_stored().into()),
            },
            QuickStartSetupDisplay {
                title: "Git and agent accelerators".into(),
                detail: format!(
                    "Git repo: {}. Remote A2A agents configured: {}.",
                    if project_root.join(".git").exists() {
                        "yes"
                    } else {
                        "no"
                    },
                    remote_agents
                ),
                status_label: if remote_agents > 0 {
                    "Optional boost".into()
                } else {
                    "Optional".into()
                },
                tone: QuickStartTone::Optional,
                action_label: Some("Open Agents".into()),
                action_panel: Some(Panel::Agents.to_stored().into()),
            },
        ];

        let next_steps = vec![
            QuickStartNextStepDisplay {
                title: "Review".into(),
                detail: "Inspect git status, diffs, and release readiness after the kickoff run starts."
                    .into(),
                panel: Panel::Review.to_stored().into(),
                action_label: "Open Git Ops".into(),
            },
            QuickStartNextStepDisplay {
                title: "Specs".into(),
                detail: "Turn the kickoff outcome into a crisp implementation plan when the work needs structure."
                    .into(),
                panel: Panel::Specs.to_stored().into(),
                action_label: "Open Specs".into(),
            },
            QuickStartNextStepDisplay {
                title: "Agents".into(),
                detail: "Use workflows or remote A2A agents when parts of the job should be delegated."
                    .into(),
                panel: Panel::Agents.to_stored().into(),
                action_label: "Open Agents".into(),
            },
            QuickStartNextStepDisplay {
                title: "Kanban".into(),
                detail: "Track execution once the first run identifies concrete follow-up tasks."
                    .into(),
                panel: Panel::Kanban.to_stored().into(),
                action_label: "Open Kanban".into(),
            },
        ];

        QuickStartPanelData {
            project_name: project_name.into(),
            project_root: project_root.display().to_string(),
            project_summary,
            total_files,
            key_symbols,
            dependencies,
            selected_template,
            templates,
            setup,
            next_steps,
            launch_ready,
            launch_hint,
            last_launch_status,
        }
    }

    fn handle_quick_start_select_template(
        &mut self,
        action: &QuickStartSelectTemplate,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.quick_start_data.selected_template = action.template_id.clone();
        self.quick_start_goal_input.update(cx, |input, cx| {
            input.set_placeholder(
                quick_start_template_placeholder(&action.template_id),
                window,
                cx,
            );
        });
        self.quick_start_data.last_launch_status = Some(format!(
            "Selected '{}' as the current Quick Start mission.",
            quick_start_template_title(&action.template_id)
        ));
        self.refresh_quick_start_data(cx);
        cx.notify();
    }

    fn handle_quick_start_open_panel(
        &mut self,
        action: &QuickStartOpenPanel,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.switch_to_panel(Panel::from_stored(&action.panel), cx);
    }

    fn handle_quick_start_run_project(
        &mut self,
        action: &QuickStartRunProject,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.chat_service.read(cx).is_streaming() {
            self.push_notification(
                cx,
                NotificationType::Warning,
                "Quick Start",
                "Wait for the current chat run to finish before starting another guided run.",
            );
            return;
        }

        if !self.quick_start_data.launch_ready {
            let message = if self
                .quick_start_data
                .setup
                .get(1)
                .is_some_and(|item| item.tone != QuickStartTone::Ready)
            {
                "Connect a cloud or local model runtime in Settings before launching Quick Start."
            } else {
                "Choose a default model in Settings before launching Quick Start."
            };

            self.quick_start_data.last_launch_status = Some(message.into());
            self.push_notification(cx, NotificationType::Warning, "Quick Start", message);
            self.switch_to_panel(Panel::Settings, cx);
            return;
        }

        let prompt = self.build_quick_start_prompt(&action.template_id, &action.detail);
        let template_title = quick_start_template_title(&action.template_id);
        self.quick_start_data.last_launch_status = Some(format!(
            "Started '{}' for {}.",
            template_title, self.current_project_name
        ));
        self.quick_start_goal_input.update(cx, |input, cx| {
            input.set_value(String::new(), window, cx);
        });

        self.chat_service.update(cx, |svc, _cx| {
            svc.new_conversation();
        });
        self.cached_chat_data.markdown_cache.clear();
        self.refresh_history();
        self.switch_to_panel(Panel::Chat, cx);
        self.handle_send_text(prompt, Vec::new(), window, cx);
    }

    fn build_quick_start_prompt(&self, template_id: &str, detail: &str) -> String {
        let mission = quick_start_template_instruction(template_id);
        let user_focus = if detail.trim().is_empty() {
            "Use your judgment from the current repository state and start with the highest-impact opportunity.".to_string()
        } else {
            let trimmed = detail.trim();
            if trimmed.len() > 500 {
                trimmed.chars().take(500).collect()
            } else {
                trimmed.to_string()
            }
        };

        format!(
            "You are kicking off work on the active project.\n\nProject: {}\nWorkspace root: {}\nMission: {}\nSpecific focus: {}\n\nExecution rules:\n1. Inspect the codebase, README or HIVE docs, and current git state before changing code.\n2. Summarize the relevant context briefly.\n3. Produce a concise impact-ordered execution plan.\n4. Start the first concrete task immediately instead of stopping at analysis.\n5. Keep changes integrated with the existing modules, tabs, and shared services.\n6. Use Review, Specs, Agents, and Kanban when they are the right handoff surfaces.\n\nMission details:\n{}",
            self.current_project_name,
            self.current_project_root.display(),
            quick_start_template_title(template_id),
            user_focus,
            mission,
        )
    }

    // -- Network panel handlers ----------------------------------------------

    fn handle_network_refresh(
        &mut self,
        _action: &NetworkRefresh,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        info!("Network: refresh");
        self.refresh_network_peer_data(cx);
        cx.notify();
    }

    /// Populate the network panel with peer data from the AppNetwork global.
    fn refresh_network_peer_data(&mut self, cx: &App) {
        if !cx.has_global::<AppNetwork>() {
            return;
        }
        let node = &cx.global::<AppNetwork>().0;
        self.network_peer_data.our_peer_id = node.peer_id().to_string();
        let mut peers: Vec<PeerDisplayInfo> = node
            .peers_snapshot()
            .into_iter()
            .map(|peer| PeerDisplayInfo {
                name: peer.identity.name,
                status: network_peer_status_label(&peer.state),
                address: peer.addr.to_string(),
                latency_ms: peer.latency_ms,
                last_seen: format_network_relative_time(peer.last_seen),
            })
            .collect();

        peers.sort_by(|a, b| {
            network_peer_status_rank(&a.status)
                .cmp(&network_peer_status_rank(&b.status))
                .then_with(|| a.name.cmp(&b.name))
        });

        self.network_peer_data.peers = peers;
    }

    // -- Agents panel handlers -----------------------------------------------

    fn handle_agents_refresh_remote_agents(
        &mut self,
        _action: &AgentsRefreshRemoteAgents,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !cx.has_global::<AppA2aClient>() {
            self.push_notification(
                cx,
                NotificationType::Error,
                "Remote Agents",
                "A2A client is not available",
            );
            return;
        }

        match cx.global::<AppA2aClient>().0.reload() {
            Ok(()) => {
                self.refresh_agents_data(cx);
                self.agents_data.remote_status = Some("Reloaded remote A2A agent config".into());
                self.push_notification(
                    cx,
                    NotificationType::Success,
                    "Remote Agents",
                    "Reloaded ~/.hive/a2a.toml",
                );
                cx.notify();
            }
            Err(e) => {
                self.agents_data.remote_status = Some(format!("Failed to reload config: {e}"));
                self.push_notification(
                    cx,
                    NotificationType::Error,
                    "Remote Agents",
                    format!("Failed to reload A2A config: {e}"),
                );
                cx.notify();
            }
        }
    }

    fn handle_agents_reload_workflows(
        &mut self,
        _action: &AgentsReloadWorkflows,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !cx.has_global::<AppAutomation>() {
            return;
        }

        let workspace_root = std::env::current_dir().unwrap_or_default();
        let report = {
            let automation = &mut cx.global_mut::<AppAutomation>().0;
            automation.ensure_builtin_workflows();
            automation.reload_user_workflows(&workspace_root)
        };

        info!(
            "Agents: reloaded workflows (loaded={}, failed={}, skipped={})",
            report.loaded, report.failed, report.skipped
        );

        if cx.has_global::<AppNotifications>() {
            let msg = format!(
                "Reloaded workflows: {} loaded, {} failed, {} skipped",
                report.loaded, report.failed, report.skipped
            );
            let notif_type = if report.failed > 0 {
                NotificationType::Warning
            } else {
                NotificationType::Success
            };
            cx.global_mut::<AppNotifications>()
                .0
                .push(AppNotification::new(notif_type, msg).with_title("Workflow Reload"));
        }

        for error in report.errors {
            warn!("Workflow load error: {error}");
        }

        self.refresh_agents_data(cx);
        cx.notify();
    }

    fn handle_agents_select_remote_agent(
        &mut self,
        action: &AgentsSelectRemoteAgent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.agents_data.selected_remote_agent = Some(action.agent_name.clone());
        if let Some(agent) = self
            .agents_data
            .remote_agents
            .iter()
            .find(|agent| agent.name == action.agent_name)
        {
            let skill_is_valid = self.agents_data.selected_remote_skill.as_ref().is_none_or(
                |skill| agent.skills.iter().any(|candidate| candidate == skill),
            );
            if !skill_is_valid {
                self.agents_data.selected_remote_skill = None;
            }
        }
        self.agents_data.remote_status =
            Some(format!("Selected remote agent '{}'", action.agent_name));
        cx.notify();
    }

    fn handle_agents_select_remote_skill(
        &mut self,
        action: &AgentsSelectRemoteSkill,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.agents_data.selected_remote_agent = Some(action.agent_name.clone());
        self.agents_data.selected_remote_skill = action.skill_id.clone();
        self.agents_data.remote_status = Some(if let Some(skill_id) = action.skill_id.as_ref() {
            format!("Pinned remote skill '{skill_id}'")
        } else {
            "Remote skill selection reset to auto".into()
        });
        cx.notify();
    }

    fn handle_agents_discover_remote_agent(
        &mut self,
        action: &AgentsDiscoverRemoteAgent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !cx.has_global::<AppA2aClient>() {
            self.push_notification(
                cx,
                NotificationType::Error,
                "Remote Agents",
                "A2A client is not available",
            );
            return;
        }

        self.agents_data.remote_busy = true;
        self.agents_data.remote_status =
            Some(format!("Discovering remote agent '{}'...", action.agent_name));
        cx.notify();

        let client = cx.global::<AppA2aClient>().0.clone();
        let agent_name = action.agent_name.clone();
        let (tx, rx) = tokio::sync::oneshot::channel();

        std::thread::spawn(move || {
            let result = match tokio::runtime::Runtime::new() {
                Ok(rt) => rt
                    .block_on(client.discover_agent(&agent_name))
                    .map_err(|e| e.to_string()),
                Err(e) => Err(format!("tokio runtime: {e}")),
            };
            let _ = tx.send(result);
        });

        cx.spawn(async move |this, app: &mut AsyncApp| {
            let result = rx.await.unwrap_or(Err("channel closed".into()));
            let _ = this.update(app, |this, cx| {
                this.agents_data.remote_busy = false;
                match result {
                    Ok(summary) => {
                        let skill_count = summary.skills.len();
                        let agent_name = summary.name.clone();
                        this.refresh_agents_data(cx);
                        this.agents_data.selected_remote_agent = Some(agent_name.clone());
                        this.agents_data.selected_remote_skill = None;
                        this.agents_data.remote_status = Some(format!(
                            "Discovered '{}' ({} skill{})",
                            agent_name,
                            skill_count,
                            if skill_count == 1 { "" } else { "s" }
                        ));
                        this.push_notification(
                            cx,
                            NotificationType::Success,
                            "Remote Agents",
                            format!("Discovered remote agent '{}'", agent_name),
                        );
                    }
                    Err(e) => {
                        this.agents_data.remote_status =
                            Some(format!("Remote discovery failed: {e}"));
                        this.push_notification(
                            cx,
                            NotificationType::Error,
                            "Remote Agents",
                            format!("Failed to discover remote agent: {e}"),
                        );
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn handle_agents_run_remote_agent(
        &mut self,
        action: &AgentsRunRemoteAgent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let prompt = action.prompt.trim();
        if prompt.is_empty() {
            self.push_notification(
                cx,
                NotificationType::Warning,
                "Remote Agents",
                "Enter a prompt before running a remote task",
            );
            return;
        }
        if !cx.has_global::<AppA2aClient>() {
            self.push_notification(
                cx,
                NotificationType::Error,
                "Remote Agents",
                "A2A client is not available",
            );
            return;
        }

        self.agents_data.remote_busy = true;
        self.agents_data.remote_status =
            Some(format!("Running remote task on '{}'...", action.agent_name));
        cx.notify();

        let client = cx.global::<AppA2aClient>().0.clone();
        let agent_name = action.agent_name.clone();
        let prompt = prompt.to_string();
        let skill_id = action.skill_id.clone();
        let agent_name_for_error = agent_name.clone();
        let skill_id_for_error = skill_id.clone();
        let (tx, rx) = tokio::sync::oneshot::channel();

        std::thread::spawn(move || {
            let result = match tokio::runtime::Runtime::new() {
                Ok(rt) => rt
                    .block_on(client.run_task(&agent_name, &prompt, skill_id.as_deref()))
                    .map_err(|e| e.to_string()),
                Err(e) => Err(format!("tokio runtime: {e}")),
            };
            let _ = tx.send(result);
        });

        cx.spawn(async move |this, app: &mut AsyncApp| {
            let result = rx.await.unwrap_or(Err("channel closed".into()));
            let _ = this.update(app, |this, cx| {
                this.agents_data.remote_busy = false;
                match result {
                    Ok(run) => {
                        this.agents_data.remote_status = Some(format!(
                            "Remote task '{}' completed on '{}'",
                            run.task_id, run.agent_name
                        ));
                        this.agents_data.remote_run_history.insert(
                            0,
                            hive_ui_panels::panels::agents::RemoteTaskDisplay {
                                agent_name: run.agent_name.clone(),
                                task_id: run.task_id.clone(),
                                state: run.state.clone(),
                                skill_id: run.skill_id.clone(),
                                output: run.output.clone(),
                                completed_at: Utc::now().format("%Y-%m-%d %H:%M").to_string(),
                                error: None,
                            },
                        );
                        this.agents_data.remote_run_history.truncate(8);
                        this.refresh_agents_data(cx);
                        this.agents_data.selected_remote_agent = Some(run.agent_name.clone());
                        this.push_notification(
                            cx,
                            NotificationType::Success,
                            "Remote Agents",
                            format!(
                                "Remote task '{}' completed on '{}'",
                                run.task_id, run.agent_name
                            ),
                        );
                    }
                    Err(e) => {
                        this.agents_data.remote_status =
                            Some(format!("Remote task failed: {e}"));
                        this.agents_data.remote_run_history.insert(
                            0,
                            hive_ui_panels::panels::agents::RemoteTaskDisplay {
                                agent_name: agent_name_for_error.clone(),
                                task_id: "error".into(),
                                state: "Failed".into(),
                                skill_id: skill_id_for_error.clone(),
                                output: String::new(),
                                completed_at: Utc::now().format("%Y-%m-%d %H:%M").to_string(),
                                error: Some(e.clone()),
                            },
                        );
                        this.agents_data.remote_run_history.truncate(8);
                        this.push_notification(
                            cx,
                            NotificationType::Error,
                            "Remote Agents",
                            format!("Remote task failed: {e}"),
                        );
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn handle_agents_run_workflow(
        &mut self,
        action: &AgentsRunWorkflow,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !cx.has_global::<AppAutomation>() {
            return;
        }

        let Some(workflow) = self.make_workflow_for_run(action, cx) else {
            return;
        };

        if cx.has_global::<AppNotifications>() {
            cx.global_mut::<AppNotifications>()
                .0
                .push(AppNotification::new(
                    NotificationType::Info,
                    format!(
                        "Running workflow '{}' ({} step(s)) from {} in {}",
                        workflow.id,
                        workflow.steps.len(),
                        if action.source.is_empty() {
                            "manual trigger"
                        } else {
                            action.source.as_str()
                        },
                        self.current_project_root.display()
                    ),
                ));
        }

        let working_dir = self
            .current_project_root
            .clone()
            .canonicalize()
            .unwrap_or_else(|_| self.current_project_root.clone());
        let workflow_for_thread = workflow.clone();
        let run_result = std::sync::Arc::new(std::sync::Mutex::new(None));
        let run_result_for_thread = std::sync::Arc::clone(&run_result);

        // Execute on a background OS thread so tokio process execution works
        // regardless of the UI executor.
        std::thread::spawn(move || {
            let result =
                hive_agents::automation::AutomationService::execute_workflow_blocking(
                    &workflow_for_thread,
                    working_dir,
                );
            *run_result_for_thread.lock().unwrap_or_else(|e| e.into_inner()) = Some(result);
        });

        let run_result_for_ui = std::sync::Arc::clone(&run_result);
        let workflow_id_for_ui = workflow.id.clone();

        cx.spawn(async move |this, app: &mut AsyncApp| {
            // Poll until the thread writes the result.
            loop {
                if let Some(result) = run_result_for_ui.lock().unwrap_or_else(|e| e.into_inner()).take() {
                    let _ = this.update(app, |this, cx| {
                        match result {
                            Ok(run) => {
                                let _ = cx.global_mut::<AppAutomation>().0.record_run(
                                    &run.workflow_id,
                                    run.success,
                                    run.steps_completed,
                                    run.error.clone(),
                                );

                                if cx.has_global::<AppNotifications>() {
                                    let notif_type = if run.success {
                                        NotificationType::Success
                                    } else {
                                        NotificationType::Error
                                    };
                                    let msg = if run.success {
                                        format!(
                                            "Workflow '{}' completed ({} steps)",
                                            run.workflow_id, run.steps_completed
                                        )
                                    } else {
                                        format!(
                                            "Workflow '{}' failed after {} step(s)",
                                            run.workflow_id, run.steps_completed
                                        )
                                    };
                                    cx.global_mut::<AppNotifications>().0.push(
                                        AppNotification::new(notif_type, msg).with_title(
                                            if run.success {
                                                "Workflow Complete"
                                            } else {
                                                "Workflow Failed"
                                            },
                                        ),
                                    );
                                }
                            }
                            Err(e) => {
                                warn!("Agents: workflow run error ({workflow_id_for_ui}): {e}");
                                if cx.has_global::<AppNotifications>() {
                                    cx.global_mut::<AppNotifications>().0.push(
                                        AppNotification::new(
                                            NotificationType::Error,
                                            format!("Workflow '{workflow_id_for_ui}' failed: {e}"),
                                        )
                                        .with_title("Workflow Run Failed"),
                                    );
                                }
                            }
                        }

                        this.refresh_agents_data(cx);
                        cx.notify();
                    });
                    break;
                }

                app.background_executor()
                    .timer(std::time::Duration::from_millis(120))
                    .await;
            }
        })
        .detach();
    }

    fn make_workflow_for_run(
        &self,
        action: &AgentsRunWorkflow,
        cx: &App,
    ) -> Option<hive_agents::automation::Workflow> {
        if !cx.has_global::<AppAutomation>() {
            return None;
        }

        let requested_id = if action.workflow_id.trim().is_empty() {
            hive_agents::automation::BUILTIN_DOGFOOD_WORKFLOW_ID.to_string()
        } else {
            action.workflow_id.clone()
        };

        let automation = &cx.global::<AppAutomation>().0;
        let workflow = automation
            .clone_workflow(&requested_id)
            .or_else(|| automation.clone_workflow(hive_agents::automation::BUILTIN_DOGFOOD_WORKFLOW_ID))
            .or_else(|| Some(Self::fallback_workflow(&requested_id)));

        let Some(mut workflow) = workflow else {
            warn!(
                "Agents: unable to resolve workflow '{requested_id}' for planned execution"
            );
            return None;
        };

        let instruction = action.instruction.trim();
        if !instruction.is_empty() {
            let planned_steps =
                self.workflow_steps_from_instruction(instruction, &action.source, &action.source_id, cx);
            if !planned_steps.is_empty() {
                workflow.steps = planned_steps;
                workflow.name = if action.source.is_empty() {
                    "Planned Workflow".to_string()
                } else if action.source_id.is_empty() {
                    format!("Planned Workflow ({})", action.source)
                } else {
                    format!("Planned Workflow ({}:{})", action.source, action.source_id)
                };
                workflow.description = format!(
                    "Planned execution for {} {}",
                    if action.source.is_empty() {
                        "manual action"
                    } else {
                        action.source.as_str()
                    },
                    if action.source_id.is_empty() {
                        "request"
                    } else {
                        action.source_id.as_str()
                    }
                );
            }
        }

        if workflow.steps.is_empty() {
            workflow.steps = self.fallback_workflow_steps();
        }

        Some(workflow)
    }

    fn workflow_steps_from_instruction(
        &self,
        instruction: &str,
        source: &str,
        source_id: &str,
        cx: &App,
    ) -> Vec<hive_agents::automation::WorkflowStep> {
        let explicit = Self::extract_explicit_commands(instruction);
        let mut commands = if explicit.is_empty() {
            self.extract_keyword_commands(instruction)
                .into_iter()
                .chain(self.extract_source_commands(source, source_id, cx))
                .collect::<Vec<_>>()
        } else {
            explicit
        };

        commands = Self::dedupe_preserve_order(commands);
        if commands.is_empty() {
            commands = self.fallback_workflow_commands();
        }

        commands
            .into_iter()
            .enumerate()
            .map(|(idx, command)| hive_agents::automation::WorkflowStep {
                id: format!("runtime:{idx}"),
                name: format!("Run command {idx}"),
                action: hive_agents::automation::ActionType::RunCommand { command },
                conditions: Vec::new(),
                timeout_secs: Some(900),
                retry_count: 0,
            })
            .collect()
    }

    fn extract_explicit_commands(instruction: &str) -> Vec<String> {
        let mut commands = Vec::new();
        let mut in_fence = false;

        for line in instruction.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            if line.starts_with("```") {
                in_fence = !in_fence;
                continue;
            }

            if in_fence {
                Self::add_command_if_valid(line, &mut commands);
                continue;
            }

            let mut remaining = line;
            while let Some(start) = remaining.find('`') {
                let after = &remaining[start + 1..];
                let Some(end) = after.find('`') else {
                    break;
                };
                let candidate = &after[..end];
                Self::add_command_if_valid(candidate, &mut commands);
                remaining = &after[end + 1..];
            }

            if let Some((prefix, rest)) = line.split_once(':') {
                let normalized = prefix.trim().to_ascii_lowercase();
                if matches!(
                    normalized.as_str(),
                    "run" | "command" | "run command" | "execute"
                ) {
                    Self::add_command_if_valid(rest, &mut commands);
                    continue;
                }
            }

            Self::add_command_if_valid(line, &mut commands);
        }

        commands
    }

    fn extract_keyword_commands(&self, instruction: &str) -> Vec<String> {
        let lower = instruction.to_lowercase();
        let mut commands = Vec::new();

        if lower.contains("build") {
            commands.push("cargo check --quiet".to_string());
        }

        if lower.contains("test") {
            commands.push("cargo test --quiet -p hive_app".to_string());
        }

        if lower.contains("lint") || lower.contains("format") {
            commands.push("cargo fmt --check".to_string());
            commands.push("cargo clippy --all-targets -- -D warnings".to_string());
        }

        if lower.contains("release") {
            commands.push("cargo build --release".to_string());
        }

        if lower.contains("docs") {
            commands.push("cargo doc --no-deps --all-features".to_string());
        }

        if lower.contains("status") {
            commands.push("git status --short".to_string());
        }

        if lower.contains("diff") {
            commands.push("git diff --stat".to_string());
        }

        Self::dedupe_preserve_order(commands)
    }

    fn extract_source_commands(&self, source: &str, source_id: &str, cx: &App) -> Vec<String> {
        let source = source.to_lowercase();
        let mut commands = Vec::new();

        if source == "spec" && !source_id.is_empty() && cx.has_global::<AppSpecs>()
            && let Some(spec) = cx.global::<AppSpecs>().0.specs.get(source_id)
        {
            if spec.entry_count() == 0 || spec.checked_count() < spec.entry_count() {
                commands.push("cargo check --quiet".to_string());
            }
            commands.push("cargo test --quiet -p hive_app".to_string());
        }

        if source == "kanban-task" && !source_id.is_empty() {
            let task_id: u64 = source_id.parse().unwrap_or(0);
            if task_id > 0 {
                for col in &self.kanban_data.columns {
                    if let Some(task) = col.tasks.iter().find(|task| task.id == task_id) {
                        let title = task.title.to_lowercase();
                        let desc = task.description.to_lowercase();
                        if title.contains("build") || desc.contains("build") {
                            commands.push("cargo check --quiet".to_string());
                        }
                        if title.contains("test") || desc.contains("test") {
                            commands.push("cargo test --quiet -p hive_app".to_string());
                        }
                        if title.contains("lint") || desc.contains("lint") {
                            commands.push("cargo fmt --check".to_string());
                            commands.push("cargo clippy --all-targets -- -D warnings".to_string());
                        }
                        break;
                    }
                }
            }
        }

        Self::dedupe_preserve_order(commands)
    }

    fn add_command_if_valid(raw: &str, out: &mut Vec<String>) {
        let Some(command) = Self::normalize_command(raw) else {
            return;
        };
        out.push(command);
    }

    fn normalize_command(raw: &str) -> Option<String> {
        let command = raw
            .trim()
            .trim_matches(['"', '\'', '`'])
            .trim_end_matches(';')
            .trim();
        if command.is_empty() || !Self::is_command_like(command) {
            return None;
        }
        Some(command.to_string())
    }

    fn is_command_like(text: &str) -> bool {
        let lower = text.to_lowercase();
        const PREFIXES: [&str; 11] = [
            "cargo ",
            "git ",
            "npm ",
            "pnpm ",
            "yarn ",
            "make ",
            "python ",
            "pytest ",
            "cargo.exe ",
            "./",
            "bash ",
        ];
        PREFIXES.iter().any(|prefix| lower.starts_with(prefix))
            || lower == "cargo"
            || lower == "git"
    }

    fn dedupe_preserve_order(commands: Vec<String>) -> Vec<String> {
        let mut seen = HashSet::new();
        commands
            .into_iter()
            .filter(|command| seen.insert(command.clone()))
            .collect()
    }

    fn fallback_workflow(workflow_id: &str) -> hive_agents::automation::Workflow {
        Self::fallback_workflow_with_id(workflow_id)
    }

    fn fallback_workflow_with_id(workflow_id: &str) -> hive_agents::automation::Workflow {
        let now = chrono::Utc::now();
        hive_agents::automation::Workflow {
            id: workflow_id.to_string(),
            name: "Local Build Check".to_string(),
            description: "Fallback local validation loop.".to_string(),
            trigger: hive_agents::automation::TriggerType::ManualTrigger,
            steps: Self::fallback_workflow_steps_static(),
            status: hive_agents::automation::WorkflowStatus::Active,
            created_at: now,
            updated_at: now,
            last_run: None,
            run_count: 0,
        }
    }

    fn fallback_workflow_steps(&self) -> Vec<hive_agents::automation::WorkflowStep> {
        Self::fallback_workflow_steps_static()
    }

    fn fallback_workflow_steps_static() -> Vec<hive_agents::automation::WorkflowStep> {
        vec![
            hive_agents::automation::WorkflowStep {
                id: "fallback:check".to_string(),
                name: "Cargo check".to_string(),
                action: hive_agents::automation::ActionType::RunCommand {
                    command: "cargo check --quiet".to_string(),
                },
                conditions: Vec::new(),
                timeout_secs: Some(900),
                retry_count: 0,
            },
            hive_agents::automation::WorkflowStep {
                id: "fallback:test".to_string(),
                name: "Cargo test".to_string(),
                action: hive_agents::automation::ActionType::RunCommand {
                    command: "cargo test --quiet -p hive_app".to_string(),
                },
                conditions: Vec::new(),
                timeout_secs: Some(1200),
                retry_count: 0,
            },
            hive_agents::automation::WorkflowStep {
                id: "fallback:status".to_string(),
                name: "Git status".to_string(),
                action: hive_agents::automation::ActionType::RunCommand {
                    command: "git status --short".to_string(),
                },
                conditions: Vec::new(),
                timeout_secs: Some(120),
                retry_count: 0,
            },
            hive_agents::automation::WorkflowStep {
                id: "fallback:diff".to_string(),
                name: "Git diff".to_string(),
                action: hive_agents::automation::ActionType::RunCommand {
                    command: "git diff --stat".to_string(),
                },
                conditions: Vec::new(),
                timeout_secs: Some(120),
                retry_count: 0,
            },
        ]
    }

    fn fallback_workflow_commands(&self) -> Vec<String> {
        Self::fallback_workflow_steps_static()
            .into_iter()
            .filter_map(|step| match step.action {
                hive_agents::automation::ActionType::RunCommand { command } => Some(command),
                _ => None,
            })
            .collect()
    }

    // -- Files panel handlers ------------------------------------------------

    fn handle_files_navigate_back(
        &mut self,
        _action: &FilesNavigateBack,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(parent) = self.files_data.current_path.parent() {
            let parent = parent.to_path_buf();
            info!("Files: navigate back to {}", parent.display());
            self.apply_project_context(&parent, cx);
            self.files_data = FilesData::from_path(&parent);
            cx.notify();
        }
    }

    fn handle_files_navigate_to(
        &mut self,
        action: &FilesNavigateTo,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let path = PathBuf::from(&action.path);
        info!("Files: navigate to {}", path.display());
        self.apply_project_context(&path, cx);
        self.files_data = FilesData::from_path(&path);
        cx.notify();
    }

    fn handle_files_open_entry(
        &mut self,
        action: &FilesOpenEntry,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if action.is_directory {
            let new_path = self.files_data.current_path.join(&action.name);
            info!("Files: open directory {}", new_path.display());
            self.apply_project_context(&new_path, cx);
            self.files_data = FilesData::from_path(&new_path);
        } else {
            let file_path = self.files_data.current_path.join(&action.name);
            // Security: canonicalize and validate path stays within current_path
            // to prevent path traversal before passing to OS shell commands.
            let file_path = match file_path.canonicalize() {
                Ok(p) => p,
                Err(e) => {
                    error!("Files: cannot resolve path: {e}");
                    return;
                }
            };
            let base = match self.files_data.current_path.canonicalize() {
                Ok(p) => p,
                Err(e) => {
                    error!("Files: cannot resolve base path: {e}");
                    return;
                }
            };
            if !file_path.starts_with(&base) {
                error!("Files: path traversal blocked: {}", file_path.display());
                return;
            }
            info!("Files: open file in viewer {}", file_path.display());
            self.files_data.selected_file = Some(action.name.clone());
            // Open in the built-in file viewer pane.
            self.files_data.open_file_viewer(&file_path);
            cx.notify();
        }
    }

    fn handle_files_close_viewer(
        &mut self,
        _action: &FilesCloseViewer,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.files_data.close_file_viewer();
        cx.notify();
    }

    fn handle_files_toggle_check(
        &mut self,
        action: &FilesToggleCheck,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let path = std::path::PathBuf::from(&action.path);
        self.files_data.toggle_check(&path);

        // Update the global context selection state.
        if cx.has_global::<AppContextSelection>() {
            let paths = self.files_data.checked_paths();
            let total_tokens: usize = paths
                .iter()
                .map(|p| {
                    std::fs::metadata(p)
                        .map(|m| m.len() as usize)
                        .unwrap_or(0)
                        / 4
                })
                .sum();
            let sel = cx.global::<AppContextSelection>().0.clone();
            if let Ok(mut guard) = sel.lock() {
                guard.selected_files = paths;
                guard.total_tokens = total_tokens;
            }
        }
        cx.notify();
    }

    fn handle_files_clear_checked(
        &mut self,
        _action: &FilesClearChecked,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.files_data.clear_checked();

        if cx.has_global::<AppContextSelection>() {
            let sel = cx.global::<AppContextSelection>().0.clone();
            if let Ok(mut guard) = sel.lock() {
                guard.selected_files.clear();
                guard.total_tokens = 0;
            }
        }
        cx.notify();
    }

    fn handle_apply_code_block(
        &mut self,
        action: &ApplyCodeBlock,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let file_path = self.current_project_root.join(&action.file_path);

        // Security: validate file path
        if cx.has_global::<AppSecurity>() {
            if let Err(e) = cx.global::<AppSecurity>().0.check_path(&file_path) {
                error!("Apply: blocked path: {e}");
                self.chat_service.update(cx, |svc, cx| {
                    svc.set_error(format!("Apply blocked: {e}"), cx);
                });
                return;
            }
        }

        // Read old content for diff display
        let old_content = std::fs::read_to_string(&file_path).ok();
        let new_content = action.content.clone();

        // Compute diff
        let diff_lines = if let Some(ref old) = old_content {
            hive_ui_panels::components::diff_viewer::compute_diff_lines_public(old, &new_content)
        } else {
            new_content
                .lines()
                .map(|l| hive_ui_panels::components::DiffLine::Added(l.to_string()))
                .collect()
        };

        // Create pending approval using existing tool approval pattern
        self.chat_service.update(cx, |svc, cx| {
            svc.pending_approval = Some(crate::chat_service::PendingToolApproval {
                tool_call_id: format!("apply-{}", action.file_path),
                tool_name: "apply_code_block".to_string(),
                file_path: file_path.to_string_lossy().to_string(),
                new_content: new_content.clone(),
                old_content,
                diff_lines,
            });
            cx.notify();
        });
        cx.notify();
    }

    fn handle_apply_all_edits(
        &mut self,
        _action: &ApplyAllEdits,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Parse the latest assistant message for edits
        let last_assistant_content = self.chat_service.read(cx).messages.iter().rev()
            .find(|m| m.role == MessageRole::Assistant)
            .map(|m| m.content.clone())
            .unwrap_or_default();

        let edits = hive_agents::parse_edits(&last_assistant_content);
        if edits.is_empty() {
            self.chat_service.update(cx, |svc, cx| {
                svc.set_error("No file edits found in the last response", cx);
            });
            return;
        }

        // Apply each edit sequentially
        for edit in &edits {
            let file_path = self.current_project_root.join(&edit.file_path);
            if let Err(e) = std::fs::write(&file_path, &edit.new_content) {
                error!("Apply all: failed to write {}: {e}", edit.file_path);
            } else {
                info!("Applied edit to {}", edit.file_path);
            }
        }

        info!("Applied {} file edit(s) from response", edits.len());
        cx.notify();
    }

    fn handle_copy_to_clipboard(
        &mut self,
        action: &CopyToClipboard,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(action.content.clone()));
    }

    fn handle_copy_full_prompt(
        &mut self,
        _action: &CopyFullPrompt,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut prompt = String::new();

        // Include selected context files
        if cx.has_global::<AppContextSelection>() {
            let sel = cx.global::<AppContextSelection>().0.clone();
            if let Ok(guard) = sel.lock() {
                for path in &guard.selected_files {
                    let rel = path
                        .strip_prefix(&self.current_project_root)
                        .unwrap_or(path);
                    let content = std::fs::read_to_string(path).unwrap_or_default();
                    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                    prompt.push_str(&format!("## {}\n```{}\n{}\n```\n\n", rel.display(), ext, content));
                }
            }
        }

        // Include chat input text
        let text = self.chat_input.read(cx).current_text(cx);
        if !text.is_empty() {
            prompt.push_str(&format!("## Instruction\n{}\n", text));
        }

        cx.write_to_clipboard(gpui::ClipboardItem::new_string(prompt));
    }

    fn handle_export_prompt(
        &mut self,
        _action: &ExportPrompt,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut prompt = String::new();

        if cx.has_global::<AppContextSelection>() {
            let sel = cx.global::<AppContextSelection>().0.clone();
            if let Ok(guard) = sel.lock() {
                for path in &guard.selected_files {
                    let rel = path
                        .strip_prefix(&self.current_project_root)
                        .unwrap_or(path);
                    let content = std::fs::read_to_string(path).unwrap_or_default();
                    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                    prompt.push_str(&format!("## {}\n```{}\n{}\n```\n\n", rel.display(), ext, content));
                }
            }
        }

        let export_path = self.current_project_root.join("hive-prompt-export.md");
        if let Err(e) = std::fs::write(&export_path, &prompt) {
            error!("Export prompt failed: {e}");
        } else {
            info!("Exported prompt to {}", export_path.display());
        }
    }

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

    fn handle_files_delete_entry(
        &mut self,
        action: &FilesDeleteEntry,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let target = self.files_data.current_path.join(&action.name);
        // Security: canonicalize and validate target stays within current_path
        // to prevent path traversal attacks (e.g. action.name = "../../etc").
        let target = match target.canonicalize() {
            Ok(p) => p,
            Err(e) => {
                error!("Files: cannot resolve path: {e}");
                return;
            }
        };
        let base = match self.files_data.current_path.canonicalize() {
            Ok(p) => p,
            Err(e) => {
                error!("Files: cannot resolve base path: {e}");
                return;
            }
        };
        if !target.starts_with(&base) {
            error!("Files: path traversal blocked: {}", target.display());
            return;
        }
        info!("Files: delete {}", target.display());
        let result = if target.is_dir() {
            std::fs::remove_dir_all(&target)
        } else {
            std::fs::remove_file(&target)
        };
        if let Err(e) = result {
            warn!("Files: failed to delete {}: {e}", target.display());
        }
        // Refresh the listing
        self.files_data = FilesData::from_path(&self.files_data.current_path.clone());
        cx.notify();
    }

    fn handle_files_refresh(
        &mut self,
        _action: &FilesRefresh,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        info!("Files: refresh");
        self.files_data = FilesData::from_path(&self.files_data.current_path.clone());
        cx.notify();
    }

    fn handle_files_new_file(
        &mut self,
        _action: &FilesNewFile,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let path = self.files_data.current_path.join("untitled.txt");
        info!("Files: create new file {}", path.display());
        if let Err(e) = std::fs::write(&path, "") {
            warn!("Files: failed to create file: {e}");
        }
        self.files_data = FilesData::from_path(&self.files_data.current_path.clone());
        cx.notify();
    }

    fn handle_files_new_folder(
        &mut self,
        _action: &FilesNewFolder,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let path = self.files_data.current_path.join("new_folder");
        info!("Files: create new folder {}", path.display());
        if let Err(e) = std::fs::create_dir(&path) {
            warn!("Files: failed to create folder: {e}");
        }
        self.files_data = FilesData::from_path(&self.files_data.current_path.clone());
        cx.notify();
    }

    // -- History panel handlers ----------------------------------------------

    fn handle_history_load(
        &mut self,
        action: &HistoryLoadConversation,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        info!("History: load conversation {}", action.conversation_id);
        let result = self.chat_service.update(cx, |svc, _cx| {
            svc.load_conversation(&action.conversation_id)
        });
        match result {
            Ok(()) => {
                self.cached_chat_data.markdown_cache.clear();
                self.sidebar.active_panel = Panel::Chat;
                self.session_dirty = true;
            }
            Err(e) => warn!("History: failed to load conversation: {e}"),
        }
        cx.notify();
    }

    fn handle_history_delete(
        &mut self,
        action: &HistoryDeleteConversation,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        info!("History: delete conversation {}", action.conversation_id);
        if let Ok(store) = hive_core::ConversationStore::new()
            && let Err(e) = store.delete(&action.conversation_id)
        {
            warn!("History: failed to delete conversation: {e}");
        }
        self.refresh_history();
        cx.notify();
    }

    fn handle_history_refresh(
        &mut self,
        _action: &HistoryRefresh,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.refresh_history();
        cx.notify();
    }

    fn handle_history_clear_all(
        &mut self,
        _action: &HistoryClearAll,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        info!("History: clear all requested — showing confirmation");
        self.history_data.confirming_clear = true;
        cx.notify();
    }

    fn handle_history_clear_all_confirm(
        &mut self,
        _action: &HistoryClearAllConfirm,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        info!("History: clear all confirmed — deleting all conversations");

        // Delete JSON files
        if let Ok(store) = hive_core::ConversationStore::new() {
            match store.delete_all() {
                Ok(count) => info!("History: deleted {count} conversation files"),
                Err(e) => warn!("History: failed to delete conversation files: {e}"),
            }
        }

        // Delete from SQLite database
        if let Ok(db) = hive_core::persistence::Database::open() {
            match db.clear_all_conversations() {
                Ok(count) => info!("History: deleted {count} conversations from database"),
                Err(e) => warn!("History: failed to clear conversations from database: {e}"),
            }
        }

        // Reset the current conversation
        self.chat_service.update(cx, |svc, _cx| {
            svc.new_conversation();
        });
        self.cached_chat_data.markdown_cache.clear();

        self.history_data = HistoryData::empty();
        self.session_dirty = true;
        cx.notify();
    }

    fn handle_history_clear_all_cancel(
        &mut self,
        _action: &HistoryClearAllCancel,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        info!("History: clear all cancelled");
        self.history_data.confirming_clear = false;
        cx.notify();
    }

    // -- Kanban panel handlers -----------------------------------------------

    fn handle_kanban_add_task(
        &mut self,
        _action: &KanbanAddTask,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use hive_ui_panels::panels::kanban::{KanbanTask, Priority};
        info!("Kanban: add task");
        let task = KanbanTask {
            id: self
                .kanban_data
                .columns
                .iter()
                .map(|c| c.tasks.len() as u64)
                .sum::<u64>()
                + 1,
            title: "New Task".to_string(),
            description: String::new(),
            priority: Priority::Medium,
            created_at: chrono::Utc::now().format("%Y-%m-%d %H:%M").to_string(),
            assigned_model: None,
        };
        self.kanban_data.columns[0].tasks.push(task);
        self.save_kanban_data();
        cx.notify();
    }

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

    fn run_checked_git_command(
        &self,
        cx: &Context<Self>,
        args: &[&str],
        security_check: &str,
    ) -> Result<std::process::Output, String> {
        if cx.has_global::<AppSecurity>() {
            cx.global::<AppSecurity>().0.check_command(security_check)?;
        }

        std::process::Command::new("git")
            .args(args)
            .current_dir(&self.current_project_root)
            .output()
            .map_err(|e| format!("Failed to run git {}: {e}", args.join(" ")))
    }

    fn handle_logs_clear(
        &mut self,
        _action: &LogsClear,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        info!("Logs: clear");
        self.logs_data.entries.clear();
        if cx.has_global::<AppDatabase>() {
            let db = &cx.global::<AppDatabase>().0;
            if let Err(e) = db.clear_logs() {
                warn!("Failed to clear persisted logs: {e}");
            }
        }
        cx.notify();
    }

    fn handle_logs_set_filter(
        &mut self,
        action: &LogsSetFilter,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use hive_ui_panels::panels::logs::LogLevel;
        info!("Logs: set filter to {}", action.level);
        self.logs_data.filter = match action.level.as_str() {
            "error" => LogLevel::Error,
            "warning" => LogLevel::Warning,
            "info" => LogLevel::Info,
            _ => LogLevel::Debug,
        };
        cx.notify();
    }

    fn handle_logs_toggle_auto_scroll(
        &mut self,
        _action: &LogsToggleAutoScroll,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.logs_data.auto_scroll = !self.logs_data.auto_scroll;
        cx.notify();
    }

    // -- Costs panel handlers ------------------------------------------------

    fn handle_costs_export_csv(
        &mut self,
        _action: &CostsExportCsv,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        info!("Costs: export CSV");
        let Some(csv) = cx
            .has_global::<AppAiService>()
            .then(|| cx.global::<AppAiService>().0.cost_tracker().export_csv())
        else {
            self.push_notification(
                cx,
                NotificationType::Warning,
                "Cost Export",
                "No cost tracker available.",
            );
            return;
        };

        let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
        let export_dir = HiveConfig::base_dir()
            .map(|d| d.join("exports"))
            .unwrap_or_else(|_| PathBuf::from(".hive/exports"));
        let export_path = export_dir.join(format!("costs-{timestamp}.csv"));

        let result = (|| -> anyhow::Result<()> {
            std::fs::create_dir_all(&export_dir)?;
            std::fs::write(&export_path, csv)?;
            Ok(())
        })();

        match result {
            Ok(()) => {
                self.push_notification(
                    cx,
                    NotificationType::Success,
                    "Cost Export",
                    format!("Exported CSV to {}", export_path.display()),
                );
            }
            Err(e) => {
                error!("Costs: failed to export CSV: {e}");
                self.push_notification(
                    cx,
                    NotificationType::Error,
                    "Cost Export",
                    format!("Failed to export CSV: {e}"),
                );
            }
        }
    }

    fn handle_costs_reset_today(
        &mut self,
        _action: &CostsResetToday,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        info!("Costs: reset today");
        if cx.has_global::<AppAiService>() {
            cx.global_mut::<AppAiService>()
                .0
                .cost_tracker_mut()
                .reset_today();
        }
        cx.notify();
    }

    fn handle_costs_clear_history(
        &mut self,
        _action: &CostsClearHistory,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        info!("Costs: clear all history");
        if cx.has_global::<AppAiService>() {
            cx.global_mut::<AppAiService>().0.cost_tracker_mut().clear();
        }
        cx.notify();
    }

    // -- Review panel handlers -----------------------------------------------

    fn handle_review_stage_all(
        &mut self,
        _action: &ReviewStageAll,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        info!("Review: stage all");
        match self.run_checked_git_command(cx, &["add", "-A"], "git add -A") {
            Ok(output) if output.status.success() => {
                self.review_data = ReviewData::from_git(&self.current_project_root);
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                self.push_notification(
                    cx,
                    NotificationType::Error,
                    "Review",
                    format!("git add -A failed: {}", stderr.trim()),
                );
            }
            Err(e) => {
                self.push_notification(cx, NotificationType::Error, "Review", e);
            }
        }
        cx.notify();
    }

    fn handle_review_unstage_all(
        &mut self,
        _action: &ReviewUnstageAll,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        info!("Review: unstage all");
        match self.run_checked_git_command(cx, &["reset", "HEAD"], "git reset HEAD") {
            Ok(output) if output.status.success() => {
                self.review_data = ReviewData::from_git(&self.current_project_root);
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                self.push_notification(
                    cx,
                    NotificationType::Error,
                    "Review",
                    format!("git reset HEAD failed: {}", stderr.trim()),
                );
            }
            Err(e) => {
                self.push_notification(cx, NotificationType::Error, "Review", e);
            }
        }
        cx.notify();
    }

    fn handle_review_commit(
        &mut self,
        _action: &ReviewCommit,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        info!("Review: commit");
        let staged = self.review_data.staged_count;
        let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M UTC");
        let message = if staged > 0 {
            format!("chore(review): apply {staged} staged change(s) ({timestamp})")
        } else {
            format!("chore(review): snapshot commit ({timestamp})")
        };

        match self.run_checked_git_command(cx, &["commit", "-m", &message], "git commit -m") {
            Ok(output) if output.status.success() => {
                let commit_hash = self
                    .run_checked_git_command(
                        cx,
                        &["rev-parse", "--short", "HEAD"],
                        "git rev-parse HEAD",
                    )
                    .ok()
                    .and_then(|o| {
                        if o.status.success() {
                            Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
                        } else {
                            None
                        }
                    })
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| "unknown".to_string());

                self.review_data = ReviewData::from_git(&self.current_project_root);
                self.push_notification(
                    cx,
                    NotificationType::Success,
                    "Review",
                    format!("Created commit {commit_hash}"),
                );
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let stdout = String::from_utf8_lossy(&output.stdout);
                let msg = if !stderr.trim().is_empty() {
                    stderr.trim().to_string()
                } else if !stdout.trim().is_empty() {
                    stdout.trim().to_string()
                } else {
                    "git commit failed".to_string()
                };
                self.push_notification(cx, NotificationType::Warning, "Review", msg);
            }
            Err(e) => {
                self.push_notification(cx, NotificationType::Error, "Review", e);
            }
        }
        cx.notify();
    }

    fn handle_review_discard_all(
        &mut self,
        _action: &ReviewDiscardAll,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        info!("Review: discard all");
        match self.run_checked_git_command(cx, &["checkout", "--", "."], "git checkout -- .") {
            Ok(output) if output.status.success() => {
                self.review_data = ReviewData::from_git(&self.current_project_root);
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                self.push_notification(
                    cx,
                    NotificationType::Error,
                    "Review",
                    format!("git checkout -- . failed: {}", stderr.trim()),
                );
            }
            Err(e) => {
                self.push_notification(cx, NotificationType::Error, "Review", e);
            }
        }
        cx.notify();
    }

    // -- Git Ops handlers (tab switching, AI commit, push, branches, PRs, LFS, gitflow) ---

    fn handle_review_switch_tab(
        &mut self,
        action: &ReviewSwitchTab,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let tab = GitOpsTab::parse_tab(&action.tab);
        self.review_data.active_tab = tab;
        match tab {
            GitOpsTab::Push => self.refresh_push_data(cx),
            GitOpsTab::Branches => self.refresh_branches_data(cx),
            GitOpsTab::Lfs => self.refresh_lfs_data(cx),
            GitOpsTab::Gitflow => self.refresh_gitflow_data(cx),
            _ => {}
        }
        cx.notify();
    }

    fn handle_review_ai_commit_message(
        &mut self,
        _action: &ReviewAiCommitMessage,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let diff = match self.run_checked_git_command(cx, &["diff", "--cached"], "git diff --cached") {
            Ok(output) if output.status.success() => {
                String::from_utf8_lossy(&output.stdout).to_string()
            }
            _ => {
                self.push_notification(cx, NotificationType::Warning, "Git Ops", "Failed to get staged diff");
                return;
            }
        };

        if diff.trim().is_empty() {
            self.push_notification(cx, NotificationType::Warning, "Git Ops", "No staged changes to generate message for");
            return;
        }

        self.review_data.ai_commit.generating = true;
        cx.notify();

        // Truncate diff to ~32K chars
        let truncated_diff = if diff.len() > 32000 {
            format!("{}...\n[truncated — {} total chars]", &diff[..32000], diff.len())
        } else {
            diff
        };

        let system_prompt = "You are a git commit message generator. Given the following diff of staged changes, write a clear, concise commit message following conventional commit format (type(scope): description). Keep the first line under 72 characters. Add a body paragraph if the changes are complex. Only output the commit message, nothing else.".to_string();

        let messages = vec![hive_ai::types::ChatMessage {
            role: hive_ai::types::MessageRole::User,
            content: format!("Generate a commit message for this diff:\n\n{}", truncated_diff),
            timestamp: chrono::Utc::now(),
            tool_call_id: None,
            tool_calls: None,
        }];

        let model = self.status_bar.current_model.clone();

        let stream_setup = if cx.has_global::<AppAiService>() {
            cx.global::<AppAiService>().0.prepare_stream(
                messages, &model, Some(system_prompt), None,
            )
        } else {
            None
        };

        let Some((provider, request)) = stream_setup else {
            self.review_data.ai_commit.generating = false;
            self.push_notification(cx, NotificationType::Error, "Git Ops", "No AI provider available");
            cx.notify();
            return;
        };

        let task = cx.spawn(async move |this: WeakEntity<HiveWorkspace>, app: &mut AsyncApp| {
            let result = std::thread::spawn(move || {
                let rt = match tokio::runtime::Runtime::new() {
                    Ok(rt) => rt,
                    Err(e) => return Err(format!("Runtime error: {e}")),
                };
                rt.block_on(async {
                    match provider.stream_chat(&request).await {
                        Ok(mut rx) => {
                            let mut accumulated = String::new();
                            while let Some(chunk) = rx.recv().await {
                                accumulated.push_str(&chunk.content);
                            }
                            Ok(accumulated)
                        }
                        Err(e) => Err(format!("AI error: {e}")),
                    }
                })
            }).join().unwrap_or(Err("Thread panicked".to_string()));

            let _ = this.update(app, |workspace, cx| {
                workspace.review_data.ai_commit.generating = false;
                match result {
                    Ok(msg) => {
                        let msg = msg.trim().to_string();
                        workspace.review_data.ai_commit.generated_message = Some(msg.clone());
                        workspace.review_data.ai_commit.user_edited_message = msg;
                        workspace.push_notification(cx, NotificationType::Success, "Git Ops", "Commit message generated");
                    }
                    Err(e) => {
                        workspace.push_notification(cx, NotificationType::Error, "Git Ops", format!("AI generation failed: {e}"));
                    }
                }
                cx.notify();
            });
        });
        self._stream_task = Some(task);
    }

    fn handle_review_set_commit_message(
        &mut self,
        action: &ReviewSetCommitMessage,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.review_data.ai_commit.user_edited_message = action.message.clone();
        cx.notify();
    }

    fn handle_review_commit_with_message(
        &mut self,
        _action: &ReviewCommitWithMessage,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let message = self.review_data.ai_commit.user_edited_message.clone();
        if message.trim().is_empty() {
            self.push_notification(cx, NotificationType::Warning, "Git Ops", "Commit message is empty");
            return;
        }

        match self.run_checked_git_command(cx, &["commit", "-m", &message], "git commit") {
            Ok(output) if output.status.success() => {
                let commit_hash = self
                    .run_checked_git_command(cx, &["rev-parse", "--short", "HEAD"], "git rev-parse")
                    .ok()
                    .and_then(|o| {
                        if o.status.success() {
                            Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
                        } else {
                            None
                        }
                    })
                    .unwrap_or_else(|| "unknown".to_string());

                self.review_data = ReviewData::from_git(&self.current_project_root);
                self.review_data.ai_commit = AiCommitState::default();
                self.push_notification(cx, NotificationType::Success, "Git Ops", format!("Commit {commit_hash} created"));
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                self.push_notification(cx, NotificationType::Error, "Git Ops", format!("Commit failed: {stderr}"));
            }
            Err(e) => {
                self.push_notification(cx, NotificationType::Error, "Git Ops", e);
            }
        }
        cx.notify();
    }

    fn handle_review_push(
        &mut self,
        _action: &ReviewPush,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.review_data.push_data.push_in_progress = true;
        cx.notify();

        let work_dir = self.current_project_root.clone();
        let task = cx.spawn(async move |this: WeakEntity<HiveWorkspace>, app: &mut AsyncApp| {
            let result = std::thread::spawn(move || {
                let output = std::process::Command::new("git")
                    .args(["push"])
                    .current_dir(&work_dir)
                    .output();
                match output {
                    Ok(o) if o.status.success() => {
                        let stdout = String::from_utf8_lossy(&o.stdout).to_string();
                        let stderr = String::from_utf8_lossy(&o.stderr).to_string();
                        Ok(format!("{stdout}{stderr}").trim().to_string())
                    }
                    Ok(o) => Err(String::from_utf8_lossy(&o.stderr).to_string()),
                    Err(e) => Err(format!("Failed to push: {e}")),
                }
            }).join().unwrap_or(Err("Thread panicked".to_string()));

            let _ = this.update(app, |workspace, cx| {
                workspace.review_data.push_data.push_in_progress = false;
                match &result {
                    Ok(msg) => {
                        workspace.push_notification(cx, NotificationType::Success, "Git Ops", format!("Push successful: {msg}"));
                    }
                    Err(e) => {
                        workspace.push_notification(cx, NotificationType::Error, "Git Ops", format!("Push failed: {e}"));
                    }
                }
                workspace.review_data.push_data.last_push_result = Some(result);
                workspace.refresh_push_data(cx);
                cx.notify();
            });
        });
        self._stream_task = Some(task);
    }

    fn handle_review_push_set_upstream(
        &mut self,
        _action: &ReviewPushSetUpstream,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.review_data.push_data.push_in_progress = true;
        cx.notify();

        let work_dir = self.current_project_root.clone();
        let branch = self.review_data.branch.clone();
        let task = cx.spawn(async move |this: WeakEntity<HiveWorkspace>, app: &mut AsyncApp| {
            let result = std::thread::spawn(move || {
                let output = std::process::Command::new("git")
                    .args(["push", "--set-upstream", "origin", &branch])
                    .current_dir(&work_dir)
                    .output();
                match output {
                    Ok(o) if o.status.success() => {
                        let combined = format!(
                            "{}{}",
                            String::from_utf8_lossy(&o.stdout),
                            String::from_utf8_lossy(&o.stderr)
                        );
                        Ok(combined.trim().to_string())
                    }
                    Ok(o) => Err(String::from_utf8_lossy(&o.stderr).to_string()),
                    Err(e) => Err(format!("Failed to push: {e}")),
                }
            }).join().unwrap_or(Err("Thread panicked".to_string()));

            let _ = this.update(app, |workspace, cx| {
                workspace.review_data.push_data.push_in_progress = false;
                match &result {
                    Ok(msg) => workspace.push_notification(cx, NotificationType::Success, "Git Ops", format!("Push successful: {msg}")),
                    Err(e) => workspace.push_notification(cx, NotificationType::Error, "Git Ops", format!("Push failed: {e}")),
                }
                workspace.review_data.push_data.last_push_result = Some(result);
                workspace.refresh_push_data(cx);
                cx.notify();
            });
        });
        self._stream_task = Some(task);
    }

    // -- Branch operations -------------------------------------------------------

    fn handle_review_branch_refresh(
        &mut self,
        _action: &ReviewBranchRefresh,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.refresh_branches_data(cx);
        cx.notify();
    }

    fn handle_review_branch_create(
        &mut self,
        _action: &ReviewBranchCreate,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let name = self.review_data.branches_data.new_branch_name.clone();
        if name.trim().is_empty() {
            self.push_notification(cx, NotificationType::Warning, "Git Ops", "Branch name is empty");
            return;
        }
        match self.run_checked_git_command(cx, &["checkout", "-b", &name], "git checkout -b") {
            Ok(output) if output.status.success() => {
                self.push_notification(cx, NotificationType::Success, "Git Ops", format!("Created and switched to branch: {name}"));
                self.review_data.branches_data.new_branch_name.clear();
                self.review_data = ReviewData::from_git(&self.current_project_root);
                self.refresh_branches_data(cx);
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                self.push_notification(cx, NotificationType::Error, "Git Ops", format!("Failed: {stderr}"));
            }
            Err(e) => self.push_notification(cx, NotificationType::Error, "Git Ops", e),
        }
        cx.notify();
    }

    fn handle_review_branch_switch(
        &mut self,
        action: &ReviewBranchSwitch,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let name = &action.branch_name;
        match self.run_checked_git_command(cx, &["checkout", name], "git checkout") {
            Ok(output) if output.status.success() => {
                self.push_notification(cx, NotificationType::Success, "Git Ops", format!("Switched to branch: {name}"));
                self.review_data = ReviewData::from_git(&self.current_project_root);
                self.refresh_branches_data(cx);
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                self.push_notification(cx, NotificationType::Error, "Git Ops", format!("Failed: {stderr}"));
            }
            Err(e) => self.push_notification(cx, NotificationType::Error, "Git Ops", e),
        }
        cx.notify();
    }

    fn handle_review_branch_delete_named(
        &mut self,
        action: &ReviewBranchDeleteNamed,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let name = &action.branch_name;
        match self.run_checked_git_command(cx, &["branch", "-d", name], "git branch -d") {
            Ok(output) if output.status.success() => {
                self.push_notification(cx, NotificationType::Success, "Git Ops", format!("Deleted branch: {name}"));
                self.refresh_branches_data(cx);
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                self.push_notification(cx, NotificationType::Error, "Git Ops", format!("Failed: {stderr}"));
            }
            Err(e) => self.push_notification(cx, NotificationType::Error, "Git Ops", e),
        }
        cx.notify();
    }

    fn handle_review_branch_set_name(
        &mut self,
        action: &ReviewBranchSetName,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.review_data.branches_data.new_branch_name = action.name.clone();
        cx.notify();
    }

    // -- PR operations -----------------------------------------------------------

    fn handle_review_pr_refresh(
        &mut self,
        _action: &ReviewPrRefresh,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.refresh_pr_data(cx);
    }

    fn handle_review_pr_ai_generate(
        &mut self,
        _action: &ReviewPrAiGenerate,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let base = self.review_data.pr_data.pr_form.base_branch.clone();

        // Get commits
        let commits = match self.run_checked_git_command(cx, &["log", &format!("{base}..HEAD"), "--oneline"], "git log") {
            Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
            _ => String::new(),
        };

        // Get diff
        let diff = match self.run_checked_git_command(cx, &["diff", &format!("{base}...HEAD")], "git diff") {
            Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
            _ => String::new(),
        };

        if commits.trim().is_empty() && diff.trim().is_empty() {
            self.push_notification(cx, NotificationType::Warning, "Git Ops", "No changes found between HEAD and base branch");
            return;
        }

        self.review_data.pr_data.pr_form.ai_generating = true;
        cx.notify();

        let truncated_diff = if diff.len() > 32000 {
            format!("{}...\n[truncated]", &diff[..32000])
        } else {
            diff
        };

        let system_prompt = "You are a pull request description generator. Given commits and a diff, generate a PR title and markdown body.\n\nOutput format:\nTITLE: <title under 72 chars>\nBODY:\n## Summary\n<2-3 bullets>\n\n## Changes\n<list of key changes>\n\n## Testing\n<how to test>\n\nOnly output in this format, nothing else.".to_string();

        let messages = vec![hive_ai::types::ChatMessage {
            role: hive_ai::types::MessageRole::User,
            content: format!("Commits:\n{}\n\nDiff:\n{}", commits, truncated_diff),
            timestamp: chrono::Utc::now(),
            tool_call_id: None,
            tool_calls: None,
        }];

        let model = self.status_bar.current_model.clone();
        let stream_setup = if cx.has_global::<AppAiService>() {
            cx.global::<AppAiService>().0.prepare_stream(messages, &model, Some(system_prompt), None)
        } else {
            None
        };

        let Some((provider, request)) = stream_setup else {
            self.review_data.pr_data.pr_form.ai_generating = false;
            self.push_notification(cx, NotificationType::Error, "Git Ops", "No AI provider available");
            cx.notify();
            return;
        };

        let task = cx.spawn(async move |this: WeakEntity<HiveWorkspace>, app: &mut AsyncApp| {
            let result = std::thread::spawn(move || {
                let rt = match tokio::runtime::Runtime::new() {
                    Ok(rt) => rt,
                    Err(e) => return Err(format!("Runtime error: {e}")),
                };
                rt.block_on(async {
                    match provider.stream_chat(&request).await {
                        Ok(mut rx) => {
                            let mut accumulated = String::new();
                            while let Some(chunk) = rx.recv().await {
                                accumulated.push_str(&chunk.content);
                            }
                            Ok(accumulated)
                        }
                        Err(e) => Err(format!("AI error: {e}")),
                    }
                })
            }).join().unwrap_or(Err("Thread panicked".to_string()));

            let _ = this.update(app, |workspace, cx| {
                workspace.review_data.pr_data.pr_form.ai_generating = false;
                match result {
                    Ok(text) => {
                        // Parse TITLE: and BODY: sections
                        let text = text.trim();
                        if let Some(title_start) = text.find("TITLE:") {
                            let after_title = &text[title_start + 6..];
                            if let Some(body_start) = after_title.find("BODY:") {
                                let title = after_title[..body_start].trim().to_string();
                                let body = after_title[body_start + 5..].trim().to_string();
                                workspace.review_data.pr_data.pr_form.title = title;
                                workspace.review_data.pr_data.pr_form.body = body;
                            } else {
                                workspace.review_data.pr_data.pr_form.title = after_title.lines().next().unwrap_or("").trim().to_string();
                            }
                        } else {
                            // Fallback: use first line as title, rest as body
                            let lines: Vec<&str> = text.lines().collect();
                            workspace.review_data.pr_data.pr_form.title = lines.first().unwrap_or(&"").to_string();
                            workspace.review_data.pr_data.pr_form.body = lines[1..].join("\n").trim().to_string();
                        }
                        workspace.push_notification(cx, NotificationType::Success, "Git Ops", "PR description generated");
                    }
                    Err(e) => {
                        workspace.push_notification(cx, NotificationType::Error, "Git Ops", format!("AI generation failed: {e}"));
                    }
                }
                cx.notify();
            });
        });
        self._stream_task = Some(task);
    }

    fn handle_review_pr_create(
        &mut self,
        _action: &ReviewPrCreate,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let token = match self.get_github_token(cx) {
            Some(t) => t,
            None => {
                self.push_notification(cx, NotificationType::Error, "Git Ops", "GitHub not connected. Connect via Settings.");
                return;
            }
        };

        let (owner, repo) = match self.parse_github_remote(cx) {
            Some(pair) => pair,
            None => {
                self.push_notification(cx, NotificationType::Error, "Git Ops", "Could not parse GitHub owner/repo from remote");
                return;
            }
        };

        let title = self.review_data.pr_data.pr_form.title.clone();
        let body = self.review_data.pr_data.pr_form.body.clone();
        let head = self.review_data.branch.clone();
        let base = self.review_data.pr_data.pr_form.base_branch.clone();

        if title.trim().is_empty() {
            self.push_notification(cx, NotificationType::Warning, "Git Ops", "PR title is empty");
            return;
        }

        self.review_data.pr_data.loading = true;
        cx.notify();

        let task = cx.spawn(async move |this: WeakEntity<HiveWorkspace>, app: &mut AsyncApp| {
            let result = std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().map_err(|e| format!("Runtime error: {e}"))?;
                rt.block_on(async {
                    let client = hive_integrations::GitHubClient::new(&token)
                        .map_err(|e| format!("GitHub client error: {e}"))?;
                    client.create_pull(&owner, &repo, &title, &body, &head, &base)
                        .await
                        .map_err(|e| format!("GitHub API error: {e}"))
                })
            }).join().unwrap_or(Err("Thread panicked".into()));

            let _ = this.update(app, |workspace, cx| {
                workspace.review_data.pr_data.loading = false;
                match result {
                    Ok(value) => {
                        let url = value.get("html_url").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let number = value.get("number").and_then(|v| v.as_u64()).unwrap_or(0);
                        workspace.push_notification(cx, NotificationType::Success, "Git Ops", format!("PR #{number} created: {url}"));
                        workspace.review_data.pr_data.pr_form = PrForm::default();
                    }
                    Err(e) => {
                        workspace.push_notification(cx, NotificationType::Error, "Git Ops", e);
                    }
                }
                cx.notify();
            });
        });
        self._stream_task = Some(task);
    }

    fn handle_review_pr_set_title(
        &mut self,
        action: &ReviewPrSetTitle,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.review_data.pr_data.pr_form.title = action.title.clone();
        cx.notify();
    }

    fn handle_review_pr_set_body(
        &mut self,
        action: &ReviewPrSetBody,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.review_data.pr_data.pr_form.body = action.body.clone();
        cx.notify();
    }

    fn handle_review_pr_set_base(
        &mut self,
        action: &ReviewPrSetBase,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.review_data.pr_data.pr_form.base_branch = action.base.clone();
        cx.notify();
    }

    // -- LFS operations ----------------------------------------------------------

    fn handle_review_lfs_refresh(
        &mut self,
        _action: &ReviewLfsRefresh,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.refresh_lfs_data(cx);
        cx.notify();
    }

    fn handle_review_lfs_track(
        &mut self,
        _action: &ReviewLfsTrack,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let pattern = self.review_data.lfs_data.new_pattern.clone();
        if pattern.trim().is_empty() {
            self.push_notification(cx, NotificationType::Warning, "Git Ops", "LFS pattern is empty");
            return;
        }
        match self.run_checked_git_command(cx, &["lfs", "track", &pattern], "git lfs track") {
            Ok(output) if output.status.success() => {
                let _ = self.run_checked_git_command(cx, &["add", ".gitattributes"], "git add .gitattributes");
                self.push_notification(cx, NotificationType::Success, "Git Ops", format!("Now tracking: {pattern}"));
                self.review_data.lfs_data.new_pattern.clear();
                self.refresh_lfs_data(cx);
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                self.push_notification(cx, NotificationType::Error, "Git Ops", format!("LFS track failed: {stderr}"));
            }
            Err(e) => self.push_notification(cx, NotificationType::Error, "Git Ops", e),
        }
        cx.notify();
    }

    fn handle_review_lfs_untrack(
        &mut self,
        _action: &ReviewLfsUntrack,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let pattern = self.review_data.lfs_data.new_pattern.clone();
        if pattern.trim().is_empty() {
            self.push_notification(cx, NotificationType::Warning, "Git Ops", "LFS pattern is empty");
            return;
        }
        match self.run_checked_git_command(cx, &["lfs", "untrack", &pattern], "git lfs untrack") {
            Ok(output) if output.status.success() => {
                self.push_notification(cx, NotificationType::Success, "Git Ops", format!("Untracked: {pattern}"));
                self.review_data.lfs_data.new_pattern.clear();
                self.refresh_lfs_data(cx);
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                self.push_notification(cx, NotificationType::Error, "Git Ops", format!("LFS untrack failed: {stderr}"));
            }
            Err(e) => self.push_notification(cx, NotificationType::Error, "Git Ops", e),
        }
        cx.notify();
    }

    fn handle_review_lfs_set_pattern(
        &mut self,
        action: &ReviewLfsSetPattern,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.review_data.lfs_data.new_pattern = action.pattern.clone();
        cx.notify();
    }

    fn handle_review_lfs_pull(
        &mut self,
        _action: &ReviewLfsPull,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.review_data.lfs_data.lfs_pull_in_progress = true;
        cx.notify();

        let work_dir = self.current_project_root.clone();
        let task = cx.spawn(async move |this: WeakEntity<HiveWorkspace>, app: &mut AsyncApp| {
            let result = std::thread::spawn(move || {
                let output = std::process::Command::new("git")
                    .args(["lfs", "pull"])
                    .current_dir(&work_dir)
                    .output();
                match output {
                    Ok(o) if o.status.success() => Ok(String::from_utf8_lossy(&o.stdout).to_string()),
                    Ok(o) => Err(String::from_utf8_lossy(&o.stderr).to_string()),
                    Err(e) => Err(format!("LFS pull failed: {e}")),
                }
            }).join().unwrap_or(Err("Thread panicked".to_string()));

            let _ = this.update(app, |workspace, cx| {
                workspace.review_data.lfs_data.lfs_pull_in_progress = false;
                match result {
                    Ok(_) => workspace.push_notification(cx, NotificationType::Success, "Git Ops", "LFS pull complete"),
                    Err(e) => workspace.push_notification(cx, NotificationType::Error, "Git Ops", format!("LFS pull failed: {e}")),
                }
                workspace.refresh_lfs_data(cx);
                cx.notify();
            });
        });
        self._stream_task = Some(task);
    }

    fn handle_review_lfs_push(
        &mut self,
        _action: &ReviewLfsPush,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.review_data.lfs_data.lfs_push_in_progress = true;
        cx.notify();

        let work_dir = self.current_project_root.clone();
        let branch = self.review_data.branch.clone();
        let task = cx.spawn(async move |this: WeakEntity<HiveWorkspace>, app: &mut AsyncApp| {
            let result = std::thread::spawn(move || {
                let output = std::process::Command::new("git")
                    .args(["lfs", "push", "origin", &branch])
                    .current_dir(&work_dir)
                    .output();
                match output {
                    Ok(o) if o.status.success() => Ok(String::from_utf8_lossy(&o.stdout).to_string()),
                    Ok(o) => Err(String::from_utf8_lossy(&o.stderr).to_string()),
                    Err(e) => Err(format!("LFS push failed: {e}")),
                }
            }).join().unwrap_or(Err("Thread panicked".to_string()));

            let _ = this.update(app, |workspace, cx| {
                workspace.review_data.lfs_data.lfs_push_in_progress = false;
                match result {
                    Ok(_) => workspace.push_notification(cx, NotificationType::Success, "Git Ops", "LFS push complete"),
                    Err(e) => workspace.push_notification(cx, NotificationType::Error, "Git Ops", format!("LFS push failed: {e}")),
                }
                workspace.refresh_lfs_data(cx);
                cx.notify();
            });
        });
        self._stream_task = Some(task);
    }

    // -- Gitflow operations ------------------------------------------------------

    fn handle_review_gitflow_init(
        &mut self,
        _action: &ReviewGitflowInit,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let commands: [(&str, &[&str]); 5] = [
            ("config", &["config", "gitflow.branch.master", "main"]),
            ("config", &["config", "gitflow.branch.develop", "develop"]),
            ("config", &["config", "gitflow.prefix.feature", "feature/"]),
            ("config", &["config", "gitflow.prefix.release", "release/"]),
            ("config", &["config", "gitflow.prefix.hotfix", "hotfix/"]),
        ];

        for (label, args) in &commands {
            if let Err(e) = self.run_checked_git_command(cx, args, &format!("git {label}")) {
                self.push_notification(cx, NotificationType::Error, "Git Ops", format!("Gitflow init failed: {e}"));
                cx.notify();
                return;
            }
        }

        // Create develop branch if it doesn't exist
        let branch_exists = self.run_checked_git_command(cx, &["rev-parse", "--verify", "develop"], "git rev-parse")
            .map(|o| o.status.success())
            .unwrap_or(false);

        if !branch_exists {
            let _ = self.run_checked_git_command(cx, &["branch", "develop"], "git branch develop");
        }

        self.push_notification(cx, NotificationType::Success, "Git Ops", "Gitflow initialized");
        self.refresh_gitflow_data(cx);
        cx.notify();
    }

    fn handle_review_gitflow_start(
        &mut self,
        action: &ReviewGitflowStart,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let name = &action.name;
        if name.trim().is_empty() {
            self.push_notification(cx, NotificationType::Warning, "Git Ops", "Name is empty");
            return;
        }

        let gf = &self.review_data.gitflow_data;
        let (branch_name, base) = match action.kind.as_str() {
            "feature" => (format!("{}{}", gf.feature_prefix, name), gf.develop_branch.clone()),
            "release" => (format!("{}{}", gf.release_prefix, name), gf.develop_branch.clone()),
            "hotfix" => (format!("{}{}", gf.hotfix_prefix, name), gf.main_branch.clone()),
            _ => {
                self.push_notification(cx, NotificationType::Error, "Git Ops", format!("Unknown gitflow kind: {}", action.kind));
                return;
            }
        };

        match self.run_checked_git_command(cx, &["checkout", "-b", &branch_name, &base], "git checkout -b") {
            Ok(output) if output.status.success() => {
                self.push_notification(cx, NotificationType::Success, "Git Ops", format!("Started {} {}", action.kind, name));
                self.review_data.gitflow_data.new_name.clear();
                self.review_data = ReviewData::from_git(&self.current_project_root);
                self.refresh_gitflow_data(cx);
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                self.push_notification(cx, NotificationType::Error, "Git Ops", format!("Failed: {stderr}"));
            }
            Err(e) => self.push_notification(cx, NotificationType::Error, "Git Ops", e),
        }
        cx.notify();
    }

    fn handle_review_gitflow_finish_named(
        &mut self,
        action: &ReviewGitflowFinishNamed,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let gf = &self.review_data.gitflow_data;
        let branch_name = match action.kind.as_str() {
            "feature" => format!("{}{}", gf.feature_prefix, action.name),
            "release" => format!("{}{}", gf.release_prefix, action.name),
            "hotfix" => format!("{}{}", gf.hotfix_prefix, action.name),
            _ => return,
        };

        let main = gf.main_branch.clone();
        let develop = gf.develop_branch.clone();

        // Helper to run and check
        let run = |this: &mut Self, cx: &mut Context<Self>, args: &[&str]| -> bool {
            match this.run_checked_git_command(cx, args, &format!("git {}", args.join(" "))) {
                Ok(o) if o.status.success() => true,
                Ok(o) => {
                    let stderr = String::from_utf8_lossy(&o.stderr);
                    this.push_notification(cx, NotificationType::Error, "Git Ops", format!("Failed: {stderr}"));
                    false
                }
                Err(e) => {
                    this.push_notification(cx, NotificationType::Error, "Git Ops", e);
                    false
                }
            }
        };

        match action.kind.as_str() {
            "feature" => {
                if !run(self, cx, &["checkout", &develop]) { return; }
                if !run(self, cx, &["merge", "--no-ff", &branch_name]) { return; }
                run(self, cx, &["branch", "-d", &branch_name]);
            }
            "release" => {
                if !run(self, cx, &["checkout", &main]) { return; }
                if !run(self, cx, &["merge", "--no-ff", &branch_name]) { return; }
                run(self, cx, &["tag", "-a", &action.name, "-m", &format!("Release {}", action.name)]);
                if !run(self, cx, &["checkout", &develop]) { return; }
                if !run(self, cx, &["merge", "--no-ff", &branch_name]) { return; }
                run(self, cx, &["branch", "-d", &branch_name]);
            }
            "hotfix" => {
                if !run(self, cx, &["checkout", &main]) { return; }
                if !run(self, cx, &["merge", "--no-ff", &branch_name]) { return; }
                run(self, cx, &["tag", "-a", &action.name, "-m", &format!("Hotfix {}", action.name)]);
                if !run(self, cx, &["checkout", &develop]) { return; }
                if !run(self, cx, &["merge", "--no-ff", &branch_name]) { return; }
                run(self, cx, &["branch", "-d", &branch_name]);
            }
            _ => return,
        }

        self.push_notification(cx, NotificationType::Success, "Git Ops", format!("Finished {} {}", action.kind, action.name));
        self.review_data = ReviewData::from_git(&self.current_project_root);
        self.refresh_gitflow_data(cx);
        cx.notify();
    }

    fn handle_review_gitflow_set_name(
        &mut self,
        action: &ReviewGitflowSetName,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.review_data.gitflow_data.new_name = action.name.clone();
        cx.notify();
    }

    // -- Git Ops helper/refresh methods ------------------------------------------

    fn get_github_token(&self, cx: &Context<Self>) -> Option<String> {
        use hive_core::config::AccountPlatform;
        if !cx.has_global::<AppConfig>() { return None; }
        cx.global::<AppConfig>().0.get_oauth_token(AccountPlatform::GitHub)
            .map(|t| t.access_token.clone())
    }

    fn parse_github_remote(&self, cx: &Context<Self>) -> Option<(String, String)> {
        let output = self.run_checked_git_command(cx, &["remote", "get-url", "origin"], "git remote get-url origin").ok()?;
        if !output.status.success() { return None; }
        let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
        parse_github_owner_repo(&url)
    }

    fn refresh_push_data(&mut self, cx: &Context<Self>) {
        // Remote URL
        if let Ok(output) = self.run_checked_git_command(cx, &["remote", "get-url", "origin"], "git remote get-url")
            && output.status.success()
        {
            self.review_data.push_data.remote_url = String::from_utf8_lossy(&output.stdout).trim().to_string();
        }

        // Tracking branch
        if let Ok(output) = self.run_checked_git_command(cx, &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"], "git rev-parse") {
            if output.status.success() {
                self.review_data.push_data.tracking_branch = Some(String::from_utf8_lossy(&output.stdout).trim().to_string());
            } else {
                self.review_data.push_data.tracking_branch = None;
            }
        }

        // Ahead/behind
        if self.review_data.push_data.tracking_branch.is_some() {
            if let Ok(output) = self.run_checked_git_command(cx, &["rev-list", "--count", "@{u}..HEAD"], "git rev-list")
                && output.status.success()
            {
                self.review_data.push_data.ahead_count = String::from_utf8_lossy(&output.stdout).trim().parse().unwrap_or(0);
            }
            if let Ok(output) = self.run_checked_git_command(cx, &["rev-list", "--count", "HEAD..@{u}"], "git rev-list")
                && output.status.success()
            {
                self.review_data.push_data.behind_count = String::from_utf8_lossy(&output.stdout).trim().parse().unwrap_or(0);
            }
        }
    }

    fn refresh_branches_data(&mut self, cx: &Context<Self>) {
        let mut branches = Vec::new();

        // Get current branch
        let current = match self.run_checked_git_command(cx, &["rev-parse", "--abbrev-ref", "HEAD"], "git rev-parse") {
            Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).trim().to_string(),
            _ => String::new(),
        };
        self.review_data.branches_data.current_branch = current.clone();

        // List all branches
        if let Ok(output) = self.run_checked_git_command(cx, &["branch", "-a", "--format=%(refname:short)\t%(objectname:short)\t%(subject)"], "git branch -a")
            && output.status.success()
        {
            let text = String::from_utf8_lossy(&output.stdout);
            for line in text.lines() {
                let parts: Vec<&str> = line.splitn(3, '\t').collect();
                if parts.is_empty() { continue; }
                let name = parts[0].to_string();
                let is_remote = name.starts_with("origin/");
                // Skip HEAD pointers
                if name.contains("HEAD") { continue; }
                let commit_msg = parts.get(2).unwrap_or(&"").to_string();
                branches.push(BranchEntry {
                    is_current: name == current,
                    is_remote,
                    last_commit_msg: commit_msg,
                    last_commit_time: String::new(),
                    name,
                });
            }
        }

        self.review_data.branches_data.branches = branches;
    }

    fn refresh_lfs_data(&mut self, cx: &Context<Self>) {
        // Check if LFS is installed
        let lfs_installed = self.run_checked_git_command(cx, &["lfs", "version"], "git lfs version")
            .map(|o| o.status.success())
            .unwrap_or(false);
        self.review_data.lfs_data.is_lfs_installed = lfs_installed;

        if !lfs_installed { return; }

        // Read tracked patterns from .gitattributes
        let gitattributes_path = self.current_project_root.join(".gitattributes");
        let mut patterns = Vec::new();
        if let Ok(content) = std::fs::read_to_string(&gitattributes_path) {
            for line in content.lines() {
                if line.contains("filter=lfs")
                    && let Some(pattern) = line.split_whitespace().next()
                {
                    patterns.push(pattern.to_string());
                }
            }
        }
        self.review_data.lfs_data.tracked_patterns = patterns;

        // List LFS files
        let mut lfs_files = Vec::new();
        if let Ok(output) = self.run_checked_git_command(cx, &["lfs", "ls-files", "--long"], "git lfs ls-files")
            && output.status.success()
        {
            let text = String::from_utf8_lossy(&output.stdout);
            for line in text.lines() {
                // Format: <oid> <*|-> <path>
                let parts: Vec<&str> = line.splitn(3, ' ').collect();
                if parts.len() >= 3 {
                    lfs_files.push(LfsFileEntry {
                        oid: parts[0].to_string(),
                        is_pointer: parts[1] == "-",
                        path: parts[2].to_string(),
                        size: String::new(),
                    });
                }
            }
        }
        self.review_data.lfs_data.lfs_files = lfs_files;
    }

    fn refresh_gitflow_data(&mut self, cx: &Context<Self>) {
        // Read gitflow config
        let read_config = |this: &Self, cx: &Context<Self>, key: &str| -> Option<String> {
            this.run_checked_git_command(cx, &["config", key], &format!("git config {key}"))
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        };

        if let Some(main) = read_config(self, cx, "gitflow.branch.master") {
            self.review_data.gitflow_data.main_branch = main;
            self.review_data.gitflow_data.initialized = true;
        } else {
            self.review_data.gitflow_data.initialized = false;
            return;
        }

        if let Some(develop) = read_config(self, cx, "gitflow.branch.develop") {
            self.review_data.gitflow_data.develop_branch = develop;
        }
        if let Some(fp) = read_config(self, cx, "gitflow.prefix.feature") {
            self.review_data.gitflow_data.feature_prefix = fp;
        }
        if let Some(rp) = read_config(self, cx, "gitflow.prefix.release") {
            self.review_data.gitflow_data.release_prefix = rp;
        }
        if let Some(hp) = read_config(self, cx, "gitflow.prefix.hotfix") {
            self.review_data.gitflow_data.hotfix_prefix = hp;
        }

        // List active feature/release/hotfix branches
        let list_active = |this: &Self, cx: &Context<Self>, prefix: &str| -> Vec<String> {
            let mut active = Vec::new();
            if let Ok(output) = this.run_checked_git_command(cx, &["branch", "--list", &format!("{prefix}*")], "git branch --list")
                && output.status.success()
            {
                let text = String::from_utf8_lossy(&output.stdout);
                for line in text.lines() {
                    let name = line.trim().trim_start_matches("* ").trim_start_matches(prefix);
                    if !name.is_empty() {
                        active.push(name.to_string());
                    }
                }
            }
            active
        };

        let fp = self.review_data.gitflow_data.feature_prefix.clone();
        let rp = self.review_data.gitflow_data.release_prefix.clone();
        let hp = self.review_data.gitflow_data.hotfix_prefix.clone();
        self.review_data.gitflow_data.active_features = list_active(self, cx, &fp);
        self.review_data.gitflow_data.active_releases = list_active(self, cx, &rp);
        self.review_data.gitflow_data.active_hotfixes = list_active(self, cx, &hp);
    }

    fn refresh_pr_data(&mut self, cx: &mut Context<Self>) {
        // Mark loading state immediately so the UI shows a spinner.
        self.review_data.pr_data.loading = true;
        cx.notify();

        // Validate that this is a git repo by reading the current branch.
        // If this fails, there is no point trying to fetch PR data.
        let _current_branch = match hive_fs::git::GitService::open(&self.current_project_root)
            .and_then(|gs| gs.current_branch())
        {
            Ok(b) => b,
            Err(e) => {
                warn!("refresh_pr_data: cannot read current branch: {e}");
                self.review_data.pr_data.loading = false;
                cx.notify();
                return;
            }
        };

        // Try the GitHub API path first if we have both a token and a
        // parseable GitHub remote.
        let github_token = self.get_github_token(cx);
        let github_remote = self.parse_github_remote(cx);

        if let (Some(token), Some((owner, repo))) = (github_token, github_remote) {
            self.review_data.pr_data.github_connected = true;

            let task = cx.spawn(async move |this: WeakEntity<HiveWorkspace>, app: &mut AsyncApp| {
                let result = std::thread::spawn(move || {
                    let rt = tokio::runtime::Runtime::new()
                        .map_err(|e| format!("Runtime error: {e}"))?;
                    rt.block_on(async {
                        let client = hive_integrations::GitHubClient::new(&token)
                            .map_err(|e| format!("GitHub client error: {e}"))?;
                        let pulls = client
                            .list_pulls(&owner, &repo)
                            .await
                            .map_err(|e| format!("GitHub API error: {e}"))?;

                        // Parse JSON array into PrSummary entries.
                        let summaries: Vec<PrSummary> = pulls
                            .as_array()
                            .unwrap_or(&Vec::new())
                            .iter()
                            .filter_map(|pr| {
                                Some(PrSummary {
                                    number: pr.get("number")?.as_u64()?,
                                    title: pr.get("title")?.as_str()?.to_string(),
                                    author: pr
                                        .get("user")
                                        .and_then(|u| u.get("login"))
                                        .and_then(|l| l.as_str())
                                        .unwrap_or("unknown")
                                        .to_string(),
                                    head: pr
                                        .get("head")
                                        .and_then(|h| h.get("ref"))
                                        .and_then(|r| r.as_str())
                                        .unwrap_or("")
                                        .to_string(),
                                    base: pr
                                        .get("base")
                                        .and_then(|b| b.get("ref"))
                                        .and_then(|r| r.as_str())
                                        .unwrap_or("")
                                        .to_string(),
                                    state: pr
                                        .get("state")
                                        .and_then(|s| s.as_str())
                                        .unwrap_or("open")
                                        .to_string(),
                                    created_at: pr
                                        .get("created_at")
                                        .and_then(|c| c.as_str())
                                        .unwrap_or("")
                                        .to_string(),
                                    url: pr
                                        .get("html_url")
                                        .and_then(|u| u.as_str())
                                        .unwrap_or("")
                                        .to_string(),
                                })
                            })
                            .collect();
                        Ok(summaries)
                    })
                })
                .join()
                .unwrap_or(Err("Thread panicked".to_string()));

                let _ = this.update(app, |workspace, cx| {
                    workspace.review_data.pr_data.loading = false;
                    match result {
                        Ok(prs) => {
                            info!("PR refresh: fetched {} open PRs from GitHub", prs.len());
                            workspace.review_data.pr_data.open_prs = prs;
                        }
                        Err(e) => {
                            warn!("PR refresh GitHub fetch failed, falling back to git log: {e}");
                            // Fallback: populate from git log.
                            workspace.populate_pr_data_from_git_log();
                        }
                    }
                    cx.notify();
                });
            });
            self._stream_task = Some(task);
        } else {
            // No GitHub integration — populate from git log data directly.
            self.review_data.pr_data.github_connected = false;
            self.populate_pr_data_from_git_log();
            self.review_data.pr_data.loading = false;
            cx.notify();
        }
    }

    /// Fallback PR population from local git log when GitHub API is unavailable.
    ///
    /// Reads recent commits on the current branch that are ahead of `main` (or
    /// the configured base branch) and synthesises pseudo-PR entries so the
    /// Pull Requests tab still shows useful information.
    fn populate_pr_data_from_git_log(&mut self) {
        let base = &self.review_data.pr_data.pr_form.base_branch;
        let git = match hive_fs::git::GitService::open(&self.current_project_root) {
            Ok(g) => g,
            Err(_) => return,
        };

        let branch = match git.current_branch() {
            Ok(b) => b,
            Err(_) => return,
        };

        // If we are on the base branch itself there is nothing to show.
        if branch == *base {
            self.review_data.pr_data.open_prs.clear();
            return;
        }

        // Use git log to get recent commits (simulate a single "draft PR").
        let commits = match git.log(20) {
            Ok(c) => c,
            Err(_) => return,
        };

        if commits.is_empty() {
            return;
        }

        // Build a single pseudo-PR from the branch's commits.
        let first_msg = commits
            .first()
            .map(|c| c.message.clone())
            .unwrap_or_default();
        let author = commits
            .first()
            .map(|c| c.author.clone())
            .unwrap_or_else(|| "local".to_string());

        let body_lines: Vec<String> = commits
            .iter()
            .take(10)
            .map(|c| format!("- {} ({})", c.message, &c.hash[..8.min(c.hash.len())]))
            .collect();

        // Pre-fill the PR form with useful defaults from the branch.
        self.review_data.pr_data.pr_form.title = first_msg.clone();
        self.review_data.pr_data.pr_form.body = body_lines.join("\n");

        let summary = PrSummary {
            number: 0,
            title: format!("[local] {}", first_msg),
            author,
            head: branch.clone(),
            base: base.clone(),
            state: "draft".to_string(),
            created_at: commits
                .first()
                .map(|c| {
                    chrono::DateTime::from_timestamp(c.timestamp, 0)
                        .map(|dt| dt.to_rfc3339())
                        .unwrap_or_default()
                })
                .unwrap_or_default(),
            url: String::new(),
        };

        self.review_data.pr_data.open_prs = vec![summary];
    }

    // -- Skills panel handlers -----------------------------------------------

    fn handle_skills_refresh(
        &mut self,
        _action: &SkillsRefresh,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        info!("Skills: refresh");
        self.refresh_skills_data(cx);
        cx.notify();
    }

    fn handle_skills_install(
        &mut self,
        action: &SkillsInstall,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        info!("ClawdHub: install skill {}", action.skill_id);

        // Find the skill in the directory catalog.
        let catalog = hive_agents::skill_marketplace::SkillMarketplace::default_directory();
        if let Some(available) = catalog.iter().find(|s| s.name == action.skill_id) {
            if cx.has_global::<AppMarketplace>() {
                let mp = &mut cx.global_mut::<AppMarketplace>().0;
                let prompt = format!(
                    "You are an expert assistant for: {}. {}",
                    available.name, available.description
                );
                if let Err(e) = mp.install_skill(
                    &available.name,
                    &available.trigger,
                    available.category,
                    &prompt,
                    Some(&available.repo_url),
                ) {
                    warn!("Failed to install skill {}: {e}", available.name);
                }
            }
        } else {
            warn!("Skill '{}' not found in catalog", action.skill_id);
        }

        self.refresh_skills_data(cx);
        cx.notify();
    }

    fn handle_skills_remove(
        &mut self,
        action: &SkillsRemove,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        info!("ClawdHub: remove skill {}", action.skill_id);

        if action.skill_id.starts_with("builtin:") {
            // Remove from built-in registry.
            let name = action.skill_id.strip_prefix("builtin:").unwrap_or(&action.skill_id);
            if cx.has_global::<hive_ui_core::AppSkills>() {
                cx.global_mut::<hive_ui_core::AppSkills>().0.uninstall(name);
            }
        } else {
            // Remove from marketplace.
            if cx.has_global::<AppMarketplace>() {
                let mp = &mut cx.global_mut::<AppMarketplace>().0;
                if let Err(e) = mp.remove_skill(&action.skill_id) {
                    warn!("Failed to remove skill {}: {e}", action.skill_id);
                }
            }
        }

        self.refresh_skills_data(cx);
        cx.notify();
    }

    fn handle_skills_toggle(
        &mut self,
        action: &SkillsToggle,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        info!("ClawdHub: toggle skill {}", action.skill_id);

        if action.skill_id.starts_with("builtin:") {
            let name = action.skill_id.strip_prefix("builtin:").unwrap_or(&action.skill_id);
            if cx.has_global::<hive_ui_core::AppSkills>() {
                cx.global_mut::<hive_ui_core::AppSkills>().0.toggle(name);
            }
        } else if cx.has_global::<AppMarketplace>() {
            let mp = &mut cx.global_mut::<AppMarketplace>().0;
            if let Err(e) = mp.toggle_skill(&action.skill_id) {
                warn!("Failed to toggle skill {}: {e}", action.skill_id);
            }
        }

        self.refresh_skills_data(cx);
        cx.notify();
    }

    fn handle_skills_create(
        &mut self,
        action: &SkillsCreate,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        info!("ClawdHub: create skill '{}'", action.name);

        // Install into the SkillsRegistry as a custom skill.
        if cx.has_global::<hive_ui_core::AppSkills>() {
            let registry = &mut cx.global_mut::<hive_ui_core::AppSkills>().0;
            if let Err(e) = registry.install(
                action.name.clone(),
                action.description.clone(),
                action.instructions.clone(),
                hive_agents::skills::SkillSource::Custom,
            ) {
                warn!("Failed to create skill '{}': {e}", action.name);
            }
        }

        // Reset the create form draft.
        self.skills_data.create_draft = hive_ui_panels::panels::skills::CreateSkillDraft::empty();

        self.refresh_skills_data(cx);
        cx.notify();
    }

    fn handle_skills_add_source(
        &mut self,
        action: &SkillsAddSource,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        info!("ClawdHub: add source '{}'", action.url);

        if !action.url.is_empty() && cx.has_global::<AppMarketplace>() {
            let mp = &mut cx.global_mut::<AppMarketplace>().0;
            if let Err(e) = mp.add_source(&action.url, &action.name) {
                warn!("Failed to add source '{}': {e}", action.url);
            }
        }

        self.refresh_skills_data(cx);
        cx.notify();
    }

    fn handle_skills_remove_source(
        &mut self,
        action: &SkillsRemoveSource,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        info!("ClawdHub: remove source '{}'", action.url);

        if cx.has_global::<AppMarketplace>() {
            let mp = &mut cx.global_mut::<AppMarketplace>().0;
            if let Err(e) = mp.remove_source(&action.url) {
                warn!("Failed to remove source '{}': {e}", action.url);
            }
        }

        self.refresh_skills_data(cx);
        cx.notify();
    }

    fn handle_skills_set_tab(
        &mut self,
        action: &SkillsSetTab,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use hive_ui_panels::panels::skills::SkillsTab;
        info!("ClawdHub: switch tab to '{}'", action.tab);

        self.skills_data.active_tab = match action.tab.as_str() {
            "Installed" => SkillsTab::Installed,
            "Directory" => SkillsTab::Directory,
            "Create" => SkillsTab::Create,
            "Add Source" => SkillsTab::AddSource,
            _ => SkillsTab::Installed,
        };
        cx.notify();
    }

    fn handle_skills_set_search(
        &mut self,
        action: &SkillsSetSearch,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.skills_data.search_query = action.query.clone();
        cx.notify();
    }

    fn handle_skills_clear_search(
        &mut self,
        _action: &SkillsClearSearch,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.skills_data.search_query.clear();
        cx.notify();
    }

    fn handle_skills_set_category(
        &mut self,
        action: &SkillsSetCategory,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use hive_ui_panels::panels::skills::SkillCategory;
        info!("ClawdHub: set category filter to '{}'", action.category);

        self.skills_data.selected_category = match action.category.as_str() {
            "All" => None,
            "Code Quality" => Some(SkillCategory::CodeQuality),
            "Testing" => Some(SkillCategory::Testing),
            "DevOps" => Some(SkillCategory::DevOps),
            "Security" => Some(SkillCategory::Security),
            "Documentation" => Some(SkillCategory::Documentation),
            "Database" => Some(SkillCategory::Database),
            "Productivity" => Some(SkillCategory::Productivity),
            "Other" => Some(SkillCategory::Other),
            _ => None,
        };
        cx.notify();
    }

    // -- Plugin action handlers -----------------------------------------------

    fn handle_plugin_import_open(
        &mut self,
        _action: &PluginImportOpen,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use hive_ui_panels::panels::skills::ImportState;
        info!("Plugin: import open");
        self.skills_data.import_state = ImportState::SelectMethod;
        cx.notify();
    }

    fn handle_plugin_import_cancel(
        &mut self,
        _action: &PluginImportCancel,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use hive_ui_panels::panels::skills::ImportState;
        info!("Plugin: import cancel");
        self.skills_data.import_state = ImportState::Closed;
        cx.notify();
    }

    /// Convert a backend `PluginPreview` into a UI-friendly `ImportPreview`.
    fn make_import_preview(preview: &PluginPreview) -> hive_ui_panels::panels::skills::ImportPreview {
        use hive_ui_panels::panels::skills::{ImportCommandEntry, ImportPreview, ImportSkillEntry};
        ImportPreview {
            name: preview.manifest.name.clone(),
            version: preview.manifest.version.clone(),
            author: preview.manifest.author.name.clone(),
            description: preview.manifest.description.clone(),
            skills: preview.skills.iter().map(|s| ImportSkillEntry {
                name: s.name.clone(),
                description: s.description.clone(),
                selected: true,
            }).collect(),
            commands: preview.commands.iter().map(|c| ImportCommandEntry {
                name: c.name.clone(),
                description: c.description.clone(),
                selected: true,
            }).collect(),
            security_warnings: preview.security_warnings.iter().map(|w| {
                format!("[{:?}] {}", w.severity, w.description)
            }).collect(),
        }
    }

    fn handle_plugin_import_from_github(
        &mut self,
        action: &PluginImportFromGitHub,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use hive_ui_panels::panels::skills::ImportState;
        info!("Plugin: import from GitHub '{}'", action.owner_repo);

        if action.owner_repo.is_empty() {
            self.skills_data.import_state = ImportState::InputGitHub(String::new());
            cx.notify();
            return;
        }

        // Validate owner/repo format.
        let parts: Vec<&str> = action.owner_repo.splitn(2, '/').collect();
        if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
            self.skills_data.import_state = ImportState::Done(
                "Invalid format. Use owner/repo (e.g. obra/superpowers)".into(),
                false,
            );
            cx.notify();
            return;
        }

        let owner = parts[0].to_string();
        let repo = parts[1].to_string();

        // Set state to Fetching.
        self.skills_data.import_state = ImportState::Fetching;
        cx.notify();

        // Clone PluginManager for the background thread.
        let pm = cx.global::<AppPluginManager>().0.clone();
        let source = PluginSource::GitHub { owner: owner.clone(), repo: repo.clone(), branch: None };

        let result_flag: Arc<std::sync::Mutex<Option<Result<PluginPreview, String>>>> =
            Arc::new(std::sync::Mutex::new(None));
        let result_for_thread = Arc::clone(&result_flag);

        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build();
            match rt {
                Ok(rt) => {
                    let result = rt.block_on(pm.fetch_from_github(&owner, &repo));
                    *result_for_thread.lock().unwrap_or_else(|e| e.into_inner()) =
                        Some(result.map_err(|e| format!("{e:#}")));
                }
                Err(e) => {
                    *result_for_thread.lock().unwrap_or_else(|e| e.into_inner()) =
                        Some(Err(format!("Failed to create async runtime: {e}")));
                }
            }
        });

        let result_for_ui = Arc::clone(&result_flag);
        cx.spawn(async move |this: WeakEntity<HiveWorkspace>, app: &mut AsyncApp| {
            loop {
                if let Some(result) = result_for_ui.lock().unwrap_or_else(|e| e.into_inner()).take() {
                    let _ = this.update(app, |this, cx| {
                        match result {
                            Ok(preview) => {
                                let ui_preview = Self::make_import_preview(&preview);
                                this.pending_plugin_preview = Some((preview, source));
                                this.skills_data.import_state = ImportState::Preview(ui_preview);
                            }
                            Err(e) => {
                                this.skills_data.import_state = ImportState::Done(
                                    format!("GitHub fetch failed: {e}"),
                                    false,
                                );
                            }
                        }
                        cx.notify();
                    });
                    break;
                }
                app.background_executor()
                    .timer(std::time::Duration::from_millis(150))
                    .await;
            }
        })
        .detach();
    }

    fn handle_plugin_import_from_url(
        &mut self,
        action: &PluginImportFromUrl,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use hive_ui_panels::panels::skills::ImportState;
        info!("Plugin: import from URL '{}'", action.url);

        if action.url.is_empty() {
            self.skills_data.import_state = ImportState::InputUrl(String::new());
            cx.notify();
            return;
        }

        // Basic URL validation.
        if !action.url.starts_with("http://") && !action.url.starts_with("https://") {
            self.skills_data.import_state = ImportState::Done(
                "Invalid URL. Must start with http:// or https://".into(),
                false,
            );
            cx.notify();
            return;
        }

        let url = action.url.clone();
        self.skills_data.import_state = ImportState::Fetching;
        cx.notify();

        let pm = cx.global::<AppPluginManager>().0.clone();
        let source = PluginSource::Url(url.clone());

        let result_flag: Arc<std::sync::Mutex<Option<Result<PluginPreview, String>>>> =
            Arc::new(std::sync::Mutex::new(None));
        let result_for_thread = Arc::clone(&result_flag);

        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build();
            match rt {
                Ok(rt) => {
                    let result = rt.block_on(pm.fetch_from_url(&url));
                    *result_for_thread.lock().unwrap_or_else(|e| e.into_inner()) =
                        Some(result.map_err(|e| format!("{e:#}")));
                }
                Err(e) => {
                    *result_for_thread.lock().unwrap_or_else(|e| e.into_inner()) =
                        Some(Err(format!("Failed to create async runtime: {e}")));
                }
            }
        });

        let result_for_ui = Arc::clone(&result_flag);
        cx.spawn(async move |this: WeakEntity<HiveWorkspace>, app: &mut AsyncApp| {
            loop {
                if let Some(result) = result_for_ui.lock().unwrap_or_else(|e| e.into_inner()).take() {
                    let _ = this.update(app, |this, cx| {
                        match result {
                            Ok(preview) => {
                                let ui_preview = Self::make_import_preview(&preview);
                                this.pending_plugin_preview = Some((preview, source));
                                this.skills_data.import_state = ImportState::Preview(ui_preview);
                            }
                            Err(e) => {
                                this.skills_data.import_state = ImportState::Done(
                                    format!("URL fetch failed: {e}"),
                                    false,
                                );
                            }
                        }
                        cx.notify();
                    });
                    break;
                }
                app.background_executor()
                    .timer(std::time::Duration::from_millis(150))
                    .await;
            }
        })
        .detach();
    }

    fn handle_plugin_import_from_local(
        &mut self,
        action: &PluginImportFromLocal,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use hive_ui_panels::panels::skills::ImportState;
        info!("Plugin: import from local '{}'", action.path);

        if action.path.is_empty() {
            self.skills_data.import_state = ImportState::InputLocal(None);
            cx.notify();
            return;
        }

        let path_str = action.path.clone();
        let path = std::path::Path::new(&path_str);
        if !path.exists() {
            self.skills_data.import_state = ImportState::Done(
                format!("Path does not exist: {}", action.path),
                false,
            );
            cx.notify();
            return;
        }

        self.skills_data.import_state = ImportState::Fetching;
        cx.notify();

        let source = PluginSource::Local { path: path_str.clone() };

        // load_from_local is sync — run on background thread to avoid blocking UI.
        let result_flag: Arc<std::sync::Mutex<Option<Result<PluginPreview, String>>>> =
            Arc::new(std::sync::Mutex::new(None));
        let result_for_thread = Arc::clone(&result_flag);

        std::thread::spawn(move || {
            let result = PluginManager::load_from_local(std::path::Path::new(&path_str));
            *result_for_thread.lock().unwrap_or_else(|e| e.into_inner()) =
                Some(result.map_err(|e| format!("{e:#}")));
        });

        let result_for_ui = Arc::clone(&result_flag);
        cx.spawn(async move |this: WeakEntity<HiveWorkspace>, app: &mut AsyncApp| {
            loop {
                if let Some(result) = result_for_ui.lock().unwrap_or_else(|e| e.into_inner()).take() {
                    let _ = this.update(app, |this, cx| {
                        match result {
                            Ok(preview) => {
                                let ui_preview = Self::make_import_preview(&preview);
                                this.pending_plugin_preview = Some((preview, source));
                                this.skills_data.import_state = ImportState::Preview(ui_preview);
                            }
                            Err(e) => {
                                this.skills_data.import_state = ImportState::Done(
                                    format!("Local load failed: {e}"),
                                    false,
                                );
                            }
                        }
                        cx.notify();
                    });
                    break;
                }
                app.background_executor()
                    .timer(std::time::Duration::from_millis(100))
                    .await;
            }
        })
        .detach();
    }

    fn handle_plugin_import_confirm(
        &mut self,
        _action: &PluginImportConfirm,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use hive_ui_panels::panels::skills::ImportState;
        info!("Plugin: import confirm");

        if let ImportState::Preview(ref preview) = self.skills_data.import_state {
            // Collect selected skill and command indices from the UI preview.
            let selected_skills: Vec<usize> = preview.skills.iter()
                .enumerate()
                .filter(|(_, s)| s.selected)
                .map(|(i, _)| i)
                .collect();
            let selected_commands: Vec<usize> = preview.commands.iter()
                .enumerate()
                .filter(|(_, c)| c.selected)
                .map(|(i, _)| i)
                .collect();

            self.skills_data.import_state = ImportState::Installing;
            cx.notify();

            if let Some((backend_preview, source)) = self.pending_plugin_preview.take() {
                if cx.has_global::<AppMarketplace>() {
                    let mp = &mut cx.global_mut::<AppMarketplace>().0;
                    let installed = mp.install_plugin(
                        &backend_preview,
                        source,
                        &selected_skills,
                        &selected_commands,
                    );
                    info!("Plugin installed: {} v{}", installed.name, installed.version);

                    // Persist to disk.
                    let plugins_path = dirs::home_dir()
                        .unwrap_or_default()
                        .join(".hive")
                        .join("plugins.json");
                    if let Err(e) = mp.save_plugins_to_file(&plugins_path) {
                        warn!("Failed to save plugins: {e}");
                    }
                }

                self.skills_data.import_state = ImportState::Done(
                    format!("Plugin '{}' installed successfully", backend_preview.manifest.name),
                    true,
                );
            } else {
                self.skills_data.import_state = ImportState::Done(
                    "No plugin data available — try importing again".into(),
                    false,
                );
            }

            self.refresh_skills_data(cx);
            cx.notify();
        }
    }

    fn handle_plugin_import_toggle_skill(
        &mut self,
        action: &PluginImportToggleSkill,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use hive_ui_panels::panels::skills::ImportState;
        info!("Plugin: toggle import skill at index {}", action.index);

        if let ImportState::Preview(ref mut preview) = self.skills_data.import_state {
            if let Some(skill) = preview.skills.get_mut(action.index) {
                skill.selected = !skill.selected;
                cx.notify();
            }
        }
    }

    fn handle_plugin_remove(
        &mut self,
        action: &PluginRemove,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        info!("Plugin: remove '{}'", action.plugin_id);

        if cx.has_global::<AppMarketplace>() {
            let mp = &mut cx.global_mut::<AppMarketplace>().0;
            if let Err(e) = mp.remove_plugin(&action.plugin_id) {
                warn!("Failed to remove plugin '{}': {e}", action.plugin_id);
            }
            let plugins_path = dirs::home_dir()
                .unwrap_or_default()
                .join(".hive")
                .join("plugins.json");
            if let Err(e) = mp.save_plugins_to_file(&plugins_path) {
                warn!("Failed to save plugins: {e}");
            }
        }

        self.refresh_skills_data(cx);
        cx.notify();
    }

    fn handle_plugin_update(
        &mut self,
        action: &PluginUpdate,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use hive_ui_panels::panels::skills::ImportState;
        info!("Plugin: update '{}'", action.plugin_id);

        // Find the installed plugin to get its source for re-fetching.
        let plugin_source = if cx.has_global::<AppMarketplace>() {
            let mp = &cx.global::<AppMarketplace>().0;
            mp.installed_plugins()
                .iter()
                .find(|p| p.id == action.plugin_id)
                .map(|p| p.source.clone())
        } else {
            None
        };

        let Some(source) = plugin_source else {
            self.skills_data.import_state = ImportState::Done(
                format!("Plugin '{}' not found", action.plugin_id),
                false,
            );
            cx.notify();
            return;
        };

        // Remove old plugin before re-fetching.
        let plugin_id = action.plugin_id.clone();
        if cx.has_global::<AppMarketplace>() {
            let mp = &mut cx.global_mut::<AppMarketplace>().0;
            let _ = mp.remove_plugin(&plugin_id);
            let plugins_path = dirs::home_dir()
                .unwrap_or_default()
                .join(".hive")
                .join("plugins.json");
            let _ = mp.save_plugins_to_file(&plugins_path);
        }

        self.skills_data.import_state = ImportState::Fetching;
        self.refresh_skills_data(cx);
        cx.notify();

        // Re-fetch from the original source.
        match source {
            PluginSource::GitHub { ref owner, ref repo, .. } => {
                let pm = cx.global::<AppPluginManager>().0.clone();
                let owner = owner.clone();
                let repo = repo.clone();
                let source_clone = source.clone();

                let result_flag: Arc<std::sync::Mutex<Option<Result<PluginPreview, String>>>> =
                    Arc::new(std::sync::Mutex::new(None));
                let result_for_thread = Arc::clone(&result_flag);

                std::thread::spawn(move || {
                    let rt = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build();
                    match rt {
                        Ok(rt) => {
                            let result = rt.block_on(pm.fetch_from_github(&owner, &repo));
                            *result_for_thread.lock().unwrap_or_else(|e| e.into_inner()) =
                                Some(result.map_err(|e| format!("{e:#}")));
                        }
                        Err(e) => {
                            *result_for_thread.lock().unwrap_or_else(|e| e.into_inner()) =
                                Some(Err(format!("Failed to create async runtime: {e}")));
                        }
                    }
                });

                let result_for_ui = Arc::clone(&result_flag);
                cx.spawn(async move |this: WeakEntity<HiveWorkspace>, app: &mut AsyncApp| {
                    loop {
                        if let Some(result) = result_for_ui.lock().unwrap_or_else(|e| e.into_inner()).take() {
                            let _ = this.update(app, |this, cx| {
                                match result {
                                    Ok(preview) => {
                                        let ui_preview = Self::make_import_preview(&preview);
                                        this.pending_plugin_preview = Some((preview, source_clone));
                                        this.skills_data.import_state = ImportState::Preview(ui_preview);
                                    }
                                    Err(e) => {
                                        this.skills_data.import_state = ImportState::Done(
                                            format!("Update fetch failed: {e}"),
                                            false,
                                        );
                                    }
                                }
                                cx.notify();
                            });
                            break;
                        }
                        app.background_executor()
                            .timer(std::time::Duration::from_millis(150))
                            .await;
                    }
                })
                .detach();
            }
            PluginSource::Url(ref url) => {
                let pm = cx.global::<AppPluginManager>().0.clone();
                let url = url.clone();
                let source_clone = source.clone();

                let result_flag: Arc<std::sync::Mutex<Option<Result<PluginPreview, String>>>> =
                    Arc::new(std::sync::Mutex::new(None));
                let result_for_thread = Arc::clone(&result_flag);

                std::thread::spawn(move || {
                    let rt = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build();
                    match rt {
                        Ok(rt) => {
                            let result = rt.block_on(pm.fetch_from_url(&url));
                            *result_for_thread.lock().unwrap_or_else(|e| e.into_inner()) =
                                Some(result.map_err(|e| format!("{e:#}")));
                        }
                        Err(e) => {
                            *result_for_thread.lock().unwrap_or_else(|e| e.into_inner()) =
                                Some(Err(format!("Failed to create async runtime: {e}")));
                        }
                    }
                });

                let result_for_ui = Arc::clone(&result_flag);
                cx.spawn(async move |this: WeakEntity<HiveWorkspace>, app: &mut AsyncApp| {
                    loop {
                        if let Some(result) = result_for_ui.lock().unwrap_or_else(|e| e.into_inner()).take() {
                            let _ = this.update(app, |this, cx| {
                                match result {
                                    Ok(preview) => {
                                        let ui_preview = Self::make_import_preview(&preview);
                                        this.pending_plugin_preview = Some((preview, source_clone));
                                        this.skills_data.import_state = ImportState::Preview(ui_preview);
                                    }
                                    Err(e) => {
                                        this.skills_data.import_state = ImportState::Done(
                                            format!("Update fetch failed: {e}"),
                                            false,
                                        );
                                    }
                                }
                                cx.notify();
                            });
                            break;
                        }
                        app.background_executor()
                            .timer(std::time::Duration::from_millis(150))
                            .await;
                    }
                })
                .detach();
            }
            PluginSource::Local { ref path } => {
                let path_str = path.clone();
                let source_clone = source.clone();

                let result_flag: Arc<std::sync::Mutex<Option<Result<PluginPreview, String>>>> =
                    Arc::new(std::sync::Mutex::new(None));
                let result_for_thread = Arc::clone(&result_flag);

                std::thread::spawn(move || {
                    let result = PluginManager::load_from_local(std::path::Path::new(&path_str));
                    *result_for_thread.lock().unwrap_or_else(|e| e.into_inner()) =
                        Some(result.map_err(|e| format!("{e:#}")));
                });

                let result_for_ui = Arc::clone(&result_flag);
                cx.spawn(async move |this: WeakEntity<HiveWorkspace>, app: &mut AsyncApp| {
                    loop {
                        if let Some(result) = result_for_ui.lock().unwrap_or_else(|e| e.into_inner()).take() {
                            let _ = this.update(app, |this, cx| {
                                match result {
                                    Ok(preview) => {
                                        let ui_preview = Self::make_import_preview(&preview);
                                        this.pending_plugin_preview = Some((preview, source_clone));
                                        this.skills_data.import_state = ImportState::Preview(ui_preview);
                                    }
                                    Err(e) => {
                                        this.skills_data.import_state = ImportState::Done(
                                            format!("Update load failed: {e}"),
                                            false,
                                        );
                                    }
                                }
                                cx.notify();
                            });
                            break;
                        }
                        app.background_executor()
                            .timer(std::time::Duration::from_millis(100))
                            .await;
                    }
                })
                .detach();
            }
        }
    }

    fn handle_plugin_toggle_expand(
        &mut self,
        action: &PluginToggleExpand,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        info!("Plugin: toggle expand '{}'", action.plugin_id);

        if let Some(plugin) = self
            .skills_data
            .installed_plugins
            .iter_mut()
            .find(|p| p.id == action.plugin_id)
        {
            plugin.expanded = !plugin.expanded;
            cx.notify();
        }
    }

    fn handle_plugin_toggle_skill(
        &mut self,
        action: &PluginToggleSkill,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        info!(
            "Plugin: toggle skill '{}' in '{}'",
            action.skill_name, action.plugin_id
        );

        if cx.has_global::<AppMarketplace>() {
            let mp = &mut cx.global_mut::<AppMarketplace>().0;
            if let Err(e) = mp.toggle_plugin_skill(&action.plugin_id, &action.skill_name) {
                warn!(
                    "Failed to toggle skill '{}' in plugin '{}': {e}",
                    action.skill_name, action.plugin_id
                );
            }
            let plugins_path = dirs::home_dir()
                .unwrap_or_default()
                .join(".hive")
                .join("plugins.json");
            if let Err(e) = mp.save_plugins_to_file(&plugins_path) {
                warn!("Failed to save plugins: {e}");
            }
        }

        self.refresh_skills_data(cx);
        cx.notify();
    }

    // -- Routing panel handlers ----------------------------------------------

    fn handle_routing_add_rule(
        &mut self,
        _action: &RoutingAddRule,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use hive_ui_panels::panels::routing::RoutingRule;
        info!("Routing: add rule");
        self.routing_data.custom_rules.push(RoutingRule {
            name: "New Rule".to_string(),
            condition: "task_type == \"code\"".to_string(),
            target_model: "auto".to_string(),
            enabled: true,
        });
        cx.notify();
    }

    // -- Token Launch panel handlers -----------------------------------------

    fn sync_token_launch_inputs_to_data(&mut self, cx: &App) {
        self.token_launch_data.token_name = self
            .token_launch_inputs
            .token_name
            .read(cx)
            .value()
            .trim()
            .to_string();
        self.token_launch_data.token_symbol = self
            .token_launch_inputs
            .token_symbol
            .read(cx)
            .value()
            .trim()
            .to_string();
        self.token_launch_data.total_supply = self
            .token_launch_inputs
            .total_supply
            .read(cx)
            .value()
            .trim()
            .to_string();

        let default_decimals = self
            .token_launch_data
            .selected_chain
            .map(|chain| chain.default_decimals())
            .unwrap_or(9);
        self.token_launch_data.decimals = self
            .token_launch_inputs
            .decimals
            .read(cx)
            .value()
            .trim()
            .parse::<u8>()
            .unwrap_or(default_decimals);

        if !matches!(
            self.token_launch_data.deploy_status,
            hive_ui_panels::panels::token_launch::DeployStatus::Deploying
        ) {
            self.token_launch_data.deploy_status =
                hive_ui_panels::panels::token_launch::DeployStatus::NotStarted;
        }
    }

    /// Retrieve (or generate) the wallet encryption password from secure storage.
    ///
    /// On first call a random 32-character alphanumeric password is generated,
    /// encrypted via `SecureStorage`, and persisted to `~/.hive/wallet_password.enc`.
    /// Subsequent calls decrypt and return the same password.
    ///
    /// Falls back to a hardcoded passphrase only when `SecureStorage` or the
    /// filesystem are completely unavailable, so existing wallets remain readable.
    fn token_launch_wallet_password() -> String {
        use hive_core::SecureStorage;

        const FALLBACK: &str = "hive-wallet-default";

        let password_path = match HiveConfig::base_dir() {
            Ok(dir) => dir.join("wallet_password.enc"),
            Err(_) => return FALLBACK.to_string(),
        };

        let storage = match SecureStorage::new() {
            Ok(s) => s,
            Err(_) => return FALLBACK.to_string(),
        };

        // Try to read an existing encrypted password.
        if let Ok(hex_ct) = std::fs::read_to_string(&password_path) {
            let hex_ct = hex_ct.trim();
            if !hex_ct.is_empty() {
                if let Ok(password) = storage.decrypt(hex_ct) {
                    return password;
                }
            }
        }

        // Generate a random 32-char alphanumeric password.
        let password = Self::generate_random_password(32);

        // Encrypt and persist.
        if let Ok(encrypted) = storage.encrypt(&password) {
            let _ = std::fs::write(&password_path, &encrypted);

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(
                    &password_path,
                    std::fs::Permissions::from_mode(0o600),
                );
            }
        }

        password
    }

    /// Generate a random alphanumeric password of the given length.
    fn generate_random_password(len: usize) -> String {
        use rand::Rng;
        const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
        let mut rng = rand::rng();
        (0..len)
            .map(|_| {
                let idx = rng.random_range(0..CHARSET.len());
                CHARSET[idx] as char
            })
            .collect()
    }

    fn token_launch_wallet_path() -> PathBuf {
        HiveConfig::base_dir()
            .map(|dir| dir.join("wallets.enc"))
            .unwrap_or_else(|_| PathBuf::from("wallets.enc"))
    }

    fn token_launch_rpc_config_path() -> PathBuf {
        HiveConfig::base_dir()
            .map(|dir| dir.join("rpc_config.json"))
            .unwrap_or_else(|_| PathBuf::from("rpc_config.json"))
    }

    fn token_launch_chain(
        option: hive_ui_panels::panels::token_launch::ChainOption,
    ) -> hive_blockchain::Chain {
        match option {
            hive_ui_panels::panels::token_launch::ChainOption::Solana => hive_blockchain::Chain::Solana,
            hive_ui_panels::panels::token_launch::ChainOption::Ethereum => hive_blockchain::Chain::Ethereum,
            hive_ui_panels::panels::token_launch::ChainOption::Base => hive_blockchain::Chain::Base,
        }
    }

    fn token_launch_secret_placeholder(
        option: Option<hive_ui_panels::panels::token_launch::ChainOption>,
    ) -> &'static str {
        match option {
            Some(hive_ui_panels::panels::token_launch::ChainOption::Solana) => {
                "Solana private key (hex or base58)"
            }
            Some(
                hive_ui_panels::panels::token_launch::ChainOption::Ethereum
                | hive_ui_panels::panels::token_launch::ChainOption::Base,
            ) => "EVM private key (hex)",
            None => "Select a chain to configure wallet import",
        }
    }

    fn token_launch_current_rpc_url(
        chain: hive_blockchain::Chain,
        cx: &App,
    ) -> String {
        if cx.has_global::<AppRpcConfig>() {
            return cx
                .global::<AppRpcConfig>()
                .0
                .get_rpc(chain)
                .map(|config| config.url.clone())
                .unwrap_or_default();
        }

        hive_blockchain::RpcConfigStore::with_defaults()
            .get_rpc(chain)
            .map(|config| config.url.clone())
            .unwrap_or_default()
    }

    fn sync_token_launch_rpc_input(
        &self,
        option: Option<hive_ui_panels::panels::token_launch::ChainOption>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (value, placeholder) = match option {
            Some(chain) => {
                let chain = Self::token_launch_chain(chain);
                (
                    Self::token_launch_current_rpc_url(chain, cx),
                    "https://rpc.example.com",
                )
            }
            None => (
                String::new(),
                "Select a chain to configure RPC",
            ),
        };

        self.token_launch_inputs.rpc_url.update(cx, |state, cx| {
            state.set_placeholder(placeholder, window, cx);
            state.set_value(value, window, cx);
        });
    }

    fn persist_token_launch_rpc_config(&self, cx: &mut Context<Self>) -> anyhow::Result<()> {
        if !cx.has_global::<AppRpcConfig>() {
            return Ok(());
        }

        let rpc_path = Self::token_launch_rpc_config_path();
        if let Some(parent) = rpc_path.parent()
            && !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }

        cx.global::<AppRpcConfig>().0.save_to_file(&rpc_path)?;
        Ok(())
    }

    fn persist_token_launch_wallets(&self, cx: &mut Context<Self>) -> anyhow::Result<()> {
        if !cx.has_global::<AppWallets>() {
            return Ok(());
        }

        let wallet_path = Self::token_launch_wallet_path();
        if let Some(parent) = wallet_path.parent()
            && !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }

        cx.global::<AppWallets>().0.save_to_file(&wallet_path)?;
        Ok(())
    }

    fn clear_token_launch_wallet_secret(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.token_launch_inputs.wallet_secret.update(cx, |state, cx| {
            state.set_value(String::new(), window, cx);
        });
    }

    fn sync_token_launch_saved_wallets(
        &mut self,
        preserve_current: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(option) = self.token_launch_data.selected_chain else {
            self.token_launch_data.available_wallets.clear();
            self.token_launch_data.wallet_id = None;
            self.token_launch_data.wallet_address = None;
            self.token_launch_data.wallet_balance = None;
            self.token_launch_inputs.wallet_name.update(cx, |state, cx| {
                state.set_value(String::new(), window, cx);
            });
            return;
        };

        let chain = Self::token_launch_chain(option);
        let current_wallet_id = if preserve_current {
            self.token_launch_data.wallet_id.clone()
        } else {
            None
        };

        let available_wallets = if cx.has_global::<AppWallets>() {
            let mut wallets = cx
                .global::<AppWallets>()
                .0
                .list_wallets()
                .into_iter()
                .filter(|wallet| wallet.chain == chain)
                .collect::<Vec<_>>();
            wallets.sort_by_key(|wallet| std::cmp::Reverse(wallet.created_at));
            wallets
                .into_iter()
                .map(|wallet| hive_ui_panels::panels::token_launch::SavedWalletOption {
                    id: wallet.id.clone(),
                    name: wallet.name.clone(),
                    address: wallet.address.clone(),
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };

        self.token_launch_data.available_wallets = available_wallets;
        let selected_wallet = current_wallet_id
            .as_deref()
            .and_then(|id| {
                self.token_launch_data
                    .available_wallets
                    .iter()
                    .find(|wallet| wallet.id == id)
            })
            .or_else(|| self.token_launch_data.available_wallets.first())
            .cloned();

        if let Some(wallet) = selected_wallet {
            self.token_launch_data.wallet_id = Some(wallet.id.clone());
            self.token_launch_data.wallet_address = Some(wallet.address.clone());
            self.token_launch_inputs.wallet_name.update(cx, |state, cx| {
                state.set_value(wallet.name.clone(), window, cx);
            });
            self.refresh_token_launch_balance(cx);
        } else {
            self.token_launch_data.wallet_id = None;
            self.token_launch_data.wallet_address = None;
            self.token_launch_data.wallet_balance = None;
            self.token_launch_inputs.wallet_name.update(cx, |state, cx| {
                state.set_value(String::new(), window, cx);
            });
        }

        self.clear_token_launch_wallet_secret(window, cx);
    }

    fn restore_token_launch_wallet_for_chain(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.sync_token_launch_saved_wallets(false, window, cx);
    }

    fn refresh_token_launch_cost(&mut self, cx: &mut Context<Self>) {
        let Some(option) = self.token_launch_data.selected_chain else {
            self.token_launch_data.estimated_cost = None;
            cx.notify();
            return;
        };
        let chain = Self::token_launch_chain(option);
        let rpc_url = Self::token_launch_current_rpc_url(chain, cx);

        cx.spawn(async move |this: WeakEntity<HiveWorkspace>, app: &mut AsyncApp| {
            let estimated_cost = match option {
                hive_ui_panels::panels::token_launch::ChainOption::Solana => {
                    hive_blockchain::solana::estimate_deploy_cost_with_rpc(Some(rpc_url.as_str()))
                        .await
                        .ok()
                }
                hive_ui_panels::panels::token_launch::ChainOption::Ethereum
                | hive_ui_panels::panels::token_launch::ChainOption::Base => {
                    hive_blockchain::evm::estimate_deploy_cost_with_rpc(
                        Self::token_launch_chain(option),
                        Some(rpc_url.as_str()),
                    )
                    .await
                    .ok()
                }
            };

            let _ = this.update(app, |this, cx| {
                this.token_launch_data.estimated_cost = estimated_cost;
                cx.notify();
            });
        })
        .detach();
    }

    fn refresh_token_launch_balance(&mut self, cx: &mut Context<Self>) {
        let (Some(option), Some(address)) = (
            self.token_launch_data.selected_chain,
            self.token_launch_data.wallet_address.clone(),
        ) else {
            self.token_launch_data.wallet_balance = None;
            cx.notify();
            return;
        };
        let chain = Self::token_launch_chain(option);
        let rpc_url = Self::token_launch_current_rpc_url(chain, cx);

        cx.spawn(async move |this: WeakEntity<HiveWorkspace>, app: &mut AsyncApp| {
            let balance = match option {
                hive_ui_panels::panels::token_launch::ChainOption::Solana => {
                    hive_blockchain::solana::get_balance_with_rpc(
                        &address,
                        Some(rpc_url.as_str()),
                    )
                    .await
                    .ok()
                }
                hive_ui_panels::panels::token_launch::ChainOption::Ethereum
                | hive_ui_panels::panels::token_launch::ChainOption::Base => {
                    hive_blockchain::evm::get_balance_with_rpc(
                        &address,
                        Self::token_launch_chain(option),
                        Some(rpc_url.as_str()),
                    )
                    .await
                    .ok()
                }
            };

            let _ = this.update(app, |this, cx| {
                this.token_launch_data.wallet_balance = balance;
                cx.notify();
            });
        })
        .detach();
    }

    fn handle_token_launch_set_step(
        &mut self,
        action: &TokenLaunchSetStep,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use hive_ui_panels::panels::token_launch::WizardStep;
        info!("TokenLaunch: set step {}", action.step);
        self.token_launch_data.current_step = match action.step {
            0 => WizardStep::SelectChain,
            1 => WizardStep::TokenDetails,
            2 => WizardStep::WalletSetup,
            _ => WizardStep::Deploy,
        };
        cx.notify();
    }

    fn handle_token_launch_select_chain(
        &mut self,
        action: &TokenLaunchSelectChain,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use hive_ui_panels::panels::token_launch::ChainOption;
        info!("TokenLaunch: select chain {}", action.chain);
        self.token_launch_data.selected_chain = match action.chain.as_str() {
            "solana" => Some(ChainOption::Solana),
            "ethereum" => Some(ChainOption::Ethereum),
            "base" => Some(ChainOption::Base),
            _ => None,
        };

        if let Some(chain) = self.token_launch_data.selected_chain {
            self.token_launch_data.decimals = chain.default_decimals();
            self.token_launch_inputs.decimals.update(cx, |state, cx| {
                state.set_value(chain.default_decimals().to_string(), window, cx);
            });
            self.token_launch_inputs.wallet_secret.update(cx, |state, cx| {
                state.set_placeholder(Self::token_launch_secret_placeholder(Some(chain)), window, cx);
            });
        } else {
            self.token_launch_data.estimated_cost = None;
            self.token_launch_inputs.wallet_secret.update(cx, |state, cx| {
                state.set_placeholder(Self::token_launch_secret_placeholder(None), window, cx);
            });
        }

        self.sync_token_launch_inputs_to_data(cx);
        self.sync_token_launch_rpc_input(self.token_launch_data.selected_chain, window, cx);
        self.restore_token_launch_wallet_for_chain(window, cx);
        self.refresh_token_launch_cost(cx);
        cx.notify();
    }

    fn handle_token_launch_save_rpc_config(
        &mut self,
        _action: &TokenLaunchSaveRpcConfig,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(option) = self.token_launch_data.selected_chain else {
            self.push_notification(
                cx,
                NotificationType::Warning,
                "Token Launch",
                "Select a target chain before saving an RPC endpoint.",
            );
            return;
        };

        let rpc_url = self
            .token_launch_inputs
            .rpc_url
            .read(cx)
            .value()
            .trim()
            .to_string();
        if rpc_url.is_empty() {
            self.push_notification(
                cx,
                NotificationType::Warning,
                "Token Launch",
                "RPC endpoint cannot be empty. Use Reset RPC to restore the default.",
            );
            return;
        }

        let chain = Self::token_launch_chain(option);
        let result = if cx.has_global::<AppRpcConfig>() {
            cx.global_mut::<AppRpcConfig>().0.set_custom_rpc(chain, rpc_url.clone())
        } else {
            Err(anyhow::anyhow!("RPC config store is not available."))
        };

        match result {
            Ok(()) => {
                if let Err(e) = self.persist_token_launch_rpc_config(cx) {
                    self.push_notification(
                        cx,
                        NotificationType::Warning,
                        "Token Launch",
                        format!("RPC endpoint saved, but persistence failed: {e}"),
                    );
                }
                self.refresh_token_launch_cost(cx);
                self.refresh_token_launch_balance(cx);
                self.push_notification(
                    cx,
                    NotificationType::Success,
                    "Token Launch",
                    format!("Saved custom RPC for {}.", chain.label()),
                );
                cx.notify();
            }
            Err(e) => {
                self.push_notification(
                    cx,
                    NotificationType::Error,
                    "Token Launch",
                    format!("Invalid RPC endpoint: {e}"),
                );
            }
        }
    }

    fn handle_token_launch_reset_rpc_config(
        &mut self,
        _action: &TokenLaunchResetRpcConfig,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(option) = self.token_launch_data.selected_chain else {
            self.push_notification(
                cx,
                NotificationType::Warning,
                "Token Launch",
                "Select a target chain before resetting an RPC endpoint.",
            );
            return;
        };

        let chain = Self::token_launch_chain(option);
        if cx.has_global::<AppRpcConfig>() {
            cx.global_mut::<AppRpcConfig>().0.reset_to_default(chain);
        } else {
            self.push_notification(
                cx,
                NotificationType::Error,
                "Token Launch",
                "RPC config store is not available.",
            );
            return;
        }

        if let Err(e) = self.persist_token_launch_rpc_config(cx) {
            self.push_notification(
                cx,
                NotificationType::Warning,
                "Token Launch",
                format!("RPC endpoint reset, but persistence failed: {e}"),
            );
        }

        self.sync_token_launch_rpc_input(Some(option), window, cx);
        self.refresh_token_launch_cost(cx);
        self.refresh_token_launch_balance(cx);
        self.push_notification(
            cx,
            NotificationType::Success,
            "Token Launch",
            format!("Restored default RPC for {}.", chain.label()),
        );
        cx.notify();
    }

    fn handle_token_launch_create_wallet(
        &mut self,
        _action: &TokenLaunchCreateWallet,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.sync_token_launch_inputs_to_data(cx);

        let Some(option) = self.token_launch_data.selected_chain else {
            self.push_notification(
                cx,
                NotificationType::Warning,
                "Token Launch",
                "Select a target chain before creating a wallet.",
            );
            return;
        };

        let chain = Self::token_launch_chain(option);
        let wallet_name = self
            .token_launch_inputs
            .wallet_name
            .read(cx)
            .value()
            .trim()
            .to_string();
        let wallet_name = if wallet_name.is_empty() {
            format!("{} Wallet", chain.label())
        } else {
            wallet_name
        };

        let (private_key, address) = match hive_blockchain::generate_wallet_material(chain) {
            Ok(material) => material,
            Err(e) => {
                self.push_notification(
                    cx,
                    NotificationType::Error,
                    "Token Launch",
                    format!("Wallet creation failed: {e}"),
                );
                return;
            }
        };

        let encrypted_key = match hive_blockchain::encrypt_key(
            &private_key,
            &Self::token_launch_wallet_password(),
        ) {
            Ok(encrypted) => encrypted,
            Err(e) => {
                self.push_notification(
                    cx,
                    NotificationType::Error,
                    "Token Launch",
                    format!("Wallet encryption failed: {e}"),
                );
                return;
            }
        };

        let wallet_id = if cx.has_global::<AppWallets>() {
            cx.global_mut::<AppWallets>()
                .0
                .add_wallet(wallet_name.clone(), chain, address.clone(), encrypted_key)
        } else {
            self.push_notification(
                cx,
                NotificationType::Error,
                "Token Launch",
                "Wallet store is not available.",
            );
            return;
        };

        if let Err(e) = self.persist_token_launch_wallets(cx) {
            self.push_notification(
                cx,
                NotificationType::Warning,
                "Token Launch",
                format!("Wallet created, but saving failed: {e}"),
            );
        }

        self.token_launch_data.wallet_id = Some(wallet_id);
        self.token_launch_data.wallet_address = Some(address);
        self.token_launch_data.wallet_balance = None;
        self.sync_token_launch_saved_wallets(true, window, cx);
        self.token_launch_inputs.wallet_name.update(cx, |state, cx| {
            state.set_value(wallet_name, window, cx);
        });
        self.clear_token_launch_wallet_secret(window, cx);
        self.refresh_token_launch_balance(cx);
        self.push_notification(
            cx,
            NotificationType::Success,
            "Token Launch",
            "Wallet created and connected.",
        );
        cx.notify();
    }

    fn handle_token_launch_import_wallet(
        &mut self,
        _action: &TokenLaunchImportWallet,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.sync_token_launch_inputs_to_data(cx);

        let Some(option) = self.token_launch_data.selected_chain else {
            self.push_notification(
                cx,
                NotificationType::Warning,
                "Token Launch",
                "Select a target chain before importing a wallet.",
            );
            return;
        };

        let chain = Self::token_launch_chain(option);
        let wallet_name = self
            .token_launch_inputs
            .wallet_name
            .read(cx)
            .value()
            .trim()
            .to_string();
        let wallet_name = if wallet_name.is_empty() {
            format!("Imported {}", chain.label())
        } else {
            wallet_name
        };
        let secret = self
            .token_launch_inputs
            .wallet_secret
            .read(cx)
            .value()
            .trim()
            .to_string();

        let (private_key, address) = match hive_blockchain::import_wallet_material(chain, &secret) {
            Ok(material) => material,
            Err(e) => {
                self.push_notification(
                    cx,
                    NotificationType::Error,
                    "Token Launch",
                    format!("Wallet import failed: {e}"),
                );
                return;
            }
        };

        let encrypted_key = match hive_blockchain::encrypt_key(
            &private_key,
            &Self::token_launch_wallet_password(),
        ) {
            Ok(encrypted) => encrypted,
            Err(e) => {
                self.push_notification(
                    cx,
                    NotificationType::Error,
                    "Token Launch",
                    format!("Wallet encryption failed: {e}"),
                );
                return;
            }
        };

        let wallet_id = if cx.has_global::<AppWallets>() {
            cx.global_mut::<AppWallets>()
                .0
                .add_wallet(wallet_name.clone(), chain, address.clone(), encrypted_key)
        } else {
            self.push_notification(
                cx,
                NotificationType::Error,
                "Token Launch",
                "Wallet store is not available.",
            );
            return;
        };

        if let Err(e) = self.persist_token_launch_wallets(cx) {
            self.push_notification(
                cx,
                NotificationType::Warning,
                "Token Launch",
                format!("Wallet imported, but saving failed: {e}"),
            );
        }

        self.token_launch_data.wallet_id = Some(wallet_id);
        self.token_launch_data.wallet_address = Some(address);
        self.token_launch_data.wallet_balance = None;
        self.sync_token_launch_saved_wallets(true, window, cx);
        self.token_launch_inputs.wallet_name.update(cx, |state, cx| {
            state.set_value(wallet_name, window, cx);
        });
        self.clear_token_launch_wallet_secret(window, cx);
        self.refresh_token_launch_balance(cx);
        self.push_notification(
            cx,
            NotificationType::Success,
            "Token Launch",
            "Wallet imported and connected.",
        );
        cx.notify();
    }

    fn handle_token_launch_select_wallet(
        &mut self,
        action: &TokenLaunchSelectWallet,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(option) = self.token_launch_data.selected_chain else {
            self.push_notification(
                cx,
                NotificationType::Warning,
                "Token Launch",
                "Select a target chain before choosing a wallet.",
            );
            return;
        };

        let chain = Self::token_launch_chain(option);
        let selected_wallet = if cx.has_global::<AppWallets>() {
            cx.global::<AppWallets>()
                .0
                .get_wallet(&action.wallet_id)
                .filter(|wallet| wallet.chain == chain)
                .map(|wallet| {
                    (
                        wallet.id.clone(),
                        wallet.name.clone(),
                        wallet.address.clone(),
                    )
                })
        } else {
            None
        };

        if let Some((wallet_id, wallet_name, wallet_address)) = selected_wallet {
            self.token_launch_data.wallet_id = Some(wallet_id);
            self.token_launch_data.wallet_address = Some(wallet_address);
            self.token_launch_inputs.wallet_name.update(cx, |state, cx| {
                state.set_value(wallet_name, window, cx);
            });
            self.sync_token_launch_saved_wallets(true, window, cx);
            self.push_notification(
                cx,
                NotificationType::Success,
                "Token Launch",
                "Connected saved wallet.",
            );
            cx.notify();
        } else {
            self.push_notification(
                cx,
                NotificationType::Error,
                "Token Launch",
                "Saved wallet not found for the selected chain.",
            );
        }
    }

    fn handle_token_launch_deploy(
        &mut self,
        _action: &TokenLaunchDeploy,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        info!("TokenLaunch: deploy");
        use hive_ui_panels::panels::token_launch::DeployStatus;
        self.sync_token_launch_inputs_to_data(cx);

        if self.token_launch_data.selected_chain.is_none() {
            self.token_launch_data.deploy_status =
                DeployStatus::Failed("Select a target chain before deploying.".to_string());
            cx.notify();
            return;
        }

        if self.token_launch_data.token_name.trim().is_empty()
            || self.token_launch_data.token_symbol.trim().is_empty()
            || self.token_launch_data.total_supply.trim().is_empty()
        {
            self.token_launch_data.deploy_status = DeployStatus::Failed(
                "Token name, symbol, and total supply are required.".to_string(),
            );
            cx.notify();
            return;
        }

        if self.token_launch_data.wallet_address.is_none() || self.token_launch_data.wallet_id.is_none() {
            self.token_launch_data.deploy_status =
                DeployStatus::Failed("Connect a wallet before deploying.".to_string());
            cx.notify();
            return;
        }

        if let (Some(balance), Some(cost)) = (
            self.token_launch_data.wallet_balance,
            self.token_launch_data.estimated_cost,
        ) && balance < cost {
            self.token_launch_data.deploy_status = DeployStatus::Failed(
                "Connected wallet does not have enough funds for the estimated deployment cost."
                    .to_string(),
            );
            cx.notify();
            return;
        }

        let wallet_id = self.token_launch_data.wallet_id.clone().unwrap_or_default();
        let private_key = if cx.has_global::<AppWallets>() {
            match cx
                .global::<AppWallets>()
                .0
                .decrypt_wallet_key(&wallet_id, &Self::token_launch_wallet_password())
            {
                Ok(key) => key,
                Err(e) => {
                    self.token_launch_data.deploy_status =
                        DeployStatus::Failed(format!("Failed to unlock wallet: {e}"));
                    cx.notify();
                    return;
                }
            }
        } else {
            self.token_launch_data.deploy_status =
                DeployStatus::Failed("Wallet store is not available.".to_string());
            cx.notify();
            return;
        };

        let selected_chain = self.token_launch_data.selected_chain.unwrap();
        let token_name = self.token_launch_data.token_name.clone();
        let token_symbol = self.token_launch_data.token_symbol.clone();
        let total_supply = self.token_launch_data.total_supply.clone();
        let decimals = self.token_launch_data.decimals;
        let rpc_url =
            Self::token_launch_current_rpc_url(Self::token_launch_chain(selected_chain), cx);

        self.token_launch_data.deploy_status = DeployStatus::Deploying;
        cx.notify();

        cx.spawn(async move |this: WeakEntity<HiveWorkspace>, app: &mut AsyncApp| {
            let deploy_result = match selected_chain {
                hive_ui_panels::panels::token_launch::ChainOption::Solana => {
                    match total_supply.parse::<u64>() {
                        Ok(supply) => hive_blockchain::solana::create_spl_token_with_rpc(
                            hive_blockchain::SplTokenParams {
                                name: token_name,
                                symbol: token_symbol,
                                decimals,
                                supply,
                                metadata_uri: None,
                            },
                            &private_key,
                            Some(rpc_url.as_str()),
                        )
                        .await
                        .map(|result| result.mint_address),
                        Err(_) => Err(anyhow::anyhow!(
                            "Total supply must fit into an unsigned 64-bit integer for Solana deployments."
                        )),
                    }
                }
                hive_ui_panels::panels::token_launch::ChainOption::Ethereum
                | hive_ui_panels::panels::token_launch::ChainOption::Base => {
                    hive_blockchain::evm::deploy_token_with_rpc(
                        hive_blockchain::TokenDeployParams {
                            name: token_name,
                            symbol: token_symbol,
                            decimals,
                            total_supply,
                            chain: Self::token_launch_chain(selected_chain),
                        },
                        &private_key,
                        Some(rpc_url.as_str()),
                    )
                    .await
                    .map(|result| result.contract_address)
                }
            };

            let _ = this.update(app, |this, cx| {
                this.token_launch_data.deploy_status = match deploy_result {
                    Ok(address) => DeployStatus::Success(address),
                    Err(e) => DeployStatus::Failed(e.to_string()),
                };
                cx.notify();
            });
        })
        .detach();
    }

    // -- Settings panel handlers ---------------------------------------------

    fn handle_settings_save(
        &mut self,
        _action: &SettingsSave,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        // Save is now handled via SettingsSaved event from SettingsView.
        // The action still dispatches to the view which emits the event.
    }

    /// Handle the `ThemeChanged` action: resolve the new theme by name,
    /// update `self.theme`, persist to config, refresh the `AppTheme` global,
    /// and propagate the theme to child views.
    fn handle_theme_changed(
        &mut self,
        action: &ThemeChanged,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let new_theme = Self::resolve_theme_by_name(&action.theme_name);
        self.theme = new_theme.clone();

        // Update the global so child views can read the new theme.
        cx.set_global(AppTheme(new_theme.clone()));
        Self::sync_gpui_theme(&new_theme, cx);

        // Persist the theme name to config.
        if cx.has_global::<AppConfig>() {
            let config_mgr = &cx.global::<AppConfig>().0;
            if let Err(e) = config_mgr.update(|cfg| {
                cfg.theme = action.theme_name.clone();
            }) {
                error!("Failed to persist theme to config: {e}");
            }
        }

        // Push the updated theme to sub-views that cache their own copy.
        let theme_name_for_settings = action.theme_name.clone();
        self.settings_view.update(cx, |view, cx| {
            view.set_theme(new_theme.clone(), cx);
            view.set_selected_theme(theme_name_for_settings, cx);
        });
        self.chat_input.update(cx, |view, cx| {
            view.set_theme(new_theme.clone(), cx);
        });
        self.models_browser_view.update(cx, |view, cx| {
            view.set_theme(new_theme.clone(), cx);
        });
        self.channels_view.update(cx, |view, cx| {
            view.set_theme(new_theme.clone(), cx);
        });
        self.workflow_builder_view.update(cx, |view, cx| {
            view.set_theme(new_theme.clone(), cx);
        });

        info!("Theme changed to: {}", action.theme_name);
        cx.notify();
    }

    /// Handle context format change: persist to config and update settings UI.
    fn handle_context_format_changed(
        &mut self,
        action: &ContextFormatChanged,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if cx.has_global::<AppConfig>() {
            let fmt = action.format.clone();
            if let Err(e) = cx.global::<AppConfig>().0.update(|cfg| {
                cfg.context_format = fmt;
            }) {
                error!("Failed to persist context_format: {e}");
            }
        }
        let fmt = action.format.clone();
        self.settings_view.update(cx, |view, cx| {
            view.set_selected_context_format(fmt, cx);
        });
        info!("Context format changed to: {}", action.format);
        cx.notify();
    }

    fn handle_export_config(
        &mut self,
        _action: &ExportConfig,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !cx.has_global::<AppConfig>() {
            warn!("ExportConfig: no AppConfig available");
            return;
        }

        // Password dialog will be wired in a future iteration; for now we use
        // a fixed passphrase so exports are portable between machines.
        let password = "hive-export-default";

        let blob = match cx.global::<AppConfig>().0.export_config(password) {
            Ok(b) => b,
            Err(e) => {
                error!("ExportConfig: export failed: {e}");
                self.push_notification(
                    cx,
                    NotificationType::Error,
                    "Config Export",
                    format!("Failed to export config: {e}"),
                );
                return;
            }
        };

        let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
        let export_dir = HiveConfig::base_dir()
            .map(|d| d.join("exports"))
            .unwrap_or_else(|_| PathBuf::from(".hive/exports"));
        let export_path = export_dir.join(format!("hive-config-{timestamp}.enc"));

        let result = (|| -> anyhow::Result<()> {
            std::fs::create_dir_all(&export_dir)?;
            std::fs::write(&export_path, &blob)?;
            Ok(())
        })();

        match result {
            Ok(()) => {
                let len = blob.len();
                info!(
                    "ExportConfig: wrote {len} bytes to {}",
                    export_path.display()
                );
                self.push_notification(
                    cx,
                    NotificationType::Success,
                    "Config Export",
                    format!("Exported to {}", export_path.display()),
                );
            }
            Err(e) => {
                error!("ExportConfig: failed to write file: {e}");
                self.push_notification(
                    cx,
                    NotificationType::Error,
                    "Config Export",
                    format!("Failed to write export file: {e}"),
                );
            }
        }
    }

    fn handle_import_config(
        &mut self,
        _action: &ImportConfig,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !cx.has_global::<AppConfig>() {
            warn!("ImportConfig: no AppConfig available");
            return;
        }

        // Locate the most recent .enc export in ~/.hive/exports/
        let export_dir = match HiveConfig::base_dir().map(|d| d.join("exports")) {
            Ok(d) => d,
            Err(e) => {
                error!("ImportConfig: cannot resolve exports dir: {e}");
                self.push_notification(
                    cx,
                    NotificationType::Error,
                    "Config Import",
                    format!("Cannot resolve exports directory: {e}"),
                );
                return;
            }
        };

        let latest_file = std::fs::read_dir(&export_dir)
            .ok()
            .and_then(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .filter(|e| {
                        e.path()
                            .extension()
                            .map_or(false, |ext| ext == "enc")
                    })
                    .filter(|e| {
                        e.file_name()
                            .to_string_lossy()
                            .starts_with("hive-config-")
                    })
                    .max_by_key(|e| {
                        e.metadata()
                            .and_then(|m| m.modified())
                            .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
                    })
                    .map(|e| e.path())
            });

        let import_path = match latest_file {
            Some(p) => p,
            None => {
                warn!("ImportConfig: no export files found in {}", export_dir.display());
                self.push_notification(
                    cx,
                    NotificationType::Warning,
                    "Config Import",
                    format!(
                        "No export files found. Place a .enc export in {}",
                        export_dir.display()
                    ),
                );
                return;
            }
        };

        let data = match std::fs::read(&import_path) {
            Ok(d) => d,
            Err(e) => {
                error!("ImportConfig: failed to read {}: {e}", import_path.display());
                self.push_notification(
                    cx,
                    NotificationType::Error,
                    "Config Import",
                    format!("Failed to read {}: {e}", import_path.display()),
                );
                return;
            }
        };

        // Must match the password used during export.
        let password = "hive-export-default";

        match cx.global::<AppConfig>().0.import_config(&data, password) {
            Ok(()) => {
                info!(
                    "ImportConfig: successfully imported from {}",
                    import_path.display()
                );
                self.push_notification(
                    cx,
                    NotificationType::Success,
                    "Config Import",
                    format!("Imported config from {}", import_path.display()),
                );
            }
            Err(e) => {
                error!("ImportConfig: import failed: {e}");
                self.push_notification(
                    cx,
                    NotificationType::Error,
                    "Config Import",
                    format!("Import failed: {e}"),
                );
            }
        }
    }

    /// Called when `SettingsView` emits `SettingsSaved`. Reads all values from
    /// the view and persists them to `AppConfig`.
    /// Push API keys + enabled providers to the models browser view.
    fn push_keys_to_models_browser(&mut self, cx: &mut Context<Self>) {
        if !cx.has_global::<AppConfig>() {
            return;
        }
        let config = cx.global::<AppConfig>().0.get();

        let mut providers = HashSet::new();
        let or_key = config.openrouter_api_key.clone();
        let openai_key = config.openai_api_key.clone();
        let anthropic_key = config.anthropic_api_key.clone();
        let google_key = config.google_api_key.clone();
        let groq_key = config.groq_api_key.clone();
        let xai_key = config.xai_api_key.clone();
        let hf_key = config.huggingface_api_key.clone();

        if or_key.is_some() {
            providers.insert(hive_ai::types::ProviderType::OpenRouter);
        }
        if openai_key.is_some() {
            providers.insert(hive_ai::types::ProviderType::OpenAI);
        }
        if anthropic_key.is_some() {
            providers.insert(hive_ai::types::ProviderType::Anthropic);
        }
        if google_key.is_some() {
            providers.insert(hive_ai::types::ProviderType::Google);
        }
        if groq_key.is_some() {
            providers.insert(hive_ai::types::ProviderType::Groq);
        }
        if xai_key.is_some() {
            providers.insert(hive_ai::types::ProviderType::XAI);
        }
        if hf_key.is_some() {
            providers.insert(hive_ai::types::ProviderType::HuggingFace);
        }

        self.models_browser_view.update(cx, |browser, cx| {
            browser.set_enabled_providers(providers, cx);
            browser.set_openrouter_api_key(or_key, cx);
            browser.set_openai_api_key(openai_key, cx);
            browser.set_anthropic_api_key(anthropic_key, cx);
            browser.set_google_api_key(google_key, cx);
            browser.set_groq_api_key(groq_key, cx);
            browser.set_xai_api_key(xai_key, cx);
            browser.set_huggingface_api_key(hf_key, cx);
        });
    }

    /// Handle changes to the project model list from the models browser.
    fn handle_project_models_changed(&mut self, models: &[String], cx: &mut Context<Self>) {
        // Persist to config.
        if cx.has_global::<AppConfig>()
            && let Err(e) = cx.global::<AppConfig>().0.update(|cfg| {
                cfg.project_models = models.to_vec();
            })
        {
            warn!("Models: failed to persist project_models: {e}");
        }

        // Push to settings model selector.
        self.settings_view.update(cx, |settings, cx| {
            settings.set_project_models(models.to_vec(), cx);
        });

        // Rebuild auto-routing fallback chain from project models.
        if cx.has_global::<AppAiService>() {
            cx.global_mut::<AppAiService>()
                .0
                .rebuild_fallback_chain_from_project_models(models);
        }

        // Validate current chat model against the project set.
        // If the active model is not in the project list, switch to the first
        // project model (or the config default).
        if !models.is_empty() {
            let current_model = self.chat_service.read(cx).current_model().to_string();
            let model_set: HashSet<String> = models.iter().cloned().collect();
            // Check if current model is a local model (always allowed) or in project set
            let is_local = current_model.starts_with("ollama/")
                || current_model.starts_with("lmstudio/")
                || current_model.starts_with("local/");
            if !is_local && !model_set.contains(&current_model) {
                let new_model = models[0].clone();
                info!(
                    "Models: active model '{}' not in project set, switching to '{}'",
                    current_model, new_model
                );
                self.chat_service.update(cx, |svc, _cx| {
                    svc.set_model(new_model.clone());
                });
                // Also update config default
                if cx.has_global::<AppConfig>() {
                    let _ = cx.global::<AppConfig>().0.update(|cfg| {
                        cfg.default_model = new_model;
                    });
                }
            }
        }

        cx.notify();
    }

    fn refresh_runtime_integrations_from_config(&mut self, cx: &mut Context<Self>) {
        if !cx.has_global::<AppConfig>() {
            return;
        }

        let config = cx.global::<AppConfig>().0.get();
        cx.set_global(AppOllamaManager(std::sync::Arc::new(
            hive_terminal::local_ai::OllamaManager::new(Some(config.ollama_url.clone())),
        )));

        let hue_client = config
            .hue_bridge_ip
            .as_deref()
            .zip(config.hue_api_key.as_deref())
            .map(|(bridge_ip, api_key)| {
                std::sync::Arc::new(hive_integrations::smart_home::PhilipsHueClient::new(
                    bridge_ip,
                    api_key,
                ))
            });
        cx.set_global(AppHueClient(hue_client));

        self.rewire_mcp_integrations(cx);
    }

    fn rewire_mcp_integrations(&mut self, cx: &mut Context<Self>) {
        if !cx.has_global::<AppMcpServer>() {
            return;
        }

        use hive_agents::integration_tools::IntegrationServices;
        let docs_indexer = if cx.has_global::<AppDocsIndexer>() {
            cx.global::<AppDocsIndexer>().0.clone()
        } else {
            std::sync::Arc::new(
                hive_integrations::docs_indexer::DocsIndexer::new().unwrap_or_else(|e| {
                    warn!("DocsIndexer fallback creation failed: {e}");
                    hive_integrations::docs_indexer::DocsIndexer::empty()
                }),
            )
        };

        let services = IntegrationServices {
            messaging: cx.global::<AppMessaging>().0.clone(),
            project_management: cx.global::<AppProjectManagement>().0.clone(),
            knowledge: cx.global::<AppKnowledge>().0.clone(),
            database: cx.global::<AppIntegrationDb>().0.clone(),
            docker: cx.global::<AppDocker>().0.clone(),
            kubernetes: cx.global::<AppKubernetes>().0.clone(),
            a2a: cx.global::<AppA2aClient>().0.clone(),
            browser: cx.global::<AppBrowser>().0.clone(),
            ollama: cx.global::<AppOllamaManager>().0.clone(),
            hue: cx.global::<AppHueClient>().0.clone(),
            aws: cx.global::<AppAws>().0.clone(),
            azure: cx.global::<AppAzure>().0.clone(),
            gcp: cx.global::<AppGcp>().0.clone(),
            docs_indexer,
        };
        cx.global_mut::<AppMcpServer>().0.wire_integrations(services);
    }

    fn handle_settings_save_from_view(&mut self, cx: &mut Context<Self>) {
        info!("Settings: persisting from SettingsView");

        let snapshot = self.settings_view.read(cx).collect_values(cx);

        if cx.has_global::<AppConfig>() {
            let config_mgr = &cx.global::<AppConfig>().0;

            // Persist non-key fields via update()
            if let Err(e) = config_mgr.update(|cfg| {
                cfg.ollama_url = snapshot.ollama_url.clone();
                cfg.lmstudio_url = snapshot.lmstudio_url.clone();
                cfg.litellm_url = snapshot.litellm_url.clone();
                cfg.local_provider_url = snapshot.custom_url.clone();
                cfg.hue_bridge_ip = snapshot.hue_bridge_ip.clone();
                cfg.default_model = snapshot.default_model.clone();
                cfg.daily_budget_usd = snapshot.daily_budget;
                cfg.monthly_budget_usd = snapshot.monthly_budget;
                cfg.privacy_mode = snapshot.privacy_mode;
                cfg.auto_routing = snapshot.auto_routing;
                cfg.speculative_decoding = snapshot.speculative_decoding;
                cfg.speculative_show_metrics = snapshot.speculative_show_metrics;
                cfg.auto_update = snapshot.auto_update;
                cfg.notifications_enabled = snapshot.notifications_enabled;
                cfg.tts_enabled = snapshot.tts_enabled;
                cfg.tts_auto_speak = snapshot.tts_auto_speak;
                cfg.clawdtalk_enabled = snapshot.clawdtalk_enabled;
                // Knowledge base
                cfg.obsidian_vault_path = snapshot.obsidian_vault_path.clone();
                // OAuth client IDs
                cfg.google_oauth_client_id = snapshot.google_oauth_client_id.clone();
                cfg.microsoft_oauth_client_id = snapshot.microsoft_oauth_client_id.clone();
                cfg.github_oauth_client_id = snapshot.github_oauth_client_id.clone();
                cfg.slack_oauth_client_id = snapshot.slack_oauth_client_id.clone();
                cfg.discord_oauth_client_id = snapshot.discord_oauth_client_id.clone();
                cfg.telegram_oauth_client_id = snapshot.telegram_oauth_client_id.clone();
            }) {
                warn!("Settings: failed to save config: {e}");
            }

            // Persist API keys only when user entered a new value
            let key_pairs: &[(&str, &Option<String>)] = &[
                ("anthropic", &snapshot.anthropic_key),
                ("openai", &snapshot.openai_key),
                ("openrouter", &snapshot.openrouter_key),
                ("google", &snapshot.google_key),
                ("groq", &snapshot.groq_key),
                ("xai", &snapshot.xai_key),
                ("huggingface", &snapshot.huggingface_key),
                ("litellm", &snapshot.litellm_key),
                ("elevenlabs", &snapshot.elevenlabs_key),
                ("telnyx", &snapshot.telnyx_key),
                ("hue", &snapshot.hue_api_key),
                ("notion", &snapshot.notion_key),
            ];
            for (provider, key) in key_pairs {
                if let Some(k) = key
                    && let Err(e) = config_mgr.set_api_key(provider, Some(k.clone()))
                {
                    warn!("Settings: failed to save {provider} API key: {e}");
                }
            }

            // Sync status bar with potentially changed model/privacy
            self.status_bar.current_model = if snapshot.default_model.is_empty() {
                "Select Model".to_string()
            } else {
                snapshot.default_model
            };
            self.status_bar.privacy_mode = snapshot.privacy_mode;
        }

        // Update live TTS service config so toggle changes take effect immediately.
        if cx.has_global::<AppTts>() {
            cx.global::<AppTts>().0.update_config(|cfg| {
                cfg.enabled = snapshot.tts_enabled;
                cfg.auto_speak = snapshot.tts_auto_speak;
            });
        }

        // Rebuild knowledge hub so changed vault paths and Notion keys take effect.
        self.rebuild_knowledge_hub(cx);

        // Rebuild shared runtime integrations so UI and MCP use the same
        // up-to-date Ollama and Hue service instances after every save.
        self.refresh_runtime_integrations_from_config(cx);

        // Re-push API keys to the models browser so new/changed keys take effect
        // immediately without requiring the user to switch away and back.
        self.push_keys_to_models_browser(cx);

        cx.notify();
    }

    // -- Knowledge hub rebuild ------------------------------------------------

    /// Rebuild the KnowledgeHub when knowledge base settings change.
    /// Replaces the `AppKnowledge` global with a newly constructed hub.
    fn rebuild_knowledge_hub(&mut self, cx: &mut Context<Self>) {
        if !cx.has_global::<AppConfig>() {
            return;
        }
        let config = cx.global::<AppConfig>().0.get();

        let mut knowledge_hub = hive_integrations::knowledge::KnowledgeHub::new();

        // Register Obsidian provider if vault path is configured.
        if let Some(ref vault_path) = config.obsidian_vault_path {
            if !vault_path.is_empty() {
                let mut obsidian =
                    hive_integrations::knowledge::ObsidianProvider::new(vault_path);
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build();
                match rt {
                    Ok(rt) => match rt.block_on(obsidian.index_vault()) {
                        Ok(count) => {
                            info!(
                                "Knowledge hub: Obsidian vault re-indexed ({count} pages)"
                            );
                        }
                        Err(e) => {
                            warn!("Knowledge hub: Obsidian re-indexing failed: {e}");
                        }
                    },
                    Err(e) => {
                        warn!("Knowledge hub: runtime creation failed: {e}");
                    }
                }
                knowledge_hub.register_provider(Box::new(obsidian));
            }
        }

        // Register Notion provider if API key is configured.
        if let Some(ref notion_key) = config.notion_api_key {
            if !notion_key.is_empty() {
                match hive_integrations::knowledge::NotionClient::new(notion_key) {
                    Ok(notion) => {
                        info!("Knowledge hub: Notion reconnected");
                        knowledge_hub.register_provider(Box::new(notion));
                    }
                    Err(e) => {
                        warn!("Knowledge hub: Notion init failed: {e}");
                    }
                }
            }
        }

        let provider_count = knowledge_hub.provider_count();
        let knowledge = std::sync::Arc::new(knowledge_hub);
        cx.set_global(AppKnowledge(knowledge));
        info!("Knowledge hub rebuilt ({provider_count} providers)");
    }

    // -- Monitor panel handlers ----------------------------------------------

    fn handle_monitor_refresh(
        &mut self,
        _action: &MonitorRefresh,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        info!("Monitor: refresh");
        self.refresh_monitor_data(cx);
        cx.notify();
    }

    // NOTE: refresh_monitor_data is defined earlier in this impl block (near
    // line 705) with the full real-metrics implementation.  The handler above
    // at `handle_monitor_refresh` calls it via `self.refresh_monitor_data(cx)`.

    /// Read system resource metrics (CPU, memory, disk) using macOS-friendly
    /// commands and stdlib APIs.
    fn gather_system_resources(&self) -> SystemResources {
        let mut res = SystemResources {
            cpu_percent: 0.0,
            memory_used: 0,
            memory_total: 0,
            disk_used: 0,
            disk_total: 0,
        };

        // Total physical memory via sysctl (macOS).
        if let Ok(output) = std::process::Command::new("sysctl")
            .args(["-n", "hw.memsize"])
            .output()
            && output.status.success()
        {
            let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
            res.memory_total = s.parse::<u64>().unwrap_or(0);
        }

        // Process memory (resident set size in KB) via ps.
        if let Ok(output) = std::process::Command::new("ps")
            .args(["-o", "rss=", "-p", &std::process::id().to_string()])
            .output()
            && output.status.success()
        {
            let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
            // ps reports RSS in kilobytes.
            res.memory_used = s.parse::<u64>().unwrap_or(0) * 1024;
        }

        // CPU usage: use `ps -o %cpu=` for this process as a quick estimate.
        if let Ok(output) = std::process::Command::new("ps")
            .args(["-o", "%cpu=", "-p", &std::process::id().to_string()])
            .output()
            && output.status.success()
        {
            let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
            res.cpu_percent = s.parse::<f64>().unwrap_or(0.0);
        }

        // Disk usage for the project directory via df.
        if let Ok(output) = std::process::Command::new("df")
            .args(["-k", &self.current_project_root.to_string_lossy()])
            .output()
            && output.status.success()
        {
            let text = String::from_utf8_lossy(&output.stdout);
            // Second line of df output contains the numbers.
            if let Some(data_line) = text.lines().nth(1) {
                let cols: Vec<&str> = data_line.split_whitespace().collect();
                // df -k columns: Filesystem 1K-blocks Used Available Capacity ...
                if cols.len() >= 4 {
                    let total_kb = cols[1].parse::<u64>().unwrap_or(0);
                    let used_kb = cols[2].parse::<u64>().unwrap_or(0);
                    res.disk_total = total_kb * 1024;
                    res.disk_used = used_kb * 1024;
                }
            }
        }

        res
    }

    // -- Workflow Builder run handler -----------------------------------------

    /// Execute a workflow from the visual workflow builder canvas.
    fn handle_workflow_builder_run(
        &mut self,
        workflow_id: String,
        cx: &mut Context<Self>,
    ) {
        // Convert the canvas to an executable workflow.
        let workflow = self.workflow_builder_view.read(cx).to_executable_workflow();

        if workflow.steps.is_empty() {
            warn!("WorkflowBuilder: no executable steps in workflow '{}'", workflow_id);
            if cx.has_global::<AppNotifications>() {
                cx.global_mut::<AppNotifications>().0.push(
                    AppNotification::new(
                        NotificationType::Warning,
                        format!("Workflow '{}' has no executable steps. Add Action nodes to the canvas.", workflow.name),
                    )
                    .with_title("Workflow Empty"),
                );
            }
            return;
        }

        info!(
            "WorkflowBuilder: running '{}' with {} step(s)",
            workflow.name,
            workflow.steps.len()
        );

        if cx.has_global::<AppNotifications>() {
            cx.global_mut::<AppNotifications>().0.push(
                AppNotification::new(
                    NotificationType::Info,
                    format!(
                        "Running workflow '{}' ({} step(s))",
                        workflow.name,
                        workflow.steps.len()
                    ),
                )
                .with_title("Workflow Started"),
            );
        }

        let working_dir = self
            .current_project_root
            .clone()
            .canonicalize()
            .unwrap_or_else(|_| self.current_project_root.clone());
        let workflow_for_thread = workflow.clone();
        let run_result = std::sync::Arc::new(std::sync::Mutex::new(None));
        let run_result_for_thread = std::sync::Arc::clone(&run_result);

        // Execute on a background OS thread (tokio runtime inside).
        std::thread::spawn(move || {
            let result =
                hive_agents::automation::AutomationService::execute_workflow_blocking(
                    &workflow_for_thread,
                    working_dir,
                );
            *run_result_for_thread.lock().unwrap_or_else(|e| e.into_inner()) = Some(result);
        });

        let run_result_for_ui = std::sync::Arc::clone(&run_result);
        let workflow_name = workflow.name.clone();

        cx.spawn(async move |this, app: &mut AsyncApp| {
            loop {
                if let Some(result) = run_result_for_ui.lock().unwrap_or_else(|e| e.into_inner()).take() {
                    let _ = this.update(app, |this, cx| {
                        match result {
                            Ok(run) => {
                                if cx.has_global::<AppAutomation>() {
                                    let _ = cx.global_mut::<AppAutomation>().0.record_run(
                                        &run.workflow_id,
                                        run.success,
                                        run.steps_completed,
                                        run.error.clone(),
                                    );
                                }

                                if cx.has_global::<AppNotifications>() {
                                    let (notif_type, title) = if run.success {
                                        (NotificationType::Success, "Workflow Complete")
                                    } else {
                                        (NotificationType::Error, "Workflow Failed")
                                    };
                                    let msg = if run.success {
                                        format!(
                                            "Workflow '{}' completed ({} steps)",
                                            workflow_name, run.steps_completed
                                        )
                                    } else {
                                        format!(
                                            "Workflow '{}' failed after {} step(s): {}",
                                            workflow_name,
                                            run.steps_completed,
                                            run.error.as_deref().unwrap_or("unknown error")
                                        )
                                    };
                                    cx.global_mut::<AppNotifications>().0.push(
                                        AppNotification::new(notif_type, msg).with_title(title),
                                    );
                                }
                            }
                            Err(e) => {
                                warn!("WorkflowBuilder: run error: {e}");
                                if cx.has_global::<AppNotifications>() {
                                    cx.global_mut::<AppNotifications>().0.push(
                                        AppNotification::new(
                                            NotificationType::Error,
                                            format!("Workflow run failed: {e}"),
                                        )
                                        .with_title("Workflow Run Failed"),
                                    );
                                }
                            }
                        }

                        this.refresh_agents_data(cx);
                        cx.notify();
                    });
                    break;
                }

                app.background_executor()
                    .timer(std::time::Duration::from_millis(120))
                    .await;
            }
        })
        .detach();
    }

    // -- Channel AI agent response handler ------------------------------------

    /// Trigger AI agent responses for a channel message. For each assigned
    /// agent, we build a ChatRequest with the persona system prompt, stream
    /// the response, and append it to the channel.
    fn handle_channel_agent_responses(
        &mut self,
        channel_id: String,
        _user_message: String,
        assigned_agents: Vec<String>,
        cx: &mut Context<Self>,
    ) {
        if assigned_agents.is_empty() {
            return;
        }

        // Determine which model to use (current chat model).
        let model = self.chat_service.read(cx).current_model().to_string();
        if model.is_empty() || model == "Select Model" {
            warn!("Channels: no model selected, cannot trigger agent responses");
            return;
        }

        // Build context messages: include recent channel history + user message.
        let mut context_messages = Vec::new();

        // Load recent messages from the channel store for context
        if cx.has_global::<AppChannels>() {
            let store = &cx.global::<AppChannels>().0;
            if let Some(channel) = store.get_channel(&channel_id) {
                // Take last 10 messages as context
                let recent = channel.messages.iter().rev().take(10).rev();
                for msg in recent {
                    let role = match &msg.author {
                        hive_core::channels::MessageAuthor::User => hive_ai::types::MessageRole::User,
                        hive_core::channels::MessageAuthor::Agent { .. } => hive_ai::types::MessageRole::Assistant,
                        hive_core::channels::MessageAuthor::System => hive_ai::types::MessageRole::System,
                    };
                    context_messages.push(hive_ai::types::ChatMessage {
                        role,
                        content: msg.content.clone(),
                        timestamp: msg.timestamp,
                        tool_calls: None,
                        tool_call_id: None,
                    });
                }
            }
        }

        // Mark streaming state in the view for the first agent
        if let Some(first_agent) = assigned_agents.first() {
            self.channels_view.update(cx, |view, cx| {
                view.set_streaming(first_agent, "", cx);
            });
        }

        // For each assigned agent, spawn a streaming task
        for agent_name in assigned_agents {
            let persona = if cx.has_global::<AppPersonas>() {
                cx.global::<AppPersonas>().0.find_by_name(&agent_name).cloned()
            } else {
                None
            };

            let system_prompt = persona
                .as_ref()
                .map(|p| format!(
                    "You are {} in an AI agent channel. Respond concisely and stay in character.\n\n{}",
                    p.name, p.system_prompt
                ));

            // Prepare the stream setup
            let stream_setup: Option<(Arc<dyn AiProvider>, ChatRequest)> =
                if cx.has_global::<AppAiService>() {
                    cx.global::<AppAiService>().0.prepare_stream(
                        context_messages.clone(),
                        &model,
                        system_prompt,
                        None,
                    )
                } else {
                    None
                };

            let Some((provider, request)) = stream_setup else {
                warn!("Channels: no provider available for agent '{agent_name}'");
                continue;
            };

            let channels_view = self.channels_view.downgrade();
            let channel_id_clone = channel_id.clone();
            let agent_name_clone = agent_name.clone();
            let model_clone = model.clone();

            cx.spawn(async move |_this, app: &mut AsyncApp| {
                match provider.stream_chat(&request).await {
                    Ok(mut rx) => {
                        let mut accumulated = String::new();
                        while let Some(chunk) = rx.recv().await {
                            accumulated.push_str(&chunk.content);

                            // Update streaming display
                            let content = accumulated.clone();
                            let agent = agent_name_clone.clone();
                            let _ = channels_view.update(app, |view, cx| {
                                view.set_streaming(&agent, &content, cx);
                            });

                            if chunk.done {
                                break;
                            }
                        }

                        // Finalize: add the completed message to the channel store
                        let final_content = accumulated.clone();
                        let agent = agent_name_clone.clone();
                        let ch_id = channel_id_clone.clone();
                        let model_str = model_clone.clone();

                        let _ = app.update(|cx| {
                            // Add to channel store
                            if cx.has_global::<AppChannels>() {
                                let msg = hive_core::channels::ChannelMessage {
                                    id: uuid::Uuid::new_v4().to_string(),
                                    author: hive_core::channels::MessageAuthor::Agent {
                                        persona: agent.clone(),
                                    },
                                    content: final_content.clone(),
                                    timestamp: chrono::Utc::now(),
                                    thread_id: None,
                                    model: Some(model_str),
                                    cost: None,
                                };
                                cx.global_mut::<AppChannels>().0.add_message(&ch_id, msg.clone());

                                // Update the view
                                let _ = channels_view.update(cx, |view, cx| {
                                    view.finish_streaming(cx);
                                    view.append_message(&msg, cx);
                                });
                            }
                        });
                    }
                    Err(e) => {
                        error!("Channels: stream error for agent '{}': {e}", agent_name_clone);
                        let _ = channels_view.update(app, |view, cx| {
                            view.finish_streaming(cx);
                        });
                    }
                }
            })
            .detach();
        }

    }

    // -- Connected Accounts OAuth flow ----------------------------------------

    /// Handle the AccountConnectPlatform action dispatched from Settings.
    fn handle_account_connect_platform(
        &mut self,
        action: &AccountConnectPlatform,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let platform_str = action.platform.clone();
        let Some(platform) = hive_core::config::AccountPlatform::parse_platform(&platform_str) else {
            warn!("OAuth: unknown platform '{platform_str}'");
            return;
        };

        info!("OAuth: initiating connect for {platform_str}");

        // Build OAuthConfig for the platform, reading client_id from config
        let cfg = if cx.has_global::<AppConfig>() {
            cx.global::<AppConfig>().0.get()
        } else {
            hive_core::config::HiveConfig::default()
        };

        let oauth_config = Self::oauth_config_for_platform(platform, &cfg);

        if oauth_config.client_id.is_empty() {
            warn!("OAuth: no client_id configured for {platform_str}. Please set it in Settings → Connected Accounts.");
            if cx.has_global::<AppNotifications>() {
                cx.global_mut::<AppNotifications>().0.push(
                    AppNotification::new(
                        NotificationType::Warning,
                        format!("No OAuth Client ID configured for {platform_str}. Go to Settings → Connected Accounts to set it up."),
                    )
                    .with_title("OAuth Setup Required"),
                );
            }
            return;
        }

        let oauth_client = hive_integrations::OAuthClient::new(oauth_config);
        let (auth_url, _state) = oauth_client.authorization_url();

        // Open the authorization URL in the default browser
        if let Err(e) = Self::open_url_in_browser(&auth_url) {
            error!("OAuth: failed to open browser: {e}");
            if cx.has_global::<AppNotifications>() {
                cx.global_mut::<AppNotifications>().0.push(
                    AppNotification::new(
                        NotificationType::Error,
                        format!("Failed to open browser for {platform_str} authentication: {e}"),
                    )
                    .with_title("OAuth Error"),
                );
            }
            return;
        }

        if cx.has_global::<AppNotifications>() {
            cx.global_mut::<AppNotifications>().0.push(
                AppNotification::new(
                    NotificationType::Info,
                    format!(
                        "Opening browser for {platform_str} authentication. \
                         Complete the sign-in flow and paste the authorization code."
                    ),
                )
                .with_title("OAuth: Browser Opened"),
            );
        }

        // Spawn a background thread to start a minimal localhost callback server
        // that waits for the OAuth redirect with the authorization code.
        let platform_for_thread = platform;
        let platform_label = platform_str.clone();
        let result_flag = std::sync::Arc::new(std::sync::Mutex::new(
            None::<Result<hive_integrations::OAuthToken, String>>,
        ));
        let result_for_thread = std::sync::Arc::clone(&result_flag);

        std::thread::spawn(move || {
            // Start a minimal HTTP server on localhost:8742 to catch the redirect
            let listener = match std::net::TcpListener::bind("127.0.0.1:8742") {
                Ok(l) => l,
                Err(e) => {
                    *result_for_thread.lock().unwrap_or_else(|e| e.into_inner()) =
                        Some(Err(format!("Failed to start callback server: {e}")));
                    return;
                }
            };

            // Set a timeout so we don't block forever
            let _ = listener.set_nonblocking(false);

            // Wait for the callback (blocks up to 5 minutes)
            let timeout = std::time::Duration::from_secs(300);
            let start = std::time::Instant::now();
            loop {
                if start.elapsed() > timeout {
                    *result_for_thread.lock().unwrap_or_else(|e| e.into_inner()) =
                        Some(Err("OAuth callback timed out after 5 minutes".to_string()));
                    return;
                }

                match listener.accept() {
                    Ok((mut stream, _addr)) => {
                        use std::io::{Read, Write};
                        let mut buf = [0u8; 4096];
                        let n = stream.read(&mut buf).unwrap_or(0);
                        let request_str = String::from_utf8_lossy(&buf[..n]);

                        // Extract the code parameter from the GET request
                        if let Some(code) = Self::extract_oauth_code(&request_str) {
                            // Send success response
                            let response = "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\n\
                                <html><body><h1>Authorization successful!</h1>\
                                <p>You can close this tab and return to Hive.</p></body></html>";
                            let _ = stream.write_all(response.as_bytes());
                            let _ = stream.flush();

                            // Exchange code for token
                            let rt = tokio::runtime::Builder::new_current_thread()
                                .enable_all()
                                .build();
                            match rt {
                                Ok(rt) => {
                                    let exchange_result =
                                        rt.block_on(oauth_client.exchange_code(&code));
                                    match exchange_result {
                                        Ok(token) => {
                                            *result_for_thread.lock().unwrap_or_else(|e| e.into_inner()) = Some(Ok(token));
                                        }
                                        Err(e) => {
                                            *result_for_thread.lock().unwrap_or_else(|e| e.into_inner()) =
                                                Some(Err(format!("Token exchange failed: {e}")));
                                        }
                                    }
                                }
                                Err(e) => {
                                    *result_for_thread.lock().unwrap_or_else(|e| e.into_inner()) = Some(Err(format!(
                                        "Failed to create runtime for token exchange: {e}"
                                    )));
                                }
                            }
                            return;
                        }

                        // Not the callback we're looking for, send 404
                        let response = "HTTP/1.1 404 Not Found\r\n\r\nNot found";
                        let _ = stream.write_all(response.as_bytes());
                    }
                    Err(_) => {
                        std::thread::sleep(std::time::Duration::from_millis(100));
                    }
                }
            }
        });

        // Poll for the result from the UI thread
        let result_for_ui = std::sync::Arc::clone(&result_flag);
        let platform_for_ui = platform_for_thread;
        let platform_label_ui = platform_label;

        cx.spawn(async move |_this, app: &mut AsyncApp| {
            loop {
                if let Some(result) = result_for_ui.lock().unwrap_or_else(|e| e.into_inner()).take() {
                    let _ = app.update(|cx| match result {
                        Ok(token) => {
                            info!("OAuth: successfully connected {platform_label_ui}");

                            // Store the token
                            if cx.has_global::<AppConfig>() {
                                let token_data = hive_core::config::OAuthTokenData {
                                    access_token: token.access_token.clone(),
                                    refresh_token: token.refresh_token.clone(),
                                    expires_at: token.expires_at.map(|t| t.to_rfc3339()),
                                };
                                let _ = cx
                                    .global::<AppConfig>()
                                    .0
                                    .set_oauth_token(platform_for_ui, &token_data);

                                // Add connected account entry
                                let account = hive_core::config::ConnectedAccount {
                                    platform: platform_for_ui,
                                    account_name: platform_label_ui.clone(),
                                    account_id: "oauth".to_string(),
                                    scopes: Vec::new(),
                                    connected_at: chrono::Utc::now().to_rfc3339(),
                                    last_synced: None,
                                    settings: hive_core::config::AccountSettings::default(),
                                };
                                let _ = cx
                                    .global::<AppConfig>()
                                    .0
                                    .add_connected_account(account);
                            }

                            // Inject the token into the assistant service so
                            // email/calendar providers can use it immediately.
                            if cx.has_global::<AppAssistant>() {
                                let access = token.access_token.clone();
                                let assistant = &mut cx.global_mut::<AppAssistant>().0;
                                match platform_for_ui {
                                    hive_core::config::AccountPlatform::Google => {
                                        assistant.set_gmail_token(access.clone());
                                        assistant.set_google_calendar_token(access);
                                    }
                                    hive_core::config::AccountPlatform::Microsoft => {
                                        assistant.set_outlook_token(access.clone());
                                        assistant.set_outlook_calendar_token(access);
                                    }
                                    _ => {}
                                }
                                info!("OAuth: injected token into assistant service for {platform_label_ui}");
                            }

                            if cx.has_global::<AppNotifications>() {
                                cx.global_mut::<AppNotifications>().0.push(
                                    AppNotification::new(
                                        NotificationType::Success,
                                        format!(
                                            "{platform_label_ui} account connected successfully!"
                                        ),
                                    )
                                    .with_title("Account Connected"),
                                );
                            }
                        }
                        Err(e) => {
                            error!("OAuth: connection failed for {platform_label_ui}: {e}");
                            if cx.has_global::<AppNotifications>() {
                                cx.global_mut::<AppNotifications>().0.push(
                                    AppNotification::new(
                                        NotificationType::Error,
                                        format!("{platform_label_ui} connection failed: {e}"),
                                    )
                                    .with_title("OAuth Error"),
                                );
                            }
                        }
                    });
                    break;
                }

                app.background_executor()
                    .timer(std::time::Duration::from_millis(200))
                    .await;
            }
        })
        .detach();
    }

    /// Extract the `code` parameter from an HTTP GET request line.
    fn extract_oauth_code(request: &str) -> Option<String> {
        let first_line = request.lines().next()?;
        let path = first_line.split_whitespace().nth(1)?;
        let query = path.split('?').nth(1)?;
        for param in query.split('&') {
            if let Some(value) = param.strip_prefix("code=") {
                // Simple URL decode: replace %XX with actual chars
                let decoded = value
                    .replace("%3D", "=")
                    .replace("%2F", "/")
                    .replace("%2B", "+")
                    .replace("%20", " ")
                    .replace('+', " ");
                return Some(decoded);
            }
        }
        None
    }

    /// Open a URL in the default system browser.
    fn open_url_in_browser(url: &str) -> Result<(), String> {
        #[cfg(target_os = "macos")]
        {
            std::process::Command::new("open")
                .arg(url)
                .spawn()
                .map_err(|e| format!("Failed to open browser: {e}"))?;
        }
        #[cfg(target_os = "windows")]
        {
            std::process::Command::new("cmd")
                .args(["/C", "start", url])
                .spawn()
                .map_err(|e| format!("Failed to open browser: {e}"))?;
        }
        #[cfg(target_os = "linux")]
        {
            std::process::Command::new("xdg-open")
                .arg(url)
                .spawn()
                .map_err(|e| format!("Failed to open browser: {e}"))?;
        }
        Ok(())
    }

    /// Build an OAuthConfig for the given platform with standard OAuth endpoints.
    /// Reads the client_id from the user's HiveConfig.
    fn oauth_config_for_platform(
        platform: hive_core::config::AccountPlatform,
        cfg: &hive_core::config::HiveConfig,
    ) -> hive_integrations::OAuthConfig {
        use hive_core::config::AccountPlatform;
        let client_id = platform
            .client_id_from_config(cfg)
            .unwrap_or_default();
        match platform {
            AccountPlatform::Google => hive_integrations::OAuthConfig {
                client_id: client_id.clone(),
                client_secret: None,
                auth_url: "https://accounts.google.com/o/oauth2/v2/auth".to_string(),
                token_url: "https://oauth2.googleapis.com/token".to_string(),
                redirect_uri: "http://127.0.0.1:8742/callback".to_string(),
                scopes: vec![
                    "https://www.googleapis.com/auth/gmail.readonly".to_string(),
                    "https://www.googleapis.com/auth/calendar.readonly".to_string(),
                ],
            },
            AccountPlatform::Microsoft => hive_integrations::OAuthConfig {
                client_id: client_id.clone(),
                client_secret: None,
                auth_url: "https://login.microsoftonline.com/common/oauth2/v2.0/authorize"
                    .to_string(),
                token_url: "https://login.microsoftonline.com/common/oauth2/v2.0/token"
                    .to_string(),
                redirect_uri: "http://127.0.0.1:8742/callback".to_string(),
                scopes: vec![
                    "Mail.Read".to_string(),
                    "Calendars.Read".to_string(),
                ],
            },
            AccountPlatform::GitHub => hive_integrations::OAuthConfig {
                client_id: client_id.clone(),
                client_secret: None,
                auth_url: "https://github.com/login/oauth/authorize".to_string(),
                token_url: "https://github.com/login/oauth/access_token".to_string(),
                redirect_uri: "http://127.0.0.1:8742/callback".to_string(),
                scopes: vec!["repo".to_string(), "read:user".to_string()],
            },
            AccountPlatform::Slack => hive_integrations::OAuthConfig {
                client_id: client_id.clone(),
                client_secret: None,
                auth_url: "https://slack.com/oauth/v2/authorize".to_string(),
                token_url: "https://slack.com/api/oauth.v2.access".to_string(),
                redirect_uri: "http://127.0.0.1:8742/callback".to_string(),
                scopes: vec![
                    "channels:read".to_string(),
                    "chat:write".to_string(),
                ],
            },
            AccountPlatform::Discord => hive_integrations::OAuthConfig {
                client_id: client_id.clone(),
                client_secret: None,
                auth_url: "https://discord.com/api/oauth2/authorize".to_string(),
                token_url: "https://discord.com/api/oauth2/token".to_string(),
                redirect_uri: "http://127.0.0.1:8742/callback".to_string(),
                scopes: vec!["identify".to_string(), "guilds".to_string()],
            },
            AccountPlatform::Telegram => hive_integrations::OAuthConfig {
                client_id: client_id.clone(),
                client_secret: None,
                auth_url: "https://oauth.telegram.org/auth".to_string(),
                token_url: "https://oauth.telegram.org/auth".to_string(),
                redirect_uri: "http://127.0.0.1:8742/callback".to_string(),
                scopes: Vec::new(),
            },
        }
    }

    // -- Voice intent handler ------------------------------------------------

    /// Handle voice text processing — classify intent and dispatch appropriate action.
    fn handle_voice_process_text(
        &mut self,
        action: &VoiceProcessText,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !cx.has_global::<AppVoiceAssistant>() {
            return;
        }
        let command = {
            let va = cx.global::<AppVoiceAssistant>();
            match va.0.lock() {
                Ok(mut voice) => voice.process_text(&action.text),
                Err(_) => return,
            }
        };

        info!(
            "Voice command: intent={:?}, confidence={:.2}",
            command.intent, command.confidence
        );

        // Map voice intents to existing panel-switch actions.
        use hive_agents::VoiceIntent;
        match command.intent {
            VoiceIntent::OpenPanel => {
                // Try to detect which panel from the text.
                let text_lower = action.text.to_lowercase();
                if text_lower.contains("file") {
                    window.dispatch_action(Box::new(SwitchToFiles), cx);
                } else if text_lower.contains("terminal") || text_lower.contains("shell") {
                    window.dispatch_action(Box::new(SwitchToTerminal), cx);
                } else if text_lower.contains("setting") {
                    window.dispatch_action(Box::new(SwitchToSettings), cx);
                } else if text_lower.contains("model") {
                    window.dispatch_action(Box::new(SwitchToModels), cx);
                } else if text_lower.contains("chat") {
                    window.dispatch_action(Box::new(SwitchToChat), cx);
                } else if text_lower.contains("history") {
                    window.dispatch_action(Box::new(SwitchToHistory), cx);
                } else if text_lower.contains("network") {
                    window.dispatch_action(Box::new(SwitchToNetwork), cx);
                } else if text_lower.contains("agent") {
                    window.dispatch_action(Box::new(SwitchToAgents), cx);
                }
            }
            VoiceIntent::SearchFiles => {
                window.dispatch_action(Box::new(SwitchToFiles), cx);
            }
            VoiceIntent::RunCommand => {
                window.dispatch_action(Box::new(SwitchToTerminal), cx);
            }
            VoiceIntent::SendMessage | VoiceIntent::CreateTask => {
                window.dispatch_action(Box::new(SwitchToChat), cx);
            }
            _ => {
                // Unknown or unhandled intent — fall through silently.
                debug!("Voice: unhandled intent {:?}, ignoring", command.intent);
            }
        }
    }

    // -- Auto-update handler -------------------------------------------------

    fn handle_trigger_app_update(
        &mut self,
        _action: &TriggerAppUpdate,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !cx.has_global::<AppUpdater>() {
            return;
        }

        let updater = cx.global::<AppUpdater>().0.clone();
        if updater.is_updating() {
            info!("Update already in progress");
            return;
        }

        info!("User triggered app update");

        // Push a notification that the update is downloading.
        if cx.has_global::<AppNotifications>() {
            cx.global_mut::<AppNotifications>().0.push(
                AppNotification::new(
                    NotificationType::Info,
                    "Downloading update... The app will need to restart when complete.",
                )
                .with_title("Updating Hive"),
            );
        }

        // Run the blocking install on a background OS thread.
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let result = updater.install_update();
            let _ = tx.send(result);
        });

        // Poll for the result on the main thread.
        cx.spawn(async move |_entity, app: &mut AsyncApp| {
            loop {
                match rx.try_recv() {
                    Ok(result) => {
                        let _ = app.update(|cx| {
                            match result {
                                Ok(_path) => {
                                    if cx.has_global::<AppNotifications>() {
                                        cx.global_mut::<AppNotifications>().0.push(
                                            AppNotification::new(
                                                NotificationType::Info,
                                                "Update installed! Please restart Hive to use the new version.",
                                            )
                                            .with_title("Update Complete"),
                                        );
                                    }
                                    info!("Update installed successfully — restart needed");
                                }
                                Err(e) => {
                                    error!("Update installation failed: {e}");
                                    if cx.has_global::<AppNotifications>() {
                                        cx.global_mut::<AppNotifications>().0.push(
                                            AppNotification::new(
                                                NotificationType::Error,
                                                format!("Update failed: {e}. You can update manually with: brew upgrade hive"),
                                            )
                                            .with_title("Update Failed"),
                                        );
                                    }
                                }
                            }
                        });
                        return;
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => {
                        app.background_executor()
                            .timer(std::time::Duration::from_millis(500))
                            .await;
                    }
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => return,
                }
            }
        })
        .detach();
    }

    // -- Ollama model management handlers ------------------------------------

    /// Handle Ollama model pull request — download a model asynchronously.
    fn handle_ollama_pull_model(
        &mut self,
        action: &OllamaPullModel,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !cx.has_global::<AppOllamaManager>() {
            return;
        }
        let ollama = cx.global::<AppOllamaManager>().0.clone();
        let model = action.model.clone();
        info!("Ollama: pulling model '{model}'");

        std::thread::Builder::new()
            .name("ollama-pull".into())
            .spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build();
                let Ok(rt) = rt else { return };
                rt.block_on(async {
                    let (tx, mut rx) = tokio::sync::mpsc::channel(64);
                    let model_clone = model.clone();
                    let pull_task = tokio::spawn(async move {
                        ollama.pull_model(&model_clone, tx).await
                    });
                    // Log progress updates.
                    while let Some(update) = rx.recv().await {
                        tracing::debug!("Ollama pull progress: {update:?}");
                    }
                    match pull_task.await {
                        Ok(Ok(())) => info!("Ollama: model '{model}' pulled successfully"),
                        Ok(Err(e)) => warn!("Ollama: pull failed for '{model}': {e}"),
                        Err(e) => warn!("Ollama: pull task panicked for '{model}': {e}"),
                    }
                });
            })
            .ok();
    }

    /// Handle Ollama model delete request.
    fn handle_ollama_delete_model(
        &mut self,
        action: &OllamaDeleteModel,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !cx.has_global::<AppOllamaManager>() {
            return;
        }
        let ollama = cx.global::<AppOllamaManager>().0.clone();
        let model = action.model.clone();
        info!("Ollama: deleting model '{model}'");

        std::thread::Builder::new()
            .name("ollama-delete".into())
            .spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build();
                let Ok(rt) = rt else { return };
                rt.block_on(async {
                    match ollama.delete_model(&model).await {
                        Ok(()) => info!("Ollama: model '{model}' deleted successfully"),
                        Err(e) => warn!("Ollama: delete failed for '{model}': {e}"),
                    }
                });
            })
            .ok();
    }
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

        self.sync_status_bar(window, cx);

        // Auto-focus: when nothing is focused, give focus to the chat input on
        // the Chat panel or the workspace root on other panels. This ensures
        // typing goes straight into the input and dispatch_action() still works.
        if window.focused(cx).is_none() {
            if self.sidebar.active_panel == Panel::Chat {
                let fh = self.chat_input.read(cx).input_focus_handle();
                window.focus(&fh);
            } else if self.sidebar.active_panel == Panel::Settings {
                let fh = self.settings_view.read(cx).focus_handle().clone();
                window.focus(&fh);
            } else {
                window.focus(&self.focus_handle);
            }
        }

        // Render the active panel first (may require &mut self for cache updates).
        let active_panel_el = self.render_active_panel(cx);

        let theme = &self.theme;
        let active_panel = self.sidebar.active_panel;
        let chat_input = self.chat_input.clone();

        div()
            .id("workspace-root")
            .track_focus(&self.focus_handle)
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.bg_primary)
            .text_color(theme.text_primary)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _window, cx| {
                if event.keystroke.key == "escape" && this.show_project_dropdown {
                    this.show_project_dropdown = false;
                    cx.notify();
                }
            }))
            // -- Action handlers for keyboard shortcuts -----------------------
            .on_action(cx.listener(Self::handle_new_conversation))
            .on_action(cx.listener(Self::handle_clear_chat))
            .on_action(cx.listener(Self::handle_switch_to_chat))
            .on_action(cx.listener(Self::handle_switch_to_quick_start))
            .on_action(cx.listener(Self::handle_switch_to_history))
            .on_action(cx.listener(Self::handle_switch_to_files))
            .on_action(cx.listener(Self::handle_switch_to_kanban))
            .on_action(cx.listener(Self::handle_switch_to_monitor))
            .on_action(cx.listener(Self::handle_switch_to_activity))
            .on_action(cx.listener(Self::handle_switch_to_logs))
            .on_action(cx.listener(Self::handle_switch_to_costs))
            .on_action(cx.listener(Self::handle_switch_to_review))
            .on_action(cx.listener(Self::handle_switch_to_skills))
            .on_action(cx.listener(Self::handle_switch_to_routing))
            .on_action(cx.listener(Self::handle_switch_to_models))
            .on_action(cx.listener(Self::handle_switch_to_token_launch))
            .on_action(cx.listener(Self::handle_switch_to_specs))
            .on_action(cx.listener(Self::handle_switch_to_agents))
            .on_action(cx.listener(Self::handle_switch_to_workflows))
            .on_action(cx.listener(Self::handle_switch_to_channels))
            .on_action(cx.listener(Self::handle_switch_to_learning))
            .on_action(cx.listener(Self::handle_switch_to_shield))
            .on_action(cx.listener(Self::handle_switch_to_assistant))
            .on_action(cx.listener(Self::handle_switch_to_settings))
            .on_action(cx.listener(Self::handle_switch_to_help))
            .on_action(cx.listener(Self::handle_switch_to_network))
            .on_action(cx.listener(Self::handle_switch_to_terminal))
            .on_action(cx.listener(Self::handle_switch_to_code_map))
            .on_action(cx.listener(Self::handle_switch_to_prompt_library))
            .on_action(cx.listener(Self::handle_prompt_library_save_current))
            .on_action(cx.listener(Self::handle_prompt_library_refresh))
            .on_action(cx.listener(Self::handle_prompt_library_load))
            .on_action(cx.listener(Self::handle_prompt_library_delete))
            .on_action(cx.listener(Self::handle_network_refresh))
            .on_action(cx.listener(Self::handle_open_workspace_directory))
            .on_action(cx.listener(Self::handle_toggle_project_dropdown))
            .on_action(cx.listener(Self::handle_switch_to_workspace_action))
            .on_action(cx.listener(Self::handle_toggle_pin_workspace))
            .on_action(cx.listener(Self::handle_remove_recent_workspace))
            // -- Panel action handlers -----------------------------------
            // Files
            .on_action(cx.listener(Self::handle_files_navigate_back))
            .on_action(cx.listener(Self::handle_files_navigate_to))
            .on_action(cx.listener(Self::handle_files_open_entry))
            .on_action(cx.listener(Self::handle_files_delete_entry))
            .on_action(cx.listener(Self::handle_files_refresh))
            .on_action(cx.listener(Self::handle_files_new_file))
            .on_action(cx.listener(Self::handle_files_new_folder))
            .on_action(cx.listener(Self::handle_files_close_viewer))
            .on_action(cx.listener(Self::handle_files_toggle_check))
            .on_action(cx.listener(Self::handle_files_clear_checked))
            // Apply mode + clipboard
            .on_action(cx.listener(Self::handle_apply_code_block))
            .on_action(cx.listener(Self::handle_apply_all_edits))
            .on_action(cx.listener(Self::handle_copy_to_clipboard))
            .on_action(cx.listener(Self::handle_copy_full_prompt))
            .on_action(cx.listener(Self::handle_export_prompt))
            // History
            .on_action(cx.listener(Self::handle_history_load))
            .on_action(cx.listener(Self::handle_history_delete))
            .on_action(cx.listener(Self::handle_history_refresh))
            .on_action(cx.listener(Self::handle_history_clear_all))
            .on_action(cx.listener(Self::handle_history_clear_all_confirm))
            .on_action(cx.listener(Self::handle_history_clear_all_cancel))
            // Kanban
            .on_action(cx.listener(Self::handle_kanban_add_task))
            // Logs
            .on_action(cx.listener(Self::handle_logs_clear))
            .on_action(cx.listener(Self::handle_logs_set_filter))
            .on_action(cx.listener(Self::handle_logs_toggle_auto_scroll))
            // Terminal
            .on_action(cx.listener(Self::handle_terminal_clear))
            .on_action(cx.listener(Self::handle_terminal_submit))
            .on_action(cx.listener(Self::handle_terminal_kill))
            .on_action(cx.listener(Self::handle_terminal_restart))
            // Tool approval
            .on_action(cx.listener(Self::handle_tool_approve))
            .on_action(cx.listener(Self::handle_tool_reject))
            // Costs
            .on_action(cx.listener(Self::handle_costs_export_csv))
            .on_action(cx.listener(Self::handle_costs_reset_today))
            .on_action(cx.listener(Self::handle_costs_clear_history))
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
            .on_action(cx.listener(Self::handle_skills_refresh))
            .on_action(cx.listener(Self::handle_skills_install))
            .on_action(cx.listener(Self::handle_skills_remove))
            .on_action(cx.listener(Self::handle_skills_toggle))
            .on_action(cx.listener(Self::handle_skills_create))
            .on_action(cx.listener(Self::handle_skills_add_source))
            .on_action(cx.listener(Self::handle_skills_remove_source))
            .on_action(cx.listener(Self::handle_skills_set_tab))
            .on_action(cx.listener(Self::handle_skills_set_search))
            .on_action(cx.listener(Self::handle_skills_set_category))
            .on_action(cx.listener(Self::handle_skills_clear_search))
            // Plugins
            .on_action(cx.listener(Self::handle_plugin_import_open))
            .on_action(cx.listener(Self::handle_plugin_import_cancel))
            .on_action(cx.listener(Self::handle_plugin_import_from_github))
            .on_action(cx.listener(Self::handle_plugin_import_from_url))
            .on_action(cx.listener(Self::handle_plugin_import_from_local))
            .on_action(cx.listener(Self::handle_plugin_import_confirm))
            .on_action(cx.listener(Self::handle_plugin_import_toggle_skill))
            .on_action(cx.listener(Self::handle_plugin_remove))
            .on_action(cx.listener(Self::handle_plugin_update))
            .on_action(cx.listener(Self::handle_plugin_toggle_expand))
            .on_action(cx.listener(Self::handle_plugin_toggle_skill))
            // Routing
            .on_action(cx.listener(Self::handle_routing_add_rule))
            // Token Launch
            .on_action(cx.listener(Self::handle_token_launch_set_step))
            .on_action(cx.listener(Self::handle_token_launch_select_chain))
            .on_action(cx.listener(Self::handle_token_launch_select_wallet))
            .on_action(cx.listener(Self::handle_token_launch_create_wallet))
            .on_action(cx.listener(Self::handle_token_launch_import_wallet))
            .on_action(cx.listener(Self::handle_token_launch_save_rpc_config))
            .on_action(cx.listener(Self::handle_token_launch_reset_rpc_config))
            .on_action(cx.listener(Self::handle_token_launch_deploy))
            // Settings
            .on_action(cx.listener(Self::handle_settings_save))
            .on_action(cx.listener(Self::handle_export_config))
            .on_action(cx.listener(Self::handle_import_config))
            // Quick Start
            .on_action(cx.listener(Self::handle_quick_start_select_template))
            .on_action(cx.listener(Self::handle_quick_start_open_panel))
            .on_action(cx.listener(Self::handle_quick_start_run_project))
            // Theme + context format
            .on_action(cx.listener(Self::handle_theme_changed))
            .on_action(cx.listener(Self::handle_context_format_changed))
            // Monitor
            .on_action(cx.listener(Self::handle_monitor_refresh))
            // Agents
            .on_action(cx.listener(Self::handle_agents_refresh_remote_agents))
            .on_action(cx.listener(Self::handle_agents_reload_workflows))
            .on_action(cx.listener(Self::handle_agents_select_remote_agent))
            .on_action(cx.listener(Self::handle_agents_select_remote_skill))
            .on_action(cx.listener(Self::handle_agents_discover_remote_agent))
            .on_action(cx.listener(Self::handle_agents_run_remote_agent))
            .on_action(cx.listener(Self::handle_agents_run_workflow))
            // Connected Accounts
            .on_action(cx.listener(Self::handle_account_connect_platform))
            // Voice
            .on_action(cx.listener(Self::handle_voice_process_text))
            // Auto-update
            .on_action(cx.listener(Self::handle_trigger_app_update))
            // Ollama model management
            .on_action(cx.listener(Self::handle_ollama_pull_model))
            .on_action(cx.listener(Self::handle_ollama_delete_model))
            // Titlebar
                .child(Titlebar::render(theme, window, &self.current_project_root))
            // Project dropdown backdrop (dismisses on click)
            .when(self.show_project_dropdown, |el| {
                el.child(
                    div()
                        .id("project-dropdown-backdrop")
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full()
                        .on_mouse_down(MouseButton::Left, |_, window, cx| {
                            cx.stop_propagation();
                            window.dispatch_action(Box::new(ToggleProjectDropdown), cx);
                        }),
                )
            })
            // Project dropdown overlay
            .when(self.show_project_dropdown, |el| {
                el.child(self.render_project_dropdown(cx))
            })
            // Main content area: sidebar + panel
            .child(
                div()
                    .flex()
                    .flex_1()
                    .overflow_hidden()
                    // Sidebar
                    .child(self.render_sidebar(cx))
                    // Active panel content
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .overflow_hidden()
                            .child(active_panel_el)
                            // Chat input (only shown on Chat panel)
                            .when(active_panel == Panel::Chat, |el: Div| el.child(chat_input)),
                    ),
            )
            // Status bar
            .child(self.status_bar.render(theme))
    }
}

impl HiveWorkspace {
    fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.theme;
        let active = self.sidebar.active_panel;
        let project = self.project_label();

        div()
            .flex()
            .flex_col()
            .w(px(232.0))
            .h_full()
            .bg(theme.bg_secondary)
            .border_r_1()
            .border_color(theme.border)
            .child(
                div()
                    .px(theme.space_3)
                    .py(theme.space_2)
                    .border_b_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(theme.space_2)
                            .cursor_pointer()
                            .on_mouse_down(MouseButton::Left, |_, window, cx| {
                                cx.stop_propagation();
                                window.dispatch_action(Box::new(OpenWorkspaceDirectory), cx);
                            })
                            .child(
                                div()
                                    .w(px(26.0))
                                    .h(px(26.0))
                                    .rounded(theme.radius_full)
                                    .bg(theme.bg_tertiary)
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(Icon::new(IconName::Folder).size_3p5().text_color(theme.accent_aqua)),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .child(
                                        div()
                                            .text_size(theme.font_size_xs)
                                            .text_color(theme.text_muted)
                                            .child("Workspace"),
                                    )
                                    .child(
                                        div()
                                            .text_size(theme.font_size_sm)
                                            .text_color(theme.text_secondary)
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .max_w(px(150.0))
                                            .overflow_hidden()
                                            .child(project),
                                    ),
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_y_scrollbar()
                    .px(theme.space_2)
                    .py(theme.space_2)
                    .gap(theme.space_2)
                    .child(render_sidebar_section(
                        "Core",
                        &[Panel::QuickStart, Panel::Chat, Panel::History, Panel::Files, Panel::Specs],
                        active,
                        theme,
                        cx,
                    ))
                    .child(render_sidebar_section(
                        "Flow",
                        &[
                            Panel::Agents,
                            Panel::Workflows,
                            Panel::Channels,
                            Panel::Kanban,
                            Panel::Review,
                            Panel::Skills,
                            Panel::Routing,
                            Panel::Models,
                            Panel::Learning,
                        ],
                        active,
                        theme,
                        cx,
                    ))
                    .child(render_sidebar_section(
                        "Observe",
                        &[Panel::Monitor, Panel::Activity, Panel::Logs, Panel::Terminal, Panel::Costs, Panel::Shield, Panel::Network],
                        active,
                        theme,
                        cx,
                    ))
                    .child(render_sidebar_section(
                        "Project",
                        &[Panel::Assistant, Panel::TokenLaunch],
                        active,
                        theme,
                        cx,
                    ))
            .child(render_sidebar_section(
                "System",
                &[Panel::Settings, Panel::Help],
                active,
                theme,
                cx,
            )),
    )
    }

    fn render_project_dropdown(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.theme;
        let current_root = &self.current_project_root;
        let pinned = &self.pinned_workspace_roots;

        let mut children: Vec<AnyElement> = Vec::new();

        // Pinned section
        for path in pinned {
            let is_active = path == current_root;
            let path_str = path.to_string_lossy().to_string();
            let name = Self::project_name_from_path(path);
            children.push(
                self.render_project_row(theme, &name, &path_str, is_active, true, cx)
                    .into_any_element(),
            );
        }

        // Separator if pinned exist
        if !pinned.is_empty() {
            children.push(
                div()
                    .h(px(1.0))
                    .mx(theme.space_2)
                    .my(theme.space_1)
                    .bg(theme.border)
                    .into_any_element(),
            );
        }

        // Recent section (exclude pinned)
        for path in &self.recent_workspace_roots {
            if pinned.contains(path) {
                continue;
            }
            let is_active = path == current_root;
            let path_str = path.to_string_lossy().to_string();
            let name = Self::project_name_from_path(path);
            children.push(
                self.render_project_row(theme, &name, &path_str, is_active, false, cx)
                    .into_any_element(),
            );
        }

        // Bottom separator
        children.push(
            div()
                .h(px(1.0))
                .mx(theme.space_2)
                .my(theme.space_1)
                .bg(theme.border)
                .into_any_element(),
        );

        // "Open folder..." row
        children.push(
            div()
                .id("open-folder-row")
                .flex()
                .flex_row()
                .items_center()
                .gap(theme.space_2)
                .px(theme.space_3)
                .py(theme.space_2)
                .rounded(theme.radius_md)
                .cursor_pointer()
                .hover(|s| s.bg(theme.bg_tertiary))
                .on_mouse_down(MouseButton::Left, |_, window, cx| {
                    cx.stop_propagation();
                    window.dispatch_action(Box::new(OpenWorkspaceDirectory), cx);
                })
                .child(Icon::new(IconName::FolderOpen).small())
                .child(
                    div()
                        .text_size(theme.font_size_xs)
                        .text_color(theme.text_secondary)
                        .child("Open folder..."),
                )
                .into_any_element(),
        );

        // Dropdown container
        div()
            .id("project-dropdown")
            .occlude()
            .absolute()
            .top(px(42.0))
            .left(px(120.0))
            .w(px(320.0))
            .max_h(px(400.0))
            .overflow_y_scroll()
            .bg(theme.bg_primary)
            .border_1()
            .border_color(theme.border)
            .rounded(theme.radius_lg)
            .shadow_lg()
            .py(theme.space_1)
            .children(children)
    }

    fn render_project_row(
        &self,
        theme: &HiveTheme,
        name: &str,
        path_str: &str,
        is_active: bool,
        is_pinned: bool,
        _cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let switch_path = path_str.to_string();
        let pin_path = path_str.to_string();

        let text_color = if is_active {
            theme.accent_cyan
        } else {
            theme.text_primary
        };

        let pin_icon_color = if is_pinned {
            theme.accent_cyan
        } else {
            theme.text_muted
        };

        div()
            .id(SharedString::from(format!("project-row-{}", path_str)))
            .flex()
            .flex_row()
            .items_center()
            .gap(theme.space_2)
            .px(theme.space_3)
            .py(theme.space_2)
            .rounded(theme.radius_md)
            .cursor_pointer()
            .hover(|s| s.bg(theme.bg_tertiary))
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                cx.stop_propagation();
                window.dispatch_action(
                    Box::new(SwitchToWorkspace { path: switch_path.clone() }),
                    cx,
                );
            })
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .overflow_hidden()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(theme.space_1)
                            .when(is_active, |el| {
                                el.child(
                                    div()
                                        .w(px(6.0))
                                        .h(px(6.0))
                                        .rounded(theme.radius_full)
                                        .bg(theme.accent_green),
                                )
                            })
                            .child(
                                div()
                                    .text_size(theme.font_size_sm)
                                    .text_color(text_color)
                                    .font_weight(if is_active {
                                        FontWeight::BOLD
                                    } else {
                                        FontWeight::NORMAL
                                    })
                                    .truncate()
                                    .child(name.to_string()),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(theme.text_muted)
                            .truncate()
                            .child(path_str.to_string()),
                    ),
            )
            // Pin toggle button
            .child(
                div()
                    .id(SharedString::from(format!("pin-btn-{}", pin_path)))
                    .flex_shrink_0()
                    .cursor_pointer()
                    .rounded(theme.radius_sm)
                    .p(px(4.0))
                    .hover(|s| s.bg(theme.bg_secondary))
                    .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                        cx.stop_propagation();
                        window.dispatch_action(
                            Box::new(TogglePinWorkspace {
                                path: pin_path.clone(),
                            }),
                            cx,
                        );
                    })
                    .child(
                        Icon::new(IconName::Star)
                            .with_size(px(14.0))
                            .text_color(pin_icon_color),
                    ),
            )
    }

}

fn render_sidebar_section(
    title: &'static str,
    panels: &[Panel],
    active: Panel,
    theme: &HiveTheme,
    cx: &mut Context<HiveWorkspace>,
) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(theme.space_1)
        .child(
            div()
                .px(theme.space_2)
                .pb(px(2.0))
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .font_weight(FontWeight::SEMIBOLD)
                
                .child(title),
        )
        .children(
            panels
                .iter()
                .copied()
                .map(|panel| render_sidebar_item(panel, active, theme, cx)),
        )
        .into_any_element()
}

fn render_sidebar_item(
    panel: Panel,
    active: Panel,
    theme: &HiveTheme,
    cx: &mut Context<HiveWorkspace>,
) -> AnyElement {
    let is_active = panel == active;
    let bg = if is_active {
        theme.bg_tertiary
    } else {
        Hsla::transparent_black()
    };
    let text_color = if is_active {
        theme.accent_aqua
    } else {
        theme.text_secondary
    };
    let border_color = if is_active {
        theme.accent_cyan
    } else {
        Hsla::transparent_black()
    };

    div()
        .id(ElementId::Name(panel.label().into()))
        .flex()
        .flex_row()
        .items_center()
        .gap(theme.space_2)
        .w_full()
        .h(px(32.0))
        .px(theme.space_2)
        .rounded(theme.radius_md)
        .bg(bg)
        .border_l_2()
        .border_color(border_color)
        .cursor_pointer()
        .hover(|style| style.bg(theme.bg_primary).text_color(theme.text_primary))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _event, _window, cx| {
                info!("Sidebar click: {:?}", panel);
                this.switch_to_panel(panel, cx);
            }),
        )
        .child(
            div()
                .w(px(16.0))
                .h(px(16.0))
                .flex()
                .items_center()
                .justify_center()
                .child(Icon::new(panel.icon()).size_3p5().text_color(text_color)),
        )
        .child(
            div()
                .text_size(theme.font_size_sm)
                .text_color(text_color)
                .font_weight(if is_active {
                    FontWeight::SEMIBOLD
                } else {
                    FontWeight::NORMAL
                })
                .child(panel.label()),
        )
        .into_any_element()
}

fn quick_start_templates() -> Vec<QuickStartTemplateDisplay> {
    vec![
        QuickStartTemplateDisplay {
            id: "dogfood".into(),
            title: "Improve This Codebase".into(),
            description: "Use Hive to find the highest-leverage gaps in the current project and start closing them."
                .into(),
            outcome: "Best when you want HiveCode to improve HiveCode itself.".into(),
        },
        QuickStartTemplateDisplay {
            id: "feature".into(),
            title: "Ship A Feature".into(),
            description: "Trace the relevant code, define the change, implement it, and verify the result."
                .into(),
            outcome: "Best when you already know the product outcome you want.".into(),
        },
        QuickStartTemplateDisplay {
            id: "bug".into(),
            title: "Fix A Bug".into(),
            description: "Reproduce the problem, isolate root cause, patch it, and confirm the regression is closed."
                .into(),
            outcome: "Best when the project is blocked by a failure or broken workflow.".into(),
        },
        QuickStartTemplateDisplay {
            id: "understand".into(),
            title: "Understand The Project".into(),
            description: "Map the architecture, explain how the pieces fit, and identify the real risks."
                .into(),
            outcome: "Best when a human needs a clear read on the codebase before deciding.".into(),
        },
        QuickStartTemplateDisplay {
            id: "review".into(),
            title: "Review Current State".into(),
            description: "Inspect git state and the working tree, then call out problems, regressions, and next actions."
                .into(),
            outcome: "Best when you want an informed starting point before more coding.".into(),
        },
    ]
}

fn quick_start_template_title(template_id: &str) -> &'static str {
    match template_id {
        "feature" => "Ship A Feature",
        "bug" => "Fix A Bug",
        "understand" => "Understand The Project",
        "review" => "Review Current State",
        _ => "Improve This Codebase",
    }
}

fn quick_start_template_placeholder(template_id: &str) -> &'static str {
    match template_id {
        "feature" => "Describe the feature outcome, user flow, or missing integration to ship",
        "bug" => "Describe the failure, broken behavior, or user-facing bug to fix",
        "understand" => "Describe the architecture, workflow, or module you want Hive to explain",
        "review" => "Describe what you want reviewed, for example the current diff, release readiness, or regressions",
        _ => "Describe what Hive should improve, complete, or tighten in this project",
    }
}

fn quick_start_template_instruction(template_id: &str) -> &'static str {
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
