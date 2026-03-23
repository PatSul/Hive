use std::path::PathBuf;

use gpui::*;
use gpui_component::input::{Input, InputState};
use gpui_component::scroll::ScrollableElement;
use gpui_component::{Icon, IconName};

use hive_ui_core::{
    HiveTheme, Panel, QuickStartOpenPanel, QuickStartSelectTemplate, SwitchToWorkspace,
    ToggleCommandPalette,
};

pub struct CommandPalette;

#[derive(Debug, Clone)]
pub enum CommandPaletteAction {
    OpenPanel(Panel),
    SwitchWorkspace(PathBuf),
    SelectMission(String),
}

#[derive(Debug, Clone)]
pub struct CommandPaletteItem {
    pub title: String,
    pub detail: String,
    pub group: &'static str,
    pub action: CommandPaletteAction,
}

impl CommandPaletteItem {
    pub fn matches_query(&self, query: &str) -> bool {
        if query.trim().is_empty() {
            return true;
        }

        let query = query.trim().to_ascii_lowercase();
        self.title.to_ascii_lowercase().contains(&query)
            || self.detail.to_ascii_lowercase().contains(&query)
            || self.group.to_ascii_lowercase().contains(&query)
    }
}

impl CommandPalette {
    pub fn render(
        items: &[CommandPaletteItem],
        input: &Entity<InputState>,
        theme: &HiveTheme,
    ) -> AnyElement {
        div()
            .id("command-palette")
            .w(px(720.0))
            .max_w(px(920.0))
            .max_h(px(560.0))
            .flex()
            .flex_col()
            .overflow_hidden()
            .rounded(theme.radius_lg)
            .bg(theme.bg_secondary)
            .border_1()
            .border_color(theme.border)
            .shadow_lg()
            .child(render_palette_header(input, theme))
            .child(render_palette_items(items, theme))
            .into_any_element()
    }
}

fn render_palette_header(input: &Entity<InputState>, theme: &HiveTheme) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(theme.space_2)
        .px(theme.space_3)
        .py(theme.space_3)
        .border_b_1()
        .border_color(theme.border)
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .gap(theme.space_2)
                .child(
                    div()
                        .text_size(theme.font_size_sm)
                        .text_color(theme.text_primary)
                        .font_weight(FontWeight::BOLD)
                        .child("Jump Anywhere"),
                )
                .child(shortcut_badge(theme)),
        )
        .child(
            Input::new(input)
                .appearance(true)
                .cleanable(false),
        )
        .child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .child("Search panels, recent workspaces, and Home missions."),
        )
        .into_any_element()
}

fn render_palette_items(items: &[CommandPaletteItem], theme: &HiveTheme) -> AnyElement {
    if items.is_empty() {
        return div()
            .flex()
            .items_center()
            .justify_center()
            .px(theme.space_4)
            .py(px(48.0))
            .text_size(theme.font_size_sm)
            .text_color(theme.text_muted)
            .child("No matches. Try a panel name, workspace, or mission.")
            .into_any_element();
    }

    div()
        .flex()
        .flex_col()
        .overflow_y_scrollbar()
        .px(theme.space_2)
        .py(theme.space_2)
        .gap(theme.space_1)
        .children(items.iter().map(|item| render_palette_item(item, theme)))
        .into_any_element()
}

fn render_palette_item(item: &CommandPaletteItem, theme: &HiveTheme) -> AnyElement {
    let icon = match &item.action {
        CommandPaletteAction::OpenPanel(panel) => panel.icon(),
        CommandPaletteAction::SwitchWorkspace(_) => IconName::FolderOpen,
        CommandPaletteAction::SelectMission(_) => IconName::Star,
    };
    let action = item.action.clone();

    div()
        .id(ElementId::Name(
            format!("command-palette-item-{}-{}", item.group, item.title).into(),
        ))
        .flex()
        .flex_row()
        .items_start()
        .gap(theme.space_3)
        .px(theme.space_3)
        .py(theme.space_2)
        .rounded(theme.radius_md)
        .cursor_pointer()
        .hover(|style| style.bg(theme.bg_primary))
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            cx.stop_propagation();
            match &action {
                CommandPaletteAction::OpenPanel(panel) => {
                    window.dispatch_action(
                        Box::new(QuickStartOpenPanel {
                            panel: panel.to_stored().into(),
                        }),
                        cx,
                    );
                }
                CommandPaletteAction::SwitchWorkspace(path) => {
                    window.dispatch_action(
                        Box::new(SwitchToWorkspace {
                            path: path.to_string_lossy().to_string(),
                        }),
                        cx,
                    );
                }
                CommandPaletteAction::SelectMission(template_id) => {
                    window.dispatch_action(
                        Box::new(QuickStartSelectTemplate {
                            template_id: template_id.clone(),
                        }),
                        cx,
                    );
                    window.dispatch_action(
                        Box::new(QuickStartOpenPanel {
                            panel: Panel::QuickStart.to_stored().into(),
                        }),
                        cx,
                    );
                }
            }
            window.dispatch_action(Box::new(ToggleCommandPalette), cx);
        })
        .child(
            div()
                .pt(px(2.0))
                .child(Icon::new(icon).size_4().text_color(theme.accent_aqua)),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .flex_1()
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .flex_wrap()
                        .items_center()
                        .gap(theme.space_2)
                        .child(
                            div()
                                .text_size(theme.font_size_sm)
                                .text_color(theme.text_primary)
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(item.title.clone()),
                        )
                        .child(group_badge(item.group, theme)),
                )
                .child(
                    div()
                        .text_size(theme.font_size_xs)
                        .text_color(theme.text_secondary)
                        .child(item.detail.clone()),
                ),
        )
        .into_any_element()
}

fn group_badge(group: &str, theme: &HiveTheme) -> AnyElement {
    div()
        .px(theme.space_2)
        .py(px(3.0))
        .rounded(theme.radius_full)
        .bg(theme.bg_primary)
        .border_1()
        .border_color(theme.border)
        .text_size(theme.font_size_xs)
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(theme.text_muted)
        .child(group.to_string())
        .into_any_element()
}

fn shortcut_badge(theme: &HiveTheme) -> AnyElement {
    let label = if cfg!(target_os = "macos") {
        "Cmd+K"
    } else {
        "Ctrl+K"
    };

    div()
        .px(theme.space_2)
        .py(px(3.0))
        .rounded(theme.radius_full)
        .bg(theme.bg_primary)
        .border_1()
        .border_color(theme.border)
        .text_size(theme.font_size_xs)
        .text_color(theme.text_secondary)
        .child(label)
        .into_any_element()
}
