use std::collections::HashSet;

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::scroll::ScrollableElement;
use gpui_component::{Icon, IconName};

use hive_agents::activity::approval::ApprovalRequest;
use hive_agents::activity::log::{ActivityEntry, ActivityFilter, CostSummary};
use hive_ui_core::{
    ActivityApprove, ActivityDeny, ActivityExpandEvent, ActivitySetFilter, ActivitySetView,
    HiveTheme,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObserveView {
    Inbox,
    Runtime,
    Spend,
    Safety,
}

impl ObserveView {
    pub const ALL: [ObserveView; 4] = [
        ObserveView::Inbox,
        ObserveView::Runtime,
        ObserveView::Spend,
        ObserveView::Safety,
    ];

    pub fn from_action(value: &str) -> Self {
        match value {
            "runtime" => Self::Runtime,
            "spend" => Self::Spend,
            "safety" => Self::Safety,
            _ => Self::Inbox,
        }
    }

    fn action_value(self) -> &'static str {
        match self {
            Self::Inbox => "inbox",
            Self::Runtime => "runtime",
            Self::Spend => "spend",
            Self::Safety => "safety",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Inbox => "Inbox",
            Self::Runtime => "Runtime",
            Self::Spend => "Spend",
            Self::Safety => "Safety",
        }
    }

    fn description(self) -> &'static str {
        match self {
            Self::Inbox => "Blocked actions and failures that need judgment now.",
            Self::Runtime => "Live run state, agents, providers, and queue pressure.",
            Self::Spend => "Cost, quality, and model outcomes tied to recent work.",
            Self::Safety => "Shield state, caught issues, and recent safety events.",
        }
    }

    fn icon(self) -> IconName {
        match self {
            Self::Inbox => IconName::Inbox,
            Self::Runtime => IconName::Loader,
            Self::Spend => IconName::ChartPie,
            Self::Safety => IconName::EyeOff,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ObserveRuntimeData {
    pub status_label: String,
    pub active_agents: usize,
    pub active_streams: usize,
    pub online_providers: usize,
    pub total_providers: usize,
    pub request_queue_length: usize,
    pub current_run_id: Option<String>,
    pub agents: Vec<ObserveAgentRow>,
    pub recent_runs: Vec<ObserveRunRow>,
}

#[derive(Debug, Clone)]
pub struct ObserveAgentRow {
    pub role: String,
    pub status: String,
    pub phase: String,
    pub model: String,
    pub started_at: String,
}

#[derive(Debug, Clone)]
pub struct ObserveRunRow {
    pub id: String,
    pub summary: String,
    pub status: String,
    pub started_at: String,
    pub cost_usd: f64,
}

#[derive(Debug, Clone)]
pub struct ObserveSpendData {
    pub quality_score: f64,
    pub quality_trend: String,
    pub cost_efficiency: f64,
    pub best_model: Option<String>,
    pub worst_model: Option<String>,
    pub weak_areas: Vec<String>,
}

impl Default for ObserveSpendData {
    fn default() -> Self {
        Self {
            quality_score: 0.0,
            quality_trend: "Stable".into(),
            cost_efficiency: 0.0,
            best_model: None,
            worst_model: None,
            weak_areas: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ObserveSafetyData {
    pub shield_enabled: bool,
    pub pii_detections: usize,
    pub secrets_blocked: usize,
    pub threats_caught: usize,
    pub recent_events: Vec<ObserveSafetyEvent>,
}

#[derive(Debug, Clone)]
pub struct ObserveSafetyEvent {
    pub timestamp: String,
    pub event_type: String,
    pub severity: String,
    pub detail: String,
}

pub struct ActivityData {
    pub entries: Vec<ActivityEntry>,
    pub filter: ActivityFilter,
    pub pending_approvals: Vec<ApprovalRequest>,
    pub cost_summary: CostSummary,
    pub expanded_events: HashSet<i64>,
    pub search_query: String,
    pub observe_view: ObserveView,
    pub runtime: ObserveRuntimeData,
    pub spend: ObserveSpendData,
    pub safety: ObserveSafetyData,
}

impl Default for ActivityData {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            filter: ActivityFilter::default(),
            pending_approvals: Vec::new(),
            cost_summary: CostSummary::default(),
            expanded_events: HashSet::new(),
            search_query: String::new(),
            observe_view: ObserveView::Inbox,
            runtime: ObserveRuntimeData::default(),
            spend: ObserveSpendData::default(),
            safety: ObserveSafetyData::default(),
        }
    }
}

pub struct ActivityPanel;

impl ActivityPanel {
    pub fn render(data: &ActivityData, theme: &HiveTheme) -> impl IntoElement {
        div()
            .id("activity-panel")
            .flex()
            .flex_col()
            .size_full()
            .overflow_hidden()
            .child(Self::header(data, theme))
            .child(Self::summary_row(data, theme))
            .child(Self::view_tabs(data, theme))
            .child(Self::body(data, theme))
    }

    fn header(data: &ActivityData, theme: &HiveTheme) -> impl IntoElement {
        div()
            .id("activity-header")
            .flex()
            .flex_row()
            .flex_wrap()
            .items_start()
            .justify_between()
            .gap(theme.space_3)
            .px(theme.space_4)
            .py(theme.space_3)
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(theme.space_2)
                            .child(
                                Icon::new(IconName::Inbox)
                                    .size_4()
                                    .text_color(theme.accent_cyan),
                            )
                            .child(
                                div()
                                    .text_size(theme.font_size_lg)
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(theme.text_primary)
                                    .child("Observe"),
                            ),
                    )
                    .child(
                        div()
                            .text_size(theme.font_size_sm)
                            .text_color(theme.text_secondary)
                            .child(data.observe_view.description()),
                    ),
            )
            .child(
                div()
                    .text_size(theme.font_size_xs)
                    .text_color(theme.text_muted)
                    .child(format!(
                        "{} events loaded - {} approvals waiting",
                        data.entries.len(),
                        data.pending_approvals.len()
                    )),
            )
    }

    fn summary_row(data: &ActivityData, theme: &HiveTheme) -> impl IntoElement {
        div()
            .flex()
            .flex_row()
            .flex_wrap()
            .gap(theme.space_2)
            .px(theme.space_4)
            .py(theme.space_2)
            .border_b_1()
            .border_color(theme.border)
            .child(summary_chip(
                format!("{} approvals", data.pending_approvals.len()),
                if data.pending_approvals.is_empty() {
                    theme.accent_green
                } else {
                    theme.accent_yellow
                },
                theme,
            ))
            .child(summary_chip(
                format!("{} failures", attention_entries(data).len()),
                if attention_entries(data).is_empty() {
                    theme.accent_green
                } else {
                    theme.accent_red
                },
                theme,
            ))
            .child(summary_chip(
                format!("{} active agents", data.runtime.active_agents),
                if data.runtime.active_agents > 0 {
                    theme.accent_aqua
                } else {
                    theme.text_muted
                },
                theme,
            ))
            .child(summary_chip(
                format!("${:.4} last 24h", data.cost_summary.total_usd),
                theme.accent_cyan,
                theme,
            ))
            .child(summary_chip(
                if data.safety.shield_enabled {
                    "Shield on".into()
                } else {
                    "Shield off".into()
                },
                if data.safety.shield_enabled {
                    theme.accent_green
                } else {
                    theme.accent_red
                },
                theme,
            ))
    }

    fn view_tabs(data: &ActivityData, theme: &HiveTheme) -> impl IntoElement {
        div()
            .flex()
            .flex_row()
            .flex_wrap()
            .gap(theme.space_2)
            .px(theme.space_4)
            .py(theme.space_2)
            .border_b_1()
            .border_color(theme.border)
            .children(ObserveView::ALL.iter().copied().map(|view| {
                let is_active = data.observe_view == view;
                let accent = if is_active {
                    theme.accent_aqua
                } else {
                    theme.text_muted
                };
                let bg = if is_active {
                    theme.bg_secondary
                } else {
                    theme.bg_surface
                };
                let border = if is_active {
                    theme.accent_cyan
                } else {
                    theme.border
                };

                div()
                    .id(gpui::ElementId::Name(
                        format!("observe-view-{}", view.action_value()).into(),
                    ))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(theme.space_2)
                    .px(theme.space_3)
                    .py(theme.space_2)
                    .rounded(theme.radius_lg)
                    .bg(bg)
                    .border_1()
                    .border_color(border)
                    .cursor_pointer()
                    .hover(|style| style.bg(theme.bg_primary))
                    .on_mouse_down(MouseButton::Left, move |_event, window, cx| {
                        window.dispatch_action(
                            Box::new(ActivitySetView {
                                view: view.action_value().into(),
                            }),
                            cx,
                        );
                    })
                    .child(Icon::new(view.icon()).size_3p5().text_color(accent))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(1.0))
                            .child(
                                div()
                                    .text_size(theme.font_size_sm)
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(if is_active {
                                        theme.text_primary
                                    } else {
                                        theme.text_secondary
                                    })
                                    .child(view.label()),
                            )
                            .child(
                                div()
                                    .text_size(theme.font_size_xs)
                                    .text_color(theme.text_muted)
                                    .child(view.description()),
                            ),
                    )
                    .into_any_element()
            }))
    }

    fn body(data: &ActivityData, theme: &HiveTheme) -> AnyElement {
        match data.observe_view {
            ObserveView::Inbox => Self::render_inbox(data, theme),
            ObserveView::Runtime => Self::render_runtime(data, theme),
            ObserveView::Spend => Self::render_spend(data, theme),
            ObserveView::Safety => Self::render_safety(data, theme),
        }
    }

    fn render_inbox(data: &ActivityData, theme: &HiveTheme) -> AnyElement {
        let attention = attention_entries(data);
        let timeline_entries: Vec<&ActivityEntry> = data.entries.iter().collect();

        div()
            .flex()
            .flex_col()
            .flex_1()
            .overflow_y_scrollbar()
            .p(theme.space_4)
            .gap(theme.space_4)
            .when(data.pending_approvals.is_empty() && attention.is_empty(), |el| {
                el.child(empty_state_card(
                    "Observe is clear",
                    "No approvals or failure signals need attention right now.",
                    theme,
                ))
            })
            .when(!data.pending_approvals.is_empty(), |el| {
                el.child(
                    render_section_shell(
                        "Blocked Actions",
                        "These actions are waiting on explicit approval before work can continue.",
                        theme,
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(theme.space_3)
                            .children(
                                data.pending_approvals
                                    .iter()
                                    .map(|request| Self::approval_card(request, theme)),
                            ),
                    ),
                )
            })
            .when(!attention.is_empty(), |el| {
                el.child(
                    render_section_shell(
                        "Failures And Warnings",
                        "These signals indicate a blocked run, failure, timeout, or cost problem.",
                        theme,
                    )
                    .child(Self::event_list_from_refs(&attention, data, theme)),
                )
            })
            .child(
                render_section_shell(
                    "Recent Timeline",
                    "The raw feed stays here as supporting evidence instead of being the main screen.",
                    theme,
                )
                .child(Self::filter_bar(data, theme))
                .child(Self::event_list_from_refs(&timeline_entries, data, theme)),
            )
            .into_any_element()
    }

    fn render_runtime(data: &ActivityData, theme: &HiveTheme) -> AnyElement {
        let runtime_entries = runtime_entries(data);

        div()
            .flex()
            .flex_col()
            .flex_1()
            .overflow_y_scrollbar()
            .p(theme.space_4)
            .gap(theme.space_4)
            .child(
                render_section_shell(
                    "Runtime Overview",
                    "Live system state, provider availability, and active run pressure stay here.",
                    theme,
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .flex_wrap()
                        .gap(theme.space_2)
                        .child(metric_card("System", &data.runtime.status_label, theme.accent_aqua, theme))
                        .child(metric_card(
                            "Active agents",
                            data.runtime.active_agents.to_string(),
                            theme.accent_green,
                            theme,
                        ))
                        .child(metric_card(
                            "Streams",
                            data.runtime.active_streams.to_string(),
                            theme.accent_cyan,
                            theme,
                        ))
                        .child(metric_card(
                            "Providers",
                            format!(
                                "{}/{} online",
                                data.runtime.online_providers, data.runtime.total_providers
                            ),
                            theme.accent_yellow,
                            theme,
                        ))
                        .child(metric_card(
                            "Queue",
                            data.runtime.request_queue_length.to_string(),
                            theme.text_muted,
                            theme,
                        )),
                )
                .when_some(data.runtime.current_run_id.as_ref(), |el: Div, run_id| {
                    el.child(fact_card("Current run", run_id, theme.text_primary, theme))
                }),
            )
            .child(
                render_section_shell(
                    "Live Agents",
                    "Roles currently working or waiting in the active orchestration.",
                    theme,
                )
                .child(if data.runtime.agents.is_empty() {
                    empty_state_card(
                        "No active agents",
                        "Observe will show live roles here once a run is in flight.",
                        theme,
                    )
                } else {
                    div()
                        .flex()
                        .flex_col()
                        .gap(theme.space_2)
                        .children(data.runtime.agents.iter().map(|agent| {
                            fact_card(
                                &format!("{} - {}", agent.role, agent.status),
                                &format!(
                                    "{} - {} - {}",
                                    agent.phase, agent.model, agent.started_at
                                ),
                                theme.text_primary,
                                theme,
                            )
                        }))
                        .into_any_element()
                }),
            )
            .child(
                render_section_shell(
                    "Recent Runs",
                    "Recent orchestration runs stay visible without opening Monitor.",
                    theme,
                )
                .child(if data.runtime.recent_runs.is_empty() {
                    empty_state_card(
                        "No recent runs",
                        "Once runs complete or fail, their summaries will appear here.",
                        theme,
                    )
                } else {
                    div()
                        .flex()
                        .flex_col()
                        .gap(theme.space_2)
                        .children(data.runtime.recent_runs.iter().map(|run| {
                            fact_card(
                                &format!("{} - {}", run.id, run.status),
                                &format!(
                                    "{} - {} - ${:.4}",
                                    run.summary, run.started_at, run.cost_usd
                                ),
                                theme.text_primary,
                                theme,
                            )
                        }))
                        .into_any_element()
                }),
            )
            .child(
                render_section_shell(
                    "Runtime Timeline",
                    "Only the runtime-relevant activity rows are surfaced here.",
                    theme,
                )
                .child(Self::event_list_from_refs(&runtime_entries, data, theme)),
            )
            .into_any_element()
    }

    fn render_spend(data: &ActivityData, theme: &HiveTheme) -> AnyElement {
        let cost_entries = cost_entries(data);

        div()
            .flex()
            .flex_col()
            .flex_1()
            .overflow_y_scrollbar()
            .p(theme.space_4)
            .gap(theme.space_4)
            .child(
                render_section_shell(
                    "Spend Overview",
                    "Cost stays tied to request volume and model quality so routing decisions have context.",
                    theme,
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .flex_wrap()
                        .gap(theme.space_2)
                        .child(metric_card(
                            "24h spend",
                            format!("${:.4}", data.cost_summary.total_usd),
                            theme.accent_aqua,
                            theme,
                        ))
                        .child(metric_card(
                            "Requests",
                            data.cost_summary.request_count.to_string(),
                            theme.accent_cyan,
                            theme,
                        ))
                        .child(metric_card(
                            "Quality",
                            format!("{:.0}%", data.spend.quality_score * 100.0),
                            theme.accent_green,
                            theme,
                        ))
                        .child(metric_card(
                            "Cost efficiency",
                            format!("{:.3}", data.spend.cost_efficiency),
                            theme.text_muted,
                            theme,
                        )),
                )
                .child(fact_card(
                    "Trend",
                    &data.spend.quality_trend,
                    theme.text_primary,
                    theme,
                )),
            )
            .child(
                render_section_shell(
                    "Model Outcomes",
                    "Best and worst recent model performers stay explicit instead of hiding in Learning.",
                    theme,
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(theme.space_2)
                        .child(fact_card(
                            "Best model",
                            data.spend.best_model.as_deref().unwrap_or("No ranking yet"),
                            theme.accent_green,
                            theme,
                        ))
                        .child(fact_card(
                            "Needs attention",
                            data.spend
                                .worst_model
                                .as_deref()
                                .unwrap_or("No weak model signal yet"),
                            theme.accent_yellow,
                            theme,
                        )),
                )
                .when(!data.spend.weak_areas.is_empty(), |el: Div| {
                    el.child(
                        div()
                            .flex()
                            .flex_row()
                            .flex_wrap()
                            .gap(theme.space_2)
                            .children(
                                data.spend
                                    .weak_areas
                                    .iter()
                                    .map(|area| summary_chip(area.clone(), theme.accent_red, theme)),
                            ),
                    )
                }),
            )
            .child(
                render_section_shell(
                    "Cost Timeline",
                    "Recent cost events stay close to the high-level metrics for quick diagnosis.",
                    theme,
                )
                .child(Self::event_list_from_refs(&cost_entries, data, theme)),
            )
            .into_any_element()
    }

    fn render_safety(data: &ActivityData, theme: &HiveTheme) -> AnyElement {
        let safety_entries = safety_entries(data);

        div()
            .flex()
            .flex_col()
            .flex_1()
            .overflow_y_scrollbar()
            .p(theme.space_4)
            .gap(theme.space_4)
            .child(
                render_section_shell(
                    "Safety Overview",
                    "Observe keeps the protection layer visible even when no panel is shouting.",
                    theme,
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .flex_wrap()
                        .gap(theme.space_2)
                        .child(metric_card(
                            "Shield",
                            if data.safety.shield_enabled { "On" } else { "Off" },
                            if data.safety.shield_enabled {
                                theme.accent_green
                            } else {
                                theme.accent_red
                            },
                            theme,
                        ))
                        .child(metric_card(
                            "Secrets blocked",
                            data.safety.secrets_blocked.to_string(),
                            theme.accent_yellow,
                            theme,
                        ))
                        .child(metric_card(
                            "PII detections",
                            data.safety.pii_detections.to_string(),
                            theme.accent_cyan,
                            theme,
                        ))
                        .child(metric_card(
                            "Threats caught",
                            data.safety.threats_caught.to_string(),
                            if data.safety.threats_caught == 0 {
                                theme.accent_green
                            } else {
                                theme.accent_red
                            },
                            theme,
                        )),
                ),
            )
            .child(
                render_section_shell(
                    "Recent Safety Events",
                    "Recent shield catches and policy events stay visible without opening a secondary panel.",
                    theme,
                )
                .child(if data.safety.recent_events.is_empty() {
                    empty_state_card(
                        "No recent safety events",
                        "Shield issues will appear here when prompts or context are intercepted.",
                        theme,
                    )
                } else {
                    div()
                        .flex()
                        .flex_col()
                        .gap(theme.space_2)
                        .children(data.safety.recent_events.iter().map(|event| {
                            fact_card(
                                &format!("{} - {}", event.event_type, event.severity),
                                &format!("{} - {}", event.timestamp, event.detail),
                                safety_severity_color(&event.severity, theme),
                                theme,
                            )
                        }))
                        .into_any_element()
                }),
            )
            .child(
                render_section_shell(
                    "Safety Timeline",
                    "Recent approval denials, warnings, and shield-related activity rows.",
                    theme,
                )
                .child(Self::event_list_from_refs(&safety_entries, data, theme)),
            )
            .into_any_element()
    }

    fn filter_bar(data: &ActivityData, theme: &HiveTheme) -> impl IntoElement {
        let categories = [
            ("All", String::new()),
            ("Agents", "agent".to_string()),
            ("Tasks", "task".to_string()),
            ("Tools", "tool".to_string()),
            ("Costs", "cost".to_string()),
            ("Approvals", "approval".to_string()),
            ("Heartbeats", "heartbeat".to_string()),
        ];
        let mut bar = div()
            .id("activity-filter-bar")
            .flex()
            .flex_wrap()
            .items_center()
            .gap(theme.space_2)
            .pb(theme.space_2);

        for (label, categories_value) in categories {
            let is_selected = if categories_value.is_empty() {
                data.filter.categories.is_none()
            } else {
                data.filter.categories.as_ref().is_some_and(|selected| {
                    selected.len() == 1 && selected[0].eq_ignore_ascii_case(&categories_value)
                })
            };
            let categories_for_action = categories_value.clone();
            bar = bar.child(
                div()
                    .px(theme.space_2)
                    .py(theme.space_1)
                    .rounded(theme.radius_md)
                    .text_size(theme.font_size_xs)
                    .bg(if is_selected {
                        theme.bg_secondary
                    } else {
                        theme.bg_primary
                    })
                    .border_1()
                    .border_color(if is_selected {
                        theme.accent_cyan
                    } else {
                        theme.border
                    })
                    .text_color(if is_selected {
                        theme.text_primary
                    } else {
                        theme.text_muted
                    })
                    .cursor_pointer()
                    .on_mouse_down(MouseButton::Left, move |_event, window, cx| {
                        window.dispatch_action(
                            Box::new(ActivitySetFilter {
                                categories: categories_for_action.clone(),
                            }),
                            cx,
                        );
                    })
                    .child(label),
            );
        }
        bar
    }

    fn event_list_from_refs(
        entries: &[&ActivityEntry],
        data: &ActivityData,
        theme: &HiveTheme,
    ) -> AnyElement {
        if entries.is_empty() {
            return empty_state_card(
                "Nothing to show",
                "Observe will populate this section as activity arrives.",
                theme,
            );
        }

        div()
            .flex()
            .flex_col()
            .gap(theme.space_1)
            .children(entries.iter().map(|entry| {
                Self::event_row(entry, data.expanded_events.contains(&entry.id), theme)
            }))
            .into_any_element()
    }

    fn approval_card(request: &ApprovalRequest, theme: &HiveTheme) -> impl IntoElement {
        let request_id = request.id.clone();
        let deny_id = request.id.clone();

        div()
            .id(gpui::ElementId::Name(
                format!("activity-approval-{}", request.id).into(),
            ))
            .flex()
            .flex_col()
            .gap(theme.space_3)
            .p(theme.space_3)
            .rounded(theme.radius_lg)
            .bg(theme.bg_primary)
            .border_1()
            .border_color(theme.accent_yellow)
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
                                    .child(format_approval_operation(request)),
                            )
                            .child(
                                div()
                                    .text_size(theme.font_size_sm)
                                    .text_color(theme.text_secondary)
                                    .child(request.context.clone()),
                            )
                            .child(
                                div()
                                    .text_size(theme.font_size_xs)
                                    .text_color(theme.text_muted)
                                    .child(format!(
                                        "Agent: {} - Rule: {}{}",
                                        request.agent_id,
                                        request.matched_rule,
                                        request
                                            .estimated_cost
                                            .map(|cost| format!(" - Est. ${cost:.4}"))
                                            .unwrap_or_default()
                                    )),
                            ),
                    )
                    .child(summary_chip(
                        "Approval needed".into(),
                        theme.accent_yellow,
                        theme,
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap(theme.space_2)
                    .child(
                        div()
                            .px(theme.space_3)
                            .py(theme.space_2)
                            .rounded(theme.radius_md)
                            .bg(theme.accent_green)
                            .text_color(theme.text_on_accent)
                            .text_size(theme.font_size_sm)
                            .font_weight(FontWeight::SEMIBOLD)
                            .cursor_pointer()
                            .hover(|style| style.opacity(0.92))
                            .on_mouse_down(MouseButton::Left, move |_event, window, cx| {
                                window.dispatch_action(
                                    Box::new(ActivityApprove {
                                        request_id: request_id.clone(),
                                    }),
                                    cx,
                                );
                            })
                            .child("Approve"),
                    )
                    .child(
                        div()
                            .px(theme.space_3)
                            .py(theme.space_2)
                            .rounded(theme.radius_md)
                            .bg(theme.bg_secondary)
                            .border_1()
                            .border_color(theme.border)
                            .text_color(theme.text_primary)
                            .text_size(theme.font_size_sm)
                            .font_weight(FontWeight::MEDIUM)
                            .cursor_pointer()
                            .hover(|style| style.bg(theme.bg_surface))
                            .on_mouse_down(MouseButton::Left, move |_event, window, cx| {
                                window.dispatch_action(
                                    Box::new(ActivityDeny {
                                        request_id: deny_id.clone(),
                                        reason: "Denied from Observe inbox".into(),
                                    }),
                                    cx,
                                );
                            })
                            .child("Deny"),
                    ),
            )
    }

    fn event_row(entry: &ActivityEntry, expanded: bool, theme: &HiveTheme) -> impl IntoElement {
        let icon = match entry.category.as_str() {
            "agent" => IconName::Bot,
            "task" => IconName::File,
            "tool" => IconName::Settings,
            "cost" => IconName::ChartPie,
            "approval" => IconName::Bell,
            "heartbeat" => IconName::Loader,
            _ => IconName::Info,
        };

        let event_id = entry.id.to_string();
        let detail_json = entry.detail_json.clone();

        div()
            .id(gpui::ElementId::Name(
                format!("activity-{}", entry.id).into(),
            ))
            .flex()
            .flex_col()
            .gap(theme.space_1)
            .px(theme.space_3)
            .py(theme.space_2)
            .rounded(theme.radius_md)
            .bg(theme.bg_primary)
            .border_1()
            .border_color(theme.border)
            .hover(|s| s.bg(theme.bg_surface))
            .cursor_pointer()
            .on_mouse_down(MouseButton::Left, move |_event, window, cx| {
                window.dispatch_action(
                    Box::new(ActivityExpandEvent {
                        event_id: event_id.clone(),
                    }),
                    cx,
                );
            })
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(theme.space_2)
                    .child(Icon::new(icon).size_4().text_color(theme.text_muted))
                    .child(
                        div()
                            .flex_1()
                            .text_size(theme.font_size_xs)
                            .text_color(theme.text_primary)
                            .child(entry.summary.clone()),
                    )
                    .child(
                        div()
                            .text_size(theme.font_size_xs)
                            .text_color(theme.text_muted)
                            .child(entry.timestamp.clone()),
                    ),
            )
            .when(expanded, |el| {
                el.child(
                    div()
                        .ml(px(24.0))
                        .px(theme.space_3)
                        .py(theme.space_2)
                        .rounded(theme.radius_md)
                        .bg(theme.bg_secondary)
                        .border_1()
                        .border_color(theme.border)
                        .text_size(theme.font_size_xs)
                        .text_color(theme.text_secondary)
                        .font_family("Consolas, Menlo, monospace")
                        .child(detail_json.unwrap_or_default()),
                )
            })
    }
}

fn render_section_shell(title: &str, detail: &str, theme: &HiveTheme) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(theme.space_3)
        .p(theme.space_3)
        .rounded(theme.radius_lg)
        .bg(theme.bg_surface)
        .border_1()
        .border_color(theme.border)
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .child(
                    div()
                        .text_size(theme.font_size_base)
                        .text_color(theme.text_primary)
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(title.to_string()),
                )
                .child(
                    div()
                        .text_size(theme.font_size_sm)
                        .text_color(theme.text_secondary)
                        .child(detail.to_string()),
                ),
        )
}

fn metric_card(title: &str, value: impl Into<String>, color: Hsla, theme: &HiveTheme) -> AnyElement {
    div()
        .min_w(px(110.0))
        .px(theme.space_3)
        .py(theme.space_2)
        .rounded(theme.radius_md)
        .bg(theme.bg_primary)
        .border_1()
        .border_color(theme.border)
        .child(
            div()
                .text_size(theme.font_size_sm)
                .text_color(color)
                .font_weight(FontWeight::BOLD)
                .child(value.into()),
        )
        .child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .child(title.to_string()),
        )
        .into_any_element()
}

fn fact_card(title: &str, detail: &str, color: Hsla, theme: &HiveTheme) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(px(2.0))
        .px(theme.space_3)
        .py(theme.space_2)
        .rounded(theme.radius_md)
        .bg(theme.bg_primary)
        .border_1()
        .border_color(theme.border)
        .child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(color)
                .font_weight(FontWeight::SEMIBOLD)
                .child(title.to_string()),
        )
        .child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_secondary)
                .child(detail.to_string()),
        )
        .into_any_element()
}

fn empty_state_card(title: &str, detail: &str, theme: &HiveTheme) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(theme.space_2)
        .items_start()
        .justify_center()
        .px(theme.space_4)
        .py(theme.space_4)
        .rounded(theme.radius_lg)
        .bg(theme.bg_primary)
        .border_1()
        .border_color(theme.border)
        .child(
            div()
                .text_size(theme.font_size_sm)
                .text_color(theme.text_primary)
                .font_weight(FontWeight::SEMIBOLD)
                .child(title.to_string()),
        )
        .child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .child(detail.to_string()),
        )
        .into_any_element()
}

fn summary_chip(label: String, color: Hsla, theme: &HiveTheme) -> AnyElement {
    div()
        .px(theme.space_2)
        .py(theme.space_1)
        .rounded(theme.radius_full)
        .bg(theme.bg_surface)
        .border_1()
        .border_color(theme.border)
        .text_size(theme.font_size_xs)
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(color)
        .child(label)
        .into_any_element()
}

fn attention_entries(data: &ActivityData) -> Vec<&ActivityEntry> {
    data.entries
        .iter()
        .filter(|entry| is_attention_event(entry))
        .take(8)
        .collect()
}

fn runtime_entries(data: &ActivityData) -> Vec<&ActivityEntry> {
    data.entries
        .iter()
        .filter(|entry| matches!(entry.category.as_str(), "agent" | "task" | "tool" | "heartbeat"))
        .take(8)
        .collect()
}

fn cost_entries(data: &ActivityData) -> Vec<&ActivityEntry> {
    data.entries
        .iter()
        .filter(|entry| entry.category.eq_ignore_ascii_case("cost"))
        .take(8)
        .collect()
}

fn safety_entries(data: &ActivityData) -> Vec<&ActivityEntry> {
    data.entries
        .iter()
        .filter(|entry| {
            let event = entry.event_type.to_ascii_lowercase();
            let summary = entry.summary.to_ascii_lowercase();
            event.contains("approval")
                || event.contains("warning")
                || event.contains("denied")
                || summary.contains("shield")
                || summary.contains("secret")
                || summary.contains("pii")
                || summary.contains("threat")
        })
        .take(8)
        .collect()
}

fn is_attention_event(entry: &ActivityEntry) -> bool {
    let event = entry.event_type.to_ascii_lowercase();
    let summary = entry.summary.to_ascii_lowercase();
    event.contains("fail")
        || event.contains("error")
        || event.contains("denied")
        || event.contains("timeout")
        || event.contains("warning")
        || event.contains("exhausted")
        || summary.contains("failed")
        || summary.contains("error")
        || summary.contains("timeout")
        || summary.contains("warning")
}

fn safety_severity_color(severity: &str, theme: &HiveTheme) -> Hsla {
    match severity {
        "critical" | "high" => theme.accent_red,
        "medium" | "warning" => theme.accent_yellow,
        "low" | "info" => theme.accent_cyan,
        _ => theme.text_muted,
    }
}

fn format_approval_operation(request: &ApprovalRequest) -> String {
    use hive_agents::activity::types::OperationType;

    match &request.operation {
        OperationType::ShellCommand(command) => format!("Run shell command: {command}"),
        OperationType::FileDelete(path) => format!("Delete file: {path}"),
        OperationType::FileModify { path, scope } => {
            format!("Modify file: {path} ({scope})")
        }
        OperationType::GitPush { remote, branch } => {
            format!("Push git branch {branch} to {remote}")
        }
        OperationType::AiCall {
            model,
            estimated_cost,
        } => {
            format!("AI call via {model} (${estimated_cost:.4})")
        }
        OperationType::Custom(label) => label.clone(),
    }
}
