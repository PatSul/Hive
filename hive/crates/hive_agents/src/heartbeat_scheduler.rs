use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum HeartbeatMode {
    FixedInterval,
    BackoffOnIdle,
    OneShot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatTask {
    pub id: String,
    pub agent_id: String,
    pub spec: String,
    pub interval_secs: u64,
    pub mode: HeartbeatMode,
    pub max_iterations: Option<u32>,
    pub paused: bool,
    pub iteration_count: u32,
    pub last_fired: Option<DateTime<Utc>>,
    pub total_cost: f64,
}

pub struct HeartbeatScheduler {
    tasks: Mutex<HashMap<String, HeartbeatTask>>,
}

impl HeartbeatScheduler {
    pub fn new() -> Self {
        Self {
            tasks: Mutex::new(HashMap::new()),
        }
    }

    pub fn add(&self, task: HeartbeatTask) {
        self.tasks.lock().unwrap().insert(task.id.clone(), task);
    }

    pub fn list(&self) -> Vec<HeartbeatTask> {
        self.tasks.lock().unwrap().values().cloned().collect()
    }

    pub fn pause(&self, id: &str) {
        if let Some(task) = self.tasks.lock().unwrap().get_mut(id) {
            task.paused = true;
        }
    }

    pub fn resume(&self, id: &str) {
        if let Some(task) = self.tasks.lock().unwrap().get_mut(id) {
            task.paused = false;
        }
    }

    pub fn cancel(&self, id: &str) {
        self.tasks.lock().unwrap().remove(id);
    }

    pub fn record_fired(&self, id: &str, cost: f64) {
        if let Some(task) = self.tasks.lock().unwrap().get_mut(id) {
            task.last_fired = Some(Utc::now());
            task.iteration_count += 1;
            task.total_cost += cost;
        }
    }

    pub fn get(&self, id: &str) -> Option<HeartbeatTask> {
        self.tasks.lock().unwrap().get(id).cloned()
    }

    pub fn is_complete(&self, id: &str) -> bool {
        self.tasks
            .lock()
            .unwrap()
            .get(id)
            .map(|t| {
                t.max_iterations
                    .map(|max| t.iteration_count >= max)
                    .unwrap_or(false)
            })
            .unwrap_or(true)
    }
}

impl Default for HeartbeatScheduler {
    fn default() -> Self {
        Self::new()
    }
}
