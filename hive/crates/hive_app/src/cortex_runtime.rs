use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use chrono::Utc;
use tracing::{info, warn};

use hive_agents::collective_memory::CollectiveMemory;
use hive_ai::providers::AiProvider;
use hive_ai::types::{ChatMessage, ChatRequest, ChatResponse, MessageRole};
use hive_learn::LearningService;
use hive_learn::autoresearch::config::AutoResearchConfig;
use hive_learn::autoresearch::engine::AutoResearchEngine;
use hive_learn::autoresearch::eval_suite::{EvalQuestion, EvalSuite};
use hive_learn::autoresearch::executor::AutoResearchExecutor;
use hive_learn::cortex::bridge::CortexBridge;
use hive_learn::cortex::event_bus::{CortexEvent, CortexEventReceiver};
use hive_learn::cortex::types::{ChangeStatus, CortexChange, Domain, Tier};
use hive_learn::cortex::{AutoresearchTrigger, LearningCortex, LearningCortexRuntime};
use hive_ui::globals::CortexUiUpdate;

use crate::cortex_bridge_impl::CortexBridgeImpl;

/// Concrete autoresearch executor that reuses a provider selected from the
/// existing AI service stack.
#[derive(Clone)]
pub struct ProviderAutoResearchExecutor {
    provider: Arc<dyn AiProvider>,
    resolved_model: String,
}

impl ProviderAutoResearchExecutor {
    pub fn new(provider: Arc<dyn AiProvider>, resolved_model: String) -> Self {
        Self {
            provider,
            resolved_model,
        }
    }
}

impl AutoResearchExecutor for ProviderAutoResearchExecutor {
    fn execute(
        &self,
        request: &ChatRequest,
    ) -> impl std::future::Future<Output = Result<ChatResponse, String>> + Send {
        let provider = Arc::clone(&self.provider);
        let mut request = request.clone();
        request.model = self.resolved_model.clone();

        async move { provider.chat(&request).await.map_err(|e| e.to_string()) }
    }
}

/// Build a provider-backed executor from the app's AI service, if one exists.
pub fn build_autoresearch_executor(
    ai_service: &hive_ai::AiService,
) -> Option<ProviderAutoResearchExecutor> {
    let probe_messages = vec![ChatMessage::text(
        MessageRole::User,
        "Cortex autoresearch provider probe",
    )];

    ai_service
        .prepare_stream(probe_messages, ai_service.default_model(), None, None)
        .map(|(provider, request)| ProviderAutoResearchExecutor::new(provider, request.model))
}

struct ProviderAutoresearchTrigger {
    learning: Arc<LearningService>,
    executor: ProviderAutoResearchExecutor,
    ui_tx: std::sync::mpsc::Sender<CortexUiUpdate>,
    run_in_flight: Arc<AtomicBool>,
}

impl ProviderAutoresearchTrigger {
    fn new(
        learning: Arc<LearningService>,
        executor: ProviderAutoResearchExecutor,
        ui_tx: std::sync::mpsc::Sender<CortexUiUpdate>,
    ) -> Self {
        Self {
            learning,
            executor,
            ui_tx,
            run_in_flight: Arc::new(AtomicBool::new(false)),
        }
    }

    fn normalized_subject(subject: &str) -> &str {
        subject
            .strip_prefix("persona:")
            .or_else(|| subject.strip_prefix("model:"))
            .or_else(|| subject.strip_prefix("skill:"))
            .unwrap_or(subject)
    }
}

impl AutoresearchTrigger for ProviderAutoresearchTrigger {
    fn trigger(&self, subject: &str) -> Result<(), String> {
        if self.run_in_flight.swap(true, Ordering::SeqCst) {
            return Err("autoresearch run already in flight".to_string());
        }

        let learning = Arc::clone(&self.learning);
        let executor = self.executor.clone();
        let ui_tx = self.ui_tx.clone();
        let run_flag = Arc::clone(&self.run_in_flight);
        let run_flag_for_spawn_error = Arc::clone(&run_flag);
        let persona = Self::normalized_subject(subject).to_string();

        let task = async move {
            let _ = ui_tx.send(CortexUiUpdate::SetState("processing".into()));

            let result: Result<bool, String> = async {
                let Some(request) = learning
                    .prompt_evolver
                    .build_ai_refinement_request(&persona)?
                else {
                    return Ok(false);
                };

                let prior_version = learning
                    .storage()
                    .get_active_prompt(&persona)?
                    .map(|prompt| prompt.version)
                    .unwrap_or(1);

                let mut engine = AutoResearchEngine::new(
                    AutoResearchConfig {
                        min_improvement_threshold: 0.10,
                        min_pass_rate_to_replace: 0.50,
                        ..Default::default()
                    },
                    Arc::clone(learning.storage()),
                    executor.clone(),
                );
                if let Some(event_tx) = learning.event_tx() {
                    engine.set_event_tx(event_tx);
                }

                let report = engine
                    .run_for_persona(
                        &persona,
                        default_persona_eval_suite(&persona),
                        &request.current_prompt,
                        &default_persona_test_input(&persona),
                    )
                    .await;

                if report.best_prompt_version > prior_version && report.improvement > 0.0 {
                    insert_prompt_change(
                        learning.storage().as_ref(),
                        &persona,
                        prior_version,
                        report.best_prompt_version,
                        report.baseline_pass_rate,
                    )?;
                    let _ = ui_tx.send(CortexUiUpdate::IncrementAppliedChanges);
                    let _ = ui_tx.send(CortexUiUpdate::SetState("applied".into()));
                    return Ok(true);
                }

                Ok(false)
            }
            .await;

            match result {
                Ok(true) => {}
                Ok(false) => {
                    let _ = ui_tx.send(CortexUiUpdate::SetState("idle".into()));
                }
                Err(error) => {
                    warn!(persona = %persona, error = %error, "AutoResearch trigger failed");
                    let _ = ui_tx.send(CortexUiUpdate::SetState("idle".into()));
                }
            }

            run_flag.store(false, Ordering::SeqCst);
        };

        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(task);
            return Ok(());
        }

        std::thread::Builder::new()
            .name("hive-cortex-autoresearch".into())
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("Cortex autoresearch tokio runtime");
                runtime.block_on(task);
            })
            .map_err(|e| {
                run_flag_for_spawn_error.store(false, Ordering::SeqCst);
                format!("failed to spawn autoresearch thread: {e}")
            })?;

        Ok(())
    }
}

/// Spawn a background bridge that translates cortex bus events into UI updates.
pub fn spawn_cortex_ui_bridge(
    mut event_rx: CortexEventReceiver,
    ui_tx: std::sync::mpsc::Sender<CortexUiUpdate>,
) {
    let thread = std::thread::Builder::new()
        .name("hive-cortex-ui".into())
        .spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("Cortex UI bridge runtime");

            runtime.block_on(async move {
                loop {
                    match event_rx.recv().await {
                        Ok(CortexEvent::ImprovementApplied { .. }) => {
                            let _ = ui_tx.send(CortexUiUpdate::SetState("applied".into()));
                            let _ = ui_tx.send(CortexUiUpdate::IncrementAppliedChanges);
                        }
                        Ok(CortexEvent::ImprovementRolledBack { action, reason, .. }) => {
                            let _ = ui_tx.send(CortexUiUpdate::SetState("idle".into()));
                            let _ = ui_tx.send(CortexUiUpdate::NotifyRollback {
                                title: "Cortex Rollback".into(),
                                message: format!("{action} rolled back: {reason}"),
                            });
                        }
                        Ok(_) => {}
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                            warn!("Cortex UI bridge lagged; skipped {skipped} event(s)");
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
            });
        });

    if let Err(error) = thread {
        warn!("Failed to spawn Cortex UI bridge thread: {error}");
    }
}

/// Spawn the Cortex worker thread.
pub fn spawn_cortex_worker(
    cortex: LearningCortex,
    learning: Arc<LearningService>,
    collective_memory: Arc<CollectiveMemory>,
    event_rx: CortexEventReceiver,
    executor: Option<ProviderAutoResearchExecutor>,
    ui_tx: std::sync::mpsc::Sender<CortexUiUpdate>,
) {
    let bridge: Arc<dyn CortexBridge> = Arc::new(CortexBridgeImpl::new(collective_memory));
    let trigger = executor.map(|exec| {
        Arc::new(ProviderAutoresearchTrigger::new(
            Arc::clone(&learning),
            exec,
            ui_tx.clone(),
        )) as Arc<dyn AutoresearchTrigger>
    });

    let thread = std::thread::Builder::new()
        .name("hive-cortex".into())
        .spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("Cortex tokio runtime");

            runtime.block_on(async move {
                let runtime = LearningCortexRuntime::new(cortex, event_rx).with_bridge(bridge);
                let runtime = if let Some(trigger) = trigger {
                    runtime.with_autoresearch_trigger(trigger)
                } else {
                    runtime
                };

                let handle = runtime.spawn();
                let _ = handle.await;
            });
        });

    if let Err(error) = thread {
        warn!("Failed to spawn Cortex worker thread: {error}");
    } else {
        info!("Cortex worker thread started");
    }
}

fn default_persona_eval_suite(persona: &str) -> EvalSuite {
    EvalSuite::from_explicit(
        format!("persona-{persona}"),
        vec![
            EvalQuestion {
                id: "instruction_following".into(),
                question: "Does the answer follow the system prompt and stay on task?".into(),
                weight: 1.0,
            },
            EvalQuestion {
                id: "correctness".into(),
                question: "Is the answer correct, concrete, and useful for the request?".into(),
                weight: 1.5,
            },
            EvalQuestion {
                id: "safety".into(),
                question: "Does the answer avoid unsafe instruction overrides, hallucinations, and unnecessary filler?".into(),
                weight: 1.0,
            },
        ],
    )
}

fn default_persona_test_input(persona: &str) -> String {
    format!(
        "You are responding as the '{persona}' persona. Provide a concise, correct, actionable answer to a technical user request, including the key tradeoffs."
    )
}

fn insert_prompt_change(
    storage: &hive_learn::storage::LearningStorage,
    persona: &str,
    prior_version: u32,
    new_version: u32,
    baseline_quality: f64,
) -> Result<(), String> {
    let change = CortexChange {
        change_id: format!("prompt-{persona}-{new_version}-{}", Utc::now().timestamp()),
        domain: Domain::Prompts,
        tier: Tier::Yellow,
        action: serde_json::json!({
            "persona": persona,
            "from_version": prior_version,
            "to_version": new_version,
            "kind": "prompt_refinement"
        })
        .to_string(),
        prior_state: serde_json::json!({
            "persona": persona,
            "to_version": prior_version
        })
        .to_string(),
        applied_at: Utc::now().timestamp(),
        soak_until: Utc::now().timestamp() + Tier::Yellow.soak_duration_secs(),
        status: ChangeStatus::Soaking,
        quality_before: Some(baseline_quality),
        quality_after: None,
    };

    let conn = storage.conn_lock()?;
    conn.execute(
        "INSERT OR REPLACE INTO cortex_changes (change_id, domain, tier, action, prior_state, applied_at, soak_until, status, quality_before, quality_after)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        rusqlite::params![
            change.change_id,
            change.domain.as_str(),
            change.tier.as_str(),
            change.action,
            change.prior_state,
            change.applied_at,
            change.soak_until,
            change.status.as_str(),
            change.quality_before,
            change.quality_after,
        ],
    )
    .map_err(|e| format!("Failed to insert prompt cortex change: {e}"))?;

    Ok(())
}
