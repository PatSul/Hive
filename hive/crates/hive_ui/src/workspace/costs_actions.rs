use std::path::PathBuf;

use gpui::*;
use tracing::{error, info};

use super::{AppAiService, HiveConfig, HiveWorkspace, NotificationType};

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
    _workspace: &mut HiveWorkspace,
    _action: &super::CostsResetToday,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    info!("Costs: reset today");
    if cx.has_global::<AppAiService>() {
        cx.global_mut::<AppAiService>()
            .0
            .cost_tracker_mut()
            .reset_today();
    }
    cx.notify();
}

pub(super) fn handle_costs_clear_history(
    _workspace: &mut HiveWorkspace,
    _action: &super::CostsClearHistory,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    info!("Costs: clear all history");
    if cx.has_global::<AppAiService>() {
        cx.global_mut::<AppAiService>().0.cost_tracker_mut().clear();
    }
    cx.notify();
}
