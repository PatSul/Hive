mod ollama;
mod openai;
mod types;

pub use ollama::OllamaEmbeddings;
pub use openai::OpenAiEmbeddings;
pub use types::{EmbeddingError, EmbeddingProvider, EmbeddingResult, MockEmbeddingProvider};
