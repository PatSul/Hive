use gpui::*;

use super::{HiveWorkspace, ToolApprove, ToolReject};

pub(super) fn handle_tool_approve(
    workspace: &mut HiveWorkspace,
    _action: &ToolApprove,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    workspace.chat_service.update(cx, |svc, cx| {
        svc.resolve_approval(true, cx);
    });
    cx.notify();
}

pub(super) fn handle_tool_reject(
    workspace: &mut HiveWorkspace,
    _action: &ToolReject,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    workspace.chat_service.update(cx, |svc, cx| {
        svc.resolve_approval(false, cx);
    });
    cx.notify();
}
