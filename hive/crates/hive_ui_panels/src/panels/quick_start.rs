use gpui::*;
use gpui_component::input::{Input, InputState};
use gpui_component::{Icon, IconName};

use hive_ui_core::{
    HiveTheme, QuickStartOpenPanel, QuickStartRunProject, QuickStartSelectTemplate,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuickStartTone {
    Ready,
    Action,
    Optional,
}

#[derive(Debug, Clone)]
pub struct QuickStartSetupDisplay {
    pub title: String,
    pub detail: String,
    pub status_label: String,
    pub tone: QuickStartTone,
    pub action_label: Option<String>,
    pub action_panel: Option<String>,
}

#[derive(Debug, Clone)]
pub struct QuickStartTemplateDisplay {
    pub id: String,
    pub title: String,
    pub description: String,
    pub outcome: String,
}

#[derive(Debug, Clone)]
pub struct QuickStartNextStepDisplay {
    pub title: String,
    pub detail: String,
    pub panel: String,
    pub action_label: String,
}

#[derive(Debug, Clone)]
pub struct QuickStartPanelData {
    pub project_name: String,
    pub project_root: String,
    pub project_summary: String,
    pub total_files: usize,
    pub key_symbols: usize,
    pub dependencies: usize,
    pub selected_template: String,
    pub templates: Vec<QuickStartTemplateDisplay>,
    pub setup: Vec<QuickStartSetupDisplay>,
    pub next_steps: Vec<QuickStartNextStepDisplay>,
    pub launch_ready: bool,
    pub launch_hint: String,
    pub last_launch_status: Option<String>,
}

impl QuickStartPanelData {
    pub fn empty() -> Self {
        Self {
            project_name: String::new(),
            project_root: String::new(),
            project_summary: String::new(),
            total_files: 0,
            key_symbols: 0,
            dependencies: 0,
            selected_template: "dogfood".into(),
            templates: Vec::new(),
            setup: Vec::new(),
            next_steps: Vec::new(),
            launch_ready: false,
            launch_hint: "Connect a model and choose a project mission to launch Quick Start."
                .into(),
            last_launch_status: None,
        }
    }
}

pub struct QuickStartPanel;

impl QuickStartPanel {
    pub fn render(
        data: &QuickStartPanelData,
        detail_input: &Entity<InputState>,
        theme: &HiveTheme,
    ) -> impl IntoElement {
        div()
            .id("quick-start-panel")
            .flex()
            .flex_col()
            .size_full()
            .overflow_y_scroll()
            .p(theme.space_4)
            .pb(px(48.0))
            .gap(theme.space_4)
            .child(render_hero(data, theme))
            .child(render_setup_section(data, theme))
            .child(render_templates_section(data, theme))
            .child(render_launch_section(data, detail_input, theme))
            .child(render_next_steps_section(data, theme))
    }
}

fn render_hero(data: &QuickStartPanelData, theme: &HiveTheme) -> AnyElement {
    let summary = if data.project_summary.trim().is_empty() {
        "Hive will use the current workspace as the project context for a guided run."
            .to_string()
    } else {
        data.project_summary.clone()
    };

    card(theme)
        .bg(theme.bg_secondary)
        .border_color(theme.accent_cyan)
        .child(
            div()
                .flex()
                .flex_row()
                .items_start()
                .gap(theme.space_4)
                .child(
                    div()
                        .w(px(48.0))
                        .h(px(48.0))
                        .rounded(theme.radius_lg)
                        .bg(theme.accent_cyan)
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            Icon::new(IconName::Star)
                                .size_6()
                                .text_color(theme.text_on_accent),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(theme.space_2)
                        .flex_1()
                        .child(
                            div()
                                .text_size(theme.font_size_xl)
                                .text_color(theme.text_primary)
                                .font_weight(FontWeight::BOLD)
                                .child(format!("Quick Start {}", data.project_name)),
                        )
                        .child(
                            div()
                                .text_size(theme.font_size_sm)
                                .text_color(theme.text_secondary)
                                .child(
                                    "Choose the job to start, let Hive validate the setup, and kick off work in the current project."
                                ),
                        )
                        .child(
                            div()
                                .text_size(theme.font_size_xs)
                                .text_color(theme.text_muted)
                                .child(data.project_root.clone()),
                        )
                        .child(
                            div()
                                .text_size(theme.font_size_sm)
                                .text_color(theme.text_muted)
                                .child(summary),
                        ),
                ),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .gap(theme.space_2)
                .child(stat_chip(
                    format!("{} files", data.total_files),
                    theme.accent_aqua,
                    theme,
                ))
                .child(stat_chip(
                    format!("{} symbols", data.key_symbols),
                    theme.accent_green,
                    theme,
                ))
                .child(stat_chip(
                    format!("{} dependencies", data.dependencies),
                    theme.accent_yellow,
                    theme,
                )),
        )
        .into_any_element()
}

fn render_setup_section(data: &QuickStartPanelData, theme: &HiveTheme) -> AnyElement {
    let mut content = div()
        .flex()
        .flex_col()
        .gap(theme.space_3)
        .child(section_header(
            "Recommended Setup",
            "Quick Start checks the current project and points you at the next required panel when something is missing.",
            theme,
        ));

    for item in &data.setup {
        content = content.child(render_setup_card(item, theme));
    }

    content.into_any_element()
}

fn render_templates_section(data: &QuickStartPanelData, theme: &HiveTheme) -> AnyElement {
    let mut grid = div().flex().flex_row().flex_wrap().gap(theme.space_3);
    for template in &data.templates {
        grid = grid.child(render_template_card(
            template,
            template.id == data.selected_template,
            theme,
        ));
    }

    div()
        .flex()
        .flex_col()
        .gap(theme.space_3)
        .child(section_header(
            "Choose The Mission",
            "Pick the kind of run you want Hive to start on this project.",
            theme,
        ))
        .child(grid)
        .into_any_element()
}

fn render_launch_section(
    data: &QuickStartPanelData,
    detail_input: &Entity<InputState>,
    theme: &HiveTheme,
) -> AnyElement {
    let template_id = data.selected_template.clone();
    let run_input = detail_input.clone();
    let button_label = if data.launch_ready {
        "Start Guided Run"
    } else {
        "Finish Setup First"
    };
    let mut section = card(theme)
        .child(section_header(
            "Launch",
            "Describe the outcome you want. Quick Start will open Chat, seed the project brief, and start the run.",
            theme,
        ))
        .child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .child("What should Hive do next on this project?"),
        )
        .child(Input::new(detail_input).text_size(theme.font_size_sm).cleanable(true))
        .child(status_banner(
            &data.launch_hint,
            if data.launch_ready {
                theme.accent_green
            } else {
                theme.accent_yellow
            },
            theme,
        ));

    if let Some(status) = data.last_launch_status.as_ref() {
        section = section.child(status_banner(status, theme.accent_aqua, theme));
    }

    section
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .gap(theme.space_3)
                .child(
                    div()
                        .text_size(theme.font_size_xs)
                        .text_color(theme.text_muted)
                        .child(
                            "Quick Start launches a fresh chat run so the kickoff stays clean and project-scoped.",
                        ),
                )
                .child(primary_button(
                    button_label,
                    if data.launch_ready {
                        theme.accent_cyan
                    } else {
                        theme.bg_primary
                    },
                    if data.launch_ready {
                        theme.text_on_accent
                    } else {
                        theme.text_primary
                    },
                    move |_event, window, cx| {
                        window.dispatch_action(
                            Box::new(QuickStartRunProject {
                                template_id: template_id.clone(),
                                detail: run_input.read(cx).value().to_string(),
                            }),
                            cx,
                        );
                    },
                    theme,
                )),
        )
        .into_any_element()
}

fn render_next_steps_section(data: &QuickStartPanelData, theme: &HiveTheme) -> AnyElement {
    let mut grid = div().flex().flex_row().flex_wrap().gap(theme.space_3);
    for step in &data.next_steps {
        grid = grid.child(render_next_step_card(step, theme));
    }

    div()
        .flex()
        .flex_col()
        .gap(theme.space_3)
        .child(section_header(
            "Follow With The Other Tabs",
            "Quick Start gets the project moving. These tabs are the fastest handoff points after the kickoff run starts.",
            theme,
        ))
        .child(grid)
        .into_any_element()
}

fn render_setup_card(item: &QuickStartSetupDisplay, theme: &HiveTheme) -> AnyElement {
    let (tone_bg, tone_fg) = match item.tone {
        QuickStartTone::Ready => (theme.bg_primary, theme.accent_green),
        QuickStartTone::Action => (theme.bg_primary, theme.accent_yellow),
        QuickStartTone::Optional => (theme.bg_primary, theme.text_muted),
    };

    let mut card = card(theme)
        .child(
            div()
                .flex()
                .flex_row()
                .items_start()
                .justify_between()
                .gap(theme.space_3)
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(theme.space_1)
                        .flex_1()
                        .child(
                            div()
                                .text_size(theme.font_size_base)
                                .text_color(theme.text_primary)
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(item.title.clone()),
                        )
                        .child(
                            div()
                                .text_size(theme.font_size_sm)
                                .text_color(theme.text_secondary)
                                .child(item.detail.clone()),
                        ),
                )
                .child(
                    div()
                        .px(theme.space_2)
                        .py(px(3.0))
                        .rounded(theme.radius_full)
                        .bg(tone_bg)
                        .text_size(theme.font_size_xs)
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(tone_fg)
                        .child(item.status_label.clone()),
                ),
        );

    if let (Some(action_label), Some(action_panel)) =
        (item.action_label.as_ref(), item.action_panel.as_ref())
    {
        let panel = action_panel.clone();
        card = card.child(
            secondary_button(
                action_label,
                move |_event, window, cx| {
                    window.dispatch_action(
                        Box::new(QuickStartOpenPanel {
                            panel: panel.clone(),
                        }),
                        cx,
                    );
                },
                theme,
            ),
        );
    }

    card.into_any_element()
}

fn render_template_card(
    template: &QuickStartTemplateDisplay,
    selected: bool,
    theme: &HiveTheme,
) -> AnyElement {
    let template_id = template.id.clone();
    div()
        .id(ElementId::Name(
            format!("quick-start-template-{}", template.id).into(),
        ))
        .flex()
        .flex_col()
        .gap(theme.space_2)
        .min_w(px(220.0))
        .max_w(px(320.0))
        .flex_grow()
        .p(theme.space_4)
        .rounded(theme.radius_md)
        .bg(if selected {
            theme.bg_secondary
        } else {
            theme.bg_surface
        })
        .border_1()
        .border_color(if selected {
            theme.accent_cyan
        } else {
            theme.border
        })
        .cursor_pointer()
        .hover(|style| style.bg(theme.bg_secondary))
        .on_mouse_down(MouseButton::Left, move |_event, window, cx| {
            window.dispatch_action(
                Box::new(QuickStartSelectTemplate {
                    template_id: template_id.clone(),
                }),
                cx,
            );
        })
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .gap(theme.space_2)
                .child(
                    div()
                        .text_size(theme.font_size_base)
                        .text_color(theme.text_primary)
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(template.title.clone()),
                )
                .child(
                    div()
                        .px(theme.space_2)
                        .py(px(2.0))
                        .rounded(theme.radius_full)
                        .bg(theme.bg_primary)
                        .text_size(theme.font_size_xs)
                        .text_color(if selected {
                            theme.accent_cyan
                        } else {
                            theme.text_muted
                        })
                        .child(if selected { "Selected" } else { "Launchable" }),
                ),
        )
        .child(
            div()
                .text_size(theme.font_size_sm)
                .text_color(theme.text_secondary)
                .child(template.description.clone()),
        )
        .child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .child(template.outcome.clone()),
        )
        .into_any_element()
}

fn render_next_step_card(step: &QuickStartNextStepDisplay, theme: &HiveTheme) -> AnyElement {
    let panel = step.panel.clone();
    card(theme)
        .min_w(px(220.0))
        .max_w(px(320.0))
        .flex_grow()
        .child(
            div()
                .text_size(theme.font_size_base)
                .text_color(theme.text_primary)
                .font_weight(FontWeight::SEMIBOLD)
                .child(step.title.clone()),
        )
        .child(
            div()
                .text_size(theme.font_size_sm)
                .text_color(theme.text_secondary)
                .child(step.detail.clone()),
        )
        .child(
            secondary_button(
                &step.action_label,
                move |_event, window, cx| {
                    window.dispatch_action(
                        Box::new(QuickStartOpenPanel {
                            panel: panel.clone(),
                        }),
                        cx,
                    );
                },
                theme,
            ),
        )
        .into_any_element()
}

fn section_header(title: &str, description: &str, theme: &HiveTheme) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(px(2.0))
        .child(
            div()
                .text_size(theme.font_size_lg)
                .text_color(theme.text_primary)
                .font_weight(FontWeight::BOLD)
                .child(title.to_string()),
        )
        .child(
            div()
                .text_size(theme.font_size_sm)
                .text_color(theme.text_muted)
                .child(description.to_string()),
        )
        .into_any_element()
}

fn card(theme: &HiveTheme) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(theme.space_3)
        .p(theme.space_4)
        .rounded(theme.radius_lg)
        .bg(theme.bg_surface)
        .border_1()
        .border_color(theme.border)
}

fn stat_chip(label: String, color: Hsla, theme: &HiveTheme) -> AnyElement {
    div()
        .px(theme.space_2)
        .py(px(3.0))
        .rounded(theme.radius_full)
        .bg(theme.bg_primary)
        .text_size(theme.font_size_xs)
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(color)
        .child(label)
        .into_any_element()
}

fn status_banner(text: &str, accent: Hsla, theme: &HiveTheme) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(theme.space_2)
        .px(theme.space_3)
        .py(theme.space_2)
        .rounded(theme.radius_md)
        .bg(theme.bg_surface)
        .border_1()
        .border_color(accent)
        .child(
            div()
                .w(px(8.0))
                .h(px(8.0))
                .rounded(theme.radius_full)
                .bg(accent),
        )
        .child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_secondary)
                .child(text.to_string()),
        )
        .into_any_element()
}

fn primary_button<F>(
    label: &str,
    bg: Hsla,
    text_color: Hsla,
    on_click: F,
    theme: &HiveTheme,
) -> AnyElement
where
    F: Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
{
    div()
        .px(theme.space_3)
        .py(theme.space_2)
        .rounded(theme.radius_md)
        .bg(bg)
        .text_size(theme.font_size_sm)
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(text_color)
        .cursor_pointer()
        .hover(|style| style.opacity(0.92))
        .on_mouse_down(MouseButton::Left, on_click)
        .child(label.to_string())
        .into_any_element()
}

fn secondary_button<F>(label: &str, on_click: F, theme: &HiveTheme) -> AnyElement
where
    F: Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
{
    div()
        .flex()
        .items_center()
        .justify_center()
        .px(theme.space_3)
        .py(theme.space_2)
        .rounded(theme.radius_md)
        .bg(theme.bg_primary)
        .border_1()
        .border_color(theme.border)
        .text_size(theme.font_size_sm)
        .font_weight(FontWeight::MEDIUM)
        .text_color(theme.text_primary)
        .cursor_pointer()
        .hover(|style| style.bg(theme.bg_secondary))
        .on_mouse_down(MouseButton::Left, on_click)
        .child(label.to_string())
        .into_any_element()
}
