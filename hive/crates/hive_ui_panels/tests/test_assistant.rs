use hive_ui_panels::panels::assistant::{
    ActiveReminder, AssistantPanelData, BriefingSummary, EmailPreview, RecentAction,
    ResearchProgress, UpcomingEvent,
};

#[test]
fn assistant_panel_data_empty() {
    let data = AssistantPanelData::empty();
    assert!(data.briefing.is_none());
    assert!(data.events.is_empty());
    assert!(data.email_groups.is_empty());
    assert!(data.reminders.is_empty());
    assert!(data.research.is_empty());
    assert!(data.recent_actions.is_empty());
}

#[test]
fn assistant_panel_data_sample() {
    let data = AssistantPanelData::sample();
    assert!(data.briefing.is_some());
    assert_eq!(data.events.len(), 3);
    assert_eq!(data.email_groups.len(), 1);
    assert_eq!(data.reminders.len(), 2);
    assert_eq!(data.research.len(), 1);
    assert_eq!(data.recent_actions.len(), 2);
}

#[test]
fn briefing_summary_fields() {
    let b = BriefingSummary {
        greeting: "Good morning".into(),
        date: "Monday".into(),
        event_count: 3,
        unread_emails: 5,
        active_reminders: 1,
        top_priority: Some("Meeting".into()),
    };
    assert_eq!(b.event_count, 3);
    assert_eq!(b.top_priority.as_deref(), Some("Meeting"));
}

#[test]
fn upcoming_event_conflict() {
    let event = UpcomingEvent {
        title: "Meeting".into(),
        time: "10:00".into(),
        location: None,
        is_conflict: true,
    };
    assert!(event.is_conflict);
}

#[test]
fn email_preview_importance() {
    let preview = EmailPreview {
        from: "test@test.com".into(),
        subject: "Test".into(),
        snippet: "...".into(),
        time: "9:00".into(),
        important: true,
    };
    assert!(preview.important);
}

#[test]
fn active_reminder_overdue() {
    let r = ActiveReminder {
        title: "Task".into(),
        due: "Yesterday".into(),
        is_overdue: true,
    };
    assert!(r.is_overdue);
}

#[test]
fn research_progress_bounds() {
    let rp = ResearchProgress {
        topic: "Test".into(),
        status: "In progress".into(),
        progress_pct: 50,
    };
    assert!(rp.progress_pct <= 100);
}

#[test]
fn recent_action_types() {
    let a = RecentAction {
        description: "Drafted email".into(),
        timestamp: "5m ago".into(),
        action_type: "email".into(),
    };
    assert_eq!(a.action_type, "email");
}
