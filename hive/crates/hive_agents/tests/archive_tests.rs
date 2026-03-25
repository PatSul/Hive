#[cfg(feature = "memory-tiering")]
mod archive_tests {
    use chrono::NaiveDate;
    use hive_agents::collective_memory::{MemoryCategory, MemoryEntry};
    use hive_agents::memory::archive::*;
    use tempfile::TempDir;

    /// Helper: create a `MemoryEntry` with the given category, content, and relevance.
    fn make_entry(category: MemoryCategory, content: &str, relevance: f64) -> MemoryEntry {
        let mut entry = MemoryEntry::new(category, content);
        entry.relevance_score = relevance;
        entry
    }

    #[test]
    fn test_consolidate_creates_file() {
        let dir = TempDir::new().unwrap();
        let archive = ArchiveService::new(dir.path().to_path_buf());
        let date = NaiveDate::from_ymd_opt(2026, 3, 23).unwrap();

        let entries = vec![
            make_entry(
                MemoryCategory::SuccessPattern,
                "Used batch inserts for speed",
                0.95,
            ),
            make_entry(
                MemoryCategory::SuccessPattern,
                "Parallel test execution",
                0.85,
            ),
            make_entry(
                MemoryCategory::General,
                "Chose LanceDB over ChromaDB",
                0.88,
            ),
        ];

        let path = archive
            .consolidate_to_daily_log(date, &entries)
            .unwrap();

        // File should exist.
        assert!(path.exists(), "daily log file was not created");

        let contents = std::fs::read_to_string(&path).unwrap();

        // Should contain the header.
        assert!(
            contents.contains("# Daily Memory Log"),
            "missing header in: {contents}"
        );
        assert!(
            contents.contains("2026-03-23"),
            "missing date in header: {contents}"
        );

        // Should contain category sections.
        assert!(
            contents.contains("## Success Patterns"),
            "missing Success Patterns section: {contents}"
        );
        assert!(
            contents.contains("## General"),
            "missing General section: {contents}"
        );

        // Should contain entry content with relevance scores.
        assert!(
            contents.contains("Used batch inserts for speed [relevance: 0.95]"),
            "missing entry content: {contents}"
        );
        assert!(
            contents.contains("Chose LanceDB over ChromaDB [relevance: 0.88]"),
            "missing entry content: {contents}"
        );
    }

    #[test]
    fn test_consolidate_empty_entries_returns_error() {
        let dir = TempDir::new().unwrap();
        let archive = ArchiveService::new(dir.path().to_path_buf());
        let date = NaiveDate::from_ymd_opt(2026, 3, 23).unwrap();

        let result = archive.consolidate_to_daily_log(date, &[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_query_by_date_range() {
        let dir = TempDir::new().unwrap();
        let archive = ArchiveService::new(dir.path().to_path_buf());

        let date1 = NaiveDate::from_ymd_opt(2026, 3, 20).unwrap();
        let date2 = NaiveDate::from_ymd_opt(2026, 3, 21).unwrap();
        let date3 = NaiveDate::from_ymd_opt(2026, 3, 22).unwrap();

        // Write logs for three dates.
        archive
            .consolidate_to_daily_log(
                date1,
                &[make_entry(MemoryCategory::General, "Entry from day 1", 0.7)],
            )
            .unwrap();
        archive
            .consolidate_to_daily_log(
                date2,
                &[make_entry(MemoryCategory::General, "Entry from day 2", 0.8)],
            )
            .unwrap();
        archive
            .consolidate_to_daily_log(
                date3,
                &[make_entry(
                    MemoryCategory::CodePattern,
                    "Entry from day 3",
                    0.9,
                )],
            )
            .unwrap();

        // Query only dates 1 and 2.
        let results = archive
            .query_daily_logs(date1, date2, None)
            .unwrap();

        // Should find entries from day 1 and day 2 but not day 3.
        let all_content: Vec<&str> = results.iter().map(|e| e.content.as_str()).collect();
        assert!(
            all_content.iter().any(|c| c.contains("day 1")),
            "missing day 1: {all_content:?}"
        );
        assert!(
            all_content.iter().any(|c| c.contains("day 2")),
            "missing day 2: {all_content:?}"
        );
        assert!(
            !all_content.iter().any(|c| c.contains("day 3")),
            "should not include day 3: {all_content:?}"
        );

        // Each result should have the correct date.
        for entry in &results {
            assert!(
                entry.date == date1 || entry.date == date2,
                "unexpected date: {}",
                entry.date
            );
        }
    }

    #[test]
    fn test_query_with_keyword() {
        let dir = TempDir::new().unwrap();
        let archive = ArchiveService::new(dir.path().to_path_buf());
        let date = NaiveDate::from_ymd_opt(2026, 3, 23).unwrap();

        let entries = vec![
            make_entry(MemoryCategory::General, "SQLite is fast", 0.8),
            make_entry(MemoryCategory::CodePattern, "Use batch inserts", 0.9),
            make_entry(MemoryCategory::General, "PostgreSQL is reliable", 0.7),
        ];

        archive
            .consolidate_to_daily_log(date, &entries)
            .unwrap();

        // Query with keyword "batch" — should match only the batch inserts entry.
        let results = archive
            .query_daily_logs(date, date, Some("batch"))
            .unwrap();

        assert_eq!(results.len(), 1, "expected 1 result, got: {results:?}");
        assert!(
            results[0].content.contains("batch"),
            "result should contain 'batch': {}",
            results[0].content
        );

        // Query with keyword "sql" — should match both SQLite and PostgreSQL
        // (case-insensitive).
        let results = archive
            .query_daily_logs(date, date, Some("sql"))
            .unwrap();

        assert_eq!(results.len(), 2, "expected 2 results, got: {results:?}");
    }

    #[test]
    fn test_list_logs() {
        let dir = TempDir::new().unwrap();
        let archive = ArchiveService::new(dir.path().to_path_buf());

        let date1 = NaiveDate::from_ymd_opt(2026, 3, 20).unwrap();
        let date2 = NaiveDate::from_ymd_opt(2026, 3, 22).unwrap();
        let date3 = NaiveDate::from_ymd_opt(2026, 3, 21).unwrap();

        // Write logs out of chronological order.
        archive
            .consolidate_to_daily_log(
                date2,
                &[make_entry(MemoryCategory::General, "second", 0.5)],
            )
            .unwrap();
        archive
            .consolidate_to_daily_log(
                date1,
                &[make_entry(MemoryCategory::General, "first", 0.5)],
            )
            .unwrap();
        archive
            .consolidate_to_daily_log(
                date3,
                &[make_entry(MemoryCategory::General, "third", 0.5)],
            )
            .unwrap();

        // Also create a non-matching file to verify it is ignored.
        std::fs::write(dir.path().join("notes.txt"), "not a log").unwrap();

        let logs = archive.list_logs().unwrap();

        // Should have 3 logs sorted chronologically.
        assert_eq!(logs.len(), 3, "expected 3 logs, got: {logs:?}");
        assert_eq!(logs[0].0, date1); // 2026-03-20
        assert_eq!(logs[1].0, date3); // 2026-03-21
        assert_eq!(logs[2].0, date2); // 2026-03-22

        // Paths should exist.
        for (_, path) in &logs {
            assert!(path.exists(), "log file should exist: {}", path.display());
        }
    }

    #[test]
    fn test_empty_archive() {
        let dir = TempDir::new().unwrap();
        let archive = ArchiveService::new(dir.path().to_path_buf());

        let from = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        let to = NaiveDate::from_ymd_opt(2026, 1, 31).unwrap();

        let results = archive.query_daily_logs(from, to, None).unwrap();
        assert!(results.is_empty(), "empty archive should return no results");

        let logs = archive.list_logs().unwrap();
        assert!(logs.is_empty(), "empty archive should list no logs");
    }

    #[test]
    fn test_log_path_format() {
        let dir = TempDir::new().unwrap();
        let archive = ArchiveService::new(dir.path().to_path_buf());
        let date = NaiveDate::from_ymd_opt(2026, 3, 23).unwrap();

        let path = archive.log_path(date);
        assert!(
            path.ends_with("2026-03-23.md"),
            "unexpected path format: {}",
            path.display()
        );
    }
}
