//! L4 ARCHIVE: Daily markdown memory logs.
//!
//! Provides persistent, human-readable storage for memory entries as dated
//! markdown files. Each day's entries are grouped by [`MemoryCategory`] with
//! relevance scores, making them easy to browse and grep.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::collective_memory::MemoryEntry;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A single entry found in an archive daily log.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveEntry {
    /// The date of the daily log this entry belongs to.
    pub date: NaiveDate,
    /// The raw text content of the line.
    pub content: String,
    /// 1-based line number within the daily log file.
    pub line_number: usize,
}

// ---------------------------------------------------------------------------
// ArchiveService
// ---------------------------------------------------------------------------

/// Service for managing daily markdown memory logs.
///
/// Each day produces a single `YYYY-MM-DD.md` file under `base_dir`.
/// Entries are grouped by [`MemoryCategory`] with relevance scores.
pub struct ArchiveService {
    base_dir: PathBuf,
}

impl ArchiveService {
    /// Create a new archive service writing logs to `base_dir`.
    pub fn new(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }

    /// Return the path to the daily log file for the given date.
    ///
    /// Format: `base_dir/YYYY-MM-DD.md`
    pub fn log_path(&self, date: NaiveDate) -> PathBuf {
        self.base_dir.join(format!("{}.md", date))
    }

    /// Consolidate a slice of memory entries into a daily markdown log.
    ///
    /// Creates (or appends to) the daily log file for `date`. Entries are
    /// grouped by [`MemoryCategory`] with human-readable section headings.
    /// Each entry line includes the relevance score in brackets.
    ///
    /// Returns the path to the written file.
    pub fn consolidate_to_daily_log(
        &self,
        date: NaiveDate,
        entries: &[MemoryEntry],
    ) -> Result<PathBuf, String> {
        if entries.is_empty() {
            return Err("no entries to consolidate".to_string());
        }

        let path = self.log_path(date);

        // Ensure parent directory exists.
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("failed to create archive directory: {e}"))?;
        }

        // Group entries by category, preserving insertion order via BTreeMap
        // keyed on the category's string representation.
        let mut groups: BTreeMap<&'static str, Vec<&MemoryEntry>> = BTreeMap::new();
        for entry in entries {
            groups
                .entry(entry.category.as_str())
                .or_default()
                .push(entry);
        }

        let mut md = String::new();

        // Only write the header if the file doesn't already exist.
        let file_exists = path.exists();
        if !file_exists {
            md.push_str(&format!("# Daily Memory Log \u{2014} {}\n\n", date));
        } else {
            // When appending, add a separator.
            md.push('\n');
        }

        for (cat_str, group) in &groups {
            let heading = category_heading(cat_str);
            md.push_str(&format!("## {heading}\n"));
            for entry in group {
                md.push_str(&format!(
                    "- {} [relevance: {:.2}]\n",
                    entry.content, entry.relevance_score
                ));
            }
            md.push('\n');
        }

        if file_exists {
            // Append to existing file.
            use std::io::Write;
            let mut f = std::fs::OpenOptions::new()
                .append(true)
                .open(&path)
                .map_err(|e| format!("failed to open archive file for append: {e}"))?;
            f.write_all(md.as_bytes())
                .map_err(|e| format!("failed to append to archive file: {e}"))?;
        } else {
            std::fs::write(&path, md.as_bytes())
                .map_err(|e| format!("failed to write archive file: {e}"))?;
        }

        Ok(path)
    }

    /// Query daily logs over a date range, optionally filtering by keyword.
    ///
    /// Iterates each date from `from` to `to` (inclusive), reads the
    /// corresponding markdown file if it exists, and returns matching
    /// content lines. When `keyword` is `Some`, only lines containing
    /// the keyword (case-insensitive) are included.
    pub fn query_daily_logs(
        &self,
        from: NaiveDate,
        to: NaiveDate,
        keyword: Option<&str>,
    ) -> Result<Vec<ArchiveEntry>, String> {
        let mut results = Vec::new();
        let mut current = from;

        while current <= to {
            let path = self.log_path(current);
            if path.exists() {
                let contents = std::fs::read_to_string(&path)
                    .map_err(|e| format!("failed to read {}: {e}", path.display()))?;

                for (idx, line) in contents.lines().enumerate() {
                    let line_number = idx + 1; // 1-based

                    // Skip empty lines and markdown headings for cleaner results.
                    let trimmed = line.trim();
                    if trimmed.is_empty() || trimmed.starts_with('#') {
                        continue;
                    }

                    let include = match keyword {
                        Some(kw) => line.to_lowercase().contains(&kw.to_lowercase()),
                        None => true,
                    };

                    if include {
                        results.push(ArchiveEntry {
                            date: current,
                            content: line.to_string(),
                            line_number,
                        });
                    }
                }
            }
            current = current
                .succ_opt()
                .ok_or_else(|| "date overflow".to_string())?;
        }

        Ok(results)
    }

    /// List all daily log files in the archive directory.
    ///
    /// Returns `(date, path)` pairs sorted chronologically (oldest first).
    /// Only files matching the `YYYY-MM-DD.md` naming convention are included.
    pub fn list_logs(&self) -> Result<Vec<(NaiveDate, PathBuf)>, String> {
        if !self.base_dir.exists() {
            return Ok(Vec::new());
        }

        let entries = std::fs::read_dir(&self.base_dir)
            .map_err(|e| format!("failed to read archive directory: {e}"))?;

        let mut logs: Vec<(NaiveDate, PathBuf)> = Vec::new();

        for entry in entries {
            let entry = entry.map_err(|e| format!("directory entry error: {e}"))?;
            let path = entry.path();

            // Only consider .md files.
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }

            // Try to parse the stem as YYYY-MM-DD.
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                if let Ok(date) = NaiveDate::parse_from_str(stem, "%Y-%m-%d") {
                    logs.push((date, path));
                }
            }
        }

        // Sort chronologically.
        logs.sort_by_key(|(date, _)| *date);

        Ok(logs)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Convert a [`MemoryCategory`] string representation to a human-readable
/// section heading for the markdown log.
fn category_heading(cat_str: &str) -> &'static str {
    match cat_str {
        "SuccessPattern" => "Success Patterns",
        "FailurePattern" => "Failure Patterns",
        "ModelInsight" => "Model Insights",
        "ConflictResolution" => "Conflict Resolutions",
        "CodePattern" => "Code Patterns",
        "UserPreference" => "User Preferences",
        "General" => "General",
        _ => "Other",
    }
}
