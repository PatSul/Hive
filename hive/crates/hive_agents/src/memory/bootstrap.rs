//! Bootstrap file generation (IDENTITY.md, MEMORY.md, CONTEXT.md).
//!
//! Generates and loads bootstrap context files from the tiered memory system.
//! These files provide an agent with identity, accumulated knowledge, and
//! current task context at session startup.

use std::path::{Path, PathBuf};

use crate::collective_memory::MemoryEntry;
use super::archive::ArchiveEntry;
use super::session_state::{Decision, SessionState};

// ---------------------------------------------------------------------------
// BootstrapContext
// ---------------------------------------------------------------------------

/// Loaded bootstrap context for session startup.
///
/// Each field is `Some` only if the corresponding file existed on disk
/// when [`BootstrapGenerator::load_bootstrap`] was called.
#[derive(Debug, Clone)]
pub struct BootstrapContext {
    /// Contents of `IDENTITY.md`, if it exists.
    pub identity: Option<String>,
    /// Contents of `MEMORY.md`, if it exists.
    pub memory: Option<String>,
    /// Contents of `CONTEXT.md`, if it exists.
    pub context: Option<String>,
}

// ---------------------------------------------------------------------------
// BootstrapGenerator
// ---------------------------------------------------------------------------

/// Generates bootstrap files from memory data.
///
/// Produces three markdown files in `memory_dir`:
/// - `IDENTITY.md` — user preferences and personality traits
/// - `MEMORY.md` — key knowledge and recent archive entries
/// - `CONTEXT.md` — current task, active context, and recent decisions
pub struct BootstrapGenerator {
    memory_dir: PathBuf,
}

impl BootstrapGenerator {
    /// Create a new generator that writes files to `memory_dir`.
    pub fn new(memory_dir: PathBuf) -> Self {
        Self { memory_dir }
    }

    /// Path to the identity bootstrap file.
    pub fn identity_path(&self) -> PathBuf {
        self.memory_dir.join("IDENTITY.md")
    }

    /// Path to the memory bootstrap file.
    pub fn memory_path(&self) -> PathBuf {
        self.memory_dir.join("MEMORY.md")
    }

    /// Path to the context bootstrap file.
    pub fn context_path(&self) -> PathBuf {
        self.memory_dir.join("CONTEXT.md")
    }

    /// Generate the identity markdown from user preference entries.
    ///
    /// Entries are sorted by `relevance_score` descending and capped at 20.
    pub fn generate_identity(&self, preferences: &[MemoryEntry]) -> Result<String, String> {
        let mut sorted: Vec<&MemoryEntry> = preferences.iter().collect();
        sorted.sort_by(|a, b| {
            b.relevance_score
                .partial_cmp(&a.relevance_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        sorted.truncate(20);

        let mut md = String::from("# Identity\n\n## User Preferences\n");
        for entry in &sorted {
            md.push_str(&format!(
                "- {} [score: {:.2}]\n",
                entry.content, entry.relevance_score
            ));
        }
        Ok(md)
    }

    /// Generate the memory markdown from knowledge entries and archive logs.
    ///
    /// Takes top 30 memories by relevance and top 10 recent archive entries.
    pub fn generate_memory(
        &self,
        memories: &[MemoryEntry],
        recent_logs: &[ArchiveEntry],
    ) -> Result<String, String> {
        let mut sorted: Vec<&MemoryEntry> = memories.iter().collect();
        sorted.sort_by(|a, b| {
            b.relevance_score
                .partial_cmp(&a.relevance_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        sorted.truncate(30);

        let mut md = String::from("# Memory\n\n## Key Knowledge\n");
        for entry in &sorted {
            md.push_str(&format!(
                "- {} [{}]\n",
                entry.content, entry.category
            ));
        }

        md.push_str("\n## Recent Archive\n");
        let archive_count = recent_logs.len().min(10);
        for entry in &recent_logs[..archive_count] {
            md.push_str(&format!("- [{}] {}\n", entry.date, entry.content));
        }

        Ok(md)
    }

    /// Generate the context markdown from session state and recent decisions.
    pub fn generate_context(
        &self,
        session: &SessionState,
        recent_decisions: &[Decision],
    ) -> Result<String, String> {
        let task_text = session
            .current_task
            .as_deref()
            .unwrap_or("No active task");

        let mut md = String::from("# Context\n\n## Current Task\n");
        md.push_str(task_text);
        md.push('\n');

        md.push_str("\n## Active Context\n");
        for item in &session.active_context {
            md.push_str(&format!("- {}\n", item));
        }

        md.push_str("\n## Recent Decisions\n");
        for decision in recent_decisions {
            let rationale = decision
                .rationale
                .as_deref()
                .unwrap_or("no rationale");
            md.push_str(&format!("- {} ({})\n", decision.content, rationale));
        }

        Ok(md)
    }

    /// Write all three bootstrap files to `memory_dir`.
    ///
    /// Creates the directory tree if it does not exist.
    pub fn write_all(&self, identity: &str, memory: &str, context: &str) -> Result<(), String> {
        std::fs::create_dir_all(&self.memory_dir)
            .map_err(|e| format!("failed to create memory dir: {e}"))?;

        std::fs::write(self.identity_path(), identity.as_bytes())
            .map_err(|e| format!("failed to write IDENTITY.md: {e}"))?;

        std::fs::write(self.memory_path(), memory.as_bytes())
            .map_err(|e| format!("failed to write MEMORY.md: {e}"))?;

        std::fs::write(self.context_path(), context.as_bytes())
            .map_err(|e| format!("failed to write CONTEXT.md: {e}"))?;

        Ok(())
    }

    /// Load bootstrap context from disk.
    ///
    /// Each file is read independently; missing files result in `None` for
    /// that field rather than an error.
    pub fn load_bootstrap(&self) -> Result<BootstrapContext, String> {
        let identity = read_optional(&self.identity_path())?;
        let memory = read_optional(&self.memory_path())?;
        let context = read_optional(&self.context_path())?;

        Ok(BootstrapContext {
            identity,
            memory,
            context,
        })
    }
}

/// Read a file if it exists, returning `None` for missing files.
fn read_optional(path: &Path) -> Result<Option<String>, String> {
    if !path.exists() {
        return Ok(None);
    }
    std::fs::read_to_string(path)
        .map(Some)
        .map_err(|e| format!("failed to read {}: {e}", path.display()))
}
