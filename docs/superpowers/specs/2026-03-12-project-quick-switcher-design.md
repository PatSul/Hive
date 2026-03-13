# Project Quick-Switcher Design

## Problem

Hive tracks up to 8 recent workspaces in `SessionState` but never exposes them in the UI. The only way to switch projects is through the OS file picker (`OpenWorkspaceDirectory`), which is slow and requires navigating the filesystem every time.

## Solution

Add a **titlebar dropdown** to the existing workspace chip. Clicking the project name opens a popup listing pinned and recent projects for one-click switching. A sidebar panel for richer project management will follow in a separate iteration.

## Scope

- Titlebar dropdown with pinned + recent projects
- Pin/unpin toggle per project
- Persistent pinned workspaces in `SessionState`
- Remove-from-recents option
- "Open folder..." at the bottom of the dropdown

A dedicated sidebar panel is **out of scope** for this iteration.

## Data Model

### `SessionState` (hive_core/src/session.rs)

Add a new field:

```rust
pub pinned_workspaces: Vec<String>,
```

This stores absolute paths of pinned projects. Pinned projects persist independently of the recent-8 list and always appear at the top of the dropdown. **Cap: 20 pinned workspaces maximum** (`MAX_PINNED_WORKSPACES`).

### `HiveWorkspace` (hive_ui/src/workspace.rs)

Add two fields:

```rust
pinned_workspace_roots: Vec<PathBuf>,
show_project_dropdown: bool,
```

- `pinned_workspace_roots` is loaded from `SessionState::pinned_workspaces` at startup, same pattern as `recent_workspace_roots`.
- `show_project_dropdown` controls dropdown visibility, toggled by click.

## Actions (hive_ui_core/src/actions.rs)

### Zero-sized (inside `actions!()` macro)

Add `ToggleProjectDropdown` to the existing `actions!(hive_workspace, [...])` macro block.

### Parameterized

```rust
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct SwitchToWorkspace {
    pub path: String,
}

#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct TogglePinWorkspace {
    pub path: String,
}

#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct RemoveRecentWorkspace {
    pub path: String,
}
```

Note: `TogglePinWorkspace` (not `PinWorkspace`) because the handler toggles pin state.

## UI: Titlebar Dropdown

### Trigger

The project name label inside `workspace_chip` dispatches `ToggleProjectDropdown` on click (replacing the current `SwitchToFiles` action). The "Open" button remains unchanged.

**UX note:** The current `SwitchToFiles` shortcut on the project name is removed. This is acceptable because the Files panel is already accessible via its sidebar icon. The dropdown provides more value in this position.

### Dropdown Layout

```
┌──────────────────────────────────┐
│  ★ pinned-project-1             │  <- pinned section
│    C:\Users\pat\code\...        │
│  ★ pinned-project-2             │
│    D:\Work\...                  │
│ ─────────────────────────────── │  <- separator (only if pinned exist)
│    recent-project-1             │  <- recent section (excludes pinned)
│    H:\WORK\AG\...              │
│    recent-project-2             │
│    C:\Projects\...              │
│ ─────────────────────────────── │
│  Open folder...                 │  <- triggers OpenWorkspaceDirectory
└──────────────────────────────────┘
```

**Scrollability:** If pinned + recent exceed the visible area, the dropdown body (excluding the "Open folder..." footer) is scrollable.

**Deduplication:** Filtering happens at render time in `workspace_chip`. The recent slice is iterated and any path present in the pinned slice is skipped. This avoids mutating the `recent_workspace_roots` list when pinning.

### Row Behavior

Each project row contains:
- **Left:** Project name (folder name, bold if current project) + truncated path below in secondary text
- **Right:** Pin toggle button (star outline -> filled star when pinned)
- **Click row:** Dispatches `SwitchToWorkspace { path }`, closes dropdown
- **Click pin button:** Dispatches `TogglePinWorkspace { path }`, stops propagation (dropdown stays open)
- Active project is highlighted with accent color and a dot indicator
- **Cannot remove active project:** The active workspace row has no remove button and cannot be removed from recents (it would be immediately re-added by `record_recent_workspace`)

### Dropdown Positioning & Drag Region

The dropdown is rendered as an absolutely-positioned `div` below the workspace chip, with a high z-index to overlay the main content.

**Critical: Drag region interaction.** On Windows/Linux, the workspace chip sits inside the `WindowControlArea::Drag` div. All interactive elements inside the dropdown **must** call `cx.stop_propagation()` in their `on_mouse_down` handlers to prevent the window drag handler from capturing pointer events. The dropdown container itself should also use `.occlude()`.

### Dropdown Dismissal

- **Click outside:** The dropdown renders a transparent full-window backdrop `div` behind it (lower z-index than the dropdown, higher than content). Clicking the backdrop sets `show_project_dropdown = false` and calls `cx.notify()`.
- **Click a project row:** Switch + close.
- **Escape key:** Handled by `HiveWorkspace` (which is a `View` with key event access). When `show_project_dropdown` is true and Escape is pressed, close the dropdown instead of propagating.

Note: The dropdown is rendered by `HiveWorkspace::render()` (not inside `Titlebar::render`) so that `HiveWorkspace` as a `View` can own the dismiss logic, key handlers, and state. `Titlebar::render` renders only the chip trigger; the dropdown overlay is a sibling element rendered conditionally by the workspace.

### Dropdown chevron

Add a small chevron icon after the project name text in the chip to indicate it's clickable/expandable.

## Titlebar Signature Change

`Titlebar::render` signature stays minimal — it only needs to know whether to show the chevron:

```rust
pub fn render(
    theme: &HiveTheme,
    window: &Window,
    current_workspace_root: &Path,
) -> impl IntoElement
```

The chevron is always rendered. The dropdown itself is rendered by `HiveWorkspace` outside of `Titlebar::render`.

## Action Handlers (workspace.rs)

### `handle_toggle_project_dropdown`
Toggle `self.show_project_dropdown`, call `cx.notify()`.

### `handle_switch_to_workspace`
Extract path from action, call existing `self.switch_to_workspace(path, cx)`, set `show_project_dropdown = false`. If the path no longer exists on disk, remove it from both recents and pinned, show a notification ("Project folder not found"), and do not switch.

### `handle_toggle_pin_workspace`
Toggle the path in `self.pinned_workspace_roots`:
- If present, remove it (unpin)
- If absent, add it (pin); enforce `MAX_PINNED_WORKSPACES` cap
Set `session_dirty = true`, call `save_session(cx)`, `cx.notify()`.

### `handle_remove_recent_workspace`
Remove the path from `self.recent_workspace_roots`. If also in pinned, remove from pinned. **No-op if the path is the active workspace** (it would be immediately re-added). Set `session_dirty = true`, save, notify.

### `save_session` update
Serialize `pinned_workspace_roots` into `SessionState::pinned_workspaces`.

## Files Modified

| File | Change |
|------|--------|
| `hive_core/src/session.rs` | Add `pinned_workspaces: Vec<String>` to `SessionState` |
| `hive_ui_core/src/actions.rs` | Add `ToggleProjectDropdown` (zero-sized), `SwitchToWorkspace`, `TogglePinWorkspace`, `RemoveRecentWorkspace` |
| `hive_ui/src/titlebar.rs` | Add chevron to workspace chip |
| `hive_ui/src/workspace.rs` | Add `pinned_workspace_roots`, `show_project_dropdown` fields; dropdown rendering; backdrop; action handlers; updated `save_session`; load pinned from session; Escape key handler |

## Testing

- `SessionState` round-trip test with `pinned_workspaces` field
- Pin/unpin toggles correctly in `pinned_workspace_roots`
- Pinned cap enforced at `MAX_PINNED_WORKSPACES`
- Recent list excludes pinned projects (dedup at render)
- `switch_to_workspace` still works as before
- Removing active workspace from recents is a no-op
- Switching to a non-existent path removes it from recents/pinned
- Deduplication: same path in both pinned and recent shows only in pinned section
