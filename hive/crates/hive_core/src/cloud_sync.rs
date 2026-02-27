//! Client-side cloud sync module.
//!
//! Provides a [`SyncClient`] that pushes/pulls opaque blobs to the Hive Cloud
//! sync API, enabling settings, conversations, and other data to follow the
//! user across devices.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors from the cloud sync service.
#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("Server error ({status}): {message}")]
    Server { status: u16, message: String },

    #[error("Not authenticated — cloud JWT is missing or expired")]
    NotAuthenticated,
}

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

/// A single entry in the sync manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestEntry {
    pub key: String,
    pub size_bytes: i64,
    pub checksum: String,
    pub updated_at: DateTime<Utc>,
}

/// The full manifest returned by the sync API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestResponse {
    pub blobs: Vec<ManifestEntry>,
    pub total_size_bytes: i64,
    pub storage_limit_bytes: i64,
}

// ---------------------------------------------------------------------------
// Client
// ---------------------------------------------------------------------------

/// HTTP client for the Hive Cloud blob sync API.
pub struct SyncClient {
    api_url: String,
    jwt: String,
    client: reqwest::Client,
}

impl SyncClient {
    /// Create a new sync client.
    ///
    /// `api_url` should be the base cloud API URL (e.g. `https://api.hivecode.app`).
    /// `jwt` is the bearer token obtained during cloud login.
    pub fn new(api_url: String, jwt: String) -> Self {
        Self {
            api_url: api_url.trim_end_matches('/').to_string(),
            jwt,
            client: reqwest::Client::new(),
        }
    }

    /// Upload (or overwrite) a blob by key.
    ///
    /// `PUT /v1/sync/blobs/{key}`
    pub async fn push(&self, key: &str, data: &[u8]) -> Result<(), SyncError> {
        if self.jwt.is_empty() {
            return Err(SyncError::NotAuthenticated);
        }

        let url = format!("{}/v1/sync/blobs/{}", self.api_url, key);
        let resp = self
            .client
            .put(&url)
            .header("Authorization", format!("Bearer {}", self.jwt))
            .header("Content-Type", "application/octet-stream")
            .body(data.to_vec())
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let message = resp.text().await.unwrap_or_default();
            return Err(SyncError::Server { status, message });
        }

        Ok(())
    }

    /// Download a blob by key.
    ///
    /// `GET /v1/sync/blobs/{key}`
    pub async fn pull(&self, key: &str) -> Result<Vec<u8>, SyncError> {
        if self.jwt.is_empty() {
            return Err(SyncError::NotAuthenticated);
        }

        let url = format!("{}/v1/sync/blobs/{}", self.api_url, key);
        let resp = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.jwt))
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let message = resp.text().await.unwrap_or_default();
            return Err(SyncError::Server { status, message });
        }

        let bytes = resp.bytes().await?;
        Ok(bytes.to_vec())
    }

    /// Retrieve the full manifest of stored blobs.
    ///
    /// `GET /v1/sync/manifest`
    pub async fn manifest(&self) -> Result<ManifestResponse, SyncError> {
        if self.jwt.is_empty() {
            return Err(SyncError::NotAuthenticated);
        }

        let url = format!("{}/v1/sync/manifest", self.api_url);
        let resp = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.jwt))
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let message = resp.text().await.unwrap_or_default();
            return Err(SyncError::Server { status, message });
        }

        let manifest: ManifestResponse = resp
            .json()
            .await
            .map_err(|e| SyncError::Server {
                status: 0,
                message: format!("Failed to parse manifest: {e}"),
            })?;

        Ok(manifest)
    }

    /// Delete a blob by key.
    ///
    /// `DELETE /v1/sync/blobs/{key}`
    pub async fn delete(&self, key: &str) -> Result<(), SyncError> {
        if self.jwt.is_empty() {
            return Err(SyncError::NotAuthenticated);
        }

        let url = format!("{}/v1/sync/blobs/{}", self.api_url, key);
        let resp = self
            .client
            .delete(&url)
            .header("Authorization", format!("Bearer {}", self.jwt))
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let message = resp.text().await.unwrap_or_default();
            return Err(SyncError::Server { status, message });
        }

        Ok(())
    }
}
