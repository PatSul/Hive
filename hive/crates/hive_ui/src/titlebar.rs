use gpui::*;
use gpui_component::{Icon, IconName, Sizable as _};
use std::path::Path;

use hive_ui_core::{
    HiveTheme, OpenWorkspaceDirectory, SwitchToFiles,
};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

const TITLE_BAR_HEIGHT: Pixels = px(42.0);
const MAC_TITLE_BAR_HEIGHT: Pixels = px(34.0);
const MAC_TRAFFIC_LIGHT_SPACING: Pixels = px(76.0);

/// Custom titlebar with app branding, workflow context, and platform controls.
pub struct Titlebar;

impl Titlebar {
    /// Render the full titlebar: left-side branding + optional project context +
    /// window control buttons.
    ///
    /// Requires `window` to check maximized state for the correct
    /// restore/maximize icon.
    pub fn render(
        theme: &HiveTheme,
        window: &Window,
        current_workspace_root: &Path,
    ) -> impl IntoElement {
        let is_maximized = window.is_maximized();
        let is_macos = cfg!(target_os = "macos");
        let title_bar_height = if is_macos {
            MAC_TITLE_BAR_HEIGHT
        } else {
            TITLE_BAR_HEIGHT
        };

        let mut bar = div()
            .id("hive-title-bar")
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .h(title_bar_height)
            .px(theme.space_2)
            .border_b_1()
            .border_color(theme.border)
            .bg(theme.bg_secondary);

        // Keep occlusion on Windows/Linux custom controls; on mac, this can
        // block the native traffic-light buttons from receiving pointer events.
        if !is_macos {
            bar = bar.occlude();
        }

        if is_macos {
            bar = bar
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(theme.space_2)
                        .flex_shrink_0()
                        .h_full()
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .pl(MAC_TRAFFIC_LIGHT_SPACING)
                        .child(branding(theme))
                        .child(workspace_chip(theme, current_workspace_root)),
                )
                .child(
                    div()
                        .id("titlebar-drag")
                        .window_control_area(WindowControlArea::Drag)
                        .flex()
                        .flex_1()
                        .h_full(),
                );
        } else {
            bar = bar
                .child(
                    div()
                        .id("titlebar-drag")
                        .window_control_area(WindowControlArea::Drag)
                        .flex()
                        .flex_1()
                        .h_full()
                        .items_center()
                        .gap(theme.space_3)
                        .px(theme.space_2)
                        .child(branding(theme))
                        .child(workspace_chip(
                            theme,
                            current_workspace_root,
                        )),
                )
                .child(window_controls(theme, is_maximized));
        }

        bar
    }
}

fn workspace_chip(theme: &HiveTheme, current_workspace_root: &Path) -> impl IntoElement {
    let current_label = current_workspace_root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("Project")
        .to_string();

    let mut chip = div()
        .flex()
        .items_center()
        .gap(theme.space_1)
        .rounded(theme.radius_full)
        .bg(theme.bg_primary)
        .border_1()
        .border_color(theme.border)
        .text_size(theme.font_size_xs)
        .text_color(theme.text_secondary)
        .whitespace_nowrap()
        .overflow_hidden()
        .line_clamp(1);

    chip = chip.child(
        div()
            .px(theme.space_2)
            .py(px(3.0))
            .flex()
            .items_center()
            .gap(theme.space_1)
            .cursor_pointer()
            .on_mouse_down(MouseButton::Left, |_, window, cx| {
                window.dispatch_action(Box::new(SwitchToFiles), cx);
            })
            .child(
                div()
                    .w(px(6.0))
                    .h(px(6.0))
                    .rounded(theme.radius_full)
                    .bg(theme.accent_green),
            )
            .child(
                div()
                    .text_size(theme.font_size_xs)
                    .truncate()
                    .child(format!("Project Space: {current_label}")),
            ),
    )
    .child(
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(theme.space_1)
            .px(theme.space_1)
            .py(px(3.0))
            .rounded(theme.radius_full)
            .bg(theme.bg_tertiary)
            .hover(|s| s.bg(theme.bg_secondary))
            .cursor_pointer()
            .on_mouse_down(MouseButton::Left, |_, window, cx| {
                cx.stop_propagation();
                window.dispatch_action(Box::new(OpenWorkspaceDirectory), cx);
            })
            .child(Icon::new(IconName::FolderOpen).small())
            .child(
                div()
                    .text_size(theme.font_size_xs)
                    .text_color(theme.text_secondary)
                    .child("Open"),
            ),
    );

    chip
}

/// Bee icon + "Hive" + version badge.
fn branding(theme: &HiveTheme) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap(theme.space_2)
        .child(
            svg()
                .path("icons/hive-bee.svg")
                .size(px(20.0))
                .text_color(theme.accent_aqua),
        )
        .child(
            div()
                .text_size(theme.font_size_base)
                .text_color(theme.text_primary)
                .font_weight(FontWeight::BOLD)
                .child("Hive"),
        )
        .child(version_badge(theme))
}

/// Compact version badge.
fn version_badge(theme: &HiveTheme) -> impl IntoElement {
    div()
        .px(theme.space_2)
        .py(px(2.0))
        .rounded(theme.radius_full)
        .bg(theme.bg_primary)
        .border_1()
        .border_color(theme.border)
        .text_size(theme.font_size_xs)
        .text_color(theme.accent_cyan)
        .child(format!("v{VERSION}"))
}

/// Minimize / Maximize-or-Restore / Close buttons.
///
/// All buttons use `window_control_area` for native NC behavior (correct
/// maximize/restore toggle via the Win32 NC handler).
fn window_controls(theme: &HiveTheme, is_maximized: bool) -> impl IntoElement {
    let fg = theme.text_primary;
    let hover_bg = hsla(0.0, 0.0, 1.0, 0.08);
    let active_bg = hsla(0.0, 0.0, 1.0, 0.02);
    let close_hover_bg = theme.accent_red;

    div()
        .flex()
        .items_center()
        .h_full()
        .flex_shrink_0()
        // Minimize
        .child(
            div()
                .id("minimize")
                .flex()
                .w(px(38.0))
                .h_full()
                .flex_shrink_0()
                .justify_center()
                .content_center()
                .items_center()
                .text_color(fg)
                .hover(|s| s.bg(hover_bg))
                .active(|s| s.bg(active_bg))
                .window_control_area(WindowControlArea::Min)
                .on_click(|_, window, cx| {
                    cx.stop_propagation();
                    window.minimize_window();
                })
                .child(Icon::new(IconName::WindowMinimize).small()),
        )
        // Maximize / Restore — no on_click; NC handler performs restore
        // correctly so this avoids conflicts with state transitions.
        .child(
            div()
                .id("maximize")
                .flex()
                .w(px(38.0))
                .h_full()
                .flex_shrink_0()
                .justify_center()
                .content_center()
                .items_center()
                .text_color(fg)
                .hover(|s| s.bg(hover_bg))
                .active(|s| s.bg(active_bg))
                .window_control_area(WindowControlArea::Max)
                .child(
                    Icon::new(if is_maximized {
                        IconName::WindowRestore
                    } else {
                        IconName::WindowMaximize
                    })
                    .small(),
                ),
        )
        // Close
        .child(
            div()
                .id("close")
                .flex()
                .w(px(38.0))
                .h_full()
                .flex_shrink_0()
                .justify_center()
                .content_center()
                .items_center()
                .text_color(fg)
                .hover(|s| {
                    s.bg(close_hover_bg)
                        .text_color(hsla(0.0, 0.0, 1.0, 1.0))
                })
                .active(|s| s.bg(close_hover_bg))
                .window_control_area(WindowControlArea::Close)
                .child(Icon::new(IconName::WindowClose).small()),
        )
}
