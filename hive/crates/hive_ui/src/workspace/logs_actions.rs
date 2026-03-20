use gpui::*;
use tracing::{info, warn};

use super::{AppDatabase, HiveWorkspace, LogsClear, LogsSetFilter, LogsToggleAutoScroll};

pub(super) fn handle_logs_clear(
    workspace: &mut HiveWorkspace,
    _action: &LogsClear,
    _window: &mut Window,
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

pub(super) fn handle_logs_toggle_auto_scroll(
    workspace: &mut HiveWorkspace,
    _action: &LogsToggleAutoScroll,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    workspace.logs_data.auto_scroll = !workspace.logs_data.auto_scroll;
    cx.notify();
}
