# Hive UX Review

**Status:** Living plan  
**Created:** 2026-06-18  
**Scope:** Hive Rust desktop app shell, navigation, onboarding, and core workflow surfaces

## Summary

Hive is feature-dense but navigationally over-built. The app has strong capabilities, but it exposes too many internal surfaces at similar weight. Users are asked to understand panels, spaces, destinations, pipeline stages, runs, missions, utilities, and configuration before they can confidently start work.

The highest-leverage fix is not visual polish. It is information architecture.

Hive should feel like a project workspace that helps the user start, run, review, and finish work. The UI should reduce the number of top-level concepts, make the next action obvious, and move advanced configuration out of the daily path without hiding it.

For the complete source-backed inventory and per-panel findings, see `docs/UX-PANEL-AUDIT.md`.

Reviewer sequencing note: do the P0 safety fixes first, independent of the IA redesign. Then repair fake/static controls, lock product terminology decisions, do the IA rename as one reviewable step, treat the canonical run store as a separate engineering project, and only then do responsive polish.

## Core Diagnosis

### 1. Too many navigation systems

Hive currently has several overlapping movement models:

- Primary destination/sidebar navigation
- Per-space panel lists
- Pipeline strip: `Context -> Plan -> Execute -> Validate -> Apply`
- Command palette / jump navigation
- Right-rail action links
- Inline action buttons inside panels

These do not all communicate the same hierarchy. The pipeline strip is the riskiest example: it represents status, but if it is drawn like a clickable wizard, users read it as navigation.

**Direction:** Pick one primary navigation model. Treat all other movement as secondary: status, shortcuts, contextual actions, or deep links.

### 2. Vocabulary overload

Hive uses many terms for a small set of concepts:

- Destinations
- Spaces
- Current Space
- Inside This Space
- Missions
- Runs
- Pipeline
- Guided run
- Panels

This increases cognitive load. Users should not need to learn Hive's internal architecture before using the product.

**Direction:** Standardize around a small vocabulary:

- **Workspace:** the project/folder Hive is operating on
- **Area:** a top-level part of the app
- **Panel:** a specific screen inside an area
- **Run:** work Hive is actively executing or has executed
- **Review:** changes, approvals, and outcomes needing user attention

Avoid using `Destination`, `Space`, and `Mission` as competing user-facing terms unless they are given distinct meanings.

### 3. Everything has near-equal weight

Hive exposes daily-driver surfaces and rare/admin surfaces as if they matter equally. For example, Chat, Files, Git/Review, Agents, Terminal, Models, Routing Matrix, Skills, Token Launch, and Network all compete for attention.

**Direction:** Tier panels by frequency:

- **Primary:** daily work surfaces
- **Secondary:** supporting workflow surfaces
- **Utility:** configuration, setup, advanced, and rarely used surfaces

### 4. First-run setup is scattered

The most important first-run path is:

1. Open a project
2. Connect or confirm a model
3. Start a run

Today this state is spread across Home cards, Settings, Files, Routing, model selectors, and panel empty states.

**Direction:** Home should own first-run setup as a dismissible checklist. Once setup is complete, Home should become a command center focused on the current workspace, active run, and next best action.

### 5. Redundant shell content

The shell currently risks repeating itself:

- `Home` area plus `Home` panel
- `Assist` plus `Assistant`
- `Current Space` card restating the selected area
- Panel descriptions visible in dense navigation lists

**Direction:** Remove repeated explanatory surfaces. Keep names visible for discovery, but move longer descriptions to onboarding, tooltips, or empty states.

## Target UX Model

### Top-Level Areas

Use a small set of stable top-level areas. These should be based on user jobs, not implementation modules.

| Area | Purpose | Primary Panels |
|---|---|---|
| Home | Start, resume, and understand current work | Setup, current run, recent workspaces, next action |
| Work | Build with project context | Chat, Files, Specs, Code Map, Terminal, History |
| Runs | Execute repeatable or agentic work | Agents, Workflows, Kanban, run history |
| Review | Inspect outcomes and apply changes | Git/Review, approvals, diffs, PR flow |
| Observe | Monitor safety, cost, logs, and activity | Activity, Monitor, Logs, Costs, Learning, Shield |
| Settings | Configure providers and advanced behavior | Models, Routing, Skills, Network, Settings, Help |

This is a target shape, not necessarily a one-commit migration. The existing `ShellDestination` layer is a good start, but names and groupings should be sharpened around user jobs.

### Navigation Rules

- The left rail should show top-level areas only.
- The active area should expose a compact panel switcher.
- The right rail should be optional, contextual, and collapsible.
- Command palette can reach every panel, but it should not compensate for unclear primary navigation.
- Advanced panels should stay reachable, but they should not dominate daily navigation.
- A panel should have one obvious home.

### Pipeline Rules

The pipeline should be one of two things:

1. **Real navigation:** each step opens a meaningful screen for that run stage.
2. **Status only:** render it as a compact chip such as `Stage: Execute`.

Do not render a permanent wizard-looking strip if it does not navigate.

Show pipeline detail only during an active run or when viewing run history.

## Recommended Vocabulary

Use these terms consistently:

| Term | Meaning |
|---|---|
| Workspace | The open project/folder and its context |
| Area | Top-level app section |
| Panel | Screen inside an area |
| Run | A unit of work Hive executes |
| Review | Changes, approvals, and decisions |
| Model | Provider/model configuration |
| Skill | Installed capability or extension |

Avoid mixing `Space`, `Destination`, and `Mission` unless product copy gives them clear and separate meanings.

## Panel Placement

Suggested steady-state mapping:

| Current Panel | Target Area | Notes |
|---|---|---|
| QuickStart / Home | Home | Default first-run and resume surface |
| Chat | Work | Primary work canvas |
| Files | Work | Project context |
| Specs | Work | Planning/spec surface |
| CodeMap | Work | Project understanding |
| Terminal | Work | Execution surface |
| History | Work | Conversation and work history |
| Agents | Runs | Merge conceptually with workflows |
| Workflows | Runs | Templates, active runs, history |
| Kanban | Runs | Task/run planning |
| Review / Git Ops | Review | Rename consistently |
| Activity | Observe | Default Observe panel |
| Monitor | Observe | Runtime status |
| Logs | Observe | Diagnostics |
| Costs | Observe | Spend and usage |
| Learning | Observe | Improvement signals |
| Shield | Observe | Consider label `Security` or `Safety` |
| Models | Settings | Important, but configuration-oriented |
| Routing | Settings | Merge with Models where possible |
| Routing Matrix | Settings | Advanced tab under Models/Routing |
| Skills | Settings | Extensions/capabilities |
| Network | Settings | Integrations/peers unless core to Runs |
| Assistant | Home or Work | Clarify whether this is a daily assistant or a panel |
| Token Launch | Settings / Labs | Keep out of daily nav unless central to current product |
| Help | Settings | Also reachable from command palette |

## Panel-Specific Notes

### Home

Home should answer:

- What workspace am I in?
- Is Hive ready to work?
- What is the next best action?
- What is currently running or waiting for me?

Recommended structure:

1. Current workspace header
2. First-run checklist when needed
3. Current or recent run
4. One primary action
5. Recent workspaces and secondary actions

Once first-run setup is complete, the setup checklist should collapse or disappear.

### Work

Work should feel like the main project workbench.

Primary path:

1. Describe the goal in Chat
2. Gather context from files/specs/code map
3. Start or continue a run
4. Send output to Review

Avoid making users choose between Chat, Specs, Files, Agents, and Git before they know what they are trying to do.

### Runs

Agents, Workflows, and Kanban should be framed around runs:

- Templates
- Active runs
- Scheduled/repeatable runs
- Run history
- Failed or blocked runs

Workflow cards should be compact and operational. Avoid making every card visually equal with identical primary buttons.

### Review

Use one name consistently: `Review`, `Git`, or `Changes`.

Recommended primary flow:

1. Review changed files
2. Inspect diff
3. Run validation
4. Stage/commit
5. Push/open PR

Repository status should be a compact header. The file list and diff should dominate the screen.

### Observe

Observe should be an inbox for things users need to know or decide:

- Pending approvals
- Failed runs
- Budget warnings
- Safety/security blocks
- Recent activity

Metric cards with zeroes should not dominate the default state. Lead with current risk and user action.

### Settings

Models, Routing, Skills, Network, Help, and low-frequency setup should live here.

Models and Routing should likely become one surface:

- Provider status
- Active model
- Fallback rules
- Budget controls
- Advanced routing matrix

Skills should use a compact list/detail layout rather than a large marketplace grid when the user is managing installed capabilities.

## Visual Direction

The current dark/cyan visual language is coherent, but it risks becoming one-note. Reserve bright cyan for primary actions and active/running state.

Guidelines:

- One primary CTA per section
- Neutral secondary buttons
- Clear warning/error/success semantics
- Fewer bordered cards in operational screens
- Compact tables/lists for dense work surfaces
- Consistent panel headers, gutters, and control heights
- Descriptions in dense nav should move to tooltips or onboarding
- Right rail should be hidden or collapsible at narrower widths

## Implementation Plan

### Phase 0: Release-blocking safety

Objective: close destructive-action and irreversible-action gaps before any broad redesign.

Tasks:

- Build one shared destructive-action confirmation component aligned with Hive's existing ApprovalGate pattern.
- Apply it to Files delete, Review/Git Ops destructive Git actions, Token Launch deploy, and clear/reset/delete actions in Logs, Costs, History, Prompt Library, and Shield.
- Use undo/trash where possible.
- Use typed acknowledgement for irreversible or financial actions.

Success criteria:

- No destructive human-triggered action bypasses the shared confirmation pattern.
- Financial/irreversible actions require explicit acknowledgement.

Implementation note, 2026-06-18:

- Added the shared destructive confirmation model and workspace modal.
- Wired Files delete, History delete/clear, Logs clear, Costs reset/clear, Prompt Library delete, Review discard all, branch delete, Gitflow finish, Shield rule delete, and Token Launch deploy.
- Token Launch deploy now requires a typed acknowledgement phrase.
- Shield rule deletion now dispatches to the root workspace confirmation flow instead of mutating inside the Shield view.

### Phase 1: Trust repair

Objective: make visible controls trustworthy before reorganizing the app.

Tasks:

- Verify static-looking controls before removal or replacement.
- Replace search-looking static divs with real GPUI inputs where search should exist.
- Wire or remove primary-looking buttons.
- Replace static form-like fields in Review/Git Ops with real editable controls.
- Add visible missing actions such as Save Current prompt.

Success criteria:

- No visible search/input/button appears interactive unless it is actually interactive.
- Primary CTAs either work or are removed.

Implementation note, 2026-06-18:

- Replaced static search-looking fields in Files, History, Logs, and Skills with real GPUI inputs backed by panel state/actions.
- Wired Review/Git Ops form-looking fields for commit message, PR title/body/base, branch name, LFS pattern, and Gitflow branch name to real inputs/actions.
- Added the visible Prompt Library `Save Current` action.
- Removed or demoted misleading controls that had no trustworthy handler: Specs `+ New Spec`, Logs `Refresh`, Kanban Filter/Move Selected/Delete Selected, and Shield `+ Add Rule` placeholder creation.

### Phase 2: Product question lock

Objective: settle the decisions that determine the IA rename.

Decisions:

- Drop the category noun; show top-level labels directly.
- Demote Assistant out of the top-level daily set or fold it into Home.
- Move Network to Settings/System unless it directly supports distributed execution in a run.
- Move Token Launch to Labs/advanced behind a flag.
- Treat pipeline as status, not navigation.

Success criteria:

- IA implementation can proceed without reopening basic vocabulary and placement questions.

Implementation note, 2026-06-18:

- Applied the locked shell decisions: top-level labels stand alone, `Build` is displayed as `Work`, `Automate` is displayed as `Runs`, Network is grouped under Settings, Assistant is folded into Home, and the pipeline is status-only.
- Token Launch is treated as Labs/advanced: it is hidden from sidebar/header/command-palette results unless `HIVE_ENABLE_LABS=1` is set, and the token action handlers also refuse to run while Labs is disabled.
- Help copy and Ctrl-number shortcut expectations were updated to match the visible shell order.

### Phase 3: Navigation and language cleanup

Objective: reduce cognitive load without rewriting panel internals.

Tasks:

- Default new sessions to Home when setup or current task state is unclear.
- Rename shell labels around user jobs.
- Remove or simplify the `Current Space` card.
- Replace long sidebar descriptions with short labels and optional tooltips.
- Decide whether pipeline is status or navigation.
- Create a clear `Settings` or `Connect` home for Models, Routing, Skills, Network, Help, and Labs.
- Update keyboard shortcuts, session restore, command palette results, and help copy in the same reviewable step.

Success criteria:

- A new user can identify where to start within 5 seconds.
- Every panel still has a reachable home.
- The left rail no longer presents advanced utilities as daily work surfaces.

Implementation note, 2026-06-18:

- Promoted Settings to a real shell destination and moved Models, Routing, Routing Matrix, Skills, Network, Token Launch, Settings, and Help under it.
- Removed the redundant `Current Space` card, renamed the active panel list to `Panels`, and removed the separate Utilities drawer.
- Grouped Agents, Workflows, Kanban, and Channels under the user-facing `Runs` label.
- Converted the permanent pipeline wizard into a compact stage status chip.
- Folded Assistant into Home and removed it from the top-level rail.
- Filtered Labs-only panels out of visible navigation until their feature flag is enabled.
- Aligned command palette panel results and help shortcut copy with the visible shell.

### Phase 4: First-run and Home command center

Objective: make setup and start-work flow obvious.

Tasks:

- Add a dismissible setup checklist: Open project, Connect model, Start run.
- Show setup blockers inline with direct actions.
- Promote current run or recent run as the central Home object.
- Collapse secondary content until setup is complete.
- Make Home the default route when no active context exists.

Success criteria:

- First-run user can complete setup without visiting multiple unrelated panels.
- Returning user sees current work and one next action.

Implementation note, 2026-06-18:

- Moved the setup checklist above the run launcher when setup is incomplete.
- Tightened setup labels to the first-run path: Open project, Connect model runtime, Choose launch model, then Start guided run.

### Phase 5: Runs and Review consolidation

Objective: make Hive's agentic work loop understandable.

Tasks:

- Group Agents, Workflows, Kanban, and run history under Runs.
- Rename Review/Git Ops consistently.
- Make changed files and diffs the dominant Review content.
- Ensure approvals appear both inline and in Observe.

Success criteria:

- Users can distinguish starting work, monitoring work, and reviewing outcomes.
- Review has a clear path from changed files to commit/PR.

### Phase 6: Observe and Settings refinement

Objective: separate operational visibility from configuration.

Tasks:

- Make Activity the default Observe surface.
- Lead Observe with approvals, failed runs, budget warnings, and safety events.
- Merge Models/Routing/Routing Matrix into one model configuration surface.
- Move Skills to a compact extension management pattern.
- Move low-frequency tools out of primary workflow areas.

Success criteria:

- Observe reads as an action inbox, not a metrics graveyard.
- Settings contains advanced configuration without blocking daily work.

### Phase 7: Canonical run model

Objective: create one source of truth for active, queued, completed, failed, and canceled runs.

Note: this is an architecture project, not a small UX cleanup. It should be scoped and reviewed separately after safety, trust, and IA work.

Tasks:

- Define one run store/model.
- Feed Agents, Workflows, Kanban, Specs, Activity, Monitor, and context rail from that model.
- Separate workflow templates from run instances.

Success criteria:

- Active run state has one canonical source.
- Panels no longer render templates as active runs or conflicting runtime truth.

Implementation note, 2026-06-18:

- Added a workspace-owned UI run store for workflow launches and automation run history.
- Agents no longer treats active/draft workflow templates as active runs.
- Workflow launches from Agents and Workflow Builder now create active run records and move them to history on completion/failure.
- Monitor and Observe derive current run/history metadata from the same run store.
- Remaining architecture work: extend the model to queued/canceled states, cancellation, step-level progress, and deeper Kanban/Specs integration.

### Phase 8: Visual and interaction pass

Objective: make the simplified IA feel polished and predictable.

Tasks:

- Standardize panel headers, section spacing, card density, and button hierarchy.
- Make the right rail collapsible and width-aware.
- Reduce repeated cyan primary buttons.
- Verify empty, loading, error, success, and disabled states.
- Check keyboard navigation and focus order.
- Capture screenshots at normal and constrained desktop widths.
- Rebase on latest main before implementation and avoid redoing completed Phase-1/P2 work: duplicate Home removal, sidebar description wrapping, rail metric overlap fixes, Home density/grid work, and model-dropdown scroll behavior.

Success criteria:

- UI density is consistent across panels.
- Text does not clip in navigation, cards, or buttons.
- Important status is visible without overwhelming the main task.

## Files Likely Involved

| File | Likely Change |
|---|---|
| `hive/crates/hive_ui_core/src/sidebar.rs` | Area/panel model, defaults, labels, grouping |
| `hive/crates/hive_ui/src/workspace/sidebar_shell.rs` | Left rail, panel switcher, utility drawer |
| `hive/crates/hive_ui/src/workspace/context_rail.rs` | Collapsible/context-specific rail |
| `hive/crates/hive_ui/src/workspace/chrome.rs` | Shell layout and rail visibility |
| `hive/crates/hive_ui/src/workspace/panel_router.rs` | Panel routing and default surfaces |
| `hive/crates/hive_ui_panels/src/panels/quick_start.rs` | Home, setup checklist, current run |
| `hive/crates/hive_ui_core/src/theme.rs` | Visual hierarchy tokens |

## Open Product Questions

- Should the user-facing top-level concept be called `Areas`, `Spaces`, or should labels stand alone without a category name?
- Is `Assistant` a core daily workflow, or should it be folded into Home/Work?
- Is `Network` a run execution surface or an advanced integration/configuration surface?
- Is `Token Launch` central to the product or a Labs/advanced capability?
- Should the pipeline become real navigation, or remain compact run status?

## Acceptance Criteria

- The app has one obvious place to start.
- The top-level nav has no more than 5-6 persistent areas.
- Daily work surfaces are visually prioritized over setup/admin surfaces.
- Every existing panel remains reachable through a clear route.
- First-run setup is linear and dismissible.
- The pipeline no longer looks like fake navigation.
- Naming is consistent across sidebar labels, page titles, and empty states.
- The right rail helps the active task and can get out of the way.
- Screenshots at common desktop widths show no clipped labels or overlapping controls.

## Guiding Principle

Hive should not ask users to browse its feature inventory. It should help them answer:

> What am I working on, what is Hive doing, and what needs my attention next?
