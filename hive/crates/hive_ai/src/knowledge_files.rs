//! Project Knowledge File Scanner.
//!
//! Scans a project root for knowledge files (`HIVE.md`, `.hive/context.md`,
//! `README.md`, `CONTRIBUTING.md`, etc.) and returns their contents as
//! prioritized context sources for injection into the AI context window.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tracing::debug;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Maximum file size (in bytes) for a knowledge file. Files larger than this
/// are silently skipped to prevent huge files from consuming the context budget.
const MAX_FILE_SIZE: u64 = 50 * 1024; // 50 KB

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// The kind of knowledge file discovered in the project.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeSourceType {
    /// `HIVE.md` — project-specific AI instructions (highest priority).
    HiveInstructions,
    /// `.hive/context.md` — project-specific context.
    HiveContext,
    /// `README.md` — general project information.
    Readme,
    /// `CONTRIBUTING.md` — contribution guidelines.
    Contributing,
    /// `.hive/*.md` — any other markdown files in the `.hive` directory.
    CustomContext,
}

/// A single knowledge file discovered in the project, with its contents and
/// priority score.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeSource {
    /// Absolute path to the knowledge file.
    pub path: PathBuf,
    /// UTF-8 content of the file (truncated at `MAX_FILE_SIZE`).
    pub content: String,
    /// Priority score: 1.0 = highest, 0.0 = lowest. Higher-priority sources
    /// appear first in the context window.
    pub priority: f32,
    /// The kind of knowledge file.
    pub source_type: KnowledgeSourceType,
}

// ---------------------------------------------------------------------------
// KnowledgeFileScanner
// ---------------------------------------------------------------------------

/// Scans a project root directory for well-known knowledge files and returns
/// their contents as prioritized [`KnowledgeSource`] entries.
///
/// The scanner looks for these files in order of priority:
///
/// 1. `HIVE.md` — project-specific AI instructions (priority 1.0)
/// 2. `.hive/context.md` — project-specific context (priority 0.9)
/// 3. `README.md` — general project info (priority 0.6)
/// 4. `CONTRIBUTING.md` — contribution guidelines (priority 0.4)
/// 5. `.hive/*.md` (excluding `context.md`) — custom context files (priority 0.5)
///
/// Files that do not exist, exceed `MAX_FILE_SIZE`, or fail UTF-8 decoding are
/// silently skipped.
pub struct KnowledgeFileScanner;

impl KnowledgeFileScanner {
    /// Scan a project root for knowledge files and return their contents
    /// sorted by priority (highest first).
    pub fn scan(project_root: &Path) -> Vec<KnowledgeSource> {
        let mut sources = Vec::new();

        // 1. HIVE.md — highest priority
        Self::try_read_file(
            project_root,
            &project_root.join("HIVE.md"),
            KnowledgeSourceType::HiveInstructions,
            1.0,
            &mut sources,
        );

        // 2. .hive/context.md
        let hive_dir = project_root.join(".hive");
        Self::try_read_file(
            project_root,
            &hive_dir.join("context.md"),
            KnowledgeSourceType::HiveContext,
            0.9,
            &mut sources,
        );

        // 3. README.md
        Self::try_read_file(
            project_root,
            &project_root.join("README.md"),
            KnowledgeSourceType::Readme,
            0.6,
            &mut sources,
        );

        // 4. CONTRIBUTING.md
        Self::try_read_file(
            project_root,
            &project_root.join("CONTRIBUTING.md"),
            KnowledgeSourceType::Contributing,
            0.4,
            &mut sources,
        );

        // 5. .hive/*.md (excluding context.md which was already handled)
        if hive_dir.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&hive_dir) {
                for entry in entries.flatten() {
                    let entry_path = entry.path();
                    if !entry_path.is_file() {
                        continue;
                    }
                    let ext = entry_path
                        .extension()
                        .map(|e| e.to_string_lossy().to_lowercase());
                    if ext.as_deref() != Some("md") {
                        continue;
                    }
                    // Skip context.md — already handled above.
                    let name = entry_path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_lowercase());
                    if name.as_deref() == Some("context.md") {
                        continue;
                    }
                    Self::try_read_file(
                        project_root,
                        &entry_path,
                        KnowledgeSourceType::CustomContext,
                        0.5,
                        &mut sources,
                    );
                }
            }
        }

        // Sort by priority descending so highest-priority sources come first.
        sources.sort_by(|a, b| {
            b.priority
                .partial_cmp(&a.priority)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        debug!(
            "KnowledgeFileScanner: found {} knowledge files in {}",
            sources.len(),
            project_root.display()
        );

        sources
    }

    /// Format all knowledge sources into a single string suitable for
    /// injection into the AI system message.
    pub fn format_for_context(sources: &[KnowledgeSource]) -> String {
        if sources.is_empty() {
            return String::new();
        }

        let mut result = String::from("# Project Knowledge\n\n");

        for source in sources {
            let label = match source.source_type {
                KnowledgeSourceType::HiveInstructions => "Project AI Instructions (HIVE.md)",
                KnowledgeSourceType::HiveContext => "Project Context (.hive/context.md)",
                KnowledgeSourceType::Readme => "Project README",
                KnowledgeSourceType::Contributing => "Contributing Guidelines",
                KnowledgeSourceType::CustomContext => {
                    // Use filename as label
                    // We build a dynamic label below
                    ""
                }
            };

            if source.source_type == KnowledgeSourceType::CustomContext {
                let filename = source
                    .path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "unknown.md".to_string());
                result.push_str(&format!("## Custom Context: {}\n\n", filename));
            } else {
                result.push_str(&format!("## {}\n\n", label));
            }

            result.push_str(&source.content);
            result.push_str("\n\n");
        }

        result
    }

    /// Try to read a single file and append it to `sources` if it exists,
    /// is within the size limit, and decodes as valid UTF-8.
    fn try_read_file(
        project_root: &Path,
        file_path: &Path,
        source_type: KnowledgeSourceType,
        priority: f32,
        sources: &mut Vec<KnowledgeSource>,
    ) {
        // Security: ensure the file is within the project root.
        let canonical_root = project_root.canonicalize().unwrap_or_else(|_| project_root.to_path_buf());
        let canonical_file = file_path.canonicalize();
        if let Ok(ref canon) = canonical_file {
            if !canon.starts_with(&canonical_root) {
                debug!(
                    "KnowledgeFileScanner: skipping {} — outside project root",
                    file_path.display()
                );
                return;
            }
        }

        if !file_path.is_file() {
            return;
        }

        // Check file size before reading.
        let metadata = match std::fs::metadata(file_path) {
            Ok(m) => m,
            Err(_) => return,
        };
        if metadata.len() > MAX_FILE_SIZE {
            debug!(
                "KnowledgeFileScanner: skipping {} — exceeds {}KB size cap ({} bytes)",
                file_path.display(),
                MAX_FILE_SIZE / 1024,
                metadata.len()
            );
            return;
        }

        // Read the file content (UTF-8 only).
        let content = match std::fs::read_to_string(file_path) {
            Ok(c) => c,
            Err(_) => {
                debug!(
                    "KnowledgeFileScanner: skipping {} — failed to read (not UTF-8?)",
                    file_path.display()
                );
                return;
            }
        };

        if content.trim().is_empty() {
            return;
        }

        sources.push(KnowledgeSource {
            path: file_path.to_path_buf(),
            content,
            priority,
            source_type,
        });
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_scan_finds_hive_md() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("HIVE.md"), "# Project Instructions\nUse Rust.").unwrap();

        let sources = KnowledgeFileScanner::scan(dir.path());

        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].source_type, KnowledgeSourceType::HiveInstructions);
        assert_eq!(sources[0].priority, 1.0);
        assert!(sources[0].content.contains("Use Rust"));
    }

    #[test]
    fn test_scan_returns_empty_for_empty_dir() {
        let dir = TempDir::new().unwrap();

        let sources = KnowledgeFileScanner::scan(dir.path());

        assert!(sources.is_empty());
    }

    #[test]
    fn test_scan_respects_size_cap() {
        let dir = TempDir::new().unwrap();
        // Write a file that exceeds 50KB.
        let big_content = "x".repeat(60 * 1024);
        std::fs::write(dir.path().join("HIVE.md"), &big_content).unwrap();

        let sources = KnowledgeFileScanner::scan(dir.path());

        assert!(sources.is_empty(), "Files exceeding size cap should be skipped");
    }

    #[test]
    fn test_scan_finds_all_standard_files() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("HIVE.md"), "# Hive instructions").unwrap();
        std::fs::write(dir.path().join("README.md"), "# My Project").unwrap();
        std::fs::write(dir.path().join("CONTRIBUTING.md"), "# How to contribute").unwrap();

        let hive_dir = dir.path().join(".hive");
        std::fs::create_dir(&hive_dir).unwrap();
        std::fs::write(hive_dir.join("context.md"), "Project context here").unwrap();

        let sources = KnowledgeFileScanner::scan(dir.path());

        assert_eq!(sources.len(), 4);

        // Verify priority ordering (highest first).
        assert!(sources[0].priority >= sources[1].priority);
        assert!(sources[1].priority >= sources[2].priority);
        assert!(sources[2].priority >= sources[3].priority);

        // Verify the types are all present.
        let types: Vec<_> = sources.iter().map(|s| s.source_type).collect();
        assert!(types.contains(&KnowledgeSourceType::HiveInstructions));
        assert!(types.contains(&KnowledgeSourceType::HiveContext));
        assert!(types.contains(&KnowledgeSourceType::Readme));
        assert!(types.contains(&KnowledgeSourceType::Contributing));
    }

    #[test]
    fn test_scan_finds_custom_hive_dir_files() {
        let dir = TempDir::new().unwrap();
        let hive_dir = dir.path().join(".hive");
        std::fs::create_dir(&hive_dir).unwrap();
        std::fs::write(hive_dir.join("context.md"), "Context").unwrap();
        std::fs::write(hive_dir.join("architecture.md"), "Architecture docs").unwrap();
        std::fs::write(hive_dir.join("notes.txt"), "Not a markdown file").unwrap();

        let sources = KnowledgeFileScanner::scan(dir.path());

        // Should find context.md (HiveContext) + architecture.md (CustomContext).
        // notes.txt should be skipped (not .md).
        assert_eq!(sources.len(), 2);

        let types: Vec<_> = sources.iter().map(|s| s.source_type).collect();
        assert!(types.contains(&KnowledgeSourceType::HiveContext));
        assert!(types.contains(&KnowledgeSourceType::CustomContext));
    }

    #[test]
    fn test_scan_skips_empty_files() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("HIVE.md"), "").unwrap();
        std::fs::write(dir.path().join("README.md"), "   \n  \n  ").unwrap();

        let sources = KnowledgeFileScanner::scan(dir.path());

        assert!(sources.is_empty(), "Empty/whitespace-only files should be skipped");
    }

    #[test]
    fn test_format_for_context_empty() {
        let result = KnowledgeFileScanner::format_for_context(&[]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_format_for_context_produces_markdown() {
        let sources = vec![
            KnowledgeSource {
                path: PathBuf::from("/project/HIVE.md"),
                content: "Use Rust only.".to_string(),
                priority: 1.0,
                source_type: KnowledgeSourceType::HiveInstructions,
            },
            KnowledgeSource {
                path: PathBuf::from("/project/README.md"),
                content: "# My Project\nA cool project.".to_string(),
                priority: 0.6,
                source_type: KnowledgeSourceType::Readme,
            },
        ];

        let formatted = KnowledgeFileScanner::format_for_context(&sources);

        assert!(formatted.contains("# Project Knowledge"));
        assert!(formatted.contains("## Project AI Instructions (HIVE.md)"));
        assert!(formatted.contains("Use Rust only."));
        assert!(formatted.contains("## Project README"));
        assert!(formatted.contains("A cool project."));
    }

    #[test]
    fn test_priority_ordering() {
        let dir = TempDir::new().unwrap();
        // Create files in reverse priority order to verify sorting.
        std::fs::write(dir.path().join("CONTRIBUTING.md"), "contrib").unwrap();
        std::fs::write(dir.path().join("README.md"), "readme").unwrap();
        std::fs::write(dir.path().join("HIVE.md"), "hive").unwrap();

        let sources = KnowledgeFileScanner::scan(dir.path());

        assert_eq!(sources.len(), 3);
        assert_eq!(sources[0].source_type, KnowledgeSourceType::HiveInstructions);
        assert_eq!(sources[1].source_type, KnowledgeSourceType::Readme);
        assert_eq!(sources[2].source_type, KnowledgeSourceType::Contributing);
    }
}
