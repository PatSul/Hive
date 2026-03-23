use hive_ai::embeddings::MockEmbeddingProvider;
use hive_ai::memory::{HiveMemory, MemoryCategory, MemoryEntry};
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::test]
async fn test_hive_memory_index_and_query_file() {
    let tmp = TempDir::new().unwrap();
    let provider = Arc::new(MockEmbeddingProvider::new(768));
    let memory = HiveMemory::open(tmp.path().join("mem.lance").to_str().unwrap(), provider)
        .await
        .unwrap();

    memory
        .index_file("src/lib.rs", "pub fn add(a: i32, b: i32) -> i32 { a + b }")
        .await
        .unwrap();

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

    memory
        .remember(MemoryEntry {
            content: "User prefers tabs over spaces".to_string(),
            category: MemoryCategory::UserPreference,
            importance: 7.0,
            conversation_id: "test".to_string(),
            decay_exempt: true,
        })
        .await
        .unwrap();

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
