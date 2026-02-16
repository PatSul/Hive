//! Concrete implementation of `LearningBridge` that connects
//! `hive_agents::CollectiveMemory` with `hive_learn::LearningService`.

use std::sync::Arc;

use hive_agents::collective_memory::{CollectiveMemory, MemoryCategory, MemoryEntry};
use hive_learn::LearningService;
use hive_learn::learning_bridge::LearningBridge;
use tracing::info;

/// Bridge implementation wiring CollectiveMemory ↔ LearningService.
pub struct HiveLearningBridge {
    learning: Arc<LearningService>,
    memory: Arc<CollectiveMemory>,
}

impl HiveLearningBridge {
    pub fn new(learning: Arc<LearningService>, memory: Arc<CollectiveMemory>) -> Self {
        Self { learning, memory }
    }
}

impl LearningBridge for HiveLearningBridge {
    fn sync_outcomes_to_memory(&self) -> Result<usize, String> {
        let outcomes = self.learning.outcome_tracker.recent_outcomes(20)?;
        let mut count = 0;

        for record in &outcomes {
            let content = format!(
                "Model {} on task '{}' (tier {}): quality {:.2}, outcome {:?}",
                record.model_id,
                record.task_type,
                record.tier,
                record.quality_score,
                record.outcome,
            );
            let category = if record.quality_score >= 0.7 {
                MemoryCategory::SuccessPattern
            } else {
                MemoryCategory::FailurePattern
            };
            let mut entry = MemoryEntry::new(category, content);
            entry.tags = vec![
                record.model_id.clone(),
                record.task_type.clone(),
                format!("tier:{}", record.tier),
            ];
            entry.relevance_score = record.quality_score;

            self.memory.remember(&entry)?;
            count += 1;
        }

        if count > 0 {
            info!("Synced {count} outcomes → collective memory");
        }
        Ok(count)
    }

    fn sync_memory_to_learning(&self) -> Result<usize, String> {
        // Pull user-preference memories and feed them as observations.
        let memories = self.memory.recall(
            "",
            Some(MemoryCategory::UserPreference),
            None,
            20,
        )?;
        let mut count = 0;

        for mem in &memories {
            // Parse "key=value" style memories into preference observations.
            if let Some((key, value)) = mem.content.split_once('=') {
                let confidence = mem.relevance_score.clamp(0.0, 1.0);
                self.learning
                    .preference_model
                    .observe(key.trim(), value.trim(), confidence)?;
                count += 1;
            }
        }

        if count > 0 {
            info!("Synced {count} memories → learning preferences");
        }
        Ok(count)
    }

    fn sync_model_insights(&self) -> Result<usize, String> {
        let prefs = self.learning.all_preferences()?;
        let mut count = 0;

        for (key, value, confidence) in &prefs {
            let content = format!("Learned preference: {key}={value} (confidence: {confidence:.2})");
            let mut entry = MemoryEntry::new(MemoryCategory::ModelInsight, content);
            entry.tags = vec!["learned_preference".into(), key.clone()];
            entry.relevance_score = *confidence;

            self.memory.remember(&entry)?;
            count += 1;
        }

        if count > 0 {
            info!("Synced {count} model insights → collective memory");
        }
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bridge_creation() {
        let learning = Arc::new(LearningService::in_memory().unwrap());
        let memory = Arc::new(CollectiveMemory::in_memory().unwrap());
        let bridge = HiveLearningBridge::new(learning, memory);

        // Full sync on empty state should succeed with 0 entries.
        let total = bridge.full_sync().unwrap();
        assert_eq!(total, 0);
    }

    #[test]
    fn test_sync_outcomes_to_memory() {
        let learning = Arc::new(LearningService::in_memory().unwrap());
        let memory = Arc::new(CollectiveMemory::in_memory().unwrap());

        // Record an outcome.
        learning
            .on_outcome(&hive_learn::OutcomeRecord {
                conversation_id: "c1".into(),
                message_id: "m1".into(),
                model_id: "gpt-4o".into(),
                task_type: "code_gen".into(),
                tier: "standard".into(),
                persona: None,
                outcome: hive_learn::Outcome::Accepted,
                edit_distance: None,
                follow_up_count: 0,
                quality_score: 0.85,
                cost: 0.002,
                latency_ms: 400,
                timestamp: chrono::Utc::now().to_rfc3339(),
            })
            .unwrap();

        let bridge = HiveLearningBridge::new(learning, Arc::clone(&memory));
        let count = bridge.sync_outcomes_to_memory().unwrap();
        assert_eq!(count, 1);

        // Verify memory was written.
        let recalled = memory.recall("gpt-4o", None, None, 10).unwrap();
        assert!(!recalled.is_empty());
    }

    #[test]
    fn test_sync_memory_to_learning() {
        let learning = Arc::new(LearningService::in_memory().unwrap());
        let memory = Arc::new(CollectiveMemory::in_memory().unwrap());

        // Store a preference in memory.
        let mut entry = MemoryEntry::new(MemoryCategory::UserPreference, "tone=concise");
        entry.relevance_score = 0.9;
        memory.remember(&entry).unwrap();

        let bridge = HiveLearningBridge::new(Arc::clone(&learning), memory);
        let count = bridge.sync_memory_to_learning().unwrap();
        assert_eq!(count, 1);

        // Verify preference was observed.
        let pref = learning.preference_model.get("tone", 0.0).unwrap();
        assert!(pref.is_some());
    }
}
