use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::{Icon, IconName, Sizable as _};

use crate::command_palette::{CommandPalette, CommandPaletteAction, CommandPaletteItem};

use super::{
    navigation, project_context, quick_start_actions, HiveTheme, HiveWorkspace,
    OpenWorkspaceDirectory, Panel, QuickStartSelectTemplate, ShellDestination,
    SwitchToWorkspace, ToggleCommandPalette, TogglePinWorkspace,
};

pub(super) fn command_palette_items(workspace: &HiveWorkspace) -> Vec<CommandPaletteItem> {
    let mut items = Vec::new();
    let active_destination = workspace.sidebar.active_destination;

    for template in quick_start_actions::quick_start_templates() {
        items.push(CommandPaletteItem {
            title: template.title,
            detail: template.description,
            group: "Mission",
            action: CommandPaletteAction::SelectMission(template.id),
        });
    }

    for destination in ShellDestination::ALL {
        items.push(CommandPaletteItem {
            title: format!("Go to {}", destination.label()),
            detail: destination.description().into(),
            group: "Destination",
            action: CommandPaletteAction::OpenPanel(destination.default_panel()),
        });
    }

    for panel in Panel::ALL {
        let location = panel
            .shell_destination()
            .map(|destination| destination.label().to_string())
            .unwrap_or_else(|| "Utility".into());
        let group = if panel.shell_destination() == Some(active_destination) {
            "Current Space"
        } else {
            "Panel"
        };

        items.push(CommandPaletteItem {
            title: panel.label().into(),
            detail: format!("Open the {location} view"),
            group,
            action: CommandPaletteAction::OpenPanel(panel),
        });
    }

    let mut workspace_paths = Vec::new();
    for path in &workspace.pinned_workspace_roots {
        if !workspace_paths.contains(path) {
            workspace_paths.push(path.clone());
        }
    }
    for path in &workspace.recent_workspace_roots {
        if !workspace_paths.contains(path) {
            workspace_paths.push(path.clone());
        }
    }

    for path in workspace_paths {
        items.push(CommandPaletteItem {
            title: project_context::project_name_from_path(&path),
            detail: path.display().to_string(),
            group: "Workspace",
            action: CommandPaletteAction::SwitchWorkspace(path),
        });
    }

    items
}

pub(super) fn filtered_command_palette_items(
    workspace: &HiveWorkspace,
    cx: &App,
) -> Vec<CommandPaletteItem> {
    let query = workspace.command_palette_input.read(cx).value().to_string();
    let query = query.trim().to_string();

    command_palette_items(workspace)
        .into_iter()
        .filter(|item| item.matches_query(&query))
        .take(if query.is_empty() { 18 } else { 28 })
        .collect()
}

pub(super) fn render_command_palette(
    workspace: &HiveWorkspace,
    cx: &mut Context<HiveWorkspace>,
) -> AnyElement {
    let theme = &workspace.theme;
    let items = filtered_command_palette_items(workspace, cx);

    div()
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .bg(hsla(0.0, 0.0, 0.0, 0.38))
        .flex()
        .justify_center()
        .items_start()
        .pt(px(68.0))
        .px(theme.space_4)
        .on_mouse_down(MouseButton::Left, |_, window, cx| {
            cx.stop_propagation();
            window.dispatch_action(Box::new(ToggleCommandPalette), cx);
        })
        .child(
            div()
                .on_mouse_down(MouseButton::Left, |_, _window, cx| {
                    cx.stop_propagation();
                })
                .child(CommandPalette::render(
                    &items,
                    &workspace.command_palette_input,
                    theme,
                )),
        )
        .into_any_element()
}

pub(super) fn handle_toggle_command_palette(
    workspace: &mut HiveWorkspace,
    window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    if workspace.show_command_palette {
        close_command_palette(workspace, window, cx);
        return;
    }

    workspace.show_project_dropdown = false;
    workspace.show_utility_drawer = false;
    workspace.show_command_palette = true;
    workspace.command_palette_input.update(cx, |input, cx| {
        input.set_value(String::new(), window, cx);
    });
    cx.notify();

    let focus_handle = workspace.command_palette_input.read(cx).focus_handle(cx);
    window.focus(&focus_handle);
}

pub(super) fn handle_command_palette_submit(
    workspace: &mut HiveWorkspace,
    window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    if let Some(item) = filtered_command_palette_items(workspace, cx).into_iter().next() {
        execute_command_palette_action(workspace, &item.action, window, cx);
    }
}

pub(super) fn close_command_palette(
    workspace: &mut HiveWorkspace,
    window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    if !workspace.show_command_palette {
        return;
    }

    workspace.show_command_palette = false;
    workspace.command_palette_input.update(cx, |input, cx| {
        input.set_value(String::new(), window, cx);
    });
    cx.notify();
    window.focus(&workspace.focus_handle);
}

pub(super) fn execute_command_palette_action(
    workspace: &mut HiveWorkspace,
    action: &CommandPaletteAction,
    window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    match action {
        CommandPaletteAction::OpenPanel(panel) => navigation::switch_to_panel(workspace, *panel, cx),
        CommandPaletteAction::SwitchWorkspace(path) => {
            navigation::handle_switch_to_workspace_action(
                workspace,
                &SwitchToWorkspace {
                    path: path.to_string_lossy().to_string(),
                },
                window,
                cx,
            );
        }
        CommandPaletteAction::SelectMission(template_id) => {
            quick_start_actions::handle_quick_start_select_template(
                workspace,
                &QuickStartSelectTemplate {
                    template_id: template_id.clone(),
                },
                window,
                cx,
            );
            navigation::switch_to_panel(workspace, Panel::QuickStart, cx);
        }
    }

    close_command_palette(workspace, window, cx);
}

pub(super) fn render_project_dropdown(
    workspace: &HiveWorkspace,
    cx: &mut Context<HiveWorkspace>,
) -> AnyElement {
    let theme = &workspace.theme;
    let current_root = &workspace.current_project_root;
    let pinned = &workspace.pinned_workspace_roots;

    let mut children: Vec<AnyElement> = Vec::new();

    for path in pinned {
        let is_active = path == current_root;
        let path_str = path.to_string_lossy().to_string();
        let name = project_context::project_name_from_path(path);
        children.push(
            render_project_row(theme, &name, &path_str, is_active, true, cx).into_any_element(),
        );
    }

    if !pinned.is_empty() {
        children.push(
            div()
                .h(px(1.0))
                .mx(theme.space_2)
                .my(theme.space_1)
                .bg(theme.border)
                .into_any_element(),
        );
    }

    for path in &workspace.recent_workspace_roots {
        if pinned.contains(path) {
            continue;
        }
        let is_active = path == current_root;
        let path_str = path.to_string_lossy().to_string();
        let name = project_context::project_name_from_path(path);
        children.push(
            render_project_row(theme, &name, &path_str, is_active, false, cx).into_any_element(),
        );
    }

    children.push(
        div()
            .h(px(1.0))
            .mx(theme.space_2)
            .my(theme.space_1)
            .bg(theme.border)
            .into_any_element(),
    );

    children.push(
        div()
            .id("open-folder-row")
            .flex()
            .flex_row()
            .items_center()
            .gap(theme.space_2)
            .px(theme.space_3)
            .py(theme.space_2)
            .rounded(theme.radius_md)
            .cursor_pointer()
            .hover(|s| s.bg(theme.bg_tertiary))
            .on_mouse_down(MouseButton::Left, |_, window, cx| {
                cx.stop_propagation();
                window.dispatch_action(Box::new(OpenWorkspaceDirectory), cx);
            })
            .child(Icon::new(IconName::FolderOpen).small())
            .child(
                div()
                    .text_size(theme.font_size_xs)
                    .text_color(theme.text_secondary)
                    .child("Open folder..."),
            )
            .into_any_element(),
    );

    div()
        .id("project-dropdown")
        .occlude()
        .absolute()
        .top(px(42.0))
        .left(px(120.0))
        .w(px(320.0))
        .max_h(px(400.0))
        .overflow_y_scroll()
        .bg(theme.bg_primary)
        .border_1()
        .border_color(theme.border)
        .rounded(theme.radius_lg)
        .shadow_lg()
        .py(theme.space_1)
        .children(children)
        .into_any_element()
}

fn render_project_row(
    theme: &HiveTheme,
    name: &str,
    path_str: &str,
    is_active: bool,
    is_pinned: bool,
    _cx: &mut Context<HiveWorkspace>,
) -> impl IntoElement {
    let switch_path = path_str.to_string();
    let pin_path = path_str.to_string();

    let text_color = if is_active {
        theme.accent_cyan
    } else {
        theme.text_primary
    };

    let pin_icon_color = if is_pinned {
        theme.accent_cyan
    } else {
        theme.text_muted
    };

    div()
        .id(SharedString::from(format!("project-row-{}", path_str)))
        .flex()
        .flex_row()
        .items_center()
        .gap(theme.space_2)
        .px(theme.space_3)
        .py(theme.space_2)
        .rounded(theme.radius_md)
        .cursor_pointer()
        .hover(|s| s.bg(theme.bg_tertiary))
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            cx.stop_propagation();
            window.dispatch_action(
                Box::new(SwitchToWorkspace {
                    path: switch_path.clone(),
                }),
                cx,
            );
        })
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .overflow_hidden()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(theme.space_1)
                        .when(is_active, |el| {
                            el.child(
                                div()
                                    .w(px(6.0))
                                    .h(px(6.0))
                                    .rounded(theme.radius_full)
                                    .bg(theme.accent_green),
                            )
                        })
                        .child(
                            div()
                                .text_size(theme.font_size_sm)
                                .text_color(text_color)
                                .font_weight(if is_active {
                                    FontWeight::BOLD
                                } else {
                                    FontWeight::NORMAL
                                })
                                .truncate()
                                .child(name.to_string()),
                        ),
                )
                .child(
                    div()
                        .text_size(px(10.0))
                        .text_color(theme.text_muted)
                        .truncate()
                        .child(path_str.to_string()),
                ),
        )
        .child(
            div()
                .id(SharedString::from(format!("pin-btn-{}", pin_path)))
                .flex_shrink_0()
                .cursor_pointer()
                .rounded(theme.radius_sm)
                .p(px(4.0))
                .hover(|s| s.bg(theme.bg_secondary))
                .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                    cx.stop_propagation();
                    window.dispatch_action(
                        Box::new(TogglePinWorkspace {
                            path: pin_path.clone(),
                        }),
                        cx,
                    );
                })
                .child(
                    Icon::new(IconName::Star)
                        .with_size(px(14.0))
                        .text_color(pin_icon_color),
                ),
        )
}
