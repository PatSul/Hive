#[cfg(feature = "memory-tiering")]
mod vector_bridge_tests {
    use hive_agents::memory::vector_bridge::*;

    #[tokio::test]
    async fn test_mock_bridge_empty_query() {
        let bridge = MockVectorBridge::new();
        let results = bridge.query_vectors("anything", 10).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_mock_bridge_store_and_query() {
        let bridge = MockVectorBridge::new();
        bridge
            .store_vector("rust async patterns", "code_pattern", 0.8)
            .await
            .unwrap();
        bridge
            .store_vector("user prefers dark theme", "user_preference", 0.5)
            .await
            .unwrap();
        bridge
            .store_vector("rust error handling", "code_pattern", 0.9)
            .await
            .unwrap();

        // Query for "rust" should match 2 of the 3 entries.
        let results = bridge.query_vectors("rust", 10).await.unwrap();
        assert_eq!(results.len(), 2);
        for r in &results {
            assert!(r.content.to_lowercase().contains("rust"));
        }

        // Query for "theme" should match exactly 1.
        let results = bridge.query_vectors("theme", 10).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].content, "user prefers dark theme");
        assert_eq!(results[0].category, "user_preference");
    }

    #[tokio::test]
    async fn test_mock_bridge_query_limit() {
        let bridge = MockVectorBridge::new();
        for i in 0..5 {
            bridge
                .store_vector(&format!("item {i}"), "general", 0.5)
                .await
                .unwrap();
        }

        // All 5 contain "item", but limit to 2.
        let results = bridge.query_vectors("item", 2).await.unwrap();
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn test_mock_bridge_stats() {
        let bridge = MockVectorBridge::new();
        bridge
            .store_vector("memory one", "general", 0.5)
            .await
            .unwrap();
        bridge
            .store_vector("memory two", "general", 0.7)
            .await
            .unwrap();

        let stats = bridge.stats().await.unwrap();
        // store_vector always creates Memory-sourced entries.
        assert_eq!(stats.total_memories, 2);
        assert_eq!(stats.total_chunks, 0);
        assert_eq!(stats.indexed_files, 0);

        // Now add a chunk-sourced entry via with_entries and verify stats.
        let bridge2 = MockVectorBridge::with_entries(vec![
            VectorResult {
                content: "chunk content".into(),
                category: "chunk".into(),
                score: 0.9,
                source: VectorSource::Chunk {
                    file: "src/main.rs".into(),
                    start_line: 0,
                    end_line: 10,
                },
            },
            VectorResult {
                content: "a memory".into(),
                category: "general".into(),
                score: 0.8,
                source: VectorSource::Memory {
                    importance: 0.6,
                    timestamp: "2026-03-23T00:00:00Z".into(),
                },
            },
        ]);

        let stats2 = bridge2.stats().await.unwrap();
        assert_eq!(stats2.total_chunks, 1);
        assert_eq!(stats2.total_memories, 1);
    }

    #[tokio::test]
    async fn test_vector_result_serialization() {
        // Round-trip a VectorResult with a Chunk source.
        let chunk_result = VectorResult {
            content: "fn main() {}".into(),
            category: "chunk".into(),
            score: 0.95,
            source: VectorSource::Chunk {
                file: "src/main.rs".into(),
                start_line: 1,
                end_line: 3,
            },
        };
        let json = serde_json::to_string(&chunk_result).unwrap();
        let deserialized: VectorResult = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.content, "fn main() {}");
        assert_eq!(deserialized.category, "chunk");
        assert!((deserialized.score - 0.95).abs() < f32::EPSILON);
        match &deserialized.source {
            VectorSource::Chunk {
                file,
                start_line,
                end_line,
            } => {
                assert_eq!(file, "src/main.rs");
                assert_eq!(*start_line, 1);
                assert_eq!(*end_line, 3);
            }
            _ => panic!("Expected Chunk source"),
        }

        // Round-trip a VectorResult with a Memory source.
        let memory_result = VectorResult {
            content: "user likes dark mode".into(),
            category: "user_preference".into(),
            score: 0.88,
            source: VectorSource::Memory {
                importance: 0.75,
                timestamp: "2026-03-23T12:00:00Z".into(),
            },
        };
        let json = serde_json::to_string(&memory_result).unwrap();
        let deserialized: VectorResult = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.content, "user likes dark mode");
        assert_eq!(deserialized.category, "user_preference");
        assert!((deserialized.score - 0.88).abs() < f32::EPSILON);
        match &deserialized.source {
            VectorSource::Memory {
                importance,
                timestamp,
            } => {
                assert!((*importance - 0.75).abs() < f32::EPSILON);
                assert_eq!(timestamp, "2026-03-23T12:00:00Z");
            }
            _ => panic!("Expected Memory source"),
        }
    }
}
