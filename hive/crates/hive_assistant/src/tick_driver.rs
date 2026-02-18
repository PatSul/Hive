//! Background tick driver for the Scheduler and ReminderService.
//!
//! Spawns a lightweight background thread with its own single-threaded tokio
//! runtime.  Every 60 seconds it:
//!
//! 1. Calls `Scheduler::tick(Utc::now())` and logs any due job IDs.
//! 2. Calls `ReminderService::tick()` and logs any triggered reminders.
//!
//! No AI model calls are made — this is purely timer + method invocations.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::Utc;
use tracing::{debug, error, info, warn};

use hive_core::scheduler::Scheduler;

use crate::reminders::ReminderService;
use crate::storage::AssistantStorage;

/// Configuration for the tick driver.
pub struct TickDriverConfig {
    /// How often to tick (default: 60 seconds).
    pub interval: Duration,
    /// Path to the assistant database (for the ReminderService connection).
    pub assistant_db_path: String,
}

impl Default for TickDriverConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(60),
            assistant_db_path: String::new(),
        }
    }
}

/// Start the background tick driver on a dedicated OS thread.
///
/// Returns a `JoinHandle` for the background thread. The thread runs
/// indefinitely until the process exits.
///
/// # Arguments
///
/// * `scheduler` - Shared scheduler instance (also stored as a GPUI global).
/// * `config` - Tick driver configuration including the DB path and interval.
pub fn start_tick_driver(
    scheduler: Arc<Mutex<Scheduler>>,
    config: TickDriverConfig,
) -> std::thread::JoinHandle<()> {
    std::thread::Builder::new()
        .name("hive-tick-driver".into())
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("tick-driver tokio runtime");

            rt.block_on(async move {
                // Open a dedicated storage connection for the tick driver.
                // This avoids contention with the main-thread AssistantService.
                let storage = match AssistantStorage::open(&config.assistant_db_path) {
                    Ok(s) => Arc::new(s),
                    Err(e) => {
                        error!(
                            "Tick driver: failed to open assistant storage at '{}': {e}",
                            config.assistant_db_path
                        );
                        return;
                    }
                };
                let reminder_service = ReminderService::new(storage);

                info!(
                    "Tick driver started (interval={}s)",
                    config.interval.as_secs()
                );

                let mut ticker = tokio::time::interval(config.interval);
                // Consume the first tick immediately (fires at t=0).
                ticker.tick().await;

                loop {
                    ticker.tick().await;

                    // -- Scheduler tick --
                    let now = Utc::now();
                    let due_jobs = {
                        match scheduler.lock() {
                            Ok(mut sched) => sched.tick(now),
                            Err(e) => {
                                warn!("Tick driver: scheduler lock poisoned: {e}");
                                Vec::new()
                            }
                        }
                    };

                    // -- Reminder tick --
                    let triggered_reminders = match reminder_service.tick() {
                        Ok(reminders) => reminders,
                        Err(e) => {
                            warn!("Tick driver: reminder tick error: {e}");
                            Vec::new()
                        }
                    };

                    debug!(
                        "Tick driver: {} job(s) due, {} reminder(s) triggered",
                        due_jobs.len(),
                        triggered_reminders.len()
                    );

                    // Log individual items at info level when something fires.
                    for job_id in &due_jobs {
                        info!("Tick driver: scheduled job due: {job_id}");
                    }
                    for reminder in &triggered_reminders {
                        info!(
                            "Tick driver: reminder triggered: id={}, title={}",
                            reminder.reminder_id, reminder.title
                        );
                    }
                }
            });
        })
        .expect("failed to spawn tick-driver thread")
}
