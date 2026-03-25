#[cfg(feature = "memory-tiering")]
mod tiered_integration_tests {
    use hive_agents::collective_memory::{CollectiveMemory, MemoryCategory, MemoryEntry};
    use hive_agents::memory::*;
    use std::sync::Arc;
    use tempfile::TempDir;

    /// Helper: build a `TieredMemory` with in-memory backends and a temp dir.
    fn setup() -> (TempDir, TieredMemory) {
        let dir = TempDir::new().unwrap();
        let collective =
            Arc::new(CollectiveMemory::in_memory().expect("in-memory db should work"));
        let archive = ArchiveService::new(dir.path().join("archive"));
        let mock_bridge = Arc::new(MockVectorBridge::new());
        let session = SessionState::new("test-session".to_string());
        let tiered = TieredMemory::new(
            session,
            Some(mock_bridge as Arc<dyn VectorMemoryBridge>),
            collective,
            archive,
        );
        (dir, tiered)
    }

    // -----------------------------------------------------------------------
    // Test 1: Full lifecycle — decisions, entities, flush, query, WAL round-trip
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_full_lifecycle() {
        let (dir, mut tiered) = setup();

        // --- Add decisions to session (Hot layer) ---
        tiered
            .session_mut()
            .log_decision("Use LanceDB", Some("native Rust"));
        tiered
            .session_mut()
            .log_decision("Prefer async-trait", Some("ergonomics"));

        // --- Add entities ---
        tiered.session_mut().touch_entity("main.rs", "file");
        tiered
            .session_mut()
            .touch_entity("TieredMemory", "struct");

        // --- Queue writes targeting Cold ---
        tiered.session_mut().queue_write(
            "batch inserts are fast in SQLite".to_string(),
            MemoryCategory::CodePattern,
            0.8,
            TargetLayer::Cold,
        );
        tiered.session_mut().queue_write(
            "user prefers dark mode".to_string(),
            MemoryCategory::UserPreference,
            0.6,
            TargetLayer::Cold,
        );

        // --- Flush session: verify count ---
        let flushed = tiered.flush_session().await.unwrap();
        assert_eq!(flushed, 2, "should flush 2 pending writes");
        assert!(
            tiered.session().pending_memory_writes.is_empty(),
            "pending writes should be drained after flush"
        );

        // --- Query Hot layer: verify decision found ---
        let hot_query = MemoryQuery {
            text: "LanceDB".to_string(),
            max_results: 10,
            layers: vec![TargetLayer::Hot],
            recency_bias: 0.0,
        };
        let hot_result = tiered.query(&hot_query).await.unwrap();
        assert!(
            !hot_result.entries.is_empty(),
            "Hot layer should contain the LanceDB decision"
        );
        assert!(
            hot_result.entries[0].content.contains("LanceDB"),
            "matched entry should mention LanceDB: {}",
            hot_result.entries[0].content
        );

        // --- Query Cold layer: verify flushed write found ---
        let cold_query = MemoryQuery {
            text: "batch".to_string(),
            max_results: 10,
            layers: vec![TargetLayer::Cold],
            recency_bias: 0.0,
        };
        let cold_result = tiered.query(&cold_query).await.unwrap();
        assert!(
            !cold_result.entries.is_empty(),
            "Cold layer should contain the flushed 'batch inserts' write"
        );
        assert_eq!(cold_result.entries[0].source_layer, TargetLayer::Cold);

        // --- WAL round-trip: flush, create new tiered, recover, verify ---
        let wal_path = dir.path().join("session.wal");
        tiered.flush_wal(&wal_path).unwrap();
        assert!(wal_path.exists(), "WAL file should exist after flush");

        // Create a fresh TieredMemory and recover session from WAL.
        let (dir2, mut tiered2) = setup();
        let _ = dir2; // keep tempdir alive
        let recovered = tiered2.recover_session(&wal_path).unwrap();
        assert!(recovered, "should successfully recover session from WAL");

        // Verify recovered state preserves decisions.
        assert_eq!(
            tiered2.session().decisions_log.len(),
            2,
            "recovered session should have 2 decisions"
        );
        assert_eq!(
            tiered2.session().decisions_log[0].content, "Use LanceDB",
            "first recovered decision should be 'Use LanceDB'"
        );

        // Verify recovered state preserves entities.
        assert!(
            tiered2.session().entity_cache.contains_key("main.rs"),
            "recovered session should contain 'main.rs' entity"
        );
    }

    // -----------------------------------------------------------------------
    // Test 2: Cross-layer query — Hot + Cold merged results
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_cross_layer_query() {
        let (dir, mut tiered) = setup();
        let _ = dir;

        // Add data to Hot layer (decision).
        tiered
            .session_mut()
            .log_decision("Optimize database queries for speed", None);

        // Add data to Cold layer (flush a write).
        tiered.session_mut().queue_write(
            "Database indexing improves query speed".to_string(),
            MemoryCategory::CodePattern,
            0.9,
            TargetLayer::Cold,
        );
        tiered.flush_session().await.unwrap();

        // Query across both layers.
        let query = MemoryQuery {
            text: "database".to_string(),
            max_results: 10,
            layers: vec![TargetLayer::Hot, TargetLayer::Cold],
            recency_bias: 0.0,
        };
        let result = tiered.query(&query).await.unwrap();

        assert!(
            result.entries.len() >= 2,
            "cross-layer query should return entries from both Hot and Cold, got {}",
            result.entries.len()
        );

        // Verify we have entries from both layers.
        let has_hot = result
            .entries
            .iter()
            .any(|e| e.source_layer == TargetLayer::Hot);
        let has_cold = result
            .entries
            .iter()
            .any(|e| e.source_layer == TargetLayer::Cold);
        assert!(has_hot, "should have at least one Hot result");
        assert!(has_cold, "should have at least one Cold result");
    }

    // -----------------------------------------------------------------------
    // Test 3: Bootstrap generation — generate + write + load round-trip
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_bootstrap_generation() {
        let dir = TempDir::new().unwrap();
        let collective =
            Arc::new(CollectiveMemory::in_memory().expect("in-memory db should work"));

        // Add memories to CollectiveMemory for identity + memory generation.
        let pref = MemoryEntry::new(MemoryCategory::UserPreference, "prefers dark mode");
        let pattern = MemoryEntry::new(MemoryCategory::CodePattern, "always use Result<T, E>");
        collective.remember(&pref).unwrap();
        collective.remember(&pattern).unwrap();

        // Create bootstrap generator.
        let memory_dir = dir.path().join("bootstrap");
        let generator = BootstrapGenerator::new(memory_dir);

        // Generate identity from user preferences.
        let preferences = collective
            .recall("", Some(MemoryCategory::UserPreference), None, 50)
            .unwrap();
        let identity_md = generator.generate_identity(&preferences).unwrap();
        assert!(
            identity_md.contains("dark mode"),
            "identity should contain user preference"
        );

        // Generate memory from all memories + empty archive.
        let all_memories = collective.recall("", None, None, 100).unwrap();
        let memory_md = generator.generate_memory(&all_memories, &[]).unwrap();
        assert!(
            memory_md.contains("Result<T, E>"),
            "memory should contain code pattern"
        );

        // Generate context from a session.
        let mut session = SessionState::new("bootstrap-test".to_string());
        session.set_task(Some("implement bootstrap".to_string()));
        session.log_decision("Use markdown format", Some("human-readable"));
        let decisions = session.decisions_log.clone();
        let context_md = generator.generate_context(&session, &decisions).unwrap();
        assert!(
            context_md.contains("implement bootstrap"),
            "context should contain current task"
        );
        assert!(
            context_md.contains("markdown format"),
            "context should contain recent decision"
        );

        // Write all, load, verify round-trip.
        generator.write_all(&identity_md, &memory_md, &context_md)
            .unwrap();

        let loaded = generator.load_bootstrap().unwrap();
        assert!(loaded.identity.is_some(), "identity should be loaded");
        assert!(loaded.memory.is_some(), "memory should be loaded");
        assert!(loaded.context.is_some(), "context should be loaded");

        assert!(
            loaded.identity.unwrap().contains("dark mode"),
            "loaded identity should preserve content"
        );
        assert!(
            loaded.memory.unwrap().contains("Result<T, E>"),
            "loaded memory should preserve content"
        );
        assert!(
            loaded.context.unwrap().contains("implement bootstrap"),
            "loaded context should preserve content"
        );
    }

    // -----------------------------------------------------------------------
    // Test 4: Archive integration — archive_daily + query
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_archive_integration() {
        let dir = TempDir::new().unwrap();
        let collective =
            Arc::new(CollectiveMemory::in_memory().expect("in-memory db should work"));

        // Add entries to CollectiveMemory.
        let entry1 = MemoryEntry::new(
            MemoryCategory::SuccessPattern,
            "Parallel tests reduce CI time",
        );
        let entry2 = MemoryEntry::new(
            MemoryCategory::CodePattern,
            "Use Arc<Mutex<T>> for shared state",
        );
        collective.remember(&entry1).unwrap();
        collective.remember(&entry2).unwrap();

        let archive = ArchiveService::new(dir.path().join("archive"));
        let session = SessionState::new("archive-test".to_string());
        let mock_bridge = Arc::new(MockVectorBridge::new());
        let mut tiered = TieredMemory::new(
            session,
            Some(mock_bridge as Arc<dyn VectorMemoryBridge>),
            collective,
            archive,
        );

        // Archive for today.
        let today = chrono::Utc::now().date_naive();
        let archive_path = tiered.archive_daily(today).unwrap();
        assert!(
            archive_path.exists(),
            "archive daily log file should exist"
        );

        // Read the archive file and verify content.
        let archive_content = std::fs::read_to_string(&archive_path).unwrap();
        assert!(
            archive_content.contains("Parallel tests"),
            "archive should contain the success pattern entry"
        );
        assert!(
            archive_content.contains("Arc<Mutex<T>>"),
            "archive should contain the code pattern entry"
        );

        // Query the archive layer.
        let query = MemoryQuery {
            text: "parallel".to_string(),
            max_results: 10,
            layers: vec![TargetLayer::Archive],
            recency_bias: 0.0,
        };
        let result = tiered.query(&query).await.unwrap();
        assert!(
            !result.entries.is_empty(),
            "archive query should find entries matching 'parallel'"
        );
        assert_eq!(
            result.entries[0].source_layer,
            TargetLayer::Archive,
            "result should come from Archive layer"
        );
    }

    // -----------------------------------------------------------------------
    // Test 5: HiveLoop compaction hooks — verify hooks fire
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_hiveloop_compaction_hooks() {
        use hive_agents::hiveloop::{HiveLoop, LoopConfig};
        use std::sync::Mutex;

        // Capture hook invocations.
        let pre_captures: Arc<Mutex<Vec<Vec<String>>>> = Arc::new(Mutex::new(Vec::new()));
        let post_captures: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

        let pre_clone = pre_captures.clone();
        let post_clone = post_captures.clone();

        let hive_loop = HiveLoop::new(LoopConfig::default())
            .with_pre_compaction(Box::new(move |messages: &[String]| {
                pre_clone
                    .lock()
                    .unwrap()
                    .push(messages.to_vec());
                // Return extracted memories.
                vec!["extracted-memory-1".to_string()]
            }))
            .with_post_compaction(Box::new(move |summary: &str| {
                post_clone
                    .lock()
                    .unwrap()
                    .push(summary.to_string());
            }));

        // Fire pre-compaction hook.
        let messages = vec![
            "message one".to_string(),
            "message two".to_string(),
        ];
        let extracted = hive_loop.fire_pre_compaction(&messages);
        assert_eq!(
            extracted,
            vec!["extracted-memory-1"],
            "pre-compaction hook should return extracted memories"
        );

        // Verify the hook captured the input.
        let pre_data = pre_captures.lock().unwrap();
        assert_eq!(pre_data.len(), 1, "pre hook should have been called once");
        assert_eq!(
            pre_data[0],
            vec!["message one", "message two"],
            "pre hook should receive the messages"
        );
        drop(pre_data);

        // Fire post-compaction hook.
        hive_loop.fire_post_compaction("compaction complete: 5 messages trimmed");

        let post_data = post_captures.lock().unwrap();
        assert_eq!(post_data.len(), 1, "post hook should have been called once");
        assert_eq!(
            post_data[0], "compaction complete: 5 messages trimmed",
            "post hook should receive the summary"
        );
    }
}
