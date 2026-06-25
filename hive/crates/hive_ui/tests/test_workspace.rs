use hive_ui::sidebar::{Panel, Sidebar};
use hive_ui::workspace::*;

#[test]
fn test_panel_switch() {
    let mut sidebar = Sidebar::new();
    assert_eq!(sidebar.active_panel, Panel::Chat);
    sidebar.active_panel = Panel::Files;
    assert_eq!(sidebar.active_panel, Panel::Files);
}

#[test]
fn test_history_data_fallback() {
    let data = HiveWorkspace::load_history_data();
    assert!(data.search_query.is_empty());
}

#[test]
fn test_set_active_panel_all_panels() {
    let mut sidebar = Sidebar::new();
    for panel in Panel::ALL {
        sidebar.active_panel = panel;
        assert_eq!(sidebar.active_panel, panel);
    }
}

/// Verify ctrl-1..ctrl-9,ctrl-0 keyboard mapping matches the visible shell
/// order used by `Panel::from_shortcut_index`.
#[test]
fn test_keyboard_panel_mapping() {
    let expected: [(usize, Panel); 10] = [
        (1, Panel::QuickStart),
        (2, Panel::Chat),
        (3, Panel::Files),
        (4, Panel::History),
        (5, Panel::Specs),
        (6, Panel::Agents),
        (7, Panel::Workflows),
        (8, Panel::Kanban),
        (9, Panel::Activity),
        (0, Panel::Settings),
    ];
    for (key, expected_panel) in expected {
        let idx = if key == 0 { 9 } else { key - 1 };
        let panel = Panel::from_shortcut_index(idx).expect("panel should exist");
        assert_eq!(panel, expected_panel, "ctrl-{key} mismatch");
    }
}

/// Compile-time check: all keyboard action types implement gpui::Action.
#[test]
fn test_action_types_implement_action_trait() {
    fn assert_action<T: gpui::Action>() {}
    assert_action::<NewConversation>();
    assert_action::<ClearChat>();
    assert_action::<SwitchToChat>();
    assert_action::<SwitchToQuickStart>();
    assert_action::<SwitchToHistory>();
    assert_action::<SwitchToFiles>();
    assert_action::<SwitchToWorkflows>();
    assert_action::<SwitchToChannels>();
    assert_action::<SwitchToKanban>();
    assert_action::<SwitchToMonitor>();
    assert_action::<SwitchToLogs>();
    assert_action::<SwitchToCosts>();
    assert_action::<SwitchToReview>();
    assert_action::<SwitchToSkills>();
    assert_action::<SwitchToRouting>();
    assert_action::<SwitchToLearning>();
    assert_action::<ToggleCommandPalette>();
    assert_action::<ActivitySetView>();
}

#[test]
fn command_palette_overlay_renders_above_workspace_content() {
    let workspace_src = include_str!("../src/workspace.rs");
    let overlays_src = include_str!("../src/workspace/overlays.rs");

    let main_content_pos = workspace_src
        .find(".child(chrome::render_main_content(")
        .expect("workspace should render main content");
    let status_bar_pos = workspace_src
        .find(".child(self.status_bar.render(theme))")
        .expect("workspace should render status bar");
    let command_palette_pos = workspace_src
        .find(".when(self.show_command_palette")
        .expect("workspace should render command palette overlay");

    assert!(
        command_palette_pos > main_content_pos && command_palette_pos > status_bar_pos,
        "command palette overlay must be mounted after main content/status so Jump opens above the workspace"
    );

    let palette_overlay_start = overlays_src
        .find("pub(super) fn render_command_palette")
        .expect("command palette overlay renderer should exist");
    let palette_overlay = &overlays_src[palette_overlay_start..];
    assert!(
        palette_overlay.contains(".id(\"command-palette-backdrop\")")
            && palette_overlay.contains(".occlude()"),
        "command palette backdrop should be an occluding top-level overlay"
    );
}
