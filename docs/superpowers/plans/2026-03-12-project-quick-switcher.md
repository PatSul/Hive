# Project Quick-Switcher Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a titlebar dropdown for one-click switching between recent and pinned projects, replacing the current file-picker-only workflow.

**Architecture:** Extend `SessionState` with `pinned_workspaces`, add 4 new actions to `hive_ui_core`, add chevron to titlebar chip, render a dropdown overlay from `HiveWorkspace::render()` with backdrop dismiss and Escape key handling. Dedup pinned vs recent at render time.

**Tech Stack:** Rust, GPUI (0.2.2), gpui_component (0.5.1)

**Spec:** `docs/superpowers/specs/2026-03-12-project-quick-switcher-design.md`

---

## File Structure

| File | Responsibility |
|------|---------------|
| `hive/crates/hive_core/src/session.rs` | Add `pinned_workspaces: Vec<String>` field + tests |
| `hive/crates/hive_ui_core/src/actions.rs` | Add 4 new actions (1 zero-sized, 3 parameterized) |
| `hive/crates/hive_ui/src/titlebar.rs` | Add chevron to workspace chip |
| `hive/crates/hive_ui/src/workspace.rs` | New fields, action handlers, dropdown rendering, backdrop, Escape key, save_session update |

---

## Chunk 1: Data Model + Actions

### Task 1: Add `pinned_workspaces` to SessionState

**Files:**
- Modify: `hive/crates/hive_core/src/session.rs:13-21` (SessionState struct)
- Modify: `hive/crates/hive_core/src/session.rs:102-245` (tests)

- [ ] **Step 1: Add the field to SessionState**

In `hive/crates/hive_core/src/session.rs`, add `pinned_workspaces` to the struct:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SessionState {
    pub active_conversation_id: Option<String>,
    pub active_panel: String,
    pub window_size: Option<[u32; 2]>,
    pub working_directory: Option<String>,
    pub recent_workspaces: Vec<String>,
    pub pinned_workspaces: Vec<String>,   // <-- NEW
    pub open_files: Vec<String>,
    pub chat_draft: Option<String>,
}
```

- [ ] **Step 2: Add round-trip test for pinned_workspaces**

Add a new test after the existing tests in `session.rs`:

```rust
#[test]
fn test_pinned_workspaces_round_trip() {
    let tmp = TempDir::new().unwrap();
    let path = session_path_in(&tmp);

    let state = SessionState {
        pinned_workspaces: vec![
            "/home/user/pinned1".into(),
            "/home/user/pinned2".into(),
        ],
        ..Default::default()
    };

    state.save_to(&path).unwrap();
    let loaded = SessionState::load_from(&path).unwrap();

    assert_eq!(
        loaded.pinned_workspaces,
        vec!["/home/user/pinned1", "/home/user/pinned2"]
    );
}

#[test]
fn test_pinned_workspaces_default_empty() {
    let tmp = TempDir::new().unwrap();
    let path = session_path_in(&tmp);

    // Old-format JSON without pinned_workspaces field
    std::fs::write(&path, r#"{ "active_panel": "Chat" }"#).unwrap();

    let loaded = SessionState::load_from(&path).unwrap();
    assert!(loaded.pinned_workspaces.is_empty());
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p hive_core -- session`
Expected: All tests pass including the two new ones.

- [ ] **Step 4: Commit**

```bash
git add hive/crates/hive_core/src/session.rs
git commit -m "feat(session): add pinned_workspaces field to SessionState"
```

---

### Task 2: Add actions to hive_ui_core

**Files:**
- Modify: `hive/crates/hive_ui_core/src/actions.rs:7-121` (actions! macro) and bottom of file

- [ ] **Step 1: Add ToggleProjectDropdown to the zero-sized actions macro**

In `hive/crates/hive_ui_core/src/actions.rs`, add `ToggleProjectDropdown` inside the `actions!(hive_workspace, [...])` block, after `OpenWorkspaceDirectory` (line 35):

```rust
        OpenWorkspaceDirectory,
        ToggleProjectDropdown,  // <-- NEW
```

- [ ] **Step 2: Add 3 parameterized actions**

At the bottom of `actions.rs` (after the `PluginToggleSkill` struct, line 512), add:

```rust
// ---------------------------------------------------------------------------
// Project quick-switcher actions
// ---------------------------------------------------------------------------

/// Switch to a workspace by path.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct SwitchToWorkspace {
    pub path: String,
}

/// Toggle pin/unpin state for a workspace.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct TogglePinWorkspace {
    pub path: String,
}

/// Remove a workspace from the recent list.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct RemoveRecentWorkspace {
    pub path: String,
}
```

- [ ] **Step 3: Add re-exports in workspace.rs**

In `hive/crates/hive_ui/src/workspace.rs`, find the `pub use hive_ui_core::{` block (around line 40-84) and add the new actions to the re-export list:

```rust
    ToggleProjectDropdown,
    SwitchToWorkspace, TogglePinWorkspace, RemoveRecentWorkspace,
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo check -p hive_ui_core -p hive_ui`
Expected: No errors.

- [ ] **Step 5: Commit**

```bash
git add hive/crates/hive_ui_core/src/actions.rs hive/crates/hive_ui/src/workspace.rs
git commit -m "feat(actions): add project quick-switcher actions"
```

---

## Chunk 2: Workspace State + Handlers

### Task 3: Add pinned state and handlers to HiveWorkspace

**Files:**
- Modify: `hive/crates/hive_ui/src/workspace.rs`

- [ ] **Step 1: Add fields to HiveWorkspace struct**

In `hive/crates/hive_ui/src/workspace.rs`, find the `HiveWorkspace` struct (line 223). Add two fields after `recent_workspace_roots` (line 286):

```rust
    recent_workspace_roots: Vec<PathBuf>,
    pinned_workspace_roots: Vec<PathBuf>,  // <-- NEW
    show_project_dropdown: bool,           // <-- NEW
```

- [ ] **Step 2: Add MAX_PINNED_WORKSPACES constant**

After `MAX_RECENT_WORKSPACES` (line 296):

```rust
const MAX_PINNED_WORKSPACES: usize = 20;
```

- [ ] **Step 3: Add load_pinned_workspace_roots method**

After `load_recent_workspace_roots` (around line 892), add:

```rust
    fn load_pinned_workspace_roots(session: &SessionState) -> Vec<PathBuf> {
        session
            .pinned_workspaces
            .iter()
            .filter_map(|p| {
                let path = PathBuf::from(p);
                if path.exists() { Some(path) } else { None }
            })
            .take(MAX_PINNED_WORKSPACES)
            .collect()
    }
```

- [ ] **Step 4: Initialize new fields in constructor**

In the `new()` method, after `let recent_workspace_roots = ...` (line 400), add:

```rust
        let pinned_workspace_roots = Self::load_pinned_workspace_roots(&session);
```

In the `Self { ... }` initializer (around line 760-808), add the two new fields after `recent_workspace_roots`:

```rust
            recent_workspace_roots,
            pinned_workspace_roots,   // <-- NEW
            show_project_dropdown: false,  // <-- NEW
```

- [ ] **Step 5: Update save_session to include pinned_workspaces**

In `save_session` (line 2274), update the `SessionState` construction to include `pinned_workspaces`:

```rust
        let state = SessionState {
            active_conversation_id: conv_id.clone(),
            active_panel: self.sidebar.active_panel.to_stored().to_string(),
            window_size: self.last_window_size,
            working_directory: Some(self.current_project_root.to_string_lossy().to_string()),
            recent_workspaces: self
                .recent_workspace_roots
                .iter()
                .map(|path| path.to_string_lossy().to_string())
                .collect(),
            pinned_workspaces: self                      // <-- NEW
                .pinned_workspace_roots                  // <-- NEW
                .iter()                                  // <-- NEW
                .map(|path| path.to_string_lossy().to_string()) // <-- NEW
                .collect(),                              // <-- NEW
            open_files: Vec::new(),
            chat_draft: None,
        };
```

- [ ] **Step 6: Add action handler methods**

After `handle_open_workspace_directory` (around line 3460), add:

```rust
    fn handle_toggle_project_dropdown(
        &mut self,
        _action: &ToggleProjectDropdown,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show_project_dropdown = !self.show_project_dropdown;
        cx.notify();
    }

    fn handle_switch_to_workspace_action(
        &mut self,
        action: &SwitchToWorkspace,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show_project_dropdown = false;
        let path = PathBuf::from(&action.path);
        if !path.exists() {
            // Remove stale path from both lists
            self.recent_workspace_roots.retain(|p| p != &path);
            self.pinned_workspace_roots.retain(|p| p != &path);
            self.session_dirty = true;
            self.save_session(cx);
            if cx.has_global::<AppNotifications>() {
                cx.global::<AppNotifications>().0.lock().unwrap().push(
                    AppNotification::new(
                        "Project folder not found",
                        NotificationType::Warning,
                    ),
                );
            }
            cx.notify();
            return;
        }
        self.switch_to_workspace(path, cx);
    }

    fn handle_toggle_pin_workspace(
        &mut self,
        action: &TogglePinWorkspace,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let path = PathBuf::from(&action.path);
        if let Some(idx) = self.pinned_workspace_roots.iter().position(|p| p == &path) {
            self.pinned_workspace_roots.remove(idx);
        } else {
            self.pinned_workspace_roots.push(path);
            self.pinned_workspace_roots.truncate(MAX_PINNED_WORKSPACES);
        }
        self.session_dirty = true;
        self.save_session(cx);
        cx.notify();
    }

    fn handle_remove_recent_workspace(
        &mut self,
        action: &RemoveRecentWorkspace,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let path = PathBuf::from(&action.path);
        // No-op for active workspace
        if path == self.current_project_root {
            return;
        }
        self.recent_workspace_roots.retain(|p| p != &path);
        self.pinned_workspace_roots.retain(|p| p != &path);
        self.session_dirty = true;
        self.save_session(cx);
        cx.notify();
    }
```

- [ ] **Step 7: Register action handlers in render()**

In `render()` (around line 10296, after the `handle_open_workspace_directory` line), add:

```rust
            .on_action(cx.listener(Self::handle_toggle_project_dropdown))
            .on_action(cx.listener(Self::handle_switch_to_workspace_action))
            .on_action(cx.listener(Self::handle_toggle_pin_workspace))
            .on_action(cx.listener(Self::handle_remove_recent_workspace))
```

- [ ] **Step 8: Verify it compiles**

Run: `cargo check -p hive_ui`
Expected: No errors.

- [ ] **Step 9: Commit**

```bash
git add hive/crates/hive_ui/src/workspace.rs
git commit -m "feat(workspace): add pinned state and quick-switcher action handlers"
```

---

## Chunk 3: Titlebar Chevron + Dropdown UI

### Task 4: Add chevron to titlebar chip

**Files:**
- Modify: `hive/crates/hive_ui/src/titlebar.rs:104-176`

- [ ] **Step 1: Replace SwitchToFiles with ToggleProjectDropdown on project name click**

In `titlebar.rs`, update the import to include `ToggleProjectDropdown`:

```rust
use hive_ui_core::{
    HiveTheme, OpenWorkspaceDirectory, ToggleProjectDropdown,
};
```

Then in `workspace_chip` (line 133), replace `SwitchToFiles` with `ToggleProjectDropdown`:

```rust
            .on_mouse_down(MouseButton::Left, |_, window, cx| {
                cx.stop_propagation();
                window.dispatch_action(Box::new(ToggleProjectDropdown), cx);
            })
```

Note: must add `cx.stop_propagation()` here too since it's inside the drag area.

- [ ] **Step 2: Add chevron icon after project name**

In `workspace_chip`, after the project name text div (line 147), add a chevron:

Find this block:
```rust
            .child(
                div()
                    .text_size(theme.font_size_xs)
                    .truncate()
                    .child(format!("Project Space: {current_label}")),
            ),
```

Replace with:
```rust
            .child(
                div()
                    .text_size(theme.font_size_xs)
                    .truncate()
                    .child(format!("Project Space: {current_label}")),
            )
            .child(
                Icon::new(IconName::ChevronDown).custom_size(px(10.0))
            ),
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check -p hive_ui`
Expected: No errors. (If `IconName::ChevronDown` doesn't exist, check available icons with grep and use the closest match.)

- [ ] **Step 4: Commit**

```bash
git add hive/crates/hive_ui/src/titlebar.rs
git commit -m "feat(titlebar): add chevron and toggle dropdown on project name click"
```

---

### Task 5: Render the dropdown overlay from HiveWorkspace

**Files:**
- Modify: `hive/crates/hive_ui/src/workspace.rs` (render method + new render_project_dropdown helper)

- [ ] **Step 1: Add render_project_dropdown method**

Add a new method to `HiveWorkspace` (e.g. after `render_sidebar`):

```rust
    fn render_project_dropdown(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.theme;
        let current_root = &self.current_project_root;
        let pinned = &self.pinned_workspace_roots;

        // Build rows: pinned first, then recent (excluding pinned), then "Open folder..."
        let mut children: Vec<AnyElement> = Vec::new();

        // Pinned section
        for path in pinned {
            let is_active = path == current_root;
            let path_str = path.to_string_lossy().to_string();
            let name = Self::project_name_from_path(path);
            children.push(
                self.render_project_row(theme, &name, &path_str, is_active, true, cx)
                    .into_any_element(),
            );
        }

        // Separator if pinned exist
        if !pinned.is_empty() {
            children.push(
                div()
                    .h(px(1.0))
                    .mx(theme.space_2)
                    .my(theme.space_1)
                    .bg(theme.border)
                    .into_any_element(),
            );
        }

        // Recent section (exclude pinned)
        for path in &self.recent_workspace_roots {
            if pinned.contains(path) {
                continue;
            }
            let is_active = path == current_root;
            let path_str = path.to_string_lossy().to_string();
            let name = Self::project_name_from_path(path);
            children.push(
                self.render_project_row(theme, &name, &path_str, is_active, false, cx)
                    .into_any_element(),
            );
        }

        // Separator
        children.push(
            div()
                .h(px(1.0))
                .mx(theme.space_2)
                .my(theme.space_1)
                .bg(theme.border)
                .into_any_element(),
        );

        // "Open folder..." row
        children.push(
            div()
                .id("open-folder-row")
                .flex()
                .flex_row()
                .items_center()
                .gap(theme.space_2)
                .px(theme.space_3)
                .py(theme.space_2)
                .rounded(theme.radius_md)
                .cursor_pointer()
                .hover(|s| s.bg(theme.bg_tertiary))
                .on_mouse_down(MouseButton::Left, |_, window, cx| {
                    cx.stop_propagation();
                    window.dispatch_action(Box::new(OpenWorkspaceDirectory), cx);
                })
                .child(Icon::new(IconName::FolderOpen).small())
                .child(
                    div()
                        .text_size(theme.font_size_xs)
                        .text_color(theme.text_secondary)
                        .child("Open folder..."),
                )
                .into_any_element(),
        );

        // Dropdown container
        div()
            .id("project-dropdown")
            .occlude()
            .absolute()
            .top(px(42.0)) // below titlebar
            .left(px(120.0)) // roughly aligned with the chip
            .w(px(320.0))
            .max_h(px(400.0))
            .overflow_y_scroll()
            .bg(theme.bg_primary)
            .border_1()
            .border_color(theme.border)
            .rounded(theme.radius_lg)
            .shadow_lg()
            .py(theme.space_1)
            .children(children)
    }

    fn render_project_row(
        &self,
        theme: &HiveTheme,
        name: &str,
        path_str: &str,
        is_active: bool,
        is_pinned: bool,
        _cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let switch_path = path_str.to_string();
        let pin_path = path_str.to_string();

        let text_color = if is_active {
            theme.accent_cyan
        } else {
            theme.text_primary
        };

        div()
            .id(SharedString::from(format!("project-row-{}", path_str)))
            .flex()
            .flex_row()
            .items_center()
            .gap(theme.space_2)
            .px(theme.space_3)
            .py(theme.space_2)
            .rounded(theme.radius_md)
            .cursor_pointer()
            .hover(|s| s.bg(theme.bg_tertiary))
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                cx.stop_propagation();
                window.dispatch_action(
                    Box::new(SwitchToWorkspace { path: switch_path.clone() }),
                    cx,
                );
            })
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .overflow_hidden()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(theme.space_1)
                            .when(is_active, |el| {
                                el.child(
                                    div()
                                        .w(px(6.0))
                                        .h(px(6.0))
                                        .rounded(theme.radius_full)
                                        .bg(theme.accent_green),
                                )
                            })
                            .child(
                                div()
                                    .text_size(theme.font_size_sm)
                                    .text_color(text_color)
                                    .font_weight(if is_active {
                                        FontWeight::BOLD
                                    } else {
                                        FontWeight::NORMAL
                                    })
                                    .truncate()
                                    .child(name.to_string()),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(theme.text_tertiary)
                            .truncate()
                            .child(path_str.to_string()),
                    ),
            )
            // Pin toggle button
            .child(
                div()
                    .id(SharedString::from(format!("pin-btn-{}", pin_path)))
                    .flex_shrink_0()
                    .cursor_pointer()
                    .rounded(theme.radius_sm)
                    .p(px(4.0))
                    .hover(|s| s.bg(theme.bg_secondary))
                    .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                        cx.stop_propagation();
                        window.dispatch_action(
                            Box::new(TogglePinWorkspace {
                                path: pin_path.clone(),
                            }),
                            cx,
                        );
                    })
                    .child(
                        Icon::new(if is_pinned {
                            IconName::StarFilled
                        } else {
                            IconName::Star
                        })
                        .custom_size(px(14.0)),
                    ),
            )
    }
```

- [ ] **Step 2: Add backdrop and dropdown to render()**

In `render()`, after the Titlebar child (line 10425) and before the main content area, add the backdrop and dropdown conditionally:

```rust
            // Titlebar
            .child(Titlebar::render(theme, window, &self.current_project_root))
            // Project dropdown backdrop (dismisses on click)
            .when(self.show_project_dropdown, |el| {
                el.child(
                    div()
                        .id("project-dropdown-backdrop")
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full()
                        .on_mouse_down(MouseButton::Left, |_, window, cx| {
                            cx.stop_propagation();
                            window.dispatch_action(Box::new(ToggleProjectDropdown), cx);
                        }),
                )
            })
            // Project dropdown overlay
            .when(self.show_project_dropdown, |el| {
                el.child(self.render_project_dropdown(cx))
            })
            // Main content area: sidebar + panel
```

- [ ] **Step 3: Add Escape key handler**

In `render()`, add a key handler on the root div. Find the root div setup (around line 10227-10268) and add after the `.id()` or early in the chain:

```rust
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _window, cx| {
                if event.keystroke.key == "escape" && this.show_project_dropdown {
                    this.show_project_dropdown = false;
                    cx.notify();
                }
            }))
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo check -p hive_ui`
Expected: No errors. If icon names like `StarFilled`/`Star` don't exist, search available icons and use alternatives.

- [ ] **Step 5: Commit**

```bash
git add hive/crates/hive_ui/src/workspace.rs
git commit -m "feat(workspace): render project quick-switcher dropdown with backdrop dismiss"
```

---

## Chunk 4: Build & Visual Verification

### Task 6: Full build and manual test

- [ ] **Step 1: Run cargo check on workspace**

Run: `cargo check --workspace --exclude hive_app`
Expected: No errors.

- [ ] **Step 2: Run tests**

Run: `cargo test -p hive_core -- session`
Expected: All session tests pass.

- [ ] **Step 3: Fix any icon name issues**

If `IconName::Star` / `IconName::StarFilled` / `IconName::ChevronDown` don't exist in `gpui_component::IconName`, search for available alternatives:

Run: `grep -r "Star\|Pin\|Chevron\|Arrow" hive/target/debug/build/gpui-component-*/out/ 2>/dev/null` or check gpui_component docs.

Use the closest available icon names. Common alternatives:
- `ChevronDown` -> `ArrowDown` or inline SVG
- `Star` -> `Bookmark` or inline unicode character

- [ ] **Step 4: Commit any fixes**

```bash
git add -A
git commit -m "fix: resolve icon name issues for project dropdown"
```

- [ ] **Step 5: Version bump and final commit**

Update `hive/crates/hive_app/Cargo.toml` version from `0.3.23` to `0.3.24`.
Update `README.md` version badge to `0.3.24`.

```bash
git add hive/crates/hive_app/Cargo.toml README.md
git commit -m "chore: bump version to 0.3.24"
```
