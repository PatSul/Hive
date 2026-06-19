use gpui::*;
use hive_ui_core::{DestructiveActionKind, DestructiveConfirmation};
use tracing::{info, warn};

use super::{
    AppDatabase, HiveWorkspace, LogsClear, LogsSetFilter, LogsSetSearchQuery, LogsToggleAutoScroll,
    destructive_actions,
};

pub(super) fn handle_logs_clear(
    workspace: &mut HiveWorkspace,
    _action: &LogsClear,
    window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    let confirmation = DestructiveConfirmation::for_action(DestructiveActionKind::LogsClear {
        entries: workspace.logs_data.entries.len(),
    });
    destructive_actions::request_confirmation(workspace, confirmation, window, cx);
}

pub(super) fn execute_confirmed_logs_clear(
    workspace: &mut HiveWorkspace,
    cx: &mut Context<HiveWorkspace>,
) {
    info!("Logs: clear");
    workspace.logs_data.entries.clear();
    if cx.has_global::<AppDatabase>() {
        let db = &cx.global::<AppDatabase>().0;
        if let Err(e) = db.clear_logs() {
            warn!("Failed to clear persisted logs: {e}");
        }
    }
    cx.notify();
}

pub(super) fn handle_logs_set_filter(
    workspace: &mut HiveWorkspace,
    action: &LogsSetFilter,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    use hive_ui_panels::panels::logs::LogLevel;

    info!("Logs: set filter to {}", action.level);
    workspace.logs_data.filter = match action.level.as_str() {
        "error" => LogLevel::Error,
        "warning" => LogLevel::Warning,
        "info" => LogLevel::Info,
        _ => LogLevel::Debug,
    };
    cx.notify();
}

pub(super) fn handle_logs_set_search_query(
    workspace: &mut HiveWorkspace,
    action: &LogsSetSearchQuery,
    window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    workspace.logs_data.set_search(action.query.clone());
    if workspace.logs_search_input.read(cx).value() != action.query {
        workspace.logs_search_input.update(cx, |input, cx| {
            input.set_value(action.query.clone(), window, cx);
        });
    }
    cx.notify();
}

pub(super) fn handle_logs_toggle_auto_scroll(
    workspace: &mut HiveWorkspace,
    _action: &LogsToggleAutoScroll,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    workspace.logs_data.auto_scroll = !workspace.logs_data.auto_scroll;
    cx.notify();
}
