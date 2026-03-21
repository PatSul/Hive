use chrono::Utc;
use gpui::*;
use tracing::{info, warn};

use super::{
    AgentsDiscoverRemoteAgent, AgentsRefreshRemoteAgents, AgentsReloadWorkflows,
    AgentsRunRemoteAgent, AgentsRunWorkflow, AgentsSelectRemoteAgent, AgentsSelectRemoteSkill,
    AppA2aClient, AppAutomation, AppNotification, AppNotifications, HiveWorkspace,
    NotificationType, workflow_actions,
};

pub(super) fn refresh_agents_data(workspace: &mut HiveWorkspace, cx: &App) {
    use hive_ui_panels::panels::agents::{
        PersonaDisplay, RemoteAgentDisplay, RunDisplay, WorkflowDisplay,
    };

    if cx.has_global::<super::AppPersonas>() {
        let registry = &cx.global::<super::AppPersonas>().0;
        workspace.agents_data.personas = registry
            .all()
            .into_iter()
            .map(|persona| PersonaDisplay {
                name: persona.name.clone(),
                kind: format!("{:?}", persona.kind),
                description: persona.description.clone(),
                model_tier: format!("{:?}", persona.model_tier),
                active: false,
            })
            .collect();
    }

    workspace.agents_data.remote_agents.clear();
    if cx.has_global::<AppA2aClient>() {
        let client = &cx.global::<AppA2aClient>().0;
        if let Err(e) = client.reload() {
            warn!("Agents: failed to reload A2A config: {e}");
        }
        match client.list_agents() {
            Ok(remote_agents) => {
                workspace.agents_data.remote_hint = Some(format!(
                    "{} configured remote agent(s) from {}",
                    remote_agents.len(),
                    client.config_path().display()
                ));
                workspace.agents_data.remote_agents = remote_agents
                    .into_iter()
                    .map(|agent| {
                        let description = agent
                            .description
                            .unwrap_or_else(|| format!("Remote A2A agent at {}", agent.url));
                        RemoteAgentDisplay {
                            name: agent.name,
                            url: agent.url,
                            description,
                            discovered: agent.discovered,
                            api_key_configured: agent.api_key_configured,
                            version: agent.version,
                            skills: agent.skills,
                        }
                    })
                    .collect();
            }
            Err(e) => {
                workspace.agents_data.remote_hint = Some("Remote A2A config unavailable".into());
                warn!("Agents: failed to list A2A agents: {e}");
            }
        }
    } else {
        workspace.agents_data.remote_hint = Some("A2A client unavailable".into());
    }

    if workspace.agents_data.remote_agents.is_empty() {
        workspace.agents_data.selected_remote_agent = None;
        workspace.agents_data.selected_remote_skill = None;
    } else {
        let selected_is_valid = workspace
            .agents_data
            .selected_remote_agent
            .as_ref()
            .is_some_and(|selected| {
                workspace
                    .agents_data
                    .remote_agents
                    .iter()
                    .any(|agent| agent.name == *selected)
            });
        if !selected_is_valid {
            workspace.agents_data.selected_remote_agent = workspace
                .agents_data
                .remote_agents
                .first()
                .map(|agent| agent.name.clone());
        }
        if let Some(selected_agent) = workspace.agents_data.selected_remote_agent.as_ref()
            && let Some(agent) = workspace
                .agents_data
                .remote_agents
                .iter()
                .find(|agent| agent.name == *selected_agent)
        {
            let skill_is_valid = workspace
                .agents_data
                .selected_remote_skill
                .as_ref()
                .is_none_or(|skill| agent.skills.iter().any(|candidate| candidate == skill));
            if !skill_is_valid {
                workspace.agents_data.selected_remote_skill = None;
            }
        }
        workspace.agents_data.personas.extend(
            workspace
                .agents_data
                .remote_agents
                .iter()
                .map(|agent| PersonaDisplay {
                    name: agent.name.clone(),
                    kind: "remote_a2a".into(),
                    description: agent.description.clone(),
                    model_tier: "Remote".into(),
                    active: true,
                }),
        );
    }

    if cx.has_global::<AppAutomation>() {
        let automation = &cx.global::<AppAutomation>().0;

        workspace.agents_data.workflows = automation
            .list_workflows()
            .iter()
            .map(|workflow| WorkflowDisplay {
                id: workflow.id.clone(),
                name: workflow.name.clone(),
                description: workflow.description.clone(),
                commands: workflow_command_preview(workflow),
                source: if workflow.id.starts_with("builtin:") {
                    "Built-in".into()
                } else if workflow.id.starts_with("file:") {
                    "User file".into()
                } else {
                    "Runtime".into()
                },
                status: format!("{:?}", workflow.status),
                trigger: trigger_label(&workflow.trigger),
                steps: workflow.steps.len(),
                run_count: workflow.run_count as usize,
                last_run: workflow
                    .last_run
                    .as_ref()
                    .map(|timestamp: &chrono::DateTime<chrono::Utc>| {
                        timestamp.format("%Y-%m-%d %H:%M").to_string()
                    }),
            })
            .collect();

        workspace.agents_data.active_runs = automation
            .list_workflows()
            .iter()
            .filter(|workflow| {
                matches!(
                    workflow.status,
                    hive_agents::automation::WorkflowStatus::Active
                        | hive_agents::automation::WorkflowStatus::Draft
                )
            })
            .map(|workflow| RunDisplay {
                id: workflow.id.clone(),
                spec_title: workflow.name.clone(),
                status: format!("{:?}", workflow.status),
                progress: if workflow.steps.is_empty() { 0.0 } else { 1.0 },
                tasks_done: workflow.steps.len(),
                tasks_total: workflow.steps.len(),
                cost: 0.0,
                elapsed: workflow
                    .last_run
                    .as_ref()
                    .map(|_| "recent".to_string())
                    .unwrap_or_else(|| "-".to_string()),
                tasks: vec![],
                disclosure: Default::default(),
            })
            .collect();

        workspace.agents_data.run_history = automation
            .list_run_history()
            .iter()
            .rev()
            .take(8)
            .filter_map(|run| {
                let workflow = automation.get_workflow(&run.workflow_id)?;
                Some(RunDisplay {
                    id: run.workflow_id.clone(),
                    spec_title: workflow.name.clone(),
                    status: if run.success {
                        "Complete".into()
                    } else {
                        "Failed".into()
                    },
                    progress: if run.success { 1.0 } else { 0.0 },
                    tasks_done: run.steps_completed,
                    tasks_total: workflow.steps.len(),
                    cost: 0.0,
                    elapsed: format!(
                        "{}s",
                        (run.completed_at - run.started_at).num_seconds().max(0)
                    ),
                    tasks: vec![],
                    disclosure: Default::default(),
                })
            })
            .collect();

        workspace.agents_data.workflow_source_dir = hive_agents::USER_WORKFLOW_DIR.to_string();
        workspace.agents_data.workflow_hint = Some(format!(
            "{} workflows loaded ({} active)",
            automation.workflow_count(),
            automation.active_count()
        ));
    }
}

fn workflow_command_preview(workflow: &hive_agents::automation::Workflow) -> Vec<String> {
    workflow
        .steps
        .iter()
        .filter_map(|step| match &step.action {
            hive_agents::automation::ActionType::RunCommand { command } => {
                Some(command.to_string())
            }
            _ => None,
        })
        .collect()
}

fn trigger_label(trigger: &hive_agents::automation::TriggerType) -> String {
    match trigger {
        hive_agents::automation::TriggerType::ManualTrigger => "Manual".into(),
        hive_agents::automation::TriggerType::Schedule { cron } => format!("Schedule ({cron})"),
        hive_agents::automation::TriggerType::FileChange { path } => {
            format!("File Change ({path})")
        }
        hive_agents::automation::TriggerType::WebhookReceived { event } => {
            format!("Webhook ({event})")
        }
        hive_agents::automation::TriggerType::OnMessage { pattern } => {
            format!("Message ({pattern})")
        }
        hive_agents::automation::TriggerType::OnError { source } => {
            format!("Error ({source})")
        }
    }
}

pub(super) fn handle_agents_refresh_remote_agents(
    workspace: &mut HiveWorkspace,
    _action: &AgentsRefreshRemoteAgents,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    if !cx.has_global::<AppA2aClient>() {
        workspace.push_notification(
            cx,
            NotificationType::Error,
            "Remote Agents",
            "A2A client is not available",
        );
        return;
    }

    match cx.global::<AppA2aClient>().0.reload() {
        Ok(()) => {
            refresh_agents_data(workspace, cx);
            workspace.agents_data.remote_status = Some("Reloaded remote A2A agent config".into());
            workspace.push_notification(
                cx,
                NotificationType::Success,
                "Remote Agents",
                "Reloaded ~/.hive/a2a.toml",
            );
            cx.notify();
        }
        Err(e) => {
            workspace.agents_data.remote_status = Some(format!("Failed to reload config: {e}"));
            workspace.push_notification(
                cx,
                NotificationType::Error,
                "Remote Agents",
                format!("Failed to reload A2A config: {e}"),
            );
            cx.notify();
        }
    }
}

pub(super) fn handle_agents_reload_workflows(
    workspace: &mut HiveWorkspace,
    _action: &AgentsReloadWorkflows,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    if !cx.has_global::<AppAutomation>() {
        return;
    }

    let workspace_root = workspace.current_project_root.clone();
    let report = {
        let automation = &mut cx.global_mut::<AppAutomation>().0;
        automation.ensure_builtin_workflows();
        automation.reload_user_workflows(&workspace_root)
    };

    info!(
        "Agents: reloaded workflows (loaded={}, failed={}, skipped={})",
        report.loaded, report.failed, report.skipped
    );

    if cx.has_global::<AppNotifications>() {
        let msg = format!(
            "Reloaded workflows: {} loaded, {} failed, {} skipped",
            report.loaded, report.failed, report.skipped
        );
        let notif_type = if report.failed > 0 {
            NotificationType::Warning
        } else {
            NotificationType::Success
        };
        cx.global_mut::<AppNotifications>()
            .0
            .push(AppNotification::new(notif_type, msg).with_title("Workflow Reload"));
    }

    for error in report.errors {
        warn!("Workflow load error: {error}");
    }

    refresh_agents_data(workspace, cx);
    workflow_actions::refresh_workflow_builder(workspace, cx);
    cx.notify();
}

pub(super) fn handle_agents_select_remote_agent(
    workspace: &mut HiveWorkspace,
    action: &AgentsSelectRemoteAgent,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    workspace.agents_data.selected_remote_agent = Some(action.agent_name.clone());
    if let Some(agent) = workspace
        .agents_data
        .remote_agents
        .iter()
        .find(|agent| agent.name == action.agent_name)
    {
        let skill_is_valid = workspace
            .agents_data
            .selected_remote_skill
            .as_ref()
            .is_none_or(|skill| agent.skills.iter().any(|candidate| candidate == skill));
        if !skill_is_valid {
            workspace.agents_data.selected_remote_skill = None;
        }
    }
    workspace.agents_data.remote_status =
        Some(format!("Selected remote agent '{}'", action.agent_name));
    cx.notify();
}

pub(super) fn handle_agents_select_remote_skill(
    workspace: &mut HiveWorkspace,
    action: &AgentsSelectRemoteSkill,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    workspace.agents_data.selected_remote_agent = Some(action.agent_name.clone());
    workspace.agents_data.selected_remote_skill = action.skill_id.clone();
    workspace.agents_data.remote_status = Some(if let Some(skill_id) = action.skill_id.as_ref() {
        format!("Pinned remote skill '{skill_id}'")
    } else {
        "Remote skill selection reset to auto".into()
    });
    cx.notify();
}

pub(super) fn handle_agents_discover_remote_agent(
    workspace: &mut HiveWorkspace,
    action: &AgentsDiscoverRemoteAgent,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    if !cx.has_global::<AppA2aClient>() {
        workspace.push_notification(
            cx,
            NotificationType::Error,
            "Remote Agents",
            "A2A client is not available",
        );
        return;
    }

    workspace.agents_data.remote_busy = true;
    workspace.agents_data.remote_status = Some(format!(
        "Discovering remote agent '{}'...",
        action.agent_name
    ));
    cx.notify();

    let client = cx.global::<AppA2aClient>().0.clone();
    let agent_name = action.agent_name.clone();
    let (tx, rx) = tokio::sync::oneshot::channel();

    std::thread::spawn(move || {
        let result = match tokio::runtime::Runtime::new() {
            Ok(runtime) => runtime
                .block_on(client.discover_agent(&agent_name))
                .map_err(|e| e.to_string()),
            Err(e) => Err(format!("tokio runtime: {e}")),
        };
        let _ = tx.send(result);
    });

    cx.spawn(async move |this, app: &mut AsyncApp| {
        let result = rx.await.unwrap_or(Err("channel closed".into()));
        let _ = this.update(app, |workspace, cx| {
            workspace.agents_data.remote_busy = false;
            match result {
                Ok(summary) => {
                    let skill_count = summary.skills.len();
                    let agent_name = summary.name.clone();
                    refresh_agents_data(workspace, cx);
                    workspace.agents_data.selected_remote_agent = Some(agent_name.clone());
                    workspace.agents_data.selected_remote_skill = None;
                    workspace.agents_data.remote_status = Some(format!(
                        "Discovered '{}' ({} skill{})",
                        agent_name,
                        skill_count,
                        if skill_count == 1 { "" } else { "s" }
                    ));
                    workspace.push_notification(
                        cx,
                        NotificationType::Success,
                        "Remote Agents",
                        format!("Discovered remote agent '{}'", agent_name),
                    );
                }
                Err(e) => {
                    workspace.agents_data.remote_status =
                        Some(format!("Remote discovery failed: {e}"));
                    workspace.push_notification(
                        cx,
                        NotificationType::Error,
                        "Remote Agents",
                        format!("Failed to discover remote agent: {e}"),
                    );
                }
            }
            cx.notify();
        });
    })
    .detach();
}

pub(super) fn handle_agents_run_remote_agent(
    workspace: &mut HiveWorkspace,
    action: &AgentsRunRemoteAgent,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    let prompt = action.prompt.trim();
    if prompt.is_empty() {
        workspace.push_notification(
            cx,
            NotificationType::Warning,
            "Remote Agents",
            "Enter a prompt before running a remote task",
        );
        return;
    }
    if !cx.has_global::<AppA2aClient>() {
        workspace.push_notification(
            cx,
            NotificationType::Error,
            "Remote Agents",
            "A2A client is not available",
        );
        return;
    }

    workspace.agents_data.remote_busy = true;
    workspace.agents_data.remote_status =
        Some(format!("Running remote task on '{}'...", action.agent_name));
    cx.notify();

    let client = cx.global::<AppA2aClient>().0.clone();
    let agent_name = action.agent_name.clone();
    let prompt = prompt.to_string();
    let skill_id = action.skill_id.clone();
    let agent_name_for_error = agent_name.clone();
    let skill_id_for_error = skill_id.clone();
    let (tx, rx) = tokio::sync::oneshot::channel();

    std::thread::spawn(move || {
        let result = match tokio::runtime::Runtime::new() {
            Ok(runtime) => runtime
                .block_on(client.run_task(&agent_name, &prompt, skill_id.as_deref()))
                .map_err(|e| e.to_string()),
            Err(e) => Err(format!("tokio runtime: {e}")),
        };
        let _ = tx.send(result);
    });

    cx.spawn(async move |this, app: &mut AsyncApp| {
        let result = rx.await.unwrap_or(Err("channel closed".into()));
        let _ = this.update(app, |workspace, cx| {
            workspace.agents_data.remote_busy = false;
            match result {
                Ok(run) => {
                    workspace.agents_data.remote_status = Some(format!(
                        "Remote task '{}' completed on '{}'",
                        run.task_id, run.agent_name
                    ));
                    workspace.agents_data.remote_run_history.insert(
                        0,
                        hive_ui_panels::panels::agents::RemoteTaskDisplay {
                            agent_name: run.agent_name.clone(),
                            task_id: run.task_id.clone(),
                            state: run.state.clone(),
                            skill_id: run.skill_id.clone(),
                            output: run.output.clone(),
                            completed_at: Utc::now().format("%Y-%m-%d %H:%M").to_string(),
                            error: None,
                        },
                    );
                    workspace.agents_data.remote_run_history.truncate(8);
                    refresh_agents_data(workspace, cx);
                    workspace.agents_data.selected_remote_agent = Some(run.agent_name.clone());
                    workspace.push_notification(
                        cx,
                        NotificationType::Success,
                        "Remote Agents",
                        format!(
                            "Remote task '{}' completed on '{}'",
                            run.task_id, run.agent_name
                        ),
                    );
                }
                Err(e) => {
                    workspace.agents_data.remote_status = Some(format!("Remote task failed: {e}"));
                    workspace.agents_data.remote_run_history.insert(
                        0,
                        hive_ui_panels::panels::agents::RemoteTaskDisplay {
                            agent_name: agent_name_for_error.clone(),
                            task_id: "error".into(),
                            state: "Failed".into(),
                            skill_id: skill_id_for_error.clone(),
                            output: String::new(),
                            completed_at: Utc::now().format("%Y-%m-%d %H:%M").to_string(),
                            error: Some(e.clone()),
                        },
                    );
                    workspace.agents_data.remote_run_history.truncate(8);
                    workspace.push_notification(
                        cx,
                        NotificationType::Error,
                        "Remote Agents",
                        format!("Remote task failed: {e}"),
                    );
                }
            }
            cx.notify();
        });
    })
    .detach();
}

pub(super) fn handle_agents_run_workflow(
    workspace: &mut HiveWorkspace,
    action: &AgentsRunWorkflow,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    if !cx.has_global::<AppAutomation>() {
        return;
    }

    let Some(workflow) = workspace.make_workflow_for_run(action, cx) else {
        return;
    };

    if cx.has_global::<AppNotifications>() {
        cx.global_mut::<AppNotifications>()
            .0
            .push(AppNotification::new(
                NotificationType::Info,
                format!(
                    "Running workflow '{}' ({} step(s)) from {} in {}",
                    workflow.id,
                    workflow.steps.len(),
                    if action.source.is_empty() {
                        "manual trigger"
                    } else {
                        action.source.as_str()
                    },
                    workspace.current_project_root.display()
                ),
            ));
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
        *run_result_for_thread.lock().unwrap_or_else(|e| e.into_inner()) = Some(result);
    });

    let run_result_for_ui = std::sync::Arc::clone(&run_result);
    let workflow_id_for_ui = workflow.id.clone();

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
                            let _ = cx.global_mut::<AppAutomation>().0.record_run(
                                &run.workflow_id,
                                run.success,
                                run.steps_completed,
                                run.error.clone(),
                            );

                            if cx.has_global::<AppNotifications>() {
                                let notification_type = if run.success {
                                    NotificationType::Success
                                } else {
                                    NotificationType::Error
                                };
                                let message = if run.success {
                                    format!(
                                        "Workflow '{}' completed ({} steps)",
                                        run.workflow_id, run.steps_completed
                                    )
                                } else {
                                    format!(
                                        "Workflow '{}' failed after {} step(s)",
                                        run.workflow_id, run.steps_completed
                                    )
                                };
                                cx.global_mut::<AppNotifications>().0.push(
                                    AppNotification::new(notification_type, message).with_title(
                                        if run.success {
                                            "Workflow Complete"
                                        } else {
                                            "Workflow Failed"
                                        },
                                    ),
                                );
                            }
                        }
                        Err(e) => {
                            warn!("Agents: workflow run error ({workflow_id_for_ui}): {e}");
                            if cx.has_global::<AppNotifications>() {
                                cx.global_mut::<AppNotifications>().0.push(
                                    AppNotification::new(
                                        NotificationType::Error,
                                        format!("Workflow '{workflow_id_for_ui}' failed: {e}"),
                                    )
                                    .with_title("Workflow Run Failed"),
                                );
                            }
                        }
                    }

                    refresh_agents_data(workspace, cx);
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
