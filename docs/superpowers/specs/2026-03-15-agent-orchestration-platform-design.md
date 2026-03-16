# Agent Orchestration Platform Design

**Date:** 2026-03-15
**Status:** Draft
**Inspired by:** [Paperclip](https://github.com/paperclipai/paperclip) — open-source orchestration for zero-human companies

## Overview

Five interconnected features that transform Hive's agent system from a chat-driven assistant into a governed, autonomous orchestration platform. All five share a single backbone: a structured **event bus** that every agent action flows through.

```
Agent Action → Event Bus → [Activity Log, Budget Enforcer, Approval Gate, Notification Tray]
```

### Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Autonomy model | Hybrid with escalation | Chat-first by default; user can spawn background heartbeat tasks for long-running work |
| Approval triggers | Cost + SecurityGateway + custom rules | Maximum flexibility with sensible defaults |
| UI surface | Notification tray + Activity panel | Tray for urgent items; one new panel for audit log; approvals as filtered view within Activity |
| Architecture | Event bus | Single source of truth, decoupled listeners, plugin-friendly |

### Delivery Phases

| Phase | Features | Depends On |
|-------|----------|-----------|
| 1 | Event bus + Activity log + Activity panel | Nothing |
| 2 | Budget enforcement | Phase 1 |
| 3 | Approval gates + notification tray | Phase 1 |
| 4 | Heartbeat scheduler | Phases 1-3 |
| 5 | Progressive disclosure (UI) | Phase 1 (parallel with 2-4) |

---

## Phase 1: Event Bus + Activity Log + Activity Panel

### 1.1 ActivityEvent Schema

All agent behavior is expressed as events. Lives in `hive_agents`.

```rust
use chrono::{DateTime, Utc};

/// Every observable agent action in the system.
pub enum ActivityEvent {
    // ── Agent lifecycle ──
    AgentStarted {
        agent_id: String,
        role: String,
        task_id: Option<String>,
    },
    AgentCompleted {
        agent_id: String,
        duration_ms: u64,
        cost: f64,
    },
    AgentFailed {
        agent_id: String,
        error: String,
    },
    AgentPaused {
        agent_id: String,
        reason: PauseReason,
    },

    // ── Task lifecycle ──
    TaskClaimed {
        task_id: String,
        agent_id: String,
    },
    TaskProgress {
        task_id: String,
        progress: f64,
        message: String,
    },
    TaskCompleted {
        task_id: String,
        agent_id: String,
        cost: f64,
    },
    TaskFailed {
        task_id: String,
        error: String,
    },

    // ── Tool/action execution ──
    ToolCalled {
        agent_id: String,
        tool_name: String,
        args_summary: String,
    },
    FileModified {
        agent_id: String,
        path: String,
        op: FileOp,
    },
    ShellExecuted {
        agent_id: String,
        command: String,
        exit_code: i32,
    },

    // ── Cost events ──
    CostIncurred {
        agent_id: String,
        model: String,
        input_tokens: u32,
        output_tokens: u32,
        cost_usd: f64,
    },
    BudgetWarning {
        agent_id: String,
        usage_pct: f64,
        limit_usd: f64,
    },
    BudgetExhausted {
        agent_id: String,
        limit_usd: f64,
    },

    // ── Approval events ──
    ApprovalRequested {
        request_id: String,
        agent_id: String,
        operation: String,
        context: String,
        rule: String,
    },
    ApprovalGranted {
        request_id: String,
    },
    ApprovalDenied {
        request_id: String,
        reason: Option<String>,
    },

    // ── Heartbeat events ──
    HeartbeatFired {
        agent_id: String,
        task_id: String,
    },
    HeartbeatScheduled {
        agent_id: String,
        interval_secs: u64,
    },
    HeartbeatCancelled {
        agent_id: String,
    },
}

pub enum PauseReason {
    BudgetExhausted,
    UserRequested,
    ApprovalTimeout,
    Error(String),
}

pub enum FileOp {
    Created,
    Modified,
    Deleted,
    Renamed { from: String },
}
```

### 1.2 ActivityService

Central hub. Created once at app startup, passed as `Arc<ActivityService>` to all agent infrastructure.

```rust
pub struct ActivityService {
    tx: broadcast::Sender<ActivityEvent>,
    log: Arc<ActivityLog>,
    budget: Arc<BudgetEnforcer>,
    approvals: Arc<ApprovalGate>,
    notifications: Arc<NotificationService>,
}

impl ActivityService {
    /// Emit an event to all listeners.
    pub fn emit(&self, event: ActivityEvent) {
        let _ = self.tx.send(event);
    }

    /// Subscribe to the event stream (for custom listeners / future plugins).
    pub fn subscribe(&self) -> broadcast::Receiver<ActivityEvent> {
        self.tx.subscribe()
    }
}
```

Each listener spawns a tokio task on construction that loops on `rx.recv()`. This follows the same pattern the Coordinator already uses for `TaskEvent` broadcasting.

**Location:** New `activity` module in `hive_agents/src/activity/` with:
- `mod.rs` — `ActivityEvent`, `ActivityService`
- `log.rs` — `ActivityLog` (SQLite persistence)
- `budget.rs` — `BudgetEnforcer` (Phase 2)
- `approval.rs` — `ApprovalGate` (Phase 3)
- `notification.rs` — `NotificationService` (Phase 3)

### 1.3 ActivityLog (SQLite Persistence)

Follows the existing `CollectiveMemory` pattern — SQLite file in `~/.hive/`.

```sql
CREATE TABLE activity_events (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp     TEXT    NOT NULL,
    event_type    TEXT    NOT NULL,    -- "agent_started", "cost_incurred", etc.
    agent_id      TEXT,
    task_id       TEXT,
    category      TEXT    NOT NULL,    -- "agent", "task", "tool", "cost", "approval", "heartbeat"
    summary       TEXT    NOT NULL,    -- Human-readable one-liner
    detail_json   TEXT,                -- Full event data serialized as JSON
    cost_usd      REAL    DEFAULT 0.0  -- Denormalized for fast cost queries
);

CREATE INDEX idx_events_category  ON activity_events(category);
CREATE INDEX idx_events_agent     ON activity_events(agent_id);
CREATE INDEX idx_events_timestamp ON activity_events(timestamp);
CREATE INDEX idx_events_type      ON activity_events(event_type);
```

Query API:

```rust
pub struct ActivityLog {
    conn: Mutex<Connection>,
}

impl ActivityLog {
    pub fn query(&self, filter: &ActivityFilter) -> Result<Vec<ActivityEntry>, String>;
    pub fn cost_summary(&self, agent_id: Option<&str>, since: DateTime<Utc>) -> Result<CostSummary, String>;
    pub fn pending_approvals(&self) -> Result<Vec<ApprovalRequest>, String>;
    pub fn total_events(&self) -> Result<usize, String>;
}

pub struct ActivityFilter {
    pub categories: Option<Vec<String>>,
    pub agent_id: Option<String>,
    pub since: Option<DateTime<Utc>>,
    pub search: Option<String>,           // Full-text on summary
    pub limit: usize,                     // Default: 100
    pub offset: usize,
}

pub struct ActivityEntry {
    pub id: i64,
    pub timestamp: DateTime<Utc>,
    pub event_type: String,
    pub agent_id: Option<String>,
    pub task_id: Option<String>,
    pub category: String,
    pub summary: String,
    pub detail_json: Option<String>,
    pub cost_usd: f64,
}

pub struct CostSummary {
    pub total_usd: f64,
    pub by_agent: Vec<(String, f64)>,
    pub by_model: Vec<(String, f64)>,
    pub request_count: usize,
}
```

### 1.4 Activity Panel

New panel in `hive_ui_panels`. Sidebar entry between Monitor and Logs.

**Data struct:**

```rust
pub struct ActivityData {
    pub entries: Vec<ActivityEntry>,
    pub filter: ActivityFilter,
    pub pending_approvals: Vec<ApprovalRequest>,  // Pinned at top
    pub cost_summary: CostSummary,                // Sidebar stats
    pub disclosure: HashMap<i64, bool>,            // Expanded event IDs
}
```

**Layout:**
- **Top bar:** Category filter toggles (pills) + text search + time range selector
- **Pinned section:** Pending approval requests with Approve/Deny buttons (only visible when approvals exist)
- **Event stream:** Reverse-chronological list. Each row shows: timestamp, category icon, summary, cost (if any). Click to expand → shows `detail_json` as formatted key-value pairs.
- **Right sidebar stats:** Total cost today, event count, active agents

**Actions:**
- `ActivityRefresh` — Re-query from SQLite
- `ActivitySetFilter` — Update filter and re-query
- `ActivityApprove { request_id }` — Approve pending request
- `ActivityDeny { request_id }` — Deny pending request
- `ActivityExpandEvent { event_id }` — Toggle detail disclosure
- `ActivityExportCsv` — Export filtered events

---

## Phase 2: Budget Enforcement

### 2.1 BudgetEnforcer

Subscribes to `CostIncurred` events. Also provides a synchronous pre-flight check for AI calls.

```rust
pub struct BudgetEnforcer {
    config: BudgetConfig,
    activity_tx: broadcast::Sender<ActivityEvent>,
    log: Arc<ActivityLog>,  // For querying cumulative spend
}

pub struct BudgetConfig {
    pub global_daily_limit_usd: Option<f64>,
    pub global_monthly_limit_usd: Option<f64>,
    pub per_agent_limit_usd: Option<f64>,     // Per-agent per-month
    pub per_task_limit_usd: Option<f64>,      // Single task ceiling
    pub warning_threshold_pct: f64,           // Default: 0.8
    pub on_exhaust: ExhaustAction,
}

pub enum ExhaustAction {
    Pause,             // Pause the agent
    ApprovalRequired,  // Route next op through approval gate
    WarnOnly,          // Emit warning, don't block
}

pub enum BudgetDecision {
    Proceed,
    Warning { usage_pct: f64, message: String },
    Blocked { reason: String },
    NeedsApproval { request_id: String },
}
```

### 2.2 Integration Point

Pre-flight check added to `AiService::chat()` and `AiService::stream_chat()`:

```rust
// In AiService, before calling provider:
let decision = self.budget_enforcer.check(agent_id, estimated_cost);
match decision {
    BudgetDecision::Proceed => { /* continue */ },
    BudgetDecision::Warning { message, .. } => {
        self.activity.emit(ActivityEvent::BudgetWarning { .. });
        // continue, but user sees notification
    },
    BudgetDecision::Blocked { reason } => {
        self.activity.emit(ActivityEvent::BudgetExhausted { .. });
        return Err(ProviderError::BudgetExceeded(reason));
    },
    BudgetDecision::NeedsApproval { request_id } => {
        // Route through approval gate (Phase 3)
    },
}
```

### 2.3 Costs Panel Enhancement

Add a budget gauge above the existing model breakdown table:

- Horizontal bar showing current spend vs. daily limit
- Amber segment at warning threshold, red at limit
- Text: "$18.40 / $25.00 daily budget (74%)"
- If no budget configured, show "No budget set" with a link to Settings

### 2.4 Configuration

In `~/.hive/config.toml`:

```toml
[budget]
daily_limit_usd = 25.0
monthly_limit_usd = 500.0
per_agent_limit_usd = 50.0
per_task_limit_usd = 5.0
warning_threshold = 0.8
on_exhaust = "pause"   # "pause" | "approval" | "warn"
```

---

## Phase 3: Approval Gates + Notification Tray

### 3.1 ApprovalGate

```rust
pub struct ApprovalGate {
    pending: Mutex<HashMap<String, ApprovalRequest>>,
    rules: Vec<ApprovalRule>,
    activity_tx: broadcast::Sender<ActivityEvent>,
    response_channels: Mutex<HashMap<String, oneshot::Sender<ApprovalDecision>>>,
}

pub struct ApprovalRequest {
    pub id: String,
    pub agent_id: String,
    pub timestamp: DateTime<Utc>,
    pub operation: OperationType,
    pub context: String,
    pub matched_rule: String,
    pub estimated_cost: Option<f64>,
    pub timeout_secs: Option<u64>,  // Default: 300 for background, None for foreground
}

pub enum OperationType {
    ShellCommand(String),
    FileDelete(String),
    FileModify { path: String, scope: String },
    GitPush { remote: String, branch: String },
    AiCall { model: String, estimated_cost: f64 },
    Custom(String),
}

pub enum ApprovalDecision {
    Approved,
    Denied { reason: Option<String> },
    Timeout,
}
```

### 3.2 Rule Engine

```rust
pub struct ApprovalRule {
    pub name: String,
    pub enabled: bool,
    pub trigger: RuleTrigger,
    pub priority: u8,  // Higher = checked first. First match wins.
}

pub enum RuleTrigger {
    SecurityGatewayBlock,                  // Wraps existing SecurityGateway
    CostExceeds { usd: f64 },
    PathMatches { glob: String },
    FilesExceed { count: usize },
    CommandMatches { pattern: String },
    Always,                                // Paranoia mode
}
```

**Default rules:**

| Priority | Name | Trigger |
|----------|------|---------|
| 100 | `security-gateway` | `SecurityGatewayBlock` |
| 90 | `expensive-ops` | `CostExceeds { usd: 5.0 }` |
| 80 | `git-push` | `CommandMatches { "git push*" }` |
| 70 | `bulk-modify` | `FilesExceed { count: 10 }` |

### 3.3 End-to-End Flow

1. Agent calls `approval_gate.check(agent_id, operation)`
2. Rules evaluated in priority order. No match → return `Proceed`
3. Match → create `ApprovalRequest`, store in `pending` map
4. Create `oneshot::channel()`, store sender in `response_channels`
5. Emit `ApprovalRequested` event on bus
6. Return `oneshot::Receiver` to caller — agent **awaits**
7. Notification tray receives event → shows badge + toast
8. User clicks Approve/Deny → `approval_gate.respond(request_id, decision)`
9. `respond()` removes from `pending`, sends on `oneshot::Sender`
10. Agent unblocks, proceeds or aborts
11. `ApprovalGranted`/`ApprovalDenied` event emitted

### 3.4 SecurityGateway Integration

The existing `SecurityGateway` in `hive_core` hard-blocks dangerous commands. We introduce a graduated response:

- **Hard deny** (unchanged): Truly catastrophic operations (`rm -rf /`, etc.) — these remain hard blocks, never approvable
- **Soft deny → approval route** (new): Risky but reasonable operations — instead of `Err("blocked")`, returns `Err(NeedsApproval(op))`. The agent layer catches this and routes through `ApprovalGate`

This requires a small change to `SecurityGateway`:

```rust
pub enum SecurityDecision {
    Allow,
    NeedsApproval(OperationType),  // New: route to approval gate
    Deny(String),                  // Unchanged: hard block
}
```

### 3.5 Notification Tray

Lives in the statusbar (bottom bar).

```rust
pub struct NotificationService {
    items: Mutex<Vec<Notification>>,
    unread_count: AtomicUsize,
}

pub struct Notification {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub kind: NotificationKind,
    pub summary: String,
    pub read: bool,
    pub action: Option<NotificationAction>,
}

pub enum NotificationKind {
    ApprovalRequest,   // Amber badge, requires action
    BudgetWarning,     // Amber badge
    BudgetExhausted,   // Red badge
    AgentCompleted,    // Green, auto-dismiss after 10s
    AgentFailed,       // Red badge
    HeartbeatReport,   // Gray, low priority
}

pub enum NotificationAction {
    Approve(String),
    Deny(String),
    SwitchToPanel(Panel),
}
```

**UI:** Bell icon in statusbar with unread badge count. Click opens a dropdown flyout (not a full panel). Each notification shows: icon, summary, timestamp, and optional action buttons. Approval notifications have inline Approve/Deny buttons. Clicking notification body switches to Activity panel filtered to that event.

### 3.6 Configuration

In `~/.hive/config.toml`:

```toml
[[approval.rules]]
name = "expensive-operations"
trigger = { cost_exceeds = { usd = 5.0 } }
enabled = true
priority = 90

[[approval.rules]]
name = "protect-core"
trigger = { path_matches = { glob = "src/core/**" } }
enabled = true
priority = 75

[approval]
default_timeout_secs = 300
```

---

## Phase 4: Heartbeat Scheduler

### 4.1 HeartbeatScheduler

Transforms the existing `HeartbeatService` from a liveness monitor into an execution driver.

```rust
pub struct HeartbeatScheduler {
    tasks: Mutex<HashMap<String, HeartbeatTask>>,
    activity_tx: broadcast::Sender<ActivityEvent>,
    budget: Arc<BudgetEnforcer>,
    approvals: Arc<ApprovalGate>,
}

pub struct HeartbeatTask {
    pub id: String,
    pub agent_id: String,
    pub spec: String,                     // What to work on
    pub interval_secs: u64,
    pub mode: HeartbeatMode,
    pub max_iterations: Option<u32>,      // None = until cancelled
    pub paused: bool,
    pub iteration_count: u32,
    pub last_fired: Option<DateTime<Utc>>,
    pub total_cost: f64,
}

pub enum HeartbeatMode {
    FixedInterval,     // Fire every N seconds regardless
    BackoffOnIdle,     // Increase interval when no work found, reset on new work
    OneShot,           // Fire once after delay (deferred execution)
}
```

### 4.2 Execution Loop

Each `HeartbeatTask` spawns a tokio task:

```
loop {
    sleep(interval)

    if paused → continue
    if max_iterations reached → break
    if budget check fails → emit AgentPaused, break

    emit HeartbeatFired

    // Execute one iteration via Coordinator
    let result = coordinator.execute_plan(spec, context).await;

    match result {
        Ok(output) => {
            emit TaskCompleted
            if mode == BackoffOnIdle && no_work_found → double interval
            if mode == BackoffOnIdle && work_found → reset interval
        },
        Err(NeedsApproval(op)) => {
            // Wait for approval, then retry or skip
        },
        Err(e) => {
            emit AgentFailed
            // Backoff or stop depending on error type
        },
    }

    iteration_count += 1
}
emit HeartbeatCancelled
```

### 4.3 Triggering Background Work

From chat, user can spawn a heartbeat task:

- Explicit: `/background refactor all error handling in hive_ai` — creates a `HeartbeatTask` with the spec
- Inferred: Chat service detects multi-file, long-running intent and suggests: "This looks like it'll take a while. Run it in the background?" User confirms → heartbeat created

The existing `HeartbeatService` (liveness monitoring) becomes a subcomponent within `HeartbeatScheduler`. It still tracks timestamps for dead agent detection, but the scheduler now drives the wake cycle.

### 4.4 Integration with Existing Orchestration

HeartbeatScheduler doesn't replace HiveMind/Coordinator/Queen — it wraps them:

- Simple background tasks → `Coordinator::execute_plan()`
- Complex multi-team goals → `Queen::execute()`
- The orchestration mode is chosen based on task complexity (same as foreground chat)

Each heartbeat iteration is a full orchestration cycle. Cross-iteration state is maintained via:
- `CollectiveMemory` (learnings persist across iterations)
- `ActivityLog` (full history of what was done)
- Task plan state (which subtasks are done, which remain)

---

## Phase 5: Progressive Disclosure

### 5.1 Disclosure Levels

No new infrastructure — a rendering change in existing panels.

```rust
pub enum DisclosureLevel {
    Summary,   // One-line: "Refactored error handling in 6 files ($0.34)"
    Steps,     // Task checklist: each subtask with status, duration, cost
    Raw,       // Full tool calls, prompts, raw model responses
}
```

### 5.2 Chat Panel Changes

`DisplayMessage` gains a `disclosure: DisclosureLevel` field (default: `Summary`).

- **Summary:** Role icon + one-line summary + cost badge + chevron to expand
- **Steps:** Expandable section showing the `TaskEvent` stream for this message's orchestration run. Uses the existing `TaskTreeView` component from `hive_ui_panels/src/components/`.
- **Raw:** Code blocks showing full prompts, responses, tool call JSON. Uses existing `render_code_block()`.

Toggle is a simple click on the chevron. State is per-message, not global.

### 5.3 Agents Panel Changes

`RunDisplay` already has `tasks: Vec<TaskDisplay>`. Add `disclosure: DisclosureLevel` per run:

- **Summary:** Progress bar + "4/7 tasks done ($1.23)"
- **Steps:** Full task tree (already rendered, just collapsed by default)
- **Raw:** Per-task tool calls and raw output

---

## Cross-Cutting Concerns

### Configuration

All new config lives under `~/.hive/config.toml`:

```toml
[budget]
daily_limit_usd = 25.0
monthly_limit_usd = 500.0
per_agent_limit_usd = 50.0
per_task_limit_usd = 5.0
warning_threshold = 0.8
on_exhaust = "pause"

[approval]
default_timeout_secs = 300

[[approval.rules]]
name = "security-gateway"
trigger = "security_gateway_block"
priority = 100

[[approval.rules]]
name = "expensive-operations"
trigger = { cost_exceeds = { usd = 5.0 } }
priority = 90

[[approval.rules]]
name = "git-push"
trigger = { command_matches = { pattern = "git push*" } }
priority = 80

[[approval.rules]]
name = "bulk-modify"
trigger = { files_exceed = { count = 10 } }
priority = 70

[heartbeat]
default_interval_secs = 60
backoff_multiplier = 2.0
max_interval_secs = 600
```

### Data Storage

- `~/.hive/activity.db` — SQLite database for activity log
- Separate from `CollectiveMemory` DB (different purpose, different query patterns)
- Retention: configurable, default 30 days. Background cleanup task.

### Thread Safety

- `ActivityService` is `Send + Sync` (all fields behind `Arc`/`Mutex`/atomic)
- Event bus uses `tokio::sync::broadcast` (multi-producer, multi-consumer)
- Approval gate uses `tokio::sync::oneshot` for per-request response channels
- Budget enforcer queries are read-only SQLite (concurrent-safe with WAL mode)

### Error Handling

- Event bus is fire-and-forget: `emit()` never fails (dropped events from slow receivers are acceptable for non-critical listeners)
- Activity log writes are best-effort: if SQLite write fails, log the error but don't crash the agent
- Budget checks are synchronous and blocking: if the check fails, the AI call is prevented
- Approval gate timeouts are configurable: background tasks auto-deny, foreground tasks wait indefinitely

### Testing Strategy

- Unit tests for each component in isolation (mock event bus, in-memory SQLite)
- Integration test: emit event → verify it appears in ActivityLog query results
- Budget enforcement: test warning at 80%, block at 100%, approval routing
- Approval gate: test rule matching, oneshot channel flow, timeout behavior
- Heartbeat: test iteration counting, backoff logic, pause/resume
- UI: test panel data refresh on event, notification badge count

---

## Files to Create/Modify

### New Files

| File | Purpose |
|------|---------|
| `hive_agents/src/activity/mod.rs` | `ActivityEvent`, `ActivityService`, re-exports |
| `hive_agents/src/activity/log.rs` | `ActivityLog` — SQLite persistence |
| `hive_agents/src/activity/budget.rs` | `BudgetEnforcer` — cost enforcement |
| `hive_agents/src/activity/approval.rs` | `ApprovalGate` — rule engine + approval routing |
| `hive_agents/src/activity/notification.rs` | `NotificationService` — UI notification push |
| `hive_agents/src/activity/rules.rs` | `ApprovalRule`, `RuleTrigger` — rule definitions |
| `hive_agents/src/heartbeat_scheduler.rs` | `HeartbeatScheduler` — execution-driving heartbeat |
| `hive_ui_panels/src/panels/activity.rs` | Activity panel UI |
| `hive_ui_panels/src/components/notification_tray.rs` | Notification dropdown in statusbar |
| `hive_ui_panels/src/components/budget_gauge.rs` | Budget bar for Costs panel |

### Modified Files

| File | Change |
|------|--------|
| `hive_agents/src/lib.rs` | Add `pub mod activity;` and `pub mod heartbeat_scheduler;` |
| `hive_agents/src/coordinator.rs` | Emit `ActivityEvent`s during task execution |
| `hive_agents/src/queen.rs` | Emit `ActivityEvent`s during team execution |
| `hive_agents/src/hivemind.rs` | Emit `ActivityEvent`s during role execution |
| `hive_ai/src/service.rs` | Add budget pre-flight check before AI calls |
| `hive_core/src/security.rs` | Add `SecurityDecision::NeedsApproval` variant |
| `hive_ui_core/src/sidebar.rs` | Add `Panel::Activity` variant |
| `hive_ui_core/src/actions.rs` | Add Activity/Approval/Notification actions |
| `hive_ui/src/workspace.rs` | Register Activity panel, wire notification tray |
| `hive_ui/src/statusbar.rs` | Add notification bell icon |
| `hive_ui_panels/src/panels/mod.rs` | Add `pub mod activity;` |
| `hive_ui_panels/src/panels/costs.rs` | Add budget gauge component |
| `hive_ui_panels/src/panels/chat.rs` | Add `DisclosureLevel` to `DisplayMessage` |
| `hive_ui_panels/src/panels/agents.rs` | Add `DisclosureLevel` to `RunDisplay` |
| `hive_ui_panels/src/components/mod.rs` | Add notification_tray, budget_gauge |
