use std::sync::Arc;

use gpui::*;
use tracing::{info, warn};

use super::{
    skills_actions, AppMarketplace, AppPluginManager, HiveWorkspace, PluginImportCancel,
    PluginImportConfirm, PluginImportFromGitHub, PluginImportFromLocal, PluginImportFromUrl,
    PluginImportOpen, PluginImportToggleSkill, PluginManager, PluginPreview, PluginRemove,
    PluginSource, PluginToggleExpand, PluginToggleSkill, PluginUpdate,
};

pub(super) fn handle_plugin_import_open(
    workspace: &mut HiveWorkspace,
    _action: &PluginImportOpen,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    use hive_ui_panels::panels::skills::ImportState;

    info!("Plugin: import open");
    workspace.skills_data.import_state = ImportState::SelectMethod;
    cx.notify();
}

pub(super) fn handle_plugin_import_cancel(
    workspace: &mut HiveWorkspace,
    _action: &PluginImportCancel,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    use hive_ui_panels::panels::skills::ImportState;

    info!("Plugin: import cancel");
    workspace.skills_data.import_state = ImportState::Closed;
    cx.notify();
}

pub(super) fn handle_plugin_import_confirm(
    workspace: &mut HiveWorkspace,
    _action: &PluginImportConfirm,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    use hive_ui_panels::panels::skills::ImportState;

    info!("Plugin: import confirm");
    if let ImportState::Preview(ref preview) = workspace.skills_data.import_state {
        let selected_skills: Vec<usize> = preview
            .skills
            .iter()
            .enumerate()
            .filter(|(_, skill)| skill.selected)
            .map(|(index, _)| index)
            .collect();
        let selected_commands: Vec<usize> = preview
            .commands
            .iter()
            .enumerate()
            .filter(|(_, command)| command.selected)
            .map(|(index, _)| index)
            .collect();

        workspace.skills_data.import_state = ImportState::Installing;
        cx.notify();

        if let Some((backend_preview, source)) = workspace.pending_plugin_preview.take() {
            if cx.has_global::<AppMarketplace>() {
                let marketplace = &mut cx.global_mut::<AppMarketplace>().0;
                let installed = marketplace.install_plugin(
                    &backend_preview,
                    source,
                    &selected_skills,
                    &selected_commands,
                );
                info!("Plugin installed: {} v{}", installed.name, installed.version);

                let plugins_path = dirs::home_dir()
                    .unwrap_or_default()
                    .join(".hive")
                    .join("plugins.json");
                if let Err(e) = marketplace.save_plugins_to_file(&plugins_path) {
                    warn!("Failed to save plugins: {e}");
                }
            }

            workspace.skills_data.import_state = ImportState::Done(
                format!(
                    "Plugin '{}' installed successfully",
                    backend_preview.manifest.name
                ),
                true,
            );
        } else {
            workspace.skills_data.import_state = ImportState::Done(
                "No plugin data available - try importing again".into(),
                false,
            );
        }

        skills_actions::refresh_skills_data(workspace, cx);
        cx.notify();
    }
}

pub(super) fn handle_plugin_import_toggle_skill(
    workspace: &mut HiveWorkspace,
    action: &PluginImportToggleSkill,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    use hive_ui_panels::panels::skills::ImportState;

    info!("Plugin: toggle import skill at index {}", action.index);
    if let ImportState::Preview(ref mut preview) = workspace.skills_data.import_state
        && let Some(skill) = preview.skills.get_mut(action.index)
    {
        skill.selected = !skill.selected;
        cx.notify();
    }
}

pub(super) fn handle_plugin_remove(
    workspace: &mut HiveWorkspace,
    action: &PluginRemove,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    info!("Plugin: remove '{}'", action.plugin_id);

    if cx.has_global::<AppMarketplace>() {
        let marketplace = &mut cx.global_mut::<AppMarketplace>().0;
        if let Err(e) = marketplace.remove_plugin(&action.plugin_id) {
            warn!("Failed to remove plugin '{}': {e}", action.plugin_id);
        }
        let plugins_path = dirs::home_dir()
            .unwrap_or_default()
            .join(".hive")
            .join("plugins.json");
        if let Err(e) = marketplace.save_plugins_to_file(&plugins_path) {
            warn!("Failed to save plugins: {e}");
        }
    }

    skills_actions::refresh_skills_data(workspace, cx);
    cx.notify();
}

pub(super) fn handle_plugin_toggle_expand(
    workspace: &mut HiveWorkspace,
    action: &PluginToggleExpand,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    info!("Plugin: toggle expand '{}'", action.plugin_id);

    if let Some(plugin) = workspace
        .skills_data
        .installed_plugins
        .iter_mut()
        .find(|plugin| plugin.id == action.plugin_id)
    {
        plugin.expanded = !plugin.expanded;
        cx.notify();
    }
}

pub(super) fn handle_plugin_toggle_skill(
    workspace: &mut HiveWorkspace,
    action: &PluginToggleSkill,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    info!(
        "Plugin: toggle skill '{}' in '{}'",
        action.skill_name, action.plugin_id
    );

    if cx.has_global::<AppMarketplace>() {
        let marketplace = &mut cx.global_mut::<AppMarketplace>().0;
        if let Err(e) = marketplace.toggle_plugin_skill(&action.plugin_id, &action.skill_name) {
            warn!(
                "Failed to toggle skill '{}' in plugin '{}': {e}",
                action.skill_name, action.plugin_id
            );
        }
        let plugins_path = dirs::home_dir()
            .unwrap_or_default()
            .join(".hive")
            .join("plugins.json");
        if let Err(e) = marketplace.save_plugins_to_file(&plugins_path) {
            warn!("Failed to save plugins: {e}");
        }
    }

    skills_actions::refresh_skills_data(workspace, cx);
    cx.notify();
}

pub(super) fn handle_plugin_import_from_github(
    workspace: &mut HiveWorkspace,
    action: &PluginImportFromGitHub,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    use hive_ui_panels::panels::skills::ImportState;

    info!("Plugin: import from GitHub '{}'", action.owner_repo);
    if action.owner_repo.is_empty() {
        workspace.skills_data.import_state = ImportState::InputGitHub(String::new());
        cx.notify();
        return;
    }

    let parts: Vec<&str> = action.owner_repo.splitn(2, '/').collect();
    if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
        workspace.skills_data.import_state = ImportState::Done(
            "Invalid format. Use owner/repo (e.g. obra/superpowers)".into(),
            false,
        );
        cx.notify();
        return;
    }

    let owner = parts[0].to_string();
    let repo = parts[1].to_string();
    workspace.skills_data.import_state = ImportState::Fetching;
    cx.notify();

    let plugin_manager = cx.global::<AppPluginManager>().0.clone();
    let source = PluginSource::GitHub {
        owner: owner.clone(),
        repo: repo.clone(),
        branch: None,
    };

    let result_flag: Arc<std::sync::Mutex<Option<Result<PluginPreview, String>>>> =
        Arc::new(std::sync::Mutex::new(None));
    let result_for_thread = Arc::clone(&result_flag);

    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build();
        match runtime {
            Ok(runtime) => {
                let result = runtime.block_on(plugin_manager.fetch_from_github(&owner, &repo));
                *result_for_thread.lock().unwrap_or_else(|e| e.into_inner()) =
                    Some(result.map_err(|e| format!("{e:#}")));
            }
            Err(e) => {
                *result_for_thread.lock().unwrap_or_else(|e| e.into_inner()) =
                    Some(Err(format!("Failed to create async runtime: {e}")));
            }
        }
    });

    poll_preview_result(
        cx,
        result_flag,
        source,
        "GitHub fetch failed",
        std::time::Duration::from_millis(150),
    );
}

pub(super) fn handle_plugin_import_from_url(
    workspace: &mut HiveWorkspace,
    action: &PluginImportFromUrl,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    use hive_ui_panels::panels::skills::ImportState;

    info!("Plugin: import from URL '{}'", action.url);
    if action.url.is_empty() {
        workspace.skills_data.import_state = ImportState::InputUrl(String::new());
        cx.notify();
        return;
    }

    if !action.url.starts_with("http://") && !action.url.starts_with("https://") {
        workspace.skills_data.import_state = ImportState::Done(
            "Invalid URL. Must start with http:// or https://".into(),
            false,
        );
        cx.notify();
        return;
    }

    let url = action.url.clone();
    workspace.skills_data.import_state = ImportState::Fetching;
    cx.notify();

    let plugin_manager = cx.global::<AppPluginManager>().0.clone();
    let source = PluginSource::Url(url.clone());

    let result_flag: Arc<std::sync::Mutex<Option<Result<PluginPreview, String>>>> =
        Arc::new(std::sync::Mutex::new(None));
    let result_for_thread = Arc::clone(&result_flag);

    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build();
        match runtime {
            Ok(runtime) => {
                let result = runtime.block_on(plugin_manager.fetch_from_url(&url));
                *result_for_thread.lock().unwrap_or_else(|e| e.into_inner()) =
                    Some(result.map_err(|e| format!("{e:#}")));
            }
            Err(e) => {
                *result_for_thread.lock().unwrap_or_else(|e| e.into_inner()) =
                    Some(Err(format!("Failed to create async runtime: {e}")));
            }
        }
    });

    poll_preview_result(
        cx,
        result_flag,
        source,
        "URL fetch failed",
        std::time::Duration::from_millis(150),
    );
}

pub(super) fn handle_plugin_import_from_local(
    workspace: &mut HiveWorkspace,
    action: &PluginImportFromLocal,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    use hive_ui_panels::panels::skills::ImportState;

    info!("Plugin: import from local '{}'", action.path);
    if action.path.is_empty() {
        workspace.skills_data.import_state = ImportState::InputLocal(None);
        cx.notify();
        return;
    }

    let path_str = action.path.clone();
    let path = std::path::Path::new(&path_str);
    if !path.exists() {
        workspace.skills_data.import_state =
            ImportState::Done(format!("Path does not exist: {}", action.path), false);
        cx.notify();
        return;
    }

    workspace.skills_data.import_state = ImportState::Fetching;
    cx.notify();

    let source = PluginSource::Local {
        path: path_str.clone(),
    };
    let result_flag: Arc<std::sync::Mutex<Option<Result<PluginPreview, String>>>> =
        Arc::new(std::sync::Mutex::new(None));
    let result_for_thread = Arc::clone(&result_flag);

    std::thread::spawn(move || {
        let result = PluginManager::load_from_local(std::path::Path::new(&path_str));
        *result_for_thread.lock().unwrap_or_else(|e| e.into_inner()) =
            Some(result.map_err(|e| format!("{e:#}")));
    });

    poll_preview_result(
        cx,
        result_flag,
        source,
        "Local load failed",
        std::time::Duration::from_millis(100),
    );
}

pub(super) fn handle_plugin_update(
    workspace: &mut HiveWorkspace,
    action: &PluginUpdate,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    use hive_ui_panels::panels::skills::ImportState;

    info!("Plugin: update '{}'", action.plugin_id);
    let plugin_source = if cx.has_global::<AppMarketplace>() {
        let marketplace = &cx.global::<AppMarketplace>().0;
        marketplace
            .installed_plugins()
            .iter()
            .find(|plugin| plugin.id == action.plugin_id)
            .map(|plugin| plugin.source.clone())
    } else {
        None
    };

    let Some(source) = plugin_source else {
        workspace.skills_data.import_state =
            ImportState::Done(format!("Plugin '{}' not found", action.plugin_id), false);
        cx.notify();
        return;
    };

    let plugin_id = action.plugin_id.clone();
    if cx.has_global::<AppMarketplace>() {
        let marketplace = &mut cx.global_mut::<AppMarketplace>().0;
        let _ = marketplace.remove_plugin(&plugin_id);
        let plugins_path = dirs::home_dir()
            .unwrap_or_default()
            .join(".hive")
            .join("plugins.json");
        let _ = marketplace.save_plugins_to_file(&plugins_path);
    }

    workspace.skills_data.import_state = ImportState::Fetching;
    skills_actions::refresh_skills_data(workspace, cx);
    cx.notify();

    match source {
        PluginSource::GitHub { owner, repo, .. } => {
            let plugin_manager = cx.global::<AppPluginManager>().0.clone();
            let source_clone = PluginSource::GitHub {
                owner: owner.clone(),
                repo: repo.clone(),
                branch: None,
            };
            let result_flag: Arc<std::sync::Mutex<Option<Result<PluginPreview, String>>>> =
                Arc::new(std::sync::Mutex::new(None));
            let result_for_thread = Arc::clone(&result_flag);

            std::thread::spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build();
                match runtime {
                    Ok(runtime) => {
                        let result = runtime.block_on(plugin_manager.fetch_from_github(&owner, &repo));
                        *result_for_thread.lock().unwrap_or_else(|e| e.into_inner()) =
                            Some(result.map_err(|e| format!("{e:#}")));
                    }
                    Err(e) => {
                        *result_for_thread.lock().unwrap_or_else(|e| e.into_inner()) =
                            Some(Err(format!("Failed to create async runtime: {e}")));
                    }
                }
            });

            poll_preview_result(
                cx,
                result_flag,
                source_clone,
                "Update fetch failed",
                std::time::Duration::from_millis(150),
            );
        }
        PluginSource::Url(url) => {
            let plugin_manager = cx.global::<AppPluginManager>().0.clone();
            let source_clone = PluginSource::Url(url.clone());
            let result_flag: Arc<std::sync::Mutex<Option<Result<PluginPreview, String>>>> =
                Arc::new(std::sync::Mutex::new(None));
            let result_for_thread = Arc::clone(&result_flag);

            std::thread::spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build();
                match runtime {
                    Ok(runtime) => {
                        let result = runtime.block_on(plugin_manager.fetch_from_url(&url));
                        *result_for_thread.lock().unwrap_or_else(|e| e.into_inner()) =
                            Some(result.map_err(|e| format!("{e:#}")));
                    }
                    Err(e) => {
                        *result_for_thread.lock().unwrap_or_else(|e| e.into_inner()) =
                            Some(Err(format!("Failed to create async runtime: {e}")));
                    }
                }
            });

            poll_preview_result(
                cx,
                result_flag,
                source_clone,
                "Update fetch failed",
                std::time::Duration::from_millis(150),
            );
        }
        PluginSource::Local { path } => {
            let source_clone = PluginSource::Local { path: path.clone() };
            let result_flag: Arc<std::sync::Mutex<Option<Result<PluginPreview, String>>>> =
                Arc::new(std::sync::Mutex::new(None));
            let result_for_thread = Arc::clone(&result_flag);

            std::thread::spawn(move || {
                let result = PluginManager::load_from_local(std::path::Path::new(&path));
                *result_for_thread.lock().unwrap_or_else(|e| e.into_inner()) =
                    Some(result.map_err(|e| format!("{e:#}")));
            });

            poll_preview_result(
                cx,
                result_flag,
                source_clone,
                "Update load failed",
                std::time::Duration::from_millis(100),
            );
        }
    }
}

pub(super) fn trigger_plugin_version_check(
    workspace: &mut HiveWorkspace,
    cx: &mut Context<HiveWorkspace>,
) {
    use hive_agents::plugin_types::PluginCache;

    if !cx.has_global::<AppPluginManager>() || !cx.has_global::<AppMarketplace>() {
        return;
    }

    let plugin_manager = cx.global::<AppPluginManager>().0.clone();
    let plugins: Vec<_> = cx.global::<AppMarketplace>().0.installed_plugins().to_vec();
    if plugins.is_empty() {
        return;
    }

    let cache_path = dirs::home_dir()
        .unwrap_or_default()
        .join(".hive")
        .join("plugin_cache.json");
    let mut cache = if cache_path.exists() {
        std::fs::read_to_string(&cache_path)
            .ok()
            .and_then(|json| serde_json::from_str::<PluginCache>(&json).ok())
            .unwrap_or_default()
    } else {
        PluginCache::default()
    };

    if let Some(last_checked) = cache.last_checked
        && (chrono::Utc::now() - last_checked).num_seconds() < 3600
    {
        for ui_plugin in &mut workspace.skills_data.installed_plugins {
            if let Some(cached) = cache.versions.get(&ui_plugin.id)
                && cached.latest_version != ui_plugin.version
            {
                ui_plugin.update_available = Some(cached.latest_version.clone());
            }
        }
        cx.notify();
        return;
    }

    let result_flag: Arc<
        std::sync::Mutex<Option<(Vec<hive_agents::plugin_types::UpdateAvailable>, PluginCache)>>,
    > = Arc::new(std::sync::Mutex::new(None));
    let result_for_thread = Arc::clone(&result_flag);
    let cache_path_for_thread = cache_path.clone();

    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build();
        if let Ok(runtime) = runtime {
            let updates = runtime.block_on(plugin_manager.check_for_updates(&plugins, &mut cache));
            if let Ok(json) = serde_json::to_string_pretty(&cache) {
                let _ = std::fs::write(&cache_path_for_thread, json);
            }
            *result_for_thread.lock().unwrap_or_else(|e| e.into_inner()) = Some((updates, cache));
        }
    });

    let result_for_ui = Arc::clone(&result_flag);
    cx.spawn(async move |this: WeakEntity<HiveWorkspace>, app: &mut AsyncApp| {
        loop {
            if let Some((updates, _cache)) = result_for_ui
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .take()
            {
                if !updates.is_empty() {
                    let _ = this.update(app, |workspace, cx| {
                        for update in &updates {
                            if let Some(ui_plugin) = workspace
                                .skills_data
                                .installed_plugins
                                .iter_mut()
                                .find(|plugin| plugin.id == update.plugin_id)
                            {
                                ui_plugin.update_available =
                                    Some(update.latest_version.clone());
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

fn make_import_preview(preview: &PluginPreview) -> hive_ui_panels::panels::skills::ImportPreview {
    use hive_ui_panels::panels::skills::{ImportCommandEntry, ImportPreview, ImportSkillEntry};

    ImportPreview {
        name: preview.manifest.name.clone(),
        version: preview.manifest.version.clone(),
        author: preview.manifest.author.name.clone(),
        description: preview.manifest.description.clone(),
        skills: preview
            .skills
            .iter()
            .map(|skill| ImportSkillEntry {
                name: skill.name.clone(),
                description: skill.description.clone(),
                selected: true,
            })
            .collect(),
        commands: preview
            .commands
            .iter()
            .map(|command| ImportCommandEntry {
                name: command.name.clone(),
                description: command.description.clone(),
                selected: true,
            })
            .collect(),
        security_warnings: preview
            .security_warnings
            .iter()
            .map(|warning| format!("[{:?}] {}", warning.severity, warning.description))
            .collect(),
    }
}

fn poll_preview_result(
    cx: &mut Context<HiveWorkspace>,
    result_flag: Arc<std::sync::Mutex<Option<Result<PluginPreview, String>>>>,
    source: PluginSource,
    error_prefix: &'static str,
    poll_interval: std::time::Duration,
) {
    let result_for_ui = Arc::clone(&result_flag);
    cx.spawn(async move |this: WeakEntity<HiveWorkspace>, app: &mut AsyncApp| {
        use hive_ui_panels::panels::skills::ImportState;

        loop {
            if let Some(result) = result_for_ui
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .take()
            {
                let _ = this.update(app, |workspace, cx| {
                    match result {
                        Ok(preview) => {
                            let ui_preview = make_import_preview(&preview);
                            workspace.pending_plugin_preview = Some((preview, source));
                            workspace.skills_data.import_state = ImportState::Preview(ui_preview);
                        }
                        Err(e) => {
                            workspace.skills_data.import_state =
                                ImportState::Done(format!("{error_prefix}: {e}"), false);
                        }
                    }
                    cx.notify();
                });
                break;
            }
            app.background_executor().timer(poll_interval).await;
        }
    })
    .detach();
}
