use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::scroll::ScrollableElement;
use gpui_component::{Icon, IconName};

use super::{
    navigation, project_context, HiveTheme, HiveWorkspace, OpenWorkspaceDirectory, Panel,
    ShellDestination,
};

pub(super) fn render_sidebar(
    workspace: &HiveWorkspace,
    cx: &mut Context<HiveWorkspace>,
) -> impl IntoElement {
    let theme = &workspace.theme;
    let active_panel = workspace.sidebar.active_panel;
    let active_destination = workspace.sidebar.active_destination;
    let project = project_context::project_label(workspace);
    let utility_panels = [
        Panel::Skills,
        Panel::Routing,
        Panel::Models,
        Panel::TokenLaunch,
        Panel::Settings,
        Panel::Help,
    ];

    div()
        .relative()
        .flex()
        .flex_col()
        .w(px(232.0))
        .h_full()
        .bg(theme.bg_secondary)
        .border_r_1()
        .border_color(theme.border)
        .child(
            div()
                .px(theme.space_3)
                .py(theme.space_2)
                .border_b_1()
                .border_color(theme.border)
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(theme.space_2)
                        .cursor_pointer()
                        .on_mouse_down(MouseButton::Left, |_, window, cx| {
                            cx.stop_propagation();
                            window.dispatch_action(Box::new(OpenWorkspaceDirectory), cx);
                        })
                        .child(
                            div()
                                .w(px(26.0))
                                .h(px(26.0))
                                .rounded(theme.radius_full)
                                .bg(theme.bg_tertiary)
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(
                                    Icon::new(IconName::Folder)
                                        .size_3p5()
                                        .text_color(theme.accent_aqua),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .child(
                                    div()
                                        .text_size(theme.font_size_xs)
                                        .text_color(theme.text_muted)
                                        .child("Workspace"),
                                )
                                .child(
                                    div()
                                        .text_size(theme.font_size_sm)
                                        .text_color(theme.text_secondary)
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .max_w(px(150.0))
                                        .overflow_hidden()
                                        .child(project),
                                ),
                        ),
                ),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_h(px(0.0))
                .overflow_y_scrollbar()
                .px(theme.space_2)
                .py(theme.space_2)
                .gap(theme.space_3)
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(theme.space_1)
                        .child(
                            div()
                                .px(theme.space_2)
                                .pb(px(2.0))
                                .text_size(theme.font_size_xs)
                                .text_color(theme.text_muted)
                                .font_weight(FontWeight::SEMIBOLD)
                                .child("Destinations"),
                        )
                        .children(ShellDestination::ALL.iter().copied().map(|destination| {
                            render_shell_destination_item(
                                destination,
                                active_destination,
                                theme,
                                cx,
                            )
                        })),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(theme.space_1)
                        .p(theme.space_2)
                        .rounded(theme.radius_lg)
                        .bg(theme.bg_tertiary)
                        .border_1()
                        .border_color(theme.border)
                        .child(
                            div()
                                .text_size(theme.font_size_xs)
                                .text_color(theme.text_muted)
                                .font_weight(FontWeight::SEMIBOLD)
                                .child("Current Space"),
                        )
                        .child(
                            div()
                                .text_size(theme.font_size_sm)
                                .text_color(theme.text_primary)
                                .font_weight(FontWeight::BOLD)
                                .child(active_destination.label()),
                        )
                        .child(
                            div()
                                .text_size(theme.font_size_xs)
                                .text_color(theme.text_secondary)
                                .child(active_destination.description()),
                        ),
                )
                .child(render_sidebar_section(
                    "Inside This Space",
                    active_destination.panels(),
                    active_panel,
                    theme,
                    cx,
                )),
        )
        .child(
            div()
                .px(theme.space_2)
                .py(theme.space_2)
                .border_t_1()
                .border_color(theme.border)
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .justify_between()
                        .gap(theme.space_2)
                        .px(theme.space_2)
                        .py(theme.space_2)
                        .rounded(theme.radius_lg)
                        .bg(if workspace.show_utility_drawer {
                            theme.bg_tertiary
                        } else {
                            theme.bg_primary
                        })
                        .border_1()
                        .border_color(if workspace.show_utility_drawer {
                            theme.accent_aqua
                        } else {
                            theme.border
                        })
                        .cursor_pointer()
                        .hover(|style| style.bg(theme.bg_tertiary))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _, _, cx| {
                                this.show_utility_drawer = !this.show_utility_drawer;
                                this.show_project_dropdown = false;
                                this.show_command_palette = false;
                                cx.notify();
                            }),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap(theme.space_2)
                                .child(
                                    Icon::new(IconName::Settings)
                                        .size_4()
                                        .text_color(theme.accent_aqua),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap(px(2.0))
                                        .child(
                                            div()
                                                .text_size(theme.font_size_sm)
                                                .text_color(theme.text_primary)
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .child("Utilities"),
                                        )
                                        .child(
                                            div()
                                                .text_size(theme.font_size_xs)
                                                .text_color(theme.text_muted)
                                                .child("Models, routing, skills, settings, and help."),
                                        ),
                                ),
                        )
                        .child(
                            Icon::new(if workspace.show_utility_drawer {
                                IconName::ChevronDown
                            } else {
                                IconName::ChevronRight
                            })
                            .size_3p5()
                            .text_color(theme.text_muted),
                        ),
                ),
        )
        .when(workspace.show_utility_drawer, |el| {
            el.child(render_utility_drawer(
                &utility_panels,
                active_panel,
                theme,
                cx,
            ))
        })
}

fn render_utility_drawer(
    utility_panels: &[Panel],
    active_panel: Panel,
    theme: &HiveTheme,
    cx: &mut Context<HiveWorkspace>,
) -> impl IntoElement {
    div()
        .absolute()
        .left(theme.space_2)
        .right(theme.space_2)
        .bottom(px(76.0))
        .rounded(theme.radius_lg)
        .bg(theme.bg_secondary)
        .border_1()
        .border_color(theme.border)
        .shadow_lg()
        .on_mouse_down(MouseButton::Left, |_, _window, cx| {
            cx.stop_propagation();
        })
        .child(
            div()
                .px(theme.space_3)
                .py(theme.space_3)
                .border_b_1()
                .border_color(theme.border)
                .child(
                    div()
                        .text_size(theme.font_size_sm)
                        .text_color(theme.text_primary)
                        .font_weight(FontWeight::BOLD)
                        .child("Configure & Extend"),
                )
                .child(
                    div()
                        .text_size(theme.font_size_xs)
                        .text_color(theme.text_muted)
                        .child("Low-frequency tools stay here so the main rail can stay focused."),
                ),
        )
        .child(
            div()
                .px(theme.space_2)
                .py(theme.space_2)
                .flex()
                .flex_col()
                .gap(theme.space_1)
                .children(
                    utility_panels
                        .iter()
                        .copied()
                        .map(|panel| render_sidebar_item(panel, active_panel, theme, cx)),
                ),
        )
}

fn render_shell_destination_item(
    destination: ShellDestination,
    active_destination: ShellDestination,
    theme: &HiveTheme,
    cx: &mut Context<HiveWorkspace>,
) -> AnyElement {
    let is_active = destination == active_destination;
    let bg = if is_active {
        theme.bg_tertiary
    } else {
        Hsla::transparent_black()
    };
    let border_color = if is_active {
        theme.accent_cyan
    } else {
        Hsla::transparent_black()
    };
    let title_color = if is_active {
        theme.text_primary
    } else {
        theme.text_secondary
    };
    let icon_color = if is_active {
        theme.accent_aqua
    } else {
        theme.text_muted
    };

    div()
        .id(ElementId::Name(
            format!("destination-{}", destination.label()).into(),
        ))
        .flex()
        .flex_row()
        .items_start()
        .gap(theme.space_2)
        .w_full()
        .px(theme.space_2)
        .py(theme.space_2)
        .rounded(theme.radius_lg)
        .bg(bg)
        .border_l_2()
        .border_color(border_color)
        .cursor_pointer()
        .hover(|style| style.bg(theme.bg_primary))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _event, _window, cx| {
                navigation::switch_to_panel(this, destination.default_panel(), cx);
            }),
        )
        .child(
            div().pt(px(2.0)).child(
                Icon::new(destination.icon())
                    .size_4()
                    .text_color(icon_color),
            ),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .flex_1()
                .child(
                    div()
                        .text_size(theme.font_size_sm)
                        .text_color(title_color)
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(destination.label()),
                )
                .child(
                    div()
                        .text_size(theme.font_size_xs)
                        .text_color(theme.text_muted)
                        .child(destination.description()),
                ),
        )
        .into_any_element()
}

fn render_sidebar_section(
    title: &'static str,
    panels: &[Panel],
    active: Panel,
    theme: &HiveTheme,
    cx: &mut Context<HiveWorkspace>,
) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(theme.space_1)
        .child(
            div()
                .px(theme.space_2)
                .pb(px(2.0))
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .font_weight(FontWeight::SEMIBOLD)
                .child(title),
        )
        .children(
            panels
                .iter()
                .copied()
                .map(|panel| render_sidebar_item(panel, active, theme, cx)),
        )
        .into_any_element()
}

fn render_sidebar_item(
    panel: Panel,
    active: Panel,
    theme: &HiveTheme,
    cx: &mut Context<HiveWorkspace>,
) -> AnyElement {
    let is_active = panel == active;
    let bg = if is_active {
        theme.bg_tertiary
    } else {
        Hsla::transparent_black()
    };
    let text_color = if is_active {
        theme.accent_aqua
    } else {
        theme.text_secondary
    };
    let border_color = if is_active {
        theme.accent_cyan
    } else {
        Hsla::transparent_black()
    };

    div()
        .id(ElementId::Name(panel.label().into()))
        .flex()
        .flex_row()
        .items_center()
        .gap(theme.space_2)
        .w_full()
        .h(px(32.0))
        .px(theme.space_2)
        .rounded(theme.radius_md)
        .bg(bg)
        .border_l_2()
        .border_color(border_color)
        .cursor_pointer()
        .hover(|style| style.bg(theme.bg_primary).text_color(theme.text_primary))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _event, _window, cx| {
                tracing::info!("Sidebar click: {:?}", panel);
                navigation::switch_to_panel(this, panel, cx);
            }),
        )
        .child(
            div()
                .w(px(16.0))
                .h(px(16.0))
                .flex()
                .items_center()
                .justify_center()
                .child(Icon::new(panel.icon()).size_3p5().text_color(text_color)),
        )
        .child(
            div()
                .text_size(theme.font_size_sm)
                .text_color(text_color)
                .font_weight(if is_active {
                    FontWeight::SEMIBOLD
                } else {
                    FontWeight::NORMAL
                })
                .child(panel.label()),
        )
        .into_any_element()
}
