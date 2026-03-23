use hive_ai::embeddings::MockEmbeddingProvider;
use hive_ai::memory::flush::MemoryExtractor;
use hive_ai::memory::{HiveMemory, MemoryCategory, MemoryEntry};
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::test]
async fn test_full_memory_lifecycle() {
    let tmp = TempDir::new().unwrap();
    let provider = Arc::new(MockEmbeddingProvider::new(768));
    let memory = HiveMemory::open(tmp.path().join("test.lance").to_str().unwrap(), provider)
        .await
        .unwrap();

    // 1. Index some files
    memory
        .index_file(
            "src/auth.rs",
            "pub fn login(user: &str, pass: &str) -> Token { validate(user, pass) }",
        )
        .await
        .unwrap();
    memory
        .index_file(
            "src/db.rs",
            "pub fn connect(url: &str) -> Pool { Pool::new(url) }",
        )
        .await
        .unwrap();

    // 2. Query should find relevant chunks
    let results = memory.query("authentication login", 5).await.unwrap();
    assert!(
        !results.chunks.is_empty(),
        "Should find indexed code chunks"
    );

    // 3. Remember a preference
    memory
        .remember(MemoryEntry {
            content: "Auth system uses JWT with 1-hour expiry".to_string(),
            category: MemoryCategory::Decision,
            importance: 8.0,
            conversation_id: "conv-1".to_string(),
            decay_exempt: false,
        })
        .await
        .unwrap();

    // 4. Recall should find the memory
    let memories = memory.recall("authentication tokens", 5).await.unwrap();
    assert!(!memories.is_empty(), "Should recall stored memories");
    assert!(
        memories[0].content.contains("JWT"),
        "Recalled memory should contain JWT"
    );

    // 5. Stats should reflect everything
    let stats = memory.stats().await.unwrap();
    assert!(
        stats.total_chunks >= 2,
        "Should have at least 2 chunks from indexed files"
    );
    assert_eq!(stats.total_memories, 1, "Should have exactly 1 memory");
}

#[test]
fn test_memory_extraction_prompt_and_parse() {
    let messages = vec![
        "User: We decided to use PostgreSQL for the database".to_string(),
        "Assistant: Good choice. I'll set up the schema with migrations...".to_string(),
    ];

    let prompt = MemoryExtractor::build_prompt(&messages);
    assert!(
        prompt.contains("extract"),
        "Prompt should mention extraction"
    );
    assert!(
        prompt.contains("PostgreSQL"),
        "Prompt should include conversation content"
    );

    // Simulate model response
    let response = r#"[
        {"content": "Database: PostgreSQL chosen for main storage", "importance": 9, "category": "decision"},
        {"content": "Schema setup in progress", "importance": 4, "category": "task_progress"}
    ]"#;

    let extracted = MemoryExtractor::parse_response(response).unwrap();
    assert_eq!(extracted.len(), 2, "Should parse both memories");

    let durable = MemoryExtractor::filter_by_importance(extracted, 5.0);
    assert_eq!(durable.len(), 1, "Only high-importance memories survive");
    assert!(
        durable[0].content.contains("PostgreSQL"),
        "Durable memory should mention PostgreSQL"
    );
}

#[tokio::test]
async fn test_multiple_memories_and_recall_ordering() {
    let tmp = TempDir::new().unwrap();
    let provider = Arc::new(MockEmbeddingProvider::new(768));
    let memory = HiveMemory::open(tmp.path().join("test.lance").to_str().unwrap(), provider)
        .await
        .unwrap();

    // Store multiple memories with different importance
    for (content, importance) in [
        ("User prefers tabs over spaces", 7.0),
        ("Project uses Rust for all backend services", 9.0),
        ("Team decided on PostgreSQL database", 8.0),
    ] {
        memory
            .remember(MemoryEntry {
                content: content.to_string(),
                category: MemoryCategory::Decision,
                importance,
                conversation_id: "test".to_string(),
                decay_exempt: false,
            })
            .await
            .unwrap();
    }

    let stats = memory.stats().await.unwrap();
    assert_eq!(stats.total_memories, 3);

    let recalled = memory
        .recall("backend technology decisions", 10)
        .await
        .unwrap();
    assert_eq!(recalled.len(), 3, "Should recall all stored memories");
}

#[tokio::test]
async fn test_indexer_and_query_workflow() {
    use hive_ai::memory::BackgroundIndexer;

    let tmp = TempDir::new().unwrap();

    // Create test source files
    let src_dir = tmp.path().join("project");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::write(
        src_dir.join("main.rs"),
        "fn main() {\n    println!(\"hello world\");\n}\n",
    )
    .unwrap();
    std::fs::write(
        src_dir.join("lib.rs"),
        "pub fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n\npub fn multiply(a: i32, b: i32) -> i32 {\n    a * b\n}\n",
    )
    .unwrap();

    let provider = Arc::new(MockEmbeddingProvider::new(768));
    let memory = Arc::new(
        HiveMemory::open(tmp.path().join("idx.lance").to_str().unwrap(), provider)
            .await
            .unwrap(),
    );

    let mut indexer = BackgroundIndexer::new(memory.clone());
    let count = indexer
        .index_directory(src_dir.to_str().unwrap())
        .await
        .unwrap();
    assert_eq!(count, 2, "Should index both .rs files");

    // Query the indexed content
    let results = memory.query("addition function", 5).await.unwrap();
    assert!(
        !results.chunks.is_empty(),
        "Should find chunks from indexed files"
    );
}
