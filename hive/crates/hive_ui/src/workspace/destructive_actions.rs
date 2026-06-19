use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::input::Input;
use gpui_component::{Icon, IconName};
use hive_ui_core::{DestructiveActionKind, DestructiveConfirmation};
use tracing::info;

use super::{
    DestructiveCancel, DestructiveConfirm, HiveWorkspace, NotificationType, costs_actions,
    file_actions, history_actions, logs_actions, prompt_library_actions, shield_actions,
    token_launch_actions,
};

pub(super) fn request_confirmation(
    workspace: &mut HiveWorkspace,
    confirmation: DestructiveConfirmation,
    window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    info!("Destructive action requested: {}", confirmation.title);
    workspace.show_command_palette = false;
    workspace.show_project_dropdown = false;
    workspace.show_utility_drawer = false;
    workspace.history_data.confirming_clear = false;

    let placeholder = confirmation
        .acknowledgement_phrase
        .as_ref()
        .map(|phrase| format!("Type {phrase} to confirm"))
        .unwrap_or_else(|| "No typed confirmation required".to_string());
    let needs_acknowledgement = confirmation.acknowledgement_phrase.is_some();
    workspace
        .destructive_confirmation_input
        .update(cx, |input, cx| {
            input.set_value(String::new(), window, cx);
            input.set_placeholder(placeholder, window, cx);
        });

    workspace.pending_destructive_confirmation = Some(confirmation);
    cx.notify();

    if needs_acknowledgement {
        let focus_handle = workspace
            .destructive_confirmation_input
            .read(cx)
            .focus_handle(cx);
        window.focus(&focus_handle);
    } else {
        window.focus(&workspace.focus_handle);
    }
}

pub(super) fn handle_destructive_cancel(
    workspace: &mut HiveWorkspace,
    _action: &DestructiveCancel,
    window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    workspace.pending_destructive_confirmation = None;
    workspace
        .destructive_confirmation_input
        .update(cx, |input, cx| {
            input.set_value(String::new(), window, cx);
        });
    cx.notify();
    window.focus(&workspace.focus_handle);
}

pub(super) fn handle_destructive_confirm(
    workspace: &mut HiveWorkspace,
    _action: &DestructiveConfirm,
    window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    let Some(confirmation) = workspace.pending_destructive_confirmation.clone() else {
        return;
    };

    let typed_acknowledgement = workspace
        .destructive_confirmation_input
        .read(cx)
        .value()
        .trim()
        .to_string();

    if !confirmation.can_confirm(&typed_acknowledgement) {
        workspace.push_notification(
            cx,
            NotificationType::Warning,
            "Confirmation Required",
            "Type the exact confirmation phrase before continuing.",
        );
        return;
    }

    workspace.pending_destructive_confirmation = None;
    workspace
        .destructive_confirmation_input
        .update(cx, |input, cx| {
            input.set_value(String::new(), window, cx);
        });
    window.focus(&workspace.focus_handle);

    execute_confirmed_action(workspace, confirmation.action, cx);
    cx.notify();
}

pub(super) fn render_destructive_confirmation(
    workspace: &HiveWorkspace,
    cx: &mut Context<HiveWorkspace>,
) -> AnyElement {
    let Some(confirmation) = workspace.pending_destructive_confirmation.as_ref() else {
        return div().into_any_element();
    };

    let theme = &workspace.theme;
    let typed_acknowledgement = workspace
        .destructive_confirmation_input
        .read(cx)
        .value()
        .trim()
        .to_string();
    let can_confirm = confirmation.can_confirm(&typed_acknowledgement);
    let confirm_label = confirmation.confirm_label.clone();
    let cancel_label = confirmation.cancel_label.clone();
    let acknowledgement_phrase = confirmation.acknowledgement_phrase.clone();

    div()
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .bg(hsla(0.0, 0.0, 0.0, 0.44))
        .flex()
        .items_center()
        .justify_center()
        .px(theme.space_4)
        .on_mouse_down(MouseButton::Left, |_, window, cx| {
            cx.stop_propagation();
            window.dispatch_action(Box::new(DestructiveCancel), cx);
        })
        .child(
            div()
                .w_full()
                .max_w(px(540.0))
                .rounded(theme.radius_lg)
                .border_1()
                .border_color(theme.accent_red)
                .bg(theme.bg_secondary)
                .shadow_lg()
                .on_mouse_down(MouseButton::Left, |_, _window, cx| {
                    cx.stop_propagation();
                })
                .child(
                    div()
                        .flex()
                        .items_start()
                        .gap(theme.space_3)
                        .px(theme.space_4)
                        .pt(theme.space_4)
                        .child(
                            div()
                                .size(px(34.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(theme.radius_md)
                                .bg(theme.accent_red.opacity(0.14))
                                .child(
                                    Icon::new(IconName::TriangleAlert)
                                        .size_4()
                                        .text_color(theme.accent_red),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(theme.space_1)
                                .min_w(px(0.0))
                                .child(
                                    div()
                                        .text_size(theme.font_size_lg)
                                        .font_weight(FontWeight::BOLD)
                                        .text_color(theme.text_primary)
                                        .child(confirmation.title.clone()),
                                )
                                .child(
                                    div()
                                        .text_size(theme.font_size_sm)
                                        .text_color(theme.text_secondary)
                                        .child(confirmation.body.clone()),
                                ),
                        ),
                )
                .child(
                    div()
                        .px(theme.space_4)
                        .pt(theme.space_3)
                        .flex()
                        .flex_col()
                        .gap(theme.space_2)
                        .children(confirmation.details.iter().map(|detail| {
                            div()
                                .rounded(theme.radius_md)
                                .border_1()
                                .border_color(theme.border)
                                .bg(theme.bg_primary)
                                .px(theme.space_3)
                                .py(theme.space_2)
                                .text_size(theme.font_size_xs)
                                .text_color(theme.text_secondary)
                                .child(detail.clone())
                        })),
                )
                .when_some(acknowledgement_phrase, |el, phrase| {
                    el.child(
                        div()
                            .px(theme.space_4)
                            .pt(theme.space_3)
                            .flex()
                            .flex_col()
                            .gap(theme.space_2)
                            .child(
                                div()
                                    .text_size(theme.font_size_xs)
                                    .text_color(theme.text_muted)
                                    .child(format!("Type {phrase} to continue.")),
                            )
                            .child(
                                Input::new(&workspace.destructive_confirmation_input)
                                    .appearance(true)
                                    .cleanable(false)
                                    .text_size(theme.font_size_sm),
                            ),
                    )
                })
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap(theme.space_2)
                        .px(theme.space_4)
                        .py(theme.space_4)
                        .child(
                            div()
                                .px(theme.space_3)
                                .py(theme.space_2)
                                .rounded(theme.radius_md)
                                .border_1()
                                .border_color(theme.border)
                                .bg(theme.bg_primary)
                                .text_color(theme.text_primary)
                                .text_size(theme.font_size_sm)
                                .font_weight(FontWeight::MEDIUM)
                                .cursor_pointer()
                                .hover(|style| style.bg(theme.bg_surface))
                                .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                                    cx.stop_propagation();
                                    window.dispatch_action(Box::new(DestructiveCancel), cx);
                                })
                                .child(cancel_label),
                        )
                        .child(
                            div()
                                .px(theme.space_3)
                                .py(theme.space_2)
                                .rounded(theme.radius_md)
                                .bg(theme.accent_red)
                                .text_color(theme.text_on_accent)
                                .text_size(theme.font_size_sm)
                                .font_weight(FontWeight::SEMIBOLD)
                                .cursor_pointer()
                                .when(!can_confirm, |el| el.opacity(0.48))
                                .hover(move |style| {
                                    if can_confirm {
                                        style.opacity(0.88)
                                    } else {
                                        style
                                    }
                                })
                                .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                                    cx.stop_propagation();
                                    if can_confirm {
                                        window.dispatch_action(Box::new(DestructiveConfirm), cx);
                                    }
                                })
                                .child(confirm_label),
                        ),
                ),
        )
        .into_any_element()
}

fn execute_confirmed_action(
    workspace: &mut HiveWorkspace,
    action: DestructiveActionKind,
    cx: &mut Context<HiveWorkspace>,
) {
    match action {
        DestructiveActionKind::FilesDeleteEntry { target_path, .. } => {
            file_actions::execute_confirmed_files_delete(workspace, &target_path, cx);
        }
        DestructiveActionKind::HistoryDeleteConversation { conversation_id } => {
            history_actions::execute_confirmed_history_delete(workspace, &conversation_id, cx);
        }
        DestructiveActionKind::HistoryClearAll { .. } => {
            history_actions::execute_confirmed_history_clear_all(workspace, cx);
        }
        DestructiveActionKind::LogsClear { .. } => {
            logs_actions::execute_confirmed_logs_clear(workspace, cx);
        }
        DestructiveActionKind::CostsResetToday => {
            costs_actions::execute_confirmed_costs_reset_today(workspace, cx);
        }
        DestructiveActionKind::CostsClearHistory => {
            costs_actions::execute_confirmed_costs_clear_history(workspace, cx);
        }
        DestructiveActionKind::ReviewDiscardAll { .. } => {
            workspace.execute_confirmed_review_discard_all(cx);
        }
        DestructiveActionKind::ReviewBranchDelete { branch_name } => {
            workspace.execute_confirmed_review_branch_delete_named(&branch_name, cx);
        }
        DestructiveActionKind::ReviewGitflowFinish { kind, name } => {
            workspace.execute_confirmed_review_gitflow_finish_named(&kind, &name, cx);
        }
        DestructiveActionKind::PromptLibraryDelete { prompt_id } => {
            prompt_library_actions::execute_confirmed_prompt_library_delete(
                workspace, &prompt_id, cx,
            );
        }
        DestructiveActionKind::ShieldDeleteRule { rule_id } => {
            shield_actions::execute_confirmed_shield_delete_rule(workspace, &rule_id, cx);
        }
        DestructiveActionKind::TokenLaunchDeploy { .. } => {
            token_launch_actions::execute_confirmed_token_launch_deploy(workspace, cx);
        }
    }
}
