pub mod approval;
pub mod budget;
pub mod log;
pub mod notification;
pub mod rules;
pub mod types;

use std::sync::Arc;
use tokio::sync::broadcast;
pub use types::{ActivityEvent, FileOp, OperationType, PauseReason};
pub use log::{ActivityLog, ActivityEntry, ActivityFilter, CostSummary};
pub use budget::{BudgetConfig, BudgetDecision, BudgetEnforcer, ExhaustAction};
pub use approval::{ApprovalGate, ApprovalDecision, ApprovalRequest};
pub use notification::{NotificationService, NotificationKind, Notification};
pub use rules::{ApprovalRule, RuleTrigger};

use crate::coordinator::TaskEvent;

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

    /// Emit an event to all listeners. Fire-and-forget.
    pub fn emit(&self, event: ActivityEvent) {
        // Ignore send errors (no receivers = that's fine)
        let _ = self.tx.send(event);
    }

    /// Subscribe to the event stream.
    pub fn subscribe(&self) -> broadcast::Receiver<ActivityEvent> {
        self.tx.subscribe()
    }

    /// Bridge coordinator TaskEvents into ActivityEvents.
    ///
    /// Call after creating a Coordinator:
    /// `activity.bridge_task_events(coordinator.subscribe())`
    pub fn bridge_task_events(&self, mut task_rx: broadcast::Receiver<TaskEvent>) {
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

fn translate_task_event(event: TaskEvent) -> Option<ActivityEvent> {
    match event {
        TaskEvent::TaskStarted { task_id, persona, .. } => {
            Some(ActivityEvent::AgentStarted {
                agent_id: persona,
                role: "task".into(),
                task_id: Some(task_id),
            })
        }
        TaskEvent::TaskCompleted { task_id, cost, .. } => {
            Some(ActivityEvent::TaskCompleted {
                task_id,
                agent_id: "coordinator".into(),
                cost,
            })
        }
        TaskEvent::TaskFailed { task_id, error } => {
            Some(ActivityEvent::TaskFailed {
                task_id,
                error,
            })
        }
        _ => None,
    }
}
