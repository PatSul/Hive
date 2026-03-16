# Wire Remaining Features Plan

**Date:** 2026-03-16
**Branch:** main
**Goal:** Wire or remove all 21 remaining dead/unwired features

## Current State

Previous work (commits up to `dabfc01`) wired 19 features:
ActivityService, ApprovalGate, NotificationService, HeartbeatScheduler,
RagService, SemanticSearch, TTS, Speculative Decoding, ContextEngine,
Embeddings, PII Shield, hive_docs generators, Docker (17 tools),
OllamaManager (4 tools), CliService, NotificationTray, DiffViewer (partial)

## Remaining: 21 Items in 4 Categories

---

### PHASE A: Wire Into Existing Pipelines (8 tasks, high value)

These features are built and tested — they just need the last-mile connection.

- [ ] **A1: BudgetEnforcer → Queen + Workspace**
  - Instantiate BudgetEnforcer in workspace.rs /swarm handler using config values
  - Pass to Queen via `queen.with_budget(budget)`
  - Already partially done: coordinator.rs checks `config.budget`, queen.rs has `with_budget()`
  - Files: `workspace.rs`

- [ ] **A2: ActivityService → Queen.with_activity()**
  - In workspace.rs /swarm handler, get AppActivityService from globals
  - Pass to Queen via `queen.with_activity(activity)`
  - Files: `workspace.rs`

- [ ] **A3: SecretScanner → chat pipeline**
  - In chat_service.rs, before sending messages to AI, call `SecretScanner::scan_text()`
  - Block or warn if secrets (API keys, tokens) detected in outgoing messages
  - PII detect/cloak already wired — add secrets next to it
  - Files: `chat_service.rs`

- [ ] **A4: CostTracker budget enforcement**
  - In AiService or chat_service, before sending API requests, check `is_daily_budget_exceeded()`
  - If exceeded, return error to user instead of sending request
  - Files: `chat_service.rs` or `hive_ai/src/service.rs`

- [ ] **A5: BudgetGauge → Costs panel**
  - In costs.rs render(), call `BudgetGauge::render()` using the existing `budget: BudgetGaugeData` field
  - Export BudgetGauge from components/mod.rs
  - Files: `costs.rs`, `components/mod.rs`

- [ ] **A6: DisclosureLevel click handlers**
  - In chat.rs message rendering, add click handler that calls `DisclosureLevel::next()`
  - Wire ToggleDisclosure action in workspace.rs
  - Show Summary by default, expand to Steps/Raw on click
  - Files: `chat.rs`, `workspace.rs`, `actions.rs`

- [ ] **A7: ThinkingIndicator → chat**
  - In chat.rs, when AI is streaming, render ThinkingIndicator component
  - Export from components/mod.rs
  - Files: `chat.rs`, `components/mod.rs`

- [ ] **A8: Toast → notification display**
  - Wire Toast component to NotificationService — when notifications push, show toast
  - Export from components/mod.rs, render in workspace overlay
  - Files: `workspace.rs`, `components/mod.rs`

---

### PHASE B: Wire As Tools/Features (5 tasks, medium value)

- [ ] **B1: CapabilityRouter → AiService**
  - In `hive_ai/src/service.rs` `resolve_provider()`, replace basic `route()` with `route_with_capabilities()` when task has specific requirements (code gen, vision, etc.)
  - Files: `hive_ai/src/service.rs`, `hive_ai/src/routing/model_router.rs`

- [ ] **B2: LocalSearch (SearXNG) → MCP tool**
  - Add `local_search` tool to integration_tools.rs
  - Instantiate LocalSearchService in main.rs if SearXNG is available
  - Files: `integration_tools.rs`, `main.rs`

- [ ] **B3: QuickIndex → project open**
  - On project directory change, run `QuickIndex::build()` to index the project
  - Store result for use by ContextEngine and RagService
  - Files: `workspace.rs`

- [ ] **B4: ModelSelector → settings panel**
  - Render ModelSelectorView in settings panel under AI provider section
  - Allow user to pick default model, see available models
  - Files: `settings.rs`, `components/mod.rs`

- [ ] **B5: Google/Microsoft OAuth token provisioning**
  - Add "Connect Google" / "Connect Microsoft" buttons in settings panel
  - On click, launch OAuth flow using existing OAuthClient
  - Store tokens for Gmail, Calendar, Drive, Sheets, Docs, Tasks, Contacts
  - Files: `settings.rs`, `workspace.rs`

---

### PHASE C: Remove Confirmed Dead Code (5 tasks)

These features have no use case, no callers, and no roadmap. Delete them.

- [ ] **C1: Remove TOON encoding** (`hive_ai/src/toon.rs`)
  - Never used, format not selected anywhere
  - Remove file + mod declaration + any re-exports

- [ ] **C2: Remove ClawdTalkClient** (`hive_integrations/src/clawdtalk.rs`)
  - Phone bridge to nonexistent service
  - Remove file + mod declaration + any re-exports

- [ ] **C3: Remove unused UI components**
  - WalletCard (blockchain not ready)
  - WizardStepper (no wizard flows)
  - SplitPane (not used in any panel)
  - ContextAttachment (no attachment system)
  - Remove files + mod declarations + re-exports from components/mod.rs

- [ ] **C4: Remove AgentSandbox/DockerSandbox stubs**
  - Never instantiated, no callers
  - Files: `hive_terminal/src/sandbox.rs`

- [ ] **C5: Remove unused cloud provider stubs**
  - Cloudflare, Supabase, Vercel clients — tests only, no production callers
  - Keep AWS/Azure/GCP (initialized in main.rs)

---

### PHASE D: Verify & Ship (3 tasks)

- [ ] **D1: Full workspace compile check**
  - `cargo check --workspace --exclude hive_app`
  - Fix any errors from removals/wiring

- [ ] **D2: Run test suite**
  - `cargo test -p hive_agents --lib`
  - `cargo test -p hive_ui --lib`
  - `cargo test -p hive_ai --lib`
  - Fix any failures

- [ ] **D3: Version bump, commit, push, CI**
  - Bump to 0.3.31
  - Update README changelog
  - Push to main
  - Verify CI passes all 3 platforms

---

## Execution Strategy

**Phase A** (8 tasks): Dispatch in 2 waves of 4 (no file conflicts within wave)
- Wave 1: A1+A2 (workspace.rs), A3 (chat_service.rs), A5 (costs.rs), A7 (chat.rs)
- Wave 2: A4 (service.rs), A6 (chat.rs+actions.rs), A8 (workspace.rs)

**Phase B** (5 tasks): 2 waves
- Wave 1: B1 (service.rs), B2 (integration_tools.rs), B3 (workspace.rs)
- Wave 2: B4 (settings.rs), B5 (settings.rs+workspace.rs)

**Phase C** (5 tasks): All parallel (independent file deletions)

**Phase D** (3 tasks): Sequential

## File Conflict Map

Files touched by multiple tasks (serialize these):
- `workspace.rs`: A1, A2, A6, A8, B3, B5
- `chat.rs`: A6, A7
- `settings.rs`: B4, B5
- `components/mod.rs`: A5, A7, A8, B4
- `service.rs`: A4, B1
- `integration_tools.rs`: B2
- `chat_service.rs`: A3
- `main.rs`: B2
