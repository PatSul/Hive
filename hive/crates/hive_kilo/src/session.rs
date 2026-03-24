//! Kilo session types and lifecycle management.
//!
//! A Kilo *session* is a long-lived stateful conversation that can hold open
//! file handles, shell processes, and MCP server connections.  Sessions are
//! forkable — `POST /session/{id}/fork` creates a copy that inherits the
//! parent's workspace context.
//!
//! This module defines the wire-format types returned by the REST API plus the
//! [`KiloSession`] handle used throughout the crate.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Wire types — Kilo REST API JSON shapes
// ---------------------------------------------------------------------------

/// Request body for `POST /session`.
#[derive(Debug, Clone, Serialize)]
pub struct CreateSessionRequest {
    /// The model to use (e.g. `"anthropic/claude-opus-4-5"` or just
    /// `"claude-opus-4-5"` depending on the Kilo version).
    /// `None` lets Kilo pick its configured default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    /// Optional system prompt injected before the conversation begins.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,

    /// Maximum tokens the model may generate per turn.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,

    /// Workspace root directory the agent should operate in.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
}

impl Default for CreateSessionRequest {
    fn default() -> Self {
        Self {
            model: None,
            system: None,
            max_tokens: None,
            workspace: None,
        }
    }
}

/// A single message in a Kilo session chat exchange.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KiloMessage {
    /// Role of the message sender (`"user"` or `"assistant"`).
    pub role: String,
    /// Text content of the message.
    pub content: String,
}

impl KiloMessage {
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".into(),
            content: content.into(),
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".into(),
            content: content.into(),
        }
    }
}

/// The status of a Kilo session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    /// Idle and ready to accept new messages.
    Idle,
    /// Currently generating a response.
    Running,
    /// Waiting for a tool result.
    ToolWait,
    /// Session has been closed.
    Closed,
}

/// The JSON representation of a session returned by `POST /session` and
/// `GET /session/{id}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    /// Unique session identifier (UUID or similar).
    pub id: String,
    /// Model being used by this session.
    #[serde(default)]
    pub model: Option<String>,
    /// Current session lifecycle state.
    #[serde(default)]
    pub status: Option<SessionStatus>,
    /// ISO-8601 creation timestamp.
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
    /// If this is a forked session, the ID of its parent.
    #[serde(default)]
    pub parent_id: Option<String>,
}

// ---------------------------------------------------------------------------
// KiloSession handle
// ---------------------------------------------------------------------------

/// An active Kilo session handle.
///
/// Created by [`crate::client::KiloClient::create_session`] or
/// [`crate::client::KiloClient::fork_session`].  Holds the session ID that
/// must be passed to subsequent API calls.
///
/// **Dropping a `KiloSession` does *not* close the remote session** — you must
/// call [`crate::client::KiloClient::close_session`] explicitly, or use a
/// session pool that handles cleanup automatically.
#[derive(Debug, Clone)]
pub struct KiloSession {
    /// Unique ID assigned by Kilo when the session was created.
    pub id: String,
    /// Model configured for this session (may be `None` if Kilo picks it).
    pub model: Option<String>,
    /// When the session was created.
    pub created_at: DateTime<Utc>,
    /// Parent session ID (set when this is a fork).
    pub parent_id: Option<String>,
}

impl From<SessionInfo> for KiloSession {
    fn from(info: SessionInfo) -> Self {
        Self {
            id: info.id,
            model: info.model,
            created_at: info.created_at.unwrap_or_else(Utc::now),
            parent_id: info.parent_id,
        }
    }
}
