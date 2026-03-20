use std::path::PathBuf;

use gpui::*;
use tracing::{error, info, warn};

use super::{
    project_context, AppContextSelection, AppSemanticSearch, FilesClearChecked, FilesCloseViewer,
    FilesData, FilesDeleteEntry, FilesNavigateBack, FilesNavigateTo, FilesNewFile,
    FilesNewFolder, FilesOpenEntry, FilesRefresh, FilesSetSearchQuery, FilesToggleCheck,
    HiveWorkspace,
};

pub(super) fn handle_files_navigate_back(
    workspace: &mut HiveWorkspace,
    _action: &FilesNavigateBack,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    if let Some(parent) = workspace.files_data.current_path.parent() {
        let parent = parent.to_path_buf();
        info!("Files: navigate back to {}", parent.display());
        project_context::apply_project_context(workspace, &parent, cx);
        workspace.files_data = FilesData::from_path(&parent);
        cx.notify();
    }
}

pub(super) fn handle_files_navigate_to(
    workspace: &mut HiveWorkspace,
    action: &FilesNavigateTo,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    let path = PathBuf::from(&action.path);
    info!("Files: navigate to {}", path.display());
    project_context::apply_project_context(workspace, &path, cx);
    workspace.files_data = FilesData::from_path(&path);
    cx.notify();
}

pub(super) fn handle_files_open_entry(
    workspace: &mut HiveWorkspace,
    action: &FilesOpenEntry,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    if action.is_directory {
        let new_path = workspace.files_data.current_path.join(&action.name);
        info!("Files: open directory {}", new_path.display());
        project_context::apply_project_context(workspace, &new_path, cx);
        workspace.files_data = FilesData::from_path(&new_path);
    } else {
        let file_path = workspace.files_data.current_path.join(&action.name);
        let file_path = match file_path.canonicalize() {
            Ok(path) => path,
            Err(e) => {
                error!("Files: cannot resolve path: {e}");
                return;
            }
        };
        let base = match workspace.files_data.current_path.canonicalize() {
            Ok(path) => path,
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
        workspace.files_data.selected_file = Some(action.name.clone());
        workspace.files_data.open_file_viewer(&file_path);
        cx.notify();
    }
}

pub(super) fn handle_files_close_viewer(
    workspace: &mut HiveWorkspace,
    _action: &FilesCloseViewer,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    workspace.files_data.close_file_viewer();
    cx.notify();
}

pub(super) fn handle_files_toggle_check(
    workspace: &mut HiveWorkspace,
    action: &FilesToggleCheck,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    let path = PathBuf::from(&action.path);
    workspace.files_data.toggle_check(&path);
    sync_context_selection(workspace, cx);
    cx.notify();
}

pub(super) fn handle_files_clear_checked(
    workspace: &mut HiveWorkspace,
    _action: &FilesClearChecked,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    workspace.files_data.clear_checked();

    if cx.has_global::<AppContextSelection>() {
        let selection = cx.global::<AppContextSelection>().0.clone();
        if let Ok(mut guard) = selection.lock() {
            guard.selected_files.clear();
            guard.total_tokens = 0;
        }
    }
    cx.notify();
}

pub(super) fn handle_files_set_search_query(
    workspace: &mut HiveWorkspace,
    action: &FilesSetSearchQuery,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    workspace.files_data.search_query = action.query.clone();
    workspace.files_data.semantic_results.clear();

    if !action.query.trim().is_empty()
        && workspace.files_data.filtered_sorted_entries().is_empty()
        && cx.has_global::<AppSemanticSearch>()
    {
        if let Ok(mut semantic) = cx.global::<AppSemanticSearch>().0.lock() {
            workspace.files_data.run_semantic_search(&mut semantic);
        }
    }

    cx.notify();
}

pub(super) fn handle_files_delete_entry(
    workspace: &mut HiveWorkspace,
    action: &FilesDeleteEntry,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    let target = workspace.files_data.current_path.join(&action.name);
    let target = match target.canonicalize() {
        Ok(path) => path,
        Err(e) => {
            error!("Files: cannot resolve path: {e}");
            return;
        }
    };
    let base = match workspace.files_data.current_path.canonicalize() {
        Ok(path) => path,
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
    workspace.files_data = FilesData::from_path(&workspace.files_data.current_path.clone());
    cx.notify();
}

pub(super) fn handle_files_refresh(
    workspace: &mut HiveWorkspace,
    _action: &FilesRefresh,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    info!("Files: refresh");
    workspace.files_data = FilesData::from_path(&workspace.files_data.current_path.clone());
    cx.notify();
}

pub(super) fn handle_files_new_file(
    workspace: &mut HiveWorkspace,
    _action: &FilesNewFile,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    let path = workspace.files_data.current_path.join("untitled.txt");
    info!("Files: create new file {}", path.display());
    if let Err(e) = std::fs::write(&path, "") {
        warn!("Files: failed to create file: {e}");
    }
    workspace.files_data = FilesData::from_path(&workspace.files_data.current_path.clone());
    cx.notify();
}

pub(super) fn handle_files_new_folder(
    workspace: &mut HiveWorkspace,
    _action: &FilesNewFolder,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    let path = workspace.files_data.current_path.join("new_folder");
    info!("Files: create new folder {}", path.display());
    if let Err(e) = std::fs::create_dir(&path) {
        warn!("Files: failed to create folder: {e}");
    }
    workspace.files_data = FilesData::from_path(&workspace.files_data.current_path.clone());
    cx.notify();
}

fn sync_context_selection(workspace: &HiveWorkspace, cx: &App) {
    if !cx.has_global::<AppContextSelection>() {
        return;
    }

    let paths = workspace.files_data.checked_paths();
    let total_tokens: usize = paths
        .iter()
        .map(|path| std::fs::metadata(path).map(|meta| meta.len() as usize).unwrap_or(0) / 4)
        .sum();
    let selection = cx.global::<AppContextSelection>().0.clone();
    if let Ok(mut guard) = selection.lock() {
        guard.selected_files = paths;
        guard.total_tokens = total_tokens;
    }
}
