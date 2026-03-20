use gpui::*;
use std::path::PathBuf;

use super::{
    agents_actions, assistant_refresh, data_refresh, network_actions, plugin_actions,
    project_context, quick_start_actions, skills_actions, terminal_host, workflow_actions,
    AppNotification, AppNotifications, FilesData, HiveWorkspace, MAX_PINNED_WORKSPACES,
    NotificationType, Panel, RemoveRecentWorkspace, ReviewData, SwitchToActivity,
    SwitchToAgents, SwitchToAssistant, SwitchToChannels, SwitchToChat, SwitchToCodeMap,
    SwitchToCosts, SwitchToFiles, SwitchToHelp, SwitchToHistory, SwitchToKanban,
    SwitchToLearning, SwitchToLogs, SwitchToModels, SwitchToMonitor, SwitchToNetwork,
    SwitchToPromptLibrary, SwitchToQuickStart, SwitchToReview, SwitchToRouting,
    SwitchToSettings, SwitchToShield, SwitchToSkills, SwitchToSpecs, SwitchToTerminal,
    SwitchToTokenLaunch, SwitchToWorkflows, SwitchToWorkspace, TogglePinWorkspace,
    ToggleProjectDropdown,
};

pub(super) fn switch_to_workspace(
    workspace: &mut HiveWorkspace,
    workspace_path: PathBuf,
    cx: &mut Context<HiveWorkspace>,
) {
    if !workspace_path.exists() {
        return;
    }

    project_context::apply_project_context(workspace, &workspace_path, cx);
    workspace.files_data = FilesData::from_path(&workspace.current_project_root);
    switch_to_panel(workspace, Panel::Files, cx);
}

pub(super) fn switch_to_panel(
    workspace: &mut HiveWorkspace,
    panel: Panel,
    cx: &mut Context<HiveWorkspace>,
) {
    tracing::info!("SwitchToPanel action: {:?}", panel);
    workspace.show_utility_drawer = false;
    workspace.sidebar.active_panel = panel;
    if let Some(destination) = panel.shell_destination() {
        workspace.sidebar.active_destination = destination;
    }

    match panel {
        Panel::QuickStart => {
            quick_start_actions::refresh_quick_start_data(workspace, cx);
        }
        Panel::History if workspace.history_data.conversations.is_empty() => {
            workspace.history_data = data_refresh::load_history_data();
        }
        Panel::Files if workspace.files_data.entries.is_empty() => {
            workspace.files_data = FilesData::from_path(&workspace.files_data.current_path.clone());
        }
        Panel::Review => {
            workspace.review_data = ReviewData::from_git(&workspace.current_project_root);
        }
        Panel::Costs => {
            data_refresh::refresh_cost_data(workspace, cx);
        }
        Panel::Learning => {
            data_refresh::refresh_learning_data(workspace, cx);
        }
        Panel::Shield => {
            data_refresh::refresh_shield_data(workspace, cx);
        }
        Panel::Routing => {
            data_refresh::refresh_routing_data(workspace, cx);
        }
        Panel::Workflows => {
            workflow_actions::refresh_workflow_builder(workspace, cx);
        }
        Panel::Channels => {
            workflow_actions::refresh_channels_view(workspace, cx);
        }
        Panel::Models => {
            super::settings_actions::push_keys_to_models_browser(workspace, cx);
            workspace.models_browser_view.update(cx, |browser, cx| {
                browser.trigger_fetches(cx);
            });
        }
        Panel::Skills => {
            skills_actions::refresh_skills_data(workspace, cx);
            plugin_actions::trigger_plugin_version_check(workspace, cx);
        }
        Panel::Agents => {
            agents_actions::refresh_agents_data(workspace, cx);
        }
        Panel::Specs => {
            data_refresh::refresh_specs_data(workspace, cx);
        }
        Panel::Assistant => {
            assistant_refresh::refresh_assistant_data(workspace, cx);
            assistant_refresh::refresh_assistant_connected_data(workspace, cx);
        }
        Panel::Monitor => {
            data_refresh::refresh_monitor_data(workspace, cx);
        }
        Panel::Activity => {
            data_refresh::refresh_monitor_data(workspace, cx);
            data_refresh::refresh_learning_data(workspace, cx);
            data_refresh::refresh_shield_data(workspace, cx);
            data_refresh::refresh_activity_data(workspace, cx);
        }
        Panel::Network => {
            network_actions::refresh_network_peer_data(workspace, cx);
        }
        Panel::Logs => {
            data_refresh::refresh_logs_data(workspace, cx);
        }
        Panel::Kanban => {
            data_refresh::refresh_kanban_data(workspace);
        }
        Panel::Terminal => {
            terminal_host::ensure_terminal_shell(workspace, cx);
        }
        _ => {}
    }

    workspace.save_session(cx);
    cx.notify();
}

pub(super) fn handle_switch_to_chat(
    workspace: &mut HiveWorkspace,
    _action: &SwitchToChat,
    window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    switch_to_panel(workspace, Panel::Chat, cx);
    let fh = workspace.chat_input.read(cx).input_focus_handle();
    window.focus(&fh);
}

pub(super) fn handle_open_workspace_directory(
    _workspace: &mut HiveWorkspace,
    _action: &super::OpenWorkspaceDirectory,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
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
                switch_to_workspace(this, workspace_path, cx);
            });
        }
    })
    .detach();
}

pub(super) fn handle_toggle_project_dropdown(
    workspace: &mut HiveWorkspace,
    _action: &ToggleProjectDropdown,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    workspace.show_command_palette = false;
    workspace.show_utility_drawer = false;
    workspace.show_project_dropdown = !workspace.show_project_dropdown;
    cx.notify();
}

pub(super) fn handle_switch_to_workspace_action(
    workspace: &mut HiveWorkspace,
    action: &SwitchToWorkspace,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    workspace.show_project_dropdown = false;
    workspace.show_command_palette = false;
    workspace.show_utility_drawer = false;

    let path = PathBuf::from(&action.path);
    if !path.exists() {
        workspace.recent_workspace_roots.retain(|p| p != &path);
        workspace.pinned_workspace_roots.retain(|p| p != &path);
        workspace.session_dirty = true;
        workspace.save_session(cx);
        if cx.has_global::<AppNotifications>() {
            cx.global_mut::<AppNotifications>()
                .0
                .push(AppNotification::new(
                    NotificationType::Warning,
                    "Project folder not found",
                ));
        }
        cx.notify();
        return;
    }

    switch_to_workspace(workspace, path, cx);
}

pub(super) fn handle_toggle_pin_workspace(
    workspace: &mut HiveWorkspace,
    action: &TogglePinWorkspace,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    let path = PathBuf::from(&action.path);
    if let Some(idx) = workspace.pinned_workspace_roots.iter().position(|p| p == &path) {
        workspace.pinned_workspace_roots.remove(idx);
    } else {
        workspace.pinned_workspace_roots.push(path);
        workspace
            .pinned_workspace_roots
            .truncate(MAX_PINNED_WORKSPACES);
    }
    workspace.session_dirty = true;
    workspace.save_session(cx);
    cx.notify();
}

pub(super) fn handle_remove_recent_workspace(
    workspace: &mut HiveWorkspace,
    action: &RemoveRecentWorkspace,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    let path = PathBuf::from(&action.path);
    if path == workspace.current_project_root {
        return;
    }
    workspace.recent_workspace_roots.retain(|p| p != &path);
    workspace.pinned_workspace_roots.retain(|p| p != &path);
    workspace.session_dirty = true;
    workspace.save_session(cx);
    cx.notify();
}

pub(super) fn handle_switch_to_quick_start(
    workspace: &mut HiveWorkspace,
    _action: &SwitchToQuickStart,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    switch_to_panel(workspace, Panel::QuickStart, cx);
}

pub(super) fn handle_switch_to_history(
    workspace: &mut HiveWorkspace,
    _action: &SwitchToHistory,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    switch_to_panel(workspace, Panel::History, cx);
}

pub(super) fn handle_switch_to_files(
    workspace: &mut HiveWorkspace,
    _action: &SwitchToFiles,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    switch_to_panel(workspace, Panel::Files, cx);
}

pub(super) fn handle_switch_to_code_map(
    workspace: &mut HiveWorkspace,
    _action: &SwitchToCodeMap,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    workspace.code_map_data = hive_ui_panels::panels::code_map::build_code_map_data(cx);
    switch_to_panel(workspace, Panel::CodeMap, cx);
}

pub(super) fn handle_switch_to_prompt_library(
    workspace: &mut HiveWorkspace,
    _action: &SwitchToPromptLibrary,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    workspace.prompt_library_data.refresh();
    switch_to_panel(workspace, Panel::PromptLibrary, cx);
}

pub(super) fn handle_switch_to_kanban(
    workspace: &mut HiveWorkspace,
    _action: &SwitchToKanban,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    switch_to_panel(workspace, Panel::Kanban, cx);
}

pub(super) fn handle_switch_to_monitor(
    workspace: &mut HiveWorkspace,
    _action: &SwitchToMonitor,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    switch_to_panel(workspace, Panel::Monitor, cx);
}

pub(super) fn handle_switch_to_activity(
    workspace: &mut HiveWorkspace,
    _action: &SwitchToActivity,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    switch_to_panel(workspace, Panel::Activity, cx);
}

pub(super) fn handle_switch_to_logs(
    workspace: &mut HiveWorkspace,
    _action: &SwitchToLogs,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    switch_to_panel(workspace, Panel::Logs, cx);
}

pub(super) fn handle_switch_to_costs(
    workspace: &mut HiveWorkspace,
    _action: &SwitchToCosts,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    switch_to_panel(workspace, Panel::Costs, cx);
}

pub(super) fn handle_switch_to_review(
    workspace: &mut HiveWorkspace,
    _action: &SwitchToReview,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    switch_to_panel(workspace, Panel::Review, cx);
}

pub(super) fn handle_switch_to_skills(
    workspace: &mut HiveWorkspace,
    _action: &SwitchToSkills,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    switch_to_panel(workspace, Panel::Skills, cx);
}

pub(super) fn handle_switch_to_routing(
    workspace: &mut HiveWorkspace,
    _action: &SwitchToRouting,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    switch_to_panel(workspace, Panel::Routing, cx);
}

pub(super) fn handle_switch_to_models(
    workspace: &mut HiveWorkspace,
    _action: &SwitchToModels,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    switch_to_panel(workspace, Panel::Models, cx);
}

pub(super) fn handle_switch_to_token_launch(
    workspace: &mut HiveWorkspace,
    _action: &SwitchToTokenLaunch,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    switch_to_panel(workspace, Panel::TokenLaunch, cx);
}

pub(super) fn handle_switch_to_specs(
    workspace: &mut HiveWorkspace,
    _action: &SwitchToSpecs,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    switch_to_panel(workspace, Panel::Specs, cx);
}

pub(super) fn handle_switch_to_agents(
    workspace: &mut HiveWorkspace,
    _action: &SwitchToAgents,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    switch_to_panel(workspace, Panel::Agents, cx);
}

pub(super) fn handle_switch_to_workflows(
    workspace: &mut HiveWorkspace,
    _action: &SwitchToWorkflows,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    switch_to_panel(workspace, Panel::Workflows, cx);
}

pub(super) fn handle_switch_to_channels(
    workspace: &mut HiveWorkspace,
    _action: &SwitchToChannels,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    switch_to_panel(workspace, Panel::Channels, cx);
}

pub(super) fn handle_switch_to_learning(
    workspace: &mut HiveWorkspace,
    _action: &SwitchToLearning,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    switch_to_panel(workspace, Panel::Learning, cx);
}

pub(super) fn handle_switch_to_shield(
    workspace: &mut HiveWorkspace,
    _action: &SwitchToShield,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    switch_to_panel(workspace, Panel::Shield, cx);
}

pub(super) fn handle_switch_to_assistant(
    workspace: &mut HiveWorkspace,
    _action: &SwitchToAssistant,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    switch_to_panel(workspace, Panel::Assistant, cx);
}

pub(super) fn handle_switch_to_settings(
    workspace: &mut HiveWorkspace,
    _action: &SwitchToSettings,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    switch_to_panel(workspace, Panel::Settings, cx);
}

pub(super) fn handle_switch_to_help(
    workspace: &mut HiveWorkspace,
    _action: &SwitchToHelp,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    switch_to_panel(workspace, Panel::Help, cx);
}

pub(super) fn handle_switch_to_network(
    workspace: &mut HiveWorkspace,
    _action: &SwitchToNetwork,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    switch_to_panel(workspace, Panel::Network, cx);
}

pub(super) fn handle_switch_to_terminal(
    workspace: &mut HiveWorkspace,
    _action: &SwitchToTerminal,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    switch_to_panel(workspace, Panel::Terminal, cx);
    terminal_host::ensure_terminal_shell(workspace, cx);
}
