#[cfg(feature = "memory-tiering")]
mod session_state_tests {
    use hive_agents::collective_memory::MemoryCategory;
    use hive_agents::memory::session_state::*;
    use hive_agents::memory::TargetLayer;
    use std::time::Duration;
    use tempfile::TempDir;

    #[test]
    fn test_new_session() {
        let state = SessionState::new("sess-001".into());
        assert_eq!(state.session_id, "sess-001");
        assert!(state.active_context.is_empty());
        assert!(state.current_task.is_none());
        assert!(state.decisions_log.is_empty());
        assert!(state.entity_cache.is_empty());
        assert!(state.pending_memory_writes.is_empty());
        assert!(state.created_at <= state.last_activity);
    }

    #[test]
    fn test_log_decision() {
        let mut state = SessionState::new("sess-002".into());

        state.log_decision("Use SQLite for storage", Some("lightweight and embedded"));
        state.log_decision("Skip caching layer", None);

        assert_eq!(state.decisions_log.len(), 2);

        let first = &state.decisions_log[0];
        assert_eq!(first.content, "Use SQLite for storage");
        assert_eq!(
            first.rationale.as_deref(),
            Some("lightweight and embedded")
        );

        let second = &state.decisions_log[1];
        assert_eq!(second.content, "Skip caching layer");
        assert!(second.rationale.is_none());

        assert!(first.timestamp <= second.timestamp);
    }

    #[test]
    fn test_touch_entity_new_and_existing() {
        let mut state = SessionState::new("sess-003".into());

        // First touch creates the entity.
        {
            let info = state.touch_entity("main.rs", "file");
            assert_eq!(info.name, "main.rs");
            assert_eq!(info.entity_type, "file");
            assert_eq!(info.mentions, 1);
        }

        // Second touch increments mentions.
        {
            let info = state.touch_entity("main.rs", "file");
            assert_eq!(info.mentions, 2);
        }

        assert_eq!(state.entity_cache.len(), 1);

        // Touch a different entity.
        state.touch_entity("parse_config", "function");
        assert_eq!(state.entity_cache.len(), 2);
    }

    #[test]
    fn test_context_operations() {
        let mut state = SessionState::new("sess-004".into());

        // set_context replaces everything.
        state.set_context(vec!["file_a.rs".into(), "file_b.rs".into()]);
        assert_eq!(state.active_context.len(), 2);
        assert_eq!(state.active_context[0], "file_a.rs");
        assert_eq!(state.active_context[1], "file_b.rs");

        // add_context appends.
        state.add_context("file_c.rs".into());
        assert_eq!(state.active_context.len(), 3);
        assert_eq!(state.active_context[2], "file_c.rs");

        // set_context replaces again.
        state.set_context(vec!["only.rs".into()]);
        assert_eq!(state.active_context.len(), 1);
        assert_eq!(state.active_context[0], "only.rs");
    }

    #[test]
    fn test_pending_writes_and_drain() {
        let mut state = SessionState::new("sess-005".into());

        state.queue_write(
            "learned pattern X".into(),
            MemoryCategory::CodePattern,
            0.8,
            TargetLayer::Cold,
        );
        state.queue_write(
            "user prefers dark theme".into(),
            MemoryCategory::UserPreference,
            0.5,
            TargetLayer::Warm,
        );

        assert_eq!(state.pending_memory_writes.len(), 2);

        let drained = state.drain_pending();
        assert_eq!(drained.len(), 2);
        assert_eq!(drained[0].content, "learned pattern X");
        assert_eq!(drained[1].content, "user prefers dark theme");

        // Second drain returns empty.
        let drained_again = state.drain_pending();
        assert!(drained_again.is_empty());
        assert!(state.pending_memory_writes.is_empty());
    }

    #[test]
    fn test_wal_roundtrip() {
        let dir = TempDir::new().unwrap();
        let wal_path = dir.path().join("session.wal");

        let mut state = SessionState::new("sess-006".into());
        state.set_task(Some("implement feature X".into()));
        state.log_decision("chose approach A", Some("simpler"));
        state.touch_entity("config.rs", "file");
        state.add_context("hive_agents/src".into());
        state.queue_write(
            "important insight".into(),
            MemoryCategory::General,
            0.9,
            TargetLayer::Cold,
        );

        // Flush to WAL.
        state.flush_to_wal(&wal_path).unwrap();
        assert!(wal_path.exists());

        // Recover from WAL.
        let recovered = SessionState::recover_from_wal(&wal_path)
            .unwrap()
            .expect("should recover session");

        assert_eq!(recovered.session_id, "sess-006");
        assert_eq!(recovered.current_task.as_deref(), Some("implement feature X"));
        assert_eq!(recovered.decisions_log.len(), 1);
        assert_eq!(recovered.decisions_log[0].content, "chose approach A");
        assert_eq!(recovered.entity_cache.len(), 1);
        assert!(recovered.entity_cache.contains_key("config.rs"));
        assert_eq!(recovered.active_context, vec!["hive_agents/src"]);
        assert_eq!(recovered.pending_memory_writes.len(), 1);
        assert_eq!(recovered.pending_memory_writes[0].content, "important insight");
    }

    #[test]
    fn test_wal_recover_missing() {
        let dir = TempDir::new().unwrap();
        let wal_path = dir.path().join("nonexistent.wal");

        let result = SessionState::recover_from_wal(&wal_path).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_wal_clear() {
        let dir = TempDir::new().unwrap();
        let wal_path = dir.path().join("session.wal");

        let state = SessionState::new("sess-007".into());
        state.flush_to_wal(&wal_path).unwrap();
        assert!(wal_path.exists());

        SessionState::clear_wal(&wal_path).unwrap();
        assert!(!wal_path.exists());

        // Clearing a non-existent file should also be Ok.
        SessionState::clear_wal(&wal_path).unwrap();
    }

    #[test]
    fn test_idle_detection() {
        let state = SessionState::new("sess-008".into());

        // Just created, should not be idle for any reasonable threshold.
        assert!(!state.is_idle(Duration::from_secs(60)));

        // With Duration::ZERO, any elapsed time counts as idle.
        assert!(state.is_idle(Duration::ZERO));
    }
}
