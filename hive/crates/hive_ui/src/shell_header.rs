use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::{Icon, Sizable as _};

use hive_ui_core::{HiveTheme, Panel, QuickStartOpenPanel, ShellDestination};

pub struct ShellHeader;

#[derive(Debug, Clone)]
pub struct ShellHeaderData {
    pub destination: ShellDestination,
    pub active_panel: Panel,
    pub project_name: String,
    pub home_focus: Option<String>,
    pub current_model: String,
    pub pending_approvals: usize,
    pub is_streaming: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PipelineStage {
    Context,
    Plan,
    Execute,
    Validate,
    Apply,
}

impl PipelineStage {
    fn label(self) -> &'static str {
        match self {
            Self::Context => "Context",
            Self::Plan => "Plan",
            Self::Execute => "Execute",
            Self::Validate => "Validate",
            Self::Apply => "Apply",
        }
    }
}

impl ShellHeaderData {
    pub fn new(
        destination: ShellDestination,
        active_panel: Panel,
        project_name: impl Into<String>,
        home_focus: Option<String>,
        current_model: impl Into<String>,
        pending_approvals: usize,
        is_streaming: bool,
    ) -> Self {
        Self {
            destination,
            active_panel,
            project_name: project_name.into(),
            home_focus,
            current_model: current_model.into(),
            pending_approvals,
            is_streaming,
        }
    }

    fn mission_title(&self) -> &'static str {
        match self.active_panel {
            Panel::QuickStart => "Launch the next mission",
            Panel::Chat => "Build in the active workspace",
            Panel::Files => "Inspect and edit project files",
            Panel::History => "Resume earlier runs",
            Panel::Specs => "Plan the next implementation slice",
            Panel::CodeMap => "Trace the codebase shape",
            Panel::PromptLibrary => "Reuse proven prompts",
            Panel::Agents => "Coordinate specialist agents",
            Panel::Kanban => "Track execution tasks",
            Panel::Review => "Review and ship changes",
            Panel::Terminal => "Run commands in project context",
            Panel::Workflows => "Run repeatable workflows",
            Panel::Channels => "Manage cross-channel execution",
            Panel::Network => "Inspect peers and distributed paths",
            Panel::Assistant => "Handle assistant operations",
            Panel::Activity => "Review the Observe inbox",
            Panel::Monitor => "Inspect runtime health",
            Panel::Logs => "Audit runtime and agent logs",
            Panel::Costs => "Track spend and model usage",
            Panel::Learning => "Inspect what Hive is learning",
            Panel::Shield => "Review safety and privacy controls",
            Panel::Skills => "Manage installed skills",
            Panel::Routing => "Tune model routing",
            Panel::RoutingMatrix => "Set per-category routing policy",
            Panel::Models => "Choose and connect models",
            Panel::TokenLaunch => "Manage token launch tools",
            Panel::Settings => "Configure the workspace",
            Panel::Help => "Review guidance and shortcuts",
        }
    }

    fn mission_detail(&self) -> String {
        match self.destination {
            ShellDestination::Home => format!(
                "Use Home to clear blockers, switch workspaces, and launch {} for {}.",
                self.home_focus
                    .as_deref()
                    .unwrap_or("the next run")
                    .to_lowercase(),
                self.project_name,
            ),
            ShellDestination::Build => format!(
                "Keep planning, code, git, and terminal work in one lane for {}.",
                self.project_name,
            ),
            ShellDestination::Automate => {
                "Workflows, channels, and peers stay grouped here for repeatable execution."
                    .into()
            }
            ShellDestination::Observe => {
                "Approvals, spend, safety, and failures stay visible here so validation is never buried."
                    .into()
            }
            ShellDestination::Settings => {
                "Models, routing, skills, integrations, and advanced tools stay out of the daily work path."
                    .into()
            }
        }
    }

    fn pipeline_stage(&self) -> PipelineStage {
        match self.active_panel {
            Panel::QuickStart
            | Panel::Files
            | Panel::CodeMap
            | Panel::PromptLibrary
            | Panel::History => PipelineStage::Context,
            Panel::Specs | Panel::Kanban => PipelineStage::Plan,
            Panel::Chat
            | Panel::Agents
            | Panel::Workflows
            | Panel::Channels
            | Panel::Network
            | Panel::Assistant
            | Panel::Terminal => PipelineStage::Execute,
            Panel::Activity
            | Panel::Monitor
            | Panel::Logs
            | Panel::Costs
            | Panel::Learning
            | Panel::Shield => PipelineStage::Validate,
            Panel::Review
            | Panel::Skills
            | Panel::Routing
            | Panel::RoutingMatrix
            | Panel::Models
            | Panel::TokenLaunch
            | Panel::Settings
            | Panel::Help => PipelineStage::Apply,
        }
    }

    fn model_label(&self) -> String {
        let model = self.current_model.trim();
        if model.is_empty() || model == "Select Model" {
            "Model pending".into()
        } else {
            format!("Model · {model}")
        }
    }
}

impl ShellHeader {
    pub fn render(data: &ShellHeaderData, theme: &HiveTheme) -> AnyElement {
        let destination_panels = data
            .destination
            .panels()
            .iter()
            .copied()
            .filter(|panel| panel.is_visible())
            .collect::<Vec<_>>();

        div()
            .id("shell-header")
            .flex()
            .flex_col()
            .w_full()
            .gap(theme.space_3)
            .px(theme.space_4)
            .py(theme.space_3)
            .border_b_1()
            .border_color(theme.border)
            .bg(theme.bg_secondary)
            .child(render_mission_row(data, theme))
            .when(destination_panels.len() > 1, |el| {
                el.child(render_local_tabs(data, &destination_panels, theme))
            })
            .child(render_pipeline_strip(data, theme))
            .into_any_element()
    }
}

fn render_mission_row(data: &ShellHeaderData, theme: &HiveTheme) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .flex_wrap()
        .w_full()
        .items_start()
        .justify_between()
        .gap(theme.space_3)
        .child(
            div()
                .flex()
                .flex_col()
                .gap(theme.space_1)
                .min_w(px(260.0))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .flex_wrap()
                        .w_full()
                        .items_center()
                        .gap(theme.space_2)
                        .child(header_badge(
                            data.destination.label(),
                            theme.accent_aqua,
                            theme,
                        )),
                )
                .child(
                    div()
                        .text_size(theme.font_size_lg)
                        .text_color(theme.text_primary)
                        .font_weight(FontWeight::BOLD)
                        .child(data.mission_title()),
                )
                .child(
                    div()
                        .max_w(px(760.0))
                        .text_size(theme.font_size_sm)
                        .text_color(theme.text_secondary)
                        .child(data.mission_detail()),
                ),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .justify_end()
                .gap(theme.space_2)
                .child(header_badge(
                    &data.model_label(),
                    if data.current_model.trim().is_empty() || data.current_model == "Select Model"
                    {
                        theme.accent_yellow
                    } else {
                        theme.accent_green
                    },
                    theme,
                ))
                .child(header_badge(
                    if data.is_streaming {
                        "Run active"
                    } else {
                        "Idle"
                    },
                    if data.is_streaming {
                        theme.accent_green
                    } else {
                        theme.text_muted
                    },
                    theme,
                ))
                .child(header_badge(
                    &format!("{} approvals", data.pending_approvals),
                    if data.pending_approvals > 0 {
                        theme.accent_yellow
                    } else {
                        theme.accent_aqua
                    },
                    theme,
                ))
                .when(data.pending_approvals > 0, |el| {
                    el.child(action_chip("Open Observe", Panel::Activity, false, theme))
                }),
        )
        .into_any_element()
}

fn render_local_tabs(data: &ShellHeaderData, panels: &[Panel], theme: &HiveTheme) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .flex_wrap()
        .gap(theme.space_2)
        .children(
            panels
                .iter()
                .copied()
                .map(|panel| render_local_tab(panel, data.active_panel, theme)),
        )
        .into_any_element()
}

fn render_local_tab(panel: Panel, active_panel: Panel, theme: &HiveTheme) -> AnyElement {
    let is_active = panel == active_panel;
    let bg = if is_active {
        theme.bg_tertiary
    } else {
        theme.bg_primary
    };
    let border = if is_active {
        theme.accent_cyan
    } else {
        theme.border
    };
    let text = if is_active {
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
            format!("shell-tab-{}", panel.to_stored()).into(),
        ))
        .flex()
        .flex_row()
        .items_center()
        .gap(theme.space_2)
        .px(theme.space_3)
        .py(px(7.0))
        .rounded(theme.radius_full)
        .bg(bg)
        .border_1()
        .border_color(border)
        .cursor_pointer()
        .hover(|style| style.bg(theme.bg_tertiary))
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            cx.stop_propagation();
            window.dispatch_action(
                Box::new(QuickStartOpenPanel {
                    panel: panel.to_stored().into(),
                }),
                cx,
            );
        })
        .child(Icon::new(panel.icon()).small().text_color(icon_color))
        .child(
            div()
                .text_size(theme.font_size_sm)
                .text_color(text)
                .font_weight(if is_active {
                    FontWeight::SEMIBOLD
                } else {
                    FontWeight::NORMAL
                })
                .child(panel.label()),
        )
        .into_any_element()
}

fn render_pipeline_strip(data: &ShellHeaderData, theme: &HiveTheme) -> AnyElement {
    let active_stage = data.pipeline_stage();
    let label = if data.is_streaming {
        format!("Active run stage: {}", active_stage.label())
    } else {
        format!("Panel stage: {}", active_stage.label())
    };

    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(theme.space_2)
        .child(
            div()
                .px(theme.space_3)
                .py(px(5.0))
                .rounded(theme.radius_full)
                .bg(if data.is_streaming {
                    theme.accent_cyan
                } else {
                    theme.bg_tertiary
                })
                .border_1()
                .border_color(if data.is_streaming {
                    theme.accent_cyan
                } else {
                    theme.border
                })
                .text_size(theme.font_size_xs)
                .text_color(if data.is_streaming {
                    theme.text_on_accent
                } else {
                    theme.text_secondary
                })
                .font_weight(FontWeight::SEMIBOLD)
                .child(label),
        )
        .into_any_element()
}

fn header_badge(label: &str, color: Hsla, theme: &HiveTheme) -> AnyElement {
    div()
        .px(theme.space_2)
        .py(px(4.0))
        .rounded(theme.radius_full)
        .bg(theme.bg_primary)
        .border_1()
        .border_color(theme.border)
        .text_size(theme.font_size_xs)
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(color)
        .child(label.to_string())
        .into_any_element()
}

fn action_chip(label: &str, panel: Panel, active: bool, theme: &HiveTheme) -> AnyElement {
    let bg = if active {
        theme.bg_tertiary
    } else {
        theme.bg_primary
    };

    div()
        .px(theme.space_2)
        .py(px(4.0))
        .rounded(theme.radius_full)
        .bg(bg)
        .border_1()
        .border_color(theme.accent_aqua)
        .text_size(theme.font_size_xs)
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(theme.accent_aqua)
        .cursor_pointer()
        .hover(|style| style.bg(theme.bg_tertiary))
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            cx.stop_propagation();
            window.dispatch_action(
                Box::new(QuickStartOpenPanel {
                    panel: panel.to_stored().into(),
                }),
                cx,
            );
        })
        .child(label.to_string())
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::{PipelineStage, ShellHeaderData};
    use hive_ui_core::{Panel, ShellDestination};

    #[test]
    fn pipeline_stage_maps_core_workbench_panels() {
        let plan = ShellHeaderData::new(
            ShellDestination::Build,
            Panel::Specs,
            "Hive",
            None,
            "auto",
            0,
            false,
        );
        let execute = ShellHeaderData::new(
            ShellDestination::Build,
            Panel::Chat,
            "Hive",
            None,
            "auto",
            0,
            false,
        );
        let validate = ShellHeaderData::new(
            ShellDestination::Observe,
            Panel::Activity,
            "Hive",
            None,
            "auto",
            0,
            false,
        );

        assert_eq!(plan.pipeline_stage(), PipelineStage::Plan);
        assert_eq!(execute.pipeline_stage(), PipelineStage::Execute);
        assert_eq!(validate.pipeline_stage(), PipelineStage::Validate);
    }

    #[test]
    fn home_detail_mentions_selected_focus() {
        let data = ShellHeaderData::new(
            ShellDestination::Home,
            Panel::QuickStart,
            "DemoWorkspace",
            Some("Ship A Feature".into()),
            "",
            2,
            false,
        );

        let detail = data.mission_detail();
        assert!(detail.contains("ship a feature"));
        assert!(detail.contains("DemoWorkspace"));
    }
}
