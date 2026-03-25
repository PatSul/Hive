//! Error types for the Kilo integration.

use thiserror::Error;

/// All errors that can be returned by the Kilo client and its bridges.
#[derive(Debug, Error)]
pub enum KiloError {
    /// The Kilo daemon is not reachable (not installed or not running).
    #[error("Kilo is not available at {url}: {source}")]
    Unavailable { url: String, source: reqwest::Error },

    /// HTTP transport error (network timeout, connection reset, etc.).
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    /// The server returned a non-2xx status code.
    #[error("Kilo API error: HTTP {status} — {body}")]
    ApiError { status: u16, body: String },

    /// A response body could not be parsed as expected JSON.
    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),

    /// The SSE stream ended unexpectedly (no `done` event received).
    #[error("SSE stream ended without a done event")]
    StreamInterrupted,

    /// A Kilo session ID was used after it had already been closed.
    #[error("Session {id} is no longer valid")]
    SessionExpired { id: String },

    /// The requested Kilo operation is not available with the current server
    /// version or configuration.
    #[error("Operation not supported: {0}")]
    NotSupported(String),

    /// Any other error with a free-form message.
    #[error("Kilo error: {0}")]
    Other(String),
}

impl KiloError {
    /// Construct an `ApiError` from an HTTP status + body string.
    pub fn api(status: u16, body: impl Into<String>) -> Self {
        Self::ApiError {
            status,
            body: body.into(),
        }
    }
}

/// Convenience alias used throughout the crate.
pub type KiloResult<T> = Result<T, KiloError>;
