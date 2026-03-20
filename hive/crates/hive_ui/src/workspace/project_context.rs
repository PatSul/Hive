use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui::*;
use gpui_component::theme::Theme as GpuiTheme;
use tracing::{info, warn};

use hive_core::session::SessionState;
use hive_core::theme_manager::ThemeManager;
use hive_ui_core::{AppHiveMemory, AppKnowledgeFiles, AppQuickIndex, AppRagService, HiveTheme};

use super::{
    quick_start_actions, HiveWorkspace, MAX_PINNED_WORKSPACES, MAX_RECENT_WORKSPACES,
};

/// Resolve a `HiveTheme` from a theme name string.
///
/// * `"dark"` / `"light"` map to the built-in constructors.
/// * Any other value is matched (case-insensitive) against the
///   `ThemeManager::builtin_themes()` catalog and custom themes on disk.
/// * Falls back to `HiveTheme::dark()` if no match is found.
pub(super) fn resolve_theme_by_name(name: &str) -> HiveTheme {
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
pub(super) fn sync_gpui_theme(theme: &HiveTheme, cx: &mut App) {
    if cx.has_global::<GpuiTheme>() {
        let gpui_theme = GpuiTheme::global_mut(cx);
        gpui_theme.foreground = theme.text_primary;
        gpui_theme.muted_foreground = theme.text_muted;
        gpui_theme.background = theme.bg_primary;
        gpui_theme.input = theme.bg_surface;
    }
}

pub(super) fn resolve_project_root_from_session(session: &SessionState) -> PathBuf {
    let fallback = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let requested = session
        .working_directory
        .as_deref()
        .map(PathBuf::from)
        .unwrap_or_else(|| fallback.clone());

    let requested = if requested.exists() {
        requested
    } else {
        fallback
    };
    discover_project_root(&requested)
}

pub(super) fn load_recent_workspace_roots(
    session: &SessionState,
    current_project_root: &Path,
) -> Vec<PathBuf> {
    let mut recents = Vec::new();
    let current_root = discover_project_root(current_project_root);
    recents.push(current_root);

    for path in &session.recent_workspaces {
        let path = PathBuf::from(path);
        if !path.exists() {
            continue;
        }

        let root = discover_project_root(&path);
        if !recents.contains(&root) {
            recents.push(root);
        }
    }

    recents.truncate(MAX_RECENT_WORKSPACES);
    recents
}

pub(super) fn load_pinned_workspace_roots(session: &SessionState) -> Vec<PathBuf> {
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

pub(super) fn record_recent_workspace(
    workspace: &mut HiveWorkspace,
    workspace_root: &Path,
    cx: &mut Context<HiveWorkspace>,
) {
    if !workspace_root.exists() {
        return;
    }

    let project_root = discover_project_root(workspace_root);
    let mut changed = false;

    if let Some(existing) = workspace
        .recent_workspace_roots
        .iter()
        .position(|path| path == &project_root)
    {
        if existing == 0 {
            return;
        }
        workspace.recent_workspace_roots.remove(existing);
        changed = true;
    }

    if !workspace.recent_workspace_roots.contains(&project_root) {
        changed = true;
    }

    if !changed {
        return;
    }

    workspace.recent_workspace_roots.insert(0, project_root);
    workspace
        .recent_workspace_roots
        .truncate(MAX_RECENT_WORKSPACES);

    workspace.session_dirty = true;
    workspace.save_session(cx);
    cx.notify();
}

pub(super) fn discover_project_root(path: &Path) -> PathBuf {
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

pub(super) fn project_name_from_path(path: &Path) -> String {
    path.file_name()
        .unwrap_or(path.as_os_str())
        .to_string_lossy()
        .to_string()
}

pub(super) fn project_label(workspace: &HiveWorkspace) -> String {
    format!(
        "{} [{}]",
        workspace.current_project_name,
        workspace.current_project_root.display()
    )
}

pub(super) fn start_background_project_indexing(
    workspace: &HiveWorkspace,
    cx: &mut Context<HiveWorkspace>,
) {
    let hive_mem = cx
        .has_global::<AppHiveMemory>()
        .then(|| cx.global::<AppHiveMemory>().0.clone());
    let rag_service = cx
        .has_global::<AppRagService>()
        .then(|| cx.global::<AppRagService>().0.clone());

    if hive_mem.is_none() && rag_service.is_none() {
        return;
    }

    let project_root = workspace.current_project_root.clone();
    std::thread::Builder::new()
        .name("hive-indexer".into())
        .spawn(move || {
            let entries = hive_ai::memory::BackgroundIndexer::collect_indexable_files(&project_root);
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

pub(super) fn apply_project_context(
    workspace: &mut HiveWorkspace,
    cwd: &Path,
    cx: &mut Context<HiveWorkspace>,
) {
    let project_root = discover_project_root(cwd);
    if project_root != workspace.current_project_root {
        workspace.current_project_root = project_root;
        workspace.current_project_name = project_name_from_path(&workspace.current_project_root);
        workspace.status_bar.active_project = project_label(workspace);
        workspace.session_dirty = true;
        workspace.save_session(cx);

        // Re-scan knowledge files for the new project root.
        let knowledge_sources = hive_ai::KnowledgeFileScanner::scan(&workspace.current_project_root);
        if !knowledge_sources.is_empty() {
            info!(
                "Re-scanned {} project knowledge file(s) for {}",
                knowledge_sources.len(),
                workspace.current_project_root.display()
            );
        }
        cx.set_global(AppKnowledgeFiles(knowledge_sources));

        // Rebuild the fast-path project index in a background thread so the UI
        // stays responsive during the <3s indexing pass.
        workspace.quick_index = None;
        let index_root = workspace.current_project_root.clone();
        let result_slot: Arc<std::sync::Mutex<Option<Arc<hive_ai::quick_index::QuickIndex>>>> =
            Arc::new(std::sync::Mutex::new(None));
        let slot_for_thread = Arc::clone(&result_slot);
        std::thread::spawn(move || {
            let start = std::time::Instant::now();
            let qi = hive_ai::quick_index::QuickIndex::build(&index_root);
            let elapsed = start.elapsed();
            info!(
                "QuickIndex rebuilt: {} files, {} symbols, {} deps in {:.2?}",
                qi.file_tree.total_files,
                qi.key_symbols.len(),
                qi.dependencies.len(),
                elapsed,
            );
            *slot_for_thread.lock().unwrap_or_else(|e| e.into_inner()) = Some(Arc::new(qi));
        });
        let slot_for_poll = Arc::clone(&result_slot);
        workspace._quick_index_task = Some(cx.spawn(
            async move |this: WeakEntity<HiveWorkspace>, app: &mut AsyncApp| {
                loop {
                    if let Some(qi) = slot_for_poll.lock().unwrap_or_else(|e| e.into_inner()).take()
                    {
                        let _ = this.update(app, |this, cx| {
                            this.quick_index = Some(qi.clone());
                            cx.set_global(AppQuickIndex(qi));
                            cx.notify();
                        });
                        break;
                    }
                    app.background_executor()
                        .timer(std::time::Duration::from_millis(100))
                        .await;
                }
            },
        ));
        start_background_project_indexing(workspace, cx);

        // Start incremental file watcher for RAG indexing.
        let rag_for_watcher = cx
            .has_global::<AppRagService>()
            .then(|| cx.global::<AppRagService>().0.clone());
        if let Some(rag_svc) = rag_for_watcher {
            let project_root = workspace.current_project_root.clone();
            match hive_fs::FileWatcher::new(&workspace.current_project_root, move |event| {
                let path = match &event {
                    hive_fs::WatchEvent::Created(p) | hive_fs::WatchEvent::Modified(p) => {
                        Some(p.clone())
                    }
                    hive_fs::WatchEvent::Renamed { to, .. } => Some(to.clone()),
                    hive_fs::WatchEvent::Deleted(_) => None,
                };
                if let Some(path) = path {
                    // Only index files with common code extensions.
                    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                    let indexable = matches!(
                        ext,
                        "rs" | "py"
                            | "js"
                            | "ts"
                            | "tsx"
                            | "jsx"
                            | "go"
                            | "java"
                            | "c"
                            | "cpp"
                            | "h"
                            | "hpp"
                            | "rb"
                            | "swift"
                            | "kt"
                            | "md"
                            | "txt"
                            | "toml"
                            | "yaml"
                            | "yml"
                            | "json"
                    );
                    if indexable && let Ok(content) = std::fs::read_to_string(&path) {
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
            }) {
                Ok(watcher) => {
                    workspace._file_watcher = Some(watcher);
                    info!(
                        "RAG file watcher started for {}",
                        workspace.current_project_root.display()
                    );
                }
                Err(e) => {
                    warn!("RAG file watcher failed to start: {e}");
                    workspace._file_watcher = None;
                }
            }
        }

        quick_start_actions::refresh_quick_start_data(workspace, cx);

        cx.notify();
    } else if workspace.current_project_name.is_empty() {
        workspace.current_project_name = project_name_from_path(&workspace.current_project_root);
        workspace.status_bar.active_project = project_label(workspace);
        quick_start_actions::refresh_quick_start_data(workspace, cx);
        cx.notify();
    }

    let project_root = workspace.current_project_root.clone();
    record_recent_workspace(workspace, &project_root, cx);
}
