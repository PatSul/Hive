//! Learning panel — Continuous self-improvement dashboard.
//!
//! Displays performance metrics, learning log, preferences, prompt suggestions,
//! pattern library, routing insights, and self-evaluation reports.
//!
//! Cortex summary surfaces are included here:
//!   - Current cortex state (idle/processing/applied) from `AppCortexStatus`
//!   - Auto-apply toggle and applied-change count
//!   - Recent soaking/applied/rolled-back changes
//!   - Strategy weights and recent cortex event feed

use gpui::*;
use gpui_component::{Icon, IconName};

use hive_ui_core::HiveTheme;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Display data for a learning log entry.
#[derive(Debug, Clone)]
pub struct LogEntryDisplay {
    pub event_type: String,
    pub description: String,
    pub timestamp: String,
}

/// Display data for a learned preference.
#[derive(Debug, Clone)]
pub struct PreferenceDisplay {
    pub key: String,
    pub value: String,
    pub confidence: f64,
}

/// Display data for a prompt suggestion.
#[derive(Debug, Clone)]
pub struct PromptSuggestionDisplay {
    pub persona: String,
    pub reason: String,
    pub current_quality: f64,
}

/// Display data for a routing insight.
#[derive(Debug, Clone)]
pub struct RoutingInsightDisplay {
    pub task_type: String,
    pub from_tier: String,
    pub to_tier: String,
    pub confidence: f64,
}

/// Display data for the current Cortex status.
#[derive(Debug, Clone)]
pub struct CortexStatusDisplay {
    pub state: String,
    pub changes_applied: u32,
    pub auto_apply_enabled: bool,
}

/// Display data for an applied or soaking Cortex change.
#[derive(Debug, Clone)]
pub struct CortexChangeDisplay {
    pub change_id: String,
    pub domain: String,
    pub tier: String,
    pub status: String,
    pub action: String,
    pub applied_at: String,
    pub soak_until: String,
    pub quality_before: Option<f64>,
    pub quality_after: Option<f64>,
}

/// Display data for a learned improvement strategy.
#[derive(Debug, Clone)]
pub struct CortexStrategyDisplay {
    pub strategy_id: String,
    pub domain: String,
    pub weight: f64,
    pub attempts: u32,
    pub successes: u32,
    pub failures: u32,
    pub avg_impact: f64,
    pub last_adjusted: String,
}

/// Display data for the Cortex event feed.
#[derive(Debug, Clone)]
pub struct CortexEventDisplay {
    pub event_type: String,
    pub summary: String,
    pub timestamp: String,
}

/// Display data for quality metrics.
#[derive(Debug, Clone)]
pub struct QualityMetrics {
    pub overall_quality: f64,
    pub trend: String,
    pub total_interactions: u64,
    pub correction_rate: f64,
    pub regeneration_rate: f64,
    pub cost_efficiency: f64,
}

impl QualityMetrics {
    pub fn empty() -> Self {
        Self {
            overall_quality: 0.0,
            trend: "Stable".into(),
            total_interactions: 0,
            correction_rate: 0.0,
            regeneration_rate: 0.0,
            cost_efficiency: 0.0,
        }
    }
}

/// All data needed to render the learning panel.
#[derive(Debug, Clone)]
pub struct LearningPanelData {
    pub metrics: QualityMetrics,
    pub log_entries: Vec<LogEntryDisplay>,
    pub preferences: Vec<PreferenceDisplay>,
    pub prompt_suggestions: Vec<PromptSuggestionDisplay>,
    pub routing_insights: Vec<RoutingInsightDisplay>,
    pub cortex_status: CortexStatusDisplay,
    pub cortex_changes: Vec<CortexChangeDisplay>,
    pub cortex_strategies: Vec<CortexStrategyDisplay>,
    pub cortex_events: Vec<CortexEventDisplay>,
    pub weak_areas: Vec<String>,
    pub best_model: Option<String>,
    pub worst_model: Option<String>,
}

impl LearningPanelData {
    pub fn empty() -> Self {
        Self {
            metrics: QualityMetrics::empty(),
            log_entries: Vec::new(),
            preferences: Vec::new(),
            prompt_suggestions: Vec::new(),
            routing_insights: Vec::new(),
            cortex_status: CortexStatusDisplay {
                state: "idle".into(),
                changes_applied: 0,
                auto_apply_enabled: true,
            },
            cortex_changes: Vec::new(),
            cortex_strategies: Vec::new(),
            cortex_events: Vec::new(),
            weak_areas: Vec::new(),
            best_model: None,
            worst_model: None,
        }
    }

    #[allow(dead_code)]
    pub fn sample() -> Self {
        Self {
            metrics: QualityMetrics {
                overall_quality: 0.78,
                trend: "Improving".into(),
                total_interactions: 142,
                correction_rate: 0.12,
                regeneration_rate: 0.04,
                cost_efficiency: 0.032,
            },
            log_entries: vec![
                LogEntryDisplay {
                    event_type: "outcome_recorded".into(),
                    description: "Accepted response for model claude-sonnet-4 (quality: 0.90)"
                        .into(),
                    timestamp: "2m ago".into(),
                },
                LogEntryDisplay {
                    event_type: "routing_analysis".into(),
                    description: "Analyzed 50 interactions — no adjustments needed".into(),
                    timestamp: "15m ago".into(),
                },
                LogEntryDisplay {
                    event_type: "preference_learned".into(),
                    description: "Learned: code_style.naming = snake_case (confidence: 0.85)"
                        .into(),
                    timestamp: "1h ago".into(),
                },
            ],
            preferences: vec![
                PreferenceDisplay {
                    key: "code_style.naming".into(),
                    value: "snake_case".into(),
                    confidence: 0.85,
                },
                PreferenceDisplay {
                    key: "response_style.verbosity".into(),
                    value: "concise".into(),
                    confidence: 0.72,
                },
            ],
            prompt_suggestions: Vec::new(),
            routing_insights: vec![RoutingInsightDisplay {
                task_type: "debugging".into(),
                from_tier: "Budget".into(),
                to_tier: "Mid".into(),
                confidence: 0.78,
            }],
            cortex_status: CortexStatusDisplay {
                state: "applied".into(),
                changes_applied: 3,
                auto_apply_enabled: true,
            },
            cortex_changes: vec![CortexChangeDisplay {
                change_id: "chg_001".into(),
                domain: "prompts".into(),
                tier: "yellow".into(),
                status: "soaking".into(),
                action: "Promote prompt v12".into(),
                applied_at: "2m ago".into(),
                soak_until: "58m remaining".into(),
                quality_before: Some(0.58),
                quality_after: None,
            }],
            cortex_strategies: vec![CortexStrategyDisplay {
                strategy_id: "prompt_mutation".into(),
                domain: "prompts".into(),
                weight: 0.72,
                attempts: 12,
                successes: 9,
                failures: 3,
                avg_impact: 0.11,
                last_adjusted: "5m ago".into(),
            }],
            cortex_events: vec![CortexEventDisplay {
                event_type: "prompt_version_created".into(),
                summary: "coder v12 at 84% average quality".into(),
                timestamp: "1m ago".into(),
            }],
            weak_areas: vec!["regex_generation".into()],
            best_model: Some("claude-sonnet-4.5".into()),
            worst_model: Some("llama-3.1-8b".into()),
        }
    }
}

// ---------------------------------------------------------------------------
// Panel
// ---------------------------------------------------------------------------

pub struct LearningPanel;

impl LearningPanel {
    pub fn render(data: &LearningPanelData, theme: &HiveTheme) -> impl IntoElement {
        div()
            .id("learning-panel")
            .flex()
            .flex_col()
            .size_full()
            .overflow_y_scroll()
            .p(theme.space_4)
            .gap(theme.space_4)
            .child(render_header(theme))
            .child(render_cortex_section(
                &data.cortex_status,
                &data.cortex_changes,
                &data.cortex_strategies,
                &data.cortex_events,
                theme,
            ))
            .child(render_metrics_section(&data.metrics, theme))
            .child(render_model_performance(
                &data.best_model,
                &data.worst_model,
                &data.weak_areas,
                theme,
            ))
            .child(render_preferences_section(&data.preferences, theme))
            .child(render_routing_section(&data.routing_insights, theme))
            .child(render_log_section(&data.log_entries, theme))
    }
}

// ---------------------------------------------------------------------------
// Header
// ---------------------------------------------------------------------------

fn render_header(theme: &HiveTheme) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(theme.space_3)
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .w(px(40.0))
                .h(px(40.0))
                .rounded(theme.radius_lg)
                .bg(theme.bg_surface)
                .border_1()
                .border_color(theme.border)
                .child(Icon::new(IconName::Redo2).size_4()),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .child(
                    div()
                        .text_size(theme.font_size_xl)
                        .text_color(theme.text_primary)
                        .font_weight(FontWeight::BOLD)
                        .child("Continuous Learning"),
                )
                .child(
                    div()
                        .text_size(theme.font_size_sm)
                        .text_color(theme.text_muted)
                        .child("Self-improvement through outcome tracking and adaptation"),
                ),
        )
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Cortex overview
// ---------------------------------------------------------------------------

fn render_cortex_section(
    status: &CortexStatusDisplay,
    changes: &[CortexChangeDisplay],
    strategies: &[CortexStrategyDisplay],
    events: &[CortexEventDisplay],
    theme: &HiveTheme,
) -> AnyElement {
    let state_color = cortex_state_color(status, theme);

    div()
        .flex()
        .flex_col()
        .gap(theme.space_3)
        .p(theme.space_4)
        .rounded(theme.radius_md)
        .bg(theme.bg_surface)
        .border_1()
        .border_color(theme.border)
        .child(section_title("Cortex", theme))
        .child(
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .gap(theme.space_2)
                .child(cortex_chip(
                    "State",
                    if status.auto_apply_enabled {
                        status.state.as_str()
                    } else {
                        "paused"
                    },
                    state_color,
                    theme,
                ))
                .child(cortex_chip(
                    "Changes",
                    &status.changes_applied.to_string(),
                    theme.accent_cyan,
                    theme,
                ))
                .child(cortex_chip(
                    "Auto-apply",
                    if status.auto_apply_enabled {
                        "on"
                    } else {
                        "off"
                    },
                    if status.auto_apply_enabled {
                        theme.accent_green
                    } else {
                        theme.accent_yellow
                    },
                    theme,
                )),
        )
        .child(cortex_subsection("Recent Changes", changes, theme))
        .child(cortex_strategy_section(strategies, theme))
        .child(cortex_event_section(events, theme))
        .into_any_element()
}

fn cortex_chip(label: &str, value: &str, color: Hsla, theme: &HiveTheme) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap(theme.space_2)
        .px(theme.space_2)
        .py(px(2.0))
        .rounded(theme.radius_sm)
        .bg(theme.bg_tertiary)
        .text_size(theme.font_size_xs)
        .child(
            div()
                .text_color(theme.text_muted)
                .child(format!("{label}:")),
        )
        .child(
            div()
                .text_color(color)
                .font_weight(FontWeight::SEMIBOLD)
                .child(value.to_string()),
        )
        .into_any_element()
}

fn cortex_state_color(status: &CortexStatusDisplay, theme: &HiveTheme) -> Hsla {
    if !status.auto_apply_enabled {
        theme.accent_yellow
    } else {
        match status.state.as_str() {
            "processing" => theme.accent_yellow,
            "applied" => theme.accent_green,
            "paused" => theme.accent_yellow,
            _ => theme.text_secondary,
        }
    }
}

fn cortex_subsection(
    title: &str,
    changes: &[CortexChangeDisplay],
    theme: &HiveTheme,
) -> AnyElement {
    let mut section = div()
        .flex()
        .flex_col()
        .gap(theme.space_2)
        .child(section_title(title, theme));

    if changes.is_empty() {
        section = section.child(empty_state("No cortex changes recorded yet", theme));
    } else {
        for change in changes {
            section = section.child(cortex_change_row(change, theme));
        }
    }

    section.into_any_element()
}

fn cortex_change_row(change: &CortexChangeDisplay, theme: &HiveTheme) -> AnyElement {
    let status_color = match change.status.as_str() {
        "confirmed" => theme.accent_green,
        "rolled_back" => theme.accent_red,
        "soaking" => theme.accent_yellow,
        _ => theme.text_secondary,
    };

    div()
        .flex()
        .flex_col()
        .gap(theme.space_2)
        .p(theme.space_2)
        .rounded(theme.radius_sm)
        .bg(theme.bg_surface)
        .border_1()
        .border_color(theme.border)
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(theme.space_2)
                .child(
                    div()
                        .text_size(theme.font_size_sm)
                        .text_color(theme.text_primary)
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(change.action.clone()),
                )
                .child(div().flex_1())
                .child(cortex_chip("Status", &change.status, status_color, theme)),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .gap(theme.space_2)
                .child(cortex_inline_pair("Domain", &change.domain, theme))
                .child(cortex_inline_pair("Tier", &change.tier, theme))
                .child(cortex_inline_pair("Applied", &change.applied_at, theme))
                .child(cortex_inline_pair("Soak until", &change.soak_until, theme)),
        )
        .child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .child(match (change.quality_before, change.quality_after) {
                    (Some(before), Some(after)) => format!(
                        "Quality {:.0}% -> {:.0}% (id: {})",
                        before * 100.0,
                        after * 100.0,
                        change.change_id
                    ),
                    (Some(before), None) => format!(
                        "Baseline {:.0}% quality (id: {})",
                        before * 100.0,
                        change.change_id
                    ),
                    _ => format!("Change ID {}", change.change_id),
                }),
        )
        .into_any_element()
}

fn cortex_strategy_section(strategies: &[CortexStrategyDisplay], theme: &HiveTheme) -> AnyElement {
    let mut section = div()
        .flex()
        .flex_col()
        .gap(theme.space_2)
        .child(section_title("Strategy Weights", theme));

    if strategies.is_empty() {
        section = section.child(empty_state(
            "No Cortex strategies have been persisted yet",
            theme,
        ));
    } else {
        for strategy in strategies {
            section = section.child(cortex_strategy_row(strategy, theme));
        }
    }

    section.into_any_element()
}

fn cortex_strategy_row(strategy: &CortexStrategyDisplay, theme: &HiveTheme) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(theme.space_2)
        .p(theme.space_2)
        .rounded(theme.radius_sm)
        .bg(theme.bg_surface)
        .border_1()
        .border_color(theme.border)
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(theme.space_2)
                .child(
                    div()
                        .text_size(theme.font_size_sm)
                        .text_color(theme.text_primary)
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(strategy.strategy_id.clone()),
                )
                .child(
                    div()
                        .text_size(theme.font_size_xs)
                        .text_color(theme.text_muted)
                        .child(format!("{} / {}", strategy.domain, strategy.last_adjusted)),
                )
                .child(div().flex_1())
                .child(
                    div()
                        .text_size(theme.font_size_sm)
                        .text_color(theme.accent_cyan)
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(format!("{:.0}%", strategy.weight * 100.0)),
                ),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .gap(theme.space_2)
                .child(cortex_inline_pair(
                    "Attempts",
                    &strategy.attempts.to_string(),
                    theme,
                ))
                .child(cortex_inline_pair(
                    "Successes",
                    &strategy.successes.to_string(),
                    theme,
                ))
                .child(cortex_inline_pair(
                    "Failures",
                    &strategy.failures.to_string(),
                    theme,
                ))
                .child(cortex_inline_pair(
                    "Avg impact",
                    &format!("{:.0}%", strategy.avg_impact * 100.0),
                    theme,
                )),
        )
        .into_any_element()
}

fn cortex_event_section(events: &[CortexEventDisplay], theme: &HiveTheme) -> AnyElement {
    let mut section = div()
        .flex()
        .flex_col()
        .gap(theme.space_2)
        .child(section_title("Event Feed", theme));

    if events.is_empty() {
        section = section.child(empty_state(
            "No Cortex events have been recorded yet",
            theme,
        ));
    } else {
        for event in events {
            section = section.child(cortex_event_row(event, theme));
        }
    }

    section.into_any_element()
}

fn cortex_event_row(event: &CortexEventDisplay, theme: &HiveTheme) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(theme.space_2)
        .p(theme.space_2)
        .rounded(theme.radius_sm)
        .bg(theme.bg_surface)
        .border_1()
        .border_color(theme.border)
        .child(
            div()
                .px(theme.space_1)
                .py(px(1.0))
                .rounded(theme.radius_sm)
                .bg(theme.bg_tertiary)
                .text_size(theme.font_size_xs)
                .text_color(theme.accent_cyan)
                .min_w(px(132.0))
                .child(event.event_type.clone()),
        )
        .child(
            div()
                .flex_1()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_primary)
                .child(event.summary.clone()),
        )
        .child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .child(event.timestamp.clone()),
        )
        .into_any_element()
}

fn cortex_inline_pair(label: &str, value: &str, theme: &HiveTheme) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap(px(4.0))
        .text_size(theme.font_size_xs)
        .child(
            div()
                .text_color(theme.text_muted)
                .child(format!("{label}:")),
        )
        .child(
            div()
                .text_color(theme.text_secondary)
                .child(value.to_string()),
        )
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Metrics dashboard
// ---------------------------------------------------------------------------

fn render_metrics_section(metrics: &QualityMetrics, theme: &HiveTheme) -> AnyElement {
    let trend_color = match metrics.trend.as_str() {
        "Improving" => theme.accent_green,
        "Declining" => theme.accent_red,
        _ => theme.text_muted,
    };

    div()
        .flex()
        .flex_col()
        .gap(theme.space_3)
        .child(section_title("Performance", theme))
        .child(
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .gap(theme.space_3)
                .child(metric_card(
                    "Quality",
                    &format!("{:.0}%", metrics.overall_quality * 100.0),
                    theme.accent_cyan,
                    theme,
                ))
                .child(metric_card("Trend", &metrics.trend, trend_color, theme))
                .child(metric_card(
                    "Interactions",
                    &metrics.total_interactions.to_string(),
                    theme.text_secondary,
                    theme,
                ))
                .child(metric_card(
                    "Corrections",
                    &format!("{:.0}%", metrics.correction_rate * 100.0),
                    theme.accent_yellow,
                    theme,
                ))
                .child(metric_card(
                    "Regenerations",
                    &format!("{:.0}%", metrics.regeneration_rate * 100.0),
                    theme.accent_red,
                    theme,
                ))
                .child(metric_card(
                    "$/Quality",
                    &format!("${:.3}", metrics.cost_efficiency),
                    theme.accent_aqua,
                    theme,
                )),
        )
        .into_any_element()
}

fn metric_card(label: &str, value: &str, color: Hsla, theme: &HiveTheme) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .w(px(120.0))
        .p(theme.space_3)
        .gap(theme.space_1)
        .rounded(theme.radius_md)
        .bg(theme.bg_surface)
        .border_1()
        .border_color(theme.border)
        .child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .child(label.to_string()),
        )
        .child(
            div()
                .text_size(theme.font_size_lg)
                .text_color(color)
                .font_weight(FontWeight::BOLD)
                .child(value.to_string()),
        )
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Model performance
// ---------------------------------------------------------------------------

fn render_model_performance(
    best: &Option<String>,
    worst: &Option<String>,
    weak_areas: &[String],
    theme: &HiveTheme,
) -> AnyElement {
    let mut section = div()
        .flex()
        .flex_col()
        .gap(theme.space_2)
        .p(theme.space_4)
        .rounded(theme.radius_md)
        .bg(theme.bg_surface)
        .border_1()
        .border_color(theme.border)
        .child(section_title("Model Insights", theme));

    if let Some(b) = best {
        section = section.child(insight_row("Best model", b, theme.accent_green, theme));
    }
    if let Some(w) = worst {
        section = section.child(insight_row("Worst model", w, theme.accent_red, theme));
    }

    if !weak_areas.is_empty() {
        section = section.child(insight_row(
            "Weak areas",
            &weak_areas.join(", "),
            theme.accent_yellow,
            theme,
        ));
    }

    section.into_any_element()
}

fn insight_row(label: &str, value: &str, color: Hsla, theme: &HiveTheme) -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(theme.space_2)
        .child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .min_w(px(90.0))
                .child(label.to_string()),
        )
        .child(
            div()
                .text_size(theme.font_size_sm)
                .text_color(color)
                .child(value.to_string()),
        )
}

// ---------------------------------------------------------------------------
// Preferences
// ---------------------------------------------------------------------------

fn render_preferences_section(prefs: &[PreferenceDisplay], theme: &HiveTheme) -> AnyElement {
    let mut section = div()
        .flex()
        .flex_col()
        .gap(theme.space_2)
        .child(section_title("Learned Preferences", theme));

    if prefs.is_empty() {
        section = section.child(empty_state("No preferences learned yet", theme));
    } else {
        for pref in prefs {
            section = section.child(render_preference_row(pref, theme));
        }
    }

    section.into_any_element()
}

fn render_preference_row(pref: &PreferenceDisplay, theme: &HiveTheme) -> AnyElement {
    let conf_color = if pref.confidence > 0.8 {
        theme.accent_green
    } else if pref.confidence > 0.5 {
        theme.accent_yellow
    } else {
        theme.text_muted
    };

    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(theme.space_2)
        .p(theme.space_2)
        .rounded(theme.radius_sm)
        .bg(theme.bg_surface)
        .border_1()
        .border_color(theme.border)
        .child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_secondary)
                .min_w(px(160.0))
                .child(pref.key.clone()),
        )
        .child(
            div()
                .text_size(theme.font_size_sm)
                .text_color(theme.text_primary)
                .font_weight(FontWeight::MEDIUM)
                .child(pref.value.clone()),
        )
        .child(div().flex_1())
        .child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(conf_color)
                .child(format!("{:.0}%", pref.confidence * 100.0)),
        )
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Routing insights
// ---------------------------------------------------------------------------

fn render_routing_section(insights: &[RoutingInsightDisplay], theme: &HiveTheme) -> AnyElement {
    let mut section = div()
        .flex()
        .flex_col()
        .gap(theme.space_2)
        .child(section_title("Routing Insights", theme));

    if insights.is_empty() {
        section = section.child(empty_state("No routing adjustments yet", theme));
    } else {
        for insight in insights {
            section = section.child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(theme.space_2)
                    .p(theme.space_2)
                    .rounded(theme.radius_sm)
                    .bg(theme.bg_surface)
                    .border_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .text_size(theme.font_size_sm)
                            .text_color(theme.text_primary)
                            .child(insight.task_type.clone()),
                    )
                    .child(
                        div()
                            .text_size(theme.font_size_xs)
                            .text_color(theme.text_muted)
                            .child(format!("{} -> {}", insight.from_tier, insight.to_tier)),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .text_size(theme.font_size_xs)
                            .text_color(theme.accent_cyan)
                            .child(format!("{:.0}% conf", insight.confidence * 100.0)),
                    ),
            );
        }
    }

    section.into_any_element()
}

// ---------------------------------------------------------------------------
// Learning log
// ---------------------------------------------------------------------------

fn render_log_section(entries: &[LogEntryDisplay], theme: &HiveTheme) -> AnyElement {
    let mut section = div()
        .flex()
        .flex_col()
        .gap(theme.space_2)
        .child(section_title("Learning Log", theme));

    if entries.is_empty() {
        section = section.child(empty_state("No learning events recorded yet", theme));
    } else {
        for entry in entries {
            section = section.child(render_log_entry(entry, theme));
        }
    }

    section.into_any_element()
}

fn render_log_entry(entry: &LogEntryDisplay, theme: &HiveTheme) -> AnyElement {
    let type_color = match entry.event_type.as_str() {
        "outcome_recorded" => theme.accent_cyan,
        "routing_analysis" => theme.accent_aqua,
        "preference_learned" => theme.accent_green,
        "self_evaluation" => theme.accent_yellow,
        _ => theme.text_secondary,
    };

    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(theme.space_2)
        .py(theme.space_1)
        .child(
            div()
                .px(theme.space_1)
                .py(px(1.0))
                .rounded(theme.radius_sm)
                .bg(theme.bg_tertiary)
                .text_size(theme.font_size_xs)
                .text_color(type_color)
                .min_w(px(100.0))
                .child(entry.event_type.clone()),
        )
        .child(
            div()
                .flex_1()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_secondary)
                .child(entry.description.clone()),
        )
        .child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .child(entry.timestamp.clone()),
        )
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn section_title(title: &str, theme: &HiveTheme) -> Div {
    div()
        .text_size(theme.font_size_lg)
        .text_color(theme.text_primary)
        .font_weight(FontWeight::SEMIBOLD)
        .child(title.to_string())
}

fn empty_state(message: &str, theme: &HiveTheme) -> AnyElement {
    div()
        .flex()
        .items_center()
        .justify_center()
        .py(theme.space_4)
        .child(
            div()
                .text_size(theme.font_size_sm)
                .text_color(theme.text_muted)
                .child(message.to_string()),
        )
        .into_any_element()
}

// ---------------------------------------------------------------------------
