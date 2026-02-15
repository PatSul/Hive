use gpui_component::IconName;
use hive_ui_panels::panels::agents::{AgentsPanelData, PersonaDisplay, RunDisplay};

#[test]
fn persona_icon_mapping() {
    let p = PersonaDisplay {
        name: "Investigator".into(),
        kind: "investigate".into(),
        description: String::new(),
        model_tier: String::new(),
        active: true,
    };
    assert!(matches!(p.icon(), IconName::Search));
}

#[test]
fn persona_icon_unknown_kind() {
    let p = PersonaDisplay {
        name: "Custom".into(),
        kind: "custom_thing".into(),
        description: String::new(),
        model_tier: String::new(),
        active: false,
    };
    assert!(matches!(p.icon(), IconName::Bot));
}

#[test]
fn run_display_is_active() {
    let running = RunDisplay {
        id: "r1".into(),
        spec_title: "Test".into(),
        status: "Running".into(),
        progress: 0.5,
        tasks_done: 3,
        tasks_total: 6,
        cost: 0.1,
        elapsed: "1m".into(),
    };
    assert!(running.is_active());

    let pending = RunDisplay {
        id: "r2".into(),
        spec_title: "Test".into(),
        status: "Pending".into(),
        progress: 0.0,
        tasks_done: 0,
        tasks_total: 4,
        cost: 0.0,
        elapsed: "0s".into(),
    };
    assert!(pending.is_active());

    let complete = RunDisplay {
        id: "r3".into(),
        spec_title: "Test".into(),
        status: "Complete".into(),
        progress: 1.0,
        tasks_done: 4,
        tasks_total: 4,
        cost: 0.2,
        elapsed: "2m".into(),
    };
    assert!(!complete.is_active());
}

#[test]
fn agents_panel_data_empty() {
    let data = AgentsPanelData::empty();
    assert!(data.personas.is_empty());
    assert!(data.workflows.is_empty());
    assert!(data.active_runs.is_empty());
    assert!(data.run_history.is_empty());
    assert_eq!(data.workflow_source_dir, ".hive/workflows");
}

#[test]
fn agents_panel_data_sample() {
    let data = AgentsPanelData::sample();
    assert_eq!(data.personas.len(), 6);
    assert_eq!(data.workflows.len(), 2);
    assert_eq!(data.active_runs.len(), 1);
    assert_eq!(data.run_history.len(), 1);
}
