//! Configuration for the Kilo integration.
//!
//! [`KiloConfig`] is constructed from [`hive_ai::service::AiServiceConfig`]
//! fields and passed to [`crate::client::KiloClient`].

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Session policy
// ---------------------------------------------------------------------------

/// Controls how Kilo sessions are allocated across calls.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionPolicy {
    /// A new session is created for every call and closed when done.
    /// Safest option — no shared state between calls.
    AlwaysEphemeral,

    /// Sessions are pooled with LRU eviction, up to `max_sessions` live at once.
    /// Reduces session-creation latency for high-throughput workloads.
    PooledLru {
        /// Maximum number of concurrent live sessions in the pool.
        max_sessions: usize,
    },
}

impl Default for SessionPolicy {
    fn default() -> Self {
        Self::AlwaysEphemeral
    }
}

// ---------------------------------------------------------------------------
// KiloConfig
// ---------------------------------------------------------------------------

/// Configuration for the Kilo REST API client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KiloConfig {
    /// Kilo server URL.  Defaults to `http://localhost:4096`.
    pub base_url: String,

    /// Password for HTTP Basic Auth when `KILO_SERVER_PASSWORD` is set
    /// on the Kilo server.  `None` means no authentication.
    pub password: Option<String>,

    /// Timeout for the initial TCP connection and HTTP response headers
    /// (does **not** apply to SSE streaming bodies).
    pub connect_timeout_secs: u64,

    /// When set, every chat/execute call explicitly requests this model from
    /// Kilo's `/provider` router.  When `None`, Kilo uses its own default.
    pub default_model: Option<String>,

    /// How sessions are allocated (see [`SessionPolicy`]).
    pub session_policy: SessionPolicy,
}

impl Default for KiloConfig {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:4096".into(),
            password: None,
            connect_timeout_secs: 5,
            default_model: None,
            session_policy: SessionPolicy::AlwaysEphemeral,
        }
    }
}

impl KiloConfig {
    /// Construct from the url/password fields exposed on `AiServiceConfig`.
    pub fn from_service_config(url: Option<&str>, password: Option<&str>) -> Self {
        Self {
            base_url: url
                .filter(|s| !s.is_empty())
                .unwrap_or("http://localhost:4096")
                .to_owned(),
            password: password.filter(|s| !s.is_empty()).map(str::to_owned),
            ..Self::default()
        }
    }
}
