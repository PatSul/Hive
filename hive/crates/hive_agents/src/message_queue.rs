//! Steering vs. follow-up message queue for agent interruption.
//!
//! Users can inject messages while an agent is mid-execution:
//! - **Steering** messages interrupt immediately — they're drained between tool
//!   rounds and prepended as high-priority context.
//! - **Follow-up** messages queue up — they're drained at iteration boundaries
//!   or after the current agent finishes.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Priority classification for user-injected messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessagePriority {
    /// High-priority interrupt — processed between tool rounds.
    Steering,
    /// Normal priority — queued until the current phase completes.
    FollowUp,
}

/// A user message injected while an agent is running.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMessage {
    pub content: String,
    pub priority: MessagePriority,
    pub timestamp: DateTime<Utc>,
}

impl AgentMessage {
    /// Create a new steering message.
    pub fn steering(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            priority: MessagePriority::Steering,
            timestamp: Utc::now(),
        }
    }

    /// Create a new follow-up message.
    pub fn follow_up(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            priority: MessagePriority::FollowUp,
            timestamp: Utc::now(),
        }
    }
}

// ---------------------------------------------------------------------------
// Queue
// ---------------------------------------------------------------------------

/// Thread-safe message queue with separate steering and follow-up lanes.
///
/// Steering messages are drained first and more frequently (between tool
/// rounds), while follow-up messages are drained at coarser boundaries
/// (iteration or role transitions).
#[derive(Debug, Default)]
pub struct AgentMessageQueue {
    steering: VecDeque<AgentMessage>,
    followup: VecDeque<AgentMessage>,
}

impl AgentMessageQueue {
    pub fn new() -> Self {
        Self::default()
    }

    /// Push a message into the appropriate lane based on its priority.
    pub fn push(&mut self, msg: AgentMessage) {
        match msg.priority {
            MessagePriority::Steering => self.steering.push_back(msg),
            MessagePriority::FollowUp => self.followup.push_back(msg),
        }
    }

    /// Push a steering message by content string.
    pub fn push_steering(&mut self, content: impl Into<String>) {
        self.steering.push_back(AgentMessage::steering(content));
    }

    /// Push a follow-up message by content string.
    pub fn push_follow_up(&mut self, content: impl Into<String>) {
        self.followup.push_back(AgentMessage::follow_up(content));
    }

    /// Drain all pending steering messages (FIFO order).
    pub fn drain_steering(&mut self) -> Vec<AgentMessage> {
        self.steering.drain(..).collect()
    }

    /// Drain all pending follow-up messages (FIFO order).
    pub fn drain_followup(&mut self) -> Vec<AgentMessage> {
        self.followup.drain(..).collect()
    }

    /// Drain both lanes — steering first, then follow-up.
    pub fn drain_all(&mut self) -> Vec<AgentMessage> {
        let mut all = self.drain_steering();
        all.extend(self.drain_followup());
        all
    }

    /// Whether there are any pending steering messages.
    pub fn has_steering(&self) -> bool {
        !self.steering.is_empty()
    }

    /// Whether there are any pending follow-up messages.
    pub fn has_followup(&self) -> bool {
        !self.followup.is_empty()
    }

    /// Whether the queue has any messages at all.
    pub fn is_empty(&self) -> bool {
        self.steering.is_empty() && self.followup.is_empty()
    }

    /// Total number of queued messages across both lanes.
    pub fn len(&self) -> usize {
        self.steering.len() + self.followup.len()
    }

    /// Number of pending steering messages.
    pub fn steering_count(&self) -> usize {
        self.steering.len()
    }

    /// Number of pending follow-up messages.
    pub fn followup_count(&self) -> usize {
        self.followup.len()
    }
}

// ---------------------------------------------------------------------------
// Shared handle
// ---------------------------------------------------------------------------

/// A clonable, thread-safe handle to an `AgentMessageQueue`.
///
/// This is the type that gets shared between the UI thread (producer)
/// and the agent execution thread (consumer).
pub type SharedMessageQueue = Arc<Mutex<AgentMessageQueue>>;

/// Create a new shared message queue.
pub fn shared_queue() -> SharedMessageQueue {
    Arc::new(Mutex::new(AgentMessageQueue::new()))
}

// ---------------------------------------------------------------------------
// Classify helper
// ---------------------------------------------------------------------------

/// Classify a user input as steering or follow-up.
///
/// Steering messages are prefixed with `!` or `/steer`.
/// Everything else is a follow-up.
pub fn classify_input(input: &str) -> MessagePriority {
    let trimmed = input.trim();
    if trimmed.starts_with('!') || trimmed.starts_with("/steer") {
        MessagePriority::Steering
    } else {
        MessagePriority::FollowUp
    }
}

/// Strip the steering prefix from user input, returning the clean content.
pub fn strip_prefix(input: &str) -> &str {
    let trimmed = input.trim();
    if let Some(rest) = trimmed.strip_prefix("/steer") {
        rest.trim()
    } else if let Some(rest) = trimmed.strip_prefix('!') {
        rest.trim()
    } else {
        trimmed
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_and_drain_steering() {
        let mut queue = AgentMessageQueue::new();
        queue.push_steering("stop doing that");
        queue.push_steering("focus on the API");

        assert!(queue.has_steering());
        assert_eq!(queue.steering_count(), 2);

        let msgs = queue.drain_steering();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].content, "stop doing that");
        assert_eq!(msgs[1].content, "focus on the API");
        assert!(!queue.has_steering());
    }

    #[test]
    fn push_and_drain_followup() {
        let mut queue = AgentMessageQueue::new();
        queue.push_follow_up("also handle edge case X");
        queue.push_follow_up("add logging too");

        assert!(queue.has_followup());
        assert_eq!(queue.followup_count(), 2);

        let msgs = queue.drain_followup();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].content, "also handle edge case X");
        assert!(!queue.has_followup());
    }

    #[test]
    fn drain_all_steering_first() {
        let mut queue = AgentMessageQueue::new();
        queue.push_follow_up("followup 1");
        queue.push_steering("steering 1");
        queue.push_follow_up("followup 2");
        queue.push_steering("steering 2");

        let all = queue.drain_all();
        assert_eq!(all.len(), 4);
        // Steering comes first
        assert_eq!(all[0].priority, MessagePriority::Steering);
        assert_eq!(all[1].priority, MessagePriority::Steering);
        assert_eq!(all[2].priority, MessagePriority::FollowUp);
        assert_eq!(all[3].priority, MessagePriority::FollowUp);
        assert!(queue.is_empty());
    }

    #[test]
    fn push_by_priority_enum() {
        let mut queue = AgentMessageQueue::new();
        queue.push(AgentMessage::steering("steer"));
        queue.push(AgentMessage::follow_up("later"));

        assert_eq!(queue.len(), 2);
        assert_eq!(queue.steering_count(), 1);
        assert_eq!(queue.followup_count(), 1);
    }

    #[test]
    fn empty_queue() {
        let queue = AgentMessageQueue::new();
        assert!(queue.is_empty());
        assert!(!queue.has_steering());
        assert!(!queue.has_followup());
        assert_eq!(queue.len(), 0);
    }

    #[test]
    fn classify_steering_prefix() {
        assert_eq!(classify_input("!stop"), MessagePriority::Steering);
        assert_eq!(classify_input("! change direction"), MessagePriority::Steering);
        assert_eq!(classify_input("/steer focus on X"), MessagePriority::Steering);
        assert_eq!(classify_input("  !urgent"), MessagePriority::Steering);
    }

    #[test]
    fn classify_followup() {
        assert_eq!(classify_input("also do Y"), MessagePriority::FollowUp);
        assert_eq!(classify_input("add tests"), MessagePriority::FollowUp);
        assert_eq!(classify_input(""), MessagePriority::FollowUp);
    }

    #[test]
    fn strip_steering_prefix() {
        assert_eq!(strip_prefix("!stop now"), "stop now");
        assert_eq!(strip_prefix("/steer focus on X"), "focus on X");
        assert_eq!(strip_prefix("  ! urgent  "), "urgent");
        assert_eq!(strip_prefix("normal text"), "normal text");
    }

    #[test]
    fn shared_queue_across_threads() {
        let queue = shared_queue();
        let queue2 = queue.clone();

        // Simulate producer
        {
            let mut q = queue.lock().unwrap();
            q.push_steering("interrupt!");
            q.push_follow_up("also this");
        }

        // Simulate consumer
        {
            let mut q = queue2.lock().unwrap();
            assert!(q.has_steering());
            let steering = q.drain_steering();
            assert_eq!(steering.len(), 1);
            assert_eq!(steering[0].content, "interrupt!");
        }
    }

    #[test]
    fn message_serialization() {
        let msg = AgentMessage::steering("test message");
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: AgentMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.content, "test message");
        assert_eq!(parsed.priority, MessagePriority::Steering);
    }

    #[test]
    fn drain_is_idempotent() {
        let mut queue = AgentMessageQueue::new();
        queue.push_steering("one");

        let first = queue.drain_steering();
        assert_eq!(first.len(), 1);

        let second = queue.drain_steering();
        assert!(second.is_empty());
    }
}
