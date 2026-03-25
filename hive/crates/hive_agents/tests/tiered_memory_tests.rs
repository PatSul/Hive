#[cfg(feature = "memory-tiering")]
mod tiered_memory_tests {
    use hive_agents::collective_memory::{CollectiveMemory, MemoryCategory, MemoryEntry};
    use hive_agents::memory::archive::ArchiveService;
    use hive_agents::memory::session_state::SessionState;
    use hive_agents::memory::tiered_memory::*;
    use hive_agents::memory::vector_bridge::{MockVectorBridge, VectorResult, VectorSource};
    use hive_agents::memory::TargetLayer;
    use std::sync::Arc;
    use tempfile::TempDir;

    /// Helper: build a `TieredMemory` with in-memory backends.
    fn make_tiered(
        vectors: Option<Arc<dyn hive_agents::memory::vector_bridge::VectorMemoryBridge>>,
    ) -> (TieredMemory, TempDir) {
        let dir = TempDir::new().unwrap();
        let session = SessionState::new("test-session".into());
        let collective =
            Arc::new(CollectiveMemory::in_memory().expect("in-memory db should work"));
        let archive = ArchiveService::new(dir.path().to_path_buf());
        let tm = TieredMemory::new(session, vectors, collective, archive);
        (tm, dir)
    }

    #[tokio::test]
    async fn test_query_hot_layer() {
        let (mut tm, _dir) = make_tiered(None);

        // Add decisions to the session.
        tm.session_mut()
            .log_decision("Use SQLite for storage", Some("lightweight"));
        tm.session_mut()
            .log_decision("Choose async runtime tokio", None);

        let q = MemoryQuery {
            text: "sqlite".to_string(),
            max_results: 10,
            layers: vec![TargetLayer::Hot],
            recency_bias: 0.0,
        };

        let result = tm.query(&q).await.unwrap();
        assert!(
            !result.entries.is_empty(),
            "should find decisions matching 'sqlite'"
        );

        // The matched entry should be from the Hot layer.
        for entry in &result.entries {
            assert_eq!(entry.source_layer, TargetLayer::Hot);
            assert!(
                entry.content.to_lowercase().contains("sqlite"),
                "content should contain 'sqlite': {}",
                entry.content
            );
        }

        // Score should be 0.9 (no recency bias).
        assert!(
            (result.entries[0].score - 0.9).abs() < 0.01,
            "hot score should be 0.9, got: {}",
            result.entries[0].score
        );
    }

    #[tokio::test]
    async fn test_query_hot_layer_entities() {
        let (mut tm, _dir) = make_tiered(None);

        // Add entities to the session.
        tm.session_mut().touch_entity("config.rs", "file");
        tm.session_mut().touch_entity("parse_config", "function");

        let q = MemoryQuery {
            text: "config".to_string(),
            max_results: 10,
            layers: vec![TargetLayer::Hot],
            recency_bias: 0.0,
        };

        let result = tm.query(&q).await.unwrap();
        assert_eq!(
            result.entries.len(),
            2,
            "should find both entities matching 'config': {:#?}",
            result.entries
        );
    }

    #[tokio::test]
    async fn test_query_cold_layer() {
        let (_unused_tm, _dir) = make_tiered(None);

        // Insert memories into CollectiveMemory.
        let entry1 = MemoryEntry::new(MemoryCategory::CodePattern, "Rust pattern matching is powerful");
        let entry2 = MemoryEntry::new(MemoryCategory::General, "Python is also useful");

        // Build a separate TieredMemory with the collective we can populate.
        let collective =
            Arc::new(CollectiveMemory::in_memory().expect("in-memory db should work"));
        collective.remember(&entry1).unwrap();
        collective.remember(&entry2).unwrap();

        let dir = TempDir::new().unwrap();
        let session = SessionState::new("cold-test".into());
        let archive = ArchiveService::new(dir.path().to_path_buf());
        let tm = TieredMemory::new(session, None, collective, archive);

        let q = MemoryQuery {
            text: "rust".to_string(),
            max_results: 10,
            layers: vec![TargetLayer::Cold],
            recency_bias: 0.0,
        };

        let result = tm.query(&q).await.unwrap();
        assert!(
            !result.entries.is_empty(),
            "should find entries matching 'rust'"
        );
        assert_eq!(result.entries[0].source_layer, TargetLayer::Cold);
        assert!(
            result.entries[0].content.to_lowercase().contains("rust"),
            "content should contain 'rust': {}",
            result.entries[0].content
        );
    }

    #[tokio::test]
    async fn test_query_warm_layer() {
        let bridge = MockVectorBridge::with_entries(vec![
            VectorResult {
                content: "async/await patterns in Rust".into(),
                category: "code_pattern".into(),
                score: 0.92,
                source: VectorSource::Memory {
                    importance: 0.8,
                    timestamp: "2026-03-23T10:00:00Z".into(),
                },
            },
            VectorResult {
                content: "error handling with anyhow".into(),
                category: "code_pattern".into(),
                score: 0.85,
                source: VectorSource::Memory {
                    importance: 0.7,
                    timestamp: "2026-03-23T11:00:00Z".into(),
                },
            },
        ]);

        let dir = TempDir::new().unwrap();
        let session = SessionState::new("warm-test".into());
        let collective =
            Arc::new(CollectiveMemory::in_memory().expect("in-memory db should work"));
        let archive = ArchiveService::new(dir.path().to_path_buf());
        let tm = TieredMemory::new(
            session,
            Some(Arc::new(bridge)),
            collective,
            archive,
        );

        let q = MemoryQuery {
            text: "async".to_string(),
            max_results: 10,
            layers: vec![TargetLayer::Warm],
            recency_bias: 0.0,
        };

        let result = tm.query(&q).await.unwrap();
        assert_eq!(
            result.entries.len(),
            1,
            "mock bridge should find 1 entry matching 'async': {:#?}",
            result.entries
        );
        assert_eq!(result.entries[0].source_layer, TargetLayer::Warm);

        // Score should be the vector similarity (0.92) with no recency bias.
        assert!(
            (result.entries[0].score - 0.92).abs() < 0.01,
            "warm score should be ~0.92, got: {}",
            result.entries[0].score
        );
    }

    #[tokio::test]
    async fn test_flush_session_dispatches() {
        let collective =
            Arc::new(CollectiveMemory::in_memory().expect("in-memory db should work"));
        let dir = TempDir::new().unwrap();
        let session = SessionState::new("flush-test".into());
        let archive = ArchiveService::new(dir.path().to_path_buf());
        let mut tm = TieredMemory::new(session, None, collective.clone(), archive);

        // Queue writes targeting Cold.
        tm.session_mut().queue_write(
            "learned batch inserts are fast".into(),
            MemoryCategory::CodePattern,
            0.9,
            TargetLayer::Cold,
        );
        tm.session_mut().queue_write(
            "user prefers dark mode".into(),
            MemoryCategory::UserPreference,
            0.6,
            TargetLayer::Cold,
        );

        // Flush.
        let flushed = tm.flush_session().await.unwrap();
        assert_eq!(flushed, 2, "should have flushed 2 writes");

        // Pending writes should be drained.
        assert!(
            tm.session().pending_memory_writes.is_empty(),
            "pending writes should be empty after flush"
        );

        // Verify entries appeared in CollectiveMemory.
        let cold_entries = collective.recall("batch", None, None, 10).unwrap();
        assert!(
            !cold_entries.is_empty(),
            "cold layer should contain the flushed entry"
        );
        assert!(
            cold_entries[0].content.contains("batch inserts"),
            "entry content mismatch: {}",
            cold_entries[0].content
        );
    }

    #[tokio::test]
    async fn test_deduplication() {
        let collective =
            Arc::new(CollectiveMemory::in_memory().expect("in-memory db should work"));

        // Insert near-duplicate entries into CollectiveMemory.
        let entry1 = MemoryEntry::new(
            MemoryCategory::General,
            "batch inserts are very fast for large datasets",
        );
        let entry2 = MemoryEntry::new(
            MemoryCategory::General,
            "batch inserts are very fast for large datasets indeed",
        );
        collective.remember(&entry1).unwrap();
        collective.remember(&entry2).unwrap();

        let dir = TempDir::new().unwrap();
        let mut session = SessionState::new("dedup-test".into());
        // Also add a near-duplicate to Hot layer.
        session.log_decision(
            "batch inserts are very fast for large datasets",
            None,
        );

        let archive = ArchiveService::new(dir.path().to_path_buf());
        let tm = TieredMemory::new(session, None, collective, archive);

        let q = MemoryQuery {
            text: "batch".to_string(),
            max_results: 10,
            layers: vec![TargetLayer::Hot, TargetLayer::Cold],
            recency_bias: 0.0,
        };

        let result = tm.query(&q).await.unwrap();

        // Due to deduplication (>80% Jaccard overlap), we should get fewer
        // results than the 3 we inserted.
        assert!(
            result.entries.len() < 3,
            "dedup should remove near-duplicates, got {} entries: {:#?}",
            result.entries.len(),
            result.entries
        );
        // Should keep at least one.
        assert!(
            !result.entries.is_empty(),
            "should keep at least one entry after dedup"
        );
    }

    #[tokio::test]
    async fn test_new_session_resets() {
        let (mut tm, _dir) = make_tiered(None);

        // Populate the session.
        tm.session_mut()
            .log_decision("important decision", None);
        tm.session_mut().touch_entity("main.rs", "file");
        tm.session_mut().set_task(Some("implement feature".into()));
        tm.session_mut().add_context("context item".into());
        tm.session_mut().queue_write(
            "pending write".into(),
            MemoryCategory::General,
            0.5,
            TargetLayer::Cold,
        );

        assert!(!tm.session().decisions_log.is_empty());
        assert!(!tm.session().entity_cache.is_empty());
        assert!(tm.session().current_task.is_some());

        // Reset session.
        tm.new_session("new-session-id".into());

        assert_eq!(tm.session().session_id, "new-session-id");
        assert!(tm.session().decisions_log.is_empty());
        assert!(tm.session().entity_cache.is_empty());
        assert!(tm.session().current_task.is_none());
        assert!(tm.session().active_context.is_empty());
        assert!(tm.session().pending_memory_writes.is_empty());
    }

    #[tokio::test]
    async fn test_recency_bias() {
        let collective =
            Arc::new(CollectiveMemory::in_memory().expect("in-memory db should work"));
        let entry = MemoryEntry::new(MemoryCategory::General, "test recency content");
        collective.remember(&entry).unwrap();

        let dir = TempDir::new().unwrap();
        let mut session = SessionState::new("recency-test".into());
        session.log_decision("test recency content decision", None);

        let archive = ArchiveService::new(dir.path().to_path_buf());
        let tm = TieredMemory::new(session, None, collective, archive);

        // Query with no recency bias — Hot and Cold should have their base scores.
        let q_no_bias = MemoryQuery {
            text: "recency".to_string(),
            max_results: 10,
            layers: vec![TargetLayer::Hot, TargetLayer::Cold],
            recency_bias: 0.0,
        };
        let result_no_bias = tm.query(&q_no_bias).await.unwrap();

        // Query with full recency bias.
        let q_full_bias = MemoryQuery {
            text: "recency".to_string(),
            max_results: 10,
            layers: vec![TargetLayer::Hot, TargetLayer::Cold],
            recency_bias: 1.0,
        };
        let result_full_bias = tm.query(&q_full_bias).await.unwrap();

        // With full recency bias, Hot layer (recency_factor=1.0) should score
        // higher than Cold (recency_factor=0.5).
        if result_full_bias.entries.len() >= 2 {
            assert_eq!(
                result_full_bias.entries[0].source_layer,
                TargetLayer::Hot,
                "with full recency bias, Hot should rank first"
            );
        }

        // With no recency bias, base scores determine order.
        // Hot base score is 0.9, Cold base score is entry.relevance_score (1.0
        // by default from MemoryEntry::new), so Cold should rank first.
        if result_no_bias.entries.len() >= 2 {
            assert_eq!(
                result_no_bias.entries[0].source_layer,
                TargetLayer::Cold,
                "with no recency bias, Cold (score 1.0) should outrank Hot (score 0.9)"
            );
        }
    }

    #[tokio::test]
    async fn test_flush_wal_and_recover() {
        let (mut tm, dir) = make_tiered(None);
        let wal_path = dir.path().join("session.wal");

        tm.session_mut()
            .log_decision("WAL test decision", None);
        tm.session_mut().set_task(Some("WAL test task".into()));

        // Flush to WAL.
        tm.flush_wal(&wal_path).unwrap();
        assert!(wal_path.exists());

        // Create a new tiered memory and recover.
        let (mut tm2, _dir2) = make_tiered(None);
        let recovered = tm2.recover_session(&wal_path).unwrap();
        assert!(recovered, "should recover from WAL");

        assert_eq!(tm2.session().decisions_log.len(), 1);
        assert_eq!(
            tm2.session().decisions_log[0].content,
            "WAL test decision"
        );
        assert_eq!(
            tm2.session().current_task.as_deref(),
            Some("WAL test task")
        );
    }

    #[tokio::test]
    async fn test_recover_missing_wal() {
        let (mut tm, dir) = make_tiered(None);
        let wal_path = dir.path().join("nonexistent.wal");

        let recovered = tm.recover_session(&wal_path).unwrap();
        assert!(!recovered, "should return false for missing WAL");
    }

    #[tokio::test]
    async fn test_max_results_truncation() {
        let collective =
            Arc::new(CollectiveMemory::in_memory().expect("in-memory db should work"));

        // Insert many entries.
        for i in 0..20 {
            let entry = MemoryEntry::new(
                MemoryCategory::General,
                format!("item number {i} is interesting"),
            );
            collective.remember(&entry).unwrap();
        }

        let dir = TempDir::new().unwrap();
        let session = SessionState::new("truncate-test".into());
        let archive = ArchiveService::new(dir.path().to_path_buf());
        let tm = TieredMemory::new(session, None, collective, archive);

        let q = MemoryQuery {
            text: "item".to_string(),
            max_results: 5,
            layers: vec![TargetLayer::Cold],
            recency_bias: 0.0,
        };

        let result = tm.query(&q).await.unwrap();
        assert!(
            result.entries.len() <= 5,
            "should truncate to max_results=5, got: {}",
            result.entries.len()
        );
    }
}
