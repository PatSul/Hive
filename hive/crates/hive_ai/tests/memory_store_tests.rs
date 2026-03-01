use hive_ai::memory::{MemoryCategory, MemoryEntry, MemoryStore};
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

    store
        .index_chunk(
            "src/main.rs",
            "fn main() { println!(\"Hello\"); }",
            &[0.5; 768],
            0,
            1,
        )
        .await
        .unwrap();

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
