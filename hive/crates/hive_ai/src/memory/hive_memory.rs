use std::sync::Arc;

use crate::embeddings::EmbeddingProvider;
use super::store::MemoryStore;
use super::types::*;

type BoxErr = Box<dyn std::error::Error + Send + Sync>;

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
    ) -> Result<Self, BoxErr> {
        let dim = embedder.dimensions();
        let store = MemoryStore::open(path).await?.with_dimensions(dim);
        Ok(Self { store, embedder })
    }

    /// Index a file's content as searchable chunks
    pub async fn index_file(
        &self,
        path: &str,
        content: &str,
    ) -> Result<(), BoxErr> {
        let chunks = self.chunk_content(content, 50, 10);

        for (i, chunk) in chunks.iter().enumerate() {
            let embedding = self.embedder.embed(&[chunk.as_str()]).await
                .map_err(|e| -> BoxErr { Box::new(e) })?;
            if let Some(emb) = embedding.first() {
                let start = (i * 40) as u32;
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
    ) -> Result<QueryResult, BoxErr> {
        let embedding = self.embedder.embed(&[text]).await
            .map_err(|e| -> BoxErr { Box::new(e) })?;
        let emb = embedding.first().ok_or::<BoxErr>("Failed to embed query".into())?;

        let chunks = self.store.search_chunks(emb, max_results).await?;
        let memories = self.store.recall(emb, max_results).await?;

        Ok(QueryResult { chunks, memories })
    }

    /// Store a durable memory
    pub async fn remember(
        &self,
        entry: MemoryEntry,
    ) -> Result<(), BoxErr> {
        let embedding = self.embedder.embed(&[entry.content.as_str()]).await
            .map_err(|e| -> BoxErr { Box::new(e) })?;
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
    ) -> Result<Vec<MemoryResult>, BoxErr> {
        let embedding = self.embedder.embed(&[text]).await
            .map_err(|e| -> BoxErr { Box::new(e) })?;
        let emb = embedding.first().ok_or::<BoxErr>("Failed to embed query".into())?;
        self.store.recall(emb, limit).await
    }

    /// Get store statistics
    pub async fn stats(&self) -> Result<StoreStats, BoxErr> {
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
