use std::collections::HashSet;

use hive_agents::activity::approval::ApprovalRequest;
use hive_agents::activity::log::{ActivityEntry, ActivityFilter, CostSummary};
use hive_ui_core::HiveTheme;

use gpui::prelude::*;
use gpui_component::IconName;

/// Data backing the Activity panel.
pub struct ActivityData {
    pub entries: Vec<ActivityEntry>,
    pub filter: ActivityFilter,
    pub pending_approvals: Vec<ApprovalRequest>,
    pub cost_summary: CostSummary,
    pub expanded_events: HashSet<i64>,
    pub search_query: String,
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
        }
    }
}

/// Stateless Activity panel renderer — same pattern as LogsPanel.
pub struct ActivityPanel;

impl ActivityPanel {
    pub fn render(data: &ActivityData, theme: &HiveTheme) -> impl IntoElement {
        use gpui::div;

        div()
            .id("activity-panel")
            .flex()
            .flex_col()
            .size_full()
            .child(Self::header(data, theme))
            .child(Self::filter_bar(data, theme))
            .child(Self::event_list(data, theme))
    }

    fn header(_data: &ActivityData, theme: &HiveTheme) -> impl IntoElement {
        use gpui::div;
        div()
            .id("activity-header")
            .flex()
            .items_center()
            .px(theme.space_4)
            .py(theme.space_2)
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(theme.space_2)
                    .child(
                        gpui_component::Icon::new(IconName::Inbox)
                            .size_4()
                            .text_color(theme.accent_cyan),
                    )
                    .child(
                        div()
                            .text_size(theme.font_size_lg)
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(theme.text_primary)
                            .child("Activity"),
                    ),
            )
    }

    fn filter_bar(_data: &ActivityData, theme: &HiveTheme) -> impl IntoElement {
        use gpui::div;
        let categories = ["agent", "task", "tool", "cost", "approval", "heartbeat"];
        let mut bar = div()
            .id("activity-filter-bar")
            .flex()
            .items_center()
            .gap(theme.space_2)
            .px(theme.space_4)
            .py(theme.space_1)
            .border_b_1()
            .border_color(theme.border);

        for cat in categories {
            bar = bar.child(
                div()
                    .px(theme.space_2)
                    .py(theme.space_1)
                    .rounded(theme.radius_md)
                    .text_size(theme.font_size_xs)
                    .bg(theme.bg_surface)
                    .text_color(theme.text_muted)
                    .child(cat),
            );
        }
        bar
    }

    fn event_list(data: &ActivityData, theme: &HiveTheme) -> impl IntoElement {
        use gpui::div;
        let mut list = div()
            .id("activity-event-list")
            .flex()
            .flex_col()
            .flex_1()
            .overflow_y_scroll();

        if data.entries.is_empty() {
            list = list.child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .py(theme.space_8)
                    .text_color(theme.text_muted)
                    .text_size(theme.font_size_sm)
                    .child("No activity events yet"),
            );
        } else {
            for entry in &data.entries {
                list = list.child(Self::event_row(entry, theme));
            }
        }
        list
    }

    fn event_row(entry: &ActivityEntry, theme: &HiveTheme) -> impl IntoElement {
        use gpui::div;

        let icon = match entry.category.as_str() {
            "agent" => IconName::Bot,
            "task" => IconName::File,
            "tool" => IconName::Settings,
            "cost" => IconName::ChartPie,
            "approval" => IconName::Bell,
            "heartbeat" => IconName::Loader,
            _ => IconName::Info,
        };

        div()
            .id(gpui::ElementId::Name(
                format!("activity-{}", entry.id).into(),
            ))
            .flex()
            .items_center()
            .gap(theme.space_2)
            .px(theme.space_4)
            .py(theme.space_1)
            .hover(|s| s.bg(theme.bg_surface))
            .child(
                gpui_component::Icon::new(icon)
                    .size_4()
                    .text_color(theme.text_muted),
            )
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
            )
    }
}
