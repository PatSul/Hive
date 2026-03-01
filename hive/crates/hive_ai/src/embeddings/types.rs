use async_trait::async_trait;
use std::fmt;

/// Result of an embedding operation — Vec of f32 vectors
pub type EmbeddingResult = Vec<Vec<f32>>;

/// Errors that can occur during embedding
#[derive(Debug, Clone)]
pub enum EmbeddingError {
    /// Provider not available (API down, Ollama not running)
    Unavailable(String),
    /// Invalid API key or authentication failure
    AuthError(String),
    /// Rate limit exceeded
    RateLimit,
    /// Network error
    Network(String),
    /// Input too long for model
    InputTooLong { max_tokens: usize, actual_tokens: usize },
    /// Other errors
    Other(String),
}

impl fmt::Display for EmbeddingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable(msg) => write!(f, "Embedding provider unavailable: {msg}"),
            Self::AuthError(msg) => write!(f, "Authentication error: {msg}"),
            Self::RateLimit => write!(f, "Rate limit exceeded"),
            Self::Network(msg) => write!(f, "Network error: {msg}"),
            Self::InputTooLong { max_tokens, actual_tokens } => {
                write!(f, "Input too long: {actual_tokens} tokens (max {max_tokens})")
            }
            Self::Other(msg) => write!(f, "Embedding error: {msg}"),
        }
    }
}

impl std::error::Error for EmbeddingError {}

/// Trait for embedding providers (OpenAI, Ollama, etc.)
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// Embed one or more texts into dense vectors
    async fn embed(&self, texts: &[&str]) -> Result<EmbeddingResult, EmbeddingError>;

    /// Name of the embedding model (e.g., "text-embedding-3-small")
    fn model_name(&self) -> &str;

    /// Dimensionality of output vectors
    fn dimensions(&self) -> usize;

    /// Check if the provider is available (API reachable, model loaded)
    async fn is_available(&self) -> bool;
}

/// Mock provider for testing — returns deterministic pseudo-random vectors
pub struct MockEmbeddingProvider {
    dims: usize,
}

impl MockEmbeddingProvider {
    pub fn new(dims: usize) -> Self {
        Self { dims }
    }
}

#[async_trait]
impl EmbeddingProvider for MockEmbeddingProvider {
    async fn embed(&self, texts: &[&str]) -> Result<EmbeddingResult, EmbeddingError> {
        Ok(texts
            .iter()
            .map(|text| {
                // Deterministic: hash-based so same text = same embedding
                let seed = text.bytes().fold(0u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32));
                (0..self.dims)
                    .map(|i| {
                        let val = ((seed.wrapping_mul(i as u32 + 1)) % 1000) as f32 / 1000.0;
                        val * 2.0 - 1.0 // normalize to [-1, 1]
                    })
                    .collect()
            })
            .collect())
    }

    fn model_name(&self) -> &str {
        "mock-384"
    }

    fn dimensions(&self) -> usize {
        self.dims
    }

    async fn is_available(&self) -> bool {
        true
    }
}
