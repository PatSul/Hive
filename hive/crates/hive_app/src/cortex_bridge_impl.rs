//! Concrete implementation of [`hive_learn::cortex::bridge::CortexBridge`].
//!
//! Lives in `hive_app` because it depends on both `hive_learn` (for the trait)
//! and `hive_agents` (for `CollectiveMemory` / `MemoryEntry`).

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use chrono::DateTime;

use hive_agents::collective_memory::{CollectiveMemory, MemoryCategory, MemoryEntry};
use hive_learn::cortex::bridge::{CortexBridge, BRIDGE_RELEVANCE_CAP};
use hive_learn::cortex::types::BridgedMemoryEntry;

/// Bridge between `LearningCortex` (hive_learn) and `CollectiveMemory`
/// (hive_agents).
///
/// Converts between the two crates' types and enforces relevance-score capping.
pub struct CortexBridgeImpl {
    collective: Arc<CollectiveMemory>,
    /// In-memory set of content hashes that have already been bridged.
    seen_hashes: Mutex<HashSet<[u8; 32]>>,
}

impl CortexBridgeImpl {
    pub fn new(collective: Arc<CollectiveMemory>) -> Self {
        Self {
            collective,
            seen_hashes: Mutex::new(HashSet::new()),
        }
    }
}

impl CortexBridge for CortexBridgeImpl {
    fn read_collective_entries(&self, since: i64, limit: usize) -> Vec<BridgedMemoryEntry> {
        // Read all recent entries from collective memory via a broad query.
        let entries = match self.collective.recall("", None, None, limit) {
            Ok(e) => e,
            Err(_) => return Vec::new(),
        };

        entries
            .into_iter()
            .filter_map(|entry| {
                // Convert RFC 3339 timestamp to epoch seconds
                let epoch = rfc3339_to_epoch(&entry.created_at)?;
                if epoch < since {
                    return None;
                }

                Some(BridgedMemoryEntry {
                    category: cortex_category_str(entry.category),
                    content: entry.content,
                    relevance_score: entry.relevance_score,
                    timestamp_epoch: epoch,
                })
            })
            .collect()
    }

    fn write_to_collective(
        &self,
        category: String,
        content: String,
        relevance_score: f64,
    ) -> Result<()> {
        // Enforce relevance cap
        let capped_score = relevance_score.min(BRIDGE_RELEVANCE_CAP);

        let mem_category = parse_cortex_category(&category);
        let mut entry = MemoryEntry::new(mem_category, &content);
        entry.relevance_score = capped_score;

        self.collective
            .remember(&entry)
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        Ok(())
    }

    fn content_hash_exists(&self, hash: &[u8; 32]) -> bool {
        match self.seen_hashes.lock() {
            Ok(guard) => guard.contains(hash),
            Err(_) => false,
        }
    }

    fn store_content_hash(&self, hash: [u8; 32]) -> Result<()> {
        let mut guard = self
            .seen_hashes
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {e}"))?;
        guard.insert(hash);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Conversion helpers
// ---------------------------------------------------------------------------

/// Convert `MemoryCategory` (hive_agents) to the string form used by
/// `CortexMemoryCategory` (hive_learn).
fn cortex_category_str(cat: MemoryCategory) -> String {
    match cat {
        MemoryCategory::SuccessPattern => "success_pattern",
        MemoryCategory::FailurePattern => "failure_pattern",
        MemoryCategory::ModelInsight => "model_insight",
        MemoryCategory::ConflictResolution => "conflict_resolution",
        MemoryCategory::CodePattern => "code_pattern",
        MemoryCategory::UserPreference => "user_preference",
        MemoryCategory::General => "general",
    }
    .to_string()
}

/// Convert a `CortexMemoryCategory` string back to `MemoryCategory`
/// (hive_agents).
fn parse_cortex_category(s: &str) -> MemoryCategory {
    match s {
        "success_pattern" => MemoryCategory::SuccessPattern,
        "failure_pattern" => MemoryCategory::FailurePattern,
        "model_insight" => MemoryCategory::ModelInsight,
        "conflict_resolution" => MemoryCategory::ConflictResolution,
        "code_pattern" => MemoryCategory::CodePattern,
        "user_preference" => MemoryCategory::UserPreference,
        _ => MemoryCategory::General,
    }
}

/// Parse an RFC 3339 timestamp string to Unix epoch seconds.
/// Returns `None` if the string is malformed.
fn rfc3339_to_epoch(s: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.timestamp())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rfc3339_to_epoch() {
        let ts = "2025-01-15T10:30:00+00:00";
        let epoch = rfc3339_to_epoch(ts).unwrap();
        assert!(epoch > 0);

        assert!(rfc3339_to_epoch("not-a-timestamp").is_none());
    }

    #[test]
    fn test_cortex_category_str_roundtrip() {
        let categories = [
            MemoryCategory::SuccessPattern,
            MemoryCategory::FailurePattern,
            MemoryCategory::ModelInsight,
            MemoryCategory::ConflictResolution,
            MemoryCategory::CodePattern,
            MemoryCategory::UserPreference,
            MemoryCategory::General,
        ];

        for cat in categories {
            let s = cortex_category_str(cat);
            let parsed = parse_cortex_category(&s);
            // MemoryCategory doesn't implement PartialEq for direct comparison,
            // but as_str() gives us a stable string representation.
            assert_eq!(cat.as_str(), parsed.as_str());
        }
    }

    #[test]
    fn test_parse_unknown_category() {
        let cat = parse_cortex_category("unknown_category");
        assert_eq!(cat.as_str(), "General");
    }

    #[test]
    fn test_bridge_impl_hash_store() {
        let mem = CollectiveMemory::in_memory().unwrap();
        let bridge = CortexBridgeImpl::new(Arc::new(mem));

        let hash = [42u8; 32];
        assert!(!bridge.content_hash_exists(&hash));
        bridge.store_content_hash(hash).unwrap();
        assert!(bridge.content_hash_exists(&hash));
    }

    #[test]
    fn test_bridge_impl_write_caps_relevance() {
        let mem = Arc::new(CollectiveMemory::in_memory().unwrap());
        let bridge = CortexBridgeImpl::new(Arc::clone(&mem));

        // Write with relevance 1.0 — should be capped to 0.6
        bridge
            .write_to_collective(
                "success_pattern".to_string(),
                "test insight".to_string(),
                1.0,
            )
            .unwrap();

        // Read back and verify capping
        let entries = mem.recall("test insight", None, None, 10).unwrap();
        assert_eq!(entries.len(), 1);
        assert!(
            (entries[0].relevance_score - BRIDGE_RELEVANCE_CAP).abs() < f64::EPSILON,
            "Expected relevance capped at {BRIDGE_RELEVANCE_CAP}, got {}",
            entries[0].relevance_score
        );
    }

    #[test]
    fn test_bridge_impl_read_entries() {
        let mem = Arc::new(CollectiveMemory::in_memory().unwrap());

        // Insert an entry directly
        let entry = MemoryEntry::new(MemoryCategory::ModelInsight, "routing tip");
        mem.remember(&entry).unwrap();

        let bridge = CortexBridgeImpl::new(Arc::clone(&mem));
        let bridged = bridge.read_collective_entries(0, 100);
        assert!(!bridged.is_empty());
        assert_eq!(bridged[0].category, "model_insight");
        assert!(bridged[0].content.contains("routing tip"));
    }
}
