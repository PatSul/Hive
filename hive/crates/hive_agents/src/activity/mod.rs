pub mod budget;
pub mod log;
pub mod rules;
pub mod types;

use std::sync::Arc;
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
}
