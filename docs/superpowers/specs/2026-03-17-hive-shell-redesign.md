# Hive Shell Redesign

## Problem

Hive currently exposes a flat list of 27 first-class panels in the main navigation (`Panel::ALL` in `hive/crates/hive_ui_core/src/sidebar.rs`). This preserves access, but it creates three UX problems:

1. The app explains its architecture instead of guiding the user's job.
2. High-frequency actions and low-frequency configuration live at the same visual level.
3. Validation, approvals, and outcome tracking exist, but they are not presented as one coherent loop.

The current welcome screen reinforces this problem. It asks the user to type a message or visit Settings / Files / Routing instead of presenting a mission-oriented starting point.

## Goals

- Preserve all current functionality and keyboard access.
- Reduce top-level navigation to a small set of obvious destinations.
- Promote Hive's strongest workflow: context -> plan -> execute -> validate -> apply.
- Make approvals, activity, and outcomes feel central instead of auxiliary.
- Keep advanced features one click away, not permanently in the main rail.

## Non-Goals

- Removing or deprecating any existing panel in the first iteration.
- Rewriting panel internals before the shell model is in place.
- Narrowing Hive into a single-purpose product.

## Solution

Introduce a new shell layer above `Panel`:

```rust
pub enum ShellDestination {
    Home,
    Build,
    Automate,
    Assist,
    Observe,
}
```

The current `Panel` enum remains the implementation-level route table. The sidebar stops rendering every panel directly. Instead, the shell selects a destination, and each destination renders a local tab strip or segmented control that routes into the existing panels.

This keeps all functionality while making the top-level navigation legible.

## Primary Destinations

### 1. Home

Purpose: command center and first-run surface.

Contents:
- Continue recent conversation / run
- Recent and pinned workspaces
- Readiness cards: model connected, indexing status, integrations ready, pending approvals
- Mission cards:
  - Build a feature
  - Fix a bug
  - Review a branch
  - Create an automation
  - Start daily briefing
- Setup blockers with one-click fixes

This replaces the current passive welcome screen and absorbs `QuickStart` as a richer launchpad.

### 2. Build

Purpose: code-focused workspace for project execution.

Default center view:
- Chat as the primary canvas

Local tabs:
- Chat
- Plan
- Tasks
- Files
- Code
- Git
- Terminal
- Prompts
- History
- Agents

Right-side context rail:
- Current spec or goal
- Active run state
- Selected file details
- Inline approval card when relevant
- Suggested next action

This destination makes Hive feel like a workbench rather than a feature index.

### 3. Automate

Purpose: repeatable and distributed execution.

Local tabs:
- Workflows
- Builder
- Channels
- Peers

Top summary cards:
- Active automations
- Last successful runs
- Failed runs needing attention
- Remote peers available

This turns automation from a hidden subsystem into a visible product line.

### 4. Assist

Purpose: personal operating surface.

Local tabs:
- Briefing
- Email
- Calendar
- Reminders
- Research
- Actions

This is a cleaner framing for the existing Assistant capability and gives it a stable destination separate from coding workflows.

### 5. Observe

Purpose: validation, safety, and operational visibility.

Default center view:
- Activity inbox

Local tabs:
- Activity
- Monitor
- Logs
- Costs
- Learning
- Security

Pinned section at the top:
- Pending approvals
- Run failures
- Budget warnings
- Shield blocks

This is the biggest concept borrowed from Typless: validation is not a side panel, it is a core destination.

## Utility Access

Low-frequency tools move out of the primary rail and into a global utility drawer plus command palette.

Utility drawer sections:
- Intelligence: Models, Routing, Skills
- System: Settings, Help
- Labs: Token Launch

Access paths:
- Bottom utility button in the rail
- Command palette / omnibox
- Existing keyboard shortcuts
- Deep links from Home cards and notifications

## Panel Mapping

No panel is removed. The shell only changes where it lives.

| Current Panel | New Surface | Placement |
|---|---|---|
| Chat | Build | Default canvas |
| QuickStart | Home | Mission launchpad |
| History | Build | History tab |
| Files | Build | Files tab |
| CodeMap | Build | Code tab |
| PromptLibrary | Build | Prompts tab |
| Specs | Build | Plan tab |
| Agents | Build | Agents tab |
| Workflows | Automate | Workflows tab |
| Channels | Automate | Channels tab |
| Kanban | Build | Tasks tab |
| Monitor | Observe | Monitor tab |
| Activity | Observe | Default inbox |
| Logs | Observe | Logs tab |
| Costs | Observe | Costs tab |
| Review | Build | Git tab |
| Skills | Utility drawer | Intelligence |
| Routing | Utility drawer | Intelligence |
| Models | Utility drawer | Intelligence |
| Learning | Observe | Learning tab |
| Shield | Observe | Security tab |
| Assistant | Assist | Briefing tab |
| TokenLaunch | Utility drawer | Labs |
| Network | Automate | Peers tab |
| Terminal | Build | Terminal tab |
| Settings | Utility drawer | System |
| Help | Utility drawer | System |

## Layout Model

### Global shell

```text
┌─────────────────────────────────────────────────────────────────────┐
│ Titlebar: workspace switcher | mission | model | run state | search│
├──────────────┬──────────────────────────────────────┬───────────────┤
│ Primary rail │ Active destination workspace         │ Context rail  │
│ Home         │                                      │ approvals     │
│ Build        │ local tabs + main canvas             │ spec/details  │
│ Automate     │                                      │ next steps    │
│ Assist       │                                      │               │
│ Observe      │                                      │               │
│ ...utility   │                                      │               │
├──────────────┴──────────────────────────────────────┴───────────────┤
│ Composer / action bar / notifications                               │
└─────────────────────────────────────────────────────────────────────┘
```

### Build workspace

```text
┌─────────────────────────────────────────────────────────────────────┐
│ Goal bar: "Implement X" | Context ready | Plan | Execute | Validate │
├─────────────────────────────────────────────────────────────────────┤
│ Tabs: Chat | Plan | Tasks | Files | Code | Git | Terminal | ...     │
├───────────────────────────────┬─────────────────────────────────────┤
│ Main canvas                   │ Context rail                        │
│ Chat / file / diff / task     │ active spec                         │
│                               │ current run                         │
│                               │ approvals                           │
│                               │ next actions                        │
└───────────────────────────────┴─────────────────────────────────────┘
```

### Observe workspace

```text
┌─────────────────────────────────────────────────────────────────────┐
│ Inbox summary: 2 approvals | 1 blocked action | 1 failed run       │
├─────────────────────────────────────────────────────────────────────┤
│ Tabs: Activity | Monitor | Logs | Costs | Learning | Security       │
├─────────────────────────────────────────────────────────────────────┤
│ Event stream / approval cards / operational metrics                 │
└─────────────────────────────────────────────────────────────────────┘
```

## Key Interaction Changes

### 1. Mission-first header

Every active destination should show what Hive is doing for the user right now, not just which panel is open.

Examples:
- "Drafting PR review for `feature/cache-index`"
- "Running workflow: nightly validation"
- "Preparing morning briefing"

### 2. Pipeline strip

Add a small persistent strip for agent work:

`Context -> Plan -> Execute -> Validate -> Apply`

This makes Hive's internal orchestration legible to the user and gives approvals a natural home.

### 3. Unified command palette

Add a command/search entry point that can:
- open any panel
- switch workspace
- run a Quick Start mission
- open settings sections
- jump to a recent file, workflow, conversation, or peer

This is the primary "no functionality loss" backstop.

### 4. Inline and centralized approvals

Approvals continue to appear inline where relevant, but every approval must also appear in `Observe -> Activity` so users can always recover pending decisions.

### 5. Outcome cards instead of raw feature discovery

Home and Observe should lead with:
- runs completed
- time saved
- tests generated
- issues found
- approvals waiting

Provider selection, routing rules, and advanced setup remain available but no longer dominate the entry experience.

## Migration Strategy

### Phase 1: Shell abstraction without panel removal

- Add `ShellDestination` and per-destination local tab state.
- Keep `Panel` as the underlying route model.
- Map existing `SwitchTo*` actions into destination + tab transitions.
- Keep old session restore working by translating the stored panel to a destination at load time.

### Phase 2: Replace welcome with Home command center

- Upgrade `QuickStart` and current welcome content into the new Home surface.
- Surface readiness, recent workspaces, and mission cards.

### Phase 3: Introduce Observe inbox

- Make `Activity` the default Observe tab.
- Pin approvals, failures, and budget/safety events above the event stream.

### Phase 4: Collapse the main rail

- Replace the flat panel list with the 5-destination rail plus utility access.
- Keep command palette, shortcuts, and deep links to every existing panel.

### Phase 5: Polish and de-duplicate

- Remove duplicate navigation affordances.
- Standardize top bars and context rails across destinations.
- Evaluate whether any panels should later merge at the implementation level.

## Compatibility Rules

To guarantee no functionality loss:

- Every existing panel must remain reachable in 1 action from somewhere in the shell.
- Existing keyboard shortcuts must continue to land on the matching destination/tab.
- Existing notifications and action dispatches must continue to work.
- Session restore must reopen the same effective screen, even if the shell groups it differently.
- No panel internals are rewritten during shell migration unless required for routing.

## Acceptance Criteria

- A new user can identify the 5 core destinations without learning Hive's internal architecture.
- A power user can still reach any current panel in one click or one shortcut.
- Approvals and validation events are visible from both the current flow and the Observe inbox.
- Build mode feels centered on getting work done, not browsing tools.
- Home gives a meaningful next step even when no conversation exists.

## Files Likely Affected

| File | Change |
|---|---|
| `hive/crates/hive_ui_core/src/sidebar.rs` | Stop treating the raw panel list as the primary rail model |
| `hive/crates/hive_ui_core/src/actions.rs` | Add shell destination / local-tab actions |
| `hive/crates/hive_ui_core/src/welcome.rs` | Replace passive welcome with Home command center |
| `hive/crates/hive_ui/src/workspace.rs` | Add shell state, destination routing, local tabs, Observe inbox defaults |
| `hive/crates/hive_ui/src/titlebar.rs` | Add mission-oriented header controls and command entry point |
| `hive/crates/hive_ui_panels/...` | Minor top-bar and context-rail adjustments where needed |

## Recommended First Implementation Slice

Do the smallest meaningful cut first:

1. Add `ShellDestination`.
2. Create a new Home surface using the existing `QuickStart` and welcome data.
3. Group `Activity`, `Logs`, `Costs`, `Learning`, and `Shield` under Observe.
4. Leave Build mostly backed by existing panels in the first pass.

That gets most of the UX win without risking a broad rewrite.
