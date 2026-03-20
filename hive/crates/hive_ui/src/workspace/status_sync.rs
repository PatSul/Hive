use std::sync::Arc;

use gpui::*;
use tracing::info;

use super::{
    network_actions, project_context, AppAgentNotifications, AppAiService, AppNotification,
    AppNotifications, AppReminderRx, AppUpdater, ConnectivityDisplay, HiveWorkspace,
    NotificationType, Panel, SystemResources,
};

/// Sync status bar with current chat service state.
/// NOTE: This runs on every render frame and must stay cheap.
pub(super) fn sync_status_bar(
    workspace: &mut HiveWorkspace,
    window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    // Read all state from the chat service first, then release the borrow.
    let (model, is_streaming, total, current_conv_id) = {
        let svc = workspace.chat_service.read(cx);
        let model = svc.current_model().to_string();
        let streaming = svc.is_streaming();
        let total: f64 = svc.messages().iter().filter_map(|m| m.cost).sum();
        let conv_id = svc.conversation_id().map(String::from);
        (model, streaming, total, conv_id)
    };

    workspace.status_bar.active_project = project_context::project_label(workspace);

    workspace.status_bar.current_model = if model.is_empty() {
        "Select Model".to_string()
    } else {
        model
    };
    workspace.status_bar.total_cost = total;

    // Sync the chat input disabled state with streaming status.
    workspace.chat_input.update(cx, |input, cx| {
        input.set_sending(is_streaming, window, cx);
    });

    // Detect conversation ID changes (e.g. after stream finalization auto-saves
    // a conversation and assigns an ID for the first time).
    if current_conv_id != workspace.last_saved_conversation_id {
        workspace.session_dirty = true;
        // Save session on actual state change - not every frame.
        workspace.save_session(cx);
    }

    // Auto-update: check if the updater has found a newer version.
    if cx.has_global::<AppUpdater>() {
        let info = cx.global::<AppUpdater>().0.available_update();
        workspace.status_bar.update_available = info.map(|i| i.version);
    }

    // Notification tray: sync from agent notification service.
    if cx.has_global::<AppAgentNotifications>() {
        let svc = &cx.global::<AppAgentNotifications>().0;
        workspace.status_bar.notification_tray.unread_count = svc.unread_count();
        let all = svc.all();

        // Surface new notifications as toast overlays.
        for n in &all {
            if !n.read && !workspace.seen_notification_ids.contains(&n.id) {
                let toast_kind = match n.kind {
                    hive_agents::activity::notification::NotificationKind::AgentCompleted => {
                        hive_ui_panels::components::toast::ToastKind::Success
                    }
                    hive_agents::activity::notification::NotificationKind::AgentFailed => {
                        hive_ui_panels::components::toast::ToastKind::Error
                    }
                    hive_agents::activity::notification::NotificationKind::BudgetWarning
                    | hive_agents::activity::notification::NotificationKind::ApprovalRequest => {
                        hive_ui_panels::components::toast::ToastKind::Warning
                    }
                    hive_agents::activity::notification::NotificationKind::BudgetExhausted => {
                        hive_ui_panels::components::toast::ToastKind::Error
                    }
                    hive_agents::activity::notification::NotificationKind::HeartbeatReport => {
                        hive_ui_panels::components::toast::ToastKind::Info
                    }
                };
                workspace.toast_messages.push((
                    n.id.clone(),
                    n.summary.clone(),
                    toast_kind,
                    std::time::Instant::now(),
                ));
                workspace.seen_notification_ids.insert(n.id.clone());
            }
        }

        workspace.status_bar.notification_tray.notifications = all;
    }

    // Auto-dismiss toasts older than 5 seconds.
    let now = std::time::Instant::now();
    workspace
        .toast_messages
        .retain(|(_id, _msg, _kind, created)| now.duration_since(*created).as_secs() < 5);

    // Discovery: periodic scan + connectivity update.
    maybe_trigger_discovery_scan(workspace, cx);
    sync_connectivity(workspace, cx);

    // Drain triggered reminders from the tick driver.
    if cx.has_global::<AppReminderRx>() {
        let rx_arc = Arc::clone(&cx.global::<AppReminderRx>().0);
        let mut pending = Vec::new();
        if let Ok(rx) = rx_arc.lock() {
            while let Ok(reminders) = rx.try_recv() {
                pending.extend(reminders);
            }
        }
        for reminder in &pending {
            info!("UI received reminder: {}", reminder.title);
            if cx.has_global::<AppNotifications>() {
                cx.global_mut::<AppNotifications>().0.push(
                    AppNotification::new(
                        NotificationType::Info,
                        format!("Reminder: {}", reminder.title),
                    )
                    .with_title("Reminder"),
                );
            }
        }
    }

    // Auto-refresh network peer data every 30 seconds when panel is active.
    if workspace.sidebar.active_panel == Panel::Network {
        let should_refresh = match workspace.last_network_refresh {
            None => true,
            Some(t) => t.elapsed() >= std::time::Duration::from_secs(30),
        };
        if should_refresh {
            workspace.last_network_refresh = Some(std::time::Instant::now());
            network_actions::refresh_network_peer_data(workspace, cx);
        }
    }
}

/// Trigger a discovery scan every 30 seconds (non-blocking).
///
/// Runs the actual HTTP probing on a background OS thread with its own Tokio
/// runtime (reqwest requires Tokio, but GPUI uses a smol-based executor).
/// On the next `sync_status_bar()` tick the completion flag is checked and the
/// UI is updated with any newly discovered models.
fn maybe_trigger_discovery_scan(workspace: &mut HiveWorkspace, cx: &mut Context<HiveWorkspace>) {
    // Check if a previous scan just finished.
    if workspace.discovery_scan_pending {
        if let Some(flag) = &workspace.discovery_done_flag
            && flag.load(std::sync::atomic::Ordering::Acquire)
        {
            workspace.discovery_scan_pending = false;
            workspace.discovery_done_flag = None;
            // Refresh UI with discovered models.
            if cx.has_global::<AppAiService>()
                && let Some(d) = cx.global::<AppAiService>().0.discovery()
            {
                let models = d.snapshot().all_models();
                workspace.settings_view.update(cx, |settings, cx| {
                    settings.refresh_local_models(models.clone(), cx);
                });
                workspace.models_browser_view.update(cx, |browser, cx| {
                    browser.set_local_models(models, cx);
                });
            }
            cx.notify();
        }
        return;
    }

    let should_scan = match workspace.last_discovery_scan {
        None => true,
        Some(t) => t.elapsed() >= std::time::Duration::from_secs(30),
    };
    if !should_scan {
        return;
    }

    let discovery = if cx.has_global::<AppAiService>() {
        cx.global::<AppAiService>().0.discovery().cloned()
    } else {
        None
    };

    let Some(discovery) = discovery else { return };

    workspace.discovery_scan_pending = true;
    workspace.last_discovery_scan = Some(std::time::Instant::now());

    let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
    workspace.discovery_done_flag = Some(Arc::clone(&done));

    std::thread::spawn(move || {
        discovery.scan_all_blocking();
        done.store(true, std::sync::atomic::Ordering::Release);
    });
}

/// Update status bar connectivity based on registered + discovered providers.
fn sync_connectivity(workspace: &mut HiveWorkspace, cx: &App) {
    if !cx.has_global::<AppAiService>() {
        return;
    }
    let ai = &cx.global::<AppAiService>().0;
    let has_cloud = ai.available_providers().iter().any(|p| {
        matches!(
            p,
            hive_ai::types::ProviderType::Anthropic
                | hive_ai::types::ProviderType::OpenAI
                | hive_ai::types::ProviderType::OpenRouter
                | hive_ai::types::ProviderType::Google
                | hive_ai::types::ProviderType::Groq
                | hive_ai::types::ProviderType::HuggingFace
        )
    });
    let has_local = ai
        .discovery()
        .map(|d| d.snapshot().any_online())
        .unwrap_or(false);

    workspace.status_bar.connectivity = match (has_cloud, has_local) {
        (true, _) => ConnectivityDisplay::Online,
        (false, true) => ConnectivityDisplay::LocalOnly,
        (false, false) => ConnectivityDisplay::Offline,
    };
}

/// Read system resource metrics (CPU, memory, disk) using macOS-friendly
/// commands and stdlib APIs.
pub(super) fn gather_system_resources(workspace: &HiveWorkspace) -> SystemResources {
    let mut res = SystemResources {
        cpu_percent: 0.0,
        memory_used: 0,
        memory_total: 0,
        disk_used: 0,
        disk_total: 0,
    };

    // Total physical memory via sysctl (macOS).
    if let Ok(output) = std::process::Command::new("sysctl")
        .args(["-n", "hw.memsize"])
        .output()
        && output.status.success()
    {
        let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
        res.memory_total = s.parse::<u64>().unwrap_or(0);
    }

    // Process memory (resident set size in KB) via ps.
    if let Ok(output) = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        && output.status.success()
    {
        let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
        // ps reports RSS in kilobytes.
        res.memory_used = s.parse::<u64>().unwrap_or(0) * 1024;
    }

    // CPU usage: use `ps -o %cpu=` for this process as a quick estimate.
    if let Ok(output) = std::process::Command::new("ps")
        .args(["-o", "%cpu=", "-p", &std::process::id().to_string()])
        .output()
        && output.status.success()
    {
        let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
        res.cpu_percent = s.parse::<f64>().unwrap_or(0.0);
    }

    // Disk usage for the project directory via df.
    if let Ok(output) = std::process::Command::new("df")
        .args(["-k", &workspace.current_project_root.to_string_lossy()])
        .output()
        && output.status.success()
    {
        let text = String::from_utf8_lossy(&output.stdout);
        if let Some(data_line) = text.lines().nth(1) {
            let cols: Vec<&str> = data_line.split_whitespace().collect();
            // df -k columns: Filesystem 1K-blocks Used Available Capacity ...
            if cols.len() >= 4 {
                let total_kb = cols[1].parse::<u64>().unwrap_or(0);
                let used_kb = cols[2].parse::<u64>().unwrap_or(0);
                res.disk_total = total_kb * 1024;
                res.disk_used = used_kb * 1024;
            }
        }
    }

    res
}
