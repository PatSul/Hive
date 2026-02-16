//! Trait-based bridge for bidirectional sync between CollectiveMemory and
//! LearningService.
//!
//! The trait lives in `hive_learn` (which does **not** depend on `hive_agents`).
//! The concrete implementation lives in `hive_ui` which depends on both crates.

use serde::{Deserialize, Serialize};

/// A single insight produced during a sync operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeInsight {
    pub category: String,
    pub content: String,
    pub tags: Vec<String>,
    pub relevance_score: f64,
}

/// Bidirectional bridge between CollectiveMemory and LearningService.
///
/// Implementations are responsible for:
/// 1. Pushing recent learning outcomes into collective memory so that all
///    agents can benefit from them.
/// 2. Pulling relevant memories back into the learning subsystem so the
///    preference model / prompt evolver can incorporate collective knowledge.
/// 3. Syncing model-level insights (quality, routing adjustments) into
///    collective memory.
pub trait LearningBridge: Send + Sync {
    /// Push recent outcomes from `OutcomeTracker` into collective memory.
    /// Returns the number of entries written.
    fn sync_outcomes_to_memory(&self) -> Result<usize, String>;

    /// Pull relevant memories from collective memory and feed them into the
    /// learning subsystem. Returns the number of entries integrated.
    fn sync_memory_to_learning(&self) -> Result<usize, String>;

    /// Push model-level quality insights to collective memory.
    /// Returns the number of insights written.
    fn sync_model_insights(&self) -> Result<usize, String>;

    /// Run all three sync phases and return total entries processed.
    fn full_sync(&self) -> Result<usize, String> {
        let a = self.sync_outcomes_to_memory()?;
        let b = self.sync_memory_to_learning()?;
        let c = self.sync_model_insights()?;
        Ok(a + b + c)
    }
}
