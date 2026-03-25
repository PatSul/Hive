#[cfg(feature = "memory-tiering")]
mod bootstrap_tests {
    use hive_agents::collective_memory::{MemoryCategory, MemoryEntry};
    use hive_agents::memory::archive::ArchiveEntry;
    use hive_agents::memory::bootstrap::*;
    use hive_agents::memory::session_state::SessionState;
    use tempfile::TempDir;

    fn make_preference(content: &str, score: f64) -> MemoryEntry {
        let mut entry = MemoryEntry::new(MemoryCategory::UserPreference, content);
        entry.relevance_score = score;
        entry
    }

    fn make_memory(content: &str, category: MemoryCategory, score: f64) -> MemoryEntry {
        let mut entry = MemoryEntry::new(category, content);
        entry.relevance_score = score;
        entry
    }

    fn make_archive(date_str: &str, content: &str) -> ArchiveEntry {
        ArchiveEntry {
            date: chrono::NaiveDate::parse_from_str(date_str, "%Y-%m-%d").unwrap(),
            content: content.to_string(),
            line_number: 1,
        }
    }

    #[test]
    fn test_generate_identity() {
        let tmp = TempDir::new().unwrap();
        let generator = BootstrapGenerator::new(tmp.path().to_path_buf());

        let prefs = vec![
            make_preference("Prefers dark mode", 0.95),
            make_preference("Uses Vim keybindings", 0.80),
            make_preference("Likes concise output", 0.60),
        ];

        let result = generator.generate_identity(&prefs).unwrap();

        assert!(result.starts_with("# Identity"));
        assert!(result.contains("## User Preferences"));
        // Highest score should come first.
        let dark_pos = result.find("Prefers dark mode").unwrap();
        let vim_pos = result.find("Uses Vim keybindings").unwrap();
        let concise_pos = result.find("Likes concise output").unwrap();
        assert!(dark_pos < vim_pos);
        assert!(vim_pos < concise_pos);
        // Check score formatting.
        assert!(result.contains("[score: 0.95]"));
        assert!(result.contains("[score: 0.80]"));
    }

    #[test]
    fn test_generate_identity_truncates_to_20() {
        let tmp = TempDir::new().unwrap();
        let generator = BootstrapGenerator::new(tmp.path().to_path_buf());

        let prefs: Vec<MemoryEntry> = (0..30)
            .map(|i| make_preference(&format!("Pref {i}"), i as f64 / 30.0))
            .collect();

        let result = generator.generate_identity(&prefs).unwrap();
        let lines: Vec<&str> = result.lines().filter(|l| l.starts_with("- ")).collect();
        assert_eq!(lines.len(), 20);
    }

    #[test]
    fn test_generate_memory() {
        let tmp = TempDir::new().unwrap();
        let generator = BootstrapGenerator::new(tmp.path().to_path_buf());

        let memories = vec![
            make_memory("Rust borrow checker rules", MemoryCategory::CodePattern, 0.9),
            make_memory("Use Arc for shared state", MemoryCategory::SuccessPattern, 0.7),
        ];

        let archives = vec![
            make_archive("2026-03-20", "Deployed v2.1"),
            make_archive("2026-03-21", "Fixed login bug"),
        ];

        let result = generator.generate_memory(&memories, &archives).unwrap();

        assert!(result.starts_with("# Memory"));
        assert!(result.contains("## Key Knowledge"));
        assert!(result.contains("Rust borrow checker rules"));
        assert!(result.contains("[CodePattern]"));
        assert!(result.contains("[SuccessPattern]"));
        assert!(result.contains("## Recent Archive"));
        assert!(result.contains("[2026-03-20] Deployed v2.1"));
        assert!(result.contains("[2026-03-21] Fixed login bug"));
    }

    #[test]
    fn test_generate_context() {
        let tmp = TempDir::new().unwrap();
        let generator = BootstrapGenerator::new(tmp.path().to_path_buf());

        let mut session = SessionState::new("test-session".into());
        session.set_task(Some("Implement bootstrap generator".into()));
        session.add_context("hive_agents crate".into());
        session.add_context("memory module".into());

        let decisions = vec![
            hive_agents::memory::session_state::Decision {
                content: "Use markdown format".into(),
                timestamp: chrono::Utc::now(),
                rationale: Some("human readable".into()),
            },
            hive_agents::memory::session_state::Decision {
                content: "Cap at 20 entries".into(),
                timestamp: chrono::Utc::now(),
                rationale: None,
            },
        ];

        let result = generator.generate_context(&session, &decisions).unwrap();

        assert!(result.contains("# Context"));
        assert!(result.contains("## Current Task"));
        assert!(result.contains("Implement bootstrap generator"));
        assert!(result.contains("## Active Context"));
        assert!(result.contains("- hive_agents crate"));
        assert!(result.contains("- memory module"));
        assert!(result.contains("## Recent Decisions"));
        assert!(result.contains("Use markdown format (human readable)"));
        assert!(result.contains("Cap at 20 entries (no rationale)"));
    }

    #[test]
    fn test_generate_context_no_task() {
        let tmp = TempDir::new().unwrap();
        let generator = BootstrapGenerator::new(tmp.path().to_path_buf());

        let session = SessionState::new("empty-session".into());
        let result = generator.generate_context(&session, &[]).unwrap();

        assert!(result.contains("No active task"));
    }

    #[test]
    fn test_write_and_load_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let generator = BootstrapGenerator::new(tmp.path().to_path_buf());

        let identity = "# Identity\n\n## User Preferences\n- Dark mode [score: 0.95]\n";
        let memory = "# Memory\n\n## Key Knowledge\n- Rust is great [CodePattern]\n";
        let context = "# Context\n\n## Current Task\nBuild things\n";

        generator.write_all(identity, memory, context).unwrap();

        // Verify files exist.
        assert!(generator.identity_path().exists());
        assert!(generator.memory_path().exists());
        assert!(generator.context_path().exists());

        // Load and verify contents.
        let loaded = generator.load_bootstrap().unwrap();
        assert_eq!(loaded.identity.as_deref(), Some(identity));
        assert_eq!(loaded.memory.as_deref(), Some(memory));
        assert_eq!(loaded.context.as_deref(), Some(context));
    }

    #[test]
    fn test_load_missing_files() {
        let tmp = TempDir::new().unwrap();
        let generator = BootstrapGenerator::new(tmp.path().to_path_buf());

        let loaded = generator.load_bootstrap().unwrap();
        assert!(loaded.identity.is_none());
        assert!(loaded.memory.is_none());
        assert!(loaded.context.is_none());
    }

    #[test]
    fn test_hiveloop_hooks() {
        use hive_agents::hiveloop::{HiveLoop, LoopConfig};
        use std::sync::{Arc, Mutex};

        let pre_captured = Arc::new(Mutex::new(Vec::<Vec<String>>::new()));
        let post_captured = Arc::new(Mutex::new(Vec::<String>::new()));

        let pre_clone = Arc::clone(&pre_captured);
        let post_clone = Arc::clone(&post_captured);

        let hive_loop = HiveLoop::new(LoopConfig::default())
            .with_pre_compaction(Box::new(move |messages: &[String]| {
                pre_clone
                    .lock()
                    .unwrap()
                    .push(messages.to_vec());
                vec!["extracted-memory-1".into(), "extracted-memory-2".into()]
            }))
            .with_post_compaction(Box::new(move |summary: &str| {
                post_clone
                    .lock()
                    .unwrap()
                    .push(summary.to_string());
            }));

        // Fire pre-compaction hook.
        let messages = vec!["msg-a".into(), "msg-b".into(), "msg-c".into()];
        let extracted = hive_loop.fire_pre_compaction(&messages);
        assert_eq!(extracted.len(), 2);
        assert_eq!(extracted[0], "extracted-memory-1");
        assert_eq!(extracted[1], "extracted-memory-2");

        // Verify pre-compaction received the correct messages.
        let pre_calls = pre_captured.lock().unwrap();
        assert_eq!(pre_calls.len(), 1);
        assert_eq!(pre_calls[0], messages);

        // Fire post-compaction hook.
        hive_loop.fire_post_compaction("Compacted 50 messages to 10");
        let post_calls = post_captured.lock().unwrap();
        assert_eq!(post_calls.len(), 1);
        assert_eq!(post_calls[0], "Compacted 50 messages to 10");
    }

    #[test]
    fn test_hiveloop_hooks_default_none() {
        use hive_agents::hiveloop::{HiveLoop, LoopConfig};

        let hive_loop = HiveLoop::new(LoopConfig::default());

        // Without hooks, fire_pre_compaction returns empty vec.
        let result = hive_loop.fire_pre_compaction(&["hello".into()]);
        assert!(result.is_empty());

        // fire_post_compaction with no hook should not panic.
        hive_loop.fire_post_compaction("summary");
    }
}
