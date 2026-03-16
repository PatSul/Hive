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
