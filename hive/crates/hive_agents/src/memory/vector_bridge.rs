//! L2 WARM: Vector memory bridge to `hive_ai::memory::HiveMemory`.
//!
//! Provides a trait-based abstraction over vector/semantic memory operations,
//! with a concrete implementation that delegates to [`hive_ai::memory::HiveMemory`]
//! and a mock implementation for testing.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Describes where a vector search result originated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VectorSource {
    /// Result came from an indexed code chunk.
    Chunk {
        /// Source file path.
        file: String,
        /// First line of the chunk (0-based).
        start_line: u32,
        /// Last line of the chunk (exclusive, 0-based).
        end_line: u32,
    },
    /// Result came from a stored memory entry.
    Memory {
        /// Importance score assigned at storage time.
        importance: f32,
        /// ISO-8601 timestamp of the original memory.
        timestamp: String,
    },
}

/// A single result returned by a vector memory query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorResult {
    /// The textual content of the result.
    pub content: String,
    /// Category label (e.g. `"code_pattern"`, `"general"`).
    pub category: String,
    /// Cosine-similarity score in `[0, 1]`.
    pub score: f32,
    /// Where this result originated.
    pub source: VectorSource,
}

/// Aggregate statistics about the vector store.
#[derive(Debug, Clone, Default)]
pub struct VectorStats {
    /// Number of indexed code chunks.
    pub total_chunks: usize,
    /// Number of stored memory entries.
    pub total_memories: usize,
    /// Number of distinct files that have been indexed.
    pub indexed_files: usize,
}

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

/// Abstraction over vector/semantic memory operations.
///
/// Implementations may delegate to a real embedding store (see
/// [`HiveVectorBridge`]) or provide canned data for tests (see
/// [`MockVectorBridge`]).
#[async_trait]
pub trait VectorMemoryBridge: Send + Sync {
    /// Query the vector store for results semantically similar to `text`.
    ///
    /// Returns at most `limit` results, sorted by relevance (highest score
    /// first).
    async fn query_vectors(&self, text: &str, limit: usize) -> Result<Vec<VectorResult>, String>;

    /// Store a new memory vector with the given `category` and `importance`.
    async fn store_vector(
        &self,
        content: &str,
        category: &str,
        importance: f32,
    ) -> Result<(), String>;

    /// Return aggregate statistics about the underlying store.
    async fn stats(&self) -> Result<VectorStats, String>;
}

// ---------------------------------------------------------------------------
// HiveVectorBridge (concrete)
// ---------------------------------------------------------------------------

/// Concrete bridge that delegates to [`hive_ai::memory::HiveMemory`].
pub struct HiveVectorBridge {
    memory: Arc<tokio::sync::Mutex<hive_ai::memory::HiveMemory>>,
}

impl HiveVectorBridge {
    /// Create a new bridge wrapping the given shared `HiveMemory` instance.
    pub fn new(memory: Arc<tokio::sync::Mutex<hive_ai::memory::HiveMemory>>) -> Self {
        Self { memory }
    }
}

/// Map a category string to the corresponding `hive_ai::memory::MemoryCategory` variant.
///
/// Falls back to `General` for unrecognised strings.
fn parse_ai_category(s: &str) -> hive_ai::memory::MemoryCategory {
    match s {
        "user_preference" => hive_ai::memory::MemoryCategory::UserPreference,
        "code_pattern" => hive_ai::memory::MemoryCategory::CodePattern,
        "task_progress" => hive_ai::memory::MemoryCategory::TaskProgress,
        "decision" => hive_ai::memory::MemoryCategory::Decision,
        _ => hive_ai::memory::MemoryCategory::General,
    }
}

#[async_trait]
impl VectorMemoryBridge for HiveVectorBridge {
    async fn query_vectors(&self, text: &str, limit: usize) -> Result<Vec<VectorResult>, String> {
        let mem = self.memory.lock().await;
        let qr = mem.query(text, limit).await.map_err(|e| e.to_string())?;

        let mut results = Vec::with_capacity(qr.chunks.len() + qr.memories.len());

        for chunk in qr.chunks {
            results.push(VectorResult {
                content: chunk.content,
                category: "chunk".to_string(),
                score: chunk.score,
                source: VectorSource::Chunk {
                    file: chunk.source_file,
                    start_line: chunk.start_line,
                    end_line: chunk.end_line,
                },
            });
        }

        for mem_result in qr.memories {
            results.push(VectorResult {
                content: mem_result.content,
                category: mem_result.category,
                score: mem_result.score,
                source: VectorSource::Memory {
                    importance: mem_result.importance,
                    timestamp: mem_result.timestamp,
                },
            });
        }

        // Sort by score descending so callers always get most-relevant first.
        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(limit);

        Ok(results)
    }

    async fn store_vector(
        &self,
        content: &str,
        category: &str,
        importance: f32,
    ) -> Result<(), String> {
        let entry = hive_ai::memory::MemoryEntry {
            content: content.to_string(),
            category: parse_ai_category(category),
            importance,
            conversation_id: String::new(),
            decay_exempt: false,
        };
        let mem = self.memory.lock().await;
        mem.remember(entry).await.map_err(|e| e.to_string())
    }

    async fn stats(&self) -> Result<VectorStats, String> {
        let mem = self.memory.lock().await;
        let s = mem.stats().await.map_err(|e| e.to_string())?;
        Ok(VectorStats {
            total_chunks: s.total_chunks,
            total_memories: s.total_memories,
            indexed_files: s.indexed_files,
        })
    }
}

// ---------------------------------------------------------------------------
// MockVectorBridge (for testing)
// ---------------------------------------------------------------------------

/// In-memory mock of [`VectorMemoryBridge`] for unit tests.
///
/// Supports simple substring matching rather than real embeddings.
pub struct MockVectorBridge {
    memories: tokio::sync::Mutex<Vec<VectorResult>>,
}

impl MockVectorBridge {
    /// Create an empty mock bridge.
    pub fn new() -> Self {
        Self {
            memories: tokio::sync::Mutex::new(Vec::new()),
        }
    }

    /// Create a mock bridge pre-populated with the given entries.
    pub fn with_entries(entries: Vec<VectorResult>) -> Self {
        Self {
            memories: tokio::sync::Mutex::new(entries),
        }
    }
}

impl Default for MockVectorBridge {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl VectorMemoryBridge for MockVectorBridge {
    async fn query_vectors(&self, text: &str, limit: usize) -> Result<Vec<VectorResult>, String> {
        let store = self.memories.lock().await;
        let lower_query = text.to_lowercase();

        let mut matches: Vec<VectorResult> = store
            .iter()
            .filter(|r| r.content.to_lowercase().contains(&lower_query))
            .cloned()
            .collect();

        // Sort by score descending.
        matches
            .sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        matches.truncate(limit);

        Ok(matches)
    }

    async fn store_vector(
        &self,
        content: &str,
        category: &str,
        importance: f32,
    ) -> Result<(), String> {
        let result = VectorResult {
            content: content.to_string(),
            category: category.to_string(),
            score: 1.0,
            source: VectorSource::Memory {
                importance,
                timestamp: chrono::Utc::now().to_rfc3339(),
            },
        };
        self.memories.lock().await.push(result);
        Ok(())
    }

    async fn stats(&self) -> Result<VectorStats, String> {
        let store = self.memories.lock().await;
        let chunk_count = store
            .iter()
            .filter(|r| matches!(r.source, VectorSource::Chunk { .. }))
            .count();
        let memory_count = store
            .iter()
            .filter(|r| matches!(r.source, VectorSource::Memory { .. }))
            .count();
        Ok(VectorStats {
            total_chunks: chunk_count,
            total_memories: memory_count,
            indexed_files: 0,
        })
    }
}
