use crate::daemon::{AgentDisposition, HiveDaemon, PendingAction, SendDisposition};
use crate::protocol::{DaemonEvent, ObserveView, PanelResponse, SessionSnapshot, ShellDestination};
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json};
use futures::SinkExt;
use futures::stream::StreamExt;
use hive_terminal::ShellOutput;
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{Duration, sleep};

/// Shared daemon state wrapped for concurrent access by axum handlers.
pub type DaemonState = Arc<RwLock<HiveDaemon>>;

#[derive(Debug, Deserialize)]
pub struct ChatRequest {
    pub conversation_id: Option<String>,
    pub content: String,
    pub model: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct NavigationRequest {
    pub destination: Option<ShellDestination>,
    pub panel: Option<String>,
    pub model: Option<String>,
    pub observe_view: Option<ObserveView>,
}

#[derive(Debug, Deserialize)]
pub struct HomeLaunchRequest {
    pub template_id: String,
    #[serde(default)]
    pub detail: String,
}

#[derive(Debug, Deserialize)]
pub struct WorkspaceSwitchRequest {
    pub workspace_path: String,
}

#[derive(Debug, Deserialize)]
pub struct ResumeConversationRequest {
    pub conversation_id: String,
}

#[derive(Debug, Deserialize)]
pub struct ApprovalDecisionRequest {
    pub approved: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AgentRequest {
    pub goal: String,
    pub orchestration_mode: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct FilePathRequest {
    pub path: String,
}

#[derive(Debug, Deserialize)]
pub struct GitCommitRequest {
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct TerminalInputRequest {
    pub input: String,
}

#[derive(Debug, Deserialize)]
pub struct WorkflowRunRequest {
    pub workflow_id: String,
}

#[derive(Debug, Deserialize)]
pub struct ChannelSelectRequest {
    pub channel_id: String,
}

#[derive(Debug, Deserialize)]
pub struct ChannelMessageRequest {
    pub channel_id: String,
    pub content: String,
}

#[derive(Debug, Deserialize)]
pub struct AssistantDecisionRequest {
    pub approved: bool,
}

#[derive(Debug, Deserialize)]
pub struct SettingsUpdateRequest {
    pub setting: String,
    pub value: bool,
}

#[derive(Debug, Deserialize)]
pub struct SettingsTextRequest {
    pub setting: String,
    pub value: String,
}

#[derive(Debug, Deserialize)]
pub struct DefaultModelRequest {
    pub model: String,
}

#[derive(Debug, Deserialize)]
pub struct RoutingUpdateRequest {
    pub enabled: bool,
}

#[derive(Debug, Deserialize)]
pub struct ProviderKeyRequest {
    pub provider: String,
    pub key: String,
}

#[derive(Debug, Deserialize)]
pub struct ProjectModelRequest {
    pub model: String,
}

#[derive(Debug, Deserialize)]
pub struct SkillToggleRequest {
    pub name: String,
    pub enabled: bool,
}

#[derive(Debug, Deserialize)]
pub struct SkillInstallRequest {
    pub name: String,
    pub description: String,
    pub instructions: String,
}

#[derive(Debug, Deserialize)]
pub struct SkillRemoveRequest {
    pub name: String,
}

pub async fn get_state(State(daemon): State<DaemonState>) -> Json<SessionSnapshot> {
    let daemon = daemon.read().await;
    Json(daemon.get_snapshot())
}

pub async fn get_panel(
    State(daemon): State<DaemonState>,
    Path(panel_id): Path<String>,
) -> Result<Json<PanelResponse>, (StatusCode, Json<serde_json::Value>)> {
    let daemon = daemon.read().await;
    daemon
        .panel_response(&panel_id)
        .map(Json)
        .map_err(|error| api_error(StatusCode::BAD_REQUEST, error.to_string()))
}

pub async fn send_message(
    State(daemon): State<DaemonState>,
    Json(req): Json<ChatRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let conversation_id = req
        .conversation_id
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    process_send_message(
        daemon,
        conversation_id,
        req.content,
        req.model.unwrap_or_default(),
    )
    .await
    .map(Json)
}

pub async fn navigate_shell(
    State(daemon): State<DaemonState>,
    Json(req): Json<NavigationRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    process_navigation(daemon, req).await.map(Json)
}

pub async fn launch_home_mission(
    State(daemon): State<DaemonState>,
    Json(req): Json<HomeLaunchRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    process_home_launch(daemon, req.template_id, req.detail)
        .await
        .map(Json)
}

pub async fn switch_workspace(
    State(daemon): State<DaemonState>,
    Json(req): Json<WorkspaceSwitchRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    process_workspace_switch(daemon, req.workspace_path)
        .await
        .map(Json)
}

pub async fn resume_conversation(
    State(daemon): State<DaemonState>,
    Json(req): Json<ResumeConversationRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    process_resume_conversation(daemon, req.conversation_id)
        .await
        .map(Json)
}

pub async fn approval_decision(
    State(daemon): State<DaemonState>,
    Path(request_id): Path<String>,
    Json(req): Json<ApprovalDecisionRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    process_approval_decision(daemon, request_id, req.approved, req.reason)
        .await
        .map(Json)
}

pub async fn agent_action(
    State(daemon): State<DaemonState>,
    Json(req): Json<AgentRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    process_agent_action(
        daemon,
        req.goal,
        req.orchestration_mode.unwrap_or_else(|| "coordinator".into()),
    )
    .await
    .map(Json)
}

pub async fn agent_cancel(
    State(daemon): State<DaemonState>,
    Path(run_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    process_agent_cancel(daemon, run_id).await.map(Json)
}

pub async fn file_navigate(
    State(daemon): State<DaemonState>,
    Json(req): Json<FilePathRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    process_file_navigate(daemon, req.path).await.map(Json)
}

pub async fn file_open(
    State(daemon): State<DaemonState>,
    Json(req): Json<FilePathRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    process_file_open(daemon, req.path).await.map(Json)
}

pub async fn spec_select(
    State(daemon): State<DaemonState>,
    Json(req): Json<FilePathRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    process_spec_select(daemon, req.path).await.map(Json)
}

pub async fn git_stage_all(
    State(daemon): State<DaemonState>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    process_git_stage_all(daemon).await.map(Json)
}

pub async fn git_unstage_all(
    State(daemon): State<DaemonState>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    process_git_unstage_all(daemon).await.map(Json)
}

pub async fn git_commit(
    State(daemon): State<DaemonState>,
    Json(req): Json<GitCommitRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    process_git_commit(daemon, req.message).await.map(Json)
}

pub async fn terminal_start(
    State(daemon): State<DaemonState>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    process_terminal_start(daemon).await.map(Json)
}

pub async fn terminal_send(
    State(daemon): State<DaemonState>,
    Json(req): Json<TerminalInputRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    process_terminal_send(daemon, req.input).await.map(Json)
}

pub async fn terminal_clear(
    State(daemon): State<DaemonState>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    process_terminal_clear(daemon).await.map(Json)
}

pub async fn terminal_kill(
    State(daemon): State<DaemonState>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    process_terminal_kill(daemon).await.map(Json)
}

pub async fn workflow_run(
    State(daemon): State<DaemonState>,
    Json(req): Json<WorkflowRunRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    process_workflow_run(daemon, req.workflow_id).await.map(Json)
}

pub async fn channel_select(
    State(daemon): State<DaemonState>,
    Json(req): Json<ChannelSelectRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    process_channel_select(daemon, req.channel_id).await.map(Json)
}

pub async fn channel_message(
    State(daemon): State<DaemonState>,
    Json(req): Json<ChannelMessageRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    process_channel_message(daemon, req.channel_id, req.content).await.map(Json)
}

pub async fn assistant_decision(
    State(daemon): State<DaemonState>,
    Path(approval_id): Path<String>,
    Json(req): Json<AssistantDecisionRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    process_assistant_decision(daemon, approval_id, req.approved)
        .await
        .map(Json)
}

pub async fn settings_update(
    State(daemon): State<DaemonState>,
    Json(req): Json<SettingsUpdateRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    process_settings_update(daemon, req.setting, req.value)
        .await
        .map(Json)
}

pub async fn models_default(
    State(daemon): State<DaemonState>,
    Json(req): Json<DefaultModelRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    process_default_model_update(daemon, req.model).await.map(Json)
}

pub async fn settings_text(
    State(daemon): State<DaemonState>,
    Json(req): Json<SettingsTextRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    process_settings_text_update(daemon, req.setting, req.value)
        .await
        .map(Json)
}

pub async fn routing_update(
    State(daemon): State<DaemonState>,
    Json(req): Json<RoutingUpdateRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    process_routing_update(daemon, req.enabled).await.map(Json)
}

pub async fn provider_key_update(
    State(daemon): State<DaemonState>,
    Json(req): Json<ProviderKeyRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    process_provider_key_update(daemon, req.provider, req.key)
        .await
        .map(Json)
}

pub async fn routing_project_model_add(
    State(daemon): State<DaemonState>,
    Json(req): Json<ProjectModelRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    process_project_model_add(daemon, req.model).await.map(Json)
}

pub async fn routing_project_model_remove(
    State(daemon): State<DaemonState>,
    Json(req): Json<ProjectModelRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    process_project_model_remove(daemon, req.model)
        .await
        .map(Json)
}

pub async fn skills_toggle(
    State(daemon): State<DaemonState>,
    Json(req): Json<SkillToggleRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    process_skill_toggle(daemon, req.name, req.enabled)
        .await
        .map(Json)
}

pub async fn skills_install(
    State(daemon): State<DaemonState>,
    Json(req): Json<SkillInstallRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    process_skill_install(daemon, req.name, req.description, req.instructions)
        .await
        .map(Json)
}

pub async fn skills_remove(
    State(daemon): State<DaemonState>,
    Json(req): Json<SkillRemoveRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    process_skill_remove(daemon, req.name).await.map(Json)
}

pub async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(daemon): State<DaemonState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_websocket(socket, daemon))
}

async fn handle_websocket(socket: WebSocket, daemon: DaemonState) {
    let (mut sender, mut receiver) = socket.split();

    if send_initial_payloads(&mut sender, daemon.clone()).await.is_err() {
        return;
    }

    let mut rx = {
        let daemon = daemon.read().await;
        daemon.subscribe()
    };

    let send_task = tokio::spawn(async move {
        while let Ok(event) = rx.recv().await {
            if let Ok(json) = serde_json::to_string(&event)
                && sender.send(Message::Text(json.into())).await.is_err()
            {
                break;
            }
        }
    });

    let daemon_clone = daemon.clone();
    let recv_task = tokio::spawn(async move {
        while let Some(Ok(message)) = receiver.next().await {
            if let Message::Text(text) = message {
                match serde_json::from_str::<DaemonEvent>(&text) {
                    Ok(event) => {
                        if let Err((status, payload)) =
                            dispatch_client_event(daemon_clone.clone(), event).await
                        {
                            let daemon = daemon_clone.read().await;
                            daemon.broadcast_event(DaemonEvent::Error {
                                code: status.as_u16(),
                                message: payload["error"]
                                    .as_str()
                                    .unwrap_or("Request failed")
                                    .to_string(),
                            });
                        }
                    }
                    Err(error) => {
                        let daemon = daemon_clone.read().await;
                        daemon.broadcast_event(DaemonEvent::Error {
                            code: StatusCode::BAD_REQUEST.as_u16(),
                            message: format!("Invalid websocket event: {error}"),
                        });
                    }
                }
            }
        }
    });

    tokio::select! {
        _ = send_task => {}
        _ = recv_task => {}
    }
}

async fn send_initial_payloads(
    sender: &mut futures::stream::SplitSink<WebSocket, Message>,
    daemon: DaemonState,
) -> Result<(), ()> {
    let (snapshot, panels) = {
        let daemon = daemon.read().await;
        let snapshot = daemon.get_snapshot();
        let mut panels = Vec::new();
        for panel in ["home", "chat", "observe", snapshot.active_panel.as_str()] {
            if let Ok(response) = daemon.panel_response(panel) {
                panels.push(DaemonEvent::PanelData {
                    panel: response.panel,
                    data: serde_json::to_value(response.data).unwrap_or_default(),
                });
            }
        }
        (snapshot, panels)
    };

    let snapshot_event = DaemonEvent::StateSnapshot(snapshot);
    let snapshot_json = serde_json::to_string(&snapshot_event).map_err(|_| ())?;
    sender
        .send(Message::Text(snapshot_json.into()))
        .await
        .map_err(|_| ())?;

    for event in panels {
        let json = serde_json::to_string(&event).map_err(|_| ())?;
        sender.send(Message::Text(json.into())).await.map_err(|_| ())?;
    }

    Ok(())
}

async fn dispatch_client_event(
    daemon: DaemonState,
    event: DaemonEvent,
) -> Result<serde_json::Value, (StatusCode, Json<serde_json::Value>)> {
    match event {
        DaemonEvent::SendMessage {
            conversation_id,
            content,
            model,
        } => process_send_message(daemon, conversation_id, content, model).await,
        DaemonEvent::SwitchPanel { panel } => {
            process_navigation(
                daemon,
                NavigationRequest {
                    destination: None,
                    panel: Some(panel),
                    model: None,
                    observe_view: None,
                },
            )
            .await
        }
        DaemonEvent::SwitchDestination { destination } => {
            process_navigation(
                daemon,
                NavigationRequest {
                    destination: Some(destination),
                    panel: None,
                    model: None,
                    observe_view: None,
                },
            )
            .await
        }
        DaemonEvent::SetModel { model } => {
            process_navigation(
                daemon,
                NavigationRequest {
                    destination: None,
                    panel: None,
                    model: Some(model),
                    observe_view: None,
                },
            )
            .await
        }
        DaemonEvent::SetObserveView { view } => {
            process_navigation(
                daemon,
                NavigationRequest {
                    destination: None,
                    panel: None,
                    model: None,
                    observe_view: Some(view),
                },
            )
            .await
        }
        DaemonEvent::SwitchWorkspace { workspace_path } => {
            process_workspace_switch(daemon, workspace_path).await
        }
        DaemonEvent::LaunchHomeMission {
            template_id,
            detail,
        } => process_home_launch(daemon, template_id, detail).await,
        DaemonEvent::ResumeConversation { conversation_id } => {
            process_resume_conversation(daemon, conversation_id).await
        }
        DaemonEvent::ApprovalDecision {
            request_id,
            approved,
            reason,
        } => process_approval_decision(daemon, request_id, approved, reason).await,
        DaemonEvent::StartAgentTask {
            goal,
            orchestration_mode,
        } => process_agent_action(daemon, goal, orchestration_mode).await,
        DaemonEvent::CancelAgentTask { run_id } => {
            let snapshot = {
                let mut daemon = daemon.write().await;
                daemon.cancel_agent_task(&run_id);
                daemon.get_snapshot()
            };
            Ok(serde_json::json!({
                "status": "cancelled",
                "run_id": run_id,
                "snapshot": snapshot,
            }))
        }
        DaemonEvent::ResponseFeedback {
            message_id,
            positive,
        } => {
            let mut daemon = daemon.write().await;
            daemon
                .handle_event(DaemonEvent::ResponseFeedback {
                    message_id: message_id.clone(),
                    positive,
                })
                .await;
            Ok(serde_json::json!({
                "status": "ok",
                "message_id": message_id,
            }))
        }
        DaemonEvent::Ping => {
            let daemon = daemon.read().await;
            daemon.broadcast_event(DaemonEvent::Pong);
            Ok(serde_json::json!({ "status": "pong" }))
        }
        _ => Ok(serde_json::json!({ "status": "ignored" })),
    }
}

async fn process_send_message(
    daemon: DaemonState,
    conversation_id: String,
    content: String,
    model: String,
) -> Result<serde_json::Value, (StatusCode, Json<serde_json::Value>)> {
    let disposition = {
        let mut daemon = daemon.write().await;
        daemon
            .begin_send_message(conversation_id.clone(), content, model)
            .map_err(|error| api_error(StatusCode::BAD_REQUEST, error.to_string()))?
    };

    let snapshot = {
        let daemon = daemon.read().await;
        daemon.get_snapshot()
    };

    match disposition {
        SendDisposition::Stream {
            conversation_id,
            model,
        } => {
            spawn_chat_stream(daemon.clone(), conversation_id.clone(), model.clone());
            Ok(serde_json::json!({
                "status": "streaming",
                "conversation_id": conversation_id,
                "model": model,
                "snapshot": snapshot,
            }))
        }
        SendDisposition::ApprovalPending { request_id } => Ok(serde_json::json!({
            "status": "approval_required",
            "request_id": request_id,
            "snapshot": snapshot,
        })),
    }
}

async fn process_navigation(
    daemon: DaemonState,
    req: NavigationRequest,
) -> Result<serde_json::Value, (StatusCode, Json<serde_json::Value>)> {
    let snapshot = {
        let mut daemon = daemon.write().await;
        if let Some(destination) = req.destination {
            daemon.switch_destination(destination);
        }
        if let Some(panel) = req.panel {
            daemon.switch_panel(&panel);
        }
        if let Some(model) = req.model {
            daemon.set_model(model);
        }
        if let Some(view) = req.observe_view {
            daemon.set_observe_view(view);
        }
        daemon.get_snapshot()
    };

    Ok(serde_json::json!({
        "status": "ok",
        "snapshot": snapshot,
    }))
}

async fn process_home_launch(
    daemon: DaemonState,
    template_id: String,
    detail: String,
) -> Result<serde_json::Value, (StatusCode, Json<serde_json::Value>)> {
    let disposition = {
        let mut daemon = daemon.write().await;
        daemon
            .launch_home_mission(template_id.clone(), detail)
            .map_err(|error| api_error(StatusCode::BAD_REQUEST, error.to_string()))?
    };

    let snapshot = {
        let daemon = daemon.read().await;
        daemon.get_snapshot()
    };

    match disposition {
        SendDisposition::Stream {
            conversation_id,
            model,
        } => {
            spawn_chat_stream(daemon.clone(), conversation_id.clone(), model.clone());
            Ok(serde_json::json!({
                "status": "launched",
                "conversation_id": conversation_id,
                "model": model,
                "template_id": template_id,
                "snapshot": snapshot,
            }))
        }
        SendDisposition::ApprovalPending { request_id } => Ok(serde_json::json!({
            "status": "approval_required",
            "request_id": request_id,
            "template_id": template_id,
            "snapshot": snapshot,
        })),
    }
}

async fn process_workspace_switch(
    daemon: DaemonState,
    workspace_path: String,
) -> Result<serde_json::Value, (StatusCode, Json<serde_json::Value>)> {
    let snapshot = {
        let mut daemon = daemon.write().await;
        daemon.switch_workspace(workspace_path.clone());
        daemon.get_snapshot()
    };

    Ok(serde_json::json!({
        "status": "ok",
        "workspace_path": workspace_path,
        "snapshot": snapshot,
    }))
}

async fn process_resume_conversation(
    daemon: DaemonState,
    conversation_id: String,
) -> Result<serde_json::Value, (StatusCode, Json<serde_json::Value>)> {
    let snapshot = {
        let mut daemon = daemon.write().await;
        daemon
            .resume_conversation(&conversation_id)
            .map_err(|error| api_error(StatusCode::BAD_REQUEST, error.to_string()))?;
        daemon.get_snapshot()
    };

    Ok(serde_json::json!({
        "status": "ok",
        "conversation_id": conversation_id,
        "snapshot": snapshot,
    }))
}

async fn process_approval_decision(
    daemon: DaemonState,
    request_id: String,
    approved: bool,
    reason: Option<String>,
) -> Result<serde_json::Value, (StatusCode, Json<serde_json::Value>)> {
    let pending = {
        let mut daemon = daemon.write().await;
        daemon
            .apply_approval_decision(&request_id, approved, reason.clone())
            .map_err(|error| api_error(StatusCode::BAD_REQUEST, error.to_string()))?
    };

    let mut status = if approved { "approved" } else { "denied" };
    let mut run_id = None;
    let mut conversation_id = None;

    if approved {
        if let Some(PendingAction::Chat {
            conversation_id: id,
            model,
            ..
        }) = pending
        {
            {
                let mut daemon = daemon.write().await;
                daemon.resume_approved_chat_stream(&id, &model);
            }
            conversation_id = Some(id.clone());
            status = "streaming";
            spawn_chat_stream(daemon.clone(), id, model);
        } else if let Some(PendingAction::Agent {
            run_id: pending_run_id,
            goal,
            orchestration_mode,
        }) = pending
        {
            {
                let mut daemon = daemon.write().await;
                daemon.update_agent_status(
                    &pending_run_id,
                    "planning",
                    format!("Preparing {} run", orchestration_mode),
                    0,
                    0.0,
                );
            }
            run_id = Some(pending_run_id.clone());
            status = "running";
            spawn_agent_run(daemon.clone(), pending_run_id, goal, orchestration_mode);
        }
    }

    let snapshot = {
        let daemon = daemon.read().await;
        daemon.get_snapshot()
    };

    Ok(serde_json::json!({
        "status": status,
        "request_id": request_id,
        "approved": approved,
        "reason": reason,
        "run_id": run_id,
        "conversation_id": conversation_id,
        "snapshot": snapshot,
    }))
}

async fn process_agent_action(
    daemon: DaemonState,
    goal: String,
    orchestration_mode: String,
) -> Result<serde_json::Value, (StatusCode, Json<serde_json::Value>)> {
    let disposition = {
        let mut daemon = daemon.write().await;
        daemon
            .start_agent_task(goal.clone(), orchestration_mode.clone())
            .map_err(|error| api_error(StatusCode::BAD_REQUEST, error.to_string()))?
    };

    let snapshot = {
        let daemon = daemon.read().await;
        daemon.get_snapshot()
    };

    match disposition {
        AgentDisposition::Run {
            run_id,
            goal,
            orchestration_mode,
        } => {
            spawn_agent_run(
                daemon.clone(),
                run_id.clone(),
                goal.clone(),
                orchestration_mode.clone(),
            );
            Ok(serde_json::json!({
                "status": "started",
                "run_id": run_id,
                "goal": goal,
                "orchestration_mode": orchestration_mode,
                "snapshot": snapshot,
            }))
        }
        AgentDisposition::ApprovalPending { request_id, run_id } => Ok(serde_json::json!({
            "status": "approval_required",
            "request_id": request_id,
            "run_id": run_id,
            "snapshot": snapshot,
        })),
    }
}

async fn process_agent_cancel(
    daemon: DaemonState,
    run_id: String,
) -> Result<serde_json::Value, (StatusCode, Json<serde_json::Value>)> {
    let snapshot = {
        let mut daemon = daemon.write().await;
        daemon.cancel_agent_task(&run_id);
        daemon.get_snapshot()
    };

    Ok(serde_json::json!({
        "status": "cancelled",
        "run_id": run_id,
        "snapshot": snapshot,
    }))
}

async fn process_workflow_run(
    daemon: DaemonState,
    workflow_id: String,
) -> Result<serde_json::Value, (StatusCode, Json<serde_json::Value>)> {
    let daemon_for_task = daemon.clone();
    let (run_id, workflow_name, workflow, snapshot) = {
        let mut daemon = daemon.write().await;
        let (run_id, workflow, _working_dir) = daemon
            .start_workflow_run(&workflow_id)
            .map_err(|error| api_error(StatusCode::BAD_REQUEST, error.to_string()))?;
        let workflow_name = workflow.name.clone();
        let snapshot = daemon.get_snapshot();
        (run_id, workflow_name, workflow, snapshot)
    };
    spawn_workflow_run(daemon_for_task, run_id.clone(), workflow);

    Ok(serde_json::json!({
        "status": "started",
        "run_id": run_id,
        "workflow_id": workflow_id,
        "workflow_name": workflow_name,
        "snapshot": snapshot,
    }))
}

async fn process_channel_select(
    daemon: DaemonState,
    channel_id: String,
) -> Result<serde_json::Value, (StatusCode, Json<serde_json::Value>)> {
    let snapshot = {
        let mut daemon = daemon.write().await;
        daemon
            .select_channel(&channel_id)
            .map_err(|error| api_error(StatusCode::BAD_REQUEST, error.to_string()))?;
        daemon.get_snapshot()
    };

    Ok(serde_json::json!({
        "status": "ok",
        "channel_id": channel_id,
        "snapshot": snapshot,
    }))
}

async fn process_channel_message(
    daemon: DaemonState,
    channel_id: String,
    content: String,
) -> Result<serde_json::Value, (StatusCode, Json<serde_json::Value>)> {
    let snapshot = {
        let mut daemon = daemon.write().await;
        daemon
            .send_channel_message(&channel_id, &content)
            .map_err(|error| api_error(StatusCode::BAD_REQUEST, error.to_string()))?;
        daemon.get_snapshot()
    };

    Ok(serde_json::json!({
        "status": "ok",
        "channel_id": channel_id,
        "snapshot": snapshot,
    }))
}

async fn process_assistant_decision(
    daemon: DaemonState,
    approval_id: String,
    approved: bool,
) -> Result<serde_json::Value, (StatusCode, Json<serde_json::Value>)> {
    let snapshot = {
        let mut daemon = daemon.write().await;
        daemon
            .decide_assistant_approval(&approval_id, approved)
            .map_err(|error| api_error(StatusCode::BAD_REQUEST, error.to_string()))?;
        daemon.get_snapshot()
    };

    Ok(serde_json::json!({
        "status": if approved { "approved" } else { "rejected" },
        "approval_id": approval_id,
        "snapshot": snapshot,
    }))
}

async fn process_settings_update(
    daemon: DaemonState,
    setting: String,
    value: bool,
) -> Result<serde_json::Value, (StatusCode, Json<serde_json::Value>)> {
    let snapshot = {
        let mut daemon = daemon.write().await;
        daemon
            .update_setting(&setting, value)
            .map_err(|error| api_error(StatusCode::BAD_REQUEST, error.to_string()))?;
        daemon.get_snapshot()
    };

    Ok(serde_json::json!({
        "status": "ok",
        "setting": setting,
        "value": value,
        "snapshot": snapshot,
    }))
}

async fn process_settings_text_update(
    daemon: DaemonState,
    setting: String,
    value: String,
) -> Result<serde_json::Value, (StatusCode, Json<serde_json::Value>)> {
    let snapshot = {
        let mut daemon = daemon.write().await;
        daemon
            .update_text_setting(&setting, value.clone())
            .map_err(|error| api_error(StatusCode::BAD_REQUEST, error.to_string()))?;
        daemon.get_snapshot()
    };

    Ok(serde_json::json!({
        "status": "ok",
        "setting": setting,
        "value": value,
        "snapshot": snapshot,
    }))
}

async fn process_default_model_update(
    daemon: DaemonState,
    model: String,
) -> Result<serde_json::Value, (StatusCode, Json<serde_json::Value>)> {
    let snapshot = {
        let mut daemon = daemon.write().await;
        daemon
            .set_default_model(&model)
            .map_err(|error| api_error(StatusCode::BAD_REQUEST, error.to_string()))?;
        daemon.get_snapshot()
    };

    Ok(serde_json::json!({
        "status": "ok",
        "model": model,
        "snapshot": snapshot,
    }))
}

async fn process_routing_update(
    daemon: DaemonState,
    enabled: bool,
) -> Result<serde_json::Value, (StatusCode, Json<serde_json::Value>)> {
    let snapshot = {
        let mut daemon = daemon.write().await;
        daemon
            .set_auto_routing(enabled)
            .map_err(|error| api_error(StatusCode::BAD_REQUEST, error.to_string()))?;
        daemon.get_snapshot()
    };

    Ok(serde_json::json!({
        "status": "ok",
        "enabled": enabled,
        "snapshot": snapshot,
    }))
}

async fn process_provider_key_update(
    daemon: DaemonState,
    provider: String,
    key: String,
) -> Result<serde_json::Value, (StatusCode, Json<serde_json::Value>)> {
    let snapshot = {
        let mut daemon = daemon.write().await;
        daemon
            .set_provider_key(&provider, key)
            .map_err(|error| api_error(StatusCode::BAD_REQUEST, error.to_string()))?;
        daemon.get_snapshot()
    };

    Ok(serde_json::json!({
        "status": "ok",
        "provider": provider,
        "snapshot": snapshot,
    }))
}

async fn process_project_model_add(
    daemon: DaemonState,
    model: String,
) -> Result<serde_json::Value, (StatusCode, Json<serde_json::Value>)> {
    let snapshot = {
        let mut daemon = daemon.write().await;
        daemon
            .add_project_model(model.clone())
            .map_err(|error| api_error(StatusCode::BAD_REQUEST, error.to_string()))?;
        daemon.get_snapshot()
    };

    Ok(serde_json::json!({
        "status": "ok",
        "model": model,
        "snapshot": snapshot,
    }))
}

async fn process_project_model_remove(
    daemon: DaemonState,
    model: String,
) -> Result<serde_json::Value, (StatusCode, Json<serde_json::Value>)> {
    let snapshot = {
        let mut daemon = daemon.write().await;
        daemon
            .remove_project_model(&model)
            .map_err(|error| api_error(StatusCode::BAD_REQUEST, error.to_string()))?;
        daemon.get_snapshot()
    };

    Ok(serde_json::json!({
        "status": "ok",
        "model": model,
        "snapshot": snapshot,
    }))
}

async fn process_skill_toggle(
    daemon: DaemonState,
    name: String,
    enabled: bool,
) -> Result<serde_json::Value, (StatusCode, Json<serde_json::Value>)> {
    let snapshot = {
        let mut daemon = daemon.write().await;
        daemon
            .set_skill_enabled(&name, enabled)
            .map_err(|error| api_error(StatusCode::BAD_REQUEST, error.to_string()))?;
        daemon.get_snapshot()
    };

    Ok(serde_json::json!({
        "status": "ok",
        "name": name,
        "enabled": enabled,
        "snapshot": snapshot,
    }))
}

async fn process_skill_install(
    daemon: DaemonState,
    name: String,
    description: String,
    instructions: String,
) -> Result<serde_json::Value, (StatusCode, Json<serde_json::Value>)> {
    let snapshot = {
        let mut daemon = daemon.write().await;
        daemon
            .install_skill(name.clone(), description, instructions)
            .map_err(|error| api_error(StatusCode::BAD_REQUEST, error.to_string()))?;
        daemon.get_snapshot()
    };

    Ok(serde_json::json!({
        "status": "ok",
        "name": name,
        "snapshot": snapshot,
    }))
}

async fn process_skill_remove(
    daemon: DaemonState,
    name: String,
) -> Result<serde_json::Value, (StatusCode, Json<serde_json::Value>)> {
    let snapshot = {
        let mut daemon = daemon.write().await;
        daemon
            .remove_skill(&name)
            .map_err(|error| api_error(StatusCode::BAD_REQUEST, error.to_string()))?;
        daemon.get_snapshot()
    };

    Ok(serde_json::json!({
        "status": "ok",
        "name": name,
        "snapshot": snapshot,
    }))
}

async fn process_file_navigate(
    daemon: DaemonState,
    path: String,
) -> Result<serde_json::Value, (StatusCode, Json<serde_json::Value>)> {
    let snapshot = {
        let mut daemon = daemon.write().await;
        daemon
            .navigate_files(&path)
            .map_err(|error| api_error(StatusCode::BAD_REQUEST, error.to_string()))?;
        daemon.get_snapshot()
    };

    Ok(serde_json::json!({
        "status": "ok",
        "path": path,
        "snapshot": snapshot,
    }))
}

async fn process_file_open(
    daemon: DaemonState,
    path: String,
) -> Result<serde_json::Value, (StatusCode, Json<serde_json::Value>)> {
    let snapshot = {
        let mut daemon = daemon.write().await;
        daemon
            .open_file(&path)
            .map_err(|error| api_error(StatusCode::BAD_REQUEST, error.to_string()))?;
        daemon.get_snapshot()
    };

    Ok(serde_json::json!({
        "status": "ok",
        "path": path,
        "snapshot": snapshot,
    }))
}

async fn process_spec_select(
    daemon: DaemonState,
    path: String,
) -> Result<serde_json::Value, (StatusCode, Json<serde_json::Value>)> {
    let snapshot = {
        let mut daemon = daemon.write().await;
        daemon
            .select_spec(&path)
            .map_err(|error| api_error(StatusCode::BAD_REQUEST, error.to_string()))?;
        daemon.get_snapshot()
    };

    Ok(serde_json::json!({
        "status": "ok",
        "path": path,
        "snapshot": snapshot,
    }))
}

async fn process_git_stage_all(
    daemon: DaemonState,
) -> Result<serde_json::Value, (StatusCode, Json<serde_json::Value>)> {
    let (staged, snapshot) = {
        let mut daemon = daemon.write().await;
        let staged = daemon
            .git_stage_all()
            .map_err(|error| api_error(StatusCode::BAD_REQUEST, error.to_string()))?;
        (staged, daemon.get_snapshot())
    };

    Ok(serde_json::json!({
        "status": "ok",
        "staged": staged,
        "snapshot": snapshot,
    }))
}

async fn process_git_unstage_all(
    daemon: DaemonState,
) -> Result<serde_json::Value, (StatusCode, Json<serde_json::Value>)> {
    let (unstaged, snapshot) = {
        let mut daemon = daemon.write().await;
        let unstaged = daemon
            .git_unstage_all()
            .map_err(|error| api_error(StatusCode::BAD_REQUEST, error.to_string()))?;
        (unstaged, daemon.get_snapshot())
    };

    Ok(serde_json::json!({
        "status": "ok",
        "unstaged": unstaged,
        "snapshot": snapshot,
    }))
}

async fn process_git_commit(
    daemon: DaemonState,
    message: String,
) -> Result<serde_json::Value, (StatusCode, Json<serde_json::Value>)> {
    let (commit, snapshot) = {
        let mut daemon = daemon.write().await;
        let commit = daemon
            .git_commit(&message)
            .map_err(|error| api_error(StatusCode::BAD_REQUEST, error.to_string()))?;
        (commit, daemon.get_snapshot())
    };

    Ok(serde_json::json!({
        "status": "ok",
        "commit": commit,
        "snapshot": snapshot,
    }))
}

async fn process_terminal_start(
    daemon: DaemonState,
) -> Result<serde_json::Value, (StatusCode, Json<serde_json::Value>)> {
    let (started, spawn_reader, snapshot) = {
        let mut daemon = daemon.write().await;
        let started = daemon
            .start_terminal()
            .map_err(|error| api_error(StatusCode::BAD_REQUEST, error.to_string()))?;
        let spawn_reader = daemon.ensure_terminal_reader();
        (started, spawn_reader, daemon.get_snapshot())
    };

    if spawn_reader {
        spawn_terminal_reader(daemon.clone());
    }

    Ok(serde_json::json!({
        "status": if started { "started" } else { "running" },
        "snapshot": snapshot,
    }))
}

async fn process_terminal_send(
    daemon: DaemonState,
    input: String,
) -> Result<serde_json::Value, (StatusCode, Json<serde_json::Value>)> {
    let daemon_state = daemon.clone();
    let (shell, snapshot) = {
        let mut daemon_guard = daemon.write().await;
        let _ = daemon_guard
            .start_terminal()
            .map_err(|error| api_error(StatusCode::BAD_REQUEST, error.to_string()))?;
        let spawn_reader = daemon_guard.ensure_terminal_reader();
        let shell = daemon_guard
            .terminal_shell()
            .ok_or_else(|| api_error(StatusCode::SERVICE_UNAVAILABLE, "Terminal is unavailable"))?;
        let snapshot = daemon_guard.get_snapshot();
        drop(daemon_guard);
        if spawn_reader {
            spawn_terminal_reader(daemon_state.clone());
        }
        (shell, snapshot)
    };

    {
        let mut shell = shell.lock().await;
        shell.write(&input).await.map_err(|error| {
            api_error(
                StatusCode::BAD_GATEWAY,
                format!("Failed to send terminal input: {error}"),
            )
        })?;
    }

    Ok(serde_json::json!({
        "status": "ok",
        "snapshot": snapshot,
    }))
}

async fn process_terminal_clear(
    daemon: DaemonState,
) -> Result<serde_json::Value, (StatusCode, Json<serde_json::Value>)> {
    let snapshot = {
        let mut daemon = daemon.write().await;
        daemon.clear_terminal();
        daemon.get_snapshot()
    };

    Ok(serde_json::json!({
        "status": "ok",
        "snapshot": snapshot,
    }))
}

async fn process_terminal_kill(
    daemon: DaemonState,
) -> Result<serde_json::Value, (StatusCode, Json<serde_json::Value>)> {
    let shell = {
        let mut daemon = daemon.write().await;
        daemon.take_terminal_shell()
    };

    if let Some(shell) = shell {
        let mut shell = shell.lock().await;
        shell.kill().await.map_err(|error| {
            api_error(
                StatusCode::BAD_GATEWAY,
                format!("Failed to stop terminal: {error}"),
            )
        })?;
    }

    let snapshot = {
        let mut daemon = daemon.write().await;
        daemon.finish_terminal_reader();
        daemon.push_terminal_output(ShellOutput::Exit(-1));
        daemon.broadcast_state_and_panels();
        daemon.get_snapshot()
    };

    Ok(serde_json::json!({
        "status": "ok",
        "snapshot": snapshot,
    }))
}

fn spawn_workflow_run(daemon: DaemonState, run_id: String, workflow: hive_agents::automation::Workflow) {
    tokio::task::spawn_blocking(move || {
        let working_dir = {
            let daemon = daemon.blocking_read();
            daemon.current_workspace_root()
        };
        let result =
            hive_agents::automation::AutomationService::execute_workflow_blocking(&workflow, working_dir)
                .map_err(|error| error.to_string());
        let mut daemon = daemon.blocking_write();
        let _ = daemon.finish_workflow_run(&run_id, result);
    });
}

fn spawn_chat_stream(daemon: DaemonState, conversation_id: String, model: String) {
    tokio::spawn(async move {
        let (messages, ai_service) = {
            let daemon_guard = daemon.read().await;
            match daemon_guard.ai_messages_for_conversation(&conversation_id) {
                Ok(messages) => (messages, daemon_guard.ai_service()),
                Err(error) => {
                    drop(daemon_guard);
                    fail_stream(
                        daemon.clone(),
                        conversation_id,
                        format!("Failed to load conversation for streaming: {error}"),
                        StatusCode::INTERNAL_SERVER_ERROR,
                    )
                    .await;
                    return;
                }
            }
        };

        let prepared = {
            let service = ai_service.lock().await;
            service.prepare_stream(messages, &model, None, None)
        };

        let Some((provider, request)) = prepared else {
            fail_stream(
                daemon.clone(),
                conversation_id,
                "Hive Remote could not find a configured AI provider for this model.".into(),
                StatusCode::SERVICE_UNAVAILABLE,
            )
            .await;
            return;
        };

        let mut rx = match provider.stream_chat(&request).await {
            Ok(rx) => rx,
            Err(error) => {
                fail_stream(
                    daemon.clone(),
                    conversation_id,
                    format!("Failed to start remote stream: {error}"),
                    StatusCode::BAD_GATEWAY,
                )
                .await;
                return;
            }
        };

        let mut content = String::new();
        let mut prompt_tokens = 0;
        let mut completion_tokens = 0;

        while let Some(chunk) = rx.recv().await {
            if !chunk.content.is_empty() {
                content.push_str(&chunk.content);
                let daemon = daemon.read().await;
                daemon.broadcast_event(DaemonEvent::StreamChunk {
                    conversation_id: conversation_id.clone(),
                    chunk: chunk.content,
                });
            }

            if let Some(usage) = chunk.usage {
                prompt_tokens = usage.prompt_tokens;
                completion_tokens = usage.completion_tokens;
            }
        }

        let mut daemon_guard = daemon.write().await;
        if let Err(error) = daemon_guard.complete_stream(
            &conversation_id,
            &request.model,
            &content,
            prompt_tokens,
            completion_tokens,
            None,
        ) {
            let message = format!("Failed to persist remote response: {error}");
            let _ = daemon_guard.fail_stream(&conversation_id, &message);
            daemon_guard.broadcast_event(DaemonEvent::Error {
                code: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                message,
            });
            return;
        }

        daemon_guard.broadcast_event(DaemonEvent::StreamComplete {
            conversation_id,
            prompt_tokens,
            completion_tokens,
            cost_usd: None,
        });
    });
}

fn spawn_terminal_reader(daemon: DaemonState) {
    tokio::spawn(async move {
        loop {
            let shell = {
                let daemon = daemon.read().await;
                daemon.terminal_shell()
            };

            let Some(shell) = shell else {
                let mut daemon = daemon.write().await;
                daemon.finish_terminal_reader();
                break;
            };

            let mut outputs = Vec::new();
            let mut exit_code = None;
            {
                let mut shell = shell.lock().await;
                while let Some(output) = shell.read() {
                    outputs.push(output);
                }
                match shell.try_wait() {
                    Ok(Some(code)) => exit_code = Some(code),
                    Ok(None) => {}
                    Err(error) => {
                        outputs.push(ShellOutput::Stderr(format!(
                            "Failed to query shell status: {error}"
                        )));
                        exit_code = Some(-1);
                    }
                }
            }

            if !outputs.is_empty() || exit_code.is_some() {
                let mut daemon = daemon.write().await;
                for output in outputs {
                    daemon.push_terminal_output(output);
                }
                if let Some(code) = exit_code {
                    daemon.push_terminal_output(ShellOutput::Exit(code));
                }
                daemon.broadcast_state_and_panels();
                if exit_code.is_some() {
                    break;
                }
            }

            sleep(Duration::from_millis(120)).await;
        }
    });
}

fn spawn_agent_run(
    daemon: DaemonState,
    run_id: String,
    goal: String,
    orchestration_mode: String,
) {
    tokio::spawn(async move {
        {
            let mut daemon = daemon.write().await;
            daemon.update_agent_status(
                &run_id,
                "planning",
                format!("Analyzing goal: {}", goal),
                150,
                0.0,
            );
        }

        sleep(Duration::from_millis(350)).await;

        {
            let mut daemon = daemon.write().await;
            daemon.update_agent_status(
                &run_id,
                "running",
                format!("{} is executing the plan", orchestration_mode),
                1_200,
                0.01,
            );
        }

        sleep(Duration::from_millis(700)).await;

        let mut daemon = daemon.write().await;
        daemon.update_agent_status(
            &run_id,
            "completed",
            format!("Finished remote run for '{}'", goal),
            2_800,
            0.03,
        );
    });
}

async fn fail_stream(
    daemon: DaemonState,
    conversation_id: String,
    message: String,
    status: StatusCode,
) {
    let mut daemon = daemon.write().await;
    let _ = daemon.fail_stream(&conversation_id, &message);
    daemon.broadcast_event(DaemonEvent::Error {
        code: status.as_u16(),
        message,
    });
}

fn api_error(
    status: StatusCode,
    message: impl Into<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let message = message.into();
    (
        status,
        Json(serde_json::json!({
            "error": message,
        })),
    )
}
