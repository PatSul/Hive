use gpui::*;
use gpui_component::scroll::ScrollableElement;

use super::{
    sync_chat_cache, ActivityPanel, AgentsPanel, AssistantPanel, ChatPanel, CostsPanel,
    FilesPanel, HelpPanel, HistoryPanel, HiveTheme, HiveWorkspace, KanbanPanel, LearningPanel,
    LogsPanel, MonitorPanel, NetworkPanel, Panel, QuickStartPanel, ReviewPanel, RoutingPanel,
    SkillsPanel, SpecsPanel, TerminalPanel, TokenLaunchPanel, ToolApprove, ToolReject,
};

pub(super) fn render_active_panel(
    workspace: &mut HiveWorkspace,
    cx: &mut Context<HiveWorkspace>,
) -> AnyElement {
    if workspace.sidebar.active_panel == Panel::Chat {
        return render_chat_cached(workspace, cx);
    }

    let theme = &workspace.theme;
    match workspace.sidebar.active_panel {
        Panel::Chat => unreachable!(),
        Panel::QuickStart => QuickStartPanel::render(
            &workspace.quick_start_data,
            &workspace.quick_start_goal_input,
            theme,
        )
        .into_any_element(),
        Panel::History => HistoryPanel::render(&workspace.history_data, theme).into_any_element(),
        Panel::Files => FilesPanel::render(&workspace.files_data, theme).into_any_element(),
        Panel::CodeMap => {
            hive_ui_panels::panels::code_map::render_code_map(&workspace.code_map_data, theme)
                .into_any_element()
        }
        Panel::PromptLibrary => {
            hive_ui_panels::panels::prompt_library::render_prompt_library(
                &workspace.prompt_library_data,
                theme,
            )
            .into_any_element()
        }
        Panel::Kanban => KanbanPanel::render(&workspace.kanban_data, theme).into_any_element(),
        Panel::Monitor => MonitorPanel::render(&workspace.monitor_data, theme).into_any_element(),
        Panel::Activity => ActivityPanel::render(&workspace.activity_data, theme).into_any_element(),
        Panel::Logs => LogsPanel::render(&workspace.logs_data, theme).into_any_element(),
        Panel::Costs => CostsPanel::render(&workspace.cost_data, theme).into_any_element(),
        Panel::Review => ReviewPanel::render(&workspace.review_data, theme).into_any_element(),
        Panel::Skills => SkillsPanel::render(&workspace.skills_data, theme).into_any_element(),
        Panel::Routing => RoutingPanel::render(&workspace.routing_data, theme).into_any_element(),
        Panel::Workflows => workspace.workflow_builder_view.clone().into_any_element(),
        Panel::Channels => workspace.channels_view.clone().into_any_element(),
        Panel::Models => workspace.models_browser_view.clone().into_any_element(),
        Panel::TokenLaunch => {
            TokenLaunchPanel::render(&workspace.token_launch_data, &workspace.token_launch_inputs, theme)
                .into_any_element()
        }
        Panel::Specs => SpecsPanel::render(&workspace.specs_data, theme).into_any_element(),
        Panel::Agents => AgentsPanel::render(
            &workspace.agents_data,
            &workspace.agents_remote_prompt_input,
            theme,
        )
        .into_any_element(),
        Panel::Shield => workspace.shield_view.clone().into_any_element(),
        Panel::Learning => LearningPanel::render(&workspace.learning_data, theme).into_any_element(),
        Panel::Assistant => AssistantPanel::render(&workspace.assistant_data, theme).into_any_element(),
        Panel::Settings => workspace.settings_view.clone().into_any_element(),
        Panel::Help => HelpPanel::render(theme).into_any_element(),
        Panel::Network => NetworkPanel::render(&workspace.network_peer_data, theme).into_any_element(),
        Panel::Terminal => div()
            .flex()
            .flex_col()
            .size_full()
            .child(TerminalPanel::render(&workspace.terminal_data, theme))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .px(theme.space_4)
                    .py(theme.space_2)
                    .border_t_1()
                    .border_color(theme.border)
                    .bg(theme.bg_secondary)
                    .gap(theme.space_2)
                    .child(
                        div()
                            .text_size(theme.font_size_sm)
                            .text_color(theme.accent_cyan)
                            .font_family(theme.font_mono.clone())
                            .child("$"),
                    )
                    .child(div().flex_1().child(workspace.terminal_input.clone())),
            )
            .into_any_element(),
    }
}

fn render_chat_cached(
    workspace: &mut HiveWorkspace,
    cx: &mut Context<HiveWorkspace>,
) -> AnyElement {
    let svc = workspace.chat_service.read(cx);

    // Rebuild display messages only when the service has mutated.
    sync_chat_cache(&mut workspace.cached_chat_data, svc);

    let streaming_content = svc.streaming_content().to_string();
    let is_streaming = svc.is_streaming();
    let current_model = svc.current_model().to_string();
    let pending_approval = svc.pending_approval.clone();

    let chat_element = ChatPanel::render_cached(
        &mut workspace.cached_chat_data,
        &streaming_content,
        is_streaming,
        &current_model,
        &workspace.theme,
    );

    if let Some(approval) = pending_approval {
        div()
            .flex()
            .flex_col()
            .size_full()
            .child(chat_element)
            .child(render_approval_card(&approval, &workspace.theme))
            .into_any_element()
    } else {
        chat_element
    }
}

fn render_approval_card(
    approval: &crate::chat_service::PendingToolApproval,
    theme: &HiveTheme,
) -> AnyElement {
    use hive_ui_panels::components::code_block::render_code_block;
    use hive_ui_panels::components::diff_viewer::render_diff;

    let is_new_file = approval.old_content.is_none();
    let file_size = approval.new_content.len();

    let lang = approval
        .file_path
        .rsplit('.')
        .next()
        .map(|ext| match ext {
            "rs" => "Rust",
            "ts" | "tsx" => "TypeScript",
            "js" | "jsx" => "JavaScript",
            "py" => "Python",
            "toml" => "TOML",
            "json" => "JSON",
            "md" => "Markdown",
            "html" => "HTML",
            "css" => "CSS",
            "yaml" | "yml" => "YAML",
            _ => "text",
        })
        .unwrap_or("text");

    let diff_or_code: AnyElement = if is_new_file {
        render_code_block(&approval.new_content, lang, theme).into_any_element()
    } else {
        render_diff(&approval.diff_lines, theme).into_any_element()
    };

    div()
        .id("tool-approval-card")
        .mx(theme.space_4)
        .mb(theme.space_4)
        .rounded(theme.radius_md)
        .border_1()
        .border_color(theme.accent_yellow)
        .bg(theme.bg_secondary)
        .overflow_hidden()
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .px(theme.space_4)
                .py(theme.space_2)
                .bg(theme.bg_tertiary)
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(theme.space_2)
                        .child(
                            div()
                                .text_size(theme.font_size_sm)
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(theme.accent_yellow)
                                .child(if is_new_file {
                                    "Create file"
                                } else {
                                    "Modify file"
                                }),
                        )
                        .child(
                            div()
                                .text_size(theme.font_size_sm)
                                .text_color(theme.text_primary)
                                .child(approval.file_path.clone()),
                        )
                        .child(
                            div()
                                .text_size(theme.font_size_xs)
                                .text_color(theme.text_muted)
                                .child(format!("{} bytes", file_size)),
                        ),
                )
                .child(
                    div()
                        .text_size(theme.font_size_xs)
                        .text_color(theme.text_muted)
                        .child(lang),
                ),
        )
        .child(
            div()
                .max_h(px(300.0))
                .overflow_y_scrollbar()
                .child(diff_or_code),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_end()
                .gap(theme.space_2)
                .px(theme.space_4)
                .py(theme.space_2)
                .border_t_1()
                .border_color(theme.border)
                .child(
                    div()
                        .id("tool-reject")
                        .cursor_pointer()
                        .px(theme.space_4)
                        .py(theme.space_1)
                        .rounded(theme.radius_sm)
                        .bg(theme.accent_red)
                        .text_size(theme.font_size_sm)
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(white())
                        .hover(|s| s.opacity(0.8))
                        .on_mouse_down(MouseButton::Left, |_, window, cx| {
                            window.dispatch_action(Box::new(ToolReject), cx);
                        })
                        .child("Reject"),
                )
                .child(
                    div()
                        .id("tool-approve")
                        .cursor_pointer()
                        .px(theme.space_4)
                        .py(theme.space_1)
                        .rounded(theme.radius_sm)
                        .bg(theme.accent_green)
                        .text_size(theme.font_size_sm)
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(white())
                        .hover(|s| s.opacity(0.8))
                        .on_mouse_down(MouseButton::Left, |_, window, cx| {
                            window.dispatch_action(Box::new(ToolApprove), cx);
                        })
                        .child("Approve"),
                ),
        )
        .into_any_element()
}
