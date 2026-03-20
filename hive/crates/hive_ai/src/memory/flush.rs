//! Memory extraction from conversation context before compaction.
//!
//! When the context window is about to be compacted (trimmed), this module
//! extracts durable insights (user preferences, decisions, code patterns)
//! and persists them to HiveMemory so they survive context trimming.

use serde::{Deserialize, Serialize};

/// A memory extracted from a conversation before compaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedMemory {
    pub content: String,
    pub importance: f32,
    pub category: String,
}

/// Builds prompts for memory extraction and parses LLM responses.
pub struct MemoryExtractor;

impl MemoryExtractor {
    /// Build the system prompt for extracting memories from conversation messages.
    pub fn build_prompt(messages: &[String]) -> String {
        let conversation = messages.join("\n");
        format!(
            r#"You are a memory extraction assistant. Analyze the following conversation and extract key memories that should be preserved long-term.

For each memory, provide:
- content: A concise statement of the memory (1-2 sentences)
- importance: A score from 1-10 (10 = critical insight, 1 = trivial detail)
- category: One of "user_preference", "code_pattern", "task_progress", "decision", "general"

Only extract memories with importance >= 5. Focus on:
- User preferences (coding style, tools, languages)
- Architectural decisions made during the conversation
- Code patterns discovered or established
- Task milestones and progress

Respond with ONLY a JSON array of memory objects. No markdown, no explanation.

Example response:
[{{"content": "User prefers Rust for backend services", "importance": 8, "category": "user_preference"}}]

Conversation to analyze:
{conversation}"#
        )
    }

    /// Parse the LLM's JSON response into extracted memories.
    pub fn parse_response(json_str: &str) -> Result<Vec<ExtractedMemory>, String> {
        // Try to find JSON array in the response (strip markdown if present)
        let trimmed = json_str.trim();
        let json = if trimmed.starts_with("```") {
            // Strip markdown code fences
            trimmed
                .trim_start_matches("```json")
                .trim_start_matches("```")
                .trim_end_matches("```")
                .trim()
        } else {
            trimmed
        };

        serde_json::from_str::<Vec<ExtractedMemory>>(json)
            .map_err(|e| format!("Failed to parse memory extraction response: {e}"))
    }

    /// Filter extracted memories to only include those above the importance threshold.
    pub fn filter_by_importance(
        memories: Vec<ExtractedMemory>,
        threshold: f32,
    ) -> Vec<ExtractedMemory> {
        memories
            .into_iter()
            .filter(|m| m.importance >= threshold)
            .collect()
    }
}
