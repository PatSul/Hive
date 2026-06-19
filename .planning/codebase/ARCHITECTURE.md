# Architecture

**Analysis Date:** 2026-06-18

## Pattern Overview

**Overall:** Native Rust desktop platform with a multi-crate workspace, service-oriented internal modules, local-first persistence, and optional network/cloud surfaces.

**Key Characteristics:**
- Single native GPUI desktop app launched from `hive/crates/hive_app/src/main.rs`.
- Workspace split into focused crates under `hive/crates/`.
- GPUI globals are used as the dependency registry for long-lived app services.
- Local-first storage in `~/.hive/` with SQLite, JSON, encrypted storage, and LanceDB.
- AI, agent orchestration, integrations, terminal execution, remote control, and cloud services are separate conceptual layers.

## Layers

**Application Bootstrap:**
- Purpose: Initialize logging, config, tray, GPUI root, service globals, background workers, and app window lifecycle.
- Contains: `hive/crates/hive_app/src/main.rs`, `hive/crates/hive_app/src/tray.rs`, `hive/crates/hive_app/src/cortex_runtime.rs`.
- Depends on: Most platform crates plus GPUI.
- Used by: Desktop binary `hive`.

**Workspace Shell and UI:**
- Purpose: Own the main window layout, active panel, chat service, action handlers, status bar, context rail, and panel routing.
- Contains: `hive/crates/hive_ui/src/workspace.rs` plus modules in `hive/crates/hive_ui/src/workspace/`.
- Depends on: `hive_ui_core`, `hive_ui_panels`, service crates.
- Used by: `hive_app`.

**Panel and Component Layer:**
- Purpose: Render individual product surfaces such as Chat, Files, Review, Agents, Workflows, Settings, Shield, Learning, and Terminal.
- Contains: `hive/crates/hive_ui_panels/src/panels/` and `hive/crates/hive_ui_panels/src/components/`.
- Depends on: GPUI component APIs and shared data types.
- Used by: `hive_ui`.

**Core Services:**
- Purpose: Configuration, persistence, notifications, security gateway, encrypted storage, scheduling, session state, channels, kanban, and update checks.
- Contains: `hive/crates/hive_core/src/`.
- Depends on: SQLite, crypto, tracing, platform helpers.
- Used by: Almost every higher-level crate.

**AI and Context Layer:**
- Purpose: AI provider abstraction, model routing, budget/cost tracking, RAG, semantic search, quick indexing, memory, embeddings, TTS, and context curation.
- Contains: `hive/crates/hive_ai/src/`.
- Depends on: `hive_core`, `hive_fs`, `hive_docs`, `hive_shield`, provider HTTP APIs, LanceDB.
- Used by: UI chat, agents, learning, remote, A2A, and cloud-adjacent workflows.

**Agent and Automation Layer:**
- Purpose: Queen/HiveMind/Coordinator orchestration, skills, workflows, MCP client/server, worktrees, tool use, approvals, activity, and ticket-to-build flows.
- Contains: `hive/crates/hive_agents/src/`.
- Depends on: AI, core, integrations, terminal, docs, learning, blockchain, filesystem.
- Used by: UI panels, remote control, A2A, and mission automation.

**External Surface Layer:**
- Purpose: Network, remote, A2A, cloud relay/admin, CLI, and admin TUI entry points.
- Contains: `hive/crates/hive_remote/`, `hive/crates/hive_a2a/`, `hive/crates/hive_cloud/`, `hive/crates/hive_cli/`, `hive/crates/hive_admin/`.
- Depends on: Axum, WebSocket, reqwest, internal service crates.
- Used by: Remote web UI, terminal clients, cloud relay, A2A-compatible agents, and admin workflows.

## Data Flow

**Desktop Startup:**

1. `main()` in `hive/crates/hive_app/src/main.rs` initializes logging and ensures `~/.hive/` directories.
2. GPUI application starts and registers actions, tray behavior, assets, theme, and root window.
3. `init_bootstrap_globals()` loads minimal config and notifications so the window can open quickly.
4. `open_main_window()` creates `HiveWorkspace` from `hive_ui`.
5. A delayed background task runs `init_services()` to create AI, database, learning, shield, memory, integrations, remote, network, assistant, and other globals.

**Chat Request:**

1. User submits through `ChatInputView` and `HiveWorkspace`.
2. `ChatService` builds conversation state and current model selection.
3. `enrich_request_with_memory()` in `hive/crates/hive_ui/src/workspace.rs` can inject LanceDB memory and knowledge hub context.
4. `hive_ai::AiService` prepares provider and request through routing and provider selection.
5. Provider streams chunks back to the UI; `ChatService` tracks content, cost, tokens, and tool approvals.
6. Stream completion updates learning and fleet metrics, then persists conversation/cost state through core services.

**Remote Control:**

1. `hive_remote::HiveDaemon` owns canonical remote session state.
2. WebSocket relay, REST API, QR pairing, and static web UI are served from `hive/crates/hive_remote/`.
3. Remote actions are translated into UI action requests or daemon state transitions.
4. Approval and safety signals are surfaced through activity and shield state.

**State Management:**
- Durable app state lives under `~/.hive/`.
- SQLite handles conversations, messages, logs, memory rows, costs, learning, assistant, and collective memory.
- JSON handles user config, session, kanban/workflows/themes, and legacy conversation backfill paths.
- LanceDB stores vector memory and indexed chunks.
- GPUI globals hold service handles during runtime.

## Key Abstractions

**GPUI Global Service Handles:**
- Purpose: App-wide dependency access without passing every service through panel constructors.
- Examples: `AppAiService`, `AppDatabase`, `AppShield`, `AppLearning`, `AppApprovalGate`.
- Pattern: Newtype globals registered during `init_services()`.

**Panel Data Objects:**
- Purpose: Keep UI panel render input explicit and testable.
- Examples: `QuickStartPanelData`, `ReviewData`, `AgentsPanelData`, `ShieldPanelData`.
- Pattern: Workspace owns data, panels render from immutable snapshots and emit actions.

**AI Provider Trait:**
- Purpose: Normalize streaming/chat behavior across cloud and local providers.
- Examples: provider modules under `hive/crates/hive_ai/src/providers/`.
- Pattern: Trait-based provider abstraction plus routing and fallback services.

**Security Gateway and Shield:**
- Purpose: Filter commands, paths, URLs, injections, secrets, PII, and outbound provider risk.
- Examples: `hive_core::SecurityGateway`, `hive_shield::HiveShield`.
- Pattern: Boundary validation before external effects.

**Skill and Workflow Definitions:**
- Purpose: User-extensible automation and model-portable skills.
- Examples: TOML skills in `~/.hive/skills/`, workflow definitions in `~/.hive/workflows/`.
- Pattern: File-backed definitions loaded by registries/executors.

## Entry Points

**Desktop App:**
- Location: `hive/crates/hive_app/src/main.rs`.
- Triggers: User launches `hive`.
- Responsibilities: Boot GPUI app, services, tray, window, and background workers.

**Terminal CLI:**
- Location: `hive/crates/hive_cli/src/main.rs`.
- Triggers: `hive` CLI subcommands.
- Responsibilities: Chat, sync, config, models, remote, and ticket/build style workflows.

**Cloud Service:**
- Location: `hive/crates/hive_cloud/src/main.rs`.
- Triggers: HTTP process start.
- Responsibilities: Build Axum router, expose relay/admin endpoints, bind `HIVE_CLOUD_BIND`.

**Admin TUI:**
- Location: `hive/crates/hive_admin/src/main.rs`.
- Triggers: `hive-admin`.
- Responsibilities: Cloud admin dashboard and API client views.

**A2A Service:**
- Location: `hive/crates/hive_a2a/src/`.
- Triggers: App startup when enabled through A2A config.
- Responsibilities: Agent card, auth, task handler, HTTP/SSE-compatible A2A protocol.

## Error Handling

**Strategy:** Use `anyhow` for application/bootstrap errors, `thiserror` for typed domain errors, and `tracing` for warnings/errors at service boundaries.

**Patterns:**
- Startup failures are logged and often reported through `AppNotification` instead of crashing the UI when non-critical.
- Optional integrations are skipped with warnings when credentials/config are absent.
- Async/background service failures are isolated where possible.
- Tests often assert specific error behavior for security and storage modules.

## Cross-Cutting Concerns

**Logging:**
- `hive_core::logging` initializes tracing.
- Most long-lived services use `tracing::{info, warn, error}`.

**Validation and Safety:**
- `hive_core::security` validates commands, URLs, paths, and injection patterns.
- `hive_shield` covers PII, secret scanning, vulnerabilities, and access control.
- Tool approvals are surfaced through `AppApprovalGate` and UI panels/context rail.

**Persistence:**
- `hive_core::persistence::Database` provides SQLite-backed app data.
- `hive_core::secure_storage::SecureStorage` handles encrypted secrets.

**Concurrency:**
- Tokio handles async tasks; some blocking startup pieces run in `std::thread::scope` or dedicated threads.
- GPUI state mutations remain on the app thread through `cx.spawn()` / app updates.

---

*Architecture analysis: 2026-06-18*
*Update when major crate boundaries or service ownership changes*
