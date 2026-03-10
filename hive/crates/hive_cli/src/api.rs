//! Cloud API client for Hive Cloud endpoints.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

const DEFAULT_API_URL: &str = "https://api.hivecode.app";

#[derive(Serialize)]
pub struct LoginRequest {
    pub email: String,
}

#[derive(Deserialize)]
pub struct TokenPair {
    pub access_token: String,
    pub refresh_token: String,
    pub token_type: String,
    pub expires_in: u64,
}

#[derive(Debug, Deserialize)]
pub struct AccountInfo {
    pub id: String,
    pub email: String,
    pub display_name: Option<String>,
    pub tier: String,
    pub token_budget_cents: i32,
    pub token_used_cents: i32,
    pub subscription_expires_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct UsageSummary {
    pub tier: String,
    pub token_budget_cents: i32,
    pub token_used_cents: i32,
    pub token_remaining_cents: i32,
    pub budget_reset_at: String,
    pub usage_percent: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Serialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    pub stream: bool,
}

#[derive(Debug, Deserialize)]
pub struct ChatResponse {
    pub id: String,
    pub model: String,
    pub content: String,
    pub usage: UsageInfo,
    pub cost_cents: i32,
}

#[derive(Debug, Deserialize)]
pub struct UsageInfo {
    pub input_tokens: i32,
    pub output_tokens: i32,
}

#[derive(Debug, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub input_price_per_mtok: f64,
    pub output_price_per_mtok: f64,
    pub context_window: u32,
    pub available: bool,
}

#[derive(Debug, Deserialize)]
pub struct ManifestEntry {
    pub key: String,
    pub size_bytes: i64,
    pub checksum: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct ManifestResponse {
    pub blobs: Vec<ManifestEntry>,
    pub total_size_bytes: i64,
    pub storage_limit_bytes: i64,
}

#[derive(Debug, Deserialize)]
pub struct SseChunk {
    pub chunk: String,
}

/// HTTP client for the Hive Cloud API.
pub struct CloudClient {
    base_url: String,
    jwt: Option<String>,
    client: reqwest::Client,
}

impl CloudClient {
    pub fn new(api_url: Option<&str>, jwt: Option<&str>) -> Self {
        let base_url = api_url
            .unwrap_or(DEFAULT_API_URL)
            .trim_end_matches('/')
            .to_string();
        Self {
            base_url,
            jwt: jwt.map(|s| s.to_string()),
            client: reqwest::Client::new(),
        }
    }

    fn auth_get(&self, path: &str) -> Result<reqwest::RequestBuilder> {
        let jwt = self.jwt.as_deref().context("Not authenticated")?;
        Ok(self
            .client
            .get(format!("{}{}", self.base_url, path))
            .header("Authorization", format!("Bearer {}", jwt)))
    }

    fn auth_post(&self, path: &str) -> Result<reqwest::RequestBuilder> {
        let jwt = self.jwt.as_deref().context("Not authenticated")?;
        Ok(self
            .client
            .post(format!("{}{}", self.base_url, path))
            .header("Authorization", format!("Bearer {}", jwt)))
    }

    fn auth_put(&self, path: &str) -> Result<reqwest::RequestBuilder> {
        let jwt = self.jwt.as_deref().context("Not authenticated")?;
        Ok(self
            .client
            .put(format!("{}{}", self.base_url, path))
            .header("Authorization", format!("Bearer {}", jwt)))
    }

    pub async fn login(&self, email: &str) -> Result<TokenPair> {
        let resp = self
            .client
            .post(format!("{}/auth/login", self.base_url))
            .json(&LoginRequest {
                email: email.to_string(),
            })
            .send()
            .await
            .context("Failed to connect to Hive Cloud")?;
        if !resp.status().is_success() {
            let s = resp.status();
            let b = resp.text().await.unwrap_or_default();
            bail!("Login failed ({s}): {b}");
        }
        resp.json::<TokenPair>()
            .await
            .context("Failed to parse login response")
    }

    pub async fn get_account(&self) -> Result<AccountInfo> {
        let resp = self
            .auth_get("/account")?
            .send()
            .await
            .context("Failed to connect to Hive Cloud")?;
        if !resp.status().is_success() {
            let s = resp.status();
            let b = resp.text().await.unwrap_or_default();
            bail!("Failed to get account ({s}): {b}");
        }
        resp.json::<AccountInfo>()
            .await
            .context("Failed to parse account response")
    }

    pub async fn get_usage(&self) -> Result<UsageSummary> {
        let resp = self
            .auth_get("/account/usage")?
            .send()
            .await
            .context("Failed to connect to Hive Cloud")?;
        if !resp.status().is_success() {
            let s = resp.status();
            let b = resp.text().await.unwrap_or_default();
            bail!("Failed to get usage ({s}): {b}");
        }
        resp.json::<UsageSummary>()
            .await
            .context("Failed to parse usage response")
    }

    pub async fn chat_completion(
        &self,
        model: &str,
        messages: &[ChatMessage],
    ) -> Result<reqwest::Response> {
        let req = ChatRequest {
            model: model.to_string(),
            messages: messages.to_vec(),
            max_tokens: Some(4096),
            temperature: Some(0.7),
            stream: true,
        };
        let resp = self
            .auth_post("/gateway/v1/chat")?
            .json(&req)
            .send()
            .await
            .context("Failed to connect to AI gateway")?;
        if !resp.status().is_success() {
            let s = resp.status();
            let b = resp.text().await.unwrap_or_default();
            bail!("Chat request failed ({s}): {b}");
        }
        Ok(resp)
    }

    pub async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        let resp = self
            .client
            .get(format!("{}/gateway/v1/models", self.base_url))
            .send()
            .await
            .context("Failed to connect to Hive Cloud")?;
        if !resp.status().is_success() {
            let s = resp.status();
            let b = resp.text().await.unwrap_or_default();
            bail!("Failed to list models ({s}): {b}");
        }
        resp.json::<Vec<ModelInfo>>()
            .await
            .context("Failed to parse models response")
    }

    pub async fn sync_push(&self, key: &str, data: &[u8]) -> Result<()> {
        let resp = self
            .auth_put(&format!("/v1/sync/blobs/{}", key))?
            .header("Content-Type", "application/octet-stream")
            .body(data.to_vec())
            .send()
            .await
            .context("Failed to push blob")?;
        if !resp.status().is_success() {
            let s = resp.status();
            let b = resp.text().await.unwrap_or_default();
            bail!("Sync push failed ({s}): {b}");
        }
        Ok(())
    }

    pub async fn sync_pull(&self, key: &str) -> Result<Vec<u8>> {
        let resp = self
            .auth_get(&format!("/v1/sync/blobs/{}", key))?
            .send()
            .await
            .context("Failed to pull blob")?;
        if !resp.status().is_success() {
            let s = resp.status();
            let b = resp.text().await.unwrap_or_default();
            bail!("Sync pull failed ({s}): {b}");
        }
        let bytes = resp.bytes().await.context("Failed to read blob data")?;
        Ok(bytes.to_vec())
    }

    pub async fn sync_manifest(&self) -> Result<ManifestResponse> {
        let resp = self
            .auth_get("/v1/sync/manifest")?
            .send()
            .await
            .context("Failed to get manifest")?;
        if !resp.status().is_success() {
            let s = resp.status();
            let b = resp.text().await.unwrap_or_default();
            bail!("Sync manifest failed ({s}): {b}");
        }
        resp.json::<ManifestResponse>()
            .await
            .context("Failed to parse manifest")
    }
}
