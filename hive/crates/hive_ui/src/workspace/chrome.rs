use gpui::prelude::FluentBuilder;
use gpui::*;

use crate::chat_input::ChatInputView;
use crate::shell_header::{ShellHeader, ShellHeaderData};

use super::{
    context_rail, quick_start_actions, sidebar_shell, AppApprovalGate, HiveWorkspace, Panel,
    ShellDestination, ToggleProjectDropdown,
};

pub(super) fn build_shell_header_data(
    workspace: &HiveWorkspace,
    cx: &mut Context<HiveWorkspace>,
) -> ShellHeaderData {
    let pending_approvals = if cx.has_global::<AppApprovalGate>() {
        cx.global::<AppApprovalGate>().0.pending_count()
    } else {
        0
    };
    let home_focus = Some(
        quick_start_actions::quick_start_template_title(&workspace.quick_start_data.selected_template)
            .to_string(),
    );
    let is_streaming = workspace.chat_service.read(cx).is_streaming();

    ShellHeaderData::new(
        workspace.sidebar.active_destination,
        workspace.sidebar.active_panel,
        workspace.current_project_name.clone(),
        home_focus,
        workspace.status_bar.current_model.clone(),
        pending_approvals,
        is_streaming,
    )
}

pub(super) fn apply_default_focus(
    workspace: &mut HiveWorkspace,
    window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    if window.focused(cx).is_some() {
        return;
    }

    if workspace.show_command_palette {
        let fh = workspace.command_palette_input.read(cx).focus_handle(cx);
        window.focus(&fh);
    } else if workspace.sidebar.active_panel == Panel::Chat {
        let fh = workspace.chat_input.read(cx).input_focus_handle();
        window.focus(&fh);
    } else if workspace.sidebar.active_panel == Panel::Settings {
        let fh = workspace.settings_view.read(cx).focus_handle().clone();
        window.focus(&fh);
    } else {
        window.focus(&workspace.focus_handle);
    }
}

pub(super) fn render_project_dropdown_backdrop() -> AnyElement {
    div()
        .id("project-dropdown-backdrop")
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .on_mouse_down(MouseButton::Left, |_, window, cx| {
            cx.stop_propagation();
            window.dispatch_action(Box::new(ToggleProjectDropdown), cx);
        })
        .into_any_element()
}

pub(super) fn render_utility_drawer_backdrop(
    cx: &mut Context<HiveWorkspace>,
) -> AnyElement {
    div()
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _, _, cx| {
                this.show_utility_drawer = false;
                cx.notify();
            }),
        )
        .into_any_element()
}

pub(super) fn render_main_content(
    workspace: &HiveWorkspace,
    active_panel_el: AnyElement,
    shell_header_data: ShellHeaderData,
    chat_input: Entity<ChatInputView>,
    cx: &mut Context<HiveWorkspace>,
) -> AnyElement {
    let theme = &workspace.theme;
    let active_panel = workspace.sidebar.active_panel;
    let show_context_rail = matches!(
        workspace.sidebar.active_destination,
        ShellDestination::Build | ShellDestination::Observe
    );

    div()
        .flex()
        .flex_1()
        .overflow_hidden()
        .child(sidebar_shell::render_sidebar(workspace, cx))
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .overflow_hidden()
                .child(ShellHeader::render(&shell_header_data, theme))
                .child(
                    div()
                        .flex()
                        .flex_1()
                        .overflow_hidden()
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .flex_1()
                                .overflow_hidden()
                                .child(active_panel_el),
                        )
                        .when(show_context_rail, |el| {
                            el.child(context_rail::render_context_rail(workspace, cx))
                        }),
                )
                .when(active_panel == Panel::Chat, |el: Div| el.child(chat_input)),
        )
        .into_any_element()
}
