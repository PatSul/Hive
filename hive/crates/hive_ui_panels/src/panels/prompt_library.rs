use gpui::*;

use hive_agents::prompt_template::{self, PromptTemplate};
use hive_ui_core::actions::{PromptLibraryDelete, PromptLibraryLoad};
use hive_ui_core::HiveTheme;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Data backing the Prompt Library panel.
#[derive(Debug, Clone, Default)]
pub struct PromptLibraryData {
    pub templates: Vec<PromptTemplate>,
    pub filter_query: String,
}

impl PromptLibraryData {
    /// Load all templates from disk.
    pub fn load() -> Self {
        let templates = prompt_template::list_templates().unwrap_or_default();
        Self {
            templates,
            filter_query: String::new(),
        }
    }

    /// Reload from disk.
    pub fn refresh(&mut self) {
        self.templates = prompt_template::list_templates().unwrap_or_default();
    }

    /// Filter templates by query.
    pub fn filtered(&self) -> Vec<&PromptTemplate> {
        if self.filter_query.is_empty() {
            return self.templates.iter().collect();
        }
        let q = self.filter_query.to_lowercase();
        self.templates
            .iter()
            .filter(|t| {
                t.name.to_lowercase().contains(&q)
                    || t.description.to_lowercase().contains(&q)
                    || t.tags.iter().any(|tag| tag.to_lowercase().contains(&q))
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

/// Render the Prompt Library panel.
pub fn render_prompt_library(data: &PromptLibraryData, theme: &HiveTheme) -> impl IntoElement {
    let filtered = data.filtered();
    let total = data.templates.len();

    div()
        .id("prompt-library-panel")
        .flex()
        .flex_col()
        .size_full()
        .overflow_y_scroll()
        .bg(theme.bg_primary)
        .child(
            // Header
            div()
                .flex()
                .items_center()
                .justify_between()
                .px(theme.space_4)
                .py(theme.space_3)
                .border_b_1()
                .border_color(theme.border)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(theme.space_2)
                        .child(
                            div()
                                .text_size(theme.font_size_lg)
                                .text_color(theme.text_primary)
                                .font_weight(FontWeight::BOLD)
                                .child("Prompt Library"),
                        )
                        .child(
                            div()
                                .text_size(theme.font_size_xs)
                                .text_color(theme.text_muted)
                                .child(format!("{} templates", total)),
                        ),
                ),
        )
        .children(if filtered.is_empty() {
            vec![render_empty_state(theme).into_any_element()]
        } else {
            filtered
                .into_iter()
                .map(|t| render_template_card(t, theme).into_any_element())
                .collect()
        })
}

/// Render empty state.
fn render_empty_state(theme: &HiveTheme) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .py(theme.space_8)
        .gap(theme.space_2)
        .child(
            div()
                .text_size(theme.font_size_lg)
                .text_color(theme.text_muted)
                .child("No saved prompts"),
        )
        .child(
            div()
                .text_size(theme.font_size_sm)
                .text_color(theme.text_muted)
                .child("Use \"Save Current\" to save your chat input as a reusable template"),
        )
}

/// Render a single template card.
fn render_template_card(template: &PromptTemplate, theme: &HiveTheme) -> impl IntoElement {
    let id = template.id.clone();
    let load_id = template.id.clone();
    let delete_id = template.id.clone();

    div()
        .flex()
        .flex_col()
        .w_full()
        .px(theme.space_4)
        .py(theme.space_3)
        .border_b_1()
        .border_color(theme.border)
        .hover(|s| s.bg(theme.bg_secondary))
        .child(
            // Title row
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_size(theme.font_size_sm)
                        .text_color(theme.text_primary)
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(template.name.clone()),
                )
                .child(
                    // Action buttons
                    div()
                        .flex()
                        .gap(theme.space_2)
                        .child(
                            div()
                                .id(SharedString::from(format!("load-{id}")))
                                .text_size(theme.font_size_xs)
                                .text_color(theme.accent_aqua)
                                .cursor_pointer()
                                .on_mouse_down(MouseButton::Left, move |_ev, _w, cx| {
                                    cx.dispatch_action(&PromptLibraryLoad {
                                        prompt_id: load_id.clone(),
                                    });
                                })
                                .child("Use"),
                        )
                        .child(
                            div()
                                .id(SharedString::from(format!("del-{}", template.id)))
                                .text_size(theme.font_size_xs)
                                .text_color(theme.accent_red)
                                .cursor_pointer()
                                .on_mouse_down(MouseButton::Left, move |_ev, _w, cx| {
                                    cx.dispatch_action(&PromptLibraryDelete {
                                        prompt_id: delete_id.clone(),
                                    });
                                })
                                .child("Delete"),
                        ),
                ),
        )
        .child(
            // Description
            div()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .mt(theme.space_1)
                .child(if template.description.is_empty() {
                    template.instruction.chars().take(100).collect::<String>()
                } else {
                    template.description.clone()
                }),
        )
        .when(!template.context_files.is_empty(), |el| {
            el.child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(theme.space_1)
                    .mt(theme.space_1)
                    .children(
                        template.context_files.iter().map(|f| {
                            div()
                                .text_size(theme.font_size_xs)
                                .text_color(theme.accent_cyan)
                                .px(theme.space_1)
                                .rounded(theme.radius_sm)
                                .bg(theme.bg_tertiary)
                                .child(f.clone())
                                .into_any_element()
                        }),
                    ),
            )
        })
        .when(!template.tags.is_empty(), |el| {
            el.child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(theme.space_1)
                    .mt(theme.space_1)
                    .children(
                        template.tags.iter().map(|tag| {
                            div()
                                .text_size(theme.font_size_xs)
                                .text_color(theme.accent_yellow)
                                .px(theme.space_1)
                                .rounded(theme.radius_sm)
                                .bg(theme.bg_tertiary)
                                .child(format!("#{tag}"))
                                .into_any_element()
                        }),
                    ),
            )
        })
}
