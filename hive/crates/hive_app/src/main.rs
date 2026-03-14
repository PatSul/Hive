#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod tray;

use std::borrow::Cow;
use std::sync::mpsc;
use std::time::Duration;

use gpui::*;
use tracing::{error, info, warn};

use hive_ai::service::AiServiceConfig;
use hive_ai::tts::TtsProviderType;
use hive_ai::tts::service::TtsServiceConfig;
use hive_core::config::{ConfigManager, HiveConfig};
use hive_core::logging;
use hive_core::notifications::{AppNotification, NotificationType};
use hive_core::persistence::Database;
use hive_core::security::SecurityGateway;
use hive_core::updater::UpdateService;
use hive_ui::globals::{
    AppA2aClient, AppAiService, AppAssistant, AppAutomation, AppAws, AppAzure, AppBitbucket, AppBrowser,
    AppChannels, AppCli, AppCollectiveMemory, AppCompetenceDetector, AppConfig, AppDatabase,
    AppDocker, AppDocsIndexer, AppFleetLearning, AppGcp, AppGitLab, AppIde, AppIntegrationDb,
    AppHueClient, AppKnowledge, AppKubernetes, AppLearning, AppMarketplace, AppMcpServer, AppMessaging, AppNetwork, AppNotifications, AppOllamaManager, AppPersonas,
    AppContextEngine, AppHiveMemory, AppPluginManager, AppProjectManagement, AppRagService, AppRpcConfig, AppScheduler,
    AppSecurity, AppSemanticSearch, AppShield, AppSkillManager, AppSkills, AppSpecs, AppStandupService,
    AppReminderRx, AppTts, AppUiActionTx, AppUpdater, AppVoiceAssistant, AppWallets,
};
use hive_ui::workspace::{
    ClearChat, HiveWorkspace, NewConversation, SwitchPanel, SwitchToAgents, SwitchToChannels,
    SwitchToChat, SwitchToFiles, SwitchToHistory, SwitchToKanban, SwitchToLogs,
    SwitchToMonitor, SwitchToSpecs, SwitchToWorkflows,
};

const VERSION: &str = env!("HIVE_VERSION");

/// Temporary global to pass the UI action receiver from `init_services` to
/// `open_main_window`.  Taken (consumed) once the window's polling loop starts.
struct UiActionRx(
    Option<mpsc::Receiver<hive_ui::core_types::action_bridge::UiActionRequest>>,
);
impl Global for UiActionRx {}

// ---------------------------------------------------------------------------
// Embedded assets (icons, images)
// ---------------------------------------------------------------------------

#[derive(rust_embed::RustEmbed)]
#[folder = "../../assets"]
struct Assets;

impl gpui::AssetSource for Assets {
    fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
        Ok(Self::get(path).map(|f| f.data))
    }

    fn list(&self, path: &str) -> gpui::Result<Vec<SharedString>> {
        Ok(Self::iter()
            .filter(|p| p.starts_with(path))
            .map(|p| SharedString::from(p.to_string()))
            .collect())
    }
}

// ---------------------------------------------------------------------------
// Tray global (prevents drop when run callback returns)
// ---------------------------------------------------------------------------

pub struct AppTray(pub Option<tray::TrayService>);
impl gpui::Global for AppTray {}

/// Walk up from `path` looking for a `.git` directory, returning the first
/// ancestor that contains one. Falls back to `path` itself if no git root is
/// found.
fn discover_git_root(path: std::path::PathBuf) -> std::path::PathBuf {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
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

// ---------------------------------------------------------------------------
// Actions
// ---------------------------------------------------------------------------

actions!(hive, [Quit, TogglePrivacy, OpenSettings]);

// ---------------------------------------------------------------------------
// Bootstrap
// ---------------------------------------------------------------------------

/// Initialize backend services and store them as GPUI globals.
fn init_services(cx: &mut App) -> anyhow::Result<()> {
    let config_manager =
        ConfigManager::new().inspect_err(|e| error!("Config manager init failed: {e}"))?;
    info!(
        "Config loaded (privacy_mode={})",
        config_manager.get().privacy_mode
    );
    cx.set_global(AppConfig(config_manager));

    cx.set_global(AppSecurity(SecurityGateway::new()));
    info!("SecurityGateway initialized");

    cx.set_global(AppNotifications(
        hive_core::notifications::NotificationStore::new(),
    ));

    // Build AI service from config (needed before wiring LearnerTierAdjuster).
    let config = cx.global::<AppConfig>().0.get().clone();
    let ai_config = AiServiceConfig {
        anthropic_api_key: config.anthropic_api_key.clone(),
        openai_api_key: config.openai_api_key.clone(),
        openrouter_api_key: config.openrouter_api_key.clone(),
        google_api_key: config.google_api_key.clone(),
        groq_api_key: config.groq_api_key.clone(),
        huggingface_api_key: config.huggingface_api_key.clone(),
        xai_api_key: config.xai_api_key.clone(),
        mistral_api_key: config.mistral_api_key.clone(),
        venice_api_key: config.venice_api_key.clone(),
        litellm_url: config.litellm_url.clone(),
        litellm_api_key: config.litellm_api_key.clone(),
        ollama_url: config.ollama_url.clone(),
        lmstudio_url: config.lmstudio_url.clone(),
        local_provider_url: config.local_provider_url.clone(),
        privacy_mode: config.privacy_mode,
        default_model: config.default_model.clone(),
        auto_routing: config.auto_routing,
    };
    cx.set_global(AppAiService(hive_ai::AiService::new(ai_config)));
    cx.global_mut::<AppAiService>().0.start_discovery();
    info!("AiService initialized");

    // Compute DB paths before the parallel section (HiveConfig::base_dir is cheap).
    let learning_db_str = HiveConfig::base_dir()
        .map(|d| d.join("learning.db"))
        .unwrap_or_else(|_| std::path::PathBuf::from("learning.db"))
        .to_string_lossy()
        .to_string();
    let assistant_db_str = HiveConfig::base_dir()
        .map(|d| d.join("assistant.db"))
        .unwrap_or_else(|_| std::path::PathBuf::from("assistant.db"))
        .to_string_lossy()
        .to_string();

    // Open all three databases in parallel — they are independent and each opens
    // its own SQLite connection.  `std::thread::scope` ensures the borrows of the
    // path strings are valid for the lifetime of the spawned threads.
    let (db_result, learning_result, assistant_result) = std::thread::scope(|s| {
        let db_handle = s.spawn(Database::open);
        let learn_handle = s.spawn(|| hive_learn::LearningService::open(&learning_db_str));
        let assist_handle = s.spawn(|| hive_assistant::AssistantService::open(&assistant_db_str));

        (
            db_handle.join().expect("Database::open thread panicked"),
            learn_handle
                .join()
                .expect("LearningService::open thread panicked"),
            assist_handle
                .join()
                .expect("AssistantService::open thread panicked"),
        )
    });

    // --- Register results with cx sequentially (cx is !Send) ---

    let db = db_result.inspect_err(|e| error!("Database open failed: {e}"))?;

    // Backfill: import any JSON conversations that aren't yet in SQLite,
    // including building their FTS5 search index.
    if let Ok(conv_dir) = HiveConfig::conversations_dir()
        && let Err(e) = db.backfill_from_json(&conv_dir)
    {
        warn!("JSON→SQLite backfill failed: {e}");
    }

    cx.set_global(AppDatabase(db));
    info!("Database opened");

    // Learning service
    match learning_result {
        Ok(learning) => {
            let learning = std::sync::Arc::new(learning);
            info!("LearningService initialized at {}", learning_db_str);

            // Wire the tier adjuster into the AI router so routing decisions
            // benefit from learned outcome data.
            let adjuster = hive_learn::LearnerTierAdjuster::new(std::sync::Arc::clone(&learning));
            cx.global_mut::<AppAiService>()
                .0
                .router_mut()
                .set_tier_adjuster(std::sync::Arc::new(adjuster));
            info!("LearnerTierAdjuster wired into ModelRouter");

            cx.set_global(AppLearning(learning));
        }
        Err(e) => {
            error!("LearningService init failed: {e}");
        }
    }

    // Privacy shield — load from persisted config.
    let shield = std::sync::Arc::new(hive_shield::HiveShield::new(config.shield.clone()));
    cx.set_global(AppShield(shield));
    info!(
        "HiveShield initialized (enabled={}, rules={})",
        config.shield_enabled,
        config.shield.user_rules.len()
    );

    // TTS service — build from config keys.
    let tts_config = TtsServiceConfig {
        default_provider: TtsProviderType::from_str_loose(&config.tts_provider)
            .unwrap_or(TtsProviderType::Qwen3),
        default_voice_id: config.tts_voice_id.clone(),
        speed: config.tts_speed,
        enabled: config.tts_enabled,
        auto_speak: config.tts_auto_speak,
        openai_api_key: config.openai_api_key.clone(),
        huggingface_api_key: config.huggingface_api_key.clone(),
        elevenlabs_api_key: config.elevenlabs_api_key.clone(),
        telnyx_api_key: config.telnyx_api_key.clone(),
    };
    let tts = std::sync::Arc::new(hive_ai::TtsService::new(tts_config));
    cx.set_global(AppTts(tts));
    info!("TTS service initialized");

    // Voice assistant — text-to-intent classification with TTS.
    {
        let mut voice = hive_agents::VoiceAssistant::new();
        if let Some(tts_arc) = cx.has_global::<AppTts>().then(|| cx.global::<AppTts>().0.clone()) {
            voice.set_tts(tts_arc);
        }
        cx.set_global(AppVoiceAssistant(std::sync::Arc::new(std::sync::Mutex::new(voice))));
        info!("VoiceAssistant initialized");
    }

    // RAG Service — document indexing + TF-IDF retrieval for context injection.
    let rag_service = hive_ai::RagService::new(50, 10);
    cx.set_global(AppRagService(std::sync::Arc::new(std::sync::Mutex::new(rag_service))));
    info!("RagService initialized");

    // Semantic Search Service — file-content search with relevance scoring.
    let semantic_search = hive_ai::SemanticSearchService::new(1000);
    cx.set_global(AppSemanticSearch(std::sync::Arc::new(std::sync::Mutex::new(semantic_search))));
    info!("SemanticSearchService initialized");

    // Context Engine — smart context curation with TF-IDF + heuristic boosts.
    let context_engine = hive_ai::ContextEngine::new();
    cx.set_global(AppContextEngine(std::sync::Arc::new(std::sync::Mutex::new(context_engine))));
    info!("ContextEngine initialized");

    // HiveMemory — LanceDB-backed vector embeddings + chunking.
    {
        let memory_path = HiveConfig::base_dir()
            .map(|d| d.join("hive_memory.lance"))
            .unwrap_or_else(|_| std::path::PathBuf::from("hive_memory.lance"));
        let embedder: std::sync::Arc<dyn hive_ai::embeddings::EmbeddingProvider> = {
            if let Some(ref key) = config.openai_api_key {
                if !key.is_empty() {
                    std::sync::Arc::new(hive_ai::embeddings::OpenAiEmbeddings::new(key.clone()))
                } else {
                    std::sync::Arc::new(hive_ai::embeddings::OllamaEmbeddings::new(
                        config.ollama_url.clone(),
                    ))
                }
            } else {
                std::sync::Arc::new(hive_ai::embeddings::OllamaEmbeddings::new(
                    config.ollama_url.clone(),
                ))
            }
        };
        let rt = tokio::runtime::Handle::try_current()
            .or_else(|_| {
                tokio::runtime::Runtime::new().map(|rt| {
                    let handle = rt.handle().clone();
                    // Leak the runtime so it lives for the app's duration.
                    std::mem::forget(rt);
                    handle
                })
            });
        if let Ok(handle) = rt {
            match handle.block_on(hive_ai::memory::HiveMemory::open(
                &memory_path.to_string_lossy(),
                embedder,
            )) {
                Ok(memory) => {
                    cx.set_global(AppHiveMemory(std::sync::Arc::new(
                        tokio::sync::Mutex::new(memory),
                    )));
                    info!("HiveMemory initialized (LanceDB)");
                }
                Err(e) => warn!("HiveMemory init failed: {e}"),
            }
        } else {
            warn!("HiveMemory init skipped: no tokio runtime available");
        }
    }

    // Fleet Learning — cross-instance pattern detection.
    let fleet_db_path = HiveConfig::base_dir()
        .map(|d| d.join("fleet_learning.db"))
        .unwrap_or_else(|_| std::path::PathBuf::from("fleet_learning.db"));
    let fleet = match hive_ai::FleetLearningService::with_db(&fleet_db_path.to_string_lossy()) {
        Ok(service) => {
            info!("FleetLearningService initialized (SQLite-backed)");
            service
        }
        Err(e) => {
            warn!("FleetLearningService DB open failed, using in-memory: {e}");
            hive_ai::FleetLearningService::new()
        }
    };
    cx.set_global(AppFleetLearning(std::sync::Arc::new(std::sync::Mutex::new(fleet))));

    // Collective Memory
    let collective_db_path = HiveConfig::base_dir()
        .map(|d| d.join("collective_memory.db"))
        .unwrap_or_else(|_| std::path::PathBuf::from("collective_memory.db"));
    let memory = hive_agents::collective_memory::CollectiveMemory::open(&collective_db_path.to_string_lossy())
        .unwrap_or_else(|_| hive_agents::collective_memory::CollectiveMemory::in_memory().unwrap());
    cx.set_global(AppCollectiveMemory(std::sync::Arc::new(memory)));
    info!("CollectiveMemory initialized");

    // Standup Service
    let standup = hive_agents::standup::StandupService::new();
    cx.set_global(AppStandupService(std::sync::Arc::new(std::sync::Mutex::new(standup))));
    info!("StandupService initialized");

    // Competence Detector
    let competence = hive_agents::competence_detection::CompetenceDetector::with_defaults();
    cx.set_global(AppCompetenceDetector(std::sync::Arc::new(std::sync::Mutex::new(competence))));
    info!("CompetenceDetector initialized");

    // Skills registry — file-backed, loads from ~/.hive/skills/*.toml.
    // Ensures all 15 built-in skills exist on disk on first run.
    {
        let skills_dir = HiveConfig::base_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from(".hive"))
            .join("skills");
        let loader = hive_agents::skill_format::SkillLoader::new(skills_dir.clone());
        cx.set_global(AppSkills(hive_agents::skills::SkillsRegistry::with_loader(loader)));
        info!("SkillsRegistry initialized (file-backed, ~/.hive/skills/)");

        // Legacy file-based skill manager kept for backward compat.
        cx.set_global(AppSkillManager(hive_agents::skills::SkillManager::new(skills_dir)));
        info!("SkillManager initialized (user skills)");
    }

    // Skill marketplace — install/remove community skills with security scanning.
    cx.set_global(AppMarketplace(hive_agents::SkillMarketplace::new()));
    info!("SkillMarketplace initialized");

    // Plugin manager — fetch, parse, and version-check external plugin packages.
    cx.set_global(AppPluginManager(hive_agents::PluginManager::new(
        reqwest::Client::new(),
    )));
    info!("PluginManager initialized");

    // Load installed plugins from disk.
    {
        let plugins_path = HiveConfig::base_dir()
            .map(|d| d.join("plugins.json"))
            .unwrap_or_else(|_| std::path::PathBuf::from("plugins.json"));
        let mp = &mut cx.global_mut::<AppMarketplace>().0;
        if let Err(e) = mp.load_plugins_from_file(&plugins_path) {
            warn!("Failed to load plugins: {e}");
        }
    }

    // Persona registry — built-in agent roles.
    cx.set_global(AppPersonas(hive_agents::personas::PersonaRegistry::new()));
    info!("PersonaRegistry initialized (6 built-in personas)");

    // Automation service — workflow engine.
    let workspace_root = discover_git_root(std::env::current_dir().unwrap_or_default());
    let mut automation = hive_agents::AutomationService::new();
    let workflow_report = automation.initialize_workflows(&workspace_root);
    if workflow_report.failed > 0 {
        for load_error in &workflow_report.errors {
            warn!("Workflow load error: {load_error}");
        }
    }
    cx.set_global(AppAutomation(automation));
    info!(
        "AutomationService initialized (loaded={}, failed={}, skipped={})",
        workflow_report.loaded, workflow_report.failed, workflow_report.skipped
    );

    // Built-in MCP tool server — file I/O, command exec, search, git.
    cx.set_global(AppMcpServer(hive_agents::mcp_server::McpServer::new(
        workspace_root,
    )));
    info!("McpServer initialized (built-in + integration tools)");

    // UI action bridge — expose every GPUI action as an MCP tool.
    //
    // An mpsc channel bridges MCP handler threads → main GPUI thread.
    // MCP handlers send `UiActionRequest`s; the main-thread polling loop
    // (in `open_main_window`) dispatches them as GPUI actions.
    {
        use hive_ui::core_types::action_bridge;

        let (action_tx, action_rx) = mpsc::channel::<action_bridge::UiActionRequest>();
        let action_tx = std::sync::Arc::new(action_tx);

        // Store sender as a global so MCP handlers can clone it.
        cx.set_global(AppUiActionTx(std::sync::Arc::clone(&action_tx)));

        // Store receiver as a global so `open_main_window` can take it.
        cx.set_global(UiActionRx(Some(action_rx)));

        // Register every UI action as an MCP tool on the server.
        let mcp = &mut cx.global_mut::<AppMcpServer>().0;
        for tool in action_bridge::ui_action_tools() {
            let tx = std::sync::Arc::clone(&action_tx);
            let action_name = tool
                .name
                .strip_prefix("ui.")
                .unwrap_or(&tool.name)
                .to_string();

            if !action_bridge::is_action_allowed(&action_name) {
                continue;
            }

            mcp.register(
                tool,
                Box::new(move |args: serde_json::Value| {
                    let (resp_tx, resp_rx) = mpsc::channel();
                    tx.send(action_bridge::UiActionRequest {
                        action_name: action_name.clone(),
                        params: args,
                        response_tx: resp_tx,
                    })
                    .map_err(|_| "UI action bridge channel closed".to_string())?;

                    resp_rx
                        .recv_timeout(Duration::from_secs(5))
                        .map_err(|e| format!("UI action dispatch timeout: {e}"))?
                }),
            );
        }
        info!(
            "UI action bridge: {} tools registered",
            action_bridge::ui_action_tools().len()
        );
    }

    // Spec manager — project specifications.
    cx.set_global(AppSpecs(hive_agents::SpecManager::new()));
    info!("SpecManager initialized");

    // CLI service — built-in commands, doctor checks.
    cx.set_global(AppCli(hive_terminal::CliService::new()));
    info!("CliService initialized");

    // Assistant service
    match assistant_result {
        Ok(assistant) => {
            cx.set_global(AppAssistant(assistant));
            info!("AssistantService initialized");
        }
        Err(e) => {
            error!("AssistantService init failed: {e}");
        }
    }

    // Scheduler + tick driver — cron jobs and reminder checks once per minute.
    //
    // The Scheduler lives in an Arc<Mutex<>> so the background tick-driver
    // thread can call tick() while the main thread retains access for
    // add/remove operations.  The tick driver opens its own AssistantStorage
    // connection for ReminderService to avoid contention with the main thread.
    {
        let scheduler_path = HiveConfig::base_dir()
            .map(|d| d.join("scheduler.json"))
            .unwrap_or_else(|_| std::path::PathBuf::from("scheduler.json"));
        let scheduler = hive_core::scheduler::Scheduler::load_from_file(&scheduler_path)
            .unwrap_or_else(|e| {
                warn!("Scheduler load failed, starting empty: {e}");
                hive_core::scheduler::Scheduler::new()
            });
        let scheduler = std::sync::Arc::new(std::sync::Mutex::new(scheduler));
        cx.set_global(AppScheduler(std::sync::Arc::clone(&scheduler)));
        info!("Scheduler initialized");

        let (reminder_tx, reminder_rx) = std::sync::mpsc::channel();
        let tick_config = hive_assistant::tick_driver::TickDriverConfig {
            interval: Duration::from_secs(60),
            assistant_db_path: assistant_db_str.clone(),
            reminder_tx: Some(reminder_tx),
        };
        hive_assistant::tick_driver::start_tick_driver(scheduler, tick_config);
        info!("Tick driver started (scheduler + reminders, 60s interval)");

        cx.set_global(AppReminderRx(std::sync::Arc::new(
            std::sync::Mutex::new(reminder_rx),
        )));
    }

    // Wallet store — load existing wallets or start empty.
    let wallet_path = HiveConfig::base_dir()
        .map(|d| d.join("wallets.enc"))
        .unwrap_or_else(|_| std::path::PathBuf::from("wallets.enc"));
    let wallets = if wallet_path.exists() {
        hive_blockchain::wallet_store::WalletStore::load_from_file(&wallet_path).unwrap_or_else(
            |e| {
                error!("WalletStore load failed: {e}");
                hive_blockchain::wallet_store::WalletStore::new()
            },
        )
    } else {
        hive_blockchain::wallet_store::WalletStore::new()
    };
    cx.set_global(AppWallets(wallets));
    info!("WalletStore initialized");

    // RPC config — load saved per-chain endpoints or fall back to defaults.
    let rpc_config_path = HiveConfig::base_dir()
        .map(|d| d.join("rpc_config.json"))
        .unwrap_or_else(|_| std::path::PathBuf::from("rpc_config.json"));
    let rpc_config = hive_blockchain::rpc_config::RpcConfigStore::load_from_file(&rpc_config_path)
        .unwrap_or_else(|e| {
            warn!("RpcConfigStore load failed, using defaults: {e}");
            hive_blockchain::rpc_config::RpcConfigStore::with_defaults()
        });
    cx.set_global(AppRpcConfig(rpc_config));
    info!("RpcConfigStore initialized");

    // IDE integration — workspace and file tracking.
    cx.set_global(AppIde(hive_integrations::ide::IdeIntegrationService::new()));
    info!("IdeIntegrationService initialized");

    // --- Integration hubs (conditionally initialized) ---

    // Messaging hub — always create, providers added when tokens configured.
    let messaging = std::sync::Arc::new(hive_integrations::messaging::MessagingHub::new());
    cx.set_global(AppMessaging(messaging));
    info!("MessagingHub initialized");

    // Project management hub — always create, providers added when tokens configured.
    let pm = std::sync::Arc::new(hive_integrations::project_management::ProjectManagementHub::new());
    cx.set_global(AppProjectManagement(pm));
    info!("ProjectManagementHub initialized");

    // Knowledge hub — always create, providers added when tokens configured.
    let mut knowledge_hub = hive_integrations::knowledge::KnowledgeHub::new();

    // Register Obsidian provider if a vault path is configured.
    if let Some(ref vault_path) = config.obsidian_vault_path {
        if !vault_path.is_empty() {
            let mut obsidian = hive_integrations::knowledge::ObsidianProvider::new(vault_path);
            // index_vault is async — run it on a temporary single-threaded runtime.
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build();
            match rt {
                Ok(rt) => match rt.block_on(obsidian.index_vault()) {
                    Ok(count) => {
                        info!("Obsidian vault indexed: {vault_path} ({count} pages)");
                    }
                    Err(e) => {
                        warn!("Obsidian vault indexing failed: {e}");
                    }
                },
                Err(e) => {
                    warn!("Failed to create Obsidian indexing runtime: {e}");
                }
            }
            knowledge_hub.register_provider(Box::new(obsidian));
        }
    }

    // Register Notion provider if an API key is configured.
    if let Some(ref notion_key) = config.notion_api_key {
        if !notion_key.is_empty() {
            match hive_integrations::knowledge::NotionClient::new(notion_key) {
                Ok(notion) => {
                    info!("Notion knowledge base connected");
                    knowledge_hub.register_provider(Box::new(notion));
                }
                Err(e) => {
                    warn!("Notion client initialization failed: {e}");
                }
            }
        }
    }

    let knowledge = std::sync::Arc::new(knowledge_hub);
    cx.set_global(AppKnowledge(knowledge));
    info!("KnowledgeHub initialized");

    // Database hub — always create, connections added at runtime.
    let db_hub = std::sync::Arc::new(hive_integrations::database::DatabaseHub::new());
    cx.set_global(AppIntegrationDb(db_hub));
    info!("DatabaseHub initialized");

    // Docker client — initialize with default docker CLI path.
    let docker = std::sync::Arc::new(hive_integrations::docker::DockerClient::new());
    cx.set_global(AppDocker(docker));
    info!("DockerClient initialized");

    // Kubernetes client — initialize with default kubeconfig.
    let k8s = std::sync::Arc::new(hive_integrations::kubernetes::KubernetesClient::new());
    cx.set_global(AppKubernetes(k8s));
    info!("KubernetesClient initialized");

    // Browser automation — headless by default.
    let browser = std::sync::Arc::new(hive_integrations::browser::BrowserAutomation::new());
    cx.set_global(AppBrowser(browser));
    info!("BrowserAutomation initialized");

    let a2a_config_path = HiveConfig::base_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from(".hive"))
        .join("a2a.toml");
    let a2a_client = match hive_a2a::A2aClientService::load_or_create(&a2a_config_path) {
        Ok(service) => std::sync::Arc::new(service),
        Err(e) => {
            warn!("A2A client config load failed, using defaults: {e}");
            std::sync::Arc::new(hive_a2a::A2aClientService::with_config(
                a2a_config_path.clone(),
                hive_a2a::A2aConfig::default(),
            ))
        }
    };
    cx.set_global(AppA2aClient(a2a_client));
    info!("A2A client initialized");

    let ollama_manager = std::sync::Arc::new(hive_terminal::local_ai::OllamaManager::new(Some(
        config.ollama_url.clone(),
    )));
    cx.set_global(AppOllamaManager(ollama_manager));
    info!("OllamaManager initialized");

    // Local AI provider detection — probe well-known ports in background.
    std::thread::Builder::new()
        .name("local-ai-detect".into())
        .spawn(|| {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build();
            if let Ok(rt) = rt {
                let detected = rt.block_on(async {
                    let detector = hive_terminal::local_ai::LocalAiDetector::new();
                    detector.detect_all().await
                });
                let available: Vec<_> = detected.iter().filter(|p| p.available).collect();
                info!(
                    "Local AI detection: found {} provider(s)",
                    available.len()
                );
                for provider in &available {
                    info!(
                        "  - {} at {} ({} model(s))",
                        provider.name,
                        provider.base_url,
                        provider.models.len()
                    );
                }
            }
        })
        .ok();

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
    info!(
        "Hue smart-home integration {}",
        if cx.global::<AppHueClient>().0.is_some() {
            "initialized"
        } else {
            "not configured"
        }
    );

    // Bitbucket client — needs username + app password from environment.
    if let (Ok(bb_user), Ok(bb_pass)) = (
        std::env::var("BITBUCKET_USERNAME"),
        std::env::var("BITBUCKET_APP_PASSWORD"),
    ) {
        match hive_integrations::bitbucket::BitbucketClient::new(bb_user, bb_pass) {
            Ok(client) => {
                cx.set_global(AppBitbucket(std::sync::Arc::new(client)));
                info!("BitbucketClient initialized");
            }
            Err(e) => warn!("BitbucketClient init failed: {e}"),
        }
    }

    // GitLab client — needs private token from environment.
    if let Ok(gl_token) = std::env::var("GITLAB_PRIVATE_TOKEN") {
        match hive_integrations::gitlab::GitLabClient::new(gl_token) {
            Ok(client) => {
                cx.set_global(AppGitLab(std::sync::Arc::new(client)));
                info!("GitLabClient initialized");
            }
            Err(e) => warn!("GitLabClient init failed: {e}"),
        }
    }

    // Cloud clients — initialize with default credential chains.
    let aws = std::sync::Arc::new(hive_integrations::cloud::AwsClient::new(None, None));
    cx.set_global(AppAws(aws));
    info!("AwsClient initialized");

    let azure = std::sync::Arc::new(hive_integrations::cloud::AzureClient::new(None));
    cx.set_global(AppAzure(azure));
    info!("AzureClient initialized");

    let gcp = std::sync::Arc::new(hive_integrations::cloud::GcpClient::new(None));
    cx.set_global(AppGcp(gcp));
    info!("GcpClient initialized");

    // Docs indexer — workspace-scoped documentation search.
    match hive_integrations::docs_indexer::DocsIndexer::new() {
        Ok(indexer) => {
            cx.set_global(AppDocsIndexer(std::sync::Arc::new(indexer)));
            info!("DocsIndexer initialized");
        }
        Err(e) => warn!("DocsIndexer init failed: {e}"),
    }

    // Wire integration tool handlers to real services (replaces stubs).
    // DocsIndexer is conditionally initialized — create a fallback if needed.
    {
        use hive_agents::integration_tools::IntegrationServices;
        let docs_indexer = if cx.has_global::<AppDocsIndexer>() {
            cx.global::<AppDocsIndexer>().0.clone()
        } else {
            // Fallback: create a minimal indexer so wiring still proceeds.
            std::sync::Arc::new(
                hive_integrations::docs_indexer::DocsIndexer::new().unwrap_or_else(|e| {
                    warn!("DocsIndexer fallback creation failed: {e}");
                    // Return a DocsIndexer with no indexed content.
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
        info!("MCP integration tools wired to live services");
    }

    // Channel store — AI agent messaging channels.
    let mut channel_store = hive_core::channels::ChannelStore::new();
    channel_store.ensure_default_channels();
    cx.set_global(AppChannels(channel_store));
    info!("ChannelStore initialized with default channels");

    // P2P network node — federation, peer discovery, WebSocket server.
    //
    // The real node is created once, then moved to a background thread with
    // its own tokio runtime. The GPUI global receives a read-only handle that
    // shares the node's live peer registry.
    {
        let network_base_dir = HiveConfig::base_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from(".hive"));
        let network_config_path = network_base_dir.join("network.json");
        let net_config = hive_network::NetworkConfig::load_or_default(&network_config_path);

        let node_name = std::env::var("HIVE_NODE_NAME").unwrap_or_else(|_| {
            #[cfg(unix)]
            {
                let mut buf = [0u8; 256];
                let c_str = unsafe {
                    libc::gethostname(buf.as_mut_ptr() as *mut libc::c_char, buf.len());
                    std::ffi::CStr::from_ptr(buf.as_ptr() as *const libc::c_char)
                };
                c_str.to_string_lossy().to_string()
            }
            #[cfg(not(unix))]
            {
                "hive-node".to_string()
            }
        });

        let identity_path = network_base_dir.join("network_identity.json");
        let identity = hive_network::NodeIdentity::load_or_generate(&identity_path, &node_name);
        let node = hive_network::HiveNode::new(identity, net_config);
        let listen_addr = node.config().listen_addr;

        cx.set_global(AppNetwork(std::sync::Arc::new(node.handle())));

        // Start the real node on a background thread with its own tokio runtime.
        std::thread::Builder::new()
            .name("hive-p2p".into())
            .spawn(move || {
                let mut node = node;
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("P2P tokio runtime");
                rt.block_on(async {
                    match node.start().await {
                        Ok(()) => info!("P2P network started — listening on {listen_addr}"),
                        Err(e) => {
                            error!("P2P network start failed (non-fatal): {e}");
                            return;
                        }
                    }
                    // Park the runtime so spawned tasks (server, discovery,
                    // heartbeat) keep running for the lifetime of the app.
                    loop {
                        tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
                    }
                });
            })
            .ok();
        info!("P2P network node initialized (background start in progress)");
    }

    // Auto-update service — checks GitHub releases for newer versions.
    let updater = UpdateService::new(VERSION);
    cx.set_global(AppUpdater(updater));
    info!("UpdateService initialized (current: v{VERSION})");

    // Remote control daemon — web UI for phone/tablet access.
    //
    // Runs on a dedicated background thread with its own tokio runtime to
    // avoid conflicts with the GPUI event loop.  Only started when the
    // user has explicitly enabled remote control in config.
    if config.remote_enabled {
        let remote_local_port = config.remote_local_port;
        let remote_web_port = config.remote_web_port;
        let data_dir = HiveConfig::base_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from(".hive"));

        std::thread::Builder::new()
            .name("hive-remote".into())
            .spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("Remote daemon tokio runtime");
                rt.block_on(async {
                    let daemon_config = hive_remote::daemon::DaemonConfig {
                        data_dir,
                        local_port: remote_local_port,
                        web_port: remote_web_port,
                        shutdown_grace_secs: 30,
                    };
                    match hive_remote::daemon::HiveDaemon::new(daemon_config) {
                        Ok(daemon) => {
                            let daemon = std::sync::Arc::new(tokio::sync::RwLock::new(daemon));
                            let router = hive_remote::web_server::build_router(daemon);
                            let addr = format!("0.0.0.0:{}", remote_web_port);
                            match tokio::net::TcpListener::bind(&addr).await {
                                Ok(listener) => {
                                    info!("Remote control web UI at http://{}", addr);
                                    let _ = axum::serve(listener, router).await;
                                }
                                Err(e) => error!("Failed to bind remote control port: {}", e),
                            }
                        }
                        Err(e) => error!("Failed to start remote daemon: {}", e),
                    }
                });
            })
            .ok();
        info!("Remote control daemon starting on port {}", config.remote_web_port);
    }

    // A2A (Agent-to-Agent) protocol server — exposes Hive agents as A2A skills.
    //
    // Runs on a dedicated background thread with its own tokio runtime,
    // following the same pattern as the P2P network and remote control daemon.
    // Config is loaded from ~/.hive/a2a.toml (created with defaults on first run).
    {
        match hive_a2a::A2aConfig::load_or_create(&a2a_config_path) {
            Ok(a2a_config) => {
                if a2a_config.server.enabled {
                    let a2a_handler = {
                        let ai_service = &cx.global::<AppAiService>().0;
                        let probe_messages = vec![hive_ai::types::ChatMessage::text(
                            hive_ai::types::MessageRole::User,
                            "A2A readiness probe",
                        )];

                        ai_service
                            .prepare_stream(
                                probe_messages,
                                ai_service.default_model(),
                                None,
                                None,
                            )
                            .map(|(provider, request)| {
                                let executor =
                                    hive_a2a::ProviderExecutor::new(provider, request.model);
                                let handler = hive_a2a::HiveTaskHandler::new(
                                    std::sync::Arc::new(executor),
                                    a2a_config.server.defaults.clone(),
                                );
                                std::sync::Arc::new(handler)
                                    as std::sync::Arc<dyn hive_a2a::TaskHandlerAdapter>
                            })
                    };

                    if a2a_handler.is_none() {
                        warn!("A2A server enabled but no AI provider configured; skipping");
                    } else {
                        let bind_addr = a2a_config.bind_addr();

                        std::thread::Builder::new()
                            .name("hive-a2a".into())
                            .spawn(move || {
                                let rt = tokio::runtime::Builder::new_current_thread()
                                    .enable_all()
                                    .build()
                                    .expect("A2A server tokio runtime");
                                rt.block_on(async {
                                    if let Err(e) =
                                        hive_a2a::start_server_with_handler(
                                            a2a_config, a2a_handler,
                                        )
                                        .await
                                    {
                                        error!("[A2A] Server error: {}", e);
                                    }
                                });
                            })
                            .ok();

                        info!("A2A server starting on {}", bind_addr);
                    }
                } else {
                    info!("A2A server disabled in config");
                }
            }
            Err(e) => {
                warn!("A2A config load failed (non-fatal): {}", e);
            }
        }
    }

    Ok(())
}

/// Register global keyboard shortcuts and action handlers.
fn register_actions(cx: &mut App) {
    // macOS uses Cmd for shortcuts; all other platforms use Ctrl.
    #[cfg(target_os = "macos")]
    cx.bind_keys([
        // App-level actions
        KeyBinding::new("cmd-q", Quit, None),
        KeyBinding::new("cmd-,", OpenSettings, None),
        KeyBinding::new("cmd-p", TogglePrivacy, None),
        // Chat actions
        KeyBinding::new("cmd-n", NewConversation, None),
        KeyBinding::new("cmd-l", ClearChat, None),
        // Panel switching: cmd-1..cmd-0 map to first 10 sidebar panels
        KeyBinding::new("cmd-1", SwitchToChat, None),
        KeyBinding::new("cmd-2", SwitchToHistory, None),
        KeyBinding::new("cmd-3", SwitchToFiles, None),
        KeyBinding::new("cmd-4", SwitchToSpecs, None),
        KeyBinding::new("cmd-5", SwitchToAgents, None),
        KeyBinding::new("cmd-6", SwitchToWorkflows, None),
        KeyBinding::new("cmd-7", SwitchToChannels, None),
        KeyBinding::new("cmd-8", SwitchToKanban, None),
        KeyBinding::new("cmd-9", SwitchToMonitor, None),
        KeyBinding::new("cmd-0", SwitchToLogs, None),
    ]);
    #[cfg(not(target_os = "macos"))]
    cx.bind_keys([
        // App-level actions
        KeyBinding::new("ctrl-q", Quit, None),
        KeyBinding::new("ctrl-,", OpenSettings, None),
        KeyBinding::new("ctrl-p", TogglePrivacy, None),
        // Chat actions
        KeyBinding::new("ctrl-n", NewConversation, None),
        KeyBinding::new("ctrl-l", ClearChat, None),
        // Panel switching: ctrl-1..ctrl-0 map to first 10 sidebar panels
        KeyBinding::new("ctrl-1", SwitchToChat, None),
        KeyBinding::new("ctrl-2", SwitchToHistory, None),
        KeyBinding::new("ctrl-3", SwitchToFiles, None),
        KeyBinding::new("ctrl-4", SwitchToSpecs, None),
        KeyBinding::new("ctrl-5", SwitchToAgents, None),
        KeyBinding::new("ctrl-6", SwitchToWorkflows, None),
        KeyBinding::new("ctrl-7", SwitchToChannels, None),
        KeyBinding::new("ctrl-8", SwitchToKanban, None),
        KeyBinding::new("ctrl-9", SwitchToMonitor, None),
        KeyBinding::new("ctrl-0", SwitchToLogs, None),
    ]);

    cx.on_action(|_: &Quit, cx: &mut App| {
        info!("Quit action triggered");
        cx.quit();
    });

    cx.on_action(|_: &OpenSettings, _cx| {
        info!("OpenSettings action triggered");
    });

    cx.on_action(|_: &TogglePrivacy, cx: &mut App| {
        info!("TogglePrivacy action triggered");
        if cx.has_global::<AppConfig>() {
            let current = cx.global::<AppConfig>().0.get().privacy_mode;
            let _ = cx
                .global_mut::<AppConfig>()
                .0
                .update(|c| c.privacy_mode = !current);
            info!("Privacy mode toggled to {}", !current);
        }
    });
}

/// Build the main window options, restoring the saved window size if available.
fn window_options(cx: &App) -> WindowOptions {
    let session = hive_core::session::SessionState::load().unwrap_or_default();
    let (w, h) = match session.window_size {
        Some([w, h]) if w >= 400 && h >= 300 => (w as f32, h as f32),
        _ => (1280.0, 800.0),
    };

    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            None,
            size(px(w), px(h)),
            cx,
        ))),
        titlebar: Some(gpui_component::TitleBar::title_bar_options()),
        ..Default::default()
    }
}

/// Update tray menu toggle text for current window visibility.
fn set_tray_window_visible(cx: &App, visible: bool) {
    if let Some(tray) = cx.global::<AppTray>().0.as_ref() {
        tray.set_visible(visible);
    }
}

/// Close all open windows while keeping the app/tray process alive.
fn hide_all_windows(cx: &mut App) {
    let windows = cx.windows();
    for handle in windows {
        let _ = handle.update(cx, |_, window, _| {
            window.remove_window();
        });
    }
    set_tray_window_visible(cx, false);
}

/// Platform-specific wording for where the background icon lives.
#[cfg(target_os = "macos")]
fn close_to_tray_target() -> &'static str {
    "menu bar"
}

#[cfg(target_os = "windows")]
fn close_to_tray_target() -> &'static str {
    "system tray"
}

#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
fn close_to_tray_target() -> &'static str {
    "tray area"
}

fn handle_main_window_close(window: &mut Window, cx: &mut App) -> bool {
    // If there is no tray icon, just quit directly.
    if cx.global::<AppTray>().0.is_none() {
        return true;
    }

    // Always prompt the user: Quit, Minimize to tray, or Cancel.
    let detail = format!(
        "Would you like to quit Hive or minimize it to the {}?",
        close_to_tray_target()
    );
    let response = window.prompt(
        PromptLevel::Info,
        "Close Hive",
        Some(&detail),
        &["Quit Hive", "Minimize to Tray", "Cancel"],
        cx,
    );

    cx.spawn(async move |app: &mut AsyncApp| {
        if let Ok(choice) = response.await {
            let _ = app.update(|cx| match choice {
                0 => cx.quit(),
                1 => hide_all_windows(cx),
                _ => {} // Cancel — do nothing
            });
        }
    })
    .detach();

    // Return false to veto the platform close; the prompt handles the outcome.
    false
}

/// Open the main application window and wire close-to-tray behavior.
fn open_main_window(cx: &mut App) -> anyhow::Result<()> {
    cx.open_window(window_options(cx), |window, cx| {
        // Keep the app alive for background tasks when the user closes the
        // window (Alt+F4 / titlebar close / platform close request).
        // Returning `false` vetoes the platform close while we remove the
        // window ourselves so the taskbar button disappears.
        window.on_window_should_close(cx, handle_main_window_close);

        let workspace = cx.new(|cx| HiveWorkspace::new(window, cx));

        // Push the git-based version (from build.rs) into the status bar.
        workspace.update(cx, |ws, _cx| {
            ws.set_version(VERSION.to_string());
        });

        cx.subscribe(&workspace, |workspace, event: &SwitchPanel, cx| {
            workspace.update(cx, |ws, cx| {
                ws.set_active_panel(event.0);
                cx.notify();
            });
        })
        .detach();

        cx.new(|cx| gpui_component::Root::new(workspace.clone(), window, cx))
    })?;

    set_tray_window_visible(cx, true);
    info!("Hive v{VERSION} window opened");
    Ok(())
}

/// Post an error notification into the global store.
fn notify_error(cx: &mut App, message: impl Into<String>) {
    if cx.has_global::<AppNotifications>() {
        cx.global_mut::<AppNotifications>().0.push(
            AppNotification::new(NotificationType::Error, message).with_title("Startup Error"),
        );
    }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() {
    let _log_guard = logging::init_logging().expect("Failed to initialize logging");
    info!("Starting Hive v{VERSION}");

    HiveConfig::ensure_dirs().expect("Failed to create config directories");

    Application::new().with_assets(Assets).run(|cx| {
        gpui_component::init(cx);

        if let Err(e) = init_services(cx) {
            error!("Service initialization failed: {e:#}");
            notify_error(cx, format!("Failed to initialize services: {e}"));
        }

        register_actions(cx);

        // Keep tray label synchronized even when windows are closed by means
        // other than the tray event loop.
        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                set_tray_window_visible(cx, false);
            }
        })
        .detach();

        let (tray_tx, tray_rx) = mpsc::channel::<tray::TrayEvent>();

        // System tray — stored as a GPUI global to prevent drop.
        let tray = tray::try_create_tray(move |event| {
            let _ = tray_tx.send(event);
        });
        cx.set_global(AppTray(tray));

        // Poll tray events on the main thread and mutate GPUI state there.
        cx.spawn(async move |app: &mut AsyncApp| {
            loop {
                loop {
                    match tray_rx.try_recv() {
                        Ok(event) => {
                            let _ = app.update(|cx| {
                                info!("Tray event: {event:?}");
                                match event {
                                    tray::TrayEvent::ToggleVisibility => {
                                        if cx.windows().is_empty() {
                                            if let Err(e) = open_main_window(cx) {
                                                error!("Failed to open window from tray: {e:#}");
                                                notify_error(
                                                    cx,
                                                    format!(
                                                        "Failed to open window from tray: {e}"
                                                    ),
                                                );
                                            } else {
                                                cx.activate(true);
                                            }
                                        } else {
                                            hide_all_windows(cx);
                                        }
                                    }
                                    tray::TrayEvent::Quit => cx.quit(),
                                }
                            });
                        }
                        Err(mpsc::TryRecvError::Empty) => break,
                        Err(mpsc::TryRecvError::Disconnected) => return,
                    }
                }

                app.background_executor()
                    .timer(Duration::from_millis(80))
                    .await;
            }
        })
        .detach();

        open_main_window(cx).expect("Failed to open window");

        // UI action bridge polling loop — receives UiActionRequests from MCP
        // tool handlers and dispatches them as GPUI actions on the main window.
        if let Some(action_rx) = cx.global_mut::<UiActionRx>().0.take() {
            cx.spawn(async move |app: &mut AsyncApp| {
                loop {
                    loop {
                        match action_rx.try_recv() {
                            Ok(req) => {
                                let result = app.update(|cx| {
                                    let action = match hive_ui::core_types::action_bridge::make_action(
                                        &req.action_name,
                                        req.params,
                                    ) {
                                        Ok(a) => a,
                                        Err(e) => return Err(e),
                                    };

                                    // Dispatch the action on the first open window.
                                    let windows = cx.windows();
                                    if let Some(handle) = windows.first() {
                                        let _ = handle.update(cx, |_, window, cx| {
                                            window.dispatch_action(action, cx);
                                        });
                                        Ok(serde_json::json!({"dispatched": req.action_name}))
                                    } else {
                                        Err("No open window to dispatch action".to_string())
                                    }
                                }).unwrap_or_else(|e| Err(format!("GPUI update failed: {e}")));

                                let _ = req.response_tx.send(result);
                            }
                            Err(mpsc::TryRecvError::Empty) => break,
                            Err(mpsc::TryRecvError::Disconnected) => return,
                        }
                    }

                    app.background_executor()
                        .timer(Duration::from_millis(50))
                        .await;
                }
            })
            .detach();
            info!("UI action bridge polling loop started");
        }

        // Bring the app to the foreground and ensure macOS shows its dock icon.
        // Without this, running the binary directly (e.g. `cargo run`) may not
        // display the app in the dock.
        cx.activate(true);

        // Background update check — runs 5s after startup and every 4 hours.
        // The blocking HTTP call runs on an OS thread; results are polled on the
        // main thread to update the status bar.
        if cx.has_global::<AppConfig>() && cx.global::<AppConfig>().0.get().auto_update {
            let updater = cx.global::<AppUpdater>().0.clone();
            cx.spawn(async move |app: &mut AsyncApp| {
                // Wait 5 seconds before first check to avoid slowing startup.
                app.background_executor()
                    .timer(Duration::from_secs(5))
                    .await;

                loop {
                    // Run the blocking HTTP check on a background OS thread.
                    let updater_clone = updater.clone();
                    let (tx, rx) = std::sync::mpsc::channel();
                    std::thread::spawn(move || {
                        let result = updater_clone.check_for_updates();
                        let _ = tx.send(result);
                    });

                    // Poll for the result.
                    let check_result = loop {
                        match rx.try_recv() {
                            Ok(result) => break result,
                            Err(std::sync::mpsc::TryRecvError::Empty) => {
                                app.background_executor()
                                    .timer(Duration::from_millis(500))
                                    .await;
                            }
                            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                                break Err(anyhow::anyhow!("Update check thread died"));
                            }
                        }
                    };

                    match check_result {
                        Ok(Some(update_info)) => {
                            info!(
                                "Update available: v{} (release: {})",
                                update_info.version, update_info.release_url
                            );
                            let version = update_info.version.clone();
                            let _ = app.update(|cx| {
                                if cx.has_global::<AppNotifications>() {
                                    cx.global_mut::<AppNotifications>().0.push(
                                        AppNotification::new(
                                            NotificationType::Info,
                                            format!(
                                                "Hive v{version} is available. Click the update badge in the status bar to install."
                                            ),
                                        )
                                        .with_title("Update Available"),
                                    );
                                }
                            });
                        }
                        Ok(None) => {
                            info!("No updates available");
                        }
                        Err(e) => {
                            warn!("Update check failed: {e}");
                        }
                    }

                    // Re-check every 4 hours.
                    app.background_executor()
                        .timer(Duration::from_secs(4 * 60 * 60))
                        .await;
                }
            })
            .detach();
        }
    });
}
