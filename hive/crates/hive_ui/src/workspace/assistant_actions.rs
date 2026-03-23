use gpui::*;
use tracing::info;

use super::{
    AppAssistant, AppConfig, CheckCalendar, CheckEmail, DailyBriefing, HiveWorkspace,
    NotificationType,
};

/// Check whether any Google or Microsoft account is connected.
fn has_connected_account(cx: &mut Context<HiveWorkspace>) -> bool {
    if !cx.has_global::<AppConfig>() {
        return false;
    }
    let config = cx.global::<AppConfig>().0.get();
    config.connected_accounts.iter().any(|a| {
        matches!(
            a.platform,
            hive_core::config::AccountPlatform::Google
                | hive_core::config::AccountPlatform::Microsoft
        )
    })
}

pub(super) fn handle_daily_briefing(
    workspace: &mut HiveWorkspace,
    _action: &DailyBriefing,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    info!("Assistant: daily briefing requested");

    if !cx.has_global::<AppAssistant>() {
        workspace.push_notification(
            cx,
            NotificationType::Warning,
            "Assistant",
            "Assistant service is not available.",
        );
        return;
    }

    if !has_connected_account(cx) {
        workspace.push_notification(
            cx,
            NotificationType::Warning,
            "Assistant",
            "No Google or Microsoft account connected. Go to Settings \u{2192} Connected Accounts to set up OAuth.",
        );
    }

    let briefing = cx
        .global::<AppAssistant>()
        .0
        .daily_briefing_for_project(Some(&workspace.current_project_root));

    let mut lines = vec![format!("**Daily Briefing** \u{2014} {}", briefing.date)];

    if briefing.events.is_empty() {
        lines.push("No calendar events today.".into());
    } else {
        lines.push(format!("**Calendar** ({} events):", briefing.events.len()));
        for event in &briefing.events {
            let loc = event
                .location
                .as_deref()
                .map(|l| format!(" @ {l}"))
                .unwrap_or_default();
            lines.push(format!(
                "- {} ({} \u{2013} {}){loc}",
                event.title, event.start, event.end
            ));
        }
    }

    if let Some(digest) = &briefing.email_summary {
        lines.push(format!(
            "**Email** ({} messages): {}",
            digest.email_count, digest.summary
        ));
    } else {
        lines.push("No new emails.".into());
    }

    if briefing.active_reminders.is_empty() {
        lines.push("No active reminders.".into());
    } else {
        lines.push(format!(
            "**Reminders** ({}):",
            briefing.active_reminders.len()
        ));
        for reminder in &briefing.active_reminders {
            lines.push(format!("- {}", reminder.title));
        }
    }

    if !briefing.action_items.is_empty() {
        lines.push("**Action Items**:".into());
        for item in &briefing.action_items {
            lines.push(format!("- {item}"));
        }
    }

    let content = lines.join("\n");
    workspace.chat_service.update(cx, |svc, cx| {
        svc.push_system_message(content, cx);
    });
    cx.notify();
}

pub(super) fn handle_check_email(
    workspace: &mut HiveWorkspace,
    _action: &CheckEmail,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    info!("Assistant: check email requested");

    if !cx.has_global::<AppAssistant>() {
        workspace.push_notification(
            cx,
            NotificationType::Warning,
            "Assistant",
            "Assistant service is not available.",
        );
        return;
    }

    if !has_connected_account(cx) {
        workspace.push_notification(
            cx,
            NotificationType::Warning,
            "Assistant",
            "No Google or Microsoft account connected. Go to Settings \u{2192} Connected Accounts to set up OAuth.",
        );
        return;
    }

    let svc = &cx.global::<AppAssistant>().0;
    let gmail = svc.fetch_gmail_emails().unwrap_or_default();
    let outlook = svc.fetch_outlook_emails().unwrap_or_default();

    if gmail.is_empty() && outlook.is_empty() {
        workspace.chat_service.update(cx, |svc, cx| {
            svc.push_system_message("**Email Check**: No new emails found.", cx);
        });
        cx.notify();
        return;
    }

    let mut lines = vec!["**Email Inbox**".to_string()];

    if !gmail.is_empty() {
        lines.push(format!("**Gmail** ({} messages):", gmail.len()));
        for email in gmail.iter().take(10) {
            let flag = if email.important { " \u{2757}" } else { "" };
            lines.push(format!("- **{}**: {}{flag}", email.from, email.subject));
        }
        if gmail.len() > 10 {
            lines.push(format!("  _...and {} more_", gmail.len() - 10));
        }
    }

    if !outlook.is_empty() {
        lines.push(format!("**Outlook** ({} messages):", outlook.len()));
        for email in outlook.iter().take(10) {
            let flag = if email.important { " \u{2757}" } else { "" };
            lines.push(format!("- **{}**: {}{flag}", email.from, email.subject));
        }
        if outlook.len() > 10 {
            lines.push(format!("  _...and {} more_", outlook.len() - 10));
        }
    }

    let content = lines.join("\n");
    workspace.chat_service.update(cx, |svc, cx| {
        svc.push_system_message(content, cx);
    });
    cx.notify();
}

pub(super) fn handle_check_calendar(
    workspace: &mut HiveWorkspace,
    _action: &CheckCalendar,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    info!("Assistant: check calendar requested");

    if !cx.has_global::<AppAssistant>() {
        workspace.push_notification(
            cx,
            NotificationType::Warning,
            "Assistant",
            "Assistant service is not available.",
        );
        return;
    }

    if !has_connected_account(cx) {
        workspace.push_notification(
            cx,
            NotificationType::Warning,
            "Assistant",
            "No Google or Microsoft account connected. Go to Settings \u{2192} Connected Accounts to set up OAuth.",
        );
        return;
    }

    let events = cx
        .global::<AppAssistant>()
        .0
        .fetch_today_events()
        .unwrap_or_default();

    if events.is_empty() {
        workspace.chat_service.update(cx, |svc, cx| {
            svc.push_system_message("**Calendar**: No upcoming events today.", cx);
        });
        cx.notify();
        return;
    }

    let mut lines = vec![format!("**Today's Calendar** ({} events):", events.len())];
    for event in &events {
        let loc = event
            .location
            .as_deref()
            .map(|l| format!(" @ {l}"))
            .unwrap_or_default();
        let attendees = if event.attendees.is_empty() {
            String::new()
        } else {
            format!(" ({})", event.attendees.join(", "))
        };
        lines.push(format!(
            "- **{}** ({} \u{2013} {}){loc}{attendees}",
            event.title, event.start, event.end
        ));
    }

    let content = lines.join("\n");
    workspace.chat_service.update(cx, |svc, cx| {
        svc.push_system_message(content, cx);
    });
    cx.notify();
}
