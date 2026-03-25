//! SSE event types emitted by Kilo's `/session/{id}/event` stream.
//!
//! Kilo uses Server-Sent Events for real-time streaming.  Each event is a
//! JSON object with a `"type"` discriminant field.  This module defines the
//! full event hierarchy plus utilities for converting a raw SSE line into a
//! [`KiloEvent`].
//!
//! Events that have no equivalent in Hive's [`hive_ai::types::StreamChunk`]
//! (e.g. [`KiloEvent::FileChange`]) are forwarded via a separate
//! `KiloFileEvent` callback so callers can surface them in the UI without
//! losing them.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Usage stats reported in the Done event
// ---------------------------------------------------------------------------

/// Token-usage statistics included in the final `done` event.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct KiloUsage {
    /// Tokens in the prompt / input.
    #[serde(default)]
    pub input_tokens: u32,
    /// Tokens in the completion / output.
    #[serde(default)]
    pub output_tokens: u32,
    /// Cache read tokens (if the underlying model supports it).
    #[serde(default)]
    pub cache_read_tokens: Option<u32>,
    /// Cache write tokens (if the underlying model supports it).
    #[serde(default)]
    pub cache_write_tokens: Option<u32>,
}

impl KiloUsage {
    pub fn total_tokens(&self) -> u32 {
        self.input_tokens + self.output_tokens
    }
}

// ---------------------------------------------------------------------------
// File change events
// ---------------------------------------------------------------------------

/// What happened to the file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileChangeKind {
    Created,
    Modified,
    Deleted,
    Renamed,
}

/// A file that was created, modified, or deleted by the coding agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChangeDelta {
    /// Workspace-relative path to the affected file.
    pub path: String,
    /// What happened.
    pub kind: FileChangeKind,
    /// Unified diff of the change, when available.
    #[serde(default)]
    pub diff: Option<String>,
}

// ---------------------------------------------------------------------------
// Tool events
// ---------------------------------------------------------------------------

/// An in-flight tool call emitted by the agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KiloToolCall {
    /// Unique ID for correlating with [`KiloEvent::ToolResult`].
    pub id: String,
    /// Name of the tool (e.g. `"read_file"`, `"bash"`).
    pub name: String,
    /// Arguments supplied to the tool, as a JSON value.
    pub input: serde_json::Value,
}

/// The result of a completed tool call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KiloToolResult {
    /// Matches the `id` on the corresponding [`KiloEvent::ToolCall`].
    pub id: String,
    /// Whether the tool call completed successfully.
    pub is_error: bool,
    /// The tool's output (text, JSON, or error message).
    pub output: serde_json::Value,
}

// ---------------------------------------------------------------------------
// Top-level event enum
// ---------------------------------------------------------------------------

/// A single event from a Kilo session SSE stream.
///
/// Deserialised with `#[serde(tag = "type", rename_all = "snake_case")]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum KiloEvent {
    /// Incremental text token from the model.
    TextDelta {
        /// The new text content fragment.
        content: String,
    },

    /// Incremental extended thinking / chain-of-thought content.
    ThinkingDelta {
        /// The new thinking content fragment.
        content: String,
    },

    /// The model is requesting a tool call.
    ToolCall(KiloToolCall),

    /// A tool call has completed and the result is being fed back.
    ToolResult(KiloToolResult),

    /// The agent created, modified, or deleted a file in the workspace.
    FileChange(FileChangeDelta),

    /// The agent's turn is complete.
    Done {
        /// Why the model stopped generating.
        #[serde(default)]
        stop_reason: Option<String>,
        /// Token usage for the completed turn.
        #[serde(default)]
        usage: Option<KiloUsage>,
    },

    /// The server encountered an error mid-stream.
    Error {
        /// Human-readable error description.
        message: String,
        /// Optional machine-readable error code.
        #[serde(default)]
        code: Option<String>,
    },
}

// ---------------------------------------------------------------------------
// SSE line parser
// ---------------------------------------------------------------------------

/// Parse a raw `data: ...` SSE line into a [`KiloEvent`].
///
/// The caller is responsible for stripping the leading `"data: "` prefix
/// before calling this function.  Returns `None` when the line is a keep-alive
/// comment (starting with `:`) or blank.
pub fn parse_sse_line(line: &str) -> Option<Result<KiloEvent, serde_json::Error>> {
    let line = line.trim();
    if line.is_empty() || line.starts_with(':') {
        return None;
    }
    // Strip "data: " prefix if present.
    let json = line.strip_prefix("data: ").unwrap_or(line);
    if json == "[DONE]" {
        // OpenAI-style terminator — treat as a Done event with no usage.
        return Some(Ok(KiloEvent::Done {
            stop_reason: None,
            usage: None,
        }));
    }
    Some(serde_json::from_str(json))
}
