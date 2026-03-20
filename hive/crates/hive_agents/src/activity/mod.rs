pub mod approval;
pub mod budget;
pub mod log;
pub mod notification;
pub mod rules;
pub mod types;

pub use approval::{ApprovalDecision, ApprovalGate, ApprovalRequest};
pub use budget::{BudgetConfig, BudgetDecision, BudgetEnforcer, ExhaustAction};
pub use log::{ActivityEntry, ActivityFilter, ActivityLog, CostSummary};
pub use notification::{Notification, NotificationKind, NotificationService};
pub use rules::{ApprovalRule, RuleTrigger};
use std::sync::Arc;
use tokio::sync::broadcast;
pub use types::{ActivityEvent, FileOp, OperationType, PauseReason};

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

        // Persist activity on an available Tokio runtime, or spin up a small
        // dedicated runtime when desktop startup has not entered one yet.
        let log_ref = log.clone();
        spawn_background(async move {
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
        spawn_background(async move {
            while let Ok(task_event) = task_rx.recv().await {
                if let Some(activity_event) = translate_task_event(task_event) {
                    let _ = tx.send(activity_event);
                }
            }
        });
    }
}

fn spawn_background<F>(future: F)
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn(future);
    } else {
        std::thread::spawn(move || match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime.block_on(future),
            Err(error) => tracing::warn!("ActivityService runtime bootstrap failed: {error}"),
        });
    }
}

fn translate_task_event(event: TaskEvent) -> Option<ActivityEvent> {
    match event {
        TaskEvent::TaskStarted {
            task_id, persona, ..
        } => Some(ActivityEvent::AgentStarted {
            agent_id: persona,
            role: "task".into(),
            task_id: Some(task_id),
        }),
        TaskEvent::TaskCompleted { task_id, cost, .. } => Some(ActivityEvent::TaskCompleted {
            task_id,
            agent_id: "coordinator".into(),
            cost,
        }),
        TaskEvent::TaskFailed { task_id, error } => {
            Some(ActivityEvent::TaskFailed { task_id, error })
        }
        TaskEvent::TaskApprovalPending {
            request_id,
            task_id,
            operation,
            rule,
        } => Some(ActivityEvent::ApprovalRequested {
            request_id,
            agent_id: "coordinator".into(),
            operation,
            context: task_id,
            rule,
        }),
        TaskEvent::TaskDenied { task_id, reason } => Some(ActivityEvent::ApprovalDenied {
            request_id: task_id,
            reason,
        }),
        _ => None,
    }
}
