use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use super::types::{EmbeddingError, EmbeddingProvider, EmbeddingResult};

const DEFAULT_MODEL: &str = "nomic-embed-text";
const DIMENSIONS: usize = 768;

#[derive(Serialize)]
struct OllamaEmbedRequest {
    model: String,
    input: Vec<String>,
}

#[derive(Deserialize)]
struct OllamaEmbedResponse {
    embeddings: Vec<Vec<f32>>,
}

pub struct OllamaEmbeddings {
    base_url: String,
    model: String,
    client: reqwest::Client,
}

impl OllamaEmbeddings {
    pub fn new(base_url: String) -> Self {
        Self {
            base_url,
            model: DEFAULT_MODEL.to_string(),
            client: reqwest::ClientBuilder::new()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_default(),
        }
    }

    pub fn with_model(mut self, model: &str) -> Self {
        self.model = model.to_string();
        self
    }
}

#[async_trait]
impl EmbeddingProvider for OllamaEmbeddings {
    async fn embed(&self, texts: &[&str]) -> Result<EmbeddingResult, EmbeddingError> {
        let url = format!("{}/api/embed", self.base_url);

        let request = OllamaEmbedRequest {
            model: self.model.clone(),
            input: texts.iter().map(|s| s.to_string()).collect(),
        };

        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| EmbeddingError::Network(e.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(EmbeddingError::Unavailable(format!(
                "Ollama returned {status}: {error_text}"
            )));
        }

        let body: OllamaEmbedResponse = response
            .json()
            .await
            .map_err(|e| EmbeddingError::Other(format!("Failed to parse Ollama response: {e}")))?;

        Ok(body.embeddings)
    }

    fn model_name(&self) -> &str {
        &self.model
    }

    fn dimensions(&self) -> usize {
        DIMENSIONS
    }

    async fn is_available(&self) -> bool {
        let url = format!("{}/api/tags", self.base_url);
        self.client
            .get(&url)
            .timeout(std::time::Duration::from_secs(2))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }
}
