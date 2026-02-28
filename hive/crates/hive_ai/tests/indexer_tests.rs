use hive_ai::embeddings::MockEmbeddingProvider;
use hive_ai::memory::{BackgroundIndexer, HiveMemory};
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::test]
async fn test_indexer_indexes_directory() {
    let tmp = TempDir::new().unwrap();
    let src_dir = tmp.path().join("src");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::write(
        src_dir.join("main.rs"),
        "fn main() { println!(\"hello\"); }",
    )
    .unwrap();
    std::fs::write(
        src_dir.join("lib.rs"),
        "pub fn add(a: i32, b: i32) -> i32 { a + b }",
    )
    .unwrap();

    let provider = Arc::new(MockEmbeddingProvider::new(768));
    let memory = Arc::new(
        HiveMemory::open(tmp.path().join("mem.lance").to_str().unwrap(), provider)
            .await
            .unwrap(),
    );

    let mut indexer = BackgroundIndexer::new(memory.clone());
    let count = indexer
        .index_directory(src_dir.to_str().unwrap())
        .await
        .unwrap();
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

    let mut indexer = BackgroundIndexer::new(memory.clone());
    let count = indexer
        .index_directory(tmp.path().to_str().unwrap())
        .await
        .unwrap();
    assert_eq!(count, 1); // Only the .rs file
}
