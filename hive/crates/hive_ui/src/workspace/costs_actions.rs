use std::path::PathBuf;

use gpui::*;
use hive_ui_core::{DestructiveActionKind, DestructiveConfirmation};
use tracing::{error, info};

use super::{
    AppAiService, HiveConfig, HiveWorkspace, NotificationType, data_refresh, destructive_actions,
};

pub(super) fn handle_costs_export_csv(
    workspace: &mut HiveWorkspace,
    _action: &super::CostsExportCsv,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    info!("Costs: export CSV");
    let Some(csv) = cx
        .has_global::<AppAiService>()
        .then(|| cx.global::<AppAiService>().0.cost_tracker().export_csv())
    else {
        workspace.push_notification(
            cx,
            NotificationType::Warning,
            "Cost Export",
            "No cost tracker available.",
        );
        return;
    };

    let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
    let export_dir = HiveConfig::base_dir()
        .map(|dir| dir.join("exports"))
        .unwrap_or_else(|_| PathBuf::from(".hive/exports"));
    let export_path = export_dir.join(format!("costs-{timestamp}.csv"));

    let result = (|| -> anyhow::Result<()> {
        std::fs::create_dir_all(&export_dir)?;
        std::fs::write(&export_path, csv)?;
        Ok(())
    })();

    match result {
        Ok(()) => {
            workspace.push_notification(
                cx,
                NotificationType::Success,
                "Cost Export",
                format!("Exported CSV to {}", export_path.display()),
            );
        }
        Err(e) => {
            error!("Costs: failed to export CSV: {e}");
            workspace.push_notification(
                cx,
                NotificationType::Error,
                "Cost Export",
                format!("Failed to export CSV: {e}"),
            );
        }
    }
}

pub(super) fn handle_costs_reset_today(
    workspace: &mut HiveWorkspace,
    _action: &super::CostsResetToday,
    window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    let confirmation = DestructiveConfirmation::for_action(DestructiveActionKind::CostsResetToday);
    destructive_actions::request_confirmation(workspace, confirmation, window, cx);
}

pub(super) fn execute_confirmed_costs_reset_today(
    workspace: &mut HiveWorkspace,
    cx: &mut Context<HiveWorkspace>,
) {
    info!("Costs: reset today");
    if cx.has_global::<AppAiService>() {
        cx.global_mut::<AppAiService>()
            .0
            .cost_tracker_mut()
            .reset_today();
    }
    data_refresh::refresh_cost_data(workspace, cx);
    cx.notify();
}

pub(super) fn handle_costs_clear_history(
    workspace: &mut HiveWorkspace,
    _action: &super::CostsClearHistory,
    window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    let confirmation =
        DestructiveConfirmation::for_action(DestructiveActionKind::CostsClearHistory);
    destructive_actions::request_confirmation(workspace, confirmation, window, cx);
}

pub(super) fn execute_confirmed_costs_clear_history(
    workspace: &mut HiveWorkspace,
    cx: &mut Context<HiveWorkspace>,
) {
    info!("Costs: clear all history");
    if cx.has_global::<AppAiService>() {
        cx.global_mut::<AppAiService>().0.cost_tracker_mut().clear();
    }
    data_refresh::refresh_cost_data(workspace, cx);
    cx.notify();
}
