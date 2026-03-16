use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NotificationKind {
    ApprovalRequest,
    BudgetWarning,
    BudgetExhausted,
    AgentCompleted,
    AgentFailed,
    HeartbeatReport,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub kind: NotificationKind,
    pub summary: String,
    pub read: bool,
}

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

    pub fn all(&self) -> Vec<Notification> {
        let mut items: Vec<_> = self.items.lock().unwrap().clone();
        items.reverse();
        items
    }

    pub fn unread_count(&self) -> usize {
        self.unread.load(Ordering::SeqCst)
    }

    pub fn mark_read(&self, id: &str) {
        let mut items = self.items.lock().unwrap();
        if let Some(item) = items.iter_mut().find(|n| n.id == id) {
            if !item.read {
                item.read = true;
                self.unread.fetch_sub(1, Ordering::SeqCst);
            }
        }
    }

    pub fn dismiss(&self, id: &str) {
        let mut items = self.items.lock().unwrap();
        if let Some(pos) = items.iter().position(|n| n.id == id) {
            let removed = items.remove(pos);
            if !removed.read {
                self.unread.fetch_sub(1, Ordering::SeqCst);
            }
        }
    }

    pub fn clear(&self) {
        self.items.lock().unwrap().clear();
        self.unread.store(0, Ordering::SeqCst);
    }
}
