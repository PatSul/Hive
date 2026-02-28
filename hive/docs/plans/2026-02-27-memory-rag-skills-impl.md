# Memory, RAG & Skills Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Give Hive persistent intelligence via vector-embedded memory (LanceDB), memory flush on context compaction, and a user-facing skills creation panel.

**Architecture:** Three systems — (1) HiveMemory module in hive_ai wrapping LanceDB + embedding providers for hybrid vector+TF-IDF search, (2) pre-compaction memory flush that extracts key insights before context trimming, (3) Skills panel in hive_ui_panels for user-created skills CRUD.

**Tech Stack:** LanceDB (embedded vector DB), Arrow (columnar data), reqwest (OpenAI embeddings API), GPUI (skills UI panel), existing hive_ai/hive_agents infrastructure.

**Design Doc:** `docs/plans/2026-02-27-memory-rag-skills-design.md`

---

## Task 1: Add LanceDB + Arrow Dependencies

**Files:**
- Modify: `hive/crates/hive_ai/Cargo.toml`
- Modify: `hive/Cargo.lock` (auto-updated)

**Step 1: Add lancedb and arrow crate dependencies**

Add to `hive/crates/hive_ai/Cargo.toml` under `[dependencies]`:
```toml
lancedb = "0.23"
arrow-array = "54"
arrow-schema = "54"
```

Note: Pin `arrow-*` to match the version LanceDB depends on. Check `cargo tree -p lancedb -d` after adding to verify compatibility.

**Step 2: Verify build compiles**

Run: `cargo check -p hive_ai`
Expected: Compiles successfully (warnings OK)

**Step 3: Commit**
```bash
git add hive/crates/hive_ai/Cargo.toml hive/Cargo.lock
git commit -m "deps: add lancedb and arrow crates to hive_ai"
```

---

## Task 2: EmbeddingProvider Trait

**Files:**
- Create: `hive/crates/hive_ai/src/embeddings/mod.rs`
- Create: `hive/crates/hive_ai/src/embeddings/types.rs`
- Modify: `hive/crates/hive_ai/src/lib.rs` (add `pub mod embeddings;`)
- Test: `hive/crates/hive_ai/tests/embeddings_tests.rs`

**Step 1: Write the failing test**

Create `hive/crates/hive_ai/tests/embeddings_tests.rs`:
```rust
use hive_ai::embeddings::{EmbeddingProvider, EmbeddingResult, MockEmbeddingProvider};

#[tokio::test]
async fn test_mock_embedding_provider_returns_correct_dimensions() {
    let provider = MockEmbeddingProvider::new(384);
    let result = provider.embed(&["hello world"]).await.unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].len(), 384);
}

#[tokio::test]
async fn test_mock_embedding_provider_batch_embed() {
    let provider = MockEmbeddingProvider::new(384);
    let texts = vec!["hello", "world", "test"];
    let result = provider.embed(&texts).await.unwrap();
    assert_eq!(result.len(), 3);
    for emb in &result {
        assert_eq!(emb.len(), 384);
    }
}

#[tokio::test]
async fn test_embedding_provider_metadata() {
    let provider = MockEmbeddingProvider::new(384);
    assert_eq!(provider.dimensions(), 384);
    assert_eq!(provider.model_name(), "mock-384");
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p hive_ai --test embeddings_tests`
Expected: FAIL — `unresolved import hive_ai::embeddings`

**Step 3: Create the types and trait**

Create `hive/crates/hive_ai/src/embeddings/types.rs`:
```rust
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
```

Create `hive/crates/hive_ai/src/embeddings/mod.rs`:
```rust
mod types;

pub use types::{EmbeddingError, EmbeddingProvider, EmbeddingResult, MockEmbeddingProvider};
```

Add to `hive/crates/hive_ai/src/lib.rs`:
```rust
pub mod embeddings;
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p hive_ai --test embeddings_tests`
Expected: 3 tests PASS

**Step 5: Commit**
```bash
git add hive/crates/hive_ai/src/embeddings/ hive/crates/hive_ai/src/lib.rs hive/crates/hive_ai/tests/embeddings_tests.rs
git commit -m "feat(hive_ai): add EmbeddingProvider trait with mock implementation"
```

---

## Task 3: OpenAI Embedding Provider

**Files:**
- Create: `hive/crates/hive_ai/src/embeddings/openai.rs`
- Modify: `hive/crates/hive_ai/src/embeddings/mod.rs`
- Test: `hive/crates/hive_ai/tests/embeddings_tests.rs` (add tests)

**Step 1: Write the failing test**

Add to `embeddings_tests.rs`:
```rust
use hive_ai::embeddings::OpenAiEmbeddings;

#[tokio::test]
async fn test_openai_embeddings_unavailable_without_key() {
    let provider = OpenAiEmbeddings::new(String::new());
    assert!(!provider.is_available().await);
}

#[tokio::test]
async fn test_openai_embeddings_metadata() {
    let provider = OpenAiEmbeddings::new("sk-test".to_string());
    assert_eq!(provider.model_name(), "text-embedding-3-small");
    assert_eq!(provider.dimensions(), 1536);
}

#[tokio::test]
async fn test_openai_embeddings_empty_key_returns_auth_error() {
    let provider = OpenAiEmbeddings::new(String::new());
    let result = provider.embed(&["test"]).await;
    assert!(matches!(result, Err(hive_ai::embeddings::EmbeddingError::AuthError(_))));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p hive_ai --test embeddings_tests test_openai`
Expected: FAIL — `unresolved import`

**Step 3: Implement OpenAI embeddings**

Create `hive/crates/hive_ai/src/embeddings/openai.rs`:
```rust
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

#[derive(Deserialize)]
struct ErrorResponse {
    error: ErrorDetail,
}

#[derive(Deserialize)]
struct ErrorDetail {
    message: String,
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
            return Err(EmbeddingError::AuthError("OpenAI API key not configured".into()));
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
            return Err(EmbeddingError::Other(format!("HTTP {status}: {error_text}")));
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
```

Update `hive/crates/hive_ai/src/embeddings/mod.rs`:
```rust
mod openai;
mod types;

pub use openai::OpenAiEmbeddings;
pub use types::{EmbeddingError, EmbeddingProvider, EmbeddingResult, MockEmbeddingProvider};
```

**Step 4: Run tests**

Run: `cargo test -p hive_ai --test embeddings_tests`
Expected: 6 tests PASS (3 mock + 3 OpenAI)

**Step 5: Commit**
```bash
git add hive/crates/hive_ai/src/embeddings/
git commit -m "feat(hive_ai): add OpenAI embedding provider (text-embedding-3-small)"
```

---

## Task 4: Ollama Embedding Provider

**Files:**
- Create: `hive/crates/hive_ai/src/embeddings/ollama.rs`
- Modify: `hive/crates/hive_ai/src/embeddings/mod.rs`
- Test: `hive/crates/hive_ai/tests/embeddings_tests.rs` (add tests)

**Step 1: Write the failing test**

Add to `embeddings_tests.rs`:
```rust
use hive_ai::embeddings::OllamaEmbeddings;

#[tokio::test]
async fn test_ollama_embeddings_metadata() {
    let provider = OllamaEmbeddings::new("http://localhost:11434".to_string());
    assert_eq!(provider.model_name(), "nomic-embed-text");
    // Ollama dimensions vary by model, but nomic-embed-text is 768
    assert_eq!(provider.dimensions(), 768);
}

#[tokio::test]
async fn test_ollama_embeddings_unavailable_when_not_running() {
    // Use a port that's definitely not running Ollama
    let provider = OllamaEmbeddings::new("http://localhost:1".to_string());
    assert!(!provider.is_available().await);
}

#[tokio::test]
async fn test_ollama_embeddings_returns_network_error_when_down() {
    let provider = OllamaEmbeddings::new("http://localhost:1".to_string());
    let result = provider.embed(&["test"]).await;
    assert!(matches!(result, Err(hive_ai::embeddings::EmbeddingError::Network(_))));
}
```

**Step 2: Run to verify failure**

Run: `cargo test -p hive_ai --test embeddings_tests test_ollama`
Expected: FAIL

**Step 3: Implement Ollama embeddings**

Create `hive/crates/hive_ai/src/embeddings/ollama.rs`:
```rust
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

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(EmbeddingError::Unavailable(format!(
                "Ollama returned {}: {error_text}",
                error_text
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
```

Update `mod.rs` to export `OllamaEmbeddings`.

**Step 4: Run tests**

Run: `cargo test -p hive_ai --test embeddings_tests`
Expected: 9 tests PASS

**Step 5: Commit**
```bash
git add hive/crates/hive_ai/src/embeddings/
git commit -m "feat(hive_ai): add Ollama embedding provider (nomic-embed-text)"
```

---

## Task 5: LanceDB Memory Store

**Files:**
- Create: `hive/crates/hive_ai/src/memory/mod.rs`
- Create: `hive/crates/hive_ai/src/memory/store.rs`
- Create: `hive/crates/hive_ai/src/memory/types.rs`
- Modify: `hive/crates/hive_ai/src/lib.rs` (add `pub mod memory;`)
- Test: `hive/crates/hive_ai/tests/memory_store_tests.rs`

**Step 1: Write the failing test**

Create `hive/crates/hive_ai/tests/memory_store_tests.rs`:
```rust
use hive_ai::memory::{MemoryStore, MemoryCategory, MemoryEntry};
use tempfile::TempDir;

#[tokio::test]
async fn test_store_creates_database_at_path() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("test.lance");
    let store = MemoryStore::open(path.to_str().unwrap()).await.unwrap();
    assert!(path.exists());
    drop(store);
}

#[tokio::test]
async fn test_store_and_recall_memory() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("test.lance");
    let store = MemoryStore::open(path.to_str().unwrap()).await.unwrap();

    let entry = MemoryEntry {
        content: "The user prefers Rust over Python".to_string(),
        category: MemoryCategory::UserPreference,
        importance: 8.0,
        conversation_id: "conv-1".to_string(),
        decay_exempt: true,
    };

    store.remember(entry, &[0.1; 768]).await.unwrap();

    let results = store.recall(&[0.1; 768], 5).await.unwrap();
    assert_eq!(results.len(), 1);
    assert!(results[0].content.contains("Rust over Python"));
}

#[tokio::test]
async fn test_store_chunk_and_search() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("test.lance");
    let store = MemoryStore::open(path.to_str().unwrap()).await.unwrap();

    store.index_chunk(
        "src/main.rs",
        "fn main() { println!(\"Hello\"); }",
        &[0.5; 768],
        0, 1,
    ).await.unwrap();

    let results = store.search_chunks(&[0.5; 768], 5).await.unwrap();
    assert_eq!(results.len(), 1);
    assert!(results[0].content.contains("main"));
}

#[tokio::test]
async fn test_store_stats() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("test.lance");
    let store = MemoryStore::open(path.to_str().unwrap()).await.unwrap();

    let stats = store.stats().await.unwrap();
    assert_eq!(stats.total_chunks, 0);
    assert_eq!(stats.total_memories, 0);
}
```

**Step 2: Run to verify failure**

Run: `cargo test -p hive_ai --test memory_store_tests`
Expected: FAIL

**Step 3: Implement types**

Create `hive/crates/hive_ai/src/memory/types.rs`:
```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MemoryCategory {
    UserPreference,
    CodePattern,
    TaskProgress,
    Decision,
    General,
}

impl MemoryCategory {
    pub fn as_str(&self) -> &str {
        match self {
            Self::UserPreference => "user_preference",
            Self::CodePattern => "code_pattern",
            Self::TaskProgress => "task_progress",
            Self::Decision => "decision",
            Self::General => "general",
        }
    }
}

#[derive(Debug, Clone)]
pub struct MemoryEntry {
    pub content: String,
    pub category: MemoryCategory,
    pub importance: f32,
    pub conversation_id: String,
    pub decay_exempt: bool,
}

#[derive(Debug, Clone)]
pub struct MemoryResult {
    pub content: String,
    pub category: String,
    pub importance: f32,
    pub score: f32,
    pub timestamp: String,
}

#[derive(Debug, Clone)]
pub struct ChunkResult {
    pub source_file: String,
    pub content: String,
    pub start_line: u32,
    pub end_line: u32,
    pub score: f32,
}

#[derive(Debug, Clone, Default)]
pub struct StoreStats {
    pub total_chunks: usize,
    pub total_memories: usize,
    pub indexed_files: usize,
}
```

**Step 4: Implement store**

Create `hive/crates/hive_ai/src/memory/store.rs` — this is the core LanceDB integration:
```rust
use std::sync::Arc;
use arrow_array::{
    Float32Array, RecordBatch, RecordBatchIterator, StringArray,
    UInt32Array, FixedSizeListArray, BooleanArray,
    types::Float32Type,
};
use arrow_schema::{DataType, Field, Schema};
use lancedb::connect;

use super::types::*;

pub struct MemoryStore {
    db: lancedb::Connection,
    vector_dim: usize,
}

impl MemoryStore {
    pub async fn open(path: &str) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let db = connect(path).execute().await?;
        let store = Self { db, vector_dim: 768 };
        store.ensure_tables().await?;
        Ok(store)
    }

    pub fn with_dimensions(mut self, dim: usize) -> Self {
        self.vector_dim = dim;
        self
    }

    async fn ensure_tables(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let table_names = self.db.table_names().execute().await?;

        if !table_names.contains(&"chunks".to_string()) {
            self.create_chunks_table().await?;
        }
        if !table_names.contains(&"memories".to_string()) {
            self.create_memories_table().await?;
        }
        Ok(())
    }

    async fn create_chunks_table(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("source_file", DataType::Utf8, false),
            Field::new("content", DataType::Utf8, false),
            Field::new(
                "embedding",
                DataType::FixedSizeList(
                    Arc::new(Field::new("item", DataType::Float32, true)),
                    self.vector_dim as i32,
                ),
                false,
            ),
            Field::new("start_line", DataType::UInt32, false),
            Field::new("end_line", DataType::UInt32, false),
        ]));

        // Create empty table with schema by inserting an empty batch
        let batch = RecordBatch::new_empty(schema.clone());
        let batches = RecordBatchIterator::new(vec![Ok(batch)], schema);
        self.db.create_table("chunks", Box::new(batches)).execute().await?;
        Ok(())
    }

    async fn create_memories_table(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("content", DataType::Utf8, false),
            Field::new(
                "embedding",
                DataType::FixedSizeList(
                    Arc::new(Field::new("item", DataType::Float32, true)),
                    self.vector_dim as i32,
                ),
                false,
            ),
            Field::new("category", DataType::Utf8, false),
            Field::new("importance", DataType::Float32, false),
            Field::new("conversation_id", DataType::Utf8, false),
            Field::new("timestamp", DataType::Utf8, false),
            Field::new("decay_exempt", DataType::Boolean, false),
        ]));

        let batch = RecordBatch::new_empty(schema.clone());
        let batches = RecordBatchIterator::new(vec![Ok(batch)], schema);
        self.db.create_table("memories", Box::new(batches)).execute().await?;
        Ok(())
    }

    pub async fn remember(
        &self,
        entry: MemoryEntry,
        embedding: &[f32],
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let table = self.db.open_table("memories").execute().await?;
        let id = uuid::Uuid::new_v4().to_string();
        let timestamp = chrono::Utc::now().to_rfc3339();

        let schema = table.schema().await?;
        let embedding_array = FixedSizeListArray::from_iter_primitive::<Float32Type, _, _>(
            vec![Some(embedding.iter().map(|v| Some(*v)).collect::<Vec<_>>())],
            self.vector_dim as i32,
        );

        let batch = RecordBatch::try_new(
            schema,
            vec![
                Arc::new(StringArray::from(vec![id.as_str()])),
                Arc::new(StringArray::from(vec![entry.content.as_str()])),
                Arc::new(embedding_array),
                Arc::new(StringArray::from(vec![entry.category.as_str()])),
                Arc::new(Float32Array::from(vec![entry.importance])),
                Arc::new(StringArray::from(vec![entry.conversation_id.as_str()])),
                Arc::new(StringArray::from(vec![timestamp.as_str()])),
                Arc::new(BooleanArray::from(vec![entry.decay_exempt])),
            ],
        )?;

        let batches = RecordBatchIterator::new(vec![Ok(batch.clone())], batch.schema());
        table.add(Box::new(batches)).execute().await?;
        Ok(())
    }

    pub async fn recall(
        &self,
        query_embedding: &[f32],
        limit: usize,
    ) -> Result<Vec<MemoryResult>, Box<dyn std::error::Error + Send + Sync>> {
        let table = self.db.open_table("memories").execute().await?;

        use futures::TryStreamExt;
        let results = table
            .query()
            .nearest_to(query_embedding)?
            .limit(limit)
            .execute()
            .await?
            .try_collect::<Vec<_>>()
            .await?;

        let mut memories = Vec::new();
        for batch in &results {
            let content_col = batch.column_by_name("content")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>());
            let category_col = batch.column_by_name("category")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>());
            let importance_col = batch.column_by_name("importance")
                .and_then(|c| c.as_any().downcast_ref::<Float32Array>());
            let timestamp_col = batch.column_by_name("timestamp")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>());
            let distance_col = batch.column_by_name("_distance")
                .and_then(|c| c.as_any().downcast_ref::<Float32Array>());

            if let (Some(content), Some(cat), Some(imp), Some(ts)) =
                (content_col, category_col, importance_col, timestamp_col)
            {
                for i in 0..batch.num_rows() {
                    let score = distance_col
                        .map(|d| 1.0 - d.value(i)) // Convert distance to similarity
                        .unwrap_or(0.0);
                    memories.push(MemoryResult {
                        content: content.value(i).to_string(),
                        category: cat.value(i).to_string(),
                        importance: imp.value(i),
                        score,
                        timestamp: ts.value(i).to_string(),
                    });
                }
            }
        }
        Ok(memories)
    }

    pub async fn index_chunk(
        &self,
        source_file: &str,
        content: &str,
        embedding: &[f32],
        start_line: u32,
        end_line: u32,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let table = self.db.open_table("chunks").execute().await?;
        let id = uuid::Uuid::new_v4().to_string();

        let schema = table.schema().await?;
        let embedding_array = FixedSizeListArray::from_iter_primitive::<Float32Type, _, _>(
            vec![Some(embedding.iter().map(|v| Some(*v)).collect::<Vec<_>>())],
            self.vector_dim as i32,
        );

        let batch = RecordBatch::try_new(
            schema,
            vec![
                Arc::new(StringArray::from(vec![id.as_str()])),
                Arc::new(StringArray::from(vec![source_file])),
                Arc::new(StringArray::from(vec![content])),
                Arc::new(embedding_array),
                Arc::new(UInt32Array::from(vec![start_line])),
                Arc::new(UInt32Array::from(vec![end_line])),
            ],
        )?;

        let batches = RecordBatchIterator::new(vec![Ok(batch.clone())], batch.schema());
        table.add(Box::new(batches)).execute().await?;
        Ok(())
    }

    pub async fn search_chunks(
        &self,
        query_embedding: &[f32],
        limit: usize,
    ) -> Result<Vec<ChunkResult>, Box<dyn std::error::Error + Send + Sync>> {
        let table = self.db.open_table("chunks").execute().await?;

        use futures::TryStreamExt;
        let results = table
            .query()
            .nearest_to(query_embedding)?
            .limit(limit)
            .execute()
            .await?
            .try_collect::<Vec<_>>()
            .await?;

        let mut chunks = Vec::new();
        for batch in &results {
            let file_col = batch.column_by_name("source_file")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>());
            let content_col = batch.column_by_name("content")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>());
            let start_col = batch.column_by_name("start_line")
                .and_then(|c| c.as_any().downcast_ref::<UInt32Array>());
            let end_col = batch.column_by_name("end_line")
                .and_then(|c| c.as_any().downcast_ref::<UInt32Array>());
            let distance_col = batch.column_by_name("_distance")
                .and_then(|c| c.as_any().downcast_ref::<Float32Array>());

            if let (Some(file), Some(content), Some(start), Some(end)) =
                (file_col, content_col, start_col, end_col)
            {
                for i in 0..batch.num_rows() {
                    let score = distance_col
                        .map(|d| 1.0 - d.value(i))
                        .unwrap_or(0.0);
                    chunks.push(ChunkResult {
                        source_file: file.value(i).to_string(),
                        content: content.value(i).to_string(),
                        start_line: start.value(i),
                        end_line: end.value(i),
                        score,
                    });
                }
            }
        }
        Ok(chunks)
    }

    pub async fn stats(&self) -> Result<StoreStats, Box<dyn std::error::Error + Send + Sync>> {
        let chunks_table = self.db.open_table("chunks").execute().await?;
        let memories_table = self.db.open_table("memories").execute().await?;

        let chunk_count = chunks_table.count_rows(None).await?;
        let memory_count = memories_table.count_rows(None).await?;

        Ok(StoreStats {
            total_chunks: chunk_count,
            total_memories: memory_count,
            indexed_files: 0, // TODO: count distinct source_file values
        })
    }

    pub async fn clear_chunks(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.db.drop_table("chunks").await?;
        self.create_chunks_table().await?;
        Ok(())
    }

    pub async fn clear_memories(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.db.drop_table("memories").await?;
        self.create_memories_table().await?;
        Ok(())
    }
}
```

Create `hive/crates/hive_ai/src/memory/mod.rs`:
```rust
mod store;
mod types;

pub use store::MemoryStore;
pub use types::*;
```

Add `pub mod memory;` to `hive/crates/hive_ai/src/lib.rs`.

Also add `uuid`, `chrono`, `futures`, `tempfile` (dev) dependencies to `Cargo.toml` if not already present.

**Step 5: Run tests**

Run: `cargo test -p hive_ai --test memory_store_tests`
Expected: 4 tests PASS

**Step 6: Commit**
```bash
git add hive/crates/hive_ai/src/memory/ hive/crates/hive_ai/tests/memory_store_tests.rs hive/crates/hive_ai/Cargo.toml
git commit -m "feat(hive_ai): add LanceDB-backed MemoryStore with chunks and memories tables"
```

---

## Task 6: HiveMemory API (Unified Interface)

**Files:**
- Create: `hive/crates/hive_ai/src/memory/hive_memory.rs`
- Modify: `hive/crates/hive_ai/src/memory/mod.rs`
- Test: `hive/crates/hive_ai/tests/hive_memory_tests.rs`

**Step 1: Write the failing test**

Create `hive/crates/hive_ai/tests/hive_memory_tests.rs`:
```rust
use hive_ai::memory::{HiveMemory, MemoryCategory, MemoryEntry};
use hive_ai::embeddings::MockEmbeddingProvider;
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::test]
async fn test_hive_memory_index_and_query_file() {
    let tmp = TempDir::new().unwrap();
    let provider = Arc::new(MockEmbeddingProvider::new(768));
    let memory = HiveMemory::open(tmp.path().join("mem.lance").to_str().unwrap(), provider)
        .await
        .unwrap();

    memory.index_file("src/lib.rs", "pub fn add(a: i32, b: i32) -> i32 { a + b }").await.unwrap();

    let results = memory.query("add function", 5).await.unwrap();
    assert!(!results.chunks.is_empty());
}

#[tokio::test]
async fn test_hive_memory_remember_and_recall() {
    let tmp = TempDir::new().unwrap();
    let provider = Arc::new(MockEmbeddingProvider::new(768));
    let memory = HiveMemory::open(tmp.path().join("mem.lance").to_str().unwrap(), provider)
        .await
        .unwrap();

    memory.remember(MemoryEntry {
        content: "User prefers tabs over spaces".to_string(),
        category: MemoryCategory::UserPreference,
        importance: 7.0,
        conversation_id: "test".to_string(),
        decay_exempt: true,
    }).await.unwrap();

    let results = memory.recall("formatting preferences", 5).await.unwrap();
    assert!(!results.is_empty());
    assert!(results[0].content.contains("tabs"));
}

#[tokio::test]
async fn test_hive_memory_stats() {
    let tmp = TempDir::new().unwrap();
    let provider = Arc::new(MockEmbeddingProvider::new(768));
    let memory = HiveMemory::open(tmp.path().join("mem.lance").to_str().unwrap(), provider)
        .await
        .unwrap();

    let stats = memory.stats().await.unwrap();
    assert_eq!(stats.total_chunks, 0);
    assert_eq!(stats.total_memories, 0);
}
```

**Step 2: Run to verify failure**

Run: `cargo test -p hive_ai --test hive_memory_tests`
Expected: FAIL

**Step 3: Implement HiveMemory**

Create `hive/crates/hive_ai/src/memory/hive_memory.rs`:
```rust
use std::sync::Arc;
use crate::embeddings::EmbeddingProvider;
use super::store::MemoryStore;
use super::types::*;

/// Query result combining chunks and memories
pub struct QueryResult {
    pub chunks: Vec<ChunkResult>,
    pub memories: Vec<MemoryResult>,
}

/// Unified memory API wrapping LanceDB + embeddings
pub struct HiveMemory {
    store: MemoryStore,
    embedder: Arc<dyn EmbeddingProvider>,
}

impl HiveMemory {
    pub async fn open(
        path: &str,
        embedder: Arc<dyn EmbeddingProvider>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let dim = embedder.dimensions();
        let store = MemoryStore::open(path).await?.with_dimensions(dim);
        Ok(Self { store, embedder })
    }

    /// Index a file's content as searchable chunks
    pub async fn index_file(
        &self,
        path: &str,
        content: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let chunks = self.chunk_content(content, 50, 10);

        for (i, chunk) in chunks.iter().enumerate() {
            let embedding = self.embedder.embed(&[chunk.as_str()]).await?;
            if let Some(emb) = embedding.first() {
                let start = (i * 40) as u32; // 50-line chunks, 10-line overlap
                let end = start + chunk.lines().count() as u32;
                self.store.index_chunk(path, chunk, emb, start, end).await?;
            }
        }
        Ok(())
    }

    /// Query both chunks and memories
    pub async fn query(
        &self,
        text: &str,
        max_results: usize,
    ) -> Result<QueryResult, Box<dyn std::error::Error + Send + Sync>> {
        let embedding = self.embedder.embed(&[text]).await?;
        let emb = embedding.first().ok_or("Failed to embed query")?;

        let chunks = self.store.search_chunks(emb, max_results).await?;
        let memories = self.store.recall(emb, max_results).await?;

        Ok(QueryResult { chunks, memories })
    }

    /// Store a durable memory
    pub async fn remember(
        &self,
        entry: MemoryEntry,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let embedding = self.embedder.embed(&[entry.content.as_str()]).await?;
        if let Some(emb) = embedding.first() {
            self.store.remember(entry, emb).await?;
        }
        Ok(())
    }

    /// Recall memories relevant to a query
    pub async fn recall(
        &self,
        text: &str,
        limit: usize,
    ) -> Result<Vec<MemoryResult>, Box<dyn std::error::Error + Send + Sync>> {
        let embedding = self.embedder.embed(&[text]).await?;
        let emb = embedding.first().ok_or("Failed to embed query")?;
        self.store.recall(emb, limit).await
    }

    /// Get store statistics
    pub async fn stats(&self) -> Result<StoreStats, Box<dyn std::error::Error + Send + Sync>> {
        self.store.stats().await
    }

    /// Split content into overlapping chunks
    fn chunk_content(&self, content: &str, chunk_size: usize, overlap: usize) -> Vec<String> {
        let lines: Vec<&str> = content.lines().collect();
        if lines.len() <= chunk_size {
            return vec![content.to_string()];
        }

        let step = chunk_size.saturating_sub(overlap).max(1);
        let mut chunks = Vec::new();
        let mut start = 0;

        while start < lines.len() {
            let end = (start + chunk_size).min(lines.len());
            chunks.push(lines[start..end].join("\n"));
            start += step;
            if end >= lines.len() {
                break;
            }
        }
        chunks
    }
}
```

Update `mod.rs` to export `HiveMemory` and `QueryResult`.

**Step 4: Run tests**

Run: `cargo test -p hive_ai --test hive_memory_tests`
Expected: 3 tests PASS

**Step 5: Commit**
```bash
git add hive/crates/hive_ai/src/memory/
git commit -m "feat(hive_ai): add HiveMemory unified API wrapping LanceDB + embeddings"
```

---

## Task 7: Background Indexer

**Files:**
- Create: `hive/crates/hive_ai/src/memory/indexer.rs`
- Modify: `hive/crates/hive_ai/src/memory/mod.rs`
- Test: `hive/crates/hive_ai/tests/indexer_tests.rs`

**Step 1: Write the failing test**

Create `hive/crates/hive_ai/tests/indexer_tests.rs`:
```rust
use hive_ai::memory::{BackgroundIndexer, HiveMemory};
use hive_ai::embeddings::MockEmbeddingProvider;
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::test]
async fn test_indexer_indexes_directory() {
    let tmp = TempDir::new().unwrap();
    let src_dir = tmp.path().join("src");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::write(src_dir.join("main.rs"), "fn main() { println!(\"hello\"); }").unwrap();
    std::fs::write(src_dir.join("lib.rs"), "pub fn add(a: i32, b: i32) -> i32 { a + b }").unwrap();

    let provider = Arc::new(MockEmbeddingProvider::new(768));
    let memory = Arc::new(
        HiveMemory::open(tmp.path().join("mem.lance").to_str().unwrap(), provider)
            .await
            .unwrap(),
    );

    let indexer = BackgroundIndexer::new(memory.clone());
    let count = indexer.index_directory(src_dir.to_str().unwrap()).await.unwrap();
    assert_eq!(count, 2);

    let stats = memory.stats().await.unwrap();
    assert!(stats.total_chunks >= 2);
}

#[tokio::test]
async fn test_indexer_skips_binary_files() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join("test.rs"), "fn test() {}").unwrap();
    std::fs::write(tmp.path().join("image.png"), &[0x89, 0x50, 0x4E, 0x47]).unwrap();

    let provider = Arc::new(MockEmbeddingProvider::new(768));
    let memory = Arc::new(
        HiveMemory::open(tmp.path().join("mem.lance").to_str().unwrap(), provider)
            .await
            .unwrap(),
    );

    let indexer = BackgroundIndexer::new(memory.clone());
    let count = indexer.index_directory(tmp.path().to_str().unwrap()).await.unwrap();
    assert_eq!(count, 1); // Only the .rs file
}
```

**Step 2: Verify failure, implement, verify pass, commit**

Implement `BackgroundIndexer` with:
- `index_directory(path)` — walks dir, skips binary/hidden/ignored files, calls `memory.index_file()` for each
- `index_single_file(path)` — for incremental updates from file watcher
- File hash tracking (HashMap<PathBuf, u64>) to skip unchanged files
- Uses `hive_fs::is_likely_binary` for binary detection
- Walks directories with `walkdir` or `std::fs::read_dir` recursive

**Step 3: Commit**
```bash
git commit -m "feat(hive_ai): add BackgroundIndexer for directory scanning"
```

---

## Task 8: Wire HiveMemory into Main App

**Files:**
- Modify: `hive/crates/hive_ui_core/src/globals.rs` (add `AppHiveMemory`)
- Modify: `hive/crates/hive_app/src/main.rs` (register global, start indexer)
- Modify: `hive/crates/hive_ui/src/workspace.rs` (use HiveMemory instead of raw RagService)
- Modify: `hive/crates/hive_app/Cargo.toml` (add hive_ai dependency if needed)

**Step 1: Add AppHiveMemory global**

In `hive_ui_core/src/globals.rs`, add:
```rust
pub struct AppHiveMemory(pub Arc<tokio::sync::Mutex<hive_ai::memory::HiveMemory>>);
```

**Step 2: Initialize in main.rs**

In `hive_app/src/main.rs`, after existing RAG initialization:
```rust
// Initialize HiveMemory with LanceDB
let hive_dir = dirs::home_dir().unwrap().join(".hive");
let memory_path = hive_dir.join("memory.lance");
let embedding_provider: Arc<dyn hive_ai::embeddings::EmbeddingProvider> = {
    // Check if OpenAI key is available
    if let Some(key) = &config.openai_api_key {
        if !key.is_empty() {
            Arc::new(hive_ai::embeddings::OpenAiEmbeddings::new(key.clone()))
        } else {
            Arc::new(hive_ai::embeddings::OllamaEmbeddings::new(
                config.ollama_url.clone(),
            ))
        }
    } else {
        Arc::new(hive_ai::embeddings::OllamaEmbeddings::new(
            config.ollama_url.clone(),
        ))
    }
};

// Spawn async init
cx.spawn(|cx| async move {
    if let Ok(memory) = hive_ai::memory::HiveMemory::open(
        memory_path.to_str().unwrap_or_default(),
        embedding_provider,
    ).await {
        let memory = Arc::new(tokio::sync::Mutex::new(memory));
        cx.update(|_, cx| {
            cx.set_global(AppHiveMemory(memory.clone()));
        }).ok();

        // Start background indexer if workspace path is known
        // (hook into workspace open event)
    }
}).detach();
```

**Step 3: Replace RagService query in workspace.rs**

In `hive_ui/src/workspace.rs` around line 1878, replace the raw RagService query with HiveMemory:
```rust
// Replace existing RAG query block with:
if cx.has_global::<AppHiveMemory>() {
    let memory = cx.global::<AppHiveMemory>().0.clone();
    let query_text = user_query_text.clone();
    // Spawn async query
    cx.spawn(|this, mut cx| async move {
        if let Ok(mem) = memory.lock().await {
            if let Ok(result) = mem.query(&query_text, 10).await {
                // Inject chunk results as context
                for chunk in &result.chunks {
                    all_context.push_str(&format!(
                        "// From {}\n{}\n\n",
                        chunk.source_file, chunk.content
                    ));
                }
                // Inject memories as context
                for mem_result in &result.memories {
                    all_context.push_str(&format!(
                        "From previous conversations: {}\n",
                        mem_result.content
                    ));
                }
            }
        }
    }).detach();
}
```

**Step 4: Build and verify**

Run: `cargo check -p hive_app`
Expected: Compiles

**Step 5: Commit**
```bash
git commit -m "feat: wire HiveMemory into main app and workspace context assembly"
```

---

## Task 9: Memory Flush on Compaction

**Files:**
- Create: `hive/crates/hive_ai/src/memory/flush.rs`
- Modify: `hive/crates/hive_core/src/context.rs` (add pre-compaction hook)
- Modify: `hive/crates/hive_ai/src/memory/mod.rs`
- Test: `hive/crates/hive_ai/tests/memory_flush_tests.rs`

**Step 1: Write the failing test**

Create `hive/crates/hive_ai/tests/memory_flush_tests.rs`:
```rust
use hive_ai::memory::flush::{MemoryExtractor, ExtractedMemory};

#[test]
fn test_parse_extracted_memories_from_json() {
    let json = r#"[
        {"content": "User prefers Rust", "importance": 8, "category": "user_preference"},
        {"content": "Auth uses JWT tokens", "importance": 6, "category": "decision"}
    ]"#;

    let memories = MemoryExtractor::parse_response(json).unwrap();
    assert_eq!(memories.len(), 2);
    assert_eq!(memories[0].content, "User prefers Rust");
    assert_eq!(memories[0].importance, 8.0);
}

#[test]
fn test_parse_filters_low_importance() {
    let json = r#"[
        {"content": "Important thing", "importance": 8, "category": "decision"},
        {"content": "Trivial detail", "importance": 3, "category": "general"}
    ]"#;

    let memories = MemoryExtractor::parse_response(json).unwrap();
    let filtered: Vec<_> = memories.into_iter().filter(|m| m.importance >= 5.0).collect();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].content, "Important thing");
}

#[test]
fn test_build_extraction_prompt() {
    let messages = vec![
        "User: How do I fix auth?".to_string(),
        "Assistant: Use JWT tokens with refresh...".to_string(),
    ];
    let prompt = MemoryExtractor::build_prompt(&messages);
    assert!(prompt.contains("extract key memories"));
    assert!(prompt.contains("JSON"));
}
```

**Step 2: Verify failure, implement, verify pass**

Implement `MemoryExtractor`:
- `build_prompt(messages) -> String` — builds the system message for memory extraction
- `parse_response(json_str) -> Result<Vec<ExtractedMemory>>` — parses model response
- Integration: called from `ContextWindow::compact()` before actual compaction

**Step 3: Add pre-compaction hook to ContextWindow**

In `hive_core/src/context.rs`, modify `compact()` to accept an optional callback:
```rust
pub type PreCompactionHook = Box<dyn FnOnce(&[ChatMessage]) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send>;

pub async fn compact_with_flush(&mut self, hook: Option<PreCompactionHook>) {
    if let Some(flush) = hook {
        let messages_to_compact = self.select_messages_for_compaction();
        flush(messages_to_compact).await;
    }
    // Continue with normal compaction...
}
```

**Step 4: Commit**
```bash
git commit -m "feat: add memory flush extraction before context compaction"
```

---

## Task 10: Memory Injection on Message Send

**Files:**
- Modify: `hive/crates/hive_ui/src/workspace.rs` (inject memories into context)

**Step 1: Add memory recall before context assembly**

In workspace.rs, in the context assembly flow (before `build_ai_messages()`):
```rust
// Query HiveMemory for relevant memories based on user message
let mut memory_context = String::new();
if cx.has_global::<AppHiveMemory>() {
    let memory = cx.global::<AppHiveMemory>().0.clone();
    let query = user_message.clone();
    if let Ok(mem) = memory.try_lock() {
        // Non-blocking recall — if lock unavailable, skip
        // (indexer might be running)
    }
    // Or spawn async and inject when ready
}

// Inject as system message if non-empty
if !memory_context.is_empty() {
    system_messages.push(format!(
        "Relevant context from previous conversations:\n{}",
        memory_context
    ));
}
```

**Step 2: Build and verify**

Run: `cargo check -p hive_ui`
Expected: Compiles

**Step 3: Commit**
```bash
git commit -m "feat: inject recalled memories into chat context before message send"
```

---

## Task 11: Skills File-Based CRUD

**Files:**
- Modify: `hive/crates/hive_agents/src/skills.rs` (add file-based CRUD)
- Test: `hive/crates/hive_agents/tests/skills_crud_tests.rs`

**Step 1: Write the failing test**

Create `hive/crates/hive_agents/tests/skills_crud_tests.rs`:
```rust
use hive_agents::skills::{SkillManager, UserSkill};
use tempfile::TempDir;

#[test]
fn test_create_skill_writes_markdown_file() {
    let tmp = TempDir::new().unwrap();
    let mgr = SkillManager::new(tmp.path().to_path_buf());

    let skill = UserSkill {
        name: "code-reviewer".to_string(),
        description: "Reviews code for bugs".to_string(),
        instructions: "You are a code review assistant.\nCheck for bugs.".to_string(),
        enabled: true,
    };

    mgr.create(&skill).unwrap();

    let path = tmp.path().join("code-reviewer.md");
    assert!(path.exists());

    let content = std::fs::read_to_string(&path).unwrap();
    assert!(content.contains("name: code-reviewer"));
    assert!(content.contains("You are a code review assistant"));
}

#[test]
fn test_list_skills_returns_all() {
    let tmp = TempDir::new().unwrap();
    let mgr = SkillManager::new(tmp.path().to_path_buf());

    mgr.create(&UserSkill {
        name: "skill-a".into(), description: "A".into(),
        instructions: "Do A".into(), enabled: true,
    }).unwrap();
    mgr.create(&UserSkill {
        name: "skill-b".into(), description: "B".into(),
        instructions: "Do B".into(), enabled: false,
    }).unwrap();

    let skills = mgr.list().unwrap();
    assert_eq!(skills.len(), 2);
}

#[test]
fn test_update_skill_modifies_file() {
    let tmp = TempDir::new().unwrap();
    let mgr = SkillManager::new(tmp.path().to_path_buf());

    mgr.create(&UserSkill {
        name: "my-skill".into(), description: "v1".into(),
        instructions: "Original".into(), enabled: true,
    }).unwrap();

    mgr.update(&UserSkill {
        name: "my-skill".into(), description: "v2".into(),
        instructions: "Updated".into(), enabled: true,
    }).unwrap();

    let skills = mgr.list().unwrap();
    assert_eq!(skills[0].description, "v2");
    assert_eq!(skills[0].instructions, "Updated");
}

#[test]
fn test_delete_skill_removes_file() {
    let tmp = TempDir::new().unwrap();
    let mgr = SkillManager::new(tmp.path().to_path_buf());

    mgr.create(&UserSkill {
        name: "temp".into(), description: "temp".into(),
        instructions: "temp".into(), enabled: true,
    }).unwrap();

    mgr.delete("temp").unwrap();
    let skills = mgr.list().unwrap();
    assert!(skills.is_empty());
}

#[test]
fn test_toggle_skill_changes_enabled() {
    let tmp = TempDir::new().unwrap();
    let mgr = SkillManager::new(tmp.path().to_path_buf());

    mgr.create(&UserSkill {
        name: "toggler".into(), description: "test".into(),
        instructions: "test".into(), enabled: true,
    }).unwrap();

    mgr.toggle("toggler", false).unwrap();
    let skills = mgr.list().unwrap();
    assert!(!skills[0].enabled);
}
```

**Step 2: Verify failure, implement, verify pass**

Implement `SkillManager` with:
- `new(skills_dir: PathBuf)` — creates `~/.hive/skills/` if not exists
- `create(skill: &UserSkill)` — writes markdown with YAML frontmatter
- `list() -> Vec<UserSkill>` — reads all `.md` files, parses frontmatter
- `update(skill: &UserSkill)` — overwrites existing file
- `delete(name: &str)` — removes file
- `toggle(name: &str, enabled: bool)` — updates enabled in frontmatter
- Security: `scan_for_injection()` on create/update

**Step 3: Commit**
```bash
git commit -m "feat(hive_agents): add SkillManager for file-based skill CRUD"
```

---

## Task 12: Skills Panel UI

**Files:**
- Create: `hive/crates/hive_ui_panels/src/skills_panel.rs`
- Modify: `hive/crates/hive_ui_panels/src/lib.rs` (register panel)
- Modify: `hive/crates/hive_ui_core/src/sidebar.rs` (add sidebar icon)
- Modify: `hive/crates/hive_ui/src/workspace.rs` (register panel in workspace)

**Step 1: Create SkillsPanel GPUI view**

Create `hive/crates/hive_ui_panels/src/skills_panel.rs`:

The panel should implement:
- `SkillsPanel` struct implementing `Render` trait
- Three tabs: My Skills, Installed, Browse
- List view showing skills with name, description, toggle switch, edit button
- "+ New Skill" button at top of My Skills tab
- Skill editor modal with:
  - Name text input
  - Description text input
  - Instructions textarea (markdown)
  - Security scan status indicator
  - Save / Test in Chat / Cancel buttons

Follow existing panel patterns from other panels in `hive_ui_panels/src/` (e.g., files_panel.rs or settings_panel.rs).

**Step 2: Register in sidebar**

Add a skills icon entry in `sidebar.rs` using an appropriate `IconName` variant (e.g., `IconName::Zap` or `IconName::BookOpen`).

**Step 3: Wire to workspace**

Register the panel in workspace panel registry so it can be opened from sidebar.

**Step 4: Build and verify**

Run: `cargo check -p hive_ui_panels`
Expected: Compiles

**Step 5: Commit**
```bash
git commit -m "feat(hive_ui_panels): add Skills panel with creation, editing, and toggle UI"
```

---

## Task 13: Wire Skills Activation to Chat

**Files:**
- Modify: `hive/crates/hive_ui/src/workspace.rs` (skill activation via /command)
- Modify: `hive/crates/hive_ui/src/chat_service.rs` (inject skill instructions)

**Step 1: Implement /skillname command routing**

In workspace.rs message handling, check if message starts with `/`:
```rust
if user_message.starts_with('/') {
    let skill_name = &user_message[1..].split_whitespace().next().unwrap_or("");
    if let Some(skill) = skill_manager.get(skill_name) {
        // Inject skill instructions as system message
        active_skill = Some(skill.instructions.clone());
    }
}
```

**Step 2: Inject active skill into context**

In `build_ai_messages()`, prepend active skill instructions as a system message.

**Step 3: Build and verify**

Run: `cargo check -p hive_ui`
Expected: Compiles

**Step 4: Commit**
```bash
git commit -m "feat: wire skill activation via /command to inject instructions into chat context"
```

---

## Task 14: Integration Test — Full Flow

**Files:**
- Create: `hive/crates/hive_ai/tests/integration_test.rs`

**Step 1: Write end-to-end test**

```rust
use hive_ai::memory::{HiveMemory, MemoryEntry, MemoryCategory};
use hive_ai::memory::flush::MemoryExtractor;
use hive_ai::embeddings::MockEmbeddingProvider;
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::test]
async fn test_full_memory_lifecycle() {
    let tmp = TempDir::new().unwrap();
    let provider = Arc::new(MockEmbeddingProvider::new(768));
    let memory = HiveMemory::open(
        tmp.path().join("test.lance").to_str().unwrap(),
        provider,
    ).await.unwrap();

    // 1. Index some files
    memory.index_file("src/auth.rs", "pub fn login(user: &str, pass: &str) -> Token { ... }").await.unwrap();
    memory.index_file("src/db.rs", "pub fn connect(url: &str) -> Pool { ... }").await.unwrap();

    // 2. Query should find relevant chunks
    let results = memory.query("authentication login", 5).await.unwrap();
    assert!(!results.chunks.is_empty());

    // 3. Remember a preference
    memory.remember(MemoryEntry {
        content: "Auth system uses JWT with 1-hour expiry".to_string(),
        category: MemoryCategory::Decision,
        importance: 8.0,
        conversation_id: "conv-1".to_string(),
        decay_exempt: false,
    }).await.unwrap();

    // 4. Recall should find the memory
    let memories = memory.recall("authentication tokens", 5).await.unwrap();
    assert!(!memories.is_empty());
    assert!(memories[0].content.contains("JWT"));

    // 5. Stats should reflect everything
    let stats = memory.stats().await.unwrap();
    assert!(stats.total_chunks >= 2);
    assert_eq!(stats.total_memories, 1);
}

#[test]
fn test_memory_extraction_prompt_and_parse() {
    let messages = vec![
        "User: We decided to use PostgreSQL for the database".to_string(),
        "Assistant: Good choice. I'll set up the schema with...".to_string(),
    ];

    let prompt = MemoryExtractor::build_prompt(&messages);
    assert!(prompt.contains("extract"));

    // Simulate model response
    let response = r#"[
        {"content": "Database: PostgreSQL chosen for main storage", "importance": 9, "category": "decision"},
        {"content": "Schema setup in progress", "importance": 4, "category": "task_progress"}
    ]"#;

    let extracted = MemoryExtractor::parse_response(response).unwrap();
    let durable: Vec<_> = extracted.into_iter().filter(|m| m.importance >= 5.0).collect();
    assert_eq!(durable.len(), 1);
    assert!(durable[0].content.contains("PostgreSQL"));
}
```

**Step 2: Run full test suite**

Run: `cargo test -p hive_ai`
Expected: All tests PASS

**Step 3: Run workspace tests**

Run: `cargo test --workspace --exclude hive_app`
Expected: All tests PASS (no regressions)

**Step 4: Commit**
```bash
git commit -m "test: add integration tests for full memory lifecycle"
```

---

## Task 15: Final Wiring & Polish

**Files:**
- Modify: `hive/crates/hive_app/src/main.rs` (start background indexer on workspace open)
- Modify: `hive/crates/hive_ui/src/workspace.rs` (hook file watcher to indexer)
- Modify: Various (cleanup, error handling)

**Step 1: Start indexer on workspace open**

When a workspace directory is set, spawn the background indexer:
```rust
// In workspace open handler:
if let Some(workspace_path) = &self.workspace_path {
    let memory = cx.global::<AppHiveMemory>().0.clone();
    let path = workspace_path.clone();
    cx.spawn(|_, _| async move {
        if let Ok(mem) = memory.lock().await {
            let indexer = BackgroundIndexer::new(Arc::new(mem));
            indexer.index_directory(&path).await.ok();
        }
    }).detach();
}
```

**Step 2: Hook file watcher for incremental updates**

Subscribe to hive_fs file watcher events and re-index changed files.

**Step 3: Full build**

Run: `cargo build -p hive_app`
Expected: Compiles

**Step 4: Final commit**
```bash
git commit -m "feat: wire background indexer to workspace open and file watcher"
```

---

## Summary

| Task | Description | Depends On |
|------|-------------|------------|
| 1 | Add LanceDB + Arrow deps | — |
| 2 | EmbeddingProvider trait + mock | 1 |
| 3 | OpenAI embedding provider | 2 |
| 4 | Ollama embedding provider | 2 |
| 5 | LanceDB MemoryStore | 1 |
| 6 | HiveMemory unified API | 2, 5 |
| 7 | Background indexer | 6 |
| 8 | Wire into main app | 6, 7 |
| 9 | Memory flush on compaction | 6 |
| 10 | Memory injection on send | 8 |
| 11 | Skills file-based CRUD | — |
| 12 | Skills panel UI | 11 |
| 13 | Wire skills to chat | 12 |
| 14 | Integration tests | 6, 9 |
| 15 | Final wiring & polish | all |

Tasks 1-10 are System 1 + System 2 (sequential).
Tasks 11-13 are System 3 (can run in parallel with 1-10).
Tasks 14-15 are integration and polish.
