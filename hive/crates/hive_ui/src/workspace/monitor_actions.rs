use gpui::*;
use tracing::info;

use super::{data_refresh, HiveWorkspace, MonitorRefresh};

pub(super) fn handle_monitor_refresh(
    workspace: &mut HiveWorkspace,
    _action: &MonitorRefresh,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    info!("Monitor: refresh");
    data_refresh::refresh_monitor_data(workspace, cx);
    cx.notify();
}
