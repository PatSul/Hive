use std::collections::HashSet;
use std::sync::Arc;

use anyhow::Result;

use super::event_bus::CortexEvent;
use super::types::BridgedMemoryEntry;

/// Maximum relevance score for cross-context writes.
/// Insights bridged from individual to collective (or vice-versa) are capped
/// here to enforce relevance decay across system boundaries.
pub const BRIDGE_RELEVANCE_CAP: f64 = 0.6;

/// Cross-crate interface between LearningCortex (hive_learn) and
/// CollectiveMemory (hive_agents).
///
/// Defined here as a trait to avoid circular dependencies.
/// Concrete implementation (CortexBridgeImpl) lives in hive_app.
pub trait CortexBridge: Send + Sync {
    /// Read recent entries from collective memory.
    fn read_collective_entries(&self, since: i64, limit: usize) -> Vec<BridgedMemoryEntry>;

    /// Write an insight to collective memory.
    /// Relevance score MUST be capped at [`BRIDGE_RELEVANCE_CAP`] (0.6) for
    /// cross-context decay.  Implementations must clamp the incoming score.
    fn write_to_collective(
        &self,
        category: String,
        content: String,
        relevance_score: f64,
    ) -> Result<()>;

    /// Check if a content hash already exists (for deduplication).
    fn content_hash_exists(&self, hash: &[u8; 32]) -> bool;

    /// Store a content hash after bridging (for deduplication).
    fn store_content_hash(&self, hash: [u8; 32]) -> Result<()>;
}

// ---------------------------------------------------------------------------
// Content-hash deduplication
// ---------------------------------------------------------------------------

/// Compute SHA-256 hash of category + content for deduplication.
pub fn compute_content_hash(category: &str, content: &str) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(category.as_bytes());
    hasher.update(b"|");
    hasher.update(content.as_bytes());
    let result = hasher.finalize();
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&result);
    hash
}

/// In-memory deduplication filter for bridge operations.
///
/// Tracks content hashes that have already been processed to prevent the same
/// insight from being bridged repeatedly within a single session.
pub struct BridgeDeduplicator {
    seen_hashes: HashSet<[u8; 32]>,
}

impl BridgeDeduplicator {
    pub fn new() -> Self {
        Self {
            seen_hashes: HashSet::new(),
        }
    }

    /// Returns `true` if this category+content pair has already been seen.
    pub fn is_duplicate(&self, category: &str, content: &str) -> bool {
        let hash = compute_content_hash(category, content);
        self.seen_hashes.contains(&hash)
    }

    /// Record a category+content pair as seen.
    pub fn mark_seen(&mut self, category: &str, content: &str) {
        let hash = compute_content_hash(category, content);
        self.seen_hashes.insert(hash);
    }

    /// Number of hashes currently tracked (useful for diagnostics).
    pub fn len(&self) -> usize {
        self.seen_hashes.len()
    }

    /// Returns `true` if no hashes have been recorded.
    pub fn is_empty(&self) -> bool {
        self.seen_hashes.is_empty()
    }
}

impl Default for BridgeDeduplicator {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// BridgeAction — the output of the bridge processor
// ---------------------------------------------------------------------------

/// Actions that the bridge processor can emit after analysing an entry or event.
#[derive(Debug, Clone, PartialEq)]
pub enum BridgeAction {
    /// A success pattern in collective memory suggests refining a persona prompt.
    SuggestPromptRefinement { persona: String, evidence: String },

    /// A model insight in collective memory suggests adjusting routing tables.
    SuggestRoutingAdjustment { task_type: String, insight: String },

    /// An individual learning event should be written back to collective memory.
    WriteToCollective { category: String, content: String },

    /// No action required.
    Noop,
}

// ---------------------------------------------------------------------------
// BridgeProcessor — routes entries/events through dedup + action mapping
// ---------------------------------------------------------------------------

/// Orchestrates bridging between collective memory and the individual learning
/// cortex.  Holds a `CortexBridge` for I/O and a `BridgeDeduplicator` to avoid
/// processing the same content twice.
pub struct BridgeProcessor {
    bridge: Arc<dyn CortexBridge>,
    dedup: BridgeDeduplicator,
}

impl BridgeProcessor {
    pub fn new(bridge: Arc<dyn CortexBridge>) -> Self {
        Self {
            bridge,
            dedup: BridgeDeduplicator::new(),
        }
    }

    /// Process a collective-memory entry into an action for the individual cortex.
    ///
    /// - `success_pattern` entries become prompt-refinement suggestions.
    /// - `model_insight` entries become routing-adjustment suggestions.
    /// - Duplicates (same category+content already processed) are skipped.
    pub fn process_collective_to_individual(&mut self, entry: &BridgedMemoryEntry) -> BridgeAction {
        // Dedup check
        if self.dedup.is_duplicate(&entry.category, &entry.content) {
            return BridgeAction::Noop;
        }

        let action = match entry.category.as_str() {
            "success_pattern" => BridgeAction::SuggestPromptRefinement {
                persona: extract_persona(&entry.content),
                evidence: entry.content.clone(),
            },
            "model_insight" => BridgeAction::SuggestRoutingAdjustment {
                task_type: extract_task_type(&entry.content),
                insight: entry.content.clone(),
            },
            _ => BridgeAction::Noop,
        };

        // Mark as seen regardless of whether an action was produced
        self.dedup.mark_seen(&entry.category, &entry.content);
        action
    }

    /// Process an individual cortex event into an action that may write to
    /// collective memory.
    ///
    /// - `PromptVersionCreated` with improving quality writes the version info.
    /// - `PatternExtracted` with quality > 0.8 writes the pattern.
    /// - Duplicates are skipped.
    pub fn process_individual_to_collective(&mut self, event: &CortexEvent) -> BridgeAction {
        match event {
            CortexEvent::PromptVersionCreated {
                persona,
                version,
                avg_quality,
            } => {
                // Only bridge if quality is improving (version > 1 implies iteration)
                if *version <= 1 {
                    return BridgeAction::Noop;
                }

                let category = "success_pattern".to_string();
                let content = format!(
                    "Prompt v{version} for persona '{persona}' achieved avg quality {avg_quality:.2}"
                );

                if self.dedup.is_duplicate(&category, &content) {
                    return BridgeAction::Noop;
                }

                self.dedup.mark_seen(&category, &content);
                BridgeAction::WriteToCollective { category, content }
            }
            CortexEvent::PatternExtracted {
                pattern_id,
                language,
                category,
                quality,
            } => {
                if *quality <= 0.8 {
                    return BridgeAction::Noop;
                }

                let cat = "code_pattern".to_string();
                let content =
                    format!("Pattern '{pattern_id}' ({language}/{category}) quality {quality:.2}");

                if self.dedup.is_duplicate(&cat, &content) {
                    return BridgeAction::Noop;
                }

                self.dedup.mark_seen(&cat, &content);
                BridgeAction::WriteToCollective {
                    category: cat,
                    content,
                }
            }
            _ => BridgeAction::Noop,
        }
    }

    /// Execute a `WriteToCollective` action through the bridge, capping
    /// relevance at [`BRIDGE_RELEVANCE_CAP`].
    pub fn execute_write(&self, category: &str, content: &str) -> Result<()> {
        self.bridge.write_to_collective(
            category.to_string(),
            content.to_string(),
            BRIDGE_RELEVANCE_CAP,
        )
    }

    /// Access the underlying bridge (e.g., for reading entries).
    pub fn bridge(&self) -> &dyn CortexBridge {
        self.bridge.as_ref()
    }

    /// Clone the underlying bridge handle.
    pub fn bridge_arc(&self) -> Arc<dyn CortexBridge> {
        Arc::clone(&self.bridge)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Best-effort extraction of a persona name from content text.
/// Falls back to "default" if no persona marker is found.
fn extract_persona(content: &str) -> String {
    // Look for patterns like "persona 'foo'" or "persona: foo"
    if let Some(start) = content.find("persona '") {
        let rest = &content[start + 9..];
        if let Some(end) = rest.find('\'') {
            return rest[..end].to_string();
        }
    }
    if let Some(start) = content.find("persona: ") {
        let rest = &content[start + 9..];
        let end = rest
            .find(|c: char| c == ',' || c == ')' || c == '\n')
            .unwrap_or(rest.len());
        return rest[..end].trim().to_string();
    }
    "default".to_string()
}

/// Best-effort extraction of a task type from content text.
/// Falls back to "general" if no task-type marker is found.
fn extract_task_type(content: &str) -> String {
    // Look for patterns like "task_type: foo" or "for task 'foo'"
    if let Some(start) = content.find("task_type: ") {
        let rest = &content[start + 11..];
        let end = rest
            .find(|c: char| c == ',' || c == ')' || c == '\n')
            .unwrap_or(rest.len());
        return rest[..end].trim().to_string();
    }
    if let Some(start) = content.find("for task '") {
        let rest = &content[start + 10..];
        if let Some(end) = rest.find('\'') {
            return rest[..end].to_string();
        }
    }
    "general".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_hash_deterministic() {
        let h1 = compute_content_hash("success_pattern", "structured output works");
        let h2 = compute_content_hash("success_pattern", "structured output works");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_content_hash_different_for_different_input() {
        let h1 = compute_content_hash("success_pattern", "structured output works");
        let h2 = compute_content_hash("failure_pattern", "structured output works");
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_deduplicator_rejects_duplicates() {
        let mut dedup = BridgeDeduplicator::new();
        assert!(!dedup.is_duplicate("cat", "content"));
        dedup.mark_seen("cat", "content");
        assert!(dedup.is_duplicate("cat", "content"));
    }

    #[test]
    fn test_deduplicator_different_content_not_duplicate() {
        let mut dedup = BridgeDeduplicator::new();
        dedup.mark_seen("cat", "content_a");
        assert!(!dedup.is_duplicate("cat", "content_b"));
    }

    #[test]
    fn test_deduplicator_len() {
        let mut dedup = BridgeDeduplicator::new();
        assert!(dedup.is_empty());
        dedup.mark_seen("a", "b");
        assert_eq!(dedup.len(), 1);
        // Same content again doesn't increase count
        dedup.mark_seen("a", "b");
        assert_eq!(dedup.len(), 1);
        dedup.mark_seen("c", "d");
        assert_eq!(dedup.len(), 2);
    }

    #[test]
    fn test_bridge_relevance_cap_value() {
        // Ensure the constant is exactly 0.6
        assert!((BRIDGE_RELEVANCE_CAP - 0.6).abs() < f64::EPSILON);
    }

    #[test]
    fn test_extract_persona() {
        assert_eq!(extract_persona("persona 'coder' improved"), "coder");
        assert_eq!(extract_persona("persona: analyst, version 3"), "analyst");
        assert_eq!(extract_persona("no persona here"), "default");
    }

    #[test]
    fn test_extract_task_type() {
        assert_eq!(
            extract_task_type("task_type: code_review, done"),
            "code_review"
        );
        assert_eq!(
            extract_task_type("for task 'debugging' quality 0.9"),
            "debugging"
        );
        assert_eq!(extract_task_type("nothing special"), "general");
    }
}
