use hive_ui_core::HiveTheme;
use hive_ui_panels::panels::shield::{PolicyDisplay, ShieldEvent, ShieldPanelData};

#[test]
fn shield_event_severity_colors() {
    let theme = HiveTheme::dark();
    let event = ShieldEvent {
        timestamp: String::new(),
        event_type: String::new(),
        severity: "high".into(),
        detail: String::new(),
    };
    assert_eq!(event.severity_color(&theme), theme.accent_red);

    let event_low = ShieldEvent {
        severity: "low".into(),
        ..event.clone()
    };
    assert_eq!(event_low.severity_color(&theme), theme.accent_cyan);

    let event_medium = ShieldEvent {
        severity: "medium".into(),
        ..event.clone()
    };
    assert_eq!(event_medium.severity_color(&theme), theme.accent_yellow);

    let event_unknown = ShieldEvent {
        severity: "unknown".into(),
        ..event
    };
    assert_eq!(event_unknown.severity_color(&theme), theme.text_muted);
}

#[test]
fn shield_panel_data_empty() {
    let data = ShieldPanelData::empty();
    assert!(data.enabled);
    assert_eq!(data.pii_detections, 0);
    assert_eq!(data.secrets_blocked, 0);
    assert_eq!(data.threats_caught, 0);
    assert!(data.recent_events.is_empty());
    assert!(data.policies.is_empty());
}

#[test]
fn shield_panel_data_sample() {
    let data = ShieldPanelData::sample();
    assert!(data.enabled);
    assert_eq!(data.pii_detections, 14);
    assert_eq!(data.secrets_blocked, 3);
    assert_eq!(data.threats_caught, 1);
    assert_eq!(data.recent_events.len(), 4);
    assert_eq!(data.policies.len(), 4);
}

#[test]
fn policy_display_fields() {
    let policy = PolicyDisplay {
        provider: "Anthropic".into(),
        trust_level: "High".into(),
        max_classification: "Confidential".into(),
        pii_cloaking: true,
    };
    assert_eq!(policy.provider, "Anthropic");
    assert!(policy.pii_cloaking);
}
