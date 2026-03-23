use gpui::*;
use gpui_component::{Icon, IconName};

use crate::SwitchToQuickStart;
use crate::theme::HiveTheme;

/// Welcome screen shown before the first message.
pub struct WelcomeScreen;

impl WelcomeScreen {
    pub fn render(theme: &HiveTheme) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .flex_1()
            .gap(theme.space_6)
            .pt(px(48.0))
            .child(
                // Logo icon — Hive bee
                svg()
                    .path("icons/hive-bee.svg")
                    .size(px(48.0))
                    .text_color(theme.accent_aqua),
            )
            .child(
                div()
                    .text_size(theme.font_size_2xl)
                    .text_color(theme.text_primary)
                    .child("Welcome to Hive"),
            )
            .child(
                div()
                    .text_size(theme.font_size_lg)
                    .text_color(theme.text_secondary)
                    .child("Your AI-powered development companion"),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(theme.space_2)
                    .mt(theme.space_4)
                    .items_center()
                    .child(
                        div()
                            .text_size(theme.font_size_base)
                            .text_color(theme.text_muted)
                            .child("Start by typing a message below, or open Home:"),
                    )
                    .child(quick_start_button(theme))
                    .child(hint_row(
                        theme,
                        IconName::Settings,
                        "Configure API keys in Settings",
                    ))
                    .child(hint_row(
                        theme,
                        IconName::Folder,
                        "Open a project folder in Files",
                    ))
                    .child(hint_row(
                        theme,
                        IconName::Map,
                        "Explore AI models in Routing",
                    )),
            )
    }
}

fn quick_start_button(theme: &HiveTheme) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap(theme.space_2)
        .px(theme.space_3)
        .py(theme.space_2)
        .rounded(theme.radius_md)
        .bg(theme.accent_cyan)
        .text_color(theme.text_on_accent)
        .text_size(theme.font_size_sm)
        .font_weight(FontWeight::SEMIBOLD)
        .hover(|style| style.bg(theme.accent_aqua))
        .cursor_pointer()
        .on_mouse_down(MouseButton::Left, |_event, window, cx| {
            window.dispatch_action(Box::new(SwitchToQuickStart), cx);
        })
        .child(
            Icon::new(IconName::Star)
                .size_4()
                .text_color(theme.text_on_accent),
        )
        .child("Open Home for this project")
}

fn hint_row(theme: &HiveTheme, icon: IconName, text: &str) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap(theme.space_2)
        .text_size(theme.font_size_sm)
        .text_color(theme.text_secondary)
        .child(Icon::new(icon).size_4().text_color(theme.accent_cyan))
        .child(div().child(text.to_string()))
}
