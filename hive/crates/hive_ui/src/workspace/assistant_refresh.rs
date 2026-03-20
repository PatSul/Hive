use chrono::Utc;
use gpui::*;
use hive_assistant::ReminderTrigger;
use tracing::warn;

use super::{AppAssistant, AppConfig, HiveWorkspace};

#[derive(Debug)]
struct EmailPreviewData {
    from: String,
    subject: String,
    snippet: String,
    time: String,
    important: bool,
}

#[derive(Debug)]
struct EventData {
    title: String,
    time: String,
    location: Option<String>,
}

#[derive(Debug)]
enum AssistantFetchResult {
    Emails {
        provider: String,
        previews: Vec<EmailPreviewData>,
    },
    Events(Vec<EventData>),
    RecentActions(Vec<String>),
}

pub(super) fn refresh_assistant_data(workspace: &mut HiveWorkspace, cx: &App) {
    use hive_ui_panels::panels::assistant::{ActiveReminder, BriefingSummary, PendingApproval};

    if cx.has_global::<AppAssistant>() {
        let svc = &cx.global::<AppAssistant>().0;
        let briefing = svc.daily_briefing_for_project(Some(&workspace.current_project_root));

        workspace.assistant_data.briefing = Some(BriefingSummary {
            greeting: "Good morning!".into(),
            date: briefing.date.clone(),
            event_count: briefing.events.len(),
            unread_emails: briefing.email_summary.as_ref().map_or(0, |d| d.email_count),
            active_reminders: briefing.active_reminders.len(),
            top_priority: briefing.action_items.first().cloned(),
        });

        workspace.assistant_data.reminders = briefing
            .active_reminders
            .iter()
            .map(|reminder| ActiveReminder {
                title: reminder.title.clone(),
                due: match &reminder.trigger {
                    ReminderTrigger::At(at) => at.format("%Y-%m-%d %H:%M").to_string(),
                    ReminderTrigger::Recurring(expr) => format!("Recurring: {expr}"),
                    ReminderTrigger::OnEvent(event) => format!("On event: {event}"),
                },
                is_overdue: matches!(
                    &reminder.trigger,
                    ReminderTrigger::At(at) if *at <= Utc::now()
                ),
            })
            .collect();

        if let Ok(pending) = svc.approval_service.list_pending() {
            workspace.assistant_data.approvals = pending
                .iter()
                .map(|approval| PendingApproval {
                    id: approval.id.clone(),
                    action: approval.action.clone(),
                    resource: approval.resource.clone(),
                    level: format!("{:?}", approval.level),
                    requested_by: approval.requested_by.clone(),
                    created_at: approval.created_at.clone(),
                })
                .collect();
        }
    }
}

pub(super) fn refresh_assistant_connected_data(
    _workspace: &mut HiveWorkspace,
    cx: &mut Context<HiveWorkspace>,
) {
    use hive_assistant::calendar::CalendarService;
    use hive_assistant::email::EmailService;
    use hive_core::config::AccountPlatform;
    use hive_ui_panels::panels::assistant::{EmailGroup, EmailPreview, UpcomingEvent};

    if !cx.has_global::<AppConfig>() {
        return;
    }

    let config = cx.global::<AppConfig>().0.get();
    let connected = config.connected_accounts.clone();
    if connected.is_empty() {
        return;
    }

    let mut tokens: Vec<(AccountPlatform, String)> = Vec::new();
    let config_mgr = &cx.global::<AppConfig>().0;
    for account in &connected {
        if let Some(token_data) = config_mgr.get_oauth_token(account.platform) {
            tokens.push((account.platform, token_data.access_token.clone()));
        }
    }

    if tokens.is_empty() {
        return;
    }

    if cx.has_global::<AppAssistant>() {
        let svc = &mut cx.global_mut::<AppAssistant>().0;
        for (platform, token) in &tokens {
            match platform {
                AccountPlatform::Google => {
                    svc.set_gmail_token(token.clone());
                    svc.set_google_calendar_token(token.clone());
                }
                AccountPlatform::Microsoft => {
                    svc.set_outlook_token(token.clone());
                    svc.set_outlook_calendar_token(token.clone());
                }
                _ => {}
            }
        }
    }

    let mut gmail_token: Option<String> = None;
    let mut outlook_token: Option<String> = None;
    let mut github_tokens: Vec<String> = Vec::new();

    for (platform, token) in &tokens {
        match platform {
            AccountPlatform::Google => gmail_token = Some(token.clone()),
            AccountPlatform::Microsoft => outlook_token = Some(token.clone()),
            AccountPlatform::GitHub => github_tokens.push(token.clone()),
            _ => {}
        }
    }

    let email_svc = EmailService::with_tokens(gmail_token.clone(), outlook_token.clone());
    let calendar_svc = CalendarService::with_tokens(gmail_token, outlook_token);
    let (tx, rx) = std::sync::mpsc::channel::<AssistantFetchResult>();

    std::thread::spawn(move || {
        let rt = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt,
            Err(e) => {
                warn!("Assistant: failed to create tokio runtime: {e}");
                return;
            }
        };

        let _guard = rt.enter();

        if let Ok(emails) = email_svc.fetch_gmail_inbox()
            && !emails.is_empty()
        {
            let previews: Vec<EmailPreviewData> = emails
                .iter()
                .map(|email| EmailPreviewData {
                    from: email.from.clone(),
                    subject: email.subject.clone(),
                    snippet: email.body.chars().take(120).collect(),
                    time: email.timestamp.clone(),
                    important: email.important,
                })
                .collect();
            let _ = tx.send(AssistantFetchResult::Emails {
                provider: "Gmail".into(),
                previews,
            });
        }

        if let Ok(events) = calendar_svc.today_events()
            && !events.is_empty()
        {
            let upcoming: Vec<EventData> = events
                .iter()
                .map(|event| EventData {
                    title: event.title.clone(),
                    time: event.start.clone(),
                    location: event.location.clone(),
                })
                .collect();
            let _ = tx.send(AssistantFetchResult::Events(upcoming));
        }

        if let Ok(emails) = email_svc.fetch_outlook_inbox()
            && !emails.is_empty()
        {
            let previews: Vec<EmailPreviewData> = emails
                .iter()
                .map(|email| EmailPreviewData {
                    from: email.from.clone(),
                    subject: email.subject.clone(),
                    snippet: email.body.chars().take(120).collect(),
                    time: email.timestamp.clone(),
                    important: email.important,
                })
                .collect();
            let _ = tx.send(AssistantFetchResult::Emails {
                provider: "Outlook".into(),
                previews,
            });
        }

        for token in &github_tokens {
            let client = match hive_integrations::GitHubClient::new(token.clone()) {
                Ok(client) => client,
                Err(_) => continue,
            };
            if let Ok(repos) = rt.block_on(client.list_repos())
                && let Some(arr) = repos.as_array()
            {
                let descriptions: Vec<String> = arr
                    .iter()
                    .take(5)
                    .filter_map(|repo| {
                        let name = repo.get("full_name")?.as_str()?;
                        Some(format!("Activity on {name}"))
                    })
                    .collect();
                let _ = tx.send(AssistantFetchResult::RecentActions(descriptions));
            }
        }
    });

    cx.spawn(
        async move |entity: WeakEntity<HiveWorkspace>, async_cx: &mut AsyncApp| {
            Timer::after(std::time::Duration::from_secs(3)).await;
            let mut email_groups: Vec<(String, Vec<EmailPreviewData>)> = Vec::new();
            let mut events: Vec<EventData> = Vec::new();
            let mut actions: Vec<String> = Vec::new();

            while let Ok(result) = rx.try_recv() {
                match result {
                    AssistantFetchResult::Emails { provider, previews } => {
                        email_groups.push((provider, previews));
                    }
                    AssistantFetchResult::Events(new_events) => {
                        events.extend(new_events);
                    }
                    AssistantFetchResult::RecentActions(new_actions) => {
                        actions.extend(new_actions);
                    }
                }
            }

            let _ = entity.update(
                async_cx,
                |workspace: &mut HiveWorkspace, cx: &mut Context<HiveWorkspace>| {
                    for (provider, previews) in &email_groups {
                        workspace.assistant_data.email_groups.push(EmailGroup {
                            provider: provider.clone(),
                            previews: previews
                                .iter()
                                .map(|preview| EmailPreview {
                                    from: preview.from.clone(),
                                    subject: preview.subject.clone(),
                                    snippet: preview.snippet.clone(),
                                    time: preview.time.clone(),
                                    important: preview.important,
                                })
                                .collect(),
                        });
                    }

                    for event in &events {
                        workspace.assistant_data.events.push(UpcomingEvent {
                            title: event.title.clone(),
                            time: event.time.clone(),
                            location: event.location.clone(),
                            is_conflict: false,
                        });
                    }

                    for action in &actions {
                        workspace.assistant_data.recent_actions.push(
                            hive_ui_panels::panels::assistant::RecentAction {
                                description: action.clone(),
                                timestamp: "Now".into(),
                                action_type: "github".into(),
                            },
                        );
                    }

                    if let Some(ref mut briefing) = workspace.assistant_data.briefing {
                        let total_emails: usize =
                            email_groups.iter().map(|(_, previews)| previews.len()).sum();
                        briefing.unread_emails = total_emails;
                        briefing.event_count = workspace.assistant_data.events.len();
                    }

                    cx.notify();
                },
            );
        },
    )
    .detach();
}
