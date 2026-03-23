# Plan: Complete All Unwired Features + Computer Use

**Date**: 2026-03-16
**Status**: Draft
**Goal**: Wire 100% of built-but-unwired code and add Anthropic Computer Use support

---

## Context

A comprehensive audit of the Hive codebase revealed ~35 fully-built features with zero production callers. Commit `dabfc01` wired the majority (document exports, RAG bootstrap, Shield pipeline, budget enforcement, MCP tools for Docker/GitLab/Bitbucket/Google Suite, UI components). This plan covers the **remaining gaps** plus the new **Computer Use** feature.

---

## Phase 1: Wire Remaining Agent Services (3 tasks)

These are built, tested, and initialized as globals — but never called from production orchestration paths.

### 1.1 Wire ApprovalGate into Coordinator task loop
- **File**: `hive/crates/hive_agents/src/coordinator.rs`
- **What**: Before executing high-risk tasks (file deletion, shell commands, deployments), check `ApprovalGate::check_with_channel()` and await user response
- **Integration point**: The existing `execute_plan()` loop, right after the budget check
- **UI side**: The Activity panel already has `ActivityApprove` / `ActivityDeny` action handlers — wire them to call `ApprovalGate::respond()`

### 1.2 Wire NotificationService into Queen/Coordinator events
- **File**: `hive/crates/hive_agents/src/queen.rs`, `coordinator.rs`
- **What**: Push notifications on: task failure, budget warning, approval request, team completion
- **Integration point**: After `TaskEvent::TaskFailed` / `TaskEvent::TaskCompleted` emissions
- **UI side**: `AppAgentNotifications` global already exists — NotificationTray component reads from it

### 1.3 Wire HeartbeatScheduler for periodic agent health checks
- **File**: `hive/crates/hive_app/src/main.rs` (already initialized as `AppHeartbeatScheduler`)
- **What**: Register default heartbeat tasks: stale agent cleanup (5min), cost summary rollup (1hr), discovery re-scan (2hr)
- **Integration point**: After `HeartbeatScheduler` global is set, call `.add()` for each default task

---

## Phase 2: Wire Remaining AI Features (5 tasks)

### 2.1 Wire SemanticSearch into chat context pipeline
- **File**: `hive/crates/hive_ui/src/workspace.rs` (send_message flow)
- **What**: Before sending a message, query `AppSemanticSearch` for relevant files and prepend as context
- **Depends on**: RAG bootstrap (already wired in `dabfc01`)

### 2.2 Wire CapabilityRouter into AiService
- **File**: `hive/crates/hive_ai/src/service.rs`
- **What**: Replace basic `route()` call with `route_with_capabilities()` when task type is known
- **Integration point**: `resolve_provider()` method

### 2.3 Wire TTS Service initialization
- **File**: `hive/crates/hive_app/src/main.rs`
- **What**: Create `TtsService` from config (provider selection: OpenAI, ElevenLabs, etc.) and register as `AppTts` global
- **Note**: `ChatReadAloud` action handler already exists (wired in `dabfc01`)

### 2.4 Wire Speculative Decoding as opt-in mode
- **File**: `hive/crates/hive_ai/src/service.rs`
- **What**: When config flag `speculative_decoding: true`, use `speculative_stream()` instead of `stream_chat()`
- **Low priority**: Performance optimization, not user-facing

### 2.5 Wire LocalSearch (SearXNG) as fallback web search
- **File**: `hive/crates/hive_agents/src/integration_tools.rs`
- **What**: Add `web_search` MCP tool that tries SearXNG first (privacy), falls back to Brave/Google
- **Depends on**: Docker availability for SearXNG container

---

## Phase 3: Computer Use (NEW FEATURE — 6 tasks)

Anthropic's Computer Use API lets Claude control a desktop by taking screenshots, clicking, typing, and scrolling. Hive already has:
- `UiDriver` (enigo) for mouse/keyboard control (`hive_agents/src/ui_automation.rs`)
- `BrowserAutomation` (Playwright) for web interaction (`hive_integrations/src/browser.rs`)
- Anthropic provider sends `computer-use` beta header (`hive_ai/src/providers/anthropic.rs:451`)

What's missing: the orchestration loop that sends screenshots to Claude and executes returned tool calls.

### 3.1 Add screen capture utility
- **New file**: `hive/crates/hive_agents/src/computer_use/capture.rs`
- **What**: Platform-native screenshot capture returning base64 PNG
- **Crate**: Use `xcap` (cross-platform screen capture) or `win-screenshot` on Windows
- **API**: `pub fn capture_screen() -> Result<String>` (base64), `pub fn capture_region(x, y, w, h) -> Result<String>`

### 3.2 Add Computer Use tool definitions
- **New file**: `hive/crates/hive_agents/src/computer_use/tools.rs`
- **What**: Define the 3 Anthropic computer use tools:
  - `computer` — screenshot, click, type, scroll, key, cursor_position
  - `text_editor` — view, create, str_replace, insert
  - `bash` — execute shell commands
- **Format**: Match Anthropic's `computer-use-2024-10-22` tool schema exactly

### 3.3 Add Computer Use execution loop
- **New file**: `hive/crates/hive_agents/src/computer_use/executor.rs`
- **What**: The core loop:
  1. Take screenshot
  2. Send to Claude with computer use tools
  3. Parse tool_use response
  4. Execute tool action (click/type/scroll via UiDriver, screenshot via capture)
  5. Send tool_result with new screenshot back to Claude
  6. Repeat until Claude responds with text (task complete)
- **Safety**: Every action goes through `SecurityGateway::check_command()` for bash, and requires approval for destructive clicks (outside Hive window)

### 3.4 Wire Computer Use into MCP server
- **File**: `hive/crates/hive_agents/src/mcp_server.rs`
- **What**: Register `computer_use_start`, `computer_use_stop`, `computer_use_status` tools
- **Integration**: Agents can invoke computer use as a tool during task execution

### 3.5 Add Computer Use chat command
- **File**: `hive/crates/hive_ui/src/workspace.rs`
- **What**: `/computer <goal>` command intercept (like `/swarm`)
- **Flow**: Creates a ComputerUseSession, starts the execution loop, streams progress to chat
- **UI**: Show screenshot thumbnails in chat as the agent works

### 3.6 Add Computer Use settings
- **File**: `hive/crates/hive_ui_panels/src/panels/settings.rs`
- **What**: Toggle to enable/disable computer use, set allowed screen regions, configure approval level (auto/ask/block)
- **Config**: Add `computer_use` section to `AppConfig`

---

## Phase 4: OAuth Token Setup UI (2 tasks)

### 4.1 Add OAuth flow for Google services
- **File**: `hive/crates/hive_ui/src/workspace.rs` (AccountConnectPlatform handler)
- **What**: When user clicks "Connect Google", launch OAuth consent flow → callback → store token
- **Services unlocked**: Gmail, Google Calendar, Drive, Sheets, Docs, Tasks, Contacts
- **Token storage**: Encrypted via `hive_core::security` keychain

### 4.2 Add OAuth flow for Microsoft services
- **File**: Same handler, different provider
- **What**: MSAL OAuth flow for Microsoft 365
- **Services unlocked**: Outlook Email, Outlook Calendar

---

## Phase 5: Clean Up Remaining Dead Code (4 tasks)

### 5.1 Wire or remove CliService
- **File**: `hive/crates/hive_terminal/src/cli.rs`
- **Decision**: Either wire `CliService` commands into the chat input (e.g., `/doctor`, `/config`) or delete it
- **Recommendation**: Wire — the `doctor` command is useful for diagnostics

### 5.2 Wire or remove DockerSandbox/AgentSandbox
- **File**: `hive/crates/hive_terminal/src/docker.rs`, `sandbox.rs`
- **Decision**: Wire into Coordinator for sandboxed agent execution, or delete
- **Recommendation**: Wire into Computer Use (Phase 3) — run computer use actions inside a container for safety

### 5.3 Wire Cloud Provider tools (AWS/Azure/GCP)
- **File**: `hive/crates/hive_agents/src/integration_tools.rs`
- **What**: Add real MCP tool handlers for: S3 list/get, EC2 status, Lambda invoke, Azure Blob, GCP Storage
- **Current state**: Clients initialized, zero tool handlers

### 5.4 Remove confirmed dead code
- **Components**: `SplitPane` (no users), `Toast` (redundant with NotificationService), `ThinkingIndicator` (inline in chat)
- **Integrations**: `ClawdTalkClient` (phone bridge — no roadmap)
- **Only delete** items with zero roadmap intent

---

## Phase 6: Verification & Ship (2 tasks)

### 6.1 Full workspace test suite
- `cargo test --workspace --exclude hive_app`
- Fix any regressions from wiring changes
- Target: 0 failures

### 6.2 Version bump, README update, push + CI
- Bump to v0.3.31
- Update README changelog with new wired features
- Push to main, verify all 3 CI platforms pass
- Update hivecode-site if needed

---

## Priority Order

| Priority | Phase | Effort | Impact |
|----------|-------|--------|--------|
| **P0** | Phase 3 (Computer Use) | High | Major new feature — competitive differentiator |
| **P1** | Phase 1 (Agent Services) | Medium | Completes the orchestration platform from v0.3.30 |
| **P1** | Phase 2.1-2.3 (SemanticSearch, CapabilityRouter, TTS) | Medium | Activates intelligence layer |
| **P2** | Phase 4 (OAuth) | Medium | Unlocks Google + Microsoft integrations |
| **P2** | Phase 5.3 (Cloud tools) | Medium | Enables cloud management |
| **P3** | Phase 2.4-2.5 (Speculative, LocalSearch) | Low | Optimizations |
| **P3** | Phase 5.1-5.2 (CLI, Sandbox) | Low | Cleanup |
| **P3** | Phase 5.4 (Dead code removal) | Low | Hygiene |

---

## Estimated Scope

- **22 tasks** across 6 phases
- **~15 files modified**, **~4 new files** (computer use module)
- **New dependency**: `xcap` or equivalent for screen capture
- **No breaking changes** — all additions are additive
