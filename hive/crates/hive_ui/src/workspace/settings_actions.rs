use std::{collections::HashSet, path::PathBuf, sync::Arc};

use gpui::*;
use tracing::{error, info, warn};

use super::{
    AppA2aClient, AppAiService, AppAws, AppAzure, AppBitbucket, AppBrowser, AppConfig,
    AppContextEngine, AppDocker, AppDocsIndexer, AppGcp, AppGitLab, AppHueClient,
    AppIntegrationDb, AppKnowledge, AppKubernetes, AppMcpServer, AppMessaging,
    AppOllamaManager, AppProjectManagement, AppTheme, AppTts, project_context,
    ContextFormatChanged, ExportConfig, HiveConfig, HiveWorkspace, ImportConfig,
    NotificationType, SettingsSave, SettingsView, ThemeChanged,
};

pub(super) fn handle_settings_save(
    _workspace: &mut HiveWorkspace,
    _action: &SettingsSave,
    _window: &mut Window,
    _cx: &mut Context<HiveWorkspace>,
) {
    // Save is handled via the SettingsSaved event from SettingsView.
}

pub(super) fn rebuild_knowledge_hub(
    _workspace: &mut HiveWorkspace,
    cx: &mut Context<HiveWorkspace>,
) {
    if !cx.has_global::<AppConfig>() {
        return;
    }
    let config = cx.global::<AppConfig>().0.get();

    let mut knowledge_hub = hive_integrations::knowledge::KnowledgeHub::new();

    if let Some(ref vault_path) = config.obsidian_vault_path
        && !vault_path.is_empty()
    {
        let mut obsidian = hive_integrations::knowledge::ObsidianProvider::new(vault_path);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build();
        match runtime {
            Ok(runtime) => match runtime.block_on(obsidian.index_vault()) {
                Ok(count) => {
                    info!("Knowledge hub: Obsidian vault re-indexed ({count} pages)");
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

    if let Some(ref notion_key) = config.notion_api_key
        && !notion_key.is_empty()
    {
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

    let provider_count = knowledge_hub.provider_count();
    cx.set_global(AppKnowledge(Arc::new(knowledge_hub)));
    info!("Knowledge hub rebuilt ({provider_count} providers)");
}

pub(super) fn handle_theme_changed(
    workspace: &mut HiveWorkspace,
    action: &ThemeChanged,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    let new_theme = project_context::resolve_theme_by_name(&action.theme_name);
    workspace.theme = new_theme.clone();

    cx.set_global(AppTheme(new_theme.clone()));
    project_context::sync_gpui_theme(&new_theme, cx);

    if cx.has_global::<AppConfig>() {
        let config_manager = &cx.global::<AppConfig>().0;
        if let Err(e) = config_manager.update(|cfg| {
            cfg.theme = action.theme_name.clone();
        }) {
            error!("Failed to persist theme to config: {e}");
        }
    }

    let theme_name_for_settings = action.theme_name.clone();
    workspace.settings_view.update(cx, |view, cx| {
        view.set_theme(new_theme.clone(), cx);
        view.set_selected_theme(theme_name_for_settings, cx);
    });
    workspace.chat_input.update(cx, |view, cx| {
        view.set_theme(new_theme.clone(), cx);
    });
    workspace.models_browser_view.update(cx, |view, cx| {
        view.set_theme(new_theme.clone(), cx);
    });
    workspace.channels_view.update(cx, |view, cx| {
        view.set_theme(new_theme.clone(), cx);
    });
    workspace.workflow_builder_view.update(cx, |view, cx| {
        view.set_theme(new_theme.clone(), cx);
    });

    info!("Theme changed to: {}", action.theme_name);
    cx.notify();
}

pub(super) fn handle_context_format_changed(
    workspace: &mut HiveWorkspace,
    action: &ContextFormatChanged,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    if cx.has_global::<AppConfig>() {
        let format = action.format.clone();
        if let Err(e) = cx.global::<AppConfig>().0.update(|cfg| {
            cfg.context_format = format;
        }) {
            error!("Failed to persist context_format: {e}");
        }
    }
    let format = action.format.clone();
    workspace.settings_view.update(cx, |view, cx| {
        view.set_selected_context_format(format, cx);
    });
    info!("Context format changed to: {}", action.format);
    cx.notify();
}

pub(super) fn handle_export_config(
    workspace: &mut HiveWorkspace,
    _action: &ExportConfig,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    if !cx.has_global::<AppConfig>() {
        warn!("ExportConfig: no AppConfig available");
        return;
    }

    let password = workspace.settings_view.read(cx).backup_password(cx);
    if password.is_empty() {
        workspace.push_notification(
            cx,
            NotificationType::Warning,
            "Config Export",
            "Enter a backup password in Settings -> Import & Export before exporting."
                .to_string(),
        );
        return;
    }

    let blob = match cx.global::<AppConfig>().0.export_config(&password) {
        Ok(blob) => blob,
        Err(e) => {
            error!("ExportConfig: export failed: {e}");
            workspace.push_notification(
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
        .map(|dir| dir.join("exports"))
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
            info!("ExportConfig: wrote {len} bytes to {}", export_path.display());
            workspace.push_notification(
                cx,
                NotificationType::Success,
                "Config Export",
                format!(
                    "Exported encrypted backup to {}\nRe-import it with the same password from Settings -> Import & Export.",
                    export_path.display()
                ),
            );
        }
        Err(e) => {
            error!("ExportConfig: failed to write file: {e}");
            workspace.push_notification(
                cx,
                NotificationType::Error,
                "Config Export",
                format!("Failed to write export file: {e}"),
            );
        }
    }
}

pub(super) fn handle_import_config(
    workspace: &mut HiveWorkspace,
    _action: &ImportConfig,
    window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    if !cx.has_global::<AppConfig>() {
        warn!("ImportConfig: no AppConfig available");
        return;
    }

    let export_dir = match HiveConfig::base_dir().map(|dir| dir.join("exports")) {
        Ok(dir) => dir,
        Err(e) => {
            error!("ImportConfig: cannot resolve exports dir: {e}");
            workspace.push_notification(
                cx,
                NotificationType::Error,
                "Config Import",
                format!("Cannot resolve exports directory: {e}"),
            );
            return;
        }
    };

    let latest_file = std::fs::read_dir(&export_dir).ok().and_then(|entries| {
        entries
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().extension().map_or(false, |ext| ext == "enc"))
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("hive-config-")
            })
            .max_by_key(|entry| {
                entry
                    .metadata()
                    .and_then(|meta| meta.modified())
                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
            })
            .map(|entry| entry.path())
    });

    let import_path = match latest_file {
        Some(path) => path,
        None => {
            warn!(
                "ImportConfig: no export files found in {}",
                export_dir.display()
            );
            workspace.push_notification(
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
        Ok(data) => data,
        Err(e) => {
            error!("ImportConfig: failed to read {}: {e}", import_path.display());
            workspace.push_notification(
                cx,
                NotificationType::Error,
                "Config Import",
                format!("Failed to read {}: {e}", import_path.display()),
            );
            return;
        }
    };

    let password = workspace.settings_view.read(cx).backup_password(cx);
    if password.is_empty() {
        workspace.push_notification(
            cx,
            NotificationType::Warning,
            "Config Import",
            "Enter the backup password in Settings -> Import & Export before importing."
                .to_string(),
        );
        return;
    }

    match cx.global::<AppConfig>().0.import_config(&data, &password) {
        Ok(()) => {
            info!(
                "ImportConfig: successfully imported from {}",
                import_path.display()
            );
            workspace.push_notification(
                cx,
                NotificationType::Success,
                "Config Import",
                format!("Imported config from {}", import_path.display()),
            );

            let config = cx.global::<AppConfig>().0.get();
            workspace.settings_view.update(cx, |view, cx| {
                *view = SettingsView::new(window, cx);
                view.set_selected_theme(config.theme.clone(), cx);
                let context_format = if config.context_format.is_empty() {
                    "markdown".to_string()
                } else {
                    config.context_format.clone()
                };
                view.set_selected_context_format(context_format, cx);
            });
            handle_settings_save_from_view(workspace, cx);
        }
        Err(e) => {
            error!("ImportConfig: import failed: {e}");
            workspace.push_notification(
                cx,
                NotificationType::Error,
                "Config Import",
                format!("Import failed: {e}"),
            );
        }
    }
}

pub(super) fn handle_project_models_changed(
    workspace: &mut HiveWorkspace,
    models: &[String],
    cx: &mut Context<HiveWorkspace>,
) {
    if cx.has_global::<AppConfig>()
        && let Err(e) = cx.global::<AppConfig>().0.update(|cfg| {
            cfg.project_models = models.to_vec();
        })
    {
        warn!("Models: failed to persist project_models: {e}");
    }

    workspace.settings_view.update(cx, |settings, cx| {
        settings.set_project_models(models.to_vec(), cx);
    });

    if cx.has_global::<AppAiService>() {
        cx.global_mut::<AppAiService>()
            .0
            .rebuild_fallback_chain_from_project_models(models);
    }

    if !models.is_empty() {
        let current_model = workspace.chat_service.read(cx).current_model().to_string();
        let model_set: HashSet<String> = models.iter().cloned().collect();
        let is_local = current_model.starts_with("ollama/")
            || current_model.starts_with("lmstudio/")
            || current_model.starts_with("local/");
        if !is_local && !model_set.contains(&current_model) {
            let new_model = models[0].clone();
            info!(
                "Models: active model '{}' not in project set, switching to '{}'",
                current_model, new_model
            );
            workspace.chat_service.update(cx, |svc, _cx| {
                svc.set_model(new_model.clone());
            });
            if cx.has_global::<AppConfig>() {
                let _ = cx.global::<AppConfig>().0.update(|cfg| {
                    cfg.default_model = new_model;
                });
            }
        }
    }

    cx.notify();
}

pub(super) fn handle_settings_save_from_view(
    workspace: &mut HiveWorkspace,
    cx: &mut Context<HiveWorkspace>,
) {
    info!("Settings: persisting from SettingsView");

    let snapshot = workspace.settings_view.read(cx).collect_values(cx);

    if cx.has_global::<AppConfig>() {
        let config_manager = &cx.global::<AppConfig>().0;

        if let Err(e) = config_manager.update(|cfg| {
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
            cfg.obsidian_vault_path = snapshot.obsidian_vault_path.clone();
            cfg.google_oauth_client_id = snapshot.google_oauth_client_id.clone();
            cfg.microsoft_oauth_client_id = snapshot.microsoft_oauth_client_id.clone();
            cfg.github_oauth_client_id = snapshot.github_oauth_client_id.clone();
            cfg.slack_oauth_client_id = snapshot.slack_oauth_client_id.clone();
            cfg.discord_oauth_client_id = snapshot.discord_oauth_client_id.clone();
            cfg.telegram_oauth_client_id = snapshot.telegram_oauth_client_id.clone();
            // Messaging bot tokens
            cfg.slack_bot_token = if snapshot.slack_bot_token.is_empty() {
                None
            } else {
                Some(snapshot.slack_bot_token.clone())
            };
            cfg.discord_bot_token = if snapshot.discord_bot_token.is_empty() {
                None
            } else {
                Some(snapshot.discord_bot_token.clone())
            };
            cfg.telegram_bot_token = if snapshot.telegram_bot_token.is_empty() {
                None
            } else {
                Some(snapshot.telegram_bot_token.clone())
            };
            cfg.whatsapp_phone_id = if snapshot.whatsapp_phone_id.is_empty() {
                None
            } else {
                Some(snapshot.whatsapp_phone_id.clone())
            };
            cfg.whatsapp_access_token = if snapshot.whatsapp_access_token.is_empty() {
                None
            } else {
                Some(snapshot.whatsapp_access_token.clone())
            };
            cfg.signal_api_url = if snapshot.signal_api_url.is_empty() {
                None
            } else {
                Some(snapshot.signal_api_url.clone())
            };
            cfg.matrix_homeserver = if snapshot.matrix_homeserver.is_empty() {
                None
            } else {
                Some(snapshot.matrix_homeserver.clone())
            };
            cfg.matrix_access_token = if snapshot.matrix_access_token.is_empty() {
                None
            } else {
                Some(snapshot.matrix_access_token.clone())
            };
            cfg.google_chat_sa_key = if snapshot.google_chat_sa_key.is_empty() {
                None
            } else {
                Some(snapshot.google_chat_sa_key.clone())
            };
            cfg.webchat_api_token = if snapshot.webchat_api_token.is_empty() {
                None
            } else {
                Some(snapshot.webchat_api_token.clone())
            };
        }) {
            warn!("Settings: failed to save config: {e}");
        }

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
            if let Some(key) = key
                && let Err(e) = config_manager.set_api_key(provider, Some(key.clone()))
            {
                warn!("Settings: failed to save {provider} API key: {e}");
            }
        }

        workspace.status_bar.current_model = if snapshot.default_model.is_empty() {
            "Select Model".to_string()
        } else {
            snapshot.default_model
        };
        workspace.status_bar.privacy_mode = snapshot.privacy_mode;
    }

    if cx.has_global::<AppTts>() {
        cx.global::<AppTts>().0.update_config(|cfg| {
            cfg.enabled = snapshot.tts_enabled;
            cfg.auto_speak = snapshot.tts_auto_speak;
        });
    }

    rebuild_knowledge_hub(workspace, cx);
    refresh_runtime_integrations_from_config(workspace, cx);
    push_keys_to_models_browser(workspace, cx);

    cx.notify();
}

pub(super) fn push_keys_to_models_browser(
    workspace: &mut HiveWorkspace,
    cx: &mut Context<HiveWorkspace>,
) {
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

    workspace.models_browser_view.update(cx, |browser, cx| {
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

fn refresh_runtime_integrations_from_config(
    workspace: &mut HiveWorkspace,
    cx: &mut Context<HiveWorkspace>,
) {
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
                bridge_ip, api_key,
            ))
        });
    cx.set_global(AppHueClient(hue_client));

    rewire_mcp_integrations(workspace, cx);
}

fn rewire_mcp_integrations(workspace: &mut HiveWorkspace, cx: &mut Context<HiveWorkspace>) {
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
        google_drive: None,
        google_sheets: None,
        google_docs: None,
        google_tasks: None,
        google_contacts: None,
        bitbucket: Some(cx.global::<AppBitbucket>().0.clone()),
        gitlab: Some(cx.global::<AppGitLab>().0.clone()),
        webhooks: Arc::new(std::sync::Mutex::new(
            hive_integrations::webhooks::WebhookRegistry::new(),
        )),
    };
    cx.global_mut::<AppMcpServer>().0.wire_integrations(services);

    if cx.has_global::<hive_ui_core::AppCollectiveMemory>()
        && cx.has_global::<AppContextEngine>()
    {
        let collective_memory = Arc::clone(&cx.global::<hive_ui_core::AppCollectiveMemory>().0);
        let context_engine = Arc::clone(&cx.global::<AppContextEngine>().0);
        cx.global_mut::<AppMcpServer>()
            .0
            .wire_memory_tools(collective_memory, context_engine);
    }

    let _ = workspace;
}
