use hive_ui_panels::components::split_pane::{PaneLayout, SplitDirection, TilingState};

#[test]
fn pane_layout_single_leaf_count() {
    let layout = PaneLayout::single("Chat");
    assert_eq!(layout.leaf_count(), 1);
}

#[test]
fn pane_layout_split_leaf_count() {
    let layout =
        PaneLayout::horizontal(PaneLayout::single("Chat"), PaneLayout::single("Files"), 0.5);
    assert_eq!(layout.leaf_count(), 2);
}

#[test]
fn pane_layout_nested_leaf_count() {
    let layout = PaneLayout::horizontal(
        PaneLayout::single("Chat"),
        PaneLayout::vertical(
            PaneLayout::single("Files"),
            PaneLayout::single("Terminal"),
            0.6,
        ),
        0.5,
    );
    assert_eq!(layout.leaf_count(), 3);
}

#[test]
fn pane_layout_panel_names() {
    let layout = PaneLayout::horizontal(
        PaneLayout::single("Chat"),
        PaneLayout::vertical(
            PaneLayout::single("Files"),
            PaneLayout::single("Terminal"),
            0.5,
        ),
        0.5,
    );
    assert_eq!(layout.panel_names(), vec!["Chat", "Files", "Terminal"]);
}

#[test]
fn pane_layout_ratio_clamped() {
    let layout = PaneLayout::horizontal(PaneLayout::single("A"), PaneLayout::single("B"), 1.5);
    if let PaneLayout::Split { ratio, .. } = layout {
        assert_eq!(ratio, 1.0);
    } else {
        panic!("Expected Split variant");
    }
}

#[test]
fn pane_layout_ratio_clamped_negative() {
    let layout = PaneLayout::horizontal(PaneLayout::single("A"), PaneLayout::single("B"), -0.5);
    if let PaneLayout::Split { ratio, .. } = layout {
        assert_eq!(ratio, 0.0);
    } else {
        panic!("Expected Split variant");
    }
}

#[test]
fn tiling_state_single() {
    let state = TilingState::single("Chat");
    assert_eq!(state.layout.leaf_count(), 1);
}

#[test]
fn split_direction_equality() {
    assert_eq!(SplitDirection::Horizontal, SplitDirection::Horizontal);
    assert_ne!(SplitDirection::Horizontal, SplitDirection::Vertical);
}
