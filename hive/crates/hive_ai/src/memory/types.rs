use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MemoryCategory {
    UserPreference,
    CodePattern,
    TaskProgress,
    Decision,
    General,
}

impl MemoryCategory {
    pub fn as_str(&self) -> &str {
        match self {
            Self::UserPreference => "user_preference",
            Self::CodePattern => "code_pattern",
            Self::TaskProgress => "task_progress",
            Self::Decision => "decision",
            Self::General => "general",
        }
    }
}

#[derive(Debug, Clone)]
pub struct MemoryEntry {
    pub content: String,
    pub category: MemoryCategory,
    pub importance: f32,
    pub conversation_id: String,
    pub decay_exempt: bool,
}

#[derive(Debug, Clone)]
pub struct MemoryResult {
    pub content: String,
    pub category: String,
    pub importance: f32,
    pub score: f32,
    pub timestamp: String,
}

#[derive(Debug, Clone)]
pub struct ChunkResult {
    pub source_file: String,
    pub content: String,
    pub start_line: u32,
    pub end_line: u32,
    pub score: f32,
}

#[derive(Debug, Clone, Default)]
pub struct StoreStats {
    pub total_chunks: usize,
    pub total_memories: usize,
    pub indexed_files: usize,
}
