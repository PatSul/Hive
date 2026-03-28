use std::collections::HashSet;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use hive_learn::cortex::bridge::CortexBridge;
use hive_learn::cortex::event_bus::create_event_bus;
use hive_learn::cortex::types::{ChangeStatus, CortexChange, Domain, Tier};
use hive_learn::cortex::{AutoresearchTrigger, LearningCortex, LearningCortexRuntime};
use hive_learn::storage::LearningStorage;

#[derive(Default)]
struct MockBridge {
    writes: Arc<Mutex<Vec<(String, String, f64)>>>,
    hashes: Arc<Mutex<HashSet<[u8; 32]>>>,
}

impl CortexBridge for MockBridge {
    fn read_collective_entries(
        &self,
        _since: i64,
        _limit: usize,
    ) -> Vec<hive_learn::cortex::types::BridgedMemoryEntry> {
        Vec::new()
    }

    fn write_to_collective(
        &self,
        category: String,
        content: String,
        relevance_score: f64,
    ) -> anyhow::Result<()> {
        self.writes
            .lock()
            .unwrap()
            .push((category, content, relevance_score));
        Ok(())
    }

    fn content_hash_exists(&self, hash: &[u8; 32]) -> bool {
        self.hashes.lock().unwrap().contains(hash)
    }

    fn store_content_hash(&self, hash: [u8; 32]) -> anyhow::Result<()> {
        self.hashes.lock().unwrap().insert(hash);
        Ok(())
    }
}

#[derive(Default)]
struct MockTrigger {
    calls: Arc<Mutex<Vec<String>>>,
}

impl AutoresearchTrigger for MockTrigger {
    fn trigger(&self, subject: &str) -> Result<(), String> {
        self.calls.lock().unwrap().push(subject.to_string());
        Ok(())
    }
}

fn make_change(change_id: &str, tier: Tier, quality_before: f64, soak_until: i64) -> CortexChange {
    CortexChange {
        change_id: change_id.to_string(),
        domain: Domain::Prompts,
        tier,
        action: "{\"prompt\":\"improve\"}".to_string(),
        prior_state: "{\"prompt\":\"old\"}".to_string(),
        applied_at: chrono::Utc::now().timestamp(),
        soak_until,
        status: ChangeStatus::Soaking,
        quality_before: Some(quality_before),
        quality_after: Some(quality_before + 0.05),
    }
}

#[test]
fn runtime_processes_bus_events_and_soak_passes() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(async {
        let storage = Arc::new(LearningStorage::in_memory().unwrap());
        let (tx, rx) = create_event_bus();
        let cortex = LearningCortex::new(Arc::clone(&storage), tx.clone());
        cortex
            .interaction_tracker()
            .store(chrono::Utc::now().timestamp() - 120, Ordering::Relaxed);

        let expired_change = make_change(
            "change-1",
            Tier::Yellow,
            0.82,
            chrono::Utc::now().timestamp() - 3600,
        );
        cortex.insert_change(&expired_change).unwrap();

        let bridge = Arc::new(MockBridge::default());
        let trigger = Arc::new(MockTrigger::default());
        let bridge_trait: Arc<dyn CortexBridge> = bridge.clone();
        let trigger_trait: Arc<dyn AutoresearchTrigger> = trigger.clone();

        let handle = LearningCortexRuntime::new(cortex, rx)
            .with_bridge(bridge_trait)
            .with_autoresearch_trigger(trigger_trait)
            .spawn();

        tx.send(
            hive_learn::cortex::event_bus::CortexEvent::PromptVersionCreated {
                persona: "coder".into(),
                version: 2,
                avg_quality: 0.83,
            },
        )
        .unwrap();

        for i in 0..25 {
            tx.send(
                hive_learn::cortex::event_bus::CortexEvent::OutcomeRecorded {
                    interaction_id: format!("msg-{i}"),
                    model: "gpt-4o".into(),
                    persona: Some("coder".into()),
                    quality_score: 0.3,
                    outcome: "ignored".into(),
                },
            )
            .unwrap();
        }

        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        handle.abort();

        let writes = bridge.writes.lock().unwrap().clone();
        assert_eq!(writes.len(), 1);
        assert_eq!(writes[0].0, "success_pattern");
        assert!(writes[0].1.contains("persona 'coder'"));

        let calls = trigger.calls.lock().unwrap().clone();
        assert!(
            calls.iter().any(|subject| subject == "persona:coder"),
            "expected autoresearch trigger for degraded persona"
        );

        let verify = LearningCortex::new(Arc::clone(&storage), create_event_bus().0);
        let change = verify.load_change("change-1").unwrap().unwrap();
        assert_eq!(change.status, ChangeStatus::Confirmed);

        let prompt_events = verify
            .load_events_by_type("prompt_version_created", 10)
            .unwrap();
        assert_eq!(prompt_events.len(), 1);

        let improvement_events = verify
            .load_events_by_type("improvement_applied", 10)
            .unwrap();
        assert!(!improvement_events.is_empty());

        let strategy_weight =
            verify.strategy_weight(hive_learn::cortex::types::StrategyId::PromptMutation);
        assert!(strategy_weight > 0.5);
    });
}
