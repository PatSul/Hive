use chrono::{DateTime, Utc};
use hive_agents::automation::{AutomationService, Workflow, WorkflowRunResult};
use hive_ui_panels::panels::agents::RunDisplay;
use hive_ui_panels::panels::monitor::{AgentSystemStatus, RunHistoryEntry};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RunStatus {
    Running,
    Complete,
    Failed,
}

impl RunStatus {
    fn label(self) -> &'static str {
        match self {
            Self::Running => "Running",
            Self::Complete => "Complete",
            Self::Failed => "Failed",
        }
    }

    fn progress(self) -> f32 {
        match self {
            Self::Running => 0.5,
            Self::Complete => 1.0,
            Self::Failed => 0.0,
        }
    }

    fn is_active(self) -> bool {
        matches!(self, Self::Running)
    }
}

#[derive(Debug, Clone)]
struct RunRecord {
    id: String,
    workflow_id: String,
    title: String,
    status: RunStatus,
    started_at: DateTime<Utc>,
    completed_at: Option<DateTime<Utc>>,
    steps_completed: usize,
    steps_total: usize,
}

impl RunRecord {
    fn history_key(&self) -> Option<String> {
        Some(format!(
            "{}:{}:{}",
            self.workflow_id,
            self.started_at.timestamp_millis(),
            self.completed_at?.timestamp_millis()
        ))
    }

    fn from_workflow(workflow: &Workflow, source: &str) -> Self {
        let now = Utc::now();
        let source = source.trim();
        let title = if source.is_empty() {
            workflow.name.clone()
        } else {
            format!("{} ({source})", workflow.name)
        };

        Self {
            id: format!("workflow:{}:{}", workflow.id, now.timestamp_millis()),
            workflow_id: workflow.id.clone(),
            title,
            status: RunStatus::Running,
            started_at: now,
            completed_at: None,
            steps_completed: 0,
            steps_total: workflow.steps.len(),
        }
    }

    fn from_history(run: &WorkflowRunResult, workflow: Option<&Workflow>) -> Self {
        let steps_total = workflow
            .map(|workflow| workflow.steps.len())
            .unwrap_or(run.steps_completed);
        Self {
            id: format!(
                "history:{}:{}",
                run.workflow_id,
                run.started_at.timestamp_millis()
            ),
            workflow_id: run.workflow_id.clone(),
            title: workflow
                .map(|workflow| workflow.name.clone())
                .unwrap_or_else(|| run.workflow_id.clone()),
            status: if run.success {
                RunStatus::Complete
            } else {
                RunStatus::Failed
            },
            started_at: run.started_at,
            completed_at: Some(run.completed_at),
            steps_completed: run.steps_completed,
            steps_total,
        }
    }

    fn complete(&mut self, run: &WorkflowRunResult) {
        self.workflow_id = run.workflow_id.clone();
        self.status = if run.success {
            RunStatus::Complete
        } else {
            RunStatus::Failed
        };
        self.started_at = run.started_at;
        self.completed_at = Some(run.completed_at);
        self.steps_completed = run.steps_completed;
        self.steps_total = self.steps_total.max(run.steps_completed);
    }

    fn fail(&mut self) {
        self.status = RunStatus::Failed;
        self.completed_at = Some(Utc::now());
    }

    fn to_display(&self) -> RunDisplay {
        RunDisplay {
            id: self.id.clone(),
            spec_title: self.title.clone(),
            status: self.status.label().into(),
            progress: self.status.progress(),
            tasks_done: self.steps_completed,
            tasks_total: self.steps_total,
            cost: 0.0,
            elapsed: elapsed_label(self.started_at, self.completed_at),
            tasks: vec![],
            disclosure: Default::default(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(super) struct RunStore {
    records: Vec<RunRecord>,
}

impl RunStore {
    pub(super) fn start_workflow(&mut self, workflow: &Workflow, source: &str) -> String {
        let record = RunRecord::from_workflow(workflow, source);
        let id = record.id.clone();
        self.records.push(record);
        id
    }

    pub(super) fn complete_workflow(&mut self, run_id: &str, run: &WorkflowRunResult) {
        if let Some(record) = self.records.iter_mut().find(|record| record.id == run_id) {
            record.complete(run);
        } else {
            self.records.push(RunRecord::from_history(run, None));
        }
    }

    pub(super) fn fail_workflow(&mut self, run_id: &str) {
        if let Some(record) = self.records.iter_mut().find(|record| record.id == run_id) {
            record.fail();
        }
    }

    pub(super) fn refresh_history_from_automation(&mut self, automation: &AutomationService) {
        let mut existing_keys = self
            .records
            .iter()
            .filter_map(RunRecord::history_key)
            .collect::<std::collections::HashSet<_>>();

        let mut new_records = Vec::new();
        for run in automation.list_run_history().iter().rev().take(50) {
            let workflow = automation.get_workflow(&run.workflow_id);
            let record = RunRecord::from_history(run, workflow);
            if let Some(key) = record.history_key()
                && existing_keys.insert(key)
            {
                new_records.push(record);
            }
        }

        self.records.extend(new_records);
        self.records.sort_by(|a, b| {
            b.started_at
                .cmp(&a.started_at)
                .then_with(|| b.id.cmp(&a.id))
        });
        self.records.truncate(80);
    }

    pub(super) fn active_displays(&self) -> Vec<RunDisplay> {
        self.records
            .iter()
            .filter(|record| record.status.is_active())
            .map(RunRecord::to_display)
            .collect()
    }

    pub(super) fn history_displays(&self, limit: usize) -> Vec<RunDisplay> {
        self.records
            .iter()
            .filter(|record| !record.status.is_active())
            .take(limit)
            .map(RunRecord::to_display)
            .collect()
    }

    pub(super) fn active_count(&self) -> usize {
        self.records
            .iter()
            .filter(|record| record.status.is_active())
            .count()
    }

    pub(super) fn total_count(&self) -> usize {
        self.records.len()
    }

    pub(super) fn current_run_id(&self) -> Option<String> {
        self.records
            .iter()
            .find(|record| record.status.is_active())
            .map(|record| record.id.clone())
    }

    pub(super) fn monitor_history_entries(&self, limit: usize) -> Vec<RunHistoryEntry> {
        self.records
            .iter()
            .filter(|record| !record.status.is_active())
            .take(limit)
            .map(|record| {
                RunHistoryEntry::new(
                    &record.id,
                    &record.title,
                    0,
                    match record.status {
                        RunStatus::Failed => AgentSystemStatus::Error,
                        RunStatus::Running => AgentSystemStatus::Running,
                        RunStatus::Complete => AgentSystemStatus::Idle,
                    },
                    0.0,
                    &record.started_at.format("%H:%M").to_string(),
                    record
                        .completed_at
                        .map(|completed_at| {
                            (completed_at - record.started_at).num_seconds().max(0) as u64
                        })
                        .unwrap_or(0),
                )
            })
            .collect()
    }
}

fn elapsed_label(started_at: DateTime<Utc>, completed_at: Option<DateTime<Utc>>) -> String {
    let end = completed_at.unwrap_or_else(Utc::now);
    format!("{}s", (end - started_at).num_seconds().max(0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use hive_agents::automation::{ActionType, TriggerType, WorkflowStatus, WorkflowStep};

    fn workflow() -> Workflow {
        let now = Utc::now();
        Workflow {
            id: "workflow:test".into(),
            name: "Test Workflow".into(),
            description: "Runs tests".into(),
            trigger: TriggerType::ManualTrigger,
            steps: vec![WorkflowStep {
                id: "step:test".into(),
                name: "Cargo test".into(),
                action: ActionType::RunCommand {
                    command: "cargo test".into(),
                },
                conditions: vec![],
                timeout_secs: None,
                retry_count: 0,
            }],
            status: WorkflowStatus::Active,
            created_at: now,
            updated_at: now,
            last_run: None,
            run_count: 0,
        }
    }

    #[test]
    fn workflow_run_moves_from_active_to_history() {
        let mut store = RunStore::default();
        let workflow = workflow();
        let run_id = store.start_workflow(&workflow, "Agents");

        assert_eq!(store.active_displays().len(), 1);
        assert_eq!(store.history_displays(8).len(), 0);

        let now = Utc::now();
        store.complete_workflow(
            &run_id,
            &WorkflowRunResult {
                workflow_id: workflow.id,
                started_at: now,
                completed_at: now,
                success: true,
                steps_completed: 1,
                error: None,
            },
        );

        assert_eq!(store.active_displays().len(), 0);
        let history = store.history_displays(8);
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].status, "Complete");
    }
}
