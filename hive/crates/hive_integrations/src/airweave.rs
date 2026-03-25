//! Airweave context retrieval integration.
//!
//! Optional REST client for the [Airweave](https://airweave.ai/) platform,
//! which connects 50+ data sources and exposes unified semantic search.
//! When configured (URL + API key), Hive agents can search across all
//! connected sources (Google Drive, Notion, Slack, Jira, etc.) in a
//! single query.

use anyhow::{Context, Result};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE, USER_AGENT};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::debug;

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

/// An Airweave collection (searchable knowledge base).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AirweaveCollection {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub readable_id: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub modified_at: String,
    #[serde(default)]
    pub embedding_model_name: Option<String>,
}

/// A connected data source within Airweave.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AirweaveSourceConnection {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub collection_id: Option<String>,
    #[serde(default)]
    pub source_short_name: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub modified_at: String,
}

/// A breadcrumb in a search result, showing the path to the entity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AirweaveBreadcrumb {
    #[serde(default)]
    pub entity_id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub entity_type: Option<String>,
}

/// A single search result from Airweave.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AirweaveSearchResult {
    #[serde(default)]
    pub entity_id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub relevance_score: f64,
    #[serde(default)]
    pub textual_representation: String,
    #[serde(default)]
    pub web_url: Option<String>,
    #[serde(default)]
    pub breadcrumbs: Vec<AirweaveBreadcrumb>,
    #[serde(default)]
    pub airweave_system_metadata: serde_json::Value,
}

/// Response from an Airweave search endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AirweaveSearchResponse {
    #[serde(default)]
    pub results: Vec<AirweaveSearchResult>,
}

// ---------------------------------------------------------------------------
// Client
// ---------------------------------------------------------------------------

/// REST client for the Airweave context retrieval API.
///
/// Supports both self-hosted (Docker, typically `http://localhost:8080`)
/// and cloud-hosted (`https://api.airweave.ai`) instances.
pub struct AirweaveClient {
    client: Client,
    base_url: String,
}

impl AirweaveClient {
    /// Create a new client targeting the given Airweave instance.
    ///
    /// `base_url` is the root API URL (e.g. `http://localhost:8080` or
    /// `https://api.airweave.ai`).  `api_key` is used as a Bearer token.
    pub fn new(base_url: &str, api_key: &str) -> Result<Self> {
        let base_url = base_url.trim_end_matches('/').to_string();

        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {api_key}"))
                .context("invalid API key characters")?,
        );
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(USER_AGENT, HeaderValue::from_static("Hive/1.0"));

        let client = Client::builder()
            .default_headers(headers)
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .context("failed to build HTTP client")?;

        Ok(Self { client, base_url })
    }

    /// Return the configured base URL.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Check connectivity by probing the collections endpoint.
    pub async fn health_check(&self) -> Result<bool> {
        let url = format!("{}/collections?limit=1", self.base_url);
        debug!("Airweave health check: GET {}", url);
        match self.client.get(&url).send().await {
            Ok(resp) => Ok(resp.status().is_success()),
            Err(_) => Ok(false),
        }
    }

    /// List available collections.
    pub async fn list_collections(&self, limit: u32) -> Result<Vec<AirweaveCollection>> {
        let url = format!("{}/collections?limit={}", self.base_url, limit);
        debug!("Airweave list collections: GET {}", url);
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .context("Airweave list_collections request failed")?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Airweave list_collections error ({status}): {body}");
        }
        resp.json()
            .await
            .context("failed to parse collections response")
    }

    /// List active source connections, optionally filtered by collection.
    pub async fn list_source_connections(
        &self,
        collection: Option<&str>,
        limit: u32,
    ) -> Result<Vec<AirweaveSourceConnection>> {
        let mut url = format!("{}/source-connections?limit={}", self.base_url, limit);
        if let Some(coll) = collection {
            url.push_str(&format!("&collection={coll}"));
        }
        debug!("Airweave list connections: GET {}", url);
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .context("Airweave list_source_connections request failed")?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Airweave list_source_connections error ({status}): {body}");
        }
        resp.json()
            .await
            .context("failed to parse source connections response")
    }

    /// Search a collection using the specified search tier.
    ///
    /// `search_type` is one of `"instant"`, `"classic"`, or `"agentic"`.
    pub async fn search(
        &self,
        collection_readable_id: &str,
        query: &str,
        limit: u32,
        search_type: &str,
    ) -> Result<AirweaveSearchResponse> {
        let url = format!(
            "{}/collections/{}/search/{}",
            self.base_url, collection_readable_id, search_type
        );
        debug!("Airweave search: POST {} query={:?}", url, query);

        let body = serde_json::json!({
            "query": query,
            "limit": limit,
        });
        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .context("Airweave search request failed")?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Airweave search error ({status}): {body}");
        }
        resp.json()
            .await
            .context("failed to parse search response")
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_strips_trailing_slash() {
        let c = AirweaveClient::new("https://api.airweave.ai/", "test-key").unwrap();
        assert_eq!(c.base_url(), "https://api.airweave.ai");
    }

    #[test]
    fn client_preserves_clean_url() {
        let c = AirweaveClient::new("http://localhost:8080", "key").unwrap();
        assert_eq!(c.base_url(), "http://localhost:8080");
    }

    #[test]
    fn collection_deserialize() {
        let json = r#"{
            "id": "abc-123",
            "name": "My KB",
            "readable_id": "my-kb",
            "status": "ACTIVE",
            "created_at": "2025-01-01T00:00:00Z",
            "modified_at": "2025-06-15T12:00:00Z"
        }"#;
        let c: AirweaveCollection = serde_json::from_str(json).unwrap();
        assert_eq!(c.id, "abc-123");
        assert_eq!(c.readable_id, "my-kb");
        assert_eq!(c.status, "ACTIVE");
        assert!(c.embedding_model_name.is_none());
    }

    #[test]
    fn collection_deserialize_with_extra_fields() {
        let json = r#"{
            "id": "x",
            "name": "Test",
            "readable_id": "test",
            "status": "ACTIVE",
            "created_at": "",
            "modified_at": "",
            "some_future_field": true
        }"#;
        // Should not fail — extra fields are silently ignored.
        let c: AirweaveCollection = serde_json::from_str(json).unwrap();
        assert_eq!(c.id, "x");
    }

    #[test]
    fn source_connection_deserialize() {
        let json = r#"{
            "id": "conn-1",
            "name": "Google Drive",
            "collection_id": "abc-123",
            "source_short_name": "google_drive",
            "status": "active",
            "created_at": "2025-01-01T00:00:00Z",
            "modified_at": "2025-06-15T12:00:00Z"
        }"#;
        let c: AirweaveSourceConnection = serde_json::from_str(json).unwrap();
        assert_eq!(c.name, "Google Drive");
        assert_eq!(c.source_short_name.as_deref(), Some("google_drive"));
    }

    #[test]
    fn search_result_deserialize() {
        let json = r#"{
            "entity_id": "ent-1",
            "name": "Project Plan",
            "relevance_score": 0.87,
            "textual_representation": "Q4 project plan with timelines...",
            "web_url": "https://notion.so/page/123",
            "breadcrumbs": [
                {"entity_id": "p-1", "name": "Workspace", "entity_type": "workspace"},
                {"entity_id": "p-2", "name": "Projects", "entity_type": "database"}
            ],
            "airweave_system_metadata": {"source": "notion"}
        }"#;
        let r: AirweaveSearchResult = serde_json::from_str(json).unwrap();
        assert_eq!(r.name, "Project Plan");
        assert!((r.relevance_score - 0.87).abs() < 0.001);
        assert_eq!(r.breadcrumbs.len(), 2);
        assert_eq!(r.breadcrumbs[0].name, "Workspace");
        assert_eq!(r.web_url.as_deref(), Some("https://notion.so/page/123"));
    }

    #[test]
    fn search_result_deserialize_minimal() {
        // Minimal response with only required id field.
        let json = r#"{"entity_id": "e1"}"#;
        let r: AirweaveSearchResult = serde_json::from_str(json).unwrap();
        assert_eq!(r.entity_id, "e1");
        assert_eq!(r.name, "");
        assert!(r.breadcrumbs.is_empty());
        assert!(r.web_url.is_none());
    }

    #[test]
    fn search_response_deserialize() {
        let json = r#"{"results": [
            {"entity_id": "e1", "name": "Doc A", "relevance_score": 0.9, "textual_representation": "content A"},
            {"entity_id": "e2", "name": "Doc B", "relevance_score": 0.7, "textual_representation": "content B"}
        ]}"#;
        let r: AirweaveSearchResponse = serde_json::from_str(json).unwrap();
        assert_eq!(r.results.len(), 2);
        assert_eq!(r.results[0].entity_id, "e1");
    }

    #[test]
    fn search_response_empty() {
        let json = r#"{"results": []}"#;
        let r: AirweaveSearchResponse = serde_json::from_str(json).unwrap();
        assert!(r.results.is_empty());
    }
}
