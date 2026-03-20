use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::scroll::ScrollableElement;

use crate::chat_service::{MessageRole, PendingToolApproval};
use hive_agents::activity::{ApprovalRequest, OperationType};

use super::{
    format_network_relative_time, quick_start_actions, ActivityApprove, ActivityDeny,
    AppApprovalGate, HiveTheme, HiveWorkspace, Panel, QuickStartOpenPanel, ShellDestination,
    ToolApprove, ToolReject,
};

pub(super) fn render_context_rail(
    workspace: &HiveWorkspace,
    cx: &mut Context<HiveWorkspace>,
) -> AnyElement {
    let theme = &workspace.theme;
    let active_destination = workspace.sidebar.active_destination;
    let (title, detail, sections) = match active_destination {
        ShellDestination::Build => (
            "Build Context",
            "Keep the active plan, run state, git handoff, and approvals visible while you work.",
            build_context_sections(workspace, cx),
        ),
        ShellDestination::Observe => (
            "Observe Context",
            "Pin the validation queue, runtime signals, spend, and safety state beside the main view.",
            observe_context_sections(workspace, cx),
        ),
        _ => return div().into_any_element(),
    };

    div()
        .id("context-rail")
        .w(px(332.0))
        .min_w(px(296.0))
        .h_full()
        .flex()
        .flex_col()
        .overflow_hidden()
        .bg(theme.bg_secondary)
        .border_l_1()
        .border_color(theme.border)
        .child(
            div()
                .px(theme.space_3)
                .py(theme.space_3)
                .border_b_1()
                .border_color(theme.border)
                .child(
                    div()
                        .text_size(theme.font_size_sm)
                        .text_color(theme.text_primary)
                        .font_weight(FontWeight::BOLD)
                        .child(title),
                )
                .child(
                    div()
                        .text_size(theme.font_size_xs)
                        .text_color(theme.text_muted)
                        .child(detail),
                ),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .overflow_y_scrollbar()
                .px(theme.space_3)
                .py(theme.space_3)
                .gap(theme.space_3)
                .children(sections),
        )
        .into_any_element()
}

fn build_context_sections(
    workspace: &HiveWorkspace,
    cx: &mut Context<HiveWorkspace>,
) -> Vec<AnyElement> {
    let theme = &workspace.theme;
    let mut sections = Vec::new();

    let template = workspace
        .quick_start_data
        .templates
        .iter()
        .find(|template| template.id == workspace.quick_start_data.selected_template);

    let focus_detail = if let Some(spec) = workspace.specs_data.active_spec() {
        format!(
            "{} - {}/{} items checked - updated {}",
            spec.status, spec.entries_checked, spec.entries_total, spec.updated_at
        )
    } else if let Some(template) = template {
        template.description.clone()
    } else {
        workspace.quick_start_data.project_summary.clone()
    };

    let focus_title = workspace
        .specs_data
        .active_spec()
        .map(|spec| spec.title.clone())
        .unwrap_or_else(|| {
            quick_start_actions::quick_start_template_title(&workspace.quick_start_data.selected_template)
                .to_string()
        });

    let focus_section = context_section_card(
        "Current Focus",
        "The plan stays pinned here even when the main panel changes.",
        theme,
    )
    .child(
        div()
            .flex()
            .flex_row()
            .flex_wrap()
            .gap(theme.space_2)
            .child(context_badge(
                if workspace.specs_data.active_spec().is_some() {
                    "Active spec"
                } else {
                    "Mission"
                },
                theme.accent_aqua,
                theme,
            ))
            .child(context_badge(
                if workspace.quick_start_data.launch_ready {
                    "Ready to run"
                } else {
                    "Needs setup"
                },
                if workspace.quick_start_data.launch_ready {
                    theme.accent_green
                } else {
                    theme.accent_yellow
                },
                theme,
            )),
    )
    .child(
        div()
            .text_size(theme.font_size_base)
            .text_color(theme.text_primary)
            .font_weight(FontWeight::SEMIBOLD)
            .child(focus_title),
    )
    .child(
        div()
            .text_size(theme.font_size_sm)
            .text_color(theme.text_secondary)
            .child(quick_start_actions::text_excerpt(&focus_detail, 170)),
    )
    .child(
        div()
            .flex()
            .flex_row()
            .flex_wrap()
            .gap(theme.space_2)
            .child(context_metric(
                "Files",
                workspace.quick_start_data.total_files.to_string(),
                theme.accent_cyan,
                theme,
            ))
            .child(context_metric(
                "Symbols",
                workspace.quick_start_data.key_symbols.to_string(),
                theme.accent_aqua,
                theme,
            ))
            .child(context_metric(
                "Deps",
                workspace.quick_start_data.dependencies.to_string(),
                theme.accent_green,
                theme,
            )),
    );
    sections.push(focus_section.into_any_element());

    let (
        current_model,
        message_count,
        is_streaming,
        pending_tool_approval,
        last_user_request,
        last_assistant_reply,
    ) = {
        let chat = workspace.chat_service.read(cx);
        let last_user = chat
            .messages()
            .iter()
            .rev()
            .find(|message| message.role == MessageRole::User)
            .map(|message| quick_start_actions::text_excerpt(&message.content, 120));
        let last_assistant = chat
            .messages()
            .iter()
            .rev()
            .find(|message| {
                message.role == MessageRole::Assistant && !message.content.trim().is_empty()
            })
            .map(|message| quick_start_actions::text_excerpt(&message.content, 120));
        (
            chat.current_model().to_string(),
            chat.messages().len(),
            chat.is_streaming(),
            chat.pending_approval.clone(),
            last_user,
            last_assistant,
        )
    };

    let mut execution_section = context_section_card(
        "Execution",
        "The live run state stays visible while you move between coding surfaces.",
        theme,
    )
    .child(
        div()
            .flex()
            .flex_row()
            .flex_wrap()
            .gap(theme.space_2)
            .child(context_metric(
                "Run",
                if is_streaming { "Active" } else { "Idle" },
                if is_streaming {
                    theme.accent_green
                } else {
                    theme.text_muted
                },
                theme,
            ))
            .child(context_metric(
                "Messages",
                message_count.to_string(),
                theme.accent_aqua,
                theme,
            ))
            .child(context_metric(
                "Model",
                if current_model.trim().is_empty() {
                    "Pending"
                } else {
                    current_model.as_str()
                },
                if current_model.trim().is_empty() {
                    theme.accent_yellow
                } else {
                    theme.accent_green
                },
                theme,
            )),
    );

    if let Some(request) = last_user_request {
        execution_section = execution_section.child(context_fact_row(
            "Latest request",
            &request,
            theme.text_primary,
            theme,
        ));
    }

    if let Some(approval) = pending_tool_approval {
        execution_section =
            execution_section.child(render_pending_tool_context(approval, theme, cx));
    } else if let Some(reply) = last_assistant_reply {
        execution_section = execution_section.child(context_fact_row(
            "Last response",
            &reply,
            theme.text_secondary,
            theme,
        ));
    }
    sections.push(execution_section.into_any_element());

    let mut git_section = context_section_card(
        "Git Handoff",
        "The current branch and dirty state stay pinned so review never becomes a context switch.",
        theme,
    );

    if workspace.review_data.is_repo {
        git_section = git_section
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .gap(theme.space_2)
                    .child(context_metric(
                        "Branch",
                        &workspace.review_data.branch,
                        theme.accent_aqua,
                        theme,
                    ))
                    .child(context_metric(
                        "Staged",
                        workspace.review_data.staged_count.to_string(),
                        theme.accent_green,
                        theme,
                    ))
                    .child(context_metric(
                        "Modified",
                        workspace.review_data.modified_count.to_string(),
                        theme.accent_yellow,
                        theme,
                    ))
                    .child(context_metric(
                        "Untracked",
                        workspace.review_data.untracked_count.to_string(),
                        theme.accent_red,
                        theme,
                    )),
            )
            .child(context_fact_row(
                "Last commit",
                &quick_start_actions::text_excerpt(&workspace.review_data.last_commit_msg, 120),
                theme.text_primary,
                theme,
            ));

        if let Some(path) = workspace.review_data.selected_file.as_ref() {
            git_section = git_section.child(context_fact_row(
                "Selected file",
                path,
                theme.text_secondary,
                theme,
            ));
        }
    } else {
        git_section = git_section.child(context_fact_row(
            "Repository status",
            "Git data is unavailable for this workspace.",
            theme.text_muted,
            theme,
        ));
    }
    sections.push(git_section.into_any_element());

    let mut next_section = context_section_card(
        "Next Actions",
        "These are the most likely follow-up moves after the current run or review step.",
        theme,
    );

    if workspace.quick_start_data.next_steps.is_empty() {
        next_section = next_section.child(render_context_action_row(
            "Open Git Ops",
            "Inspect the working tree and stage changes before shipping anything.",
            "Open Review",
            Panel::Review,
            theme,
            cx,
        ));
    } else {
        for step in workspace.quick_start_data.next_steps.iter().take(3) {
            let panel = Panel::from_stored(&step.panel);
            next_section = next_section.child(render_context_action_row(
                &step.title,
                &step.detail,
                &step.action_label,
                panel,
                theme,
                cx,
            ));
        }
    }
    sections.push(next_section.into_any_element());

    let approvals = pending_approval_requests(workspace, cx);
    if !approvals.is_empty() {
        let mut approvals_section = context_section_card(
            "Approval Queue",
            "High-friction actions stay visible here so the build lane does not hide validation work.",
            theme,
        );

        for request in approvals.iter().take(2) {
            approvals_section =
                approvals_section.child(render_context_approval_row(request, theme, cx));
        }

        if approvals.len() > 2 {
            approvals_section = approvals_section.child(
                div()
                    .text_size(theme.font_size_xs)
                    .text_color(theme.text_muted)
                    .child(format!(
                        "+{} more approval requests in Observe.",
                        approvals.len() - 2
                    )),
            );
        }

        sections.push(approvals_section.into_any_element());
    }

    sections
}

fn observe_context_sections(
    workspace: &HiveWorkspace,
    cx: &mut Context<HiveWorkspace>,
) -> Vec<AnyElement> {
    let theme = &workspace.theme;
    let approvals = pending_approval_requests(workspace, cx);
    let failure_count = workspace
        .activity_data
        .entries
        .iter()
        .filter(|entry| {
            let event = entry.event_type.to_ascii_lowercase();
            event.contains("fail") || event.contains("error")
        })
        .count();
    let online_providers = workspace
        .monitor_data
        .providers
        .iter()
        .filter(|provider| provider.online)
        .count();

    let mut sections = Vec::new();

    let mut validation_section = context_section_card(
        "Validation Queue",
        "Blocked actions and failure signals stay pinned while you inspect the timeline.",
        theme,
    )
    .child(
        div()
            .flex()
            .flex_row()
            .flex_wrap()
            .gap(theme.space_2)
            .child(context_metric(
                "Approvals",
                approvals.len().to_string(),
                if approvals.is_empty() {
                    theme.accent_green
                } else {
                    theme.accent_yellow
                },
                theme,
            ))
            .child(context_metric(
                "Failures",
                failure_count.to_string(),
                if failure_count == 0 {
                    theme.accent_green
                } else {
                    theme.accent_red
                },
                theme,
            ))
            .child(context_metric(
                "Alerts",
                workspace.shield_data.threats_caught.to_string(),
                if workspace.shield_data.threats_caught == 0 {
                    theme.accent_green
                } else {
                    theme.accent_red
                },
                theme,
            )),
    );

    if approvals.is_empty() {
        validation_section = validation_section.child(context_fact_row(
            "Approval state",
            "No pending approvals. Observe is currently clear.",
            theme.text_muted,
            theme,
        ));
    } else {
        for request in approvals.iter().take(3) {
            validation_section =
                validation_section.child(render_context_approval_row(request, theme, cx));
        }
    }
    sections.push(validation_section.into_any_element());

    let mut runtime_section = context_section_card(
        "Runtime Signals",
        "Live system and agent activity stay next to the inbox instead of disappearing inside Monitor.",
        theme,
    )
    .child(
        div()
            .flex()
            .flex_row()
            .flex_wrap()
            .gap(theme.space_2)
            .child(context_metric(
                "System",
                workspace.monitor_data.status.label(),
                match workspace.monitor_data.status {
                    hive_ui_panels::panels::monitor::AgentSystemStatus::Idle => theme.text_muted,
                    hive_ui_panels::panels::monitor::AgentSystemStatus::Running => {
                        theme.accent_green
                    }
                    hive_ui_panels::panels::monitor::AgentSystemStatus::Paused => {
                        theme.accent_yellow
                    }
                    hive_ui_panels::panels::monitor::AgentSystemStatus::Error => {
                        theme.accent_red
                    }
                },
                theme,
            ))
            .child(context_metric(
                "Agents",
                workspace.monitor_data.active_agents.len().to_string(),
                theme.accent_aqua,
                theme,
            ))
            .child(context_metric(
                "Streams",
                workspace.monitor_data.active_streams.to_string(),
                theme.accent_cyan,
                theme,
            ))
            .child(context_metric(
                "Providers",
                online_providers.to_string(),
                theme.accent_green,
                theme,
            )),
    );

    if let Some(run_id) = workspace.monitor_data.current_run_id.as_ref() {
        runtime_section = runtime_section.child(context_fact_row(
            "Current run",
            run_id,
            theme.text_primary,
            theme,
        ));
    }

    for agent in workspace.monitor_data.active_agents.iter().take(2) {
        runtime_section = runtime_section.child(context_fact_row(
            &format!("{} - {}", agent.role, agent.status.label()),
            &format!(
                "{} - {} - {}",
                agent.phase,
                quick_start_actions::text_excerpt(&agent.model, 36),
                format_network_relative_time(agent.started_at)
            ),
            theme.text_secondary,
            theme,
        ));
    }
    sections.push(runtime_section.into_any_element());

    let learning_quality = format!(
        "{:.0}%",
        workspace.learning_data.metrics.overall_quality * 100.0
    );
    let mut spend_section = context_section_card(
        "Spend & Learning",
        "Observe combines spend with model quality so routing changes stay tied to outcomes.",
        theme,
    )
    .child(
        div()
            .flex()
            .flex_row()
            .flex_wrap()
            .gap(theme.space_2)
            .child(context_metric(
                "24h spend",
                format!("${:.4}", workspace.activity_data.cost_summary.total_usd),
                theme.accent_aqua,
                theme,
            ))
            .child(context_metric(
                "Requests",
                workspace.activity_data.cost_summary.request_count.to_string(),
                theme.accent_cyan,
                theme,
            ))
            .child(context_metric(
                "Quality",
                learning_quality,
                theme.accent_green,
                theme,
            )),
    )
    .child(context_fact_row(
        "Trend",
        &workspace.learning_data.metrics.trend,
        theme.text_primary,
        theme,
    ));

    if let Some(best_model) = workspace.learning_data.best_model.as_ref() {
        spend_section = spend_section.child(context_fact_row(
            "Best model",
            best_model,
            theme.text_secondary,
            theme,
        ));
    }
    if let Some(worst_model) = workspace.learning_data.worst_model.as_ref() {
        spend_section = spend_section.child(context_fact_row(
            "Needs attention",
            worst_model,
            theme.text_secondary,
            theme,
        ));
    }
    sections.push(spend_section.into_any_element());

    let mut safety_section = context_section_card(
        "Safety",
        "Guardrail state stays visible so a quiet inbox does not hide real risk.",
        theme,
    )
    .child(
        div()
            .flex()
            .flex_row()
            .flex_wrap()
            .gap(theme.space_2)
            .child(context_metric(
                "Shield",
                if workspace.shield_data.shield_enabled {
                    "On"
                } else {
                    "Off"
                },
                if workspace.shield_data.shield_enabled {
                    theme.accent_green
                } else {
                    theme.accent_red
                },
                theme,
            ))
            .child(context_metric(
                "Secrets",
                workspace.shield_data.secrets_blocked.to_string(),
                theme.accent_yellow,
                theme,
            ))
            .child(context_metric(
                "PII",
                workspace.shield_data.pii_detections.to_string(),
                theme.accent_cyan,
                theme,
            )),
    );

    if let Some(event) = workspace.shield_data.recent_events.first() {
        safety_section = safety_section.child(context_fact_row(
            &format!("{} - {}", event.event_type, event.severity),
            &quick_start_actions::text_excerpt(&event.detail, 110),
            event.severity_color(theme),
            theme,
        ));
    }
    sections.push(safety_section.into_any_element());

    let mut timeline_section = context_section_card(
        "Latest Activity",
        "Recent events stay summarized here so you can keep the main pane focused on details.",
        theme,
    );

    if workspace.activity_data.entries.is_empty() {
        timeline_section = timeline_section.child(context_fact_row(
            "Timeline",
            "No activity entries have been loaded yet.",
            theme.text_muted,
            theme,
        ));
    } else {
        for entry in workspace.activity_data.entries.iter().take(3) {
            timeline_section = timeline_section.child(context_fact_row(
                &format!("{} - {}", entry.category, entry.timestamp),
                &quick_start_actions::text_excerpt(&entry.summary, 120),
                theme.text_secondary,
                theme,
            ));
        }
    }
    sections.push(timeline_section.into_any_element());

    sections
}

fn pending_approval_requests(workspace: &HiveWorkspace, cx: &App) -> Vec<ApprovalRequest> {
    if cx.has_global::<AppApprovalGate>() {
        cx.global::<AppApprovalGate>().0.pending_requests()
    } else {
        workspace.activity_data.pending_approvals.clone()
    }
}

fn render_pending_tool_context(
    approval: PendingToolApproval,
    theme: &HiveTheme,
    _cx: &mut Context<HiveWorkspace>,
) -> AnyElement {
    let file_path = approval.file_path.clone();
    let diff_lines = approval.diff_lines.len();

    div()
        .flex()
        .flex_col()
        .gap(theme.space_2)
        .px(theme.space_3)
        .py(theme.space_3)
        .rounded(theme.radius_md)
        .bg(theme.bg_primary)
        .border_1()
        .border_color(theme.accent_yellow)
        .child(
            div()
                .text_size(theme.font_size_sm)
                .text_color(theme.text_primary)
                .font_weight(FontWeight::SEMIBOLD)
                .child("Tool approval waiting"),
        )
        .child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_secondary)
                .child(format!(
                    "{} wants to update {} ({} diff lines).",
                    approval.tool_name, file_path, diff_lines
                )),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .gap(theme.space_2)
                .child(
                    div()
                        .px(theme.space_2)
                        .py(theme.space_1)
                        .rounded(theme.radius_md)
                        .bg(theme.bg_secondary)
                        .border_1()
                        .border_color(theme.border)
                        .text_size(theme.font_size_xs)
                        .text_color(theme.text_secondary)
                        .cursor_pointer()
                        .on_mouse_down(MouseButton::Left, |_event, window, cx| {
                            cx.stop_propagation();
                            window.dispatch_action(Box::new(ToolReject), cx);
                        })
                        .child("Reject"),
                )
                .child(
                    div()
                        .px(theme.space_2)
                        .py(theme.space_1)
                        .rounded(theme.radius_md)
                        .bg(theme.accent_green)
                        .text_size(theme.font_size_xs)
                        .text_color(theme.bg_primary)
                        .font_weight(FontWeight::SEMIBOLD)
                        .cursor_pointer()
                        .on_mouse_down(MouseButton::Left, |_event, window, cx| {
                            cx.stop_propagation();
                            window.dispatch_action(Box::new(ToolApprove), cx);
                        })
                        .child("Approve"),
                ),
        )
        .into_any_element()
}

fn render_context_action_row(
    title: &str,
    detail: &str,
    action_label: &str,
    panel: Panel,
    theme: &HiveTheme,
    cx: &mut Context<HiveWorkspace>,
) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .items_start()
        .gap(theme.space_2)
        .px(theme.space_3)
        .py(theme.space_2)
        .rounded(theme.radius_md)
        .bg(theme.bg_primary)
        .border_1()
        .border_color(theme.border)
        .cursor_pointer()
        .hover(|style| style.bg(theme.bg_tertiary))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |_this, _event, window, cx| {
                cx.stop_propagation();
                window.dispatch_action(
                    Box::new(QuickStartOpenPanel {
                        panel: panel.to_stored().into(),
                    }),
                    cx,
                );
            }),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .flex_1()
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
                        .text_color(theme.text_secondary)
                        .child(detail.to_string()),
                ),
        )
        .child(context_badge(action_label, theme.accent_aqua, theme))
        .into_any_element()
}

fn render_context_approval_row(
    request: &ApprovalRequest,
    theme: &HiveTheme,
    _cx: &mut Context<HiveWorkspace>,
) -> AnyElement {
    let approve_id = request.id.clone();
    let deny_id = request.id.clone();
    let operation_title = approval_operation_title(&request.operation);
    let operation_detail = approval_operation_detail(request);
    let context_detail = quick_start_actions::text_excerpt(&request.context, 96);

    div()
        .flex()
        .flex_col()
        .gap(theme.space_2)
        .px(theme.space_3)
        .py(theme.space_3)
        .rounded(theme.radius_md)
        .bg(theme.bg_primary)
        .border_1()
        .border_color(theme.border)
        .child(
            div()
                .text_size(theme.font_size_sm)
                .text_color(theme.text_primary)
                .font_weight(FontWeight::SEMIBOLD)
                .child(operation_title),
        )
        .child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_secondary)
                .child(operation_detail),
        )
        .when(!context_detail.is_empty(), |el| {
            el.child(
                div()
                    .text_size(theme.font_size_xs)
                    .text_color(theme.text_muted)
                    .child(context_detail.clone()),
            )
        })
        .child(
            div()
                .flex()
                .flex_row()
                .gap(theme.space_2)
                .child(
                    div()
                        .px(theme.space_2)
                        .py(theme.space_1)
                        .rounded(theme.radius_md)
                        .bg(theme.bg_secondary)
                        .border_1()
                        .border_color(theme.border)
                        .text_size(theme.font_size_xs)
                        .text_color(theme.text_secondary)
                        .cursor_pointer()
                        .on_mouse_down(MouseButton::Left, move |_event, window, cx| {
                            cx.stop_propagation();
                            window.dispatch_action(
                                Box::new(ActivityDeny {
                                    request_id: deny_id.clone(),
                                    reason: "Denied from context rail".into(),
                                }),
                                cx,
                            );
                        })
                        .child("Deny"),
                )
                .child(
                    div()
                        .px(theme.space_2)
                        .py(theme.space_1)
                        .rounded(theme.radius_md)
                        .bg(theme.accent_green)
                        .text_size(theme.font_size_xs)
                        .text_color(theme.bg_primary)
                        .font_weight(FontWeight::SEMIBOLD)
                        .cursor_pointer()
                        .on_mouse_down(MouseButton::Left, move |_event, window, cx| {
                            cx.stop_propagation();
                            window.dispatch_action(
                                Box::new(ActivityApprove {
                                    request_id: approve_id.clone(),
                                }),
                                cx,
                            );
                        })
                        .child("Approve"),
                ),
        )
        .into_any_element()
}

fn context_section_card(title: &str, detail: &str, theme: &HiveTheme) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(theme.space_3)
        .px(theme.space_3)
        .py(theme.space_3)
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
                        .text_size(theme.font_size_sm)
                        .text_color(theme.text_primary)
                        .font_weight(FontWeight::BOLD)
                        .child(title.to_string()),
                )
                .child(
                    div()
                        .text_size(theme.font_size_xs)
                        .text_color(theme.text_muted)
                        .child(detail.to_string()),
                ),
        )
}

fn context_metric(
    label: &str,
    value: impl Into<String>,
    accent: Hsla,
    theme: &HiveTheme,
) -> AnyElement {
    div()
        .min_w(px(82.0))
        .px(theme.space_2)
        .py(theme.space_2)
        .rounded(theme.radius_md)
        .bg(theme.bg_primary)
        .border_1()
        .border_color(theme.border)
        .child(
            div()
                .text_size(theme.font_size_sm)
                .text_color(accent)
                .font_weight(FontWeight::BOLD)
                .child(value.into()),
        )
        .child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .child(label.to_string()),
        )
        .into_any_element()
}

fn context_fact_row(
    title: &str,
    detail: &str,
    title_color: Hsla,
    theme: &HiveTheme,
) -> AnyElement {
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
                .text_color(title_color)
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

fn context_badge(label: &str, accent: Hsla, theme: &HiveTheme) -> AnyElement {
    div()
        .px(theme.space_2)
        .py(px(3.0))
        .rounded(theme.radius_full)
        .bg(theme.bg_primary)
        .border_1()
        .border_color(theme.border)
        .text_size(theme.font_size_xs)
        .text_color(accent)
        .font_weight(FontWeight::SEMIBOLD)
        .child(label.to_string())
        .into_any_element()
}

fn approval_operation_title(operation: &OperationType) -> String {
    match operation {
        OperationType::ShellCommand(command) => {
            format!("Shell command: {}", quick_start_actions::text_excerpt(command, 44))
        }
        OperationType::FileDelete(path) => {
            format!("Delete file: {}", quick_start_actions::text_excerpt(path, 44))
        }
        OperationType::FileModify { path, .. } => {
            format!("Edit file: {}", quick_start_actions::text_excerpt(path, 44))
        }
        OperationType::GitPush { remote, branch } => format!("Git push: {remote}/{branch}"),
        OperationType::AiCall { model, .. } => {
            format!("AI call: {}", quick_start_actions::text_excerpt(model, 32))
        }
        OperationType::Custom(label) => quick_start_actions::text_excerpt(label, 48),
    }
}

fn approval_operation_detail(request: &ApprovalRequest) -> String {
    let mut detail = format!(
        "{} - {} - {}",
        request.agent_id,
        request.matched_rule,
        format_network_relative_time(request.timestamp)
    );

    if let Some(cost) = request.estimated_cost {
        detail.push_str(&format!(" - ${cost:.4}"));
    }

    detail
}
