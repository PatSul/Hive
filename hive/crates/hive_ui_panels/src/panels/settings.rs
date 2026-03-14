use std::collections::HashSet;

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::switch::Switch;
use gpui_component::{Icon, IconName};
use hive_ai::types::ProviderType;
use hive_integrations::smart_home::{HueBridge, HueLight, HueScene, PhilipsHueClient};
use hive_terminal::local_ai::PullProgress;

use crate::components::model_selector::{ModelSelected, ModelSelectorView};
use hive_core::theme_manager::ThemeManager;
use hive_ui_core::{AppConfig, AppHueClient, AppOllamaManager};
use hive_ui_core::{AppTheme, ContextFormatChanged, HiveTheme, ThemeChanged};
use hive_ui_core::{AccountConnectPlatform, ExportConfig, ImportConfig};

// ---------------------------------------------------------------------------
// Actions
// ---------------------------------------------------------------------------

actions!(
    hive_settings,
    [
        SettingsTogglePrivacy,
        SettingsToggleAutoRouting,
        SettingsToggleAutoUpdate,
        SettingsToggleNotifications,
        SettingsToggleTts,
        SettingsToggleTtsAutoSpeak,
        SettingsToggleClawdTalk,
        SettingsToggleSpeculativeDecoding,
        SettingsToggleSpeculativeMetrics,
    ]
);

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

/// Emitted when any setting changes. The workspace subscribes to this and
/// persists the values to `AppConfig`.
#[derive(Debug, Clone)]
pub struct SettingsSaved;

// ---------------------------------------------------------------------------
// SettingsData -- read-only snapshot for other panels
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct SettingsData {
    pub has_anthropic_key: bool,
    pub has_openai_key: bool,
    pub has_openrouter_key: bool,
    pub has_google_key: bool,
    pub has_groq_key: bool,
    pub has_xai_key: bool,
    pub has_huggingface_key: bool,
    pub has_litellm_key: bool,
    pub has_hue_key: bool,
    pub ollama_url: String,
    pub lmstudio_url: String,
    pub local_provider_url: Option<String>,
    pub hue_bridge_ip: Option<String>,
    pub privacy_mode: bool,
    pub default_model: String,
    pub auto_routing: bool,
    pub speculative_decoding: bool,
    pub speculative_show_metrics: bool,
    pub daily_budget_usd: f64,
    pub monthly_budget_usd: f64,
    pub theme: String,
    pub font_size: u32,
    pub auto_update: bool,
    pub notifications_enabled: bool,
    pub log_level: String,
    // TTS
    pub has_elevenlabs_key: bool,
    pub has_telnyx_key: bool,
    pub tts_enabled: bool,
    pub tts_auto_speak: bool,
    pub tts_provider: String,
    pub tts_speed: f32,
    pub clawdtalk_enabled: bool,
    // Cloud account
    pub cloud_logged_in: bool,
    pub cloud_email: Option<String>,
    pub cloud_tier: Option<String>,
    pub cloud_token_used: Option<i64>,
    pub cloud_token_budget: Option<i64>,
    pub cloud_sync_last: Option<String>,
    pub cloud_relay_connected: bool,
}

impl Default for SettingsData {
    fn default() -> Self {
        Self {
            has_anthropic_key: false,
            has_openai_key: false,
            has_openrouter_key: false,
            has_google_key: false,
            has_groq_key: false,
            has_xai_key: false,
            has_huggingface_key: false,
            has_litellm_key: false,
            has_hue_key: false,
            ollama_url: "http://localhost:11434".into(),
            lmstudio_url: "http://localhost:1234".into(),
            local_provider_url: None,
            hue_bridge_ip: None,
            privacy_mode: false,
            default_model: String::new(),
            auto_routing: true,
            speculative_decoding: false,
            speculative_show_metrics: true,
            daily_budget_usd: 10.0,
            monthly_budget_usd: 100.0,
            theme: "HiveCode Dark".into(),
            font_size: 14,
            auto_update: true,
            notifications_enabled: true,
            log_level: "info".into(),
            has_elevenlabs_key: false,
            has_telnyx_key: false,
            tts_enabled: false,
            tts_auto_speak: false,
            tts_provider: "qwen3".into(),
            tts_speed: 1.0,
            clawdtalk_enabled: false,
            cloud_logged_in: false,
            cloud_email: None,
            cloud_tier: None,
            cloud_token_used: None,
            cloud_token_budget: None,
            cloud_sync_last: None,
            cloud_relay_connected: false,
        }
    }
}

impl Global for SettingsData {}

impl SettingsData {
    pub fn from_config(cfg: &hive_core::HiveConfig) -> Self {
        Self {
            has_anthropic_key: cfg
                .anthropic_api_key
                .as_ref()
                .is_some_and(|k| !k.is_empty()),
            has_openai_key: cfg.openai_api_key.as_ref().is_some_and(|k| !k.is_empty()),
            has_openrouter_key: cfg
                .openrouter_api_key
                .as_ref()
                .is_some_and(|k| !k.is_empty()),
            has_google_key: cfg.google_api_key.as_ref().is_some_and(|k| !k.is_empty()),
            has_groq_key: cfg.groq_api_key.as_ref().is_some_and(|k| !k.is_empty()),
            has_xai_key: cfg.xai_api_key.as_ref().is_some_and(|k| !k.is_empty()),
            has_huggingface_key: cfg
                .huggingface_api_key
                .as_ref()
                .is_some_and(|k| !k.is_empty()),
            has_litellm_key: cfg
                .litellm_api_key
                .as_ref()
                .is_some_and(|k| !k.is_empty()),
            has_hue_key: cfg.hue_api_key.as_ref().is_some_and(|k| !k.is_empty()),
            ollama_url: cfg.ollama_url.clone(),
            lmstudio_url: cfg.lmstudio_url.clone(),
            local_provider_url: cfg.local_provider_url.clone(),
            hue_bridge_ip: cfg.hue_bridge_ip.clone(),
            privacy_mode: cfg.privacy_mode,
            default_model: cfg.default_model.clone(),
            auto_routing: cfg.auto_routing,
            speculative_decoding: cfg.speculative_decoding,
            speculative_show_metrics: cfg.speculative_show_metrics,
            daily_budget_usd: cfg.daily_budget_usd,
            monthly_budget_usd: cfg.monthly_budget_usd,
            theme: cfg.theme.clone(),
            font_size: cfg.font_size,
            auto_update: cfg.auto_update,
            notifications_enabled: cfg.notifications_enabled,
            log_level: cfg.log_level.clone(),
            has_elevenlabs_key: cfg
                .elevenlabs_api_key
                .as_ref()
                .is_some_and(|k| !k.is_empty()),
            has_telnyx_key: cfg.telnyx_api_key.as_ref().is_some_and(|k| !k.is_empty()),
            tts_enabled: cfg.tts_enabled,
            tts_auto_speak: cfg.tts_auto_speak,
            tts_provider: cfg.tts_provider.clone(),
            tts_speed: cfg.tts_speed,
            clawdtalk_enabled: cfg.clawdtalk_enabled,
            cloud_logged_in: cfg.cloud_jwt.as_ref().is_some_and(|t| !t.is_empty()),
            cloud_email: None,
            cloud_tier: cfg.cloud_tier.clone(),
            cloud_token_used: None,
            cloud_token_budget: None,
            cloud_sync_last: None,
            cloud_relay_connected: false,
        }
    }

    pub fn configured_key_count(&self) -> usize {
        [
            self.has_anthropic_key,
            self.has_openai_key,
            self.has_openrouter_key,
            self.has_google_key,
            self.has_groq_key,
            self.has_xai_key,
            self.has_huggingface_key,
            self.has_litellm_key,
        ]
        .iter()
        .filter(|&&v| v)
        .count()
    }

    pub fn has_any_cloud_key(&self) -> bool {
        self.configured_key_count() > 0
    }
}

impl From<&hive_core::HiveConfig> for SettingsData {
    fn from(cfg: &hive_core::HiveConfig) -> Self {
        Self::from_config(cfg)
    }
}

#[derive(Debug, Clone)]
struct ManagedOllamaModel {
    name: String,
    size: Option<u64>,
    modified_at: Option<String>,
}

// ---------------------------------------------------------------------------
// SettingsView -- interactive entity
// ---------------------------------------------------------------------------

/// Interactive settings panel backed by real GPUI input widgets.
/// Auto-saves on every blur (focus-out) from text inputs and on every toggle.
pub struct SettingsView {
    theme: HiveTheme,

    // API key inputs (masked)
    anthropic_key_input: Entity<InputState>,
    openai_key_input: Entity<InputState>,
    openrouter_key_input: Entity<InputState>,
    google_key_input: Entity<InputState>,
    groq_key_input: Entity<InputState>,
    xai_key_input: Entity<InputState>,
    huggingface_key_input: Entity<InputState>,

    // LiteLLM inputs
    litellm_key_input: Entity<InputState>,
    litellm_url_input: Entity<InputState>,

    // URL inputs
    ollama_url_input: Entity<InputState>,
    ollama_pull_model_input: Entity<InputState>,
    lmstudio_url_input: Entity<InputState>,
    custom_url_input: Entity<InputState>,
    hue_bridge_ip_input: Entity<InputState>,
    hue_api_key_input: Entity<InputState>,

    // Model selector
    model_selector: Entity<ModelSelectorView>,

    // Budget inputs
    daily_budget_input: Entity<InputState>,
    monthly_budget_input: Entity<InputState>,

    // Toggle states
    privacy_mode: bool,
    auto_routing: bool,
    speculative_decoding: bool,
    speculative_show_metrics: bool,
    auto_update: bool,
    notifications_enabled: bool,

    // TTS key inputs
    elevenlabs_key_input: Entity<InputState>,
    telnyx_key_input: Entity<InputState>,

    // TTS toggles
    tts_enabled: bool,
    tts_auto_speak: bool,
    clawdtalk_enabled: bool,

    // Track whether keys existed before editing (to preserve on empty save)
    had_anthropic_key: bool,
    had_openai_key: bool,
    had_openrouter_key: bool,
    had_google_key: bool,
    had_groq_key: bool,
    had_xai_key: bool,
    had_huggingface_key: bool,
    had_litellm_key: bool,
    had_elevenlabs_key: bool,
    had_telnyx_key: bool,
    had_hue_key: bool,

    // Discovery status
    discovered_model_count: usize,
    ollama_models: Vec<ManagedOllamaModel>,
    ollama_status: Option<String>,
    ollama_busy: bool,
    ollama_inspect_summary: Option<String>,
    hue_bridges: Vec<HueBridge>,
    hue_lights: Vec<HueLight>,
    hue_scenes: Vec<HueScene>,
    hue_status: Option<String>,
    hue_busy: bool,

    // OAuth client ID inputs per platform
    google_client_id_input: Entity<InputState>,
    microsoft_client_id_input: Entity<InputState>,
    github_client_id_input: Entity<InputState>,
    slack_client_id_input: Entity<InputState>,
    discord_client_id_input: Entity<InputState>,
    telegram_client_id_input: Entity<InputState>,

    // Theme picker
    selected_theme: String,
    available_themes: Vec<String>,

    // Context format picker
    selected_context_format: String,

    // Focus handle — required so dispatch_action reaches our on_action handlers
    focus_handle: FocusHandle,
}

impl EventEmitter<SettingsSaved> for SettingsView {}

impl SettingsView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        // Read current config
        let cfg = if cx.has_global::<AppConfig>() {
            cx.global::<AppConfig>().0.get()
        } else {
            hive_core::HiveConfig::default()
        };

        let had_anthropic = cfg
            .anthropic_api_key
            .as_ref()
            .is_some_and(|k| !k.is_empty());
        let had_openai = cfg.openai_api_key.as_ref().is_some_and(|k| !k.is_empty());
        let had_openrouter = cfg
            .openrouter_api_key
            .as_ref()
            .is_some_and(|k| !k.is_empty());
        let had_google = cfg.google_api_key.as_ref().is_some_and(|k| !k.is_empty());
        let had_groq = cfg.groq_api_key.as_ref().is_some_and(|k| !k.is_empty());
        let had_xai = cfg.xai_api_key.as_ref().is_some_and(|k| !k.is_empty());
        let had_huggingface = cfg
            .huggingface_api_key
            .as_ref()
            .is_some_and(|k| !k.is_empty());
        let had_litellm = cfg
            .litellm_api_key
            .as_ref()
            .is_some_and(|k| !k.is_empty());
        let had_elevenlabs = cfg
            .elevenlabs_api_key
            .as_ref()
            .is_some_and(|k| !k.is_empty());
        let had_telnyx = cfg.telnyx_api_key.as_ref().is_some_and(|k| !k.is_empty());
        let had_hue = cfg.hue_api_key.as_ref().is_some_and(|k| !k.is_empty());

        // API key inputs — always start empty, placeholder indicates status
        let anthropic_key_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_placeholder(key_placeholder(had_anthropic), window, cx);
            state
        });
        let openai_key_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_placeholder(key_placeholder(had_openai), window, cx);
            state
        });
        let openrouter_key_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_placeholder(key_placeholder(had_openrouter), window, cx);
            state
        });
        let google_key_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_placeholder(key_placeholder(had_google), window, cx);
            state
        });

        // Groq + xAI + HuggingFace key inputs
        let groq_key_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_placeholder(key_placeholder(had_groq), window, cx);
            state
        });
        let xai_key_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_placeholder(key_placeholder(had_xai), window, cx);
            state
        });
        let huggingface_key_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_placeholder(key_placeholder(had_huggingface), window, cx);
            state
        });

        // LiteLLM inputs
        let litellm_key_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_placeholder(key_placeholder(had_litellm), window, cx);
            state
        });
        let litellm_url_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_placeholder("http://localhost:4000", window, cx);
            if let Some(ref url) = cfg.litellm_url {
                state.set_value(url.clone(), window, cx);
            }
            state
        });

        // TTS key inputs
        let elevenlabs_key_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_placeholder(key_placeholder(had_elevenlabs), window, cx);
            state
        });
        let telnyx_key_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_placeholder(key_placeholder(had_telnyx), window, cx);
            state
        });

        // URL inputs — pre-filled with current values
        let ollama_url_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_placeholder("http://localhost:11434", window, cx);
            state.set_value(cfg.ollama_url.clone(), window, cx);
            state
        });
        let ollama_pull_model_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_placeholder("llama3.2:latest", window, cx);
            state
        });
        let lmstudio_url_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_placeholder("http://localhost:1234", window, cx);
            state.set_value(cfg.lmstudio_url.clone(), window, cx);
            state
        });
        let custom_url_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_placeholder("Custom provider URL (optional)", window, cx);
            if let Some(ref url) = cfg.local_provider_url {
                state.set_value(url.clone(), window, cx);
            }
            state
        });
        let hue_bridge_ip_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_placeholder("192.168.x.x", window, cx);
            if let Some(ref ip) = cfg.hue_bridge_ip {
                state.set_value(ip.clone(), window, cx);
            }
            state
        });
        let hue_api_key_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_placeholder(key_placeholder(had_hue), window, cx);
            state
        });

        // Model selector dropdown
        let model_selector =
            cx.new(|cx| ModelSelectorView::new(cfg.default_model.clone(), window, cx));

        // Budget inputs
        let daily_budget_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_placeholder("0.00", window, cx);
            state.set_value(format!("{:.2}", cfg.daily_budget_usd), window, cx);
            state
        });
        let monthly_budget_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_placeholder("0.00", window, cx);
            state.set_value(format!("{:.2}", cfg.monthly_budget_usd), window, cx);
            state
        });

        // OAuth client ID inputs per platform
        let google_client_id_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_placeholder("Google OAuth Client ID", window, cx);
            if let Some(ref val) = cfg.google_oauth_client_id {
                state.set_value(val.clone(), window, cx);
            }
            state
        });
        let microsoft_client_id_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_placeholder("Microsoft OAuth Client ID", window, cx);
            if let Some(ref val) = cfg.microsoft_oauth_client_id {
                state.set_value(val.clone(), window, cx);
            }
            state
        });
        let github_client_id_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_placeholder("GitHub OAuth Client ID", window, cx);
            if let Some(ref val) = cfg.github_oauth_client_id {
                state.set_value(val.clone(), window, cx);
            }
            state
        });
        let slack_client_id_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_placeholder("Slack OAuth Client ID", window, cx);
            if let Some(ref val) = cfg.slack_oauth_client_id {
                state.set_value(val.clone(), window, cx);
            }
            state
        });
        let discord_client_id_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_placeholder("Discord OAuth Client ID", window, cx);
            if let Some(ref val) = cfg.discord_oauth_client_id {
                state.set_value(val.clone(), window, cx);
            }
            state
        });
        let telegram_client_id_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_placeholder("Telegram Bot Token", window, cx);
            if let Some(ref val) = cfg.telegram_oauth_client_id {
                state.set_value(val.clone(), window, cx);
            }
            state
        });

        // Subscribe to blur events on all text inputs → auto-save
        let all_inputs = [
            &anthropic_key_input,
            &openai_key_input,
            &openrouter_key_input,
            &google_key_input,
            &groq_key_input,
            &xai_key_input,
            &huggingface_key_input,
            &litellm_key_input,
            &litellm_url_input,
            &elevenlabs_key_input,
            &telnyx_key_input,
            &ollama_url_input,
            &hue_bridge_ip_input,
            &hue_api_key_input,
            &lmstudio_url_input,
            &custom_url_input,
            &daily_budget_input,
            &monthly_budget_input,
            &google_client_id_input,
            &microsoft_client_id_input,
            &github_client_id_input,
            &slack_client_id_input,
            &discord_client_id_input,
            &telegram_client_id_input,
        ];
        for input in all_inputs {
            cx.subscribe_in(input, window, Self::on_input_event)
                .detach();
        }

        // Subscribe to model selector → auto-save on pick
        cx.subscribe_in(&model_selector, window, Self::on_model_selected)
            .detach();

        let theme = if cx.has_global::<AppTheme>() {
            cx.global::<AppTheme>().0.clone()
        } else {
            HiveTheme::dark()
        };

        // Build available theme list from built-in + custom.
        let mut available_themes: Vec<String> = ThemeManager::builtin_themes()
            .iter()
            .map(|t| t.name.clone())
            .collect();
        if let Ok(mgr) = ThemeManager::new() {
            for t in mgr.list_custom_themes() {
                if !available_themes.iter().any(|n| n.to_lowercase() == t.name.to_lowercase()) {
                    available_themes.push(t.name.clone());
                }
            }
        }

        let selected_theme = cfg.theme.clone();
        let selected_context_format = if cfg.context_format.is_empty() {
            "markdown".to_string()
        } else {
            cfg.context_format.clone()
        };
        let focus_handle = cx.focus_handle();

        let view = Self {
            theme,
            anthropic_key_input,
            openai_key_input,
            openrouter_key_input,
            google_key_input,
            groq_key_input,
            xai_key_input,
            huggingface_key_input,
            litellm_key_input,
            litellm_url_input,
            ollama_url_input,
            ollama_pull_model_input,
            lmstudio_url_input,
            custom_url_input,
            hue_bridge_ip_input,
            hue_api_key_input,
            model_selector,
            daily_budget_input,
            monthly_budget_input,
            privacy_mode: cfg.privacy_mode,
            auto_routing: cfg.auto_routing,
            speculative_decoding: cfg.speculative_decoding,
            speculative_show_metrics: cfg.speculative_show_metrics,
            auto_update: cfg.auto_update,
            notifications_enabled: cfg.notifications_enabled,
            elevenlabs_key_input,
            telnyx_key_input,
            tts_enabled: cfg.tts_enabled,
            tts_auto_speak: cfg.tts_auto_speak,
            clawdtalk_enabled: cfg.clawdtalk_enabled,
            had_anthropic_key: had_anthropic,
            had_openai_key: had_openai,
            had_openrouter_key: had_openrouter,
            had_google_key: had_google,
            had_groq_key: had_groq,
            had_xai_key: had_xai,
            had_huggingface_key: had_huggingface,
            had_litellm_key: had_litellm,
            had_elevenlabs_key: had_elevenlabs,
            had_telnyx_key: had_telnyx,
            had_hue_key: had_hue,
            discovered_model_count: 0,
            ollama_models: Vec::new(),
            ollama_status: None,
            ollama_busy: false,
            ollama_inspect_summary: None,
            hue_bridges: Vec::new(),
            hue_lights: Vec::new(),
            hue_scenes: Vec::new(),
            hue_status: None,
            hue_busy: false,
            google_client_id_input,
            microsoft_client_id_input,
            github_client_id_input,
            slack_client_id_input,
            discord_client_id_input,
            telegram_client_id_input,
            selected_theme,
            available_themes,
            selected_context_format,
            focus_handle,
        };

        // Initialize model selector with current provider availability
        view.sync_enabled_providers(cx);

        view
    }

    /// Return the focus handle so the workspace can focus this view.
    pub fn focus_handle(&self) -> &FocusHandle {
        &self.focus_handle
    }

    /// Replace the cached theme and trigger a re-render.
    pub fn set_theme(&mut self, theme: HiveTheme, cx: &mut Context<Self>) {
        self.theme = theme;
        cx.notify();
    }

    /// Update the selected theme name (called from the workspace after
    /// ThemeChanged resolves). Keeps the picker highlight in sync.
    pub fn set_selected_theme(&mut self, name: String, cx: &mut Context<Self>) {
        self.selected_theme = name;
        cx.notify();
    }

    /// Update the selected context format (called from the workspace after
    /// ContextFormatChanged resolves).
    pub fn set_selected_context_format(&mut self, format: String, cx: &mut Context<Self>) {
        self.selected_context_format = format;
        cx.notify();
    }

    fn refresh_ollama_models(&mut self, cx: &mut Context<Self>) {
        if !cx.has_global::<AppOllamaManager>() {
            self.ollama_status = Some("Ollama manager is unavailable".into());
            cx.notify();
            return;
        }

        self.ollama_busy = true;
        self.ollama_status = Some("Refreshing Ollama models...".into());
        let manager = cx.global::<AppOllamaManager>().0.clone();
        let (tx, rx) = tokio::sync::oneshot::channel();

        std::thread::spawn(move || {
            let result = match tokio::runtime::Runtime::new() {
                Ok(rt) => rt
                    .block_on(manager.list_models())
                    .map(|models| {
                        models
                            .into_iter()
                            .map(|model| ManagedOllamaModel {
                                name: model.name,
                                size: model.size,
                                modified_at: model.modified_at,
                            })
                            .collect::<Vec<_>>()
                    })
                    .map_err(|e| e.to_string()),
                Err(e) => Err(format!("tokio runtime: {e}")),
            };
            let _ = tx.send(result);
        });

        cx.spawn(async move |this, app: &mut AsyncApp| {
            let result = rx.await.unwrap_or(Err("channel closed".into()));
            let _ = this.update(app, |this, cx| {
                this.ollama_busy = false;
                match result {
                    Ok(models) => {
                        let count = models.len();
                        this.ollama_models = models;
                        this.ollama_status = Some(format!(
                            "Loaded {count} Ollama model{}",
                            if count == 1 { "" } else { "s" }
                        ));
                    }
                    Err(e) => {
                        this.ollama_status = Some(format!("Failed to list Ollama models: {e}"));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn pull_ollama_model(&mut self, model_name: String, cx: &mut Context<Self>) {
        let model_name = model_name.trim().to_string();
        if model_name.is_empty() {
            self.ollama_status = Some("Enter a model name to pull".into());
            cx.notify();
            return;
        }
        if !cx.has_global::<AppOllamaManager>() {
            self.ollama_status = Some("Ollama manager is unavailable".into());
            cx.notify();
            return;
        }

        self.ollama_busy = true;
        self.ollama_status = Some(format!("Pulling '{model_name}'..."));
        let manager = cx.global::<AppOllamaManager>().0.clone();
        let progress_text = std::sync::Arc::new(std::sync::Mutex::new(None::<String>));
        let progress_text_for_thread = std::sync::Arc::clone(&progress_text);
        let result = std::sync::Arc::new(std::sync::Mutex::new(None::<Result<(), String>>));
        let result_for_thread = std::sync::Arc::clone(&result);

        std::thread::spawn({
            let model_name = model_name.clone();
            move || {
                let outcome = match tokio::runtime::Runtime::new() {
                    Ok(rt) => rt.block_on(async {
                        let (tx, mut rx) = tokio::sync::mpsc::channel::<PullProgress>(32);
                        let progress_for_task = std::sync::Arc::clone(&progress_text_for_thread);
                        let pump = tokio::spawn(async move {
                            while let Some(update) = rx.recv().await {
                                *progress_for_task.lock().unwrap_or_else(|e| e.into_inner()) =
                                    Some(format_pull_progress(
                                        &update.status,
                                        update.completed,
                                        update.total,
                                    ));
                            }
                        });
                        let pull_result = manager.pull_model(&model_name, tx).await;
                        let _ = pump.await;
                        pull_result
                    }),
                    Err(e) => Err(format!("tokio runtime: {e}")),
                };
                *result_for_thread.lock().unwrap_or_else(|e| e.into_inner()) = Some(outcome);
            }
        });

        cx.spawn(async move |this, app: &mut AsyncApp| {
            let start = std::time::Instant::now();
            loop {
                if start.elapsed() > std::time::Duration::from_secs(1800) {
                    let _ = this.update(app, |this, cx| {
                        this.ollama_busy = false;
                        this.ollama_status =
                            Some(format!("Timed out pulling '{model_name}' after 30 minutes"));
                        cx.notify();
                    });
                    break;
                }

                let progress = progress_text
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .clone();
                let maybe_result = result.lock().unwrap_or_else(|e| e.into_inner()).take();
                let done = maybe_result.is_some();
                let _ = this.update(app, |this, cx| {
                    if let Some(progress) = progress.clone() {
                        this.ollama_status = Some(format!("Pulling '{model_name}': {progress}"));
                    }
                    if let Some(outcome) = maybe_result {
                        this.ollama_busy = false;
                        match outcome {
                            Ok(()) => {
                                this.ollama_status = Some(format!("Pulled model '{model_name}'"));
                                this.refresh_ollama_models(cx);
                            }
                            Err(e) => {
                                this.ollama_status =
                                    Some(format!("Failed to pull '{model_name}': {e}"));
                            }
                        }
                    }
                    cx.notify();
                });

                if done {
                    break;
                }

                app.background_executor()
                    .timer(std::time::Duration::from_millis(150))
                    .await;
            }
        })
        .detach();
    }

    fn show_ollama_model(&mut self, model_name: String, cx: &mut Context<Self>) {
        if !cx.has_global::<AppOllamaManager>() {
            self.ollama_status = Some("Ollama manager is unavailable".into());
            cx.notify();
            return;
        }

        self.ollama_busy = true;
        self.ollama_status = Some(format!("Inspecting '{model_name}'..."));
        let manager = cx.global::<AppOllamaManager>().0.clone();
        let (tx, rx) = tokio::sync::oneshot::channel();

        std::thread::spawn(move || {
            let result = match tokio::runtime::Runtime::new() {
                Ok(rt) => rt.block_on(manager.show_model(&model_name)).map_err(|e| e.to_string()),
                Err(e) => Err(format!("tokio runtime: {e}")),
            };
            let _ = tx.send(result);
        });

        cx.spawn(async move |this, app: &mut AsyncApp| {
            let result = rx.await.unwrap_or(Err("channel closed".into()));
            let _ = this.update(app, |this, cx| {
                this.ollama_busy = false;
                match result {
                    Ok(model) => {
                        let summary = format!(
                            "{} | size {} | modified {}",
                            model.name,
                            format_optional_size(model.size),
                            model.modified_at.unwrap_or_else(|| "unknown".into())
                        );
                        this.ollama_inspect_summary = Some(summary.clone());
                        this.ollama_status = Some(summary);
                    }
                    Err(e) => {
                        this.ollama_status = Some(format!("Failed to inspect model: {e}"));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn delete_ollama_model(&mut self, model_name: String, cx: &mut Context<Self>) {
        if !cx.has_global::<AppOllamaManager>() {
            self.ollama_status = Some("Ollama manager is unavailable".into());
            cx.notify();
            return;
        }

        self.ollama_busy = true;
        self.ollama_status = Some(format!("Deleting '{model_name}'..."));
        let manager = cx.global::<AppOllamaManager>().0.clone();
        let deleted_model_name = model_name.clone();
        let (tx, rx) = tokio::sync::oneshot::channel();

        std::thread::spawn(move || {
            let result = match tokio::runtime::Runtime::new() {
                Ok(rt) => rt
                    .block_on(manager.delete_model(&model_name))
                    .map_err(|e| e.to_string()),
                Err(e) => Err(format!("tokio runtime: {e}")),
            };
            let _ = tx.send(result);
        });

        cx.spawn(async move |this, app: &mut AsyncApp| {
            let result = rx.await.unwrap_or(Err("channel closed".into()));
            let _ = this.update(app, |this, cx| {
                this.ollama_busy = false;
                match result {
                    Ok(()) => {
                        this.ollama_status =
                            Some(format!("Deleted model '{deleted_model_name}'"));
                        this.ollama_models
                            .retain(|model| model.name != deleted_model_name);
                    }
                    Err(e) => {
                        this.ollama_status =
                            Some(format!("Failed to delete '{deleted_model_name}': {e}"));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn discover_hue_bridges(&mut self, cx: &mut Context<Self>) {
        self.hue_busy = true;
        self.hue_status = Some("Discovering Hue bridges...".into());
        let (tx, rx) = tokio::sync::oneshot::channel();

        std::thread::spawn(move || {
            let result = match tokio::runtime::Runtime::new() {
                Ok(rt) => rt
                    .block_on(PhilipsHueClient::discover_bridges())
                    .map_err(|e| e.to_string()),
                Err(e) => Err(format!("tokio runtime: {e}")),
            };
            let _ = tx.send(result);
        });

        cx.spawn(async move |this, app: &mut AsyncApp| {
            let result = rx.await.unwrap_or(Err("channel closed".into()));
            let _ = this.update(app, |this, cx| {
                this.hue_busy = false;
                match result {
                    Ok(bridges) => {
                        let count = bridges.len();
                        this.hue_bridges = bridges;
                        this.hue_status = Some(format!(
                            "Discovered {count} Hue bridge{}",
                            if count == 1 { "" } else { "s" }
                        ));
                    }
                    Err(e) => {
                        this.hue_status = Some(format!("Hue discovery failed: {e}"));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn refresh_hue_state(&mut self, cx: &mut Context<Self>) {
        let Some(client) = cx
            .has_global::<AppHueClient>()
            .then(|| cx.global::<AppHueClient>().0.clone())
            .flatten()
        else {
            self.hue_status = Some(
                "Hue is not configured yet. Set bridge IP and API key, then click away to save."
                    .into(),
            );
            cx.notify();
            return;
        };

        self.hue_busy = true;
        self.hue_status = Some("Refreshing Hue lights and scenes...".into());
        let (tx, rx) = tokio::sync::oneshot::channel();

        std::thread::spawn(move || {
            let result = match tokio::runtime::Runtime::new() {
                Ok(rt) => rt.block_on(async {
                    let lights = client.list_lights().await.map_err(|e| e.to_string())?;
                    let scenes = client.list_scenes().await.map_err(|e| e.to_string())?;
                    Ok::<_, String>((lights, scenes))
                }),
                Err(e) => Err(format!("tokio runtime: {e}")),
            };
            let _ = tx.send(result);
        });

        cx.spawn(async move |this, app: &mut AsyncApp| {
            let result = rx.await.unwrap_or(Err("channel closed".into()));
            let _ = this.update(app, |this, cx| {
                this.hue_busy = false;
                match result {
                    Ok((lights, scenes)) => {
                        let light_count = lights.len();
                        let scene_count = scenes.len();
                        this.hue_lights = lights;
                        this.hue_scenes = scenes;
                        this.hue_status = Some(format!(
                            "Loaded {light_count} light{} and {scene_count} scene{}",
                            if light_count == 1 { "" } else { "s" },
                            if scene_count == 1 { "" } else { "s" }
                        ));
                    }
                    Err(e) => {
                        this.hue_status = Some(format!("Failed to refresh Hue state: {e}"));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn set_hue_light_state(
        &mut self,
        light_id: String,
        on: bool,
        brightness: Option<u8>,
        cx: &mut Context<Self>,
    ) {
        let Some(client) = cx
            .has_global::<AppHueClient>()
            .then(|| cx.global::<AppHueClient>().0.clone())
            .flatten()
        else {
            self.hue_status = Some("Hue is not configured yet".into());
            cx.notify();
            return;
        };

        self.hue_busy = true;
        self.hue_status = Some(format!("Updating light '{light_id}'..."));
        let updated_light_id = light_id.clone();
        let (tx, rx) = tokio::sync::oneshot::channel();

        std::thread::spawn(move || {
            let result = match tokio::runtime::Runtime::new() {
                Ok(rt) => rt
                    .block_on(client.set_light_state(&light_id, on, brightness))
                    .map_err(|e| e.to_string()),
                Err(e) => Err(format!("tokio runtime: {e}")),
            };
            let _ = tx.send(result);
        });

        cx.spawn(async move |this, app: &mut AsyncApp| {
            let result = rx.await.unwrap_or(Err("channel closed".into()));
            let _ = this.update(app, |this, cx| {
                this.hue_busy = false;
                match result {
                    Ok(()) => {
                        this.hue_status = Some(format!("Updated light '{updated_light_id}'"));
                        this.refresh_hue_state(cx);
                    }
                    Err(e) => {
                        this.hue_status = Some(format!("Failed to update light: {e}"));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn activate_hue_scene(&mut self, scene_id: String, cx: &mut Context<Self>) {
        let Some(client) = cx
            .has_global::<AppHueClient>()
            .then(|| cx.global::<AppHueClient>().0.clone())
            .flatten()
        else {
            self.hue_status = Some("Hue is not configured yet".into());
            cx.notify();
            return;
        };

        self.hue_busy = true;
        self.hue_status = Some(format!("Activating scene '{scene_id}'..."));
        let activated_scene_id = scene_id.clone();
        let (tx, rx) = tokio::sync::oneshot::channel();

        std::thread::spawn(move || {
            let result = match tokio::runtime::Runtime::new() {
                Ok(rt) => rt
                    .block_on(client.activate_scene(&scene_id))
                    .map_err(|e| e.to_string()),
                Err(e) => Err(format!("tokio runtime: {e}")),
            };
            let _ = tx.send(result);
        });

        cx.spawn(async move |this, app: &mut AsyncApp| {
            let result = rx.await.unwrap_or(Err("channel closed".into()));
            let _ = this.update(app, |this, cx| {
                this.hue_busy = false;
                match result {
                    Ok(()) => {
                        this.hue_status =
                            Some(format!("Activated scene '{activated_scene_id}'"));
                        this.refresh_hue_state(cx);
                    }
                    Err(e) => {
                        this.hue_status = Some(format!("Failed to activate scene: {e}"));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Called for every InputEvent from any subscribed input.
    /// Auto-saves on blur (when focus leaves the field).
    fn on_input_event(
        &mut self,
        _state: &Entity<InputState>,
        event: &InputEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::Blur => {
                self.sync_enabled_providers(cx);
                cx.emit(SettingsSaved);
            }
            InputEvent::Change => {
                self.sync_enabled_providers(cx);
                cx.notify();
            }
            _ => {}
        }
    }

    /// Called when the user picks a model from the dropdown.
    fn on_model_selected(
        &mut self,
        _view: &Entity<ModelSelectorView>,
        _event: &ModelSelected,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.emit(SettingsSaved);
    }

    /// Collect all current field values for persistence.
    pub fn collect_values(&self, cx: &App) -> SettingsSnapshot {
        let anthropic_val = self.anthropic_key_input.read(cx).value().to_string();
        let openai_val = self.openai_key_input.read(cx).value().to_string();
        let openrouter_val = self.openrouter_key_input.read(cx).value().to_string();
        let google_val = self.google_key_input.read(cx).value().to_string();
        let groq_val = self.groq_key_input.read(cx).value().to_string();
        let xai_val = self.xai_key_input.read(cx).value().to_string();
        let huggingface_val = self.huggingface_key_input.read(cx).value().to_string();
        let litellm_val = self.litellm_key_input.read(cx).value().to_string();

        let elevenlabs_val = self.elevenlabs_key_input.read(cx).value().to_string();
        let telnyx_val = self.telnyx_key_input.read(cx).value().to_string();
        let hue_val = self.hue_api_key_input.read(cx).value().to_string();

        SettingsSnapshot {
            // Only update keys where input is non-empty
            anthropic_key: non_empty_trimmed(&anthropic_val),
            openai_key: non_empty_trimmed(&openai_val),
            openrouter_key: non_empty_trimmed(&openrouter_val),
            google_key: non_empty_trimmed(&google_val),
            groq_key: non_empty_trimmed(&groq_val),
            xai_key: non_empty_trimmed(&xai_val),
            huggingface_key: non_empty_trimmed(&huggingface_val),
            litellm_key: non_empty_trimmed(&litellm_val),
            elevenlabs_key: non_empty_trimmed(&elevenlabs_val),
            telnyx_key: non_empty_trimmed(&telnyx_val),
            hue_api_key: non_empty_trimmed(&hue_val),

            ollama_url: self.ollama_url_input.read(cx).value().to_string(),
            lmstudio_url: self.lmstudio_url_input.read(cx).value().to_string(),
            hue_bridge_ip: {
                let v = self.hue_bridge_ip_input.read(cx).value().to_string();
                non_empty_trimmed(&v)
            },
            litellm_url: {
                let v = self.litellm_url_input.read(cx).value().to_string();
                non_empty_trimmed(&v)
            },
            custom_url: {
                let v = self.custom_url_input.read(cx).value().to_string();
                non_empty_trimmed(&v)
            },

            default_model: self.model_selector.read(cx).current_model().to_string(),

            daily_budget: self
                .daily_budget_input
                .read(cx)
                .value()
                .parse::<f64>()
                .unwrap_or(0.0),
            monthly_budget: self
                .monthly_budget_input
                .read(cx)
                .value()
                .parse::<f64>()
                .unwrap_or(0.0),

            privacy_mode: self.privacy_mode,
            auto_routing: self.auto_routing,
            speculative_decoding: self.speculative_decoding,
            speculative_show_metrics: self.speculative_show_metrics,
            auto_update: self.auto_update,
            notifications_enabled: self.notifications_enabled,
            tts_enabled: self.tts_enabled,
            tts_auto_speak: self.tts_auto_speak,
            clawdtalk_enabled: self.clawdtalk_enabled,
            google_oauth_client_id: non_empty_trimmed(
                self.google_client_id_input.read(cx).value().as_ref(),
            ),
            microsoft_oauth_client_id: non_empty_trimmed(
                self.microsoft_client_id_input.read(cx).value().as_ref(),
            ),
            github_oauth_client_id: non_empty_trimmed(
                self.github_client_id_input.read(cx).value().as_ref(),
            ),
            slack_oauth_client_id: non_empty_trimmed(
                self.slack_client_id_input.read(cx).value().as_ref(),
            ),
            discord_oauth_client_id: non_empty_trimmed(
                self.discord_client_id_input.read(cx).value().as_ref(),
            ),
            telegram_oauth_client_id: non_empty_trimmed(
                self.telegram_client_id_input.read(cx).value().as_ref(),
            ),
            // Knowledge base — read from config (no UI inputs yet)
            notion_key: None,
            obsidian_vault_path: None,
        }
    }

    /// Whether a given API key is configured (either pre-existing or newly entered).
    fn key_is_set(&self, had_key: bool, input: &Entity<InputState>, cx: &Context<Self>) -> bool {
        had_key || !input.read(cx).value().is_empty()
    }

    /// Sync the model selector's enabled-provider set and API keys
    /// based on current input field values.
    fn sync_enabled_providers(&self, cx: &mut Context<Self>) {
        let anthropic_set = self.key_is_set(self.had_anthropic_key, &self.anthropic_key_input, cx);
        let openai_set = self.key_is_set(self.had_openai_key, &self.openai_key_input, cx);
        let openrouter_set =
            self.key_is_set(self.had_openrouter_key, &self.openrouter_key_input, cx);
        let google_set = self.key_is_set(self.had_google_key, &self.google_key_input, cx);
        let groq_set = self.key_is_set(self.had_groq_key, &self.groq_key_input, cx);
        let xai_set = self.key_is_set(self.had_xai_key, &self.xai_key_input, cx);
        let huggingface_set =
            self.key_is_set(self.had_huggingface_key, &self.huggingface_key_input, cx);

        let mut providers = HashSet::new();
        if anthropic_set {
            providers.insert(ProviderType::Anthropic);
        }
        if openai_set {
            providers.insert(ProviderType::OpenAI);
        }
        if openrouter_set {
            providers.insert(ProviderType::OpenRouter);
        }
        if google_set {
            providers.insert(ProviderType::Google);
        }
        if groq_set {
            providers.insert(ProviderType::Groq);
        }
        if xai_set {
            providers.insert(ProviderType::XAI);
        }
        if huggingface_set {
            providers.insert(ProviderType::HuggingFace);
        }

        // Helper: resolve an API key from input field or saved config
        let resolve_key = |input: &Entity<InputState>,
                           had_key: bool,
                           cx: &Context<Self>,
                           config_field: fn(&hive_core::HiveConfig) -> &Option<String>|
         -> Option<String> {
            let val = input.read(cx).value().to_string();
            if !val.trim().is_empty() {
                Some(val.trim().to_string())
            } else if had_key {
                if cx.has_global::<AppConfig>() {
                    config_field(&cx.global::<AppConfig>().0.get()).clone()
                } else {
                    None
                }
            } else {
                None
            }
        };

        let or_key = resolve_key(
            &self.openrouter_key_input,
            self.had_openrouter_key,
            cx,
            |cfg| &cfg.openrouter_api_key,
        );
        let openai_key = resolve_key(&self.openai_key_input, self.had_openai_key, cx, |cfg| {
            &cfg.openai_api_key
        });
        let anthropic_key = resolve_key(
            &self.anthropic_key_input,
            self.had_anthropic_key,
            cx,
            |cfg| &cfg.anthropic_api_key,
        );
        let google_key = resolve_key(&self.google_key_input, self.had_google_key, cx, |cfg| {
            &cfg.google_api_key
        });
        let groq_key = resolve_key(&self.groq_key_input, self.had_groq_key, cx, |cfg| {
            &cfg.groq_api_key
        });

        self.model_selector.update(cx, |selector, cx| {
            selector.set_enabled_providers(providers, cx);
            selector.set_openrouter_api_key(or_key, cx);
            selector.set_openai_api_key(openai_key, cx);
            selector.set_anthropic_api_key(anthropic_key, cx);
            selector.set_google_api_key(google_key, cx);
            selector.set_groq_api_key(groq_key, cx);
        });
    }

    /// Push the curated project model list into the model selector.
    pub fn set_project_models(
        &mut self,
        models: Vec<String>,
        cx: &mut Context<Self>,
    ) {
        self.model_selector.update(cx, |selector, cx| {
            selector.set_project_models(models, cx);
        });
    }

    /// Feed discovered local models into the model selector.
    pub fn refresh_local_models(
        &mut self,
        models: Vec<hive_ai::types::ModelInfo>,
        cx: &mut Context<Self>,
    ) {
        self.discovered_model_count = models.len();
        self.model_selector.update(cx, |selector, cx| {
            selector.set_local_models(models, cx);
        });
        cx.notify();
    }
}

/// Snapshot of settings values collected from the view.
pub struct SettingsSnapshot {
    pub anthropic_key: Option<String>,
    pub openai_key: Option<String>,
    pub openrouter_key: Option<String>,
    pub google_key: Option<String>,
    pub groq_key: Option<String>,
    pub xai_key: Option<String>,
    pub huggingface_key: Option<String>,
    pub litellm_key: Option<String>,
    pub elevenlabs_key: Option<String>,
    pub telnyx_key: Option<String>,
    pub hue_api_key: Option<String>,
    pub ollama_url: String,
    pub lmstudio_url: String,
    pub litellm_url: Option<String>,
    pub custom_url: Option<String>,
    pub hue_bridge_ip: Option<String>,
    pub default_model: String,
    pub daily_budget: f64,
    pub monthly_budget: f64,
    pub privacy_mode: bool,
    pub auto_routing: bool,
    pub speculative_decoding: bool,
    pub speculative_show_metrics: bool,
    pub auto_update: bool,
    pub notifications_enabled: bool,
    pub tts_enabled: bool,
    pub tts_auto_speak: bool,
    pub clawdtalk_enabled: bool,
    // Knowledge base
    pub notion_key: Option<String>,
    pub obsidian_vault_path: Option<String>,
    // OAuth client IDs
    pub google_oauth_client_id: Option<String>,
    pub microsoft_oauth_client_id: Option<String>,
    pub github_oauth_client_id: Option<String>,
    pub slack_oauth_client_id: Option<String>,
    pub discord_oauth_client_id: Option<String>,
    pub telegram_oauth_client_id: Option<String>,
}

fn key_placeholder(has_key: bool) -> &'static str {
    if has_key {
        "Key configured (enter new to replace)"
    } else {
        "sk-... or enter API key"
    }
}

fn non_empty_trimmed(s: &str) -> Option<String> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

// ---------------------------------------------------------------------------
// Render
// ---------------------------------------------------------------------------

impl Render for SettingsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.theme;

        // Count configured keys (existing + newly entered)
        let anthropic_set = self.key_is_set(self.had_anthropic_key, &self.anthropic_key_input, cx);
        let openai_set = self.key_is_set(self.had_openai_key, &self.openai_key_input, cx);
        let openrouter_set =
            self.key_is_set(self.had_openrouter_key, &self.openrouter_key_input, cx);
        let google_set = self.key_is_set(self.had_google_key, &self.google_key_input, cx);
        let groq_set = self.key_is_set(self.had_groq_key, &self.groq_key_input, cx);
        let xai_set = self.key_is_set(self.had_xai_key, &self.xai_key_input, cx);
        let huggingface_set =
            self.key_is_set(self.had_huggingface_key, &self.huggingface_key_input, cx);
        let key_count = [
            anthropic_set,
            openai_set,
            openrouter_set,
            google_set,
            groq_set,
            xai_set,
            huggingface_set,
        ]
        .iter()
        .filter(|&&v| v)
        .count();

        div()
            .id("settings-scroll")
            .track_focus(&self.focus_handle)
            .flex()
            .flex_col()
            .flex_1()
            .size_full()
            .p(theme.space_4)
            .gap(theme.space_4)
            .overflow_y_scroll()
            .on_action(
                cx.listener(|this: &mut Self, _: &SettingsTogglePrivacy, _, cx| {
                    this.privacy_mode = !this.privacy_mode;
                    cx.emit(SettingsSaved);
                    cx.notify();
                }),
            )
            .on_action(
                cx.listener(|this: &mut Self, _: &SettingsToggleAutoRouting, _, cx| {
                    this.auto_routing = !this.auto_routing;
                    cx.emit(SettingsSaved);
                    cx.notify();
                }),
            )
            .on_action(
                cx.listener(|this: &mut Self, _: &SettingsToggleAutoUpdate, _, cx| {
                    this.auto_update = !this.auto_update;
                    cx.emit(SettingsSaved);
                    cx.notify();
                }),
            )
            .on_action(
                cx.listener(|this: &mut Self, _: &SettingsToggleNotifications, _, cx| {
                    this.notifications_enabled = !this.notifications_enabled;
                    cx.emit(SettingsSaved);
                    cx.notify();
                }),
            )
            .on_action(
                cx.listener(|this: &mut Self, _: &SettingsToggleSpeculativeDecoding, _, cx| {
                    this.speculative_decoding = !this.speculative_decoding;
                    cx.emit(SettingsSaved);
                    cx.notify();
                }),
            )
            .on_action(
                cx.listener(|this: &mut Self, _: &SettingsToggleSpeculativeMetrics, _, cx| {
                    this.speculative_show_metrics = !this.speculative_show_metrics;
                    cx.emit(SettingsSaved);
                    cx.notify();
                }),
            )
            .on_action(
                cx.listener(|this: &mut Self, _: &SettingsToggleTts, _, cx| {
                    this.tts_enabled = !this.tts_enabled;
                    cx.emit(SettingsSaved);
                    cx.notify();
                }),
            )
            .on_action(
                cx.listener(|this: &mut Self, _: &SettingsToggleTtsAutoSpeak, _, cx| {
                    this.tts_auto_speak = !this.tts_auto_speak;
                    cx.emit(SettingsSaved);
                    cx.notify();
                }),
            )
            .on_action(
                cx.listener(|this: &mut Self, _: &SettingsToggleClawdTalk, _, cx| {
                    this.clawdtalk_enabled = !this.clawdtalk_enabled;
                    cx.emit(SettingsSaved);
                    cx.notify();
                }),
            )
            .child(
                div()
                    .w_full()
                    .mx_auto()
                    .flex()
                    .flex_col()
                    .gap(theme.space_4)
                    // Header
                    .child(render_header(key_count, theme))
                    // Summary strip
                    .child(render_settings_overview(
                        key_count,
                        self.discovered_model_count,
                        self.privacy_mode,
                        self.auto_routing,
                        theme,
                    ))
                    // Main content
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .flex_wrap()
                            .items_start()
                            .gap(theme.space_4)
                            // Left column
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(theme.space_4)
                                    .flex_1()
                                    .min_w(px(340.0))
                                    .child(render_api_keys_section(
                                        key_count,
                                        anthropic_set,
                                        &self.anthropic_key_input,
                                        openai_set,
                                        &self.openai_key_input,
                                        openrouter_set,
                                        &self.openrouter_key_input,
                                        google_set,
                                        &self.google_key_input,
                                        groq_set,
                                        &self.groq_key_input,
                                        xai_set,
                                        &self.xai_key_input,
                                        huggingface_set,
                                        &self.huggingface_key_input,
                                        theme,
                                    ))
                                    .child(self.render_local_ai_section(cx))
                                    .child(self.render_smart_home_section(cx)),
                            )
                            // Right column
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(theme.space_4)
                                    .flex_1()
                                    .min_w(px(320.0))
                                    .child(self.render_model_routing_section(cx))
                                    .child(self.render_budget_section(cx))
                                    .child(self.render_voice_tts_section(cx))
                                    .child(self.render_connected_accounts_section(cx))
                                    .child(self.render_general_section(cx))
                                    .child(self.render_import_export_section(cx)),
                            ),
                    ),
            )
    }
}

impl SettingsView {
    fn render_local_ai_section(&self, cx: &Context<Self>) -> AnyElement {
        let theme = &self.theme;
        let litellm_set = self.key_is_set(self.had_litellm_key, &self.litellm_key_input, cx);
        let entity = cx.entity().clone();

        let discovery_text = if self.discovered_model_count > 0 {
            format!(
                "{} local model{} discovered",
                self.discovered_model_count,
                if self.discovered_model_count == 1 {
                    ""
                } else {
                    "s"
                }
            )
        } else {
            "No local models found".to_string()
        };

        let mut section = card(theme)
            .child(section_title("\u{1F4BB}", "Local AI", theme))
            .child(section_desc(
                "Connect to locally-running models for free, private inference.",
                theme,
            ))
            .child(separator(theme))
            .child(input_row("Ollama URL", &self.ollama_url_input, theme))
            .child(input_row("LM Studio URL", &self.lmstudio_url_input, theme))
            .child(input_row("Custom Local URL", &self.custom_url_input, theme))
            .child(separator(theme))
            .child(input_row("LiteLLM Proxy URL", &self.litellm_url_input, theme))
            .child(api_key_row("LiteLLM API Key", litellm_set, &self.litellm_key_input, theme))
            .child(separator(theme))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(theme.space_2)
                    .px(theme.space_3)
                    .py(theme.space_2)
                    .rounded(theme.radius_sm)
                    .bg(theme.bg_primary)
                    .child(
                        div()
                            .w(px(8.0))
                            .h(px(8.0))
                            .rounded(theme.radius_full)
                            .bg(if self.discovered_model_count > 0 {
                                theme.accent_green
                            } else {
                                theme.text_muted
                            }),
                    )
                    .child(
                        div()
                            .text_size(theme.font_size_xs)
                            .text_color(theme.text_muted)
                            .child(discovery_text),
                    ),
            )
            .child(separator(theme))
            .child(input_row("Pull Model", &self.ollama_pull_model_input, theme))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap(theme.space_2)
                    .child(settings_action_button(
                        if self.ollama_busy {
                            "Refreshing..."
                        } else {
                            "Refresh Models"
                        },
                        self.ollama_busy,
                        theme,
                        {
                            let entity = entity.clone();
                            move |_event, _window, cx| {
                                entity.update(cx, |this, cx| this.refresh_ollama_models(cx));
                            }
                        },
                    ))
                    .child(settings_action_button(
                        if self.ollama_busy { "Pulling..." } else { "Pull Model" },
                        self.ollama_busy,
                        theme,
                        {
                            let entity = entity.clone();
                            move |_event, _window, cx| {
                                let model_name = entity
                                    .read(cx)
                                    .ollama_pull_model_input
                                    .read(cx)
                                    .value()
                                    .to_string();
                                entity.update(cx, |this, cx| {
                                    this.pull_ollama_model(model_name, cx);
                                });
                            }
                        },
                    )),
            )
            .child(switch_row(
                "Privacy Mode",
                "privacy-switch",
                self.privacy_mode,
                SettingsTogglePrivacy,
                theme,
            ))
            .child(
                div()
                    .px(theme.space_3)
                    .py(theme.space_2)
                    .rounded(theme.radius_sm)
                    .bg(theme.bg_primary)
                    .text_size(theme.font_size_xs)
                    .text_color(theme.text_muted)
                    .child(if self.privacy_mode {
                        "Privacy mode ON -- requests are routed to local providers only. No data leaves your machine."
                    } else {
                        "Privacy mode OFF -- requests may be sent to cloud providers when local models are unavailable."
                    }),
            );

        if let Some(status) = &self.ollama_status {
            section = section.child(status_banner(status, self.ollama_busy, theme));
        }

        if let Some(summary) = &self.ollama_inspect_summary {
            section = section.child(
                div()
                    .px(theme.space_3)
                    .py(theme.space_2)
                    .rounded(theme.radius_sm)
                    .bg(theme.bg_primary)
                    .text_size(theme.font_size_xs)
                    .text_color(theme.text_muted)
                    .child(summary.clone()),
            );
        }

        if self.ollama_models.is_empty() {
            section = section.child(
                div()
                    .text_size(theme.font_size_xs)
                    .text_color(theme.text_muted)
                    .child("No Ollama models loaded yet."),
            );
        } else {
            let mut rows = div().flex().flex_col().gap(theme.space_2);
            for model in &self.ollama_models {
                rows = rows.child(render_ollama_model_row(entity.clone(), model, self.ollama_busy, theme));
            }
            section = section.child(rows);
        }

        section.into_any_element()
    }

    fn render_smart_home_section(&self, cx: &Context<Self>) -> AnyElement {
        let theme = &self.theme;
        let entity = cx.entity().clone();
        let hue_connected = self.key_is_set(self.had_hue_key, &self.hue_api_key_input, cx)
            && !self.hue_bridge_ip_input.read(cx).value().trim().is_empty();

        let mut section = card(theme)
            .child(section_title("\u{1F4A1}", "Smart Home", theme))
            .child(section_desc(
                "Manage Philips Hue bridges, lights, and scenes from the shared app integration.",
                theme,
            ))
            .child(separator(theme))
            .child(input_row("Hue Bridge IP", &self.hue_bridge_ip_input, theme))
            .child(api_key_row(
                "Hue API Key",
                self.key_is_set(self.had_hue_key, &self.hue_api_key_input, cx),
                &self.hue_api_key_input,
                theme,
            ))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap(theme.space_2)
                    .child(settings_action_button(
                        if self.hue_busy {
                            "Discovering..."
                        } else {
                            "Discover Bridges"
                        },
                        self.hue_busy,
                        theme,
                        {
                            let entity = entity.clone();
                            move |_event, _window, cx| {
                                entity.update(cx, |this, cx| this.discover_hue_bridges(cx));
                            }
                        },
                    ))
                    .child(settings_action_button(
                        if self.hue_busy { "Refreshing..." } else { "Refresh Devices" },
                        self.hue_busy,
                        theme,
                        {
                            let entity = entity.clone();
                            move |_event, _window, cx| {
                                entity.update(cx, |this, cx| this.refresh_hue_state(cx));
                            }
                        },
                    )),
            )
            .child(
                div()
                    .px(theme.space_3)
                    .py(theme.space_2)
                    .rounded(theme.radius_sm)
                    .bg(theme.bg_primary)
                    .text_size(theme.font_size_xs)
                    .text_color(theme.text_muted)
                    .child(if hue_connected {
                        "Hue bridge credentials are present. Click away from the inputs after editing so the shared client is rebuilt."
                    } else {
                        "Enter the bridge IP and API key, then click away from the field to save and enable the shared Hue client."
                    }),
            );

        if let Some(status) = &self.hue_status {
            section = section.child(status_banner(status, self.hue_busy, theme));
        }

        if self.hue_bridges.is_empty() {
            section = section.child(
                div()
                    .text_size(theme.font_size_xs)
                    .text_color(theme.text_muted)
                    .child("No discovered Hue bridges yet."),
            );
        } else {
            let mut bridge_rows = div().flex().flex_col().gap(theme.space_2);
            for bridge in &self.hue_bridges {
                bridge_rows = bridge_rows.child(render_hue_bridge_row(entity.clone(), bridge, theme));
            }
            section = section.child(bridge_rows);
        }

        section = section.child(separator(theme));
        if self.hue_lights.is_empty() {
            section = section.child(
                div()
                    .text_size(theme.font_size_xs)
                    .text_color(theme.text_muted)
                    .child("No Hue lights loaded."),
            );
        } else {
            let mut light_rows = div().flex().flex_col().gap(theme.space_2);
            for light in &self.hue_lights {
                light_rows = light_rows.child(render_hue_light_row(entity.clone(), light, self.hue_busy, theme));
            }
            section = section.child(light_rows);
        }

        if self.hue_scenes.is_empty() {
            section = section.child(
                div()
                    .text_size(theme.font_size_xs)
                    .text_color(theme.text_muted)
                    .child("No Hue scenes loaded."),
            );
        } else {
            let mut scene_rows = div().flex().flex_col().gap(theme.space_2);
            for scene in &self.hue_scenes {
                scene_rows = scene_rows.child(render_hue_scene_row(entity.clone(), scene, self.hue_busy, theme));
            }
            section = section.child(scene_rows);
        }

        section.into_any_element()
    }

    fn render_model_routing_section(&self, _cx: &Context<Self>) -> AnyElement {
        let theme = &self.theme;

        card(theme)
            .child(section_title("\u{1F500}", "Model Routing", theme))
            .child(section_desc(
                "Control which model handles your requests.",
                theme,
            ))
            .child(separator(theme))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(theme.space_4)
                    .py(theme.space_1)
                    .child(
                        div()
                            .text_size(theme.font_size_base)
                            .text_color(theme.text_secondary)
                            .child("Default Model"),
                    )
                    .child(
                        div().min_w(px(180.0)).child(self.model_selector.clone()),
                    ),
            )
            .child(switch_row(
                "Auto Routing",
                "auto-routing-switch",
                self.auto_routing,
                SettingsToggleAutoRouting,
                theme,
            ))
            .child(
                div()
                    .px(theme.space_3)
                    .py(theme.space_2)
                    .rounded(theme.radius_sm)
                    .bg(theme.bg_primary)
                    .text_size(theme.font_size_xs)
                    .text_color(theme.text_muted)
                    .child(if self.auto_routing {
                        "Requests are automatically routed to the best model based on task complexity."
                    } else {
                        "All requests will use the default model above."
                    }),
            )
            .child(separator(theme))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(theme.space_2)
                    .child(
                        div()
                            .text_size(theme.font_size_base)
                            .text_color(theme.text_primary)
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Speculative Decoding"),
                    )
                    .child(
                        div()
                            .px(theme.space_2)
                            .py(px(1.0))
                            .rounded(theme.radius_sm)
                            .bg(theme.accent_cyan.opacity(0.15))
                            .text_size(px(10.0))
                            .text_color(theme.accent_cyan)
                            .font_weight(FontWeight::BOLD)
                            .child("BETA"),
                    ),
            )
            .child(
                div()
                    .px(theme.space_3)
                    .py(theme.space_2)
                    .rounded(theme.radius_sm)
                    .bg(theme.bg_primary)
                    .text_size(theme.font_size_xs)
                    .text_color(theme.text_muted)
                    .child(
                        "\"Guess and Check\" — sends your request to a fast draft model and primary model simultaneously. See instant results from the draft while the high-quality response loads."
                    ),
            )
            .child(switch_row(
                "Enable Speculative Decoding",
                "speculative-decoding-switch",
                self.speculative_decoding,
                SettingsToggleSpeculativeDecoding,
                theme,
            ))
            .when(self.speculative_decoding, |el| {
                el.child(switch_row(
                    "Show Speed Metrics",
                    "speculative-metrics-switch",
                    self.speculative_show_metrics,
                    SettingsToggleSpeculativeMetrics,
                    theme,
                ))
            })
            .into_any_element()
    }

    fn render_budget_section(&self, _cx: &Context<Self>) -> AnyElement {
        let theme = &self.theme;

        card(theme)
            .child(section_title("\u{1F4B0}", "Budget", theme))
            .child(section_desc(
                "Set spending limits to prevent unexpected charges.",
                theme,
            ))
            .child(separator(theme))
            .child(budget_row("Daily Budget", &self.daily_budget_input, theme))
            .child(budget_row(
                "Monthly Budget",
                &self.monthly_budget_input,
                theme,
            ))
            .into_any_element()
    }

    fn render_voice_tts_section(&self, cx: &Context<Self>) -> AnyElement {
        let theme = &self.theme;
        let elevenlabs_set =
            self.key_is_set(self.had_elevenlabs_key, &self.elevenlabs_key_input, cx);
        let telnyx_set = self.key_is_set(self.had_telnyx_key, &self.telnyx_key_input, cx);

        card(theme)
            .child(section_title("\u{1F50A}", "Voice & TTS", theme))
            .child(section_desc(
                "Text-to-speech synthesis. Local providers (Qwen3, F5) work offline; cloud providers require API keys.",
                theme,
            ))
            .child(separator(theme))
            .child(switch_row(
                "Enable TTS",
                "tts-enable-switch",
                self.tts_enabled,
                SettingsToggleTts,
                theme,
            ))
            .child(switch_row(
                "Auto-Speak Responses",
                "tts-auto-speak-switch",
                self.tts_auto_speak,
                SettingsToggleTtsAutoSpeak,
                theme,
            ))
            .child(separator(theme))
            .child(api_key_row("ElevenLabs API Key", elevenlabs_set, &self.elevenlabs_key_input, theme))
            .child(api_key_row("Telnyx API Key", telnyx_set, &self.telnyx_key_input, theme))
            .child(separator(theme))
            .child(switch_row(
                "ClawdTalk Phone Bridge",
                "clawdtalk-switch",
                self.clawdtalk_enabled,
                SettingsToggleClawdTalk,
                theme,
            ))
            .child(
                div()
                    .px(theme.space_3)
                    .py(theme.space_2)
                    .rounded(theme.radius_sm)
                    .bg(theme.bg_primary)
                    .text_size(theme.font_size_xs)
                    .text_color(theme.text_muted)
                    .child(if self.tts_enabled {
                        "TTS enabled -- assistant responses will be spoken aloud."
                    } else {
                        "TTS disabled -- enable to hear assistant responses."
                    }),
            )
            .into_any_element()
    }

    fn render_connected_accounts_section(&self, cx: &Context<Self>) -> AnyElement {
        use hive_core::config::AccountPlatform;
        let theme = &self.theme;

        // Read connected accounts from config
        let connected = if cx.has_global::<AppConfig>() {
            cx.global::<AppConfig>()
                .0
                .get()
                .connected_accounts
                .clone()
        } else {
            Vec::new()
        };

        let platforms = AccountPlatform::ALL;
        let mut rows: Vec<AnyElement> = Vec::new();

        for platform in &platforms {
            let is_connected = connected.iter().any(|a| a.platform == *platform);
            let account_name = connected
                .iter()
                .find(|a| a.platform == *platform)
                .map(|a| a.account_name.clone())
                .unwrap_or_default();
            let last_synced = connected
                .iter()
                .find(|a| a.platform == *platform)
                .and_then(|a| a.last_synced.clone());

            // Get the client ID input entity for this platform
            let client_id_input = match platform {
                AccountPlatform::Google => self.google_client_id_input.clone(),
                AccountPlatform::Microsoft => self.microsoft_client_id_input.clone(),
                AccountPlatform::GitHub => self.github_client_id_input.clone(),
                AccountPlatform::Slack => self.slack_client_id_input.clone(),
                AccountPlatform::Discord => self.discord_client_id_input.clone(),
                AccountPlatform::Telegram => self.telegram_client_id_input.clone(),
            };

            let setup_url = platform.setup_url();

            rows.push(
                div()
                    .flex()
                    .flex_col()
                    .gap(theme.space_2)
                    .py(theme.space_3)
                    .border_b_1()
                    .border_color(theme.border)
                    .child(
                        // Platform header row
                        div()
                            .flex()
                            .items_center()
                            .gap(theme.space_3)
                            .child(
                                div()
                                    .text_size(px(20.0))
                                    .child(platform.icon().to_string()),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .child(
                                        div()
                                            .text_size(theme.font_size_sm)
                                            .text_color(theme.text_primary)
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child(platform.label().to_string()),
                                    )
                                    .when(is_connected, |el: Div| {
                                        el.child(
                                            div()
                                                .text_size(theme.font_size_xs)
                                                .text_color(theme.text_muted)
                                                .child(account_name.clone()),
                                        )
                                    })
                                    .when(last_synced.is_some(), |el: Div| {
                                        el.child(
                                            div()
                                                .text_size(px(9.0))
                                                .text_color(theme.text_muted)
                                                .child(format!(
                                                    "Last synced: {}",
                                                    last_synced.as_deref().unwrap_or("never")
                                                )),
                                        )
                                    }),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(theme.space_2)
                                    .child(status_dot(is_connected, theme))
                                    .child(if is_connected {
                                        div()
                                            .text_size(theme.font_size_xs)
                                            .text_color(theme.accent_green)
                                            .child("Connected")
                                            .into_any_element()
                                    } else {
                                        let plat_name = platform.label().to_string();
                                        div()
                                            .id(ElementId::Name(
                                                format!("connect-{}", platform.label()).into(),
                                            ))
                                            .px(theme.space_3)
                                            .py(theme.space_1)
                                            .rounded(theme.radius_md)
                                            .bg(theme.accent_cyan)
                                            .text_size(theme.font_size_xs)
                                            .text_color(theme.bg_primary)
                                            .font_weight(FontWeight::BOLD)
                                            .cursor_pointer()
                                            .hover(|s| s.opacity(0.85))
                                            .on_click(move |_ev, window, cx| {
                                                window.dispatch_action(
                                                    Box::new(AccountConnectPlatform {
                                                        platform: plat_name.clone(),
                                                    }),
                                                    cx,
                                                );
                                            })
                                            .child("Connect")
                                            .into_any_element()
                                    }),
                            ),
                    )
                    .child(
                        // OAuth Client ID input row
                        div()
                            .flex()
                            .items_center()
                            .gap(theme.space_2)
                            .pl(px(32.0)) // Indent under icon
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .child(
                                        Input::new(&client_id_input)
                                            .text_size(theme.font_size_xs)
                                            .cleanable(true),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .items_end()
                                    .gap(px(2.0))
                                    .child({
                                        let url = setup_url.to_string();
                                        div()
                                            .id(ElementId::Name(
                                                format!("setup-link-{}", platform.label()).into(),
                                            ))
                                            .text_size(theme.font_size_xs)
                                            .text_color(theme.accent_cyan)
                                            .cursor_pointer()
                                            .hover(|s| s.text_color(theme.accent_aqua))
                                            .on_click(move |_ev, _window, cx| {
                                                cx.open_url(&url);
                                            })
                                            .child("Setup \u{2197}")
                                    })
                                    .child(
                                        div()
                                            .text_size(px(8.0))
                                            .text_color(theme.text_muted)
                                            .max_w(px(150.0))
                                            .overflow_hidden()
                                            .child(setup_url.to_string()),
                                    ),
                            ),
                    )
                    .into_any_element(),
            );
        }

        card(theme)
            .child(section_title("\u{1F517}", "Connected Accounts", theme))
            .child(section_desc(
                "Link external services for calendar, email, repos, and messaging integration. Provide your own OAuth Client ID for each platform.",
                theme,
            ))
            .child(separator(theme))
            .children(rows)
            .into_any_element()
    }

    fn render_general_section(&self, _cx: &Context<Self>) -> AnyElement {
        let theme = &self.theme;

        // Build theme picker buttons.
        let selected = self.selected_theme.to_lowercase();
        let theme_buttons: Vec<AnyElement> = self
            .available_themes
            .iter()
            .map(|name| {
                let is_active = name.to_lowercase() == selected
                    || (selected == "dark" && name == "HiveCode Dark")
                    || (selected == "light" && name == "HiveCode Light");
                let label = name.clone();
                let action_name = name.clone();
                div()
                    .id(SharedString::from(format!("theme-btn-{}", name)))
                    .cursor_pointer()
                    .px(theme.space_3)
                    .py(theme.space_2)
                    .rounded(theme.radius_sm)
                    .text_size(theme.font_size_sm)
                    .text_color(if is_active {
                        theme.text_on_accent
                    } else {
                        theme.text_primary
                    })
                    .bg(if is_active {
                        theme.accent_aqua
                    } else {
                        theme.bg_tertiary
                    })
                    .hover(|s| {
                        s.bg(if is_active {
                            theme.accent_cyan
                        } else {
                            theme.bg_secondary
                        })
                    })
                    .on_mouse_down(MouseButton::Left, move |_ev, window, cx| {
                        window.dispatch_action(
                            Box::new(ThemeChanged {
                                theme_name: action_name.clone(),
                            }),
                            cx,
                        );
                    })
                    .child(label)
                    .into_any_element()
            })
            .collect();

        card(theme)
            .child(section_title("\u{2699}", "General", theme))
            .child(section_desc(
                "Application preferences and display settings.",
                theme,
            ))
            .child(separator(theme))
            // Theme picker
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(theme.space_2)
                    .child(
                        div()
                            .text_size(theme.font_size_sm)
                            .text_color(theme.text_secondary)
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Theme"),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .flex_wrap()
                            .gap(theme.space_2)
                            .children(theme_buttons),
                    ),
            )
            .child(separator(theme))
            // Context format picker
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(theme.space_2)
                    .child(
                        div()
                            .text_size(theme.font_size_sm)
                            .text_color(theme.text_secondary)
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Context Format"),
                    )
                    .child(
                        div()
                            .text_size(theme.font_size_xs)
                            .text_color(theme.text_muted)
                            .child("How project context is encoded in AI prompts"),
                    )
                    .child({
                        let sel = self.selected_context_format.to_lowercase();
                        let options = vec![
                            ("markdown", "Markdown"),
                            ("toon", "TOON"),
                            ("xml", "XML"),
                        ];
                        div()
                            .flex()
                            .flex_row()
                            .flex_wrap()
                            .gap(theme.space_2)
                            .children(options.into_iter().map(|(value, label)| {
                                let is_active = sel == value;
                                let action_value = value.to_string();
                                div()
                                    .id(SharedString::from(format!("ctx-fmt-{}", value)))
                                    .cursor_pointer()
                                    .px(theme.space_3)
                                    .py(theme.space_2)
                                    .rounded(theme.radius_sm)
                                    .text_size(theme.font_size_sm)
                                    .text_color(if is_active {
                                        theme.text_on_accent
                                    } else {
                                        theme.text_primary
                                    })
                                    .bg(if is_active {
                                        theme.accent_aqua
                                    } else {
                                        theme.bg_tertiary
                                    })
                                    .hover(|s| {
                                        s.bg(if is_active {
                                            theme.accent_cyan
                                        } else {
                                            theme.bg_secondary
                                        })
                                    })
                                    .on_mouse_down(MouseButton::Left, move |_ev, window, cx| {
                                        window.dispatch_action(
                                            Box::new(ContextFormatChanged {
                                                format: action_value.clone(),
                                            }),
                                            cx,
                                        );
                                    })
                                    .child(label)
                                    .into_any_element()
                            }))
                    }),
            )
            .child(separator(theme))
            .child(switch_row(
                "Auto Update",
                "auto-update-switch",
                self.auto_update,
                SettingsToggleAutoUpdate,
                theme,
            ))
            .child(switch_row(
                "Notifications",
                "notifications-switch",
                self.notifications_enabled,
                SettingsToggleNotifications,
                theme,
            ))
            .into_any_element()
    }

    fn render_import_export_section(&self, _cx: &Context<Self>) -> AnyElement {
        let theme = &self.theme;

        card(theme)
            .child(section_title("\u{1F4E6}", "Import & Export", theme))
            .child(section_desc(
                "Export your settings, API keys, and OAuth tokens as an encrypted backup. Import a previously exported backup to restore.",
                theme,
            ))
            .child(separator(theme))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(theme.space_3)
                    .child(
                        div()
                            .id("settings-export-btn")
                            .flex()
                            .items_center()
                            .gap(theme.space_2)
                            .px(theme.space_3)
                            .py(theme.space_2)
                            .rounded(theme.radius_sm)
                            .bg(theme.bg_surface)
                            .border_1()
                            .border_color(theme.border)
                            .text_size(theme.font_size_sm)
                            .text_color(theme.accent_cyan)
                            .cursor_pointer()
                            .hover(|s| s.bg(theme.bg_tertiary))
                            .child(Icon::new(IconName::ArrowDown).size_3p5().text_color(theme.accent_cyan))
                            .child("Export Settings")
                            .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                                cx.dispatch_action(&ExportConfig);
                            }),
                    )
                    .child(
                        div()
                            .id("settings-import-btn")
                            .flex()
                            .items_center()
                            .gap(theme.space_2)
                            .px(theme.space_3)
                            .py(theme.space_2)
                            .rounded(theme.radius_sm)
                            .bg(theme.bg_surface)
                            .border_1()
                            .border_color(theme.border)
                            .text_size(theme.font_size_sm)
                            .text_color(theme.accent_yellow)
                            .cursor_pointer()
                            .hover(|s| s.bg(theme.bg_tertiary))
                            .child(Icon::new(IconName::ArrowUp).size_3p5().text_color(theme.accent_yellow))
                            .child("Import Settings")
                            .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                                cx.dispatch_action(&ImportConfig);
                            }),
                    ),
            )
            .child(
                div()
                    .px(theme.space_3)
                    .py(theme.space_2)
                    .rounded(theme.radius_sm)
                    .bg(theme.bg_primary)
                    .text_size(theme.font_size_xs)
                    .text_color(theme.text_muted)
                    .child("Exported files are encrypted with a password you provide. Keep the password safe -- it cannot be recovered."),
            )
            .into_any_element()
    }
}

// ---------------------------------------------------------------------------
// Shared card helpers
// ---------------------------------------------------------------------------

fn card(theme: &HiveTheme) -> Div {
    div()
        .flex()
        .flex_col()
        .p(theme.space_6)
        .gap(theme.space_4)
        .rounded(theme.radius_md)
        .bg(theme.bg_surface)
        .border_1()
        .border_color(theme.border)
}

fn section_title(icon: &str, label: &str, theme: &HiveTheme) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(theme.space_2)
        .child(div().text_size(theme.font_size_lg).child(icon.to_string()))
        .child(
            div()
                .text_size(theme.font_size_lg)
                .text_color(theme.text_primary)
                .font_weight(FontWeight::BOLD)
                .child(label.to_string()),
        )
        .into_any_element()
}

fn section_desc(text: &str, theme: &HiveTheme) -> AnyElement {
    div()
        .text_size(theme.font_size_sm)
        .text_color(theme.text_muted)
        .child(text.to_string())
        .into_any_element()
}

fn separator(theme: &HiveTheme) -> AnyElement {
    div()
        .w_full()
        .h(px(1.0))
        .bg(theme.border)
        .into_any_element()
}

fn status_dot(present: bool, theme: &HiveTheme) -> AnyElement {
    let color = if present {
        theme.accent_green
    } else {
        theme.accent_red
    };
    div()
        .w(px(8.0))
        .h(px(8.0))
        .rounded(theme.radius_full)
        .bg(color)
        .into_any_element()
}

fn status_text(connected: bool, theme: &HiveTheme) -> AnyElement {
    let (label, color) = if connected {
        ("Connected", theme.accent_green)
    } else {
        ("Not configured", theme.accent_red)
    };
    div()
        .text_size(theme.font_size_xs)
        .text_color(color)
        .child(label)
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Section: API Keys (free function to avoid borrow issues)
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn render_api_keys_section(
    key_count: usize,
    anthropic_set: bool,
    anthropic_input: &Entity<InputState>,
    openai_set: bool,
    openai_input: &Entity<InputState>,
    openrouter_set: bool,
    openrouter_input: &Entity<InputState>,
    google_set: bool,
    google_input: &Entity<InputState>,
    groq_set: bool,
    groq_input: &Entity<InputState>,
    xai_set: bool,
    xai_input: &Entity<InputState>,
    huggingface_set: bool,
    huggingface_input: &Entity<InputState>,
    theme: &HiveTheme,
) -> AnyElement {
    card(theme)
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .child(section_title("\u{1F511}", "API Keys", theme))
                .child(
                    div()
                        .px(theme.space_2)
                        .py(px(2.0))
                        .rounded(theme.radius_sm)
                        .bg(theme.bg_tertiary)
                        .text_size(theme.font_size_xs)
                        .text_color(if key_count > 0 {
                            theme.accent_green
                        } else {
                            theme.accent_red
                        })
                        .child(format!("{}/7 configured", key_count)),
                ),
        )
        .child(section_desc(
            "Provider API keys for cloud model access. Keys are stored locally and encrypted. Changes save automatically.",
            theme,
        ))
        .child(separator(theme))
        .child(api_key_row("Anthropic API Key", anthropic_set, anthropic_input, theme))
        .child(api_key_row("OpenAI API Key", openai_set, openai_input, theme))
        .child(api_key_row("OpenRouter API Key", openrouter_set, openrouter_input, theme))
        .child(api_key_row("Google API Key", google_set, google_input, theme))
        .child(api_key_row("Groq API Key", groq_set, groq_input, theme))
        .child(api_key_row("xAI API Key", xai_set, xai_input, theme))
        .child(api_key_row("Hugging Face API Key", huggingface_set, huggingface_input, theme))
        .into_any_element()
}

fn render_ollama_model_row(
    entity: Entity<SettingsView>,
    model: &ManagedOllamaModel,
    busy: bool,
    theme: &HiveTheme,
) -> AnyElement {
    let inspect_name = model.name.clone();
    let delete_name = model.name.clone();

    div()
        .flex()
        .flex_col()
        .gap(theme.space_2)
        .p(theme.space_3)
        .rounded(theme.radius_sm)
        .bg(theme.bg_primary)
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(theme.space_2)
                .child(
                    div()
                        .text_size(theme.font_size_sm)
                        .text_color(theme.text_primary)
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(model.name.clone()),
                )
                .child(div().flex_1())
                .child(settings_action_button("Inspect", busy, theme, {
                    let entity = entity.clone();
                    move |_event, _window, cx| {
                        entity.update(cx, |this, cx| {
                            this.show_ollama_model(inspect_name.clone(), cx);
                        });
                    }
                }))
                .child(settings_action_button("Delete", busy, theme, {
                    let entity = entity.clone();
                    move |_event, _window, cx| {
                        entity.update(cx, |this, cx| {
                            this.delete_ollama_model(delete_name.clone(), cx);
                        });
                    }
                })),
        )
        .child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .child(format!(
                    "Size {} | Modified {}",
                    format_optional_size(model.size),
                    model.modified_at.clone().unwrap_or_else(|| "unknown".into())
                )),
        )
        .into_any_element()
}

fn render_hue_bridge_row(
    entity: Entity<SettingsView>,
    bridge: &HueBridge,
    theme: &HiveTheme,
) -> AnyElement {
    let bridge_ip = bridge.ip.clone();

    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(theme.space_3)
        .p(theme.space_3)
        .rounded(theme.radius_sm)
        .bg(theme.bg_primary)
        .child(
            div()
                .flex_1()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .child(
                    div()
                        .text_size(theme.font_size_sm)
                        .text_color(theme.text_primary)
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(bridge.id.clone()),
                )
                .child(
                    div()
                        .text_size(theme.font_size_xs)
                        .text_color(theme.text_muted)
                        .child(bridge.ip.clone()),
                ),
        )
        .child(settings_action_button("Use Bridge", false, theme, move |_event, window, cx| {
            entity.update(cx, |this, cx| {
                this.hue_bridge_ip_input.update(cx, |state, cx| {
                    state.set_value(bridge_ip.clone(), window, cx);
                });
                cx.emit(SettingsSaved);
                cx.notify();
            });
        }))
        .into_any_element()
}

fn render_hue_light_row(
    entity: Entity<SettingsView>,
    light: &HueLight,
    busy: bool,
    theme: &HiveTheme,
) -> AnyElement {
    let light_id_on = light.id.clone();
    let light_id_dim = light.id.clone();
    let light_id_bright = light.id.clone();

    div()
        .flex()
        .flex_col()
        .gap(theme.space_2)
        .p(theme.space_3)
        .rounded(theme.radius_sm)
        .bg(theme.bg_primary)
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(theme.space_2)
                .child(status_dot(light.reachable, theme))
                .child(
                    div()
                        .text_size(theme.font_size_sm)
                        .text_color(theme.text_primary)
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(light.name.clone()),
                )
                .child(div().flex_1())
                .child(
                    div()
                        .text_size(theme.font_size_xs)
                        .text_color(theme.text_muted)
                        .child(format!(
                            "{} | bri {}",
                            if light.on { "On" } else { "Off" },
                            light.brightness
                        )),
                ),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .gap(theme.space_2)
                .child(settings_action_button(
                    if light.on { "Turn Off" } else { "Turn On" },
                    busy,
                    theme,
                    {
                        let entity = entity.clone();
                        let turn_on = !light.on;
                        move |_event, _window, cx| {
                            entity.update(cx, |this, cx| {
                                this.set_hue_light_state(
                                    light_id_on.clone(),
                                    turn_on,
                                    if turn_on { Some(254) } else { None },
                                    cx,
                                );
                            });
                        }
                    },
                ))
                .child(settings_action_button("Dim", busy, theme, {
                    let entity = entity.clone();
                    move |_event, _window, cx| {
                        entity.update(cx, |this, cx| {
                            this.set_hue_light_state(light_id_dim.clone(), true, Some(96), cx);
                        });
                    }
                }))
                .child(settings_action_button("Bright", busy, theme, {
                    move |_event, _window, cx| {
                        entity.update(cx, |this, cx| {
                            this.set_hue_light_state(light_id_bright.clone(), true, Some(254), cx);
                        });
                    }
                })),
        )
        .into_any_element()
}

fn render_hue_scene_row(
    entity: Entity<SettingsView>,
    scene: &HueScene,
    busy: bool,
    theme: &HiveTheme,
) -> AnyElement {
    let scene_id = scene.id.clone();

    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(theme.space_3)
        .p(theme.space_3)
        .rounded(theme.radius_sm)
        .bg(theme.bg_primary)
        .child(
            div()
                .flex_1()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .child(
                    div()
                        .text_size(theme.font_size_sm)
                        .text_color(theme.text_primary)
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(scene.name.clone()),
                )
                .child(
                    div()
                        .text_size(theme.font_size_xs)
                        .text_color(theme.text_muted)
                        .child(scene.id.clone()),
                ),
        )
        .child(settings_action_button("Activate", busy, theme, move |_event, _window, cx| {
            entity.update(cx, |this, cx| {
                this.activate_hue_scene(scene_id.clone(), cx);
            });
        }))
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Row helpers with interactive widgets
// ---------------------------------------------------------------------------

/// An API key row with status dot, masked input, and status badge.
fn api_key_row(
    label: &str,
    has_key: bool,
    input_state: &Entity<InputState>,
    theme: &HiveTheme,
) -> AnyElement {
    div()
        .flex()
        .items_start()
        .gap(theme.space_4)
        .py(theme.space_2)
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .gap(theme.space_2)
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(theme.space_2)
                        .child(status_dot(has_key, theme))
                        .child(
                            div()
                                .text_size(theme.font_size_base)
                                .text_color(theme.text_secondary)
                                .child(label.to_string()),
                        ),
                )
                .child(status_text(has_key, theme)),
        )
        .child(
            div()
                .min_w(px(280.0))
                .max_w(px(420.0))
                .w_full()
                .child(
                    Input::new(input_state)
                        .appearance(true)
                        .mask_toggle()
                        .cleanable(false),
                ),
        )
        .into_any_element()
}

/// A standard input row with label on the left and Input on the right.
fn input_row(label: &str, input_state: &Entity<InputState>, theme: &HiveTheme) -> AnyElement {
    div()
        .flex()
        .items_start()
        .gap(theme.space_4)
        .py(theme.space_2)
        .child(
            div()
                .flex_1()
                .text_size(theme.font_size_base)
                .text_color(theme.text_secondary)
                .child(label.to_string()),
        )
        .child(
            div()
                .min_w(px(280.0))
                .max_w(px(420.0))
                .w_full()
                .child(Input::new(input_state).appearance(true).cleanable(false)),
        )
        .into_any_element()
}

/// A budget input row with $ prefix label.
fn budget_row(label: &str, input_state: &Entity<InputState>, theme: &HiveTheme) -> AnyElement {
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap(theme.space_4)
        .py(theme.space_1)
        .child(
            div()
                .text_size(theme.font_size_base)
                .text_color(theme.text_secondary)
                .child(label.to_string()),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(theme.space_1)
                .child(
                    div()
                        .text_size(theme.font_size_sm)
                        .text_color(theme.text_muted)
                        .child("$"),
                )
                .child(
                    div()
                        .min_w(px(100.0))
                        .child(Input::new(input_state).appearance(true).cleanable(false)),
                )
                .child(
                    div()
                        .text_size(theme.font_size_xs)
                        .text_color(theme.text_muted)
                        .child("USD"),
                ),
        )
        .into_any_element()
}

/// A toggle row with label on the left and Switch on the right.
fn switch_row<A: Action + Clone>(
    label: &str,
    id: impl Into<ElementId>,
    checked: bool,
    action: A,
    theme: &HiveTheme,
) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap(theme.space_4)
        .py(theme.space_2)
        .child(
            div()
                .flex_1()
                .text_size(theme.font_size_base)
                .text_color(theme.text_secondary)
                .child(label.to_string()),
        )
        .child(
            Switch::new(id)
                .checked(checked)
                .on_click(move |_new_checked, window, cx| {
                    window.dispatch_action(Box::new(action.clone()), cx);
                }),
        )
        .into_any_element()
}

fn settings_action_button<F>(
    label: &str,
    disabled: bool,
    theme: &HiveTheme,
    on_click: F,
) -> AnyElement
where
    F: Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
{
    let label = label.to_string();
    div()
        .px(theme.space_3)
        .py(theme.space_2)
        .rounded(theme.radius_sm)
        .bg(if disabled {
            theme.bg_tertiary
        } else {
            theme.accent_cyan
        })
        .text_size(theme.font_size_xs)
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(if disabled {
            theme.text_muted
        } else {
            theme.text_on_accent
        })
        .hover(|style| style.opacity(0.9))
        .on_mouse_down(MouseButton::Left, move |event, window, cx| {
            if disabled {
                return;
            }
            on_click(event, window, cx);
        })
        .child(label)
        .into_any_element()
}

fn status_banner(text: &str, busy: bool, theme: &HiveTheme) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap(theme.space_2)
        .px(theme.space_3)
        .py(theme.space_2)
        .rounded(theme.radius_sm)
        .bg(theme.bg_primary)
        .border_1()
        .border_color(if busy {
            theme.accent_cyan
        } else {
            theme.border
        })
        .child(
            div()
                .w(px(8.0))
                .h(px(8.0))
                .rounded(theme.radius_full)
                .bg(if busy {
                    theme.accent_cyan
                } else {
                    theme.accent_green
                }),
        )
        .child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .child(text.to_string()),
        )
        .into_any_element()
}

fn format_optional_size(size: Option<u64>) -> String {
    size.map(format_bytes).unwrap_or_else(|| "unknown".into())
}

fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let value = bytes as f64;
    if value >= GB {
        format!("{:.1} GB", value / GB)
    } else if value >= MB {
        format!("{:.1} MB", value / MB)
    } else if value >= KB {
        format!("{:.1} KB", value / KB)
    } else {
        format!("{bytes} B")
    }
}

fn format_pull_progress(status: &str, completed: Option<u64>, total: Option<u64>) -> String {
    match (completed, total) {
        (Some(completed), Some(total)) if total > 0 => {
            let pct = ((completed as f64 / total as f64) * 100.0).round() as u32;
            format!("{status} ({pct}% of {})", format_bytes(total))
        }
        _ => status.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Header
// ---------------------------------------------------------------------------

fn render_header(key_count: usize, theme: &HiveTheme) -> AnyElement {
    let summary = if key_count > 0 {
        format!(
            "{} cloud provider{} connected",
            key_count,
            if key_count == 1 { "" } else { "s" },
        )
    } else {
        "No cloud providers configured -- local-only mode".into()
    };

    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(theme.space_3)
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .w(px(40.0))
                .h(px(40.0))
                .rounded(theme.radius_lg)
                .bg(theme.bg_surface)
                .border_1()
                .border_color(theme.border)
                .child(Icon::new(IconName::Settings).size_4()),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .child(
                    div()
                        .text_size(theme.font_size_2xl)
                        .text_color(theme.text_primary)
                        .font_weight(FontWeight::BOLD)
                        .child("Settings"),
                )
                .child(
                    div()
                        .text_size(theme.font_size_sm)
                        .text_color(theme.text_muted)
                        .child(summary),
                ),
        )
        .into_any_element()
}

fn render_settings_overview(
    key_count: usize,
    discovered_model_count: usize,
    privacy_mode: bool,
    auto_routing: bool,
    theme: &HiveTheme,
) -> AnyElement {
    card(theme)
        .child(
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap(theme.space_2)
                .child(overview_chip(
                    "Cloud Providers",
                    format!("{key_count}/6 configured"),
                    if key_count > 0 {
                        theme.accent_green
                    } else {
                        theme.accent_red
                    },
                    theme,
                ))
                .child(overview_chip(
                    "Local Models",
                    if discovered_model_count > 0 {
                        format!("{discovered_model_count} discovered")
                    } else {
                        "none detected".to_string()
                    },
                    if discovered_model_count > 0 {
                        theme.accent_green
                    } else {
                        theme.text_muted
                    },
                    theme,
                ))
                .child(overview_chip(
                    "Privacy",
                    if privacy_mode {
                        "local-only".to_string()
                    } else {
                        "cloud enabled".to_string()
                    },
                    if privacy_mode {
                        theme.accent_green
                    } else {
                        theme.accent_yellow
                    },
                    theme,
                ))
                .child(overview_chip(
                    "Routing",
                    if auto_routing {
                        "automatic".to_string()
                    } else {
                        "manual default".to_string()
                    },
                    theme.accent_cyan,
                    theme,
                )),
        )
        .into_any_element()
}

fn overview_chip(label: &str, value: String, accent: Hsla, theme: &HiveTheme) -> AnyElement {
    div()
        .px(theme.space_3)
        .py(theme.space_2)
        .rounded(theme.radius_md)
        .bg(theme.bg_primary)
        .border_1()
        .border_color(theme.border)
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(theme.space_2)
                .child(
                    div()
                        .w(px(8.0))
                        .h(px(8.0))
                        .rounded(theme.radius_full)
                        .bg(accent),
                )
                .child(
                    div()
                        .text_size(theme.font_size_xs)
                        .text_color(theme.text_muted)
                        .child(format!("{label}: ")),
                )
                .child(
                    div()
                        .text_size(theme.font_size_sm)
                        .text_color(theme.text_primary)
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(value),
                ),
        )
        .into_any_element()
}
