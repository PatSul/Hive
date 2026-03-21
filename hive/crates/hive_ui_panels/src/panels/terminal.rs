use chrono::{DateTime, Utc};
use gpui::*;
use gpui_component::scroll::ScrollableElement;
use gpui_component::{Icon, IconName};

use hive_ui_core::HiveTheme;
use hive_ui_core::{TerminalClear, TerminalKill, TerminalRestart};

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// The kind of a single terminal output line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalLineKind {
    Stdout,
    Stderr,
    Stdin,
    System,
}

/// A single line of terminal output with metadata.
#[derive(Clone, Debug)]
pub struct TerminalLine {
    pub kind: TerminalLineKind,
    pub content: String,
    pub timestamp: DateTime<Utc>,
}

/// Data backing the Terminal panel.
pub struct TerminalData {
    pub lines: Vec<TerminalLine>,
    pub cwd: String,
    pub is_running: bool,
}

impl Default for TerminalData {
    fn default() -> Self {
        Self::empty()
    }
}

impl TerminalData {
    pub fn empty() -> Self {
        let cwd = std::env::current_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| ".".to_string());
        Self {
            lines: Vec::new(),
            cwd,
            is_running: false,
        }
    }

    /// Push a new line of output.
    pub fn push_line(&mut self, kind: TerminalLineKind, content: String) {
        self.lines.push(TerminalLine {
            kind,
            content,
            timestamp: Utc::now(),
        });
    }

    /// Push a system message (grey text).
    pub fn push_system(&mut self, msg: impl Into<String>) {
        self.push_line(TerminalLineKind::System, msg.into());
    }
}

// ---------------------------------------------------------------------------
// Stateless panel renderer
// ---------------------------------------------------------------------------

pub struct TerminalPanel;

impl TerminalPanel {
    /// Render header + scrollable output. The input bar is rendered by the
    /// workspace so it can embed the real `InputState` entity.
    pub fn render(data: &TerminalData, theme: &HiveTheme) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(theme.bg_primary)
            .child(Self::render_header(data, theme))
            .child(Self::render_output(data, theme))
    }

    fn render_header(data: &TerminalData, theme: &HiveTheme) -> impl IntoElement {
        div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .px(theme.space_4)
            .py(theme.space_2)
            .border_b_1()
            .border_color(theme.border)
            .bg(theme.bg_secondary)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(theme.space_2)
                    .child(
                        Icon::new(IconName::Dash)
                            .size_4()
                            .text_color(theme.accent_aqua),
                    )
                    .child(
                        div()
                            .text_size(theme.font_size_sm)
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.text_primary)
                            .child("Terminal"),
                    )
                    .child(
                        div()
                            .text_size(theme.font_size_xs)
                            .text_color(theme.text_muted)
                            .max_w(px(400.0))
                            .overflow_hidden()
                            .child(data.cwd.clone()),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(theme.space_1)
                    // Status indicator
                    .child(
                        div()
                            .text_size(theme.font_size_xs)
                            .text_color(if data.is_running {
                                theme.accent_green
                            } else {
                                theme.text_muted
                            })
                            .child(if data.is_running { "Running" } else { "Idle" }),
                    )
                    // Kill button
                    .child(
                        div()
                            .id("terminal-kill")
                            .cursor_pointer()
                            .px(theme.space_2)
                            .py(theme.space_1)
                            .rounded(theme.radius_sm)
                            .text_size(theme.font_size_xs)
                            .text_color(theme.accent_red)
                            .hover(|s| s.bg(theme.bg_tertiary))
                            .on_mouse_down(MouseButton::Left, |_, window, cx| {
                                window.dispatch_action(Box::new(TerminalKill), cx);
                            })
                            .child("Kill"),
                    )
                    // Restart button
                    .child(
                        div()
                            .id("terminal-restart")
                            .cursor_pointer()
                            .px(theme.space_2)
                            .py(theme.space_1)
                            .rounded(theme.radius_sm)
                            .text_size(theme.font_size_xs)
                            .text_color(theme.accent_cyan)
                            .hover(|s| s.bg(theme.bg_tertiary))
                            .on_mouse_down(MouseButton::Left, |_, window, cx| {
                                window.dispatch_action(Box::new(TerminalRestart), cx);
                            })
                            .child("Restart"),
                    )
                    // Clear button
                    .child(
                        div()
                            .id("terminal-clear")
                            .cursor_pointer()
                            .px(theme.space_2)
                            .py(theme.space_1)
                            .rounded(theme.radius_sm)
                            .hover(|s| s.bg(theme.bg_tertiary))
                            .on_mouse_down(MouseButton::Left, |_, window, cx| {
                                window.dispatch_action(Box::new(TerminalClear), cx);
                            })
                            .child(
                                Icon::new(IconName::Close)
                                    .size_3p5()
                                    .text_color(theme.text_muted),
                            ),
                    ),
            )
    }

    fn render_output(data: &TerminalData, theme: &HiveTheme) -> impl IntoElement {
        let lines: Vec<_> = data
            .lines
            .iter()
            .enumerate()
            .map(|(i, line)| {
                let color = match line.kind {
                    TerminalLineKind::Stdout => theme.text_primary,
                    TerminalLineKind::Stderr => theme.accent_red,
                    TerminalLineKind::Stdin => theme.accent_cyan,
                    TerminalLineKind::System => theme.text_muted,
                };
                let prefix = match line.kind {
                    TerminalLineKind::Stdin => "$ ",
                    TerminalLineKind::System => "# ",
                    _ => "",
                };

                div()
                    .id(ElementId::Name(format!("term-line-{i}").into()))
                    .flex()
                    .flex_row()
                    .px(theme.space_4)
                    .py(px(1.0))
                    .font_family(theme.font_mono.clone())
                    .text_size(theme.font_size_sm)
                    .text_color(color)
                    .child(format!("{}{}", prefix, line.content))
            })
            .collect();

        div()
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scrollbar()
            .bg(theme.bg_primary)
            .py(theme.space_2)
            .children(lines)
    }
}
