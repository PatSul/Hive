use std::path::{Path, PathBuf};

use gpui::*;

use crate::chat_service::{ChatService, MessageRole};
use super::{
    chat_actions, data_refresh, navigation, project_context, AppA2aClient, AppAiService,
    AppApprovalGate, AppKnowledgeFiles, AppQuickIndex, HiveWorkspace, NotificationType, Panel,
    QuickStartOpenPanel, QuickStartRunProject, QuickStartSelectTemplate, QuickStartTone,
    ReviewData, SpecPanelData,
};
use hive_ui_panels::panels::quick_start::{
    QuickStartNextStepDisplay, QuickStartPanelData, QuickStartPriorityDisplay,
    QuickStartSetupDisplay, QuickStartStatusDisplay, QuickStartTemplateDisplay,
    QuickStartWorkspaceDisplay,
};

pub(super) fn handle_quick_start_select_template(
    workspace: &mut HiveWorkspace,
    action: &QuickStartSelectTemplate,
    window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    workspace.quick_start_data.selected_template = action.template_id.clone();
    workspace.quick_start_goal_input.update(cx, |input, cx| {
        input.set_placeholder(
            quick_start_template_placeholder(&action.template_id),
            window,
            cx,
        );
    });
    workspace.quick_start_data.last_launch_status = Some(format!(
        "Selected '{}' as the current Home mission.",
        quick_start_template_title(&action.template_id)
    ));
    refresh_quick_start_data(workspace, cx);
    cx.notify();
}

pub(super) fn handle_quick_start_open_panel(
    workspace: &mut HiveWorkspace,
    action: &QuickStartOpenPanel,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    navigation::switch_to_panel(workspace, Panel::from_stored(&action.panel), cx);
}

pub(super) fn handle_quick_start_run_project(
    workspace: &mut HiveWorkspace,
    action: &QuickStartRunProject,
    window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    if workspace.chat_service.read(cx).is_streaming() {
        workspace.push_notification(
            cx,
            NotificationType::Warning,
            "Home",
            "Wait for the current chat run to finish before starting another guided run.",
        );
        return;
    }

    if !workspace.quick_start_data.launch_ready {
        let message = if workspace
            .quick_start_data
            .setup
            .get(1)
            .is_some_and(|item| item.tone != QuickStartTone::Ready)
        {
            "Connect a cloud or local model runtime in Settings before launching Home."
        } else {
            "Choose a default model in Settings before launching Home."
        };

        workspace.quick_start_data.last_launch_status = Some(message.into());
        workspace.push_notification(cx, NotificationType::Warning, "Home", message);
        navigation::switch_to_panel(workspace, Panel::Settings, cx);
        return;
    }

    let prompt = build_quick_start_prompt(workspace, &action.template_id, &action.detail);
    let template_title = quick_start_template_title(&action.template_id);
    workspace.quick_start_data.last_launch_status = Some(format!(
        "Started '{}' for {}.",
        template_title, workspace.current_project_name
    ));
    workspace.quick_start_goal_input.update(cx, |input, cx| {
        input.set_value(String::new(), window, cx);
    });

    workspace.chat_service.update(cx, |svc, _cx| {
        svc.new_conversation();
    });
    workspace.cached_chat_data.markdown_cache.clear();
    data_refresh::refresh_history(workspace);
    navigation::switch_to_panel(workspace, Panel::Chat, cx);
    chat_actions::handle_send_text(workspace, prompt, Vec::new(), window, cx);
}

pub(super) fn refresh_quick_start_data(
    workspace: &mut HiveWorkspace,
    cx: &mut Context<HiveWorkspace>,
) {
    data_refresh::refresh_specs_data(workspace, cx);
    let selected_template = workspace.quick_start_data.selected_template.clone();
    let last_launch_status = workspace.quick_start_data.last_launch_status.clone();
    workspace.quick_start_data = build_quick_start_data(
        &workspace.current_project_root,
        &workspace.current_project_name,
        &workspace.recent_workspace_roots,
        &workspace.pinned_workspace_roots,
        &workspace.chat_service,
        &workspace.specs_data,
        &selected_template,
        last_launch_status,
        cx,
    );
}

pub(super) fn build_quick_start_data(
    project_root: &Path,
    project_name: &str,
    recent_workspace_roots: &[PathBuf],
    pinned_workspace_roots: &[PathBuf],
    chat_service: &Entity<ChatService>,
    specs_data: &SpecPanelData,
    selected_template: &str,
    last_launch_status: Option<String>,
    cx: &App,
) -> QuickStartPanelData {
    let templates = quick_start_templates();
    let selected_template = templates
        .iter()
        .find(|template| template.id == selected_template)
        .map(|template| template.id.clone())
        .unwrap_or_else(|| "dogfood".into());

    let current_model = chat_service.read(cx).current_model().to_string();
    let has_selected_model = !current_model.trim().is_empty();
    let pending_approvals = cx
        .has_global::<AppApprovalGate>()
        .then(|| cx.global::<AppApprovalGate>().0.pending_count())
        .unwrap_or(0);
    let git_state = ReviewData::from_git(project_root);
    let active_spec = specs_data.active_spec().cloned();
    let (is_streaming, message_count, latest_user_request) = {
        let svc = chat_service.read(cx);
        let latest_request = svc
            .messages()
            .iter()
            .rev()
            .find(|message| message.role == MessageRole::User)
            .map(|message| text_excerpt(&message.content, 110));
        (svc.is_streaming(), svc.messages().len(), latest_request)
    };

    let (has_cloud_runtime, has_local_runtime) = if cx.has_global::<AppAiService>() {
        let ai = &cx.global::<AppAiService>().0;
        let has_cloud = ai.available_providers().iter().any(|provider| {
            matches!(
                provider,
                hive_ai::types::ProviderType::Anthropic
                    | hive_ai::types::ProviderType::OpenAI
                    | hive_ai::types::ProviderType::OpenRouter
                    | hive_ai::types::ProviderType::Google
                    | hive_ai::types::ProviderType::Groq
                    | hive_ai::types::ProviderType::HuggingFace
                    | hive_ai::types::ProviderType::XAI
                    | hive_ai::types::ProviderType::Mistral
                    | hive_ai::types::ProviderType::Venice
            )
        });
        let has_local = ai
            .discovery()
            .map(|discovery| discovery.snapshot().any_online())
            .unwrap_or(false);
        (has_cloud, has_local)
    } else {
        (false, false)
    };
    let has_ai_runtime = has_cloud_runtime || has_local_runtime;

    let knowledge_files = if cx.has_global::<AppKnowledgeFiles>() {
        cx.global::<AppKnowledgeFiles>().0.len()
    } else {
        0
    };

    let remote_agents = if cx.has_global::<AppA2aClient>() {
        cx.global::<AppA2aClient>()
            .0
            .list_agents()
            .map(|agents| agents.len())
            .unwrap_or(0)
    } else {
        0
    };

    let (project_summary, total_files, key_symbols, dependencies) =
        if cx.has_global::<AppQuickIndex>() {
            let quick_index = cx.global::<AppQuickIndex>().0.clone();
            let summary = if quick_index.file_tree.summary.trim().is_empty() {
                format!("Using {} as the active project root.", project_root.display())
            } else {
                quick_index.file_tree.summary.clone()
            };
            (
                summary,
                quick_index.file_tree.total_files,
                quick_index.key_symbols.len(),
                quick_index.dependencies.len(),
            )
        } else {
            (
                format!("Using {} as the active project root.", project_root.display()),
                0,
                0,
                0,
            )
        };

    let launch_ready = has_ai_runtime && has_selected_model;
    let launch_hint = if !has_ai_runtime {
        "Connect at least one cloud or local model runtime in Settings before starting a guided run."
            .into()
    } else if !has_selected_model {
        "Choose a default model in Settings so Home knows where to launch the project run."
            .into()
    } else {
        format!(
            "Ready to launch a fresh project run in Chat with {}.",
            current_model
        )
    };

    let setup = vec![
        QuickStartSetupDisplay {
            title: "Project context".into(),
            detail: format!(
                "Workspace root: {}. Knowledge files loaded: {}.",
                project_root.display(),
                knowledge_files
            ),
            status_label: "Ready".into(),
            tone: QuickStartTone::Ready,
            action_label: Some("Open Files".into()),
            action_panel: Some(Panel::Files.to_stored().into()),
        },
        QuickStartSetupDisplay {
            title: "AI runtime".into(),
            detail: if has_ai_runtime {
                format!(
                    "Cloud runtime: {}. Local runtime: {}.",
                    if has_cloud_runtime {
                        "connected"
                    } else {
                        "not connected"
                    },
                    if has_local_runtime {
                        "online"
                    } else {
                        "offline"
                    }
                )
            } else {
                "No cloud or local models are available yet.".into()
            },
            status_label: if has_ai_runtime {
                "Connected".into()
            } else {
                "Needs setup".into()
            },
            tone: if has_ai_runtime {
                QuickStartTone::Ready
            } else {
                QuickStartTone::Action
            },
            action_label: Some(if has_ai_runtime {
                "Review Settings".into()
            } else {
                "Connect Models".into()
            }),
            action_panel: Some(Panel::Settings.to_stored().into()),
        },
        QuickStartSetupDisplay {
            title: "Default model".into(),
            detail: if has_selected_model {
                format!("Current launch model: {}.", current_model)
            } else {
                "No default model is selected for Chat yet.".into()
            },
            status_label: if has_selected_model {
                "Selected".into()
            } else {
                "Choose one".into()
            },
            tone: if has_selected_model {
                QuickStartTone::Ready
            } else {
                QuickStartTone::Action
            },
            action_label: Some("Open Settings".into()),
            action_panel: Some(Panel::Settings.to_stored().into()),
        },
        QuickStartSetupDisplay {
            title: "Git and agent accelerators".into(),
            detail: format!(
                "Git repo: {}. Remote A2A agents configured: {}.",
                if project_root.join(".git").exists() {
                    "yes"
                } else {
                    "no"
                },
                remote_agents
            ),
            status_label: if remote_agents > 0 {
                "Optional boost".into()
            } else {
                "Optional".into()
            },
            tone: QuickStartTone::Optional,
            action_label: Some("Open Agents".into()),
            action_panel: Some(Panel::Agents.to_stored().into()),
        },
    ];

    let next_steps = vec![
        QuickStartNextStepDisplay {
            title: "Review".into(),
            detail: "Inspect git status, diffs, and release readiness after the kickoff run starts."
                .into(),
            panel: Panel::Review.to_stored().into(),
            action_label: "Open Git Ops".into(),
        },
        QuickStartNextStepDisplay {
            title: "Specs".into(),
            detail: "Turn the kickoff outcome into a crisp implementation plan when the work needs structure."
                .into(),
            panel: Panel::Specs.to_stored().into(),
            action_label: "Open Specs".into(),
        },
        QuickStartNextStepDisplay {
            title: "Agents".into(),
            detail: "Use workflows or remote A2A agents when parts of the job should be delegated."
                .into(),
            panel: Panel::Agents.to_stored().into(),
            action_label: "Open Agents".into(),
        },
        QuickStartNextStepDisplay {
            title: "Kanban".into(),
            detail: "Track execution once the first run identifies concrete follow-up tasks.".into(),
            panel: Panel::Kanban.to_stored().into(),
            action_label: "Open Kanban".into(),
        },
    ];

    let priorities = {
        let run_priority = if is_streaming {
            QuickStartPriorityDisplay {
                eyebrow: "Resume run".into(),
                title: "Chat is actively running".into(),
                detail: latest_user_request
                    .clone()
                    .map(|request| format!("Latest request: {request}"))
                    .unwrap_or_else(|| {
                        "Open Chat to watch the current run and steer the next step.".into()
                    }),
                action_label: "Open Chat".into(),
                action_panel: Panel::Chat.to_stored().into(),
                tone: QuickStartTone::Ready,
            }
        } else if message_count > 0 {
            QuickStartPriorityDisplay {
                eyebrow: "Resume context".into(),
                title: "Continue the latest conversation".into(),
                detail: latest_user_request
                    .clone()
                    .map(|request| format!("Last prompt: {request}"))
                    .unwrap_or_else(|| {
                        "Open Chat to continue the current project-scoped conversation.".into()
                    }),
                action_label: "Resume Chat".into(),
                action_panel: Panel::Chat.to_stored().into(),
                tone: QuickStartTone::Ready,
            }
        } else {
            QuickStartPriorityDisplay {
                eyebrow: "No active run".into(),
                title: "Start the next project run".into(),
                detail: "Choose a mission, describe the outcome, and let Home launch a clean Chat run."
                    .into(),
                action_label: "Open Launch".into(),
                action_panel: Panel::QuickStart.to_stored().into(),
                tone: if launch_ready {
                    QuickStartTone::Ready
                } else {
                    QuickStartTone::Action
                },
            }
        };

        let spec_priority = if let Some(spec) = active_spec.as_ref() {
            QuickStartPriorityDisplay {
                eyebrow: "Active spec".into(),
                title: spec.title.clone(),
                detail: format!(
                    "{} - {}/{} checked - updated {}",
                    spec.status, spec.entries_checked, spec.entries_total, spec.updated_at
                ),
                action_label: "Open Specs".into(),
                action_panel: Panel::Specs.to_stored().into(),
                tone: if spec.entries_checked < spec.entries_total {
                    QuickStartTone::Ready
                } else {
                    QuickStartTone::Optional
                },
            }
        } else {
            QuickStartPriorityDisplay {
                eyebrow: "Plan next".into(),
                title: "No active spec is pinned".into(),
                detail: format!(
                    "Turn '{}' into a tracked implementation plan when the work needs structure.",
                    quick_start_template_title(&selected_template)
                ),
                action_label: "Plan In Specs".into(),
                action_panel: Panel::Specs.to_stored().into(),
                tone: QuickStartTone::Optional,
            }
        };

        let observe_priority = if pending_approvals > 0 {
            QuickStartPriorityDisplay {
                eyebrow: "Blocked work".into(),
                title: format!("{pending_approvals} approvals are waiting"),
                detail:
                    "Observe should be the first stop before you continue another run or ship changes."
                        .into(),
                action_label: "Open Observe".into(),
                action_panel: Panel::Activity.to_stored().into(),
                tone: QuickStartTone::Action,
            }
        } else {
            QuickStartPriorityDisplay {
                eyebrow: "Observe".into(),
                title: "No approvals are blocking work".into(),
                detail: "Use Observe to inspect recent failures, spend, or safety signals when you need validation context."
                    .into(),
                action_label: "Open Observe".into(),
                action_panel: Panel::Activity.to_stored().into(),
                tone: QuickStartTone::Ready,
            }
        };

        let recommended_priority = if pending_approvals > 0 {
            QuickStartPriorityDisplay {
                eyebrow: "Recommended next".into(),
                title: "Clear the inbox before more execution".into(),
                detail: "Approvals are the highest-friction blockers, so resolve them before another run starts."
                    .into(),
                action_label: "Review Observe".into(),
                action_panel: Panel::Activity.to_stored().into(),
                tone: QuickStartTone::Action,
            }
        } else if git_state.is_repo
            && (git_state.modified_count > 0
                || git_state.staged_count > 0
                || git_state.untracked_count > 0)
        {
            QuickStartPriorityDisplay {
                eyebrow: "Recommended next".into(),
                title: "Review the working tree".into(),
                detail: format!(
                    "{} modified - {} staged - {} untracked on {}",
                    git_state.modified_count,
                    git_state.staged_count,
                    git_state.untracked_count,
                    git_state.branch
                ),
                action_label: "Open Git Ops".into(),
                action_panel: Panel::Review.to_stored().into(),
                tone: QuickStartTone::Ready,
            }
        } else if !launch_ready {
            QuickStartPriorityDisplay {
                eyebrow: "Recommended next".into(),
                title: "Finish model setup".into(),
                detail: launch_hint.clone(),
                action_label: "Open Settings".into(),
                action_panel: Panel::Settings.to_stored().into(),
                tone: QuickStartTone::Action,
            }
        } else if active_spec.is_some() {
            QuickStartPriorityDisplay {
                eyebrow: "Recommended next".into(),
                title: "Continue the planned implementation".into(),
                detail: "The active spec is already pinned, so the fastest next move is to execute it in Chat."
                    .into(),
                action_label: "Open Chat".into(),
                action_panel: Panel::Chat.to_stored().into(),
                tone: QuickStartTone::Ready,
            }
        } else {
            QuickStartPriorityDisplay {
                eyebrow: "Recommended next".into(),
                title: "Launch a fresh guided run".into(),
                detail:
                    "Home is ready to start the next mission in a clean project-scoped chat."
                        .into(),
                action_label: "Open Launch".into(),
                action_panel: Panel::QuickStart.to_stored().into(),
                tone: QuickStartTone::Ready,
            }
        };

        vec![
            run_priority,
            spec_priority,
            observe_priority,
            recommended_priority,
        ]
    };

    let mut saved_workspaces: Vec<QuickStartWorkspaceDisplay> = Vec::new();
    for path in pinned_workspace_roots {
        saved_workspaces.push(QuickStartWorkspaceDisplay {
            name: project_context::project_name_from_path(path),
            path: path.display().to_string(),
            is_current: path == project_root,
            is_pinned: true,
        });
    }
    for path in recent_workspace_roots {
        if pinned_workspace_roots.contains(path) {
            continue;
        }
        saved_workspaces.push(QuickStartWorkspaceDisplay {
            name: project_context::project_name_from_path(path),
            path: path.display().to_string(),
            is_current: path == project_root,
            is_pinned: false,
        });
    }
    saved_workspaces.truncate(6);

    let status_cards = vec![
        QuickStartStatusDisplay {
            title: "Observe inbox".into(),
            value: if pending_approvals > 0 {
                format!("{pending_approvals} pending approvals")
            } else {
                "No pending approvals".into()
            },
            detail: if pending_approvals > 0 {
                "Review blocked actions, approvals, and recent events before continuing the next run."
                    .into()
            } else {
                "Observe is currently quiet. Open it to inspect recent runs, costs, and safety signals."
                    .into()
            },
            tone: if pending_approvals > 0 {
                QuickStartTone::Action
            } else {
                QuickStartTone::Ready
            },
            action_label: Some("Open Observe".into()),
            action_panel: Some(Panel::Activity.to_stored().into()),
        },
        QuickStartStatusDisplay {
            title: "Model routing".into(),
            value: if has_selected_model {
                current_model.clone()
            } else {
                "No default model".into()
            },
            detail: if has_selected_model {
                "Home will use this model when launching a new guided run in Chat.".into()
            } else {
                "Choose a default model in Settings so Home can launch guided runs without another stop."
                    .into()
            },
            tone: if has_selected_model {
                QuickStartTone::Ready
            } else {
                QuickStartTone::Action
            },
            action_label: Some("Open Settings".into()),
            action_panel: Some(Panel::Settings.to_stored().into()),
        },
        QuickStartStatusDisplay {
            title: "Saved workspaces".into(),
            value: format!("{} available", saved_workspaces.len()),
            detail: format!(
                "{} pinned, {} recent, and ready to reopen from Home.",
                pinned_workspace_roots.len(),
                recent_workspace_roots.len()
            ),
            tone: if saved_workspaces.is_empty() {
                QuickStartTone::Optional
            } else {
                QuickStartTone::Ready
            },
            action_label: Some("Open Files".into()),
            action_panel: Some(Panel::Files.to_stored().into()),
        },
    ];

    QuickStartPanelData {
        project_name: project_name.into(),
        project_root: project_root.display().to_string(),
        project_summary,
        total_files,
        key_symbols,
        dependencies,
        selected_template,
        templates,
        status_cards,
        setup,
        next_steps,
        priorities,
        saved_workspaces,
        current_model,
        pending_approvals,
        launch_ready,
        launch_hint,
        last_launch_status,
    }
}

pub(super) fn text_excerpt(text: &str, max_chars: usize) -> String {
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.len() <= max_chars {
        return normalized;
    }

    let mut clipped = String::new();
    for ch in normalized.chars().take(max_chars.saturating_sub(1)) {
        clipped.push(ch);
    }
    clipped.push('…');
    clipped
}

pub(super) fn quick_start_templates() -> Vec<QuickStartTemplateDisplay> {
    vec![
        QuickStartTemplateDisplay {
            id: "dogfood".into(),
            title: "Improve This Codebase".into(),
            description: "Use Hive to find the highest-leverage gaps in the current project and start closing them."
                .into(),
            outcome: "Best when you want HiveCode to improve HiveCode itself.".into(),
        },
        QuickStartTemplateDisplay {
            id: "feature".into(),
            title: "Ship A Feature".into(),
            description: "Trace the relevant code, define the change, implement it, and verify the result."
                .into(),
            outcome: "Best when you already know the product outcome you want.".into(),
        },
        QuickStartTemplateDisplay {
            id: "bug".into(),
            title: "Fix A Bug".into(),
            description: "Reproduce the problem, isolate root cause, patch it, and confirm the regression is closed."
                .into(),
            outcome: "Best when the project is blocked by a failure or broken workflow.".into(),
        },
        QuickStartTemplateDisplay {
            id: "understand".into(),
            title: "Understand The Project".into(),
            description: "Map the architecture, explain how the pieces fit, and identify the real risks."
                .into(),
            outcome: "Best when a human needs a clear read on the codebase before deciding.".into(),
        },
        QuickStartTemplateDisplay {
            id: "review".into(),
            title: "Review Current State".into(),
            description: "Inspect git state and the working tree, then call out problems, regressions, and next actions."
                .into(),
            outcome: "Best when you want an informed starting point before more coding.".into(),
        },
    ]
}

pub(super) fn quick_start_template_title(template_id: &str) -> &'static str {
    match template_id {
        "feature" => "Ship A Feature",
        "bug" => "Fix A Bug",
        "understand" => "Understand The Project",
        "review" => "Review Current State",
        _ => "Improve This Codebase",
    }
}

pub(super) fn quick_start_template_placeholder(template_id: &str) -> &'static str {
    match template_id {
        "feature" => "Describe the feature outcome, user flow, or missing integration to ship",
        "bug" => "Describe the failure, broken behavior, or user-facing bug to fix",
        "understand" => "Describe the architecture, workflow, or module you want Hive to explain",
        "review" => {
            "Describe what you want reviewed, for example the current diff, release readiness, or regressions"
        }
        _ => "Describe what Hive should improve, complete, or tighten in this project",
    }
}

pub(super) fn quick_start_template_instruction(template_id: &str) -> &'static str {
    match template_id {
        "feature" => {
            "Trace the feature area, identify the files and interfaces involved, implement the change end-to-end, and verify the result with the right checks."
        }
        "bug" => {
            "Reproduce the failure from repo context, isolate the root cause, implement a precise fix, and verify that the bug is actually closed."
        }
        "understand" => {
            "Build a practical map of the codebase, call out the major modules and dependencies, identify incomplete or risky seams, and recommend the next high-impact work."
        }
        "review" => {
            "Start with repository state and current changes, identify the most important risks or regressions, and recommend the next concrete actions to move the project forward."
        }
        _ => {
            "Treat this as a dogfooding and completion run: find the highest-impact gaps in the product, prioritize what will make the app more integrated and more usable, and begin closing those gaps."
        }
    }
}

fn build_quick_start_prompt(
    workspace: &HiveWorkspace,
    template_id: &str,
    detail: &str,
) -> String {
    let mission = quick_start_template_instruction(template_id);
    let user_focus = if detail.trim().is_empty() {
        "Use your judgment from the current repository state and start with the highest-impact opportunity.".to_string()
    } else {
        let trimmed = detail.trim();
        if trimmed.len() > 500 {
            trimmed.chars().take(500).collect()
        } else {
            trimmed.to_string()
        }
    };

    format!(
        "You are kicking off work on the active project.\n\nProject: {}\nWorkspace root: {}\nMission: {}\nSpecific focus: {}\n\nExecution rules:\n1. Inspect the codebase, README or HIVE docs, and current git state before changing code.\n2. Summarize the relevant context briefly.\n3. Produce a concise impact-ordered execution plan.\n4. Start the first concrete task immediately instead of stopping at analysis.\n5. Keep changes integrated with the existing modules, tabs, and shared services.\n6. Use Review, Specs, Agents, and Kanban when they are the right handoff surfaces.\n\nMission details:\n{}",
        workspace.current_project_name,
        workspace.current_project_root.display(),
        quick_start_template_title(template_id),
        user_focus,
        mission,
    )
}
