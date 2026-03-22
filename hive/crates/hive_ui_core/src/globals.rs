//! GPUI Global wrappers for backend services.
//!
//! These are defined in `hive_ui` (not `hive_app`) so that both the workspace
//! (which reads them) and the bootstrap code (which sets them) share the same
//! types.  Each wrapper is a newtype around the service it wraps.

use std::sync::{Arc, Mutex};

use gpui::Global;

use crate::theme::HiveTheme;
use hive_a2a::A2aClientService;
use hive_agents::automation::AutomationService;
use hive_agents::collective_memory::CollectiveMemory;
use hive_agents::competence_detection::CompetenceDetector;
use hive_agents::mcp_server::McpServer;
use hive_agents::personas::PersonaRegistry;
use hive_agents::plugin_manager::PluginManager;
use hive_agents::skill_marketplace::SkillMarketplace;
use hive_agents::skills::{SkillManager, SkillsRegistry};
use hive_agents::specs::SpecManager;
use hive_agents::standup::StandupService;
use hive_ai::context_engine::ContextEngine;
use hive_ai::memory::HiveMemory;
use hive_ai::quick_index::QuickIndex;
use hive_ai::rag::RagService;
use hive_ai::semantic_search::SemanticSearchService;
use hive_ai::service::AiService;
use hive_ai::tts::service::TtsService;
use hive_assistant::AssistantService;
use hive_blockchain::rpc_config::RpcConfigStore;
use hive_blockchain::wallet_store::WalletStore;
use hive_core::channels::ChannelStore;
use hive_core::config::ConfigManager;
use hive_core::notifications::NotificationStore;
use hive_core::persistence::Database;
use hive_core::scheduler::Scheduler;
use hive_core::security::SecurityGateway;
use hive_core::updater::UpdateService;
use hive_integrations::bitbucket::BitbucketClient;
use hive_integrations::browser::BrowserAutomation;
use hive_integrations::cloud::{AwsClient, AzureClient, GcpClient};
use hive_integrations::database::DatabaseHub;
use hive_integrations::docker::DockerClient;
use hive_integrations::docs_indexer::DocsIndexer;
use hive_integrations::gitlab::GitLabClient;
use hive_integrations::ide::IdeIntegrationService;
use hive_integrations::knowledge::KnowledgeHub;
use hive_integrations::kubernetes::KubernetesClient;
use hive_integrations::messaging::CrossChannelService;
use hive_integrations::messaging::MessagingHub;
use hive_integrations::project_management::ProjectManagementHub;
use hive_integrations::smart_home::PhilipsHueClient;
use hive_learn::LearningService;
use hive_network::HiveNodeHandle;
use hive_shield::HiveShield;
use hive_terminal::CliService;
use hive_terminal::local_ai::OllamaManager;

/// Global wrapper for the AI service (providers, routing, cost tracking).
pub struct AppAiService(pub AiService);
impl Global for AppAiService {}

/// Global wrapper for the configuration manager (hot-reload, read/write).
pub struct AppConfig(pub ConfigManager);
impl Global for AppConfig {}

/// Global wrapper for the SQLite database (conversations, memory, costs).
pub struct AppDatabase(pub Database);
impl Global for AppDatabase {}

/// Global wrapper for in-app notification storage.
pub struct AppNotifications(pub NotificationStore);
impl Global for AppNotifications {}

/// Global wrapper for the security gateway (command/URL/path validation).
pub struct AppSecurity(pub SecurityGateway);
impl Global for AppSecurity {}

/// Global wrapper for the learning service (outcome tracking, routing adjustments).
pub struct AppLearning(pub Arc<LearningService>);
impl Global for AppLearning {}

/// Global wrapper for the privacy/security shield (PII, secrets, threats).
pub struct AppShield(pub Arc<HiveShield>);
impl Global for AppShield {}

/// Global wrapper for the TTS service (voice synthesis, provider routing).
pub struct AppTts(pub Arc<TtsService>);
impl Global for AppTts {}

/// Global wrapper for the skills registry (/command dispatch, built-in skills).
pub struct AppSkills(pub SkillsRegistry);
impl Global for AppSkills {}

/// Global wrapper for the file-based skill manager (user-created skills).
pub struct AppSkillManager(pub SkillManager);
impl Global for AppSkillManager {}

/// Global wrapper for the skill marketplace (install/remove, security scanning).
pub struct AppMarketplace(pub SkillMarketplace);
impl Global for AppMarketplace {}

/// Global wrapper for the plugin manager (fetch/parse/version-check external plugins).
pub struct AppPluginManager(pub PluginManager);
impl Global for AppPluginManager {}

/// Global wrapper for the built-in MCP tool server.
pub struct AppMcpServer(pub McpServer);
impl Global for AppMcpServer {}

/// Global wrapper for the persona registry (agent roles + custom personas).
pub struct AppPersonas(pub PersonaRegistry);
impl Global for AppPersonas {}

/// Global wrapper for the automation service (workflow engine).
pub struct AppAutomation(pub AutomationService);
impl Global for AppAutomation {}

/// Global wrapper for the spec manager (project specifications).
pub struct AppSpecs(pub SpecManager);
impl Global for AppSpecs {}

/// Global wrapper for the CLI service (built-in commands, doctor checks).
pub struct AppCli(pub CliService);
impl Global for AppCli {}

/// Global wrapper for the assistant service (email, calendar, reminders).
pub struct AppAssistant(pub AssistantService);
impl Global for AppAssistant {}

/// Global wrapper for the voice assistant service (text-to-intent classification).
pub struct AppVoiceAssistant(pub std::sync::Arc<std::sync::Mutex<hive_agents::VoiceAssistant>>);
impl Global for AppVoiceAssistant {}

/// Global wrapper for the cron-based task scheduler.
///
/// Wrapped in `Arc<Mutex<_>>` so the background tick driver thread can call
/// `Scheduler::tick()` while the GPUI main thread retains read/write access
/// for adding and removing jobs.
pub struct AppScheduler(pub Arc<Mutex<Scheduler>>);
impl Global for AppScheduler {}

/// Global wrapper for the wallet store (blockchain accounts).
pub struct AppWallets(pub WalletStore);
impl Global for AppWallets {}

/// Global wrapper for blockchain RPC endpoint configuration.
pub struct AppRpcConfig(pub RpcConfigStore);
impl Global for AppRpcConfig {}

/// Global wrapper for IDE integration (diagnostics, symbols, workspace info).
pub struct AppIde(pub IdeIntegrationService);
impl Global for AppIde {}

/// Global wrapper for the AI agent channel store (persistent messaging channels).
pub struct AppChannels(pub ChannelStore);
impl Global for AppChannels {}

/// Global wrapper for the live P2P network query handle.
///
/// The running node lives on a background runtime. `AppNetwork` exposes a
/// shared read-only handle so the UI can inspect the real peer registry.
pub struct AppNetwork(pub Arc<HiveNodeHandle>);
impl Global for AppNetwork {}

/// Global wrapper for the messaging hub (Slack, Discord, Teams, etc.).
pub struct AppMessaging(pub Arc<MessagingHub>);
impl Global for AppMessaging {}

/// Global wrapper for the cross-channel memory service (channel/thread linking,
/// conversation tracking, unified search across messaging platforms).
pub struct AppCrossChannel(pub Arc<Mutex<CrossChannelService>>);
impl Global for AppCrossChannel {}

/// Global wrapper for project management (Jira, Linear, Asana).
pub struct AppProjectManagement(pub Arc<ProjectManagementHub>);
impl Global for AppProjectManagement {}

/// Global wrapper for knowledge bases (Notion, Obsidian).
pub struct AppKnowledge(pub Arc<KnowledgeHub>);
impl Global for AppKnowledge {}

/// Global wrapper for database integrations (Postgres, MySQL, SQLite).
pub struct AppIntegrationDb(pub Arc<DatabaseHub>);
impl Global for AppIntegrationDb {}

/// Global wrapper for Docker integration.
pub struct AppDocker(pub Arc<DockerClient>);
impl Global for AppDocker {}

/// Global wrapper for Kubernetes integration.
pub struct AppKubernetes(pub Arc<KubernetesClient>);
impl Global for AppKubernetes {}

/// Global wrapper for browser automation.
pub struct AppBrowser(pub Arc<BrowserAutomation>);
impl Global for AppBrowser {}

/// Global wrapper for outbound A2A client operations.
pub struct AppA2aClient(pub Arc<A2aClientService>);
impl Global for AppA2aClient {}

/// Global wrapper for Ollama model management.
pub struct AppOllamaManager(pub Arc<OllamaManager>);
impl Global for AppOllamaManager {}

/// Global wrapper for Philips Hue smart-home integration.
pub struct AppHueClient(pub Option<Arc<PhilipsHueClient>>);
impl Global for AppHueClient {}

/// Global wrapper for Bitbucket integration.
pub struct AppBitbucket(pub Arc<BitbucketClient>);
impl Global for AppBitbucket {}

/// Global wrapper for GitLab integration.
pub struct AppGitLab(pub Arc<GitLabClient>);
impl Global for AppGitLab {}

/// Global wrapper for AWS cloud integration.
pub struct AppAws(pub Arc<AwsClient>);
impl Global for AppAws {}

/// Global wrapper for Azure cloud integration.
pub struct AppAzure(pub Arc<AzureClient>);
impl Global for AppAzure {}

/// Global wrapper for GCP cloud integration.
pub struct AppGcp(pub Arc<GcpClient>);
impl Global for AppGcp {}

/// Global wrapper for documentation indexer.
pub struct AppDocsIndexer(pub Arc<DocsIndexer>);
impl Global for AppDocsIndexer {}

/// Global wrapper for the auto-update service (version check, binary replacement).
pub struct AppUpdater(pub UpdateService);
impl Global for AppUpdater {}

/// Global wrapper for fleet learning (cross-instance pattern detection).
pub struct AppFleetLearning(pub Arc<Mutex<hive_ai::FleetLearningService>>);
impl Global for AppFleetLearning {}

/// Global wrapper for the active application theme.
///
/// Set during workspace initialization from the config-driven theme resolution.
/// Updated when the user switches themes via the Settings panel.
pub struct AppTheme(pub HiveTheme);
impl Global for AppTheme {}

/// Global wrapper for RAG service (document indexing + retrieval).
pub struct AppRagService(pub Arc<Mutex<RagService>>);
impl Global for AppRagService {}

/// Global wrapper for semantic search service.
pub struct AppSemanticSearch(pub Arc<Mutex<SemanticSearchService>>);
impl Global for AppSemanticSearch {}

/// Global wrapper for context curation engine.
pub struct AppContextEngine(pub Arc<Mutex<ContextEngine>>);
impl Global for AppContextEngine {}

/// Global wrapper for collective memory.
///
/// `CollectiveMemory` is internally synchronised (`Mutex<Connection>`) so no
/// outer `Mutex` is needed — just share the `Arc` directly.
pub struct AppCollectiveMemory(pub Arc<CollectiveMemory>);
impl Global for AppCollectiveMemory {}

/// Global wrapper for standup service.
pub struct AppStandupService(pub Arc<Mutex<StandupService>>);
impl Global for AppStandupService {}

/// Global wrapper for competence detector.
pub struct AppCompetenceDetector(pub Arc<Mutex<CompetenceDetector>>);
impl Global for AppCompetenceDetector {}

/// Global wrapper for HiveMemory (LanceDB-backed vector embeddings + chunking).
/// Uses `tokio::sync::Mutex` so the guard can be held across `.await` points
/// (HiveMemory's methods are all async).
pub struct AppHiveMemory(pub Arc<tokio::sync::Mutex<HiveMemory>>);
impl Global for AppHiveMemory {}

/// Global wrapper for project knowledge files (HIVE.md, README.md, etc.).
///
/// Populated by `KnowledgeFileScanner::scan()` when the workspace opens or
/// switches project roots. Re-scanned on each chat message for freshness.
pub struct AppKnowledgeFiles(pub Vec<hive_ai::knowledge_files::KnowledgeSource>);
impl Global for AppKnowledgeFiles {}

/// Global wrapper for the fast-path project index.
///
/// Built synchronously on project open (<3 seconds). Provides immediate
/// project context (file tree, symbols, deps, git log) for AI queries
/// while the deeper RAG/vector index runs in the background.
pub struct AppQuickIndex(pub Arc<QuickIndex>);
impl Global for AppQuickIndex {}

/// State tracking user-selected files/symbols for AI context attachment.
///
/// Updated when files are checked/unchecked in the Files panel.
/// Read by `ChatInputView` to show context chips and by the workspace
/// to inject selected file contents into the AI request.
pub struct ContextSelectionState {
    /// Absolute paths of files selected for context.
    pub selected_files: Vec<std::path::PathBuf>,
    /// Estimated total tokens across all selected files.
    pub total_tokens: usize,
}

impl Default for ContextSelectionState {
    fn default() -> Self {
        Self {
            selected_files: Vec::new(),
            total_tokens: 0,
        }
    }
}

/// Global wrapper for context selection state (files/symbols attached to chat).
pub struct AppContextSelection(pub Arc<Mutex<ContextSelectionState>>);
impl Global for AppContextSelection {}

/// Global wrapper for the UI action bridge sender.
///
/// MCP tool handlers clone this sender to dispatch UI actions across the
/// channel to the main GPUI thread.  Stored as `Arc` so clones are cheap
/// and the sender can be captured by `Send + Sync` closures.
pub struct AppUiActionTx(pub Arc<std::sync::mpsc::Sender<crate::action_bridge::UiActionRequest>>);
impl Global for AppUiActionTx {}

/// Global wrapper for the reminder notification receiver (fed by tick driver).
///
/// The tick driver sends `Vec<TriggeredReminder>` over this channel whenever
/// reminders fire.  The workspace drains it in `sync_status_bar()` to push
/// in-app notifications.
pub struct AppReminderRx(
    pub Arc<Mutex<std::sync::mpsc::Receiver<Vec<hive_assistant::reminders::TriggeredReminder>>>>,
);
impl Global for AppReminderRx {}

/// Global wrapper for the activity event bus (agent events, cost tracking, persistence).
pub struct AppActivityService(pub Arc<hive_agents::ActivityService>);
impl Global for AppActivityService {}

/// Global wrapper for the agent notification service (approval requests, budget warnings).
pub struct AppAgentNotifications(pub Arc<hive_agents::NotificationService>);
impl Global for AppAgentNotifications {}

/// Global wrapper for the heartbeat scheduler (periodic agent health checks).
pub struct AppHeartbeatScheduler(pub Arc<hive_agents::HeartbeatScheduler>);
impl Global for AppHeartbeatScheduler {}

/// Global wrapper for the approval gate (rule-based operation approval for agents).
pub struct AppApprovalGate(pub Arc<hive_agents::ApprovalGate>);
impl Global for AppApprovalGate {}

/// Global wrapper for detected local AI providers (Ollama, LM Studio, etc.).
///
/// Populated asynchronously by the `local-ai-detect` background thread.
/// Initially set to an empty vec; updated once detection completes.
/// Uses `Arc<Mutex<_>>` so the background thread can write results directly.
pub struct AppLocalAiDetection(pub Arc<Mutex<Vec<hive_terminal::local_ai::LocalProviderInfo>>>);
impl Global for AppLocalAiDetection {}
