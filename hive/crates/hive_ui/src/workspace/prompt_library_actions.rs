use gpui::*;
use tracing::{error, info};

use super::{
    navigation, HiveWorkspace, Panel, PromptLibraryDelete, PromptLibraryLoad,
    PromptLibraryRefresh, PromptLibrarySaveCurrent,
};

pub(super) fn handle_prompt_library_save_current(
    workspace: &mut HiveWorkspace,
    _action: &PromptLibrarySaveCurrent,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    use hive_agents::prompt_template;

    let instruction = workspace.chat_input.read(cx).current_text(cx);
    if instruction.trim().is_empty() {
        return;
    }

    let context_files: Vec<String> = workspace
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
        workspace.prompt_library_data.refresh();
        cx.notify();
    }
}

pub(super) fn handle_prompt_library_refresh(
    workspace: &mut HiveWorkspace,
    _action: &PromptLibraryRefresh,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    workspace.prompt_library_data.refresh();
    cx.notify();
}

pub(super) fn handle_prompt_library_load(
    workspace: &mut HiveWorkspace,
    action: &PromptLibraryLoad,
    window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    use hive_agents::prompt_template;

    match prompt_template::load_template(&action.prompt_id) {
        Ok(template) => {
            workspace.chat_input.update(cx, |input, cx| {
                input.set_text(&template.instruction, window, cx);
            });

            for file in &template.context_files {
                if std::path::Path::new(file).is_absolute() || file.contains("..") {
                    tracing::warn!("Skipping unsafe context file path: {file}");
                    continue;
                }
                let path = std::path::PathBuf::from(file);
                if !workspace.files_data.checked_files.contains(&path) {
                    workspace.files_data.checked_files.insert(path);
                }
            }

            navigation::switch_to_panel(workspace, Panel::Chat, cx);
            info!("Loaded prompt template: {}", template.name);
        }
        Err(e) => {
            error!("Failed to load prompt template: {e}");
        }
    }
}

pub(super) fn handle_prompt_library_delete(
    workspace: &mut HiveWorkspace,
    action: &PromptLibraryDelete,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    use hive_agents::prompt_template;

    if let Err(e) = prompt_template::delete_template(&action.prompt_id) {
        error!("Failed to delete prompt template: {e}");
    } else {
        workspace.prompt_library_data.refresh();
        cx.notify();
    }
}
