use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

use super::types::{CortexMemoryCategory, Domain, StrategyId};

/// Buffer size for the broadcast channel.
/// When full, oldest events are dropped and receivers get RecvError::Lagged.
pub const EVENT_BUS_BUFFER_SIZE: usize = 256;

/// Type alias for the event sender. Cloned and passed to all producers.
pub type CortexEventSender = broadcast::Sender<CortexEvent>;

/// Type alias for the event receiver. One per consumer (only the Cortex subscribes).
pub type CortexEventReceiver = broadcast::Receiver<CortexEvent>;

/// Creates a new event bus (sender + receiver pair).
pub fn create_event_bus() -> (CortexEventSender, CortexEventReceiver) {
    broadcast::channel(EVENT_BUS_BUFFER_SIZE)
}

/// All events use only types defined in hive_learn — no hive_agents imports.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CortexEvent {
    // ── From hive_learn ──────────────────────────────────────────────
    /// An AI interaction outcome was recorded.
    OutcomeRecorded {
        interaction_id: String,
        model: String,
        persona: Option<String>,
        quality_score: f64,
        /// Serialized from hive_learn::Outcome.
        outcome: String,
    },

    /// A routing decision was made.
    RoutingDecision {
        task_type: String,
        model_chosen: String,
        tier: u8,
        quality_result: Option<f64>,
    },

    /// A new prompt version was created.
    PromptVersionCreated {
        persona: String,
        version: u32,
        avg_quality: f64,
    },

    /// A code pattern was extracted from a high-quality response.
    PatternExtracted {
        pattern_id: String,
        language: String,
        category: String,
        quality: f64,
    },

    /// Self-evaluation completed.
    SelfEvalCompleted {
        overall_quality: f64,
        /// Serialized from hive_learn::QualityTrend.
        trend: String,
        weak_areas: Vec<String>,
    },

    // ── From hive_agents ─────────────────────────────────────────────
    /// A multi-agent swarm run completed.
    SwarmCompleted {
        goal_id: String,
        success: bool,
        agent_count: usize,
        duration_ms: u64,
        patterns_recorded: u32,
    },

    /// An entry was added to collective memory.
    CollectiveMemoryEntry {
        category: CortexMemoryCategory,
        content: String,
        relevance_score: f64,
    },

    /// Queen generated a plan.
    QueenPlanGenerated {
        goal_id: String,
        team_count: usize,
        memory_context_used: bool,
    },

    // ── From AutoResearch ────────────────────────────────────────────
    /// A skill evaluation completed.
    SkillEvalCompleted {
        skill_id: String,
        pass_rate: f64,
        iteration: u32,
    },

    /// A prompt was mutated (and potentially promoted).
    PromptMutated {
        skill_id: String,
        old_pass_rate: f64,
        new_pass_rate: f64,
        promoted: bool,
    },

    // ── From Cortex itself (meta-events) ─────────────────────────────
    /// An improvement was applied.
    ImprovementApplied {
        domain: Domain,
        action: String,
        expected_impact: f64,
    },

    /// An improvement was rolled back.
    ImprovementRolledBack {
        domain: Domain,
        action: String,
        reason: String,
    },

    /// A strategy weight was adjusted.
    StrategyWeightAdjusted {
        strategy: StrategyId,
        old_weight: f64,
        new_weight: f64,
    },
}

impl CortexEvent {
    /// Returns the event type as a string for persistence.
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::OutcomeRecorded { .. } => "outcome_recorded",
            Self::RoutingDecision { .. } => "routing_decision",
            Self::PromptVersionCreated { .. } => "prompt_version_created",
            Self::PatternExtracted { .. } => "pattern_extracted",
            Self::SelfEvalCompleted { .. } => "self_eval_completed",
            Self::SwarmCompleted { .. } => "swarm_completed",
            Self::CollectiveMemoryEntry { .. } => "collective_memory_entry",
            Self::QueenPlanGenerated { .. } => "queen_plan_generated",
            Self::SkillEvalCompleted { .. } => "skill_eval_completed",
            Self::PromptMutated { .. } => "prompt_mutated",
            Self::ImprovementApplied { .. } => "improvement_applied",
            Self::ImprovementRolledBack { .. } => "improvement_rolled_back",
            Self::StrategyWeightAdjusted { .. } => "strategy_weight_adjusted",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_event_bus() {
        let (tx, _rx) = create_event_bus();
        // Should be able to clone sender
        let _tx2 = tx.clone();
    }

    #[test]
    fn test_send_receive_event() {
        let (tx, mut rx) = create_event_bus();
        let event = CortexEvent::OutcomeRecorded {
            interaction_id: "test-001".to_string(),
            model: "claude-3".to_string(),
            persona: Some("coder".to_string()),
            quality_score: 0.85,
            outcome: "accepted".to_string(),
        };
        tx.send(event).unwrap();

        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        rt.block_on(async {
            let received = rx.recv().await.unwrap();
            assert_eq!(received.event_type(), "outcome_recorded");
        });
    }

    #[test]
    fn test_event_serde_roundtrip() {
        let event = CortexEvent::SwarmCompleted {
            goal_id: "goal-1".to_string(),
            success: true,
            agent_count: 5,
            duration_ms: 12000,
            patterns_recorded: 3,
        };
        let json = serde_json::to_string(&event).unwrap();
        let parsed: CortexEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.event_type(), "swarm_completed");
    }

    #[test]
    fn test_event_type_strings() {
        let events: Vec<CortexEvent> = vec![
            CortexEvent::OutcomeRecorded {
                interaction_id: String::new(),
                model: String::new(),
                persona: None,
                quality_score: 0.0,
                outcome: String::new(),
            },
            CortexEvent::ImprovementApplied {
                domain: Domain::Routing,
                action: String::new(),
                expected_impact: 0.0,
            },
            CortexEvent::StrategyWeightAdjusted {
                strategy: StrategyId::PromptMutation,
                old_weight: 0.5,
                new_weight: 0.6,
            },
        ];
        let expected = [
            "outcome_recorded",
            "improvement_applied",
            "strategy_weight_adjusted",
        ];
        for (event, exp) in events.iter().zip(expected.iter()) {
            assert_eq!(event.event_type(), *exp);
        }
    }

    #[test]
    fn test_buffer_overflow_drops_old() {
        let (tx, mut rx) = create_event_bus();
        // Fill buffer + 10 extra
        for i in 0..(EVENT_BUS_BUFFER_SIZE + 10) {
            let _ = tx.send(CortexEvent::OutcomeRecorded {
                interaction_id: format!("test-{}", i),
                model: "test".to_string(),
                persona: None,
                quality_score: 0.5,
                outcome: "unknown".to_string(),
            });
        }

        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        rt.block_on(async {
            // Should get Lagged error
            match rx.recv().await {
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    assert!(n > 0);
                }
                _ => panic!("Expected Lagged error"),
            }
        });
    }
}
