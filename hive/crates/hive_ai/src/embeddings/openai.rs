use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use super::types::{EmbeddingError, EmbeddingProvider, EmbeddingResult};

const OPENAI_EMBEDDINGS_URL: &str = "https://api.openai.com/v1/embeddings";
const MODEL: &str = "text-embedding-3-small";
const DIMENSIONS: usize = 1536;

#[derive(Serialize)]
struct EmbeddingRequest {
    model: String,
    input: Vec<String>,
}

#[derive(Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
}

#[derive(Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
}

pub struct OpenAiEmbeddings {
    api_key: String,
    client: reqwest::Client,
}

impl OpenAiEmbeddings {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl EmbeddingProvider for OpenAiEmbeddings {
    async fn embed(&self, texts: &[&str]) -> Result<EmbeddingResult, EmbeddingError> {
        if self.api_key.is_empty() {
            return Err(EmbeddingError::AuthError(
                "OpenAI API key not configured".into(),
            ));
        }

        let request = EmbeddingRequest {
            model: MODEL.to_string(),
            input: texts.iter().map(|s| s.to_string()).collect(),
        };

        let response = self
            .client
            .post(OPENAI_EMBEDDINGS_URL)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| EmbeddingError::Network(e.to_string()))?;

        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(EmbeddingError::AuthError("Invalid API key".into()));
        }
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(EmbeddingError::RateLimit);
        }
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(EmbeddingError::Other(format!(
                "HTTP {status}: {error_text}"
            )));
        }

        let body: EmbeddingResponse = response
            .json()
            .await
            .map_err(|e| EmbeddingError::Other(format!("Failed to parse response: {e}")))?;

        Ok(body.data.into_iter().map(|d| d.embedding).collect())
    }

    fn model_name(&self) -> &str {
        MODEL
    }

    fn dimensions(&self) -> usize {
        DIMENSIONS
    }

    async fn is_available(&self) -> bool {
        !self.api_key.is_empty()
    }
}
