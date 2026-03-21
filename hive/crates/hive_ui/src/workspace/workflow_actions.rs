use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Context as _;
use chrono::Utc;
use gpui::*;
use tracing::{error, info, warn};

use hive_agents::automation::{
    USER_WORKFLOW_DIR, Workflow, WorkflowStepTemplate, WorkflowTemplate,
};

use super::{
    agents_actions, AiProvider, AppAiService, AppAutomation, AppChannels, AppNotification,
    AppNotifications, AppPersonas, ChannelMessageSent, ChatRequest, HiveWorkspace,
    NotificationType, WorkflowBuilderLoadWorkflow,
};
use hive_ui_panels::panels::workflow_builder::{WorkflowCanvasState, WorkflowListEntry};

pub(super) fn refresh_workflow_builder(
    workspace: &mut HiveWorkspace,
    cx: &mut Context<HiveWorkspace>,
) {
    if cx.has_global::<AppAutomation>() {
        let automation = &cx.global::<AppAutomation>().0;
        let workflows = automation.list_workflows();
        let entries: Vec<WorkflowListEntry> = workflows
            .iter()
            .map(|workflow| WorkflowListEntry {
                id: workflow.id.clone(),
                name: workflow.name.clone(),
                is_builtin: workflow.id.starts_with("builtin:"),
                status: format!("{:?}", workflow.status),
            })
            .collect();

        workspace.workflow_builder_view.update(cx, |view, cx| {
            view.refresh_workflow_list(entries, cx);
        });
    }
}

pub(super) fn handle_workflow_saved(
    workspace: &mut HiveWorkspace,
    canvas_workflow_id: &str,
    cx: &mut Context<HiveWorkspace>,
) {
    info!("Workflow saved: {}", canvas_workflow_id);

    let workflow = workspace.workflow_builder_view.read(cx).to_executable_workflow();
    if workflow.steps.is_empty() {
        push_workflow_notification(
            cx,
            NotificationType::Warning,
            "Workflow Canvas Saved",
            format!(
                "Saved the canvas for '{}', but it still needs at least one Action node before Hive can register it as an executable workflow.",
                workflow.name
            ),
        );
        return;
    }

    match persist_workflow_template(&workspace.current_project_root, &workflow) {
        Ok(path) => {
            let report = reload_workspace_workflows(workspace, cx);
            workspace.workflow_builder_view.update(cx, |view, cx| {
                view.set_active_workflow_id(Some(format!("file:{}", workflow.id)), cx);
            });
            refresh_workflow_builder(workspace, cx);
            agents_actions::refresh_agents_data(workspace, cx);

            let (notif_type, summary) = if let Some(report) = &report {
                (
                    if report.failed > 0 {
                        NotificationType::Warning
                    } else {
                        NotificationType::Success
                    },
                    format!(
                        "Saved '{}' to {} and reloaded {} workflow file(s).",
                        workflow.name,
                        path.display(),
                        report.loaded
                    ),
                )
            } else {
                (
                    NotificationType::Success,
                    format!("Saved '{}' to {}.", workflow.name, path.display()),
                )
            };
            push_workflow_notification(cx, notif_type, "Workflow Saved", summary);
        }
        Err(err) => {
            warn!(
                workflow_id = %workflow.id,
                error = %err,
                "WorkflowBuilder: failed to persist workflow template"
            );
            push_workflow_notification(
                cx,
                NotificationType::Error,
                "Workflow Save Failed",
                format!(
                    "Saved the canvas, but Hive could not write the executable workflow file: {err}"
                ),
            );
        }
    }
}

pub(super) fn handle_workflow_builder_load(
    workspace: &mut HiveWorkspace,
    action: &WorkflowBuilderLoadWorkflow,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    let workflow_id = action.workflow_id.clone();
    let canvas = workflow_canvas_for_id(workspace, &workflow_id, cx);

    match canvas {
        Some(canvas) => {
            workspace.workflow_builder_view.update(cx, |view, cx| {
                view.load_canvas(canvas, Some(workflow_id.clone()), cx);
            });
            info!("WorkflowBuilder: loaded workflow '{workflow_id}'");
        }
        None => {
            warn!("WorkflowBuilder: failed to load workflow '{workflow_id}'");
            push_workflow_notification(
                cx,
                NotificationType::Error,
                "Workflow Load Failed",
                format!(
                    "Hive couldn't load '{workflow_id}'. The workflow file or saved canvas may be missing."
                ),
            );
        }
    }
}

pub(super) fn refresh_channels_view(
    workspace: &mut HiveWorkspace,
    cx: &mut Context<HiveWorkspace>,
) {
    if cx.has_global::<AppChannels>() {
        let channel_data: Vec<_> = cx
            .global::<AppChannels>()
            .0
            .list_channels()
            .iter()
            .map(|channel| {
                (
                    channel.id.clone(),
                    channel.name.clone(),
                    channel.icon.clone(),
                    channel.description.clone(),
                    channel.messages.len(),
                    channel.assigned_agents.clone(),
                )
            })
            .collect();

        workspace.channels_view.update(cx, |view, cx| {
            view.refresh_from_data(channel_data, cx);
        });
    }
}

pub(super) fn handle_workflow_run_requested(
    workspace: &mut HiveWorkspace,
    workflow_id: String,
    cx: &mut Context<HiveWorkspace>,
) {
    info!("Workflow run requested: {}", workflow_id);

    let workflow = workspace.workflow_builder_view.read(cx).to_executable_workflow();

    if workflow.steps.is_empty() {
        warn!(
            "WorkflowBuilder: no executable steps in workflow '{}'",
            workflow_id
        );
        if cx.has_global::<AppNotifications>() {
            cx.global_mut::<AppNotifications>().0.push(
                AppNotification::new(
                    NotificationType::Warning,
                    format!(
                        "Workflow '{}' has no executable steps. Add Action nodes to the canvas.",
                        workflow.name
                    ),
                )
                .with_title("Workflow Empty"),
            );
        }
        return;
    }

    info!(
        "WorkflowBuilder: running '{}' with {} step(s)",
        workflow.name,
        workflow.steps.len()
    );

    if cx.has_global::<AppNotifications>() {
        cx.global_mut::<AppNotifications>().0.push(
            AppNotification::new(
                NotificationType::Info,
                format!(
                    "Running workflow '{}' ({} step(s))",
                    workflow.name,
                    workflow.steps.len()
                ),
            )
            .with_title("Workflow Started"),
        );
    }

    let working_dir = workspace
        .current_project_root
        .clone()
        .canonicalize()
        .unwrap_or_else(|_| workspace.current_project_root.clone());
    let workflow_for_thread = workflow.clone();
    let run_result = std::sync::Arc::new(std::sync::Mutex::new(None));
    let run_result_for_thread = std::sync::Arc::clone(&run_result);

    std::thread::spawn(move || {
        let result = hive_agents::automation::AutomationService::execute_workflow_blocking(
            &workflow_for_thread,
            working_dir,
        );
        *run_result_for_thread
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Some(result);
    });

    let run_result_for_ui = std::sync::Arc::clone(&run_result);
    let workflow_name = workflow.name.clone();

    cx.spawn(async move |this, app: &mut AsyncApp| {
        loop {
            if let Some(result) = run_result_for_ui
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .take()
            {
                let _ = this.update(app, |workspace, cx| {
                    match result {
                        Ok(run) => {
                            if cx.has_global::<AppAutomation>() {
                                let _ = cx.global_mut::<AppAutomation>().0.record_run(
                                    &run.workflow_id,
                                    run.success,
                                    run.steps_completed,
                                    run.error.clone(),
                                );
                            }

                            if cx.has_global::<AppNotifications>() {
                                let (notif_type, title) = if run.success {
                                    (NotificationType::Success, "Workflow Complete")
                                } else {
                                    (NotificationType::Error, "Workflow Failed")
                                };
                                let msg = if run.success {
                                    format!(
                                        "Workflow '{}' completed ({} steps)",
                                        workflow_name, run.steps_completed
                                    )
                                } else {
                                    format!(
                                        "Workflow '{}' failed after {} step(s): {}",
                                        workflow_name,
                                        run.steps_completed,
                                        run.error.as_deref().unwrap_or("unknown error")
                                    )
                                };
                                cx.global_mut::<AppNotifications>().0.push(
                                    AppNotification::new(notif_type, msg).with_title(title),
                                );
                            }
                        }
                        Err(e) => {
                            warn!("WorkflowBuilder: run error: {e}");
                            if cx.has_global::<AppNotifications>() {
                                cx.global_mut::<AppNotifications>().0.push(
                                    AppNotification::new(
                                        NotificationType::Error,
                                        format!("Workflow run failed: {e}"),
                                    )
                                    .with_title("Workflow Run Failed"),
                                );
                            }
                        }
                    }

                    agents_actions::refresh_agents_data(workspace, cx);
                    cx.notify();
                });
                break;
            }

            app.background_executor()
                .timer(std::time::Duration::from_millis(120))
                .await;
        }
    })
    .detach();
}

fn push_workflow_notification(
    cx: &mut Context<HiveWorkspace>,
    notification_type: NotificationType,
    title: impl Into<String>,
    message: impl Into<String>,
) {
    if cx.has_global::<AppNotifications>() {
        cx.global_mut::<AppNotifications>()
            .0
            .push(AppNotification::new(notification_type, message.into()).with_title(title));
    }
}

fn reload_workspace_workflows(
    workspace: &HiveWorkspace,
    cx: &mut Context<HiveWorkspace>,
) -> Option<hive_agents::automation::WorkflowLoadReport> {
    if !cx.has_global::<AppAutomation>() {
        return None;
    }

    let workspace_root = workspace.current_project_root.clone();
    Some({
        let automation = &mut cx.global_mut::<AppAutomation>().0;
        automation.ensure_builtin_workflows();
        automation.reload_user_workflows(&workspace_root)
    })
}

fn workflow_canvas_for_id(
    _workspace: &HiveWorkspace,
    workflow_id: &str,
    cx: &Context<HiveWorkspace>,
) -> Option<WorkflowCanvasState> {
    if let Some(canvas_id) = workflow_file_stem(workflow_id)
        && let Ok(canvas) = WorkflowCanvasState::load_from_disk(canvas_id)
    {
        return Some(canvas);
    }

    if !cx.has_global::<AppAutomation>() {
        return None;
    }

    let workflow = cx
        .global::<AppAutomation>()
        .0
        .get_workflow(workflow_id)
        .cloned()?;
    Some(WorkflowCanvasState::from_workflow(
        &workflow,
        generated_canvas_id(workflow_id),
    ))
}

fn persist_workflow_template(workspace_root: &Path, workflow: &Workflow) -> anyhow::Result<PathBuf> {
    let workflow_dir = workspace_root.join(USER_WORKFLOW_DIR);
    std::fs::create_dir_all(&workflow_dir).with_context(|| {
        format!(
            "failed to create workflow directory {}",
            workflow_dir.display()
        )
    })?;

    let safe_id = sanitize_workflow_id(&workflow.id);
    let path = workflow_dir.join(format!("{safe_id}.json"));
    let tmp_path = workflow_dir.join(format!("{safe_id}.json.tmp"));
    let template = workflow_to_template(workflow);
    let json = serde_json::to_string_pretty(&template)
        .context("failed to serialize workflow template to JSON")?;
    std::fs::write(&tmp_path, &json)
        .with_context(|| format!("failed to write workflow file {}", tmp_path.display()))?;
    std::fs::rename(&tmp_path, &path)
        .with_context(|| format!("failed to finalize workflow file {}", path.display()))?;
    Ok(path)
}

fn workflow_to_template(workflow: &Workflow) -> WorkflowTemplate {
    WorkflowTemplate {
        name: workflow.name.clone(),
        description: workflow.description.clone(),
        trigger: Some(workflow.trigger.clone()),
        enabled: workflow.status != hive_agents::automation::WorkflowStatus::Paused,
        steps: workflow
            .steps
            .iter()
            .map(|step| WorkflowStepTemplate {
                name: step.name.clone(),
                action: step.action.clone(),
                conditions: step.conditions.clone(),
                timeout_secs: step.timeout_secs,
                retry_count: step.retry_count,
            })
            .collect(),
    }
}

/// Strip path separators and traversal sequences from a workflow ID so it
/// cannot escape its target directory when used in a filename.
fn sanitize_workflow_id(id: &str) -> String {
    id.replace(['/', '\\', ':', '\0'], "_")
        .replace("..", "_")
}

fn workflow_file_stem(workflow_id: &str) -> Option<&str> {
    workflow_id.strip_prefix("file:")
}

fn generated_canvas_id(workflow_id: &str) -> String {
    workflow_file_stem(workflow_id)
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use hive_agents::automation::{ActionType, TriggerType, WorkflowStatus, WorkflowStep};

    fn sample_workflow() -> Workflow {
        Workflow {
            id: "1234-test".into(),
            name: "Ship It".into(),
            description: "Run checks and notify".into(),
            trigger: TriggerType::ManualTrigger,
            steps: vec![WorkflowStep {
                id: "step-1".into(),
                name: "Cargo check".into(),
                action: ActionType::RunCommand {
                    command: "cargo check --quiet".into(),
                },
                conditions: Vec::new(),
                timeout_secs: Some(300),
                retry_count: 1,
            }],
            status: WorkflowStatus::Active,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            last_run: None,
            run_count: 0,
        }
    }

    #[test]
    fn workflow_to_template_preserves_step_shape() {
        let template = workflow_to_template(&sample_workflow());
        assert_eq!(template.name, "Ship It");
        assert_eq!(template.steps.len(), 1);
        assert!(template.enabled);
        match &template.steps[0].action {
            ActionType::RunCommand { command } => assert_eq!(command, "cargo check --quiet"),
            action => panic!("unexpected action: {action:?}"),
        }
    }

    #[test]
    fn persist_workflow_template_writes_workspace_json() {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = persist_workflow_template(temp_dir.path(), &sample_workflow()).unwrap();

        assert_eq!(
            path,
            temp_dir
                .path()
                .join(USER_WORKFLOW_DIR)
                .join("1234-test.json")
        );

        let raw = std::fs::read_to_string(path).unwrap();
        let parsed: WorkflowTemplate = serde_json::from_str(&raw).unwrap();
        assert_eq!(parsed.name, "Ship It");
        assert_eq!(parsed.steps.len(), 1);
    }
}

pub(super) fn handle_channel_message_sent(
    workspace: &mut HiveWorkspace,
    event: &ChannelMessageSent,
    cx: &mut Context<HiveWorkspace>,
) {
    info!(
        "Channel message sent in {}: {}",
        event.channel_id, event.content
    );

    if cx.has_global::<AppChannels>() {
        let user_msg = hive_core::channels::ChannelMessage {
            id: uuid::Uuid::new_v4().to_string(),
            author: hive_core::channels::MessageAuthor::User,
            content: event.content.clone(),
            timestamp: Utc::now(),
            thread_id: None,
            model: None,
            cost: None,
        };
        cx.global_mut::<AppChannels>()
            .0
            .add_message(&event.channel_id, user_msg);
    }

    handle_channel_agent_responses(
        workspace,
        event.channel_id.clone(),
        event.assigned_agents.clone(),
        cx,
    );
}

fn handle_channel_agent_responses(
    workspace: &mut HiveWorkspace,
    channel_id: String,
    assigned_agents: Vec<String>,
    cx: &mut Context<HiveWorkspace>,
) {
    if assigned_agents.is_empty() {
        return;
    }

    let model = workspace.chat_service.read(cx).current_model().to_string();
    if model.is_empty() || model == "Select Model" {
        warn!("Channels: no model selected, cannot trigger agent responses");
        return;
    }

    let mut context_messages = Vec::new();

    if cx.has_global::<AppChannels>() {
        let store = &cx.global::<AppChannels>().0;
        if let Some(channel) = store.get_channel(&channel_id) {
            let recent = channel.messages.iter().rev().take(10).rev();
            for msg in recent {
                let role = match &msg.author {
                    hive_core::channels::MessageAuthor::User => hive_ai::types::MessageRole::User,
                    hive_core::channels::MessageAuthor::Agent { .. } => {
                        hive_ai::types::MessageRole::Assistant
                    }
                    hive_core::channels::MessageAuthor::System => {
                        hive_ai::types::MessageRole::System
                    }
                };
                context_messages.push(hive_ai::types::ChatMessage {
                    role,
                    content: msg.content.clone(),
                    timestamp: msg.timestamp,
                    tool_calls: None,
                    tool_call_id: None,
                });
            }
        }
    }

    if let Some(first_agent) = assigned_agents.first() {
        workspace.channels_view.update(cx, |view, cx| {
            view.set_streaming(first_agent, "", cx);
        });
    }

    for agent_name in assigned_agents {
        let persona = if cx.has_global::<AppPersonas>() {
            cx.global::<AppPersonas>()
                .0
                .find_by_name(&agent_name)
                .cloned()
        } else {
            None
        };

        let system_prompt = persona.as_ref().map(|p| {
            format!(
                "You are {} in an AI agent channel. Respond concisely and stay in character.\n\n{}",
                p.name, p.system_prompt
            )
        });

        let stream_setup: Option<(Arc<dyn AiProvider>, ChatRequest)> =
            if cx.has_global::<AppAiService>() {
                cx.global::<AppAiService>()
                    .0
                    .prepare_stream(context_messages.clone(), &model, system_prompt, None)
            } else {
                None
            };

        let Some((provider, request)) = stream_setup else {
            warn!("Channels: no provider available for agent '{agent_name}'");
            continue;
        };

        let channels_view = workspace.channels_view.downgrade();
        let channel_id_clone = channel_id.clone();
        let agent_name_clone = agent_name.clone();
        let model_clone = model.clone();

        cx.spawn(async move |_this, app: &mut AsyncApp| {
            match provider.stream_chat(&request).await {
                Ok(mut rx) => {
                    let mut accumulated = String::new();
                    while let Some(chunk) = rx.recv().await {
                        accumulated.push_str(&chunk.content);

                        let content = accumulated.clone();
                        let agent = agent_name_clone.clone();
                        let _ = channels_view.update(app, |view, cx| {
                            view.set_streaming(&agent, &content, cx);
                        });

                        if chunk.done {
                            break;
                        }
                    }

                    let final_content = accumulated.clone();
                    let agent = agent_name_clone.clone();
                    let ch_id = channel_id_clone.clone();
                    let model_str = model_clone.clone();

                    let _ = app.update(|cx| {
                        if cx.has_global::<AppChannels>() {
                            let msg = hive_core::channels::ChannelMessage {
                                id: uuid::Uuid::new_v4().to_string(),
                                author: hive_core::channels::MessageAuthor::Agent {
                                    persona: agent.clone(),
                                },
                                content: final_content.clone(),
                                timestamp: Utc::now(),
                                thread_id: None,
                                model: Some(model_str),
                                cost: None,
                            };
                            cx.global_mut::<AppChannels>().0.add_message(&ch_id, msg.clone());

                            let _ = channels_view.update(cx, |view, cx| {
                                view.finish_streaming(cx);
                                view.append_message(&msg, cx);
                            });
                        }
                    });
                }
                Err(e) => {
                    error!("Channels: stream error for agent '{}': {e}", agent_name_clone);
                    let _ = channels_view.update(app, |view, cx| {
                        view.finish_streaming(cx);
                    });
                }
            }
        })
        .detach();
    }
}
