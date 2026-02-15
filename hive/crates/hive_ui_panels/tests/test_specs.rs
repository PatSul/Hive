use hive_ui_panels::panels::specs::{SpecPanelData, SpecSummary, SpecViewMode};

#[test]
fn spec_summary_progress_empty() {
    let spec = SpecSummary {
        id: "s1".into(),
        title: "Test".into(),
        status: "Draft".into(),
        entries_total: 0,
        entries_checked: 0,
        updated_at: "now".into(),
    };
    assert_eq!(spec.progress(), 0.0);
}

#[test]
fn spec_summary_progress_partial() {
    let spec = SpecSummary {
        id: "s2".into(),
        title: "Test".into(),
        status: "In Progress".into(),
        entries_total: 10,
        entries_checked: 3,
        updated_at: "now".into(),
    };
    let p = spec.progress();
    assert!((p - 0.3).abs() < f32::EPSILON);
}

#[test]
fn spec_summary_progress_complete() {
    let spec = SpecSummary {
        id: "s3".into(),
        title: "Test".into(),
        status: "Complete".into(),
        entries_total: 5,
        entries_checked: 5,
        updated_at: "now".into(),
    };
    assert_eq!(spec.progress(), 1.0);
}

#[test]
fn spec_panel_data_empty() {
    let data = SpecPanelData::empty();
    assert!(data.specs.is_empty());
    assert_eq!(data.view_mode, SpecViewMode::List);
    assert!(data.active_spec_id.is_none());
}

#[test]
fn spec_panel_data_sample_has_specs() {
    let data = SpecPanelData::sample();
    assert_eq!(data.specs.len(), 3);
}

#[test]
fn spec_panel_data_active_spec_lookup() {
    let mut data = SpecPanelData::sample();
    assert!(data.active_spec().is_none());

    data.active_spec_id = Some("spec-002".into());
    let spec = data.active_spec().expect("should find spec-002");
    assert_eq!(spec.title, "API Rate Limiting");
}

#[test]
fn spec_panel_data_active_spec_missing() {
    let mut data = SpecPanelData::sample();
    data.active_spec_id = Some("nonexistent".into());
    assert!(data.active_spec().is_none());
}
