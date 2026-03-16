# Agent Orchestration Platform Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add event bus, activity log, budget enforcement, approval gates, heartbeat scheduler, notification tray, and progressive disclosure to Hive's agent system.

**Architecture:** Event bus backbone (tokio broadcast channel) with listeners for persistence (SQLite), budget enforcement, approval routing, and UI notifications. Each phase builds on the bus, delivered incrementally with tests at every step.

**Tech Stack:** Rust, tokio (broadcast/oneshot/mpsc), rusqlite (SQLite), GPUI (UI panels), serde/serde_json, chrono, uuid, glob (pattern matching for approval rules)

---

## File Structure

### New Files

| File | Responsibility |
|------|---------------|
| `hive_agents/src/activity/mod.rs` | `ActivityEvent` enum, `ActivityService` hub, re-exports |
| `hive_agents/src/activity/log.rs` | `ActivityLog` — SQLite persistence listener |
| `hive_agents/src/activity/budget.rs` | `BudgetEnforcer` — cost limit checking + enforcement |
| `hive_agents/src/activity/approval.rs` | `ApprovalGate` — rule engine + oneshot approval channels |
| `hive_agents/src/activity/rules.rs` | `ApprovalRule`, `RuleTrigger` — rule definitions + matching |
| `hive_agents/src/activity/notification.rs` | `NotificationService` — UI notification push |
| `hive_agents/src/activity/types.rs` | Shared types: `PauseReason`, `FileOp`, `OperationType`, `BudgetConfig`, etc. |
| `hive_agents/src/heartbeat_scheduler.rs` | `HeartbeatScheduler` — execution-driving heartbeat loop |
| `hive_agents/tests/activity_log_tests.rs` | Activity log integration tests |
| `hive_agents/tests/budget_tests.rs` | Budget enforcement tests |
| `hive_agents/tests/approval_tests.rs` | Approval gate + rule engine tests |
| `hive_agents/tests/heartbeat_scheduler_tests.rs` | Heartbeat scheduler tests |
| `hive_ui_panels/src/panels/activity.rs` | Activity panel UI rendering |
| `hive_ui_panels/src/components/notification_tray.rs` | Notification dropdown for statusbar |
| `hive_ui_panels/src/components/budget_gauge.rs` | Budget usage bar for Costs panel |
| `hive_ui_panels/tests/activity_panel_tests.rs` | Activity panel data tests |

### Modified Files

| File | Change |
|------|--------|
| `hive_agents/src/lib.rs` | Add `pub mod activity;` + `pub mod heartbeat_scheduler;` + re-exports |
| `hive_agents/Cargo.toml` | Add `glob = "0.3"` dependency |
| `hive_core/src/security.rs` | Add `SecurityDecision` enum with `NeedsApproval` variant |
| `hive_ui_core/src/sidebar.rs` | Add `Panel::Activity` variant |
| `hive_ui_core/src/actions.rs` | Add Activity/Approval/Notification actions |
| `hive_ui_panels/src/panels/mod.rs` | Add `pub mod activity;` |
| `hive_ui_panels/src/panels/costs.rs` | Add budget gauge rendering |
| `hive_ui_panels/src/panels/chat.rs` | Add `DisclosureLevel` to `DisplayMessage` |
| `hive_ui_panels/src/panels/agents.rs` | Add `DisclosureLevel` to `RunDisplay` |
| `hive_ui_panels/src/components/mod.rs` | Add `pub mod notification_tray;` + `pub mod budget_gauge;` |
| `hive_ui/src/workspace.rs` | Register Activity panel + notification tray + wire event handlers |

---

## Chunk 1: Event Bus Core + Activity Types

### Task 1: ActivityEvent enum and shared types

**Files:**
- Create: `hive/crates/hive_agents/src/activity/types.rs`
- Create: `hive/crates/hive_agents/src/activity/mod.rs`

- [ ] **Step 1: Write the failing test for ActivityEvent serialization**

Create `hive/crates/hive_agents/tests/activity_log_tests.rs`:

```rust
use hive_agents::activity::{ActivityEvent, PauseReason, FileOp};

#[test]
fn activity_event_serializes_to_json() {
    let event = ActivityEvent::AgentStarted {
        agent_id: "agent-1".into(),
        role: "Coder".into(),
        task_id: Some("task-42".into()),
    };
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("AgentStarted"));
    assert!(json.contains("agent-1"));
}

#[test]
fn activity_event_cost_incurred_round_trip() {
    let event = ActivityEvent::CostIncurred {
        agent_id: "agent-2".into(),
        model: "claude-sonnet-4-20250514".into(),
        input_tokens: 1000,
        output_tokens: 500,
        cost_usd: 0.012,
    };
    let json = serde_json::to_string(&event).unwrap();
    let parsed: ActivityEvent = serde_json::from_str(&json).unwrap();
    match parsed {
        ActivityEvent::CostIncurred { cost_usd, .. } => {
            assert!((cost_usd - 0.012).abs() < 0.001);
        }
        _ => panic!("wrong variant"),
    }
}

#[test]
fn pause_reason_variants() {
    let reasons = vec![
        PauseReason::BudgetExhausted,
        PauseReason::UserRequested,
        PauseReason::ApprovalTimeout,
        PauseReason::Error("test".into()),
    ];
    for reason in reasons {
        let json = serde_json::to_string(&reason).unwrap();
        let _: PauseReason = serde_json::from_str(&json).unwrap();
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd hive && cargo test --package hive_agents --test activity_log_tests -- --nocapture 2>&1 | head -30`
Expected: compilation error — `activity` module doesn't exist

- [ ] **Step 3: Create the types module**

Create `hive/crates/hive_agents/src/activity/types.rs`:

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Reason an agent was paused.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PauseReason {
    BudgetExhausted,
    UserRequested,
    ApprovalTimeout,
    Error(String),
}

/// File operation type.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FileOp {
    Created,
    Modified,
    Deleted,
    Renamed { from: String },
}

/// Operation that may require approval.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum OperationType {
    ShellCommand(String),
    FileDelete(String),
    FileModify { path: String, scope: String },
    GitPush { remote: String, branch: String },
    AiCall { model: String, estimated_cost: f64 },
    Custom(String),
}

/// Every observable agent action in the system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ActivityEvent {
    // Agent lifecycle
    AgentStarted { agent_id: String, role: String, task_id: Option<String> },
    AgentCompleted { agent_id: String, duration_ms: u64, cost: f64 },
    AgentFailed { agent_id: String, error: String },
    AgentPaused { agent_id: String, reason: PauseReason },

    // Task lifecycle
    TaskClaimed { task_id: String, agent_id: String },
    TaskProgress { task_id: String, progress: f64, message: String },
    TaskCompleted { task_id: String, agent_id: String, cost: f64 },
    TaskFailed { task_id: String, error: String },

    // Tool/action execution
    ToolCalled { agent_id: String, tool_name: String, args_summary: String },
    FileModified { agent_id: String, path: String, op: FileOp },
    ShellExecuted { agent_id: String, command: String, exit_code: i32 },

    // Cost events
    CostIncurred {
        agent_id: String,
        model: String,
        input_tokens: u32,
        output_tokens: u32,
        cost_usd: f64,
    },
    BudgetWarning { agent_id: String, usage_pct: f64, limit_usd: f64 },
    BudgetExhausted { agent_id: String, limit_usd: f64 },

    // Approval events
    ApprovalRequested {
        request_id: String,
        agent_id: String,
        operation: String,
        context: String,
        rule: String,
    },
    ApprovalGranted { request_id: String },
    ApprovalDenied { request_id: String, reason: Option<String> },

    // Heartbeat events
    HeartbeatFired { agent_id: String, task_id: String },
    HeartbeatScheduled { agent_id: String, interval_secs: u64 },
    HeartbeatCancelled { agent_id: String },
}

impl ActivityEvent {
    /// Human-readable event type string for storage.
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::AgentStarted { .. } => "agent_started",
            Self::AgentCompleted { .. } => "agent_completed",
            Self::AgentFailed { .. } => "agent_failed",
            Self::AgentPaused { .. } => "agent_paused",
            Self::TaskClaimed { .. } => "task_claimed",
            Self::TaskProgress { .. } => "task_progress",
            Self::TaskCompleted { .. } => "task_completed",
            Self::TaskFailed { .. } => "task_failed",
            Self::ToolCalled { .. } => "tool_called",
            Self::FileModified { .. } => "file_modified",
            Self::ShellExecuted { .. } => "shell_executed",
            Self::CostIncurred { .. } => "cost_incurred",
            Self::BudgetWarning { .. } => "budget_warning",
            Self::BudgetExhausted { .. } => "budget_exhausted",
            Self::ApprovalRequested { .. } => "approval_requested",
            Self::ApprovalGranted { .. } => "approval_granted",
            Self::ApprovalDenied { .. } => "approval_denied",
            Self::HeartbeatFired { .. } => "heartbeat_fired",
            Self::HeartbeatScheduled { .. } => "heartbeat_scheduled",
            Self::HeartbeatCancelled { .. } => "heartbeat_cancelled",
        }
    }

    /// Category for filtering in UI.
    pub fn category(&self) -> &'static str {
        match self {
            Self::AgentStarted { .. }
            | Self::AgentCompleted { .. }
            | Self::AgentFailed { .. }
            | Self::AgentPaused { .. } => "agent",
            Self::TaskClaimed { .. }
            | Self::TaskProgress { .. }
            | Self::TaskCompleted { .. }
            | Self::TaskFailed { .. } => "task",
            Self::ToolCalled { .. }
            | Self::FileModified { .. }
            | Self::ShellExecuted { .. } => "tool",
            Self::CostIncurred { .. }
            | Self::BudgetWarning { .. }
            | Self::BudgetExhausted { .. } => "cost",
            Self::ApprovalRequested { .. }
            | Self::ApprovalGranted { .. }
            | Self::ApprovalDenied { .. } => "approval",
            Self::HeartbeatFired { .. }
            | Self::HeartbeatScheduled { .. }
            | Self::HeartbeatCancelled { .. } => "heartbeat",
        }
    }

    /// Human-readable summary for display.
    pub fn summary(&self) -> String {
        match self {
            Self::AgentStarted { agent_id, role, task_id } => {
                let task = task_id.as_deref().unwrap_or("no task");
                format!("{role} agent '{agent_id}' started (task: {task})")
            }
            Self::AgentCompleted { agent_id, duration_ms, cost } => {
                format!("Agent '{agent_id}' completed in {duration_ms}ms (${cost:.4})")
            }
            Self::AgentFailed { agent_id, error } => {
                format!("Agent '{agent_id}' failed: {error}")
            }
            Self::AgentPaused { agent_id, reason } => {
                format!("Agent '{agent_id}' paused: {reason:?}")
            }
            Self::TaskClaimed { task_id, agent_id } => {
                format!("Agent '{agent_id}' claimed task '{task_id}'")
            }
            Self::TaskProgress { task_id, progress, message } => {
                format!("Task '{task_id}': {:.0}% — {message}", progress * 100.0)
            }
            Self::TaskCompleted { task_id, agent_id, cost } => {
                format!("Task '{task_id}' completed by '{agent_id}' (${cost:.4})")
            }
            Self::TaskFailed { task_id, error } => {
                format!("Task '{task_id}' failed: {error}")
            }
            Self::ToolCalled { agent_id, tool_name, .. } => {
                format!("Agent '{agent_id}' called tool '{tool_name}'")
            }
            Self::FileModified { agent_id, path, op } => {
                format!("Agent '{agent_id}' {op:?} '{path}'")
            }
            Self::ShellExecuted { agent_id, command, exit_code } => {
                let status = if *exit_code == 0 { "ok" } else { "FAIL" };
                format!("Agent '{agent_id}' ran `{command}` [{status}]")
            }
            Self::CostIncurred { agent_id, model, cost_usd, input_tokens, output_tokens } => {
                let total_tok = input_tokens + output_tokens;
                format!("Agent '{agent_id}' spent ${cost_usd:.4} on {model} ({total_tok} tokens)")
            }
            Self::BudgetWarning { agent_id, usage_pct, limit_usd } => {
                format!("Budget warning: '{agent_id}' at {:.0}% of ${limit_usd:.2}", usage_pct * 100.0)
            }
            Self::BudgetExhausted { agent_id, limit_usd } => {
                format!("Budget exhausted: '{agent_id}' hit ${limit_usd:.2} limit")
            }
            Self::ApprovalRequested { agent_id, operation, .. } => {
                format!("Agent '{agent_id}' requests approval: {operation}")
            }
            Self::ApprovalGranted { request_id } => {
                format!("Approval granted: {request_id}")
            }
            Self::ApprovalDenied { request_id, reason } => {
                let r = reason.as_deref().unwrap_or("no reason");
                format!("Approval denied: {request_id} ({r})")
            }
            Self::HeartbeatFired { agent_id, task_id } => {
                format!("Heartbeat fired for '{agent_id}' on task '{task_id}'")
            }
            Self::HeartbeatScheduled { agent_id, interval_secs } => {
                format!("Heartbeat scheduled for '{agent_id}' every {interval_secs}s")
            }
            Self::HeartbeatCancelled { agent_id } => {
                format!("Heartbeat cancelled for '{agent_id}'")
            }
        }
    }

    /// Extract agent_id if present.
    pub fn agent_id(&self) -> Option<&str> {
        match self {
            Self::AgentStarted { agent_id, .. }
            | Self::AgentCompleted { agent_id, .. }
            | Self::AgentFailed { agent_id, .. }
            | Self::AgentPaused { agent_id, .. }
            | Self::TaskClaimed { agent_id, .. }
            | Self::TaskCompleted { agent_id, .. }
            | Self::ToolCalled { agent_id, .. }
            | Self::FileModified { agent_id, .. }
            | Self::ShellExecuted { agent_id, .. }
            | Self::CostIncurred { agent_id, .. }
            | Self::BudgetWarning { agent_id, .. }
            | Self::BudgetExhausted { agent_id, .. }
            | Self::ApprovalRequested { agent_id, .. }
            | Self::HeartbeatFired { agent_id, .. }
            | Self::HeartbeatScheduled { agent_id, .. }
            | Self::HeartbeatCancelled { agent_id, .. } => Some(agent_id),
            Self::TaskProgress { .. }
            | Self::TaskFailed { .. }
            | Self::ApprovalGranted { .. }
            | Self::ApprovalDenied { .. } => None,
        }
    }

    /// Extract cost_usd if this is a cost-bearing event.
    pub fn cost_usd(&self) -> f64 {
        match self {
            Self::CostIncurred { cost_usd, .. } => *cost_usd,
            Self::AgentCompleted { cost, .. } => *cost,
            Self::TaskCompleted { cost, .. } => *cost,
            _ => 0.0,
        }
    }
}
```

- [ ] **Step 4: Create the activity module entry point**

Create `hive/crates/hive_agents/src/activity/mod.rs`:

```rust
pub mod types;

pub use types::{ActivityEvent, FileOp, OperationType, PauseReason};
```

- [ ] **Step 5: Wire into hive_agents lib.rs**

In `hive/crates/hive_agents/src/lib.rs`, add after the existing `pub mod automation;` line:

```rust
pub mod activity;
```

And in the re-exports section at the bottom, add:

```rust
pub use activity::{ActivityEvent, FileOp, OperationType, PauseReason};
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cd hive && cargo test --package hive_agents --test activity_log_tests -- --nocapture 2>&1 | tail -10`
Expected: all 3 tests PASS

- [ ] **Step 7: Commit**

```bash
git add hive/crates/hive_agents/src/activity/ hive/crates/hive_agents/src/lib.rs hive/crates/hive_agents/tests/activity_log_tests.rs
git commit -m "feat(agents): add ActivityEvent enum and shared types"
```

### Task 2: ActivityService (event bus hub)

**Files:**
- Modify: `hive/crates/hive_agents/src/activity/mod.rs`

- [ ] **Step 1: Write the failing test for ActivityService**

Append to `hive/crates/hive_agents/tests/activity_log_tests.rs`:

```rust
use hive_agents::activity::ActivityService;

#[tokio::test]
async fn activity_service_emits_and_receives_events() {
    let service = ActivityService::new_bus_only();
    let mut rx = service.subscribe();

    service.emit(ActivityEvent::AgentStarted {
        agent_id: "test-agent".into(),
        role: "Coder".into(),
        task_id: None,
    });

    let event = rx.recv().await.unwrap();
    assert_eq!(event.event_type(), "agent_started");
    assert_eq!(event.agent_id(), Some("test-agent"));
}

#[tokio::test]
async fn activity_service_multiple_subscribers() {
    let service = ActivityService::new_bus_only();
    let mut rx1 = service.subscribe();
    let mut rx2 = service.subscribe();

    service.emit(ActivityEvent::CostIncurred {
        agent_id: "a".into(),
        model: "test".into(),
        input_tokens: 100,
        output_tokens: 50,
        cost_usd: 0.01,
    });

    let e1 = rx1.recv().await.unwrap();
    let e2 = rx2.recv().await.unwrap();
    assert_eq!(e1.event_type(), "cost_incurred");
    assert_eq!(e2.event_type(), "cost_incurred");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd hive && cargo test --package hive_agents --test activity_log_tests activity_service -- --nocapture 2>&1 | head -20`
Expected: compilation error — `ActivityService` doesn't exist

- [ ] **Step 3: Implement ActivityService**

Update `hive/crates/hive_agents/src/activity/mod.rs`:

```rust
pub mod types;

use tokio::sync::broadcast;
pub use types::{ActivityEvent, FileOp, OperationType, PauseReason};

/// Central event bus for all agent activity.
///
/// Created once at app startup. All agent infrastructure holds an `Arc<ActivityService>`
/// and calls `emit()` to publish events. Listeners subscribe via `subscribe()`.
pub struct ActivityService {
    tx: broadcast::Sender<ActivityEvent>,
}

impl ActivityService {
    /// Create a minimal service with only the broadcast bus (no listeners).
    /// Used for testing and incremental construction.
    pub fn new_bus_only() -> Self {
        let (tx, _) = broadcast::channel(1024);
        Self { tx }
    }

    /// Emit an event to all listeners. Fire-and-forget.
    pub fn emit(&self, event: ActivityEvent) {
        // Ignore send errors (no receivers = that's fine)
        let _ = self.tx.send(event);
    }

    /// Subscribe to the event stream.
    pub fn subscribe(&self) -> broadcast::Receiver<ActivityEvent> {
        self.tx.subscribe()
    }
}
```

Note: `ActivityEvent` must derive `Clone` for broadcast. Add `#[derive(Clone)]` to `ActivityEvent` in `types.rs` (it's already there from the serde derives — `Serialize, Deserialize` don't require Clone, but `broadcast::Sender::send()` does). Update the derive line to:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ActivityEvent {
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd hive && cargo test --package hive_agents --test activity_log_tests -- --nocapture 2>&1 | tail -10`
Expected: all 5 tests PASS

- [ ] **Step 5: Commit**

```bash
git add hive/crates/hive_agents/src/activity/
git commit -m "feat(agents): add ActivityService event bus"
```

### Task 3: ActivityLog (SQLite persistence)

**Files:**
- Create: `hive/crates/hive_agents/src/activity/log.rs`
- Modify: `hive/crates/hive_agents/src/activity/mod.rs`

- [ ] **Step 1: Write the failing test**

Append to `hive/crates/hive_agents/tests/activity_log_tests.rs`:

```rust
use hive_agents::activity::log::{ActivityLog, ActivityFilter};

#[test]
fn activity_log_insert_and_query() {
    let log = ActivityLog::open_in_memory().unwrap();

    let event = ActivityEvent::CostIncurred {
        agent_id: "agent-1".into(),
        model: "claude-sonnet-4-20250514".into(),
        input_tokens: 1000,
        output_tokens: 500,
        cost_usd: 0.015,
    };
    log.record(&event).unwrap();

    let entries = log.query(&ActivityFilter::default()).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].event_type, "cost_incurred");
    assert!((entries[0].cost_usd - 0.015).abs() < 0.001);
    assert!(entries[0].summary.contains("agent-1"));
}

#[test]
fn activity_log_filter_by_category() {
    let log = ActivityLog::open_in_memory().unwrap();

    log.record(&ActivityEvent::AgentStarted {
        agent_id: "a".into(), role: "Coder".into(), task_id: None,
    }).unwrap();
    log.record(&ActivityEvent::CostIncurred {
        agent_id: "a".into(), model: "m".into(),
        input_tokens: 100, output_tokens: 50, cost_usd: 0.01,
    }).unwrap();

    let filter = ActivityFilter {
        categories: Some(vec!["cost".into()]),
        ..Default::default()
    };
    let entries = log.query(&filter).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].category, "cost");
}

#[test]
fn activity_log_cost_summary() {
    let log = ActivityLog::open_in_memory().unwrap();

    for i in 0..5 {
        log.record(&ActivityEvent::CostIncurred {
            agent_id: if i < 3 { "a".into() } else { "b".into() },
            model: "claude-sonnet-4-20250514".into(),
            input_tokens: 1000, output_tokens: 500, cost_usd: 1.0,
        }).unwrap();
    }

    let summary = log.cost_summary(None, chrono::Utc::now() - chrono::Duration::hours(1)).unwrap();
    assert!((summary.total_usd - 5.0).abs() < 0.01);
    assert_eq!(summary.request_count, 5);
    assert_eq!(summary.by_agent.len(), 2);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd hive && cargo test --package hive_agents --test activity_log_tests activity_log -- --nocapture 2>&1 | head -20`
Expected: compilation error — `activity::log` module doesn't exist

- [ ] **Step 3: Implement ActivityLog**

Create `hive/crates/hive_agents/src/activity/log.rs`:

```rust
use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;

use super::ActivityEvent;

/// A persisted activity event entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityEntry {
    pub id: i64,
    pub timestamp: String,
    pub event_type: String,
    pub agent_id: Option<String>,
    pub task_id: Option<String>,
    pub category: String,
    pub summary: String,
    pub detail_json: Option<String>,
    pub cost_usd: f64,
}

/// Filter for querying activity events.
#[derive(Debug, Clone, Default)]
pub struct ActivityFilter {
    pub categories: Option<Vec<String>>,
    pub agent_id: Option<String>,
    pub since: Option<DateTime<Utc>>,
    pub search: Option<String>,
    pub limit: usize,
    pub offset: usize,
}

/// Cost summary across agents and models.
#[derive(Debug, Clone, Default)]
pub struct CostSummary {
    pub total_usd: f64,
    pub by_agent: Vec<(String, f64)>,
    pub by_model: Vec<(String, f64)>,
    pub request_count: usize,
}

/// SQLite-backed activity log.
pub struct ActivityLog {
    conn: Mutex<Connection>,
}

impl ActivityLog {
    /// Open the activity log at the given path.
    pub fn open(path: &std::path::Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        let log = Self { conn: Mutex::new(conn) };
        log.init_schema()?;
        Ok(log)
    }

    /// Open an in-memory database (for testing).
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let log = Self { conn: Mutex::new(conn) };
        log.init_schema()?;
        Ok(log)
    }

    fn init_schema(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS activity_events (
                id            INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp     TEXT    NOT NULL,
                event_type    TEXT    NOT NULL,
                agent_id      TEXT,
                task_id       TEXT,
                category      TEXT    NOT NULL,
                summary       TEXT    NOT NULL,
                detail_json   TEXT,
                cost_usd      REAL    DEFAULT 0.0
            );
            CREATE INDEX IF NOT EXISTS idx_events_category  ON activity_events(category);
            CREATE INDEX IF NOT EXISTS idx_events_agent     ON activity_events(agent_id);
            CREATE INDEX IF NOT EXISTS idx_events_timestamp ON activity_events(timestamp);
            CREATE INDEX IF NOT EXISTS idx_events_type      ON activity_events(event_type);",
        )?;
        Ok(())
    }

    /// Record an activity event.
    pub fn record(&self, event: &ActivityEvent) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        let detail = serde_json::to_string(event).ok();
        conn.execute(
            "INSERT INTO activity_events (timestamp, event_type, agent_id, task_id, category, summary, detail_json, cost_usd)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                Utc::now().to_rfc3339(),
                event.event_type(),
                event.agent_id(),
                extract_task_id(event),
                event.category(),
                event.summary(),
                detail,
                event.cost_usd(),
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// Query events with filters.
    pub fn query(&self, filter: &ActivityFilter) -> Result<Vec<ActivityEntry>> {
        let conn = self.conn.lock().unwrap();
        let mut sql = String::from("SELECT id, timestamp, event_type, agent_id, task_id, category, summary, detail_json, cost_usd FROM activity_events WHERE 1=1");
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

        if let Some(ref cats) = filter.categories {
            if !cats.is_empty() {
                let placeholders: Vec<String> = cats.iter().enumerate().map(|(i, _)| format!("?{}", param_values.len() + i + 1)).collect();
                sql.push_str(&format!(" AND category IN ({})", placeholders.join(",")));
                for c in cats {
                    param_values.push(Box::new(c.clone()));
                }
            }
        }

        if let Some(ref agent) = filter.agent_id {
            param_values.push(Box::new(agent.clone()));
            sql.push_str(&format!(" AND agent_id = ?{}", param_values.len()));
        }

        if let Some(ref since) = filter.since {
            param_values.push(Box::new(since.to_rfc3339()));
            sql.push_str(&format!(" AND timestamp >= ?{}", param_values.len()));
        }

        if let Some(ref search) = filter.search {
            param_values.push(Box::new(format!("%{search}%")));
            sql.push_str(&format!(" AND summary LIKE ?{}", param_values.len()));
        }

        sql.push_str(" ORDER BY id DESC");

        let limit = if filter.limit == 0 { 100 } else { filter.limit };
        param_values.push(Box::new(limit as i64));
        sql.push_str(&format!(" LIMIT ?{}", param_values.len()));

        if filter.offset > 0 {
            param_values.push(Box::new(filter.offset as i64));
            sql.push_str(&format!(" OFFSET ?{}", param_values.len()));
        }

        let params_refs: Vec<&dyn rusqlite::types::ToSql> = param_values.iter().map(|p| p.as_ref()).collect();
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params_refs.as_slice(), |row| {
            Ok(ActivityEntry {
                id: row.get(0)?,
                timestamp: row.get(1)?,
                event_type: row.get(2)?,
                agent_id: row.get(3)?,
                task_id: row.get(4)?,
                category: row.get(5)?,
                summary: row.get(6)?,
                detail_json: row.get(7)?,
                cost_usd: row.get(8)?,
            })
        })?;

        let mut entries = Vec::new();
        for row in rows {
            entries.push(row?);
        }
        Ok(entries)
    }

    /// Summarize costs since a given timestamp.
    pub fn cost_summary(&self, agent_id: Option<&str>, since: DateTime<Utc>) -> Result<CostSummary> {
        let conn = self.conn.lock().unwrap();
        let since_str = since.to_rfc3339();

        // Total
        let (total, count): (f64, usize) = if let Some(aid) = agent_id {
            conn.query_row(
                "SELECT COALESCE(SUM(cost_usd), 0), COUNT(*) FROM activity_events WHERE event_type = 'cost_incurred' AND timestamp >= ?1 AND agent_id = ?2",
                params![since_str, aid],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?
        } else {
            conn.query_row(
                "SELECT COALESCE(SUM(cost_usd), 0), COUNT(*) FROM activity_events WHERE event_type = 'cost_incurred' AND timestamp >= ?1",
                params![since_str],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?
        };

        // By agent
        let mut stmt = conn.prepare(
            "SELECT agent_id, SUM(cost_usd) FROM activity_events WHERE event_type = 'cost_incurred' AND timestamp >= ?1 GROUP BY agent_id",
        )?;
        let by_agent: Vec<(String, f64)> = stmt.query_map(params![since_str], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
        })?.filter_map(|r| r.ok()).collect();

        // By model — parse from detail_json
        let mut stmt = conn.prepare(
            "SELECT detail_json, cost_usd FROM activity_events WHERE event_type = 'cost_incurred' AND timestamp >= ?1",
        )?;
        let mut model_map = std::collections::HashMap::new();
        let mut rows = stmt.query(params![since_str])?;
        while let Some(row) = rows.next()? {
            let json: Option<String> = row.get(0)?;
            let cost: f64 = row.get(1)?;
            if let Some(json) = json {
                if let Ok(event) = serde_json::from_str::<ActivityEvent>(&json) {
                    if let ActivityEvent::CostIncurred { model, .. } = event {
                        *model_map.entry(model).or_insert(0.0) += cost;
                    }
                }
            }
        }
        let by_model: Vec<(String, f64)> = model_map.into_iter().collect();

        Ok(CostSummary {
            total_usd: total,
            by_agent,
            by_model,
            request_count: count,
        })
    }

    /// Total number of events.
    pub fn total_events(&self) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM activity_events", [], |row| row.get(0))?;
        Ok(count as usize)
    }
}

/// Extract task_id from events that have one.
fn extract_task_id(event: &ActivityEvent) -> Option<&str> {
    match event {
        ActivityEvent::AgentStarted { task_id, .. } => task_id.as_deref(),
        ActivityEvent::TaskClaimed { task_id, .. }
        | ActivityEvent::TaskProgress { task_id, .. }
        | ActivityEvent::TaskCompleted { task_id, .. }
        | ActivityEvent::TaskFailed { task_id, .. }
        | ActivityEvent::HeartbeatFired { task_id, .. } => Some(task_id),
        _ => None,
    }
}
```

- [ ] **Step 4: Update activity/mod.rs to export log module**

```rust
pub mod log;
pub mod types;

pub use types::{ActivityEvent, FileOp, OperationType, PauseReason};
// ... existing ActivityService code
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd hive && cargo test --package hive_agents --test activity_log_tests -- --nocapture 2>&1 | tail -15`
Expected: all 8 tests PASS

- [ ] **Step 6: Commit**

```bash
git add hive/crates/hive_agents/src/activity/log.rs hive/crates/hive_agents/src/activity/mod.rs hive/crates/hive_agents/tests/activity_log_tests.rs
git commit -m "feat(agents): add ActivityLog SQLite persistence"
```

### Task 4: Wire ActivityLog as bus listener

**Files:**
- Modify: `hive/crates/hive_agents/src/activity/mod.rs`

- [ ] **Step 1: Write the failing test**

Append to `hive/crates/hive_agents/tests/activity_log_tests.rs`:

```rust
#[tokio::test]
async fn activity_service_with_log_persists_events() {
    let log = std::sync::Arc::new(ActivityLog::open_in_memory().unwrap());
    let service = ActivityService::new_with_log(log.clone());

    service.emit(ActivityEvent::AgentStarted {
        agent_id: "test".into(),
        role: "Coder".into(),
        task_id: None,
    });

    // Give the listener task a moment to process
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let entries = log.query(&ActivityFilter::default()).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].event_type, "agent_started");
}
```

- [ ] **Step 2: Run test to verify it fails**

Expected: `new_with_log` method doesn't exist

- [ ] **Step 3: Implement the listener wiring**

Update `ActivityService` in `activity/mod.rs` to add `new_with_log`:

```rust
use std::sync::Arc;
use tokio::sync::broadcast;

pub struct ActivityService {
    tx: broadcast::Sender<ActivityEvent>,
}

impl ActivityService {
    pub fn new_bus_only() -> Self {
        let (tx, _) = broadcast::channel(1024);
        Self { tx }
    }

    /// Create service with an ActivityLog listener that persists all events.
    pub fn new_with_log(log: Arc<log::ActivityLog>) -> Self {
        let (tx, _) = broadcast::channel(1024);
        let mut rx = tx.subscribe();

        // Spawn listener task
        let log_ref = log.clone();
        tokio::spawn(async move {
            while let Ok(event) = rx.recv().await {
                if let Err(e) = log_ref.record(&event) {
                    tracing::warn!("ActivityLog write failed: {e}");
                }
            }
        });

        Self { tx }
    }

    pub fn emit(&self, event: ActivityEvent) {
        let _ = self.tx.send(event);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ActivityEvent> {
        self.tx.subscribe()
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd hive && cargo test --package hive_agents --test activity_log_tests -- --nocapture 2>&1 | tail -10`
Expected: all 9 tests PASS

- [ ] **Step 5: Commit**

```bash
git add hive/crates/hive_agents/src/activity/mod.rs hive/crates/hive_agents/tests/activity_log_tests.rs
git commit -m "feat(agents): wire ActivityLog as event bus listener"
```

---

## Chunk 2: Budget Enforcement

### Task 5: BudgetEnforcer

**Files:**
- Create: `hive/crates/hive_agents/src/activity/budget.rs`
- Create: `hive/crates/hive_agents/tests/budget_tests.rs`

- [ ] **Step 1: Write the failing tests**

Create `hive/crates/hive_agents/tests/budget_tests.rs`:

```rust
use hive_agents::activity::budget::{BudgetConfig, BudgetDecision, BudgetEnforcer, ExhaustAction};
use hive_agents::activity::log::ActivityLog;
use hive_agents::activity::{ActivityEvent, ActivityService};
use std::sync::Arc;

fn test_enforcer(daily_limit: f64) -> (Arc<ActivityLog>, BudgetEnforcer) {
    let log = Arc::new(ActivityLog::open_in_memory().unwrap());
    let config = BudgetConfig {
        global_daily_limit_usd: Some(daily_limit),
        global_monthly_limit_usd: None,
        per_agent_limit_usd: None,
        per_task_limit_usd: None,
        warning_threshold_pct: 0.8,
        on_exhaust: ExhaustAction::Pause,
    };
    let enforcer = BudgetEnforcer::new(config, log.clone());
    (log, enforcer)
}

#[test]
fn budget_proceed_when_under_limit() {
    let (log, enforcer) = test_enforcer(10.0);
    // No costs recorded — should proceed
    let decision = enforcer.check("agent-1", 0.5);
    assert!(matches!(decision, BudgetDecision::Proceed));
}

#[test]
fn budget_warning_at_threshold() {
    let (log, enforcer) = test_enforcer(10.0);
    // Record $8.50 in costs (85% > 80% threshold)
    for _ in 0..85 {
        log.record(&ActivityEvent::CostIncurred {
            agent_id: "agent-1".into(),
            model: "test".into(),
            input_tokens: 100,
            output_tokens: 50,
            cost_usd: 0.1,
        }).unwrap();
    }
    let decision = enforcer.check("agent-1", 0.5);
    assert!(matches!(decision, BudgetDecision::Warning { .. }));
}

#[test]
fn budget_blocked_at_limit() {
    let (log, enforcer) = test_enforcer(1.0);
    // Record $1.50 in costs (over $1.0 limit)
    for _ in 0..15 {
        log.record(&ActivityEvent::CostIncurred {
            agent_id: "agent-1".into(),
            model: "test".into(),
            input_tokens: 100,
            output_tokens: 50,
            cost_usd: 0.1,
        }).unwrap();
    }
    let decision = enforcer.check("agent-1", 0.5);
    assert!(matches!(decision, BudgetDecision::Blocked { .. }));
}

#[test]
fn budget_no_limit_always_proceeds() {
    let log = Arc::new(ActivityLog::open_in_memory().unwrap());
    let config = BudgetConfig::default();
    let enforcer = BudgetEnforcer::new(config, log);
    let decision = enforcer.check("agent-1", 100.0);
    assert!(matches!(decision, BudgetDecision::Proceed));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd hive && cargo test --package hive_agents --test budget_tests -- --nocapture 2>&1 | head -20`
Expected: compilation error — `budget` module doesn't exist

- [ ] **Step 3: Implement BudgetEnforcer**

Create `hive/crates/hive_agents/src/activity/budget.rs`:

```rust
use std::sync::Arc;
use chrono::{Utc, Duration};
use serde::{Deserialize, Serialize};

use super::log::ActivityLog;

/// Budget configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetConfig {
    pub global_daily_limit_usd: Option<f64>,
    pub global_monthly_limit_usd: Option<f64>,
    pub per_agent_limit_usd: Option<f64>,
    pub per_task_limit_usd: Option<f64>,
    pub warning_threshold_pct: f64,
    pub on_exhaust: ExhaustAction,
}

impl Default for BudgetConfig {
    fn default() -> Self {
        Self {
            global_daily_limit_usd: None,
            global_monthly_limit_usd: None,
            per_agent_limit_usd: None,
            per_task_limit_usd: None,
            warning_threshold_pct: 0.8,
            on_exhaust: ExhaustAction::Pause,
        }
    }
}

/// What to do when budget is exhausted.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ExhaustAction {
    Pause,
    ApprovalRequired,
    WarnOnly,
}

/// Result of a budget check.
#[derive(Debug, Clone)]
pub enum BudgetDecision {
    Proceed,
    Warning { usage_pct: f64, message: String },
    Blocked { reason: String },
}

/// Enforces cost budgets by checking cumulative spend against limits.
pub struct BudgetEnforcer {
    config: BudgetConfig,
    log: Arc<ActivityLog>,
}

impl BudgetEnforcer {
    pub fn new(config: BudgetConfig, log: Arc<ActivityLog>) -> Self {
        Self { config, log }
    }

    /// Check if an operation should proceed given current spend.
    pub fn check(&self, agent_id: &str, estimated_cost: f64) -> BudgetDecision {
        // Check daily global limit
        if let Some(daily_limit) = self.config.global_daily_limit_usd {
            let since = Utc::now() - Duration::hours(24);
            if let Ok(summary) = self.log.cost_summary(None, since) {
                let projected = summary.total_usd + estimated_cost;
                let usage_pct = summary.total_usd / daily_limit;

                if projected >= daily_limit {
                    return BudgetDecision::Blocked {
                        reason: format!(
                            "Daily budget ${:.2} would be exceeded (current: ${:.2}, estimated: ${:.2})",
                            daily_limit, summary.total_usd, estimated_cost
                        ),
                    };
                }

                if usage_pct >= self.config.warning_threshold_pct {
                    return BudgetDecision::Warning {
                        usage_pct,
                        message: format!(
                            "Daily budget at {:.0}% (${:.2} / ${:.2})",
                            usage_pct * 100.0, summary.total_usd, daily_limit
                        ),
                    };
                }
            }
        }

        // Check per-agent limit
        if let Some(agent_limit) = self.config.per_agent_limit_usd {
            let since = Utc::now() - Duration::days(30);
            if let Ok(summary) = self.log.cost_summary(Some(agent_id), since) {
                let projected = summary.total_usd + estimated_cost;
                if projected >= agent_limit {
                    return BudgetDecision::Blocked {
                        reason: format!(
                            "Agent '{agent_id}' monthly budget ${agent_limit:.2} would be exceeded"
                        ),
                    };
                }
            }
        }

        BudgetDecision::Proceed
    }

    /// Get current config.
    pub fn config(&self) -> &BudgetConfig {
        &self.config
    }

    /// Update config at runtime.
    pub fn set_config(&mut self, config: BudgetConfig) {
        self.config = config;
    }
}
```

- [ ] **Step 4: Update activity/mod.rs to export budget module**

Add `pub mod budget;` to `activity/mod.rs`.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd hive && cargo test --package hive_agents --test budget_tests -- --nocapture 2>&1 | tail -10`
Expected: all 4 tests PASS

- [ ] **Step 6: Commit**

```bash
git add hive/crates/hive_agents/src/activity/budget.rs hive/crates/hive_agents/src/activity/mod.rs hive/crates/hive_agents/tests/budget_tests.rs
git commit -m "feat(agents): add BudgetEnforcer with daily and per-agent limits"
```

---

## Chunk 3: Approval Gates

### Task 6: ApprovalRule and RuleTrigger

**Files:**
- Create: `hive/crates/hive_agents/src/activity/rules.rs`
- Create: `hive/crates/hive_agents/tests/approval_tests.rs`

- [ ] **Step 1: Write the failing tests**

Create `hive/crates/hive_agents/tests/approval_tests.rs`:

```rust
use hive_agents::activity::rules::{ApprovalRule, RuleTrigger};
use hive_agents::activity::OperationType;

#[test]
fn rule_matches_shell_command_pattern() {
    let rule = ApprovalRule {
        name: "git-push".into(),
        enabled: true,
        trigger: RuleTrigger::CommandMatches { pattern: "git push*".into() },
        priority: 80,
    };
    let op = OperationType::ShellCommand("git push origin main".into());
    assert!(rule.matches(&op));
}

#[test]
fn rule_does_not_match_different_command() {
    let rule = ApprovalRule {
        name: "git-push".into(),
        enabled: true,
        trigger: RuleTrigger::CommandMatches { pattern: "git push*".into() },
        priority: 80,
    };
    let op = OperationType::ShellCommand("git status".into());
    assert!(!rule.matches(&op));
}

#[test]
fn rule_matches_cost_threshold() {
    let rule = ApprovalRule {
        name: "expensive".into(),
        enabled: true,
        trigger: RuleTrigger::CostExceeds { usd: 5.0 },
        priority: 90,
    };
    let op = OperationType::AiCall { model: "claude-opus-4-6".into(), estimated_cost: 7.50 };
    assert!(rule.matches(&op));
}

#[test]
fn rule_cost_under_threshold_no_match() {
    let rule = ApprovalRule {
        name: "expensive".into(),
        enabled: true,
        trigger: RuleTrigger::CostExceeds { usd: 5.0 },
        priority: 90,
    };
    let op = OperationType::AiCall { model: "claude-haiku".into(), estimated_cost: 0.50 };
    assert!(!rule.matches(&op));
}

#[test]
fn rule_matches_path_glob() {
    let rule = ApprovalRule {
        name: "protect-core".into(),
        enabled: true,
        trigger: RuleTrigger::PathMatches { glob: "src/core/**".into() },
        priority: 75,
    };
    let op = OperationType::FileModify {
        path: "src/core/config.rs".into(),
        scope: "1 file".into(),
    };
    assert!(rule.matches(&op));
}

#[test]
fn disabled_rule_never_matches() {
    let rule = ApprovalRule {
        name: "disabled".into(),
        enabled: false,
        trigger: RuleTrigger::Always,
        priority: 100,
    };
    let op = OperationType::ShellCommand("anything".into());
    assert!(!rule.matches(&op));
}

#[test]
fn rules_sorted_by_priority_descending() {
    let mut rules = vec![
        ApprovalRule { name: "low".into(), enabled: true, trigger: RuleTrigger::Always, priority: 10 },
        ApprovalRule { name: "high".into(), enabled: true, trigger: RuleTrigger::Always, priority: 100 },
        ApprovalRule { name: "mid".into(), enabled: true, trigger: RuleTrigger::Always, priority: 50 },
    ];
    rules.sort_by(|a, b| b.priority.cmp(&a.priority));
    assert_eq!(rules[0].name, "high");
    assert_eq!(rules[1].name, "mid");
    assert_eq!(rules[2].name, "low");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd hive && cargo test --package hive_agents --test approval_tests -- --nocapture 2>&1 | head -20`
Expected: compilation error — `rules` module doesn't exist

- [ ] **Step 3: Implement rules module**

Create `hive/crates/hive_agents/src/activity/rules.rs`:

```rust
use serde::{Deserialize, Serialize};

use super::OperationType;

/// An approval rule that triggers on matching operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRule {
    pub name: String,
    pub enabled: bool,
    pub trigger: RuleTrigger,
    pub priority: u8,
}

/// What triggers an approval requirement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RuleTrigger {
    /// Wraps SecurityGateway soft-deny results.
    SecurityGatewayBlock,
    /// Cost exceeds a threshold.
    CostExceeds { usd: f64 },
    /// File path matches a glob pattern.
    PathMatches { glob: String },
    /// More than N files affected.
    FilesExceed { count: usize },
    /// Shell command matches a glob pattern.
    CommandMatches { pattern: String },
    /// Always triggers (paranoia/debug mode).
    Always,
}

impl ApprovalRule {
    /// Check if this rule matches the given operation.
    pub fn matches(&self, op: &OperationType) -> bool {
        if !self.enabled {
            return false;
        }
        match &self.trigger {
            RuleTrigger::SecurityGatewayBlock => {
                // This is matched externally by the SecurityGateway integration
                false
            }
            RuleTrigger::CostExceeds { usd } => {
                if let OperationType::AiCall { estimated_cost, .. } = op {
                    *estimated_cost > *usd
                } else {
                    false
                }
            }
            RuleTrigger::PathMatches { glob: pattern } => {
                let path = match op {
                    OperationType::FileModify { path, .. } => Some(path.as_str()),
                    OperationType::FileDelete(path) => Some(path.as_str()),
                    _ => None,
                };
                if let Some(path) = path {
                    glob_match(pattern, path)
                } else {
                    false
                }
            }
            RuleTrigger::FilesExceed { count } => {
                if let OperationType::FileModify { scope, .. } = op {
                    // Parse "N files" from scope string
                    scope.split_whitespace()
                        .next()
                        .and_then(|n| n.parse::<usize>().ok())
                        .map(|n| n > *count)
                        .unwrap_or(false)
                } else {
                    false
                }
            }
            RuleTrigger::CommandMatches { pattern } => {
                if let OperationType::ShellCommand(cmd) = op {
                    glob_match(pattern, cmd)
                } else {
                    false
                }
            }
            RuleTrigger::Always => true,
        }
    }

    /// Default rules shipped with Hive.
    pub fn defaults() -> Vec<ApprovalRule> {
        vec![
            ApprovalRule {
                name: "security-gateway".into(),
                enabled: true,
                trigger: RuleTrigger::SecurityGatewayBlock,
                priority: 100,
            },
            ApprovalRule {
                name: "expensive-operations".into(),
                enabled: true,
                trigger: RuleTrigger::CostExceeds { usd: 5.0 },
                priority: 90,
            },
            ApprovalRule {
                name: "git-push".into(),
                enabled: true,
                trigger: RuleTrigger::CommandMatches { pattern: "git push*".into() },
                priority: 80,
            },
            ApprovalRule {
                name: "bulk-modify".into(),
                enabled: true,
                trigger: RuleTrigger::FilesExceed { count: 10 },
                priority: 70,
            },
        ]
    }
}

/// Simple glob matching: `*` matches any sequence of characters.
fn glob_match(pattern: &str, text: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return pattern == text;
    }
    let mut pos = 0;
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        match text[pos..].find(part) {
            Some(idx) => {
                if i == 0 && idx != 0 {
                    // First part must match at start if pattern doesn't start with *
                    return false;
                }
                pos += idx + part.len();
            }
            None => return false,
        }
    }
    // If pattern doesn't end with *, text must end exactly
    if !pattern.ends_with('*') && pos != text.len() {
        return false;
    }
    true
}
```

- [ ] **Step 4: Add `pub mod rules;` to activity/mod.rs**

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd hive && cargo test --package hive_agents --test approval_tests -- --nocapture 2>&1 | tail -10`
Expected: all 7 tests PASS

- [ ] **Step 6: Commit**

```bash
git add hive/crates/hive_agents/src/activity/rules.rs hive/crates/hive_agents/src/activity/mod.rs hive/crates/hive_agents/tests/approval_tests.rs
git commit -m "feat(agents): add ApprovalRule engine with glob matching"
```

### Task 7: ApprovalGate (oneshot channels)

**Files:**
- Create: `hive/crates/hive_agents/src/activity/approval.rs`

- [ ] **Step 1: Write the failing tests**

Append to `hive/crates/hive_agents/tests/approval_tests.rs`:

```rust
use hive_agents::activity::approval::{ApprovalGate, ApprovalDecision, ApprovalRequest};

#[tokio::test]
async fn approval_gate_no_rules_match_proceeds() {
    let gate = ApprovalGate::new(vec![]);
    let result = gate.check("agent-1", &OperationType::ShellCommand("ls".into())).await;
    assert!(result.is_ok()); // None = no approval needed
}

#[tokio::test]
async fn approval_gate_rule_match_creates_request() {
    let rules = vec![
        ApprovalRule {
            name: "always".into(),
            enabled: true,
            trigger: RuleTrigger::Always,
            priority: 100,
        },
    ];
    let gate = ApprovalGate::new(rules);

    // Check creates a pending request
    let pending = gate.check_sync("agent-1", &OperationType::ShellCommand("test".into()));
    assert!(pending.is_some());
    let request = pending.unwrap();
    assert_eq!(request.matched_rule, "always");

    // Respond to it
    gate.respond(&request.id, ApprovalDecision::Approved);
    assert_eq!(gate.pending_count(), 0);
}

#[tokio::test]
async fn approval_gate_respond_deny() {
    let rules = vec![
        ApprovalRule {
            name: "always".into(),
            enabled: true,
            trigger: RuleTrigger::Always,
            priority: 100,
        },
    ];
    let gate = ApprovalGate::new(rules);

    let pending = gate.check_sync("agent-1", &OperationType::ShellCommand("test".into()));
    assert!(pending.is_some());

    gate.respond(&pending.unwrap().id, ApprovalDecision::Denied { reason: Some("nope".into()) });
    assert_eq!(gate.pending_count(), 0);
}
```

- [ ] **Step 2: Run test to verify it fails**

Expected: `approval` module doesn't exist

- [ ] **Step 3: Implement ApprovalGate**

Create `hive/crates/hive_agents/src/activity/approval.rs`:

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use tokio::sync::oneshot;
use uuid::Uuid;

use super::rules::ApprovalRule;
use super::OperationType;

/// A pending approval request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRequest {
    pub id: String,
    pub agent_id: String,
    pub timestamp: DateTime<Utc>,
    pub operation: OperationType,
    pub context: String,
    pub matched_rule: String,
    pub estimated_cost: Option<f64>,
    pub timeout_secs: Option<u64>,
}

/// Decision on an approval request.
#[derive(Debug, Clone, PartialEq)]
pub enum ApprovalDecision {
    Approved,
    Denied { reason: Option<String> },
    Timeout,
}

/// Approval gate that routes operations through rules and human approval.
pub struct ApprovalGate {
    rules: Vec<ApprovalRule>,
    pending: Mutex<HashMap<String, ApprovalRequest>>,
    response_channels: Mutex<HashMap<String, oneshot::Sender<ApprovalDecision>>>,
}

impl ApprovalGate {
    pub fn new(mut rules: Vec<ApprovalRule>) -> Self {
        // Sort by priority descending — first match wins
        rules.sort_by(|a, b| b.priority.cmp(&a.priority));
        Self {
            rules,
            pending: Mutex::new(HashMap::new()),
            response_channels: Mutex::new(HashMap::new()),
        }
    }

    /// Check if an operation needs approval. Returns None if no rule matches.
    /// If a rule matches, creates a pending request and returns it + a oneshot receiver.
    pub fn check_with_channel(
        &self,
        agent_id: &str,
        operation: &OperationType,
    ) -> Option<(ApprovalRequest, oneshot::Receiver<ApprovalDecision>)> {
        let matched_rule = self.rules.iter().find(|r| r.matches(operation))?;

        let request = ApprovalRequest {
            id: Uuid::new_v4().to_string(),
            agent_id: agent_id.into(),
            timestamp: Utc::now(),
            operation: operation.clone(),
            context: format!("{operation:?}"),
            matched_rule: matched_rule.name.clone(),
            estimated_cost: match operation {
                OperationType::AiCall { estimated_cost, .. } => Some(*estimated_cost),
                _ => None,
            },
            timeout_secs: Some(300),
        };

        let (tx, rx) = oneshot::channel();

        self.pending.lock().unwrap().insert(request.id.clone(), request.clone());
        self.response_channels.lock().unwrap().insert(request.id.clone(), tx);

        Some((request, rx))
    }

    /// Synchronous check — returns the request if approval is needed, without the channel.
    /// Used in tests and UI-only flows.
    pub fn check_sync(&self, agent_id: &str, operation: &OperationType) -> Option<ApprovalRequest> {
        self.check_with_channel(agent_id, operation).map(|(req, _rx)| req)
    }

    /// Respond to a pending approval request.
    pub fn respond(&self, request_id: &str, decision: ApprovalDecision) {
        self.pending.lock().unwrap().remove(request_id);
        if let Some(tx) = self.response_channels.lock().unwrap().remove(request_id) {
            let _ = tx.send(decision);
        }
    }

    /// Number of pending requests.
    pub fn pending_count(&self) -> usize {
        self.pending.lock().unwrap().len()
    }

    /// Get all pending requests (for UI display).
    pub fn pending_requests(&self) -> Vec<ApprovalRequest> {
        self.pending.lock().unwrap().values().cloned().collect()
    }

    /// Get current rules.
    pub fn rules(&self) -> &[ApprovalRule] {
        &self.rules
    }
}
```

- [ ] **Step 4: Add `pub mod approval;` to activity/mod.rs**

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd hive && cargo test --package hive_agents --test approval_tests -- --nocapture 2>&1 | tail -10`
Expected: all 10 tests PASS

- [ ] **Step 6: Commit**

```bash
git add hive/crates/hive_agents/src/activity/approval.rs hive/crates/hive_agents/src/activity/mod.rs hive/crates/hive_agents/tests/approval_tests.rs
git commit -m "feat(agents): add ApprovalGate with oneshot response channels"
```

---

## Chunk 4: Notification Service + SecurityGateway Integration

### Task 8: NotificationService

**Files:**
- Create: `hive/crates/hive_agents/src/activity/notification.rs`

- [ ] **Step 1: Write the failing test**

Append to `hive/crates/hive_agents/tests/approval_tests.rs`:

```rust
use hive_agents::activity::notification::{NotificationService, NotificationKind};

#[test]
fn notification_service_push_and_read() {
    let svc = NotificationService::new();
    svc.push(NotificationKind::AgentCompleted, "Agent finished task");
    assert_eq!(svc.unread_count(), 1);

    let items = svc.all();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].summary, "Agent finished task");
}

#[test]
fn notification_mark_read() {
    let svc = NotificationService::new();
    svc.push(NotificationKind::BudgetWarning, "Budget at 80%");
    assert_eq!(svc.unread_count(), 1);

    let items = svc.all();
    svc.mark_read(&items[0].id);
    assert_eq!(svc.unread_count(), 0);
}

#[test]
fn notification_dismiss() {
    let svc = NotificationService::new();
    svc.push(NotificationKind::AgentCompleted, "Done");
    svc.push(NotificationKind::BudgetWarning, "Warning");
    assert_eq!(svc.all().len(), 2);

    let id = svc.all()[0].id.clone();
    svc.dismiss(&id);
    assert_eq!(svc.all().len(), 1);
}
```

- [ ] **Step 2: Run test to verify it fails**

- [ ] **Step 3: Implement NotificationService**

Create `hive/crates/hive_agents/src/activity/notification.rs`:

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use uuid::Uuid;

/// Notification severity/type.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NotificationKind {
    ApprovalRequest,
    BudgetWarning,
    BudgetExhausted,
    AgentCompleted,
    AgentFailed,
    HeartbeatReport,
}

/// A notification item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub kind: NotificationKind,
    pub summary: String,
    pub read: bool,
}

/// Service that manages UI notifications.
pub struct NotificationService {
    items: Mutex<Vec<Notification>>,
    unread: AtomicUsize,
}

impl NotificationService {
    pub fn new() -> Self {
        Self {
            items: Mutex::new(Vec::new()),
            unread: AtomicUsize::new(0),
        }
    }

    /// Push a new notification.
    pub fn push(&self, kind: NotificationKind, summary: &str) {
        let notification = Notification {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            kind,
            summary: summary.into(),
            read: false,
        };
        self.items.lock().unwrap().push(notification);
        self.unread.fetch_add(1, Ordering::SeqCst);
    }

    /// Get all notifications (newest first).
    pub fn all(&self) -> Vec<Notification> {
        let mut items: Vec<_> = self.items.lock().unwrap().clone();
        items.reverse();
        items
    }

    /// Number of unread notifications.
    pub fn unread_count(&self) -> usize {
        self.unread.load(Ordering::SeqCst)
    }

    /// Mark a notification as read.
    pub fn mark_read(&self, id: &str) {
        let mut items = self.items.lock().unwrap();
        if let Some(item) = items.iter_mut().find(|n| n.id == id) {
            if !item.read {
                item.read = true;
                self.unread.fetch_sub(1, Ordering::SeqCst);
            }
        }
    }

    /// Remove a notification.
    pub fn dismiss(&self, id: &str) {
        let mut items = self.items.lock().unwrap();
        if let Some(pos) = items.iter().position(|n| n.id == id) {
            let removed = items.remove(pos);
            if !removed.read {
                self.unread.fetch_sub(1, Ordering::SeqCst);
            }
        }
    }

    /// Clear all notifications.
    pub fn clear(&self) {
        self.items.lock().unwrap().clear();
        self.unread.store(0, Ordering::SeqCst);
    }
}
```

- [ ] **Step 4: Add `pub mod notification;` to activity/mod.rs**

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd hive && cargo test --package hive_agents --test approval_tests notification -- --nocapture 2>&1 | tail -10`
Expected: all 3 notification tests PASS

- [ ] **Step 6: Commit**

```bash
git add hive/crates/hive_agents/src/activity/notification.rs hive/crates/hive_agents/src/activity/mod.rs hive/crates/hive_agents/tests/approval_tests.rs
git commit -m "feat(agents): add NotificationService for UI push notifications"
```

### Task 9: SecurityGateway graduated response

**Files:**
- Modify: `hive/crates/hive_core/src/security.rs`

- [ ] **Step 1: Write the failing test**

Add to the existing tests in `hive/crates/hive_core/src/security.rs`:

```rust
#[test]
fn check_command_graduated_returns_needs_approval_for_risky() {
    let g = gw();
    let result = g.check_command_graduated("echo $(whoami)");
    assert!(matches!(result, SecurityDecision::NeedsApproval(_)));
}

#[test]
fn check_command_graduated_returns_deny_for_dangerous() {
    let g = gw();
    let result = g.check_command_graduated("rm -rf /");
    assert!(matches!(result, SecurityDecision::Deny(_)));
}

#[test]
fn check_command_graduated_returns_allow_for_safe() {
    let g = gw();
    let result = g.check_command_graduated("ls -la");
    assert!(matches!(result, SecurityDecision::Allow));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd hive && cargo test --package hive_core -- check_command_graduated --nocapture 2>&1 | head -20`
Expected: compilation error — `SecurityDecision` and `check_command_graduated` don't exist

- [ ] **Step 3: Add SecurityDecision and graduated check**

In `hive/crates/hive_core/src/security.rs`, add after the `SandboxPolicy` enum:

```rust
/// Graduated security decision — allows routing risky (but not catastrophic)
/// operations through an approval gate instead of hard-blocking.
#[derive(Debug, Clone, PartialEq)]
pub enum SecurityDecision {
    Allow,
    NeedsApproval(String),  // Operation description
    Deny(String),           // Hard block reason
}
```

Then add this method to `impl SecurityGateway`:

```rust
    /// Graduated command check: dangerous commands are Deny, risky patterns
    /// are NeedsApproval, safe commands are Allow.
    pub fn check_command_graduated(&self, command: &str) -> SecurityDecision {
        if self.policy == SandboxPolicy::Sandboxed {
            return SecurityDecision::Allow;
        }

        // Dangerous commands → hard deny (unchanged behavior)
        for pattern in &self.dangerous_commands {
            if pattern.is_match(command) {
                return SecurityDecision::Deny(format!("Blocked dangerous command: {command}"));
            }
        }

        // Risky patterns → soft deny → route to approval
        for pattern in &self.risky_patterns {
            if pattern.is_match(command) {
                return SecurityDecision::NeedsApproval(command.to_string());
            }
        }

        SecurityDecision::Allow
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd hive && cargo test --package hive_core -- check_command_graduated --nocapture 2>&1 | tail -10`
Expected: all 3 tests PASS

- [ ] **Step 5: Verify existing tests still pass**

Run: `cd hive && cargo test --package hive_core -- --nocapture 2>&1 | tail -5`
Expected: all existing security tests PASS (no regressions)

- [ ] **Step 6: Commit**

```bash
git add hive/crates/hive_core/src/security.rs
git commit -m "feat(core): add SecurityDecision graduated response for approval routing"
```

---

## Chunk 5: Heartbeat Scheduler

### Task 10: HeartbeatScheduler

**Files:**
- Create: `hive/crates/hive_agents/src/heartbeat_scheduler.rs`
- Create: `hive/crates/hive_agents/tests/heartbeat_scheduler_tests.rs`

- [ ] **Step 1: Write the failing tests**

Create `hive/crates/hive_agents/tests/heartbeat_scheduler_tests.rs`:

```rust
use hive_agents::heartbeat_scheduler::{HeartbeatMode, HeartbeatScheduler, HeartbeatTask};

#[test]
fn scheduler_add_and_list_tasks() {
    let scheduler = HeartbeatScheduler::new();
    let task = HeartbeatTask {
        id: "hb-1".into(),
        agent_id: "agent-1".into(),
        spec: "refactor error handling".into(),
        interval_secs: 60,
        mode: HeartbeatMode::FixedInterval,
        max_iterations: Some(10),
        paused: false,
        iteration_count: 0,
        last_fired: None,
        total_cost: 0.0,
    };
    scheduler.add(task);
    assert_eq!(scheduler.list().len(), 1);
    assert_eq!(scheduler.list()[0].spec, "refactor error handling");
}

#[test]
fn scheduler_pause_and_resume() {
    let scheduler = HeartbeatScheduler::new();
    scheduler.add(HeartbeatTask {
        id: "hb-1".into(),
        agent_id: "agent-1".into(),
        spec: "test".into(),
        interval_secs: 60,
        mode: HeartbeatMode::FixedInterval,
        max_iterations: None,
        paused: false,
        iteration_count: 0,
        last_fired: None,
        total_cost: 0.0,
    });

    scheduler.pause("hb-1");
    assert!(scheduler.list()[0].paused);

    scheduler.resume("hb-1");
    assert!(!scheduler.list()[0].paused);
}

#[test]
fn scheduler_cancel_removes_task() {
    let scheduler = HeartbeatScheduler::new();
    scheduler.add(HeartbeatTask {
        id: "hb-1".into(),
        agent_id: "agent-1".into(),
        spec: "test".into(),
        interval_secs: 60,
        mode: HeartbeatMode::FixedInterval,
        max_iterations: None,
        paused: false,
        iteration_count: 0,
        last_fired: None,
        total_cost: 0.0,
    });
    assert_eq!(scheduler.list().len(), 1);

    scheduler.cancel("hb-1");
    assert_eq!(scheduler.list().len(), 0);
}

#[test]
fn heartbeat_mode_backoff_doubles_interval() {
    // BackoffOnIdle should double interval, capped at max
    let mut interval = 60u64;
    let max = 600u64;
    let multiplier = 2.0f64;

    interval = ((interval as f64) * multiplier).min(max as f64) as u64;
    assert_eq!(interval, 120);

    interval = ((interval as f64) * multiplier).min(max as f64) as u64;
    assert_eq!(interval, 240);

    interval = ((interval as f64) * multiplier).min(max as f64) as u64;
    assert_eq!(interval, 480);

    interval = ((interval as f64) * multiplier).min(max as f64) as u64;
    assert_eq!(interval, 600); // capped
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd hive && cargo test --package hive_agents --test heartbeat_scheduler_tests -- --nocapture 2>&1 | head -20`
Expected: compilation error — `heartbeat_scheduler` module doesn't exist

- [ ] **Step 3: Implement HeartbeatScheduler**

Create `hive/crates/hive_agents/src/heartbeat_scheduler.rs`:

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;

/// How the heartbeat interval behaves.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum HeartbeatMode {
    /// Fire every N seconds regardless.
    FixedInterval,
    /// Double interval when no work found, reset when work found.
    BackoffOnIdle,
    /// Fire once after delay (deferred execution).
    OneShot,
}

/// A scheduled heartbeat task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatTask {
    pub id: String,
    pub agent_id: String,
    pub spec: String,
    pub interval_secs: u64,
    pub mode: HeartbeatMode,
    pub max_iterations: Option<u32>,
    pub paused: bool,
    pub iteration_count: u32,
    pub last_fired: Option<DateTime<Utc>>,
    pub total_cost: f64,
}

/// Scheduler that manages heartbeat tasks.
///
/// Task management is synchronous (add/pause/cancel). The actual execution
/// loop is spawned as a tokio task when `start()` is called (Phase 4 integration).
pub struct HeartbeatScheduler {
    tasks: Mutex<HashMap<String, HeartbeatTask>>,
}

impl HeartbeatScheduler {
    pub fn new() -> Self {
        Self {
            tasks: Mutex::new(HashMap::new()),
        }
    }

    /// Add a heartbeat task.
    pub fn add(&self, task: HeartbeatTask) {
        self.tasks.lock().unwrap().insert(task.id.clone(), task);
    }

    /// List all tasks.
    pub fn list(&self) -> Vec<HeartbeatTask> {
        self.tasks.lock().unwrap().values().cloned().collect()
    }

    /// Pause a task by ID.
    pub fn pause(&self, id: &str) {
        if let Some(task) = self.tasks.lock().unwrap().get_mut(id) {
            task.paused = true;
        }
    }

    /// Resume a paused task.
    pub fn resume(&self, id: &str) {
        if let Some(task) = self.tasks.lock().unwrap().get_mut(id) {
            task.paused = false;
        }
    }

    /// Cancel and remove a task.
    pub fn cancel(&self, id: &str) {
        self.tasks.lock().unwrap().remove(id);
    }

    /// Record a heartbeat firing.
    pub fn record_fired(&self, id: &str, cost: f64) {
        if let Some(task) = self.tasks.lock().unwrap().get_mut(id) {
            task.last_fired = Some(Utc::now());
            task.iteration_count += 1;
            task.total_cost += cost;
        }
    }

    /// Get a task by ID.
    pub fn get(&self, id: &str) -> Option<HeartbeatTask> {
        self.tasks.lock().unwrap().get(id).cloned()
    }

    /// Check if a task has reached its max iterations.
    pub fn is_complete(&self, id: &str) -> bool {
        self.tasks.lock().unwrap().get(id).map(|t| {
            t.max_iterations.map(|max| t.iteration_count >= max).unwrap_or(false)
        }).unwrap_or(true)
    }
}

impl Default for HeartbeatScheduler {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **Step 4: Add `pub mod heartbeat_scheduler;` to lib.rs**

In `hive/crates/hive_agents/src/lib.rs`, add:

```rust
pub mod heartbeat_scheduler;
```

And in re-exports:

```rust
pub use heartbeat_scheduler::{HeartbeatMode, HeartbeatScheduler, HeartbeatTask};
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd hive && cargo test --package hive_agents --test heartbeat_scheduler_tests -- --nocapture 2>&1 | tail -10`
Expected: all 4 tests PASS

- [ ] **Step 6: Commit**

```bash
git add hive/crates/hive_agents/src/heartbeat_scheduler.rs hive/crates/hive_agents/src/lib.rs hive/crates/hive_agents/tests/heartbeat_scheduler_tests.rs
git commit -m "feat(agents): add HeartbeatScheduler for background task execution"
```

---

## Chunk 6: UI — Activity Panel + Notification Tray + Budget Gauge + Progressive Disclosure

### Task 11: Register Activity panel in sidebar

**Files:**
- Modify: `hive/crates/hive_ui_core/src/sidebar.rs`
- Modify: `hive/crates/hive_ui_core/src/actions.rs`

- [ ] **Step 1: Add `Panel::Activity` variant to sidebar.rs**

In the `Panel` enum, add `Activity` after `Monitor`:

```rust
    Monitor,
    Activity,  // NEW
    Logs,
```

Update `Panel::ALL` array to include `Panel::Activity` (now 27 items), update the count comment.

Add match arms in `label()`:
```rust
Self::Activity => "Activity",
```

Add match arm in `icon()`:
```rust
Self::Activity => IconName::Inbox, // Activity feed icon
```

Add match arm in `from_stored()`:
```rust
"Activity" => Self::Activity,
```

Add match arm in `to_stored()`:
```rust
Self::Activity => "Activity",
```

- [ ] **Step 2: Add actions in actions.rs**

In the `actions!` macro, add:

```rust
SwitchToActivity,
// Activity panel
ActivityRefresh,
ActivityExportCsv,
```

Add data-carrying actions after the existing ones:

```rust
/// Set activity filter by category.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct ActivitySetFilter {
    pub categories: String, // comma-separated
}

/// Approve an approval request by ID.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct ActivityApprove {
    pub request_id: String,
}

/// Deny an approval request by ID.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct ActivityDeny {
    pub request_id: String,
    pub reason: String,
}

/// Toggle event detail disclosure.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct ActivityExpandEvent {
    pub event_id: String,
}
```

- [ ] **Step 3: Verify it compiles**

Run: `cd hive && cargo check --package hive_ui_core 2>&1 | tail -10`
Expected: compiles (warnings OK)

- [ ] **Step 4: Commit**

```bash
git add hive/crates/hive_ui_core/src/sidebar.rs hive/crates/hive_ui_core/src/actions.rs
git commit -m "feat(ui): register Activity panel and approval actions"
```

### Task 12: Activity panel rendering

**Files:**
- Create: `hive/crates/hive_ui_panels/src/panels/activity.rs`
- Modify: `hive/crates/hive_ui_panels/src/panels/mod.rs`

- [ ] **Step 1: Create the activity panel**

Create `hive/crates/hive_ui_panels/src/panels/activity.rs` following the existing panel patterns (data struct + static render methods). The panel should display:

- Filter bar with category toggle pills
- Pinned approval requests section (if any pending)
- Reverse-chronological event stream with category icons, summaries, timestamps
- Expandable event details

Data struct:

```rust
use hive_agents::activity::log::{ActivityEntry, ActivityFilter};
use hive_agents::activity::approval::ApprovalRequest;
use hive_agents::activity::log::CostSummary;
use std::collections::HashSet;

pub struct ActivityData {
    pub entries: Vec<ActivityEntry>,
    pub filter: ActivityFilter,
    pub pending_approvals: Vec<ApprovalRequest>,
    pub cost_summary: CostSummary,
    pub expanded_events: HashSet<i64>,
    pub search_query: String,
}

impl Default for ActivityData {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            filter: ActivityFilter::default(),
            pending_approvals: Vec::new(),
            cost_summary: CostSummary::default(),
            expanded_events: HashSet::new(),
            search_query: String::new(),
        }
    }
}
```

Render implementation follows the exact pattern from `logs.rs` — header with filter controls, scrollable list of entries, each entry is a row with icon + summary + timestamp.

- [ ] **Step 2: Register in panels/mod.rs**

Add `pub mod activity;` to `hive/crates/hive_ui_panels/src/panels/mod.rs`.

- [ ] **Step 3: Verify it compiles**

Run: `cd hive && cargo check --package hive_ui_panels 2>&1 | tail -10`
Expected: compiles

- [ ] **Step 4: Commit**

```bash
git add hive/crates/hive_ui_panels/src/panels/activity.rs hive/crates/hive_ui_panels/src/panels/mod.rs
git commit -m "feat(ui): add Activity panel with event stream and approval display"
```

### Task 13: Notification tray component

**Files:**
- Create: `hive/crates/hive_ui_panels/src/components/notification_tray.rs`
- Modify: `hive/crates/hive_ui_panels/src/components/mod.rs`

- [ ] **Step 1: Create the notification tray component**

This renders as a bell icon with badge count in the statusbar. On click, shows a dropdown with recent notifications and inline Approve/Deny buttons.

- [ ] **Step 2: Register in components/mod.rs**

Add `pub mod notification_tray;`

- [ ] **Step 3: Verify it compiles**

Run: `cd hive && cargo check --package hive_ui_panels 2>&1 | tail -10`

- [ ] **Step 4: Commit**

```bash
git add hive/crates/hive_ui_panels/src/components/notification_tray.rs hive/crates/hive_ui_panels/src/components/mod.rs
git commit -m "feat(ui): add notification tray component for statusbar"
```

### Task 14: Budget gauge component

**Files:**
- Create: `hive/crates/hive_ui_panels/src/components/budget_gauge.rs`
- Modify: `hive/crates/hive_ui_panels/src/components/mod.rs`
- Modify: `hive/crates/hive_ui_panels/src/panels/costs.rs`

- [ ] **Step 1: Create the budget gauge component**

Horizontal bar showing current spend vs daily limit with amber/red segments.

- [ ] **Step 2: Add to Costs panel**

In `costs.rs`, render the budget gauge above the model breakdown table.

- [ ] **Step 3: Register in components/mod.rs**

Add `pub mod budget_gauge;`

- [ ] **Step 4: Verify it compiles**

Run: `cd hive && cargo check --package hive_ui_panels 2>&1 | tail -10`

- [ ] **Step 5: Commit**

```bash
git add hive/crates/hive_ui_panels/src/components/budget_gauge.rs hive/crates/hive_ui_panels/src/components/mod.rs hive/crates/hive_ui_panels/src/panels/costs.rs
git commit -m "feat(ui): add budget gauge to Costs panel"
```

### Task 15: Progressive disclosure in chat/agents panels

**Files:**
- Modify: `hive/crates/hive_ui_panels/src/panels/chat.rs`
- Modify: `hive/crates/hive_ui_panels/src/panels/agents.rs`

- [ ] **Step 1: Add DisclosureLevel enum**

In `chat.rs`, add:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum DisclosureLevel {
    #[default]
    Summary,
    Steps,
    Raw,
}

impl DisclosureLevel {
    pub fn next(self) -> Self {
        match self {
            Self::Summary => Self::Steps,
            Self::Steps => Self::Raw,
            Self::Raw => Self::Summary,
        }
    }
}
```

Add `pub disclosure: DisclosureLevel` field to `DisplayMessage`.

- [ ] **Step 2: Add DisclosureLevel to RunDisplay in agents.rs**

Add `pub disclosure: DisclosureLevel` field to `RunDisplay`.

- [ ] **Step 3: Verify it compiles**

Run: `cd hive && cargo check --package hive_ui_panels 2>&1 | tail -10`

- [ ] **Step 4: Commit**

```bash
git add hive/crates/hive_ui_panels/src/panels/chat.rs hive/crates/hive_ui_panels/src/panels/agents.rs
git commit -m "feat(ui): add progressive disclosure levels to chat and agents panels"
```

---

## Chunk 7: Wire Everything Into Workspace

### Task 16: Wire Activity panel + notification tray into workspace

**Files:**
- Modify: `hive/crates/hive_ui/src/workspace.rs`

- [ ] **Step 1: Add ActivityData field to HiveWorkspace**

Add `activity_data: ActivityData` field alongside the other panel data fields.

- [ ] **Step 2: Add render case for Activity panel**

In `render_active_panel()`, add the match arm:

```rust
Panel::Activity => ActivityPanel::render(&self.activity_data, &self.theme),
```

- [ ] **Step 3: Add switch handler**

In `switch_to_panel()`, add the `Panel::Activity` case that lazy-loads activity data from the ActivityLog on first visit.

- [ ] **Step 4: Wire notification tray into statusbar**

In the statusbar rendering, add the notification bell icon with badge count.

- [ ] **Step 5: Register action handlers**

Register handlers for `SwitchToActivity`, `ActivityRefresh`, `ActivityApprove`, `ActivityDeny`, `ActivityExpandEvent`, `ActivityExportCsv`, `ActivitySetFilter`.

- [ ] **Step 6: Verify it compiles**

Run: `cd hive && cargo check --package hive_ui 2>&1 | tail -10`

- [ ] **Step 7: Commit**

```bash
git add hive/crates/hive_ui/src/workspace.rs
git commit -m "feat(ui): wire Activity panel, notification tray, and approval handlers into workspace"
```

### Task 17: Final integration — wire event bus into orchestration

**Key design decisions (from spec review):**
- **No duplicate events:** ActivityService bridges from Coordinator's existing `TaskEvent` broadcast — no changes to Coordinator/HiveMind/Queen for task lifecycle events.
- **Budget check in orchestrators, not AiService:** Avoids circular dependency (`hive_agents` → `hive_ai`, not the reverse). Coordinator calls `budget.check()` before dispatching AI calls.
- **HeartbeatScheduler calls `coordinator.execute()`** (not `execute_plan()`) since it takes a string spec.

**Files:**
- Modify: `hive/crates/hive_agents/src/activity/mod.rs`
- Modify: `hive/crates/hive_agents/src/coordinator.rs`
- Modify: `hive/crates/hive_agents/src/hivemind.rs`
- Modify: `hive/crates/hive_agents/src/queen.rs`

- [ ] **Step 1: Add TaskEvent → ActivityEvent bridge in ActivityService**

In `activity/mod.rs`, add a `bridge_task_events()` method that subscribes to the Coordinator's existing `TaskEvent` broadcast and translates events into `ActivityEvent`s. This means the Coordinator emits events once (as `TaskEvent`), and the bridge produces the corresponding `ActivityEvent`s automatically.

```rust
impl ActivityService {
    pub fn bridge_task_events(&self, mut task_rx: broadcast::Receiver<coordinator::TaskEvent>) {
        let tx = self.tx.clone();
        tokio::spawn(async move {
            while let Ok(task_event) = task_rx.recv().await {
                if let Some(activity_event) = translate_task_event(task_event) {
                    let _ = tx.send(activity_event);
                }
            }
        });
    }
}
```

- [ ] **Step 2: Add budget pre-flight check to Coordinator and Queen**

Add `budget: Option<Arc<BudgetEnforcer>>` field to `CoordinatorConfig` and `SwarmConfig`. Before each task's AI call in the execution loop, check the budget. This does NOT touch `hive_ai` at all — the check happens in `hive_agents`.

- [ ] **Step 3: Add ActivityService to HiveMind for cost events**

Only `CostIncurred` events need explicit emission (task lifecycle is handled by the bridge). Add `activity: Option<Arc<ActivityService>>` and emit `CostIncurred` after each role's AI response returns.

- [ ] **Step 4: Wire bridge in Queen after creating Coordinator**

When Queen creates a Coordinator for a team, pass the Coordinator's `subscribe()` receiver to `activity.bridge_task_events()`.

- [ ] **Step 5: Run full test suite**

Run: `cd hive && cargo test --workspace --exclude hive_app 2>&1 | tail -20`
Expected: all tests pass (no regressions)

- [ ] **Step 6: Commit**

```bash
git add hive/crates/hive_agents/src/activity/mod.rs hive/crates/hive_agents/src/coordinator.rs hive/crates/hive_agents/src/hivemind.rs hive/crates/hive_agents/src/queen.rs
git commit -m "feat: wire event bus bridge and budget checks into orchestration layer"
```

### Task 18: Update lib.rs re-exports and final cleanup

**Files:**
- Modify: `hive/crates/hive_agents/src/lib.rs`
- Modify: `hive/crates/hive_agents/src/activity/mod.rs`

- [ ] **Step 1: Add all re-exports to activity/mod.rs**

```rust
pub mod approval;
pub mod budget;
pub mod log;
pub mod notification;
pub mod rules;
pub mod types;

pub use types::{ActivityEvent, FileOp, OperationType, PauseReason};
pub use log::{ActivityLog, ActivityEntry, ActivityFilter, CostSummary};
pub use budget::{BudgetConfig, BudgetDecision, BudgetEnforcer, ExhaustAction};
pub use approval::{ApprovalGate, ApprovalDecision, ApprovalRequest};
pub use notification::{NotificationService, NotificationKind, Notification};
pub use rules::{ApprovalRule, RuleTrigger};
```

- [ ] **Step 2: Update hive_agents/src/lib.rs re-exports**

Add comprehensive re-exports for the new public API.

- [ ] **Step 3: Run full test suite**

Run: `cd hive && cargo test --workspace --exclude hive_app 2>&1 | tail -20`
Expected: all tests pass

- [ ] **Step 4: Commit**

```bash
git add hive/crates/hive_agents/src/lib.rs hive/crates/hive_agents/src/activity/mod.rs
git commit -m "chore(agents): finalize activity module re-exports"
```
