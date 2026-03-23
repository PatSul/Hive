use gpui::*;
use tracing::{info, warn};

use super::{
    data_refresh, ActivityApprove, ActivityDeny, ActivityExpandEvent, ActivityExportCsv,
    ActivityRefresh, ActivitySetFilter, ActivitySetView, AppActivityService, AppApprovalGate,
    HiveWorkspace, NotificationType, ObserveView,
};

pub(super) fn handle_activity_refresh(
    workspace: &mut HiveWorkspace,
    _action: &ActivityRefresh,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    info!("Activity refresh requested");
    data_refresh::refresh_monitor_data(workspace, cx);
    data_refresh::refresh_learning_data(workspace, cx);
    data_refresh::refresh_shield_data(workspace, cx);
    data_refresh::refresh_activity_data(workspace, cx);
    cx.notify();
}

pub(super) fn handle_activity_set_view(
    workspace: &mut HiveWorkspace,
    action: &ActivitySetView,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    workspace.activity_data.observe_view = ObserveView::from_action(&action.view);
    workspace.activity_data.filter.categories = None;
    match workspace.activity_data.observe_view {
        ObserveView::Inbox => {}
        ObserveView::Runtime => data_refresh::refresh_monitor_data(workspace, cx),
        ObserveView::Spend => data_refresh::refresh_learning_data(workspace, cx),
        ObserveView::Safety => data_refresh::refresh_shield_data(workspace, cx),
    }
    data_refresh::refresh_activity_data(workspace, cx);
    cx.notify();
}

pub(super) fn handle_activity_approve(
    workspace: &mut HiveWorkspace,
    action: &ActivityApprove,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    info!("Activity: approve request_id={}", action.request_id);
    if cx.has_global::<AppApprovalGate>() {
        cx.global::<AppApprovalGate>()
            .0
            .respond(&action.request_id, hive_agents::ApprovalDecision::Approved);
    }
    if cx.has_global::<AppActivityService>() {
        cx.global::<AppActivityService>()
            .0
            .emit(hive_agents::ActivityEvent::ApprovalGranted {
                request_id: action.request_id.clone(),
            });
    }
    data_refresh::refresh_activity_data(workspace, cx);
    workspace.push_notification(
        cx,
        NotificationType::Success,
        "Observe",
        format!("Approved request {}.", action.request_id),
    );
    cx.notify();
}

pub(super) fn handle_activity_deny(
    workspace: &mut HiveWorkspace,
    action: &ActivityDeny,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    info!(
        "Activity: deny request_id={} reason={}",
        action.request_id, action.reason
    );
    if cx.has_global::<AppApprovalGate>() {
        cx.global::<AppApprovalGate>().0.respond(
            &action.request_id,
            hive_agents::ApprovalDecision::Denied {
                reason: Some(action.reason.clone()),
            },
        );
    }
    if cx.has_global::<AppActivityService>() {
        cx.global::<AppActivityService>()
            .0
            .emit(hive_agents::ActivityEvent::ApprovalDenied {
                request_id: action.request_id.clone(),
                reason: Some(action.reason.clone()),
            });
    }
    data_refresh::refresh_activity_data(workspace, cx);
    workspace.push_notification(
        cx,
        NotificationType::Warning,
        "Observe",
        format!("Denied request {}.", action.request_id),
    );
    cx.notify();
}

pub(super) fn handle_activity_expand_event(
    workspace: &mut HiveWorkspace,
    action: &ActivityExpandEvent,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    if let Ok(id) = action.event_id.parse::<i64>() {
        if !workspace.activity_data.expanded_events.remove(&id) {
            workspace.activity_data.expanded_events.insert(id);
        }
        cx.notify();
    } else {
        warn!("Activity: invalid event_id '{}'", action.event_id);
    }
}

pub(super) fn handle_activity_export_csv(
    _workspace: &mut HiveWorkspace,
    _action: &ActivityExportCsv,
    _window: &mut Window,
    _cx: &mut Context<HiveWorkspace>,
) {
    info!("Activity: CSV export requested");
}

pub(super) fn handle_activity_set_filter(
    workspace: &mut HiveWorkspace,
    action: &ActivitySetFilter,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    let categories: Vec<String> = action
        .categories
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    info!("Activity: set filter categories={:?}", categories);
    workspace.activity_data.filter.categories = if categories.is_empty() {
        None
    } else {
        Some(categories)
    };
    data_refresh::refresh_activity_data(workspace, cx);
    cx.notify();
}
