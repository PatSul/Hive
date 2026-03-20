use gpui::*;
use tracing::{debug, error, info, warn};

use super::{
    AppNotification, AppNotifications, AppOllamaManager, AppUpdater, AppVoiceAssistant,
    HiveWorkspace, NotificationType, OllamaDeleteModel, OllamaPullModel, SwitchToAgents,
    SwitchToChat, SwitchToFiles, SwitchToHistory, SwitchToModels, SwitchToNetwork,
    SwitchToSettings, SwitchToTerminal, ToggleDisclosure, TriggerAppUpdate, VoiceProcessText,
};

pub(super) fn handle_voice_process_text(
    _workspace: &mut HiveWorkspace,
    action: &VoiceProcessText,
    window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    if !cx.has_global::<AppVoiceAssistant>() {
        return;
    }
    let command = {
        let voice_assistant = cx.global::<AppVoiceAssistant>();
        match voice_assistant.0.lock() {
            Ok(mut voice) => voice.process_text(&action.text),
            Err(_) => return,
        }
    };

    info!(
        "Voice command: intent={:?}, confidence={:.2}",
        command.intent, command.confidence
    );

    use hive_agents::VoiceIntent;
    match command.intent {
        VoiceIntent::OpenPanel => {
            let text_lower = action.text.to_lowercase();
            if text_lower.contains("file") {
                window.dispatch_action(Box::new(SwitchToFiles), cx);
            } else if text_lower.contains("terminal") || text_lower.contains("shell") {
                window.dispatch_action(Box::new(SwitchToTerminal), cx);
            } else if text_lower.contains("setting") {
                window.dispatch_action(Box::new(SwitchToSettings), cx);
            } else if text_lower.contains("model") {
                window.dispatch_action(Box::new(SwitchToModels), cx);
            } else if text_lower.contains("chat") {
                window.dispatch_action(Box::new(SwitchToChat), cx);
            } else if text_lower.contains("history") {
                window.dispatch_action(Box::new(SwitchToHistory), cx);
            } else if text_lower.contains("network") {
                window.dispatch_action(Box::new(SwitchToNetwork), cx);
            } else if text_lower.contains("agent") {
                window.dispatch_action(Box::new(SwitchToAgents), cx);
            }
        }
        VoiceIntent::SearchFiles => {
            window.dispatch_action(Box::new(SwitchToFiles), cx);
        }
        VoiceIntent::RunCommand => {
            window.dispatch_action(Box::new(SwitchToTerminal), cx);
        }
        VoiceIntent::SendMessage | VoiceIntent::CreateTask => {
            window.dispatch_action(Box::new(SwitchToChat), cx);
        }
        _ => {
            debug!("Voice: unhandled intent {:?}, ignoring", command.intent);
        }
    }
}

pub(super) fn handle_trigger_app_update(
    _workspace: &mut HiveWorkspace,
    _action: &TriggerAppUpdate,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    if !cx.has_global::<AppUpdater>() {
        return;
    }

    let updater = cx.global::<AppUpdater>().0.clone();
    if updater.is_updating() {
        info!("Update already in progress");
        return;
    }

    info!("User triggered app update");
    if cx.has_global::<AppNotifications>() {
        cx.global_mut::<AppNotifications>().0.push(
            AppNotification::new(
                NotificationType::Info,
                "Downloading update... The app will need to restart when complete.",
            )
            .with_title("Updating Hive"),
        );
    }

    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let result = updater.install_update();
        let _ = tx.send(result);
    });

    cx.spawn(async move |_entity, app: &mut AsyncApp| {
        loop {
            match rx.try_recv() {
                Ok(result) => {
                    let _ = app.update(|cx| match result {
                        Ok(_path) => {
                            if cx.has_global::<AppNotifications>() {
                                cx.global_mut::<AppNotifications>().0.push(
                                    AppNotification::new(
                                        NotificationType::Info,
                                        "Update installed! Please restart Hive to use the new version.",
                                    )
                                    .with_title("Update Complete"),
                                );
                            }
                            info!("Update installed successfully - restart needed");
                        }
                        Err(e) => {
                            error!("Update installation failed: {e}");
                            if cx.has_global::<AppNotifications>() {
                                cx.global_mut::<AppNotifications>().0.push(
                                    AppNotification::new(
                                        NotificationType::Error,
                                        format!("Update failed: {e}. You can update manually with: brew upgrade hive"),
                                    )
                                    .with_title("Update Failed"),
                                );
                            }
                        }
                    });
                    return;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    app.background_executor()
                        .timer(std::time::Duration::from_millis(500))
                        .await;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => return,
            }
        }
    })
    .detach();
}

pub(super) fn handle_ollama_pull_model(
    _workspace: &mut HiveWorkspace,
    action: &OllamaPullModel,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    if !cx.has_global::<AppOllamaManager>() {
        return;
    }
    let ollama = cx.global::<AppOllamaManager>().0.clone();
    let model = action.model.clone();
    info!("Ollama: pulling model '{model}'");

    std::thread::Builder::new()
        .name("ollama-pull".into())
        .spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build();
            let Ok(runtime) = runtime else { return };
            runtime.block_on(async {
                let (tx, mut rx) = tokio::sync::mpsc::channel(64);
                let model_clone = model.clone();
                let pull_task =
                    tokio::spawn(async move { ollama.pull_model(&model_clone, tx).await });
                while let Some(update) = rx.recv().await {
                    tracing::debug!("Ollama pull progress: {update:?}");
                }
                match pull_task.await {
                    Ok(Ok(())) => info!("Ollama: model '{model}' pulled successfully"),
                    Ok(Err(e)) => warn!("Ollama: pull failed for '{model}': {e}"),
                    Err(e) => warn!("Ollama: pull task panicked for '{model}': {e}"),
                }
            });
        })
        .ok();
}

pub(super) fn handle_ollama_delete_model(
    _workspace: &mut HiveWorkspace,
    action: &OllamaDeleteModel,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    if !cx.has_global::<AppOllamaManager>() {
        return;
    }
    let ollama = cx.global::<AppOllamaManager>().0.clone();
    let model = action.model.clone();
    info!("Ollama: deleting model '{model}'");

    std::thread::Builder::new()
        .name("ollama-delete".into())
        .spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build();
            let Ok(runtime) = runtime else { return };
            runtime.block_on(async {
                match ollama.delete_model(&model).await {
                    Ok(()) => info!("Ollama: model '{model}' deleted successfully"),
                    Err(e) => warn!("Ollama: delete failed for '{model}': {e}"),
                }
            });
        })
        .ok();
}

pub(super) fn handle_toggle_disclosure(
    workspace: &mut HiveWorkspace,
    action: &ToggleDisclosure,
    _window: &mut Window,
    _cx: &mut Context<HiveWorkspace>,
) {
    workspace
        .cached_chat_data
        .toggle_disclosure(action.message_index);
}
