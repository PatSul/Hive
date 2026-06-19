# Hive UX Panel Audit

**Status:** Living inventory  
**Created:** 2026-06-18  
**Scope:** All 28 current `Panel::ALL` surfaces plus shell/navigation behavior  
**Method:** Multi-agent source audit across shell/home, workbench, runs/automation, review/observe, and settings/utility panel families

## Executive Summary

The complete panel audit confirms the overview in `docs/UX-REVIEW.md`: Hive's main UX problem is not visual style, it is action clarity and information architecture.

The source of truth currently exposes 28 navigable panels in `hive/crates/hive_ui_core/src/sidebar.rs`. The prior "27 panels" count was a rough estimate; the current code has 28.

The highest-risk findings are:

- **P0:** `Files` exposes permanent one-click file/folder delete through `remove_file` / `remove_dir_all` with no confirmation, undo, or trash.
- **P0:** `Review / Git Ops` exposes destructive Git actions such as discard, branch delete, and Gitflow finish without confirmation or undo.
- **P0:** `Token Launch` exposes a financial/irreversible deploy path without an explicit confirmation, testnet guard, or typed acknowledgement.
- **P1:** Several panels render controls that look interactive but are static divs or unwired buttons.
- **P1:** Runtime truth is fragmented across Agents, Workflows, Monitor, Activity, and the context rail.
- **P1:** Utility panels can leave the shell highlighting the wrong destination.
- **P1:** Setup and empty states frequently tell users to go elsewhere instead of giving direct CTAs.

## Reviewer Feedback

This audit is endorsed with sequencing changes:

- The P0 safety work is the real headline and should be decoupled from the IA redesign. These fixes are valuable regardless of the future shell model.
- Hive already gates agent file writes through the ApprovalGate, but several human-triggered one-click deletes/discards bypass that same safety pattern. The fix should be one shared destructive-action confirmation component, ideally aligned with the existing approval pattern, not a set of bespoke dialogs.
- The canonical run store is not a normal UX cleanup slice. It is an architecture project with the highest regression risk in the plan. Track it as its own engineering effort after safety and trust repairs.
- IA rename/restructure is correct but high-blast-radius. It touches `Panel`, `ShellDestination`, routing, keyboard shortcuts, persisted session strings, and help copy. Do it as one reviewable step after product questions are settled.
- Trust fixes should be verified before removal. Some static-looking controls may be command-palette or action-backed WIP; the rule is still that visible controls must become real, or they should be removed from the visible UI.

Reviewer answers to the open product questions:

| Question | Decision |
|---|---|
| Category noun: Areas/Spaces/none? | Drop the category noun. Show the top-level labels directly. |
| Assistant placement? | Not top-5 for a dev-AI product. Demote to secondary or fold into Home. |
| Network placement? | Advanced config/System unless it directly affects distributed execution in a run. |
| Token Launch placement? | Labs/advanced behind a flag, with hard safety gates. |
| Pipeline behavior? | Status, not navigation. Use a compact chip; show full detail only during active runs. |

Current-main reconciliation before implementation:

- Rebase on latest main first.
- Do not redo Phase-1/P2 items that are already complete today: duplicate Home removal, sidebar description wrapping, rail metric overlap fixes, Home density/grid work, and model-dropdown scroll behavior.
- Shell items completed in this pass: removed the `Current Space` card, converted pipeline to a status chip, folded Assistant into Home, added Settings as a real destination, and moved first-run setup higher in Home.

## Severity Key

| Severity | Meaning |
|---|---|
| P0 | Release-blocking UX/safety issue: destructive, financial, or state-corrupting behavior without adequate guardrails |
| P1 | High-impact issue that breaks user trust, creates false state, blocks a workflow, or makes the UI misleading |
| P2 | Medium-impact clarity, density, consistency, accessibility, or polish issue |

## Cross-Cutting Findings

### P0: Destructive actions need a shared confirmation pattern

Several panels expose deletion, discard, reset, clear, or deployment actions as ordinary clicks. Hive needs one shared destructive-action pattern:

- Clear action label
- Consequence summary
- Affected object list/count
- Confirm/cancel
- Undo or trash where possible
- Typed acknowledgement for irreversible/financial actions

Immediate candidates: `Files`, `Review / Git Ops`, `Token Launch`, `Logs`, `Costs`, `History`, `Prompt Library`, and `Shield`.

The preferred implementation is a shared confirmation/approval component aligned with Hive's existing ApprovalGate behavior. The goal is not 15 panel-specific dialogs; it is one reusable destructive-action contract that human-triggered file deletes, Git discards, branch deletes, Gitflow finish, token deploy, and clear/reset actions all use consistently.

### P1: Too many fake or partially wired controls

Search fields, inputs, buttons, and tabs often look interactive but are static or only partially wired. This is worse than a missing feature because it teaches users not to trust the UI.

Common examples:

- Static search-looking fields in `Files`, `History`, `Logs`, and `Skills`
- Static edit-looking fields in `Review / Git Ops`
- Buttons with no handler in `Specs`, `Logs`, and `Review / Git Ops`
- Registered actions with no visible control, such as prompt save and some search setters

### P1: Runtime/run state has no canonical model

Run state appears in multiple places, but no single UI model appears to own active/queued/completed/failed/canceled runs.

Affected panels:

- `Agents`
- `Workflows`
- `Kanban`
- `Specs`
- `Activity`
- `Monitor`
- Context rail

Recommendation: create one canonical `Run` store and feed all run-aware panels from it.

Sequencing note: this should be tracked as a dedicated engineering project, not bundled into the first UX cleanup pass.

### P1: Utility panels are detached from navigation

Utility panels return no `ShellDestination`, so opening Settings/Models/Routing can leave the prior destination highlighted while the main content is a utility screen.

Recommendation: make utilities a real `Settings` / `Configure` destination, or keep the utility drawer open and visibly active while a utility panel is selected.

### P1: Small-window layout is not protected

The app can restore to very small window sizes while using fixed shell widths:

- Sidebar: about `232px`
- Context rail: about `296-332px`
- Command palette: about `720px`

Recommendation: add breakpoints. Collapse the context rail first, shorten nav descriptions, and set a realistic minimum window size for the full shell.

### P1: Setup is scattered

First-run setup currently spans Home, Settings, Files, Models, Routing, and panel empty states.

Recommendation: Home owns the checklist:

1. Open project
2. Connect model
3. Start run

Other panels should deep-link back into that checklist or directly into the relevant setup subsection.

## Canonical Panel Inventory

| Panel | Current Placement | Target Placement | Primary Goal | Top Risk |
|---|---|---|---|---|
| QuickStart / Home | Home | Home | Start/resume work and clear setup blockers | Home data can be stale/empty on restore |
| Chat | Build | Work | Start and steer AI work | Important chat actions are hidden in shortcuts |
| History | Build | Work | Resume prior conversations | Static search and unsafe delete behavior |
| Files | Build | Work | Browse/select project context | P0 one-click permanent delete |
| Code Map | Build | Work / Symbols | Inspect indexed symbols | Passive list with no open/filter actions |
| Prompt Library | Build | Work / Prompts | Reuse prompt templates | Missing visible Save Current flow |
| Specs | Build | Work / Plan | Track implementation specs | Primary CTA and detail/edit flows unwired |
| Agents | Build | Runs | Inspect agents/capabilities | Templates rendered as active runs |
| Workflows | Automate | Runs | Build reusable workflow templates | Save/register/run semantics are unclear |
| Channels | Automate | Runs / Collaborate | Coordinate multi-agent conversations | Channel state and stream state can be wrong |
| Kanban | Build | Runs / Plan | Plan tasks and launch task runs | Run/destructive controls lack guardrails |
| Monitor | Observe | Observe / Runtime | Detailed runtime/system dashboard | Platform/provider health claims are misleading |
| Activity | Observe | Observe / Inbox | Approvals and operational inbox | Duplicates other Observe dashboards |
| Logs | Observe | Observe / Diagnostics | Inspect logs | Search/refresh fake; clear unsafe |
| Costs | Observe | Observe / Spend | Inspect spend and budgets | Reset/clear immediate with weak feedback |
| Review | Build | Review / Git | Review and ship changes | P0 destructive Git actions lack confirmation |
| Skills | Utility | Settings / Extensions | Manage skills/plugins | Static search and weak import safety hierarchy |
| Routing | Utility | Settings / Models & Routing | Observe routing behavior | Add Rule creates blank rule with no editor |
| Routing Matrix | Utility | Settings / Models & Routing Advanced | Edit routing policy | High-density expert editor exposed too directly |
| Models | Utility | Settings / Models & Routing | Browse/select models | No-key empty state lacks CTA |
| Learning | Observe | Observe / Learning Insights | Model/self-improvement insight | Internal terms and no policy actions |
| Shield | Observe | Observe + Settings / Security | Monitor/configure safety | Placeholder active rules and immediate deletes |
| Assistant | Assist | Home/Assist | Personal operational assistant | Approvals and setup states lack action CTAs |
| Token Launch | Utility | Separate Launch/Web3 area | Deploy/import token workflow | P0 deploy lacks explicit safety gate |
| Network | Automate | Settings or Runs | Peer/distributed readiness | Service unavailable and no-peer states collapse |
| Terminal | Build | Work | Run commands in project context | CWD can drift from selected workspace |
| Settings | Utility | Settings | Configure providers/integrations/app | Too many domains and unclear save state |
| Help | Utility | Help / Support | In-app guidance | Full manual without TOC/search/collapse |

## Shell And Home Findings

### Shell / Navigation

**Goal:** Provide one stable navigation model and clear current location.

Findings:

- **P1:** Utility panels are visually detached from navigation; selecting Settings/Models/Routing can keep the previous destination highlighted.
- **P1:** Fixed-width shell elements are risky at small restored window sizes.
- **P1:** Overlay/backdrop stacking is fragile because project dropdown, command palette, and utility backdrops are mounted before main content without explicit stacking.
- **P2:** Sidebar carries long descriptions plus a `Current Space` explainer, making nav read like documentation.
- **P2:** Local tabs duplicate sidebar panel navigation.
- **P2:** Keyboard shortcuts still follow the old raw panel order instead of the visible destination/panel order.

Recommendations:

- Create a real `Settings`/`Configure` destination for utility panels.
- Remove the `Current Space` card or reduce it to a compact active-area label.
- Collapse the pipeline unless an active run exists.
- Make the context rail collapsible below a defined width.
- Align keyboard shortcuts with visible navigation, or hide shortcut hints where bindings do not match.

### QuickStart / Home

**Goal:** Start, resume, and understand current work.

Findings:

- **P1:** Home can render from `QuickStartPanelData::empty()` on session restore and show stale/blank state until a manual switch refreshes it.
- **P2:** `Finish Setup First` looks disabled but remains clickable and redirects to Settings, which is ambiguous action hierarchy.
- **P2:** Workspace switching appears in titlebar, sidebar header, and Home Projects with different labels.

Recommendations:

- Refresh Home data before first Home render and again after bootstrap/indexing changes.
- Replace fake-disabled launch with either a true disabled state plus setup CTA, or a primary `Finish setup` action.
- Make Home the single owner of first-run setup.

## Workbench Panels

### Chat

**Goal:** Start and steer AI work in the active project.

Findings:

- **P2:** New conversation and clear chat actions exist as shortcuts/actions but are not visible in the Chat panel.
- **P2:** Message header metadata can overflow at narrow widths because model, cost, and read-aloud controls share a non-wrapping row.

Recommendations:

- Add a compact chat toolbar with `New chat`, `Clear`, model/status, and conversation metadata.
- Allow message metadata to wrap or collapse into an overflow menu.

### Files

**Goal:** Browse project files, open content, and select context for Chat/Runs.

Findings:

- **P0:** Delete is always available and calls permanent file/folder removal with no confirmation, undo, or trash.
- **P1:** Search looks like an input but is rendered as a static `div`; search actions exist separately.
- **P2:** Context checkbox is small and nested inside a clickable row, creating misclick/open-file risk.

Recommendations:

- Replace delete with confirm + trash/undo where possible.
- Use a real input for search.
- Separate row open behavior from context checkbox behavior and increase hit target size.

### History

**Goal:** Find and resume previous conversations.

Findings:

- **P1:** Search field is static; there is no input-backed conversation search UI.
- **P1:** Per-conversation delete is immediate and nested inside a card that also loads the conversation.
- **P2:** Empty state says `Start chatting!` but provides no direct Chat action.

Recommendations:

- Add real search input and keyboard selection.
- Add delete confirmation or move delete into a secondary menu.
- Add `Start chat` CTA in empty state.

### Code Map

**Goal:** Inspect indexed code symbols grouped by file.

Findings:

- **P1:** Symbols are passive: not clickable, cannot open files, and no visible filter UI is rendered.
- **P2:** Empty state mentions indexing but gives no reindex/open Files action.

Recommendations:

- Rename to `Symbols` unless it becomes a graph/map.
- Make symbol rows open the file at the symbol.
- Add filter/search and reindex/open-project CTAs.

### Prompt Library

**Goal:** Reuse, save, load, and delete prompt templates.

Findings:

- **P1:** Empty state tells users to use `Save Current`, and a handler exists, but the panel exposes no visible `Save Current` button.
- **P1:** Sidebar says `Prompts` while panel title says `Prompt Library`.
- **P2:** Template delete is immediate with no confirmation.

Recommendations:

- Use one label, preferably `Prompts`.
- Add `Save current prompt` as a visible primary action when applicable.
- Confirm delete or move it into an overflow menu.

### Specs

**Goal:** Track implementation specs and run planned work.

Findings:

- **P1:** `+ New Spec` is styled as a primary CTA but has no click handler.
- **P1:** Detail/edit modes exist in rendering logic but are unreachable from list cards; Back is also static.
- **P1:** Status rendering expects strings such as `Complete` / `In Progress`, while live data uses enum/debug-style values such as `Active` / `Completed`.

Recommendations:

- Wire or remove `+ New Spec`.
- Make list cards open details and wire Back/Edit.
- Normalize spec statuses through a typed display mapping.

### Terminal

**Goal:** Run commands in the active project context.

Findings:

- **P1:** Terminal initializes from process current directory and is not clearly updated on workspace switch.
- **P2:** Kill is always visible even when idle.
- **P2:** Empty/cleared output has no helper state.

Recommendations:

- Bind terminal CWD to the active workspace and show it clearly.
- Disable/hide Kill unless a command is running.
- Add empty state with examples and current directory.

## Runs And Automation Panels

### Agents

**Goal:** Choose agents, inspect capabilities, and understand agent execution.

Findings:

- **P1:** `Active Runs` is populated from active/draft workflow definitions, not actual in-flight executions.
- **P1:** Remote A2A, workflow templates, run history, and personas are mixed in one long scroll with competing primary actions.
- **P2:** Empty active-run copy says to click `Run Spec`, but that action is not available in the panel.

Recommendations:

- Narrow Agents to agent capability/catalog, or move it under `Runs` with tabs.
- Feed Active Runs from a canonical run store.
- Replace stale `Run Spec` copy with actual available actions.

### Workflows

**Goal:** Create, edit, validate, and register reusable workflow templates.

Findings:

- **P1:** Save/register and Run are treated as peer actions, but they mean different things. Users cannot tell whether they are running a draft, saved template, or registered workflow.
- **P1:** Workflow execution starts/completes through notifications but has no durable active-run row, step progress, cancel affordance, or running state on the canvas button.
- **P2:** Palette exposes unsupported nodes such as Condition/Execute Skill, then blocks runtime validation.

Recommendations:

- Split `Templates`, `Builder`, and `Runs`.
- Show run state in the canvas and in a canonical Runs surface.
- Hide unsupported nodes or label them as disabled/upcoming.

### Channels

**Goal:** Coordinate a multi-agent conversation with clear participant and response state.

Findings:

- **P1:** Channel selection changes local `active_channel_id` but does not load selected channel messages; stale messages can appear under a new channel header.
- **P1:** Multiple agent responses share one `is_streaming` / `streaming_agent` slot, so concurrent progress can overwrite itself.
- **P2:** New channel instantly creates `#custom-n` with all default agents, without naming or participant selection.

Recommendations:

- Load channel messages when selection changes.
- Track streaming state per agent/message.
- Use a creation dialog with name and participant selection.

### Kanban

**Goal:** Plan work items and launch/track task-specific runs.

Findings:

- **P1:** Task `Run` launches an inferred workflow with no confirmation, command preview, progress link, or task-level run state.
- **P1:** Toolbar exposes Filter, Move Selected, and Delete Selected as active-looking controls without handlers/selection model.
- **P2:** UI uses a simplified panel-local Kanban model while core has richer board/task behavior.

Recommendations:

- Make task Run open a run sheet with command preview, selected template, confirmation, progress, cancel, and log link.
- Hide or wire toolbar controls.
- Align UI model with the richer core Kanban model.

### Network

**Goal:** Understand distributed execution readiness and peer connectivity.

Findings:

- **P2:** Missing network service and no peers both collapse into empty peer data / `Not initialized`.
- **P2:** Refresh has no refreshing, last-refreshed, discovery progress, or failure state.

Recommendations:

- Add explicit states: unavailable, disabled, discovering, no peers, connected, failed.
- Keep Network under Runs only if it affects distributed execution; otherwise move it to Settings/System.

## Review And Observe Panels

### Review / Git Ops

**Goal:** Safely review working tree state and ship changes through commit, push, PR, branch, LFS, and Gitflow workflows.

Findings:

- **P0:** `Discard All`, branch delete, and Gitflow finish are one-click destructive actions with no confirm/undo flow.
- **P0:** `Discard All` runs `git checkout -- .`, which does not match the word `All` for untracked files.
- **P1:** `Review Decision` buttons render as buttons but do not dispatch actions.
- **P1:** Commit message, PR title/body/base, branch name, LFS pattern, and Gitflow name render like inputs but are static divs.
- **P2:** Terminology splits across enum `Review`, sidebar `Git Ops`, header `Code Review`, and `GitOpsTab`.

Recommendations:

- Add confirmation/undo for destructive Git operations.
- Use real inputs for commit/PR/branch/LFS/Gitflow forms.
- Choose one label. Recommended: `Git` or `Review`.
- If review verdicts are real, split them into a focused review flow.

### Activity

**Goal:** Observe inbox for approvals, failures, and recent operational evidence.

Findings:

- **P1:** Activity has the strongest inbox pattern, but Runtime/Spend/Safety tabs duplicate Monitor/Costs/Learning/Shield and blur inbox vs dashboard.
- **P2:** Deny uses a fixed reason with no reason capture.
- **P2:** Export CSV is registered but only logs a request; no visible export result.
- **P2:** Search query is data-backed but no search UI appears.

Recommendations:

- Make Activity the Observe default and rename it `Inbox`.
- Keep approvals/failures/warnings central; move dashboards to separate panels.
- Add optional deny reason and export feedback.

### Monitor

**Goal:** Detailed runtime/system dashboard.

Findings:

- **P1:** Resource collection is macOS-command based, so Windows can show zeros while the dashboard still claims live status.
- **P1:** Provider `online` is based on config/key presence with `0ms` latency, not real health.
- **P2:** Static `Available Agent Roles` competes with live runtime signals.
- **P2:** Refresh has no loading/error state.

Recommendations:

- Rename to `Runtime`.
- Distinguish unavailable/stale/sampled platform data.
- Treat provider key presence separately from live provider health.

### Logs

**Goal:** Diagnostic log inspection.

Findings:

- **P1:** Header `Refresh` is cursor-styled but has no action.
- **P1:** Search field is non-editable and no logs search action is registered.
- **P1:** Clear deletes logs immediately without confirmation.
- **P2:** Empty/error states do not distinguish no logs from database unavailable.

Recommendations:

- Wire Refresh or remove it.
- Add real search.
- Confirm clear and report success/failure.

### Costs

**Goal:** Spend dashboard and cost data management.

Findings:

- **P1:** `Reset Today` and `Clear History` execute immediately without confirmation or success feedback.
- **P2:** Budget gauge returns an empty div when no budget is configured.
- **P2:** Dense table labels such as `Input Tok` reduce readability.

Recommendations:

- Rename to `Spend`.
- Move destructive reset/clear into overflow or Settings data management.
- Add empty budget state and readable table labels.

### Learning

**Goal:** Model quality and self-improvement insight surface.

Findings:

- **P1:** Shows auto-apply/routing state but offers no action to pause, approve, inspect, or change policy.
- **P2:** If learning service is unavailable, panel silently remains empty/default.
- **P2:** Internal terms such as `Cortex`, `soaking`, `tier`, and `$/Quality` are hard to parse.

Recommendations:

- Rename to `Learning Insights` or `Model Learning`.
- Add policy controls or deep links to policy settings.
- Translate internal terms into user-facing labels.

### Shield

**Goal:** Safety/privacy monitoring and guardrail configuration.

Findings:

- **P1:** `+ Add Rule` immediately persists an active placeholder regex `pattern` with no editor or validation first.
- **P1:** Rule delete and master disable are immediate.
- **P2:** Severity relies heavily on color/dots, and delete is icon-only.
- **P2:** Recent activity and policies lack source/provider recency detail.

Recommendations:

- Rename to `Security` or `Privacy Shield`.
- Create rules in draft state, validate before enabling, and confirm deletion.
- Move configuration-heavy controls to Settings > Security while keeping safety summary in Observe.

## Settings And Utility Panels

### Skills

**Goal:** Install, enable, create, import, and source skills/plugins.

Findings:

- **P1:** Search looks interactive but is not input-backed.
- **P1:** Import security warnings do not change action hierarchy; `Install Selected` remains primary without required acknowledgement.
- **P2:** Installed, Directory, Create, and Add Source mix novice marketplace use with advanced authoring/source administration.

Recommendations:

- Split default views into `Installed` and `Discover`.
- Move Create, Import Plugin, and Sources into Advanced.
- Require acknowledgement when import warnings exist.

### Routing

**Goal:** Observe routing behavior, provider health, fallback rules, and custom routing rules.

Findings:

- **P1:** `+ Add Rule` creates a blank rule, but the panel has no rule editor.
- **P1:** Live metrics are mixed with hardcoded model registry, fallback chains, and hard rules, risking stale/conflicting truth.
- **P2:** Fixed-width table cells can overflow long providers, model IDs, and conditions.

Recommendations:

- Merge with Models/Routing Matrix into `Models & Routing`.
- Make Routing mostly diagnostics/read-only unless an editor exists.
- Use responsive tables or detail drawers for long rules/models.

### Routing Matrix

**Goal:** Edit per-task routing policy, model allow-lists, floors, and cost preference.

Findings:

- **P1:** Repeats model chips across many task categories with no collapse/search/provider filter.
- **P1:** Expert terms such as allow-list, floor, and cost aggressiveness are exposed as the primary interaction model.
- **P2:** Save is only at the bottom of a long scroll, with no dirty/reset state.

Recommendations:

- Make this the Advanced tab of `Models & Routing`.
- Add search/filter/collapse.
- Add sticky Save/Reset and dirty-state feedback.

### Models

**Goal:** Browse provider catalogs and curate project models for routing.

Findings:

- **P2:** No-key empty state tells users to go to Settings but gives no direct action button.
- **P2:** Refresh is icon-only.
- **P2:** Long model names plus pricing metadata can crowd rows.

Recommendations:

- Add `Connect provider` / `Open model settings` CTA.
- Add labelled refresh or tooltip.
- Add row expansion for pricing/capability detail.

### Assistant

**Goal:** Show daily operational assistant data: briefing, events, email, reminders, approvals, research.

Findings:

- **P1:** Pending approvals are displayed but cannot be approved/rejected.
- **P1:** Setup empty state says to connect accounts but has no Settings/Connected Accounts CTA.
- **P2:** Many sections render in one long scroll without priority grouping.

Recommendations:

- Add actionable approval controls or deep-link to Observe Inbox.
- Add setup CTA for connected accounts.
- Group by priority: needs attention, schedule/email, research/actions.

### Token Launch

**Goal:** Create/import wallet and deploy a token.

Findings:

- **P0:** `Deploy Token` is a direct financial/irreversible action with no explicit confirmation, testnet guard, or typed acknowledgement in UI.
- **P1:** Token Launch is not a utility/config panel; placing it beside Settings/Help hides risk and intent.
- **P2:** Disabled Next gives no reason; validation is implicit.

Recommendations:

- Move to a dedicated `Launch` or `Web3` workflow area if it remains in product.
- Add network/testnet visibility and an explicit confirmation step.
- Require typed acknowledgement for mainnet/irreversible deploy.
- Explain validation blockers inline.

### Settings

**Goal:** Configure provider keys, local AI, routing defaults, budgets, integrations, theme, backup, and app behavior.

Findings:

- **P1:** One panel mixes first-run essentials with advanced integrations, smart home, TTS, backup, routing, budgets, and appearance.
- **P1:** Auto-save on blur/toggle has no persistent saved/dirty state, risky for API keys and budgets.
- **P2:** OAuth setup requires client IDs plus Connect buttons, but the sequence is dense and developer-oriented.

Recommendations:

- Split first-run setup from advanced settings.
- Add dirty/saved/error state.
- Create guided connected-account setup.

### Help

**Goal:** In-app guidance, quick start, feature catalog, shortcuts, credits, and support.

Findings:

- **P1:** Help is a full manual with no table of contents, search, or collapsible sections.
- **P2:** Copy references stale or incorrect IA terms such as `Core section`.
- **P2:** Two-column feature/credit rows can overflow at narrow widths.

Recommendations:

- Add TOC/search and collapsible sections.
- Update copy after IA rename.
- Move credits/about/support to secondary sections.

## Recommended Remediation Order

### Slice 1: Release-blocking safety

- Build one shared destructive-action confirmation component aligned with the existing ApprovalGate pattern.
- Apply it to Files delete.
- Apply it to Review/Git Ops destructive actions: discard, branch delete, Gitflow finish.
- Apply it to Token Launch deploy with network/testnet visibility and typed acknowledgement for irreversible deploys.
- Apply it to Logs, Costs, History, Prompt Library, and Shield clear/delete/reset actions.

Status, 2026-06-18: implemented for the audited human-triggered destructive paths. A shared destructive confirmation model/modal now covers Files delete, History delete/clear, Logs clear, Costs reset/clear, Prompt Library delete, Review discard all, branch delete, Gitflow finish, Shield rule delete, and Token Launch deploy. Token deploy requires a typed acknowledgement phrase.

### Slice 2: Trust repair for fake controls

- Verify each static-looking control before changing it.
- Replace static search fields with real GPUI `Input` widgets where the feature should exist, or remove the search-looking UI.
- Wire or remove primary-looking buttons.
- Replace static form fields in Review/Git Ops with real editable controls.
- Add visible missing actions such as Save Current prompt.

Status, 2026-06-18: partially implemented for the highest-trust surfaces. Files, History, Logs, and Skills search are real inputs. Review/Git Ops commit/PR/branch/LFS/Gitflow fields are real inputs. Prompt Library exposes Save Current. Unwired Specs, Logs, Kanban, and Shield placeholder controls were removed. Remaining trust work should focus on lower-priority panel-specific empty states and any newly discovered static controls.

### Slice 3: Product question lock

- Drop the category noun and show top-level labels directly.
- Demote Assistant out of the top-level daily set or fold it into Home.
- Move Network to Settings/System unless it directly supports distributed run execution.
- Move Token Launch to Labs/advanced behind a flag.
- Treat pipeline as status, not navigation.

Status, 2026-06-18: applied in the shell. Labels stand alone, Network is in Settings, pipeline is status-only, Assistant is folded into Home, and Token Launch is Labs-gated behind `HIVE_ENABLE_LABS=1` with action-handler guards.

### Slice 4: IA cleanup

- Promote a real Settings/Configure destination.
- Collapse Models/Routing/Routing Matrix into `Models & Routing`.
- Move Agents/Workflows/Kanban/Channels toward `Runs`.
- Rename Activity to `Inbox`, Monitor to `Runtime`, Costs to `Spend`, and Code Map to `Symbols` unless behavior changes.
- Update keyboard shortcuts, session restore, help copy, and command palette results in the same reviewable step.

Status, 2026-06-18: implementation pass complete for the shell scope. Settings is a real destination, utilities no longer leave the prior destination highlighted, Agents/Workflows/Kanban/Channels are under Runs, Assistant is under Home, the redundant Current Space card and Utilities drawer are removed, and the panel sub-list is simply `Panels`. Help copy and Ctrl-number shortcut expectations now match the visible shell.

### Slice 5: Canonical run model

- Treat this as a dedicated architecture effort.
- Define one run store with active, queued, completed, failed, canceled states.
- Feed Agents, Workflows, Kanban, Specs, Activity, Monitor, and context rail from it.
- Separate templates from runs everywhere.

Status, 2026-06-18: first engineering slice implemented. A workspace-owned run store now records workflow launches and automation history. Agents no longer renders workflow templates as active runs, and Workflow Builder/Agents launches move active runs to history on completion/failure. Monitor and Observe consume the same run metadata. Remaining scope: queued/canceled states, cancellation, step-level progress, and deeper Kanban/Specs integration.

### Slice 6: Responsive polish

- Add shell breakpoints.
- Make context rail collapsible.
- Standardize panel headers and action rows.
- Add empty/loading/error/success states consistently.
- Verify at normal and constrained desktop widths.

## Acceptance Criteria

- All 28 panels have one clear home and one clear user goal.
- No destructive action executes without confirmation or undo/escape path.
- No search-looking or input-looking control is static.
- Home owns first-run setup.
- Active run state comes from one canonical source.
- Utility/config panels do not leave the shell in a misleading active destination.
- Empty states offer direct next actions.
- Panel names match sidebar labels, headers, and help copy.
- Narrow windows do not clip key labels or hide critical actions.
