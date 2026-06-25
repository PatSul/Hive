//! Loop engineering contracts and runner.
//!
//! This module gives Hive a first-class shape for long-running agent loops:
//! trigger metadata, definition-of-done rules, tool/skill intent, guardrails,
//! run transcripts, and work-memory synthesis.

use anyhow::Result;
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use uuid::Uuid;

use crate::activity::{ApprovalGate, OperationType};
use crate::automation::{ActionType, AutomationService, TriggerType, Workflow};
use crate::collective_memory::{CollectiveMemory, MemoryCategory, MemoryEntry};
use crate::hiveloop::{HiveLoop, LoopConfig, LoopStatus};

/// Definition-of-done rules for an autonomous loop.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DoneCriteria {
    /// Output phrases that indicate the loop is ready to verify or finish.
    pub completion_phrases: Vec<String>,
    /// Whether a verifier must pass before the loop can complete.
    pub require_verifier_pass: bool,
    /// Whether the final state requires explicit human approval.
    pub require_human_approval: bool,
}

impl Default for DoneCriteria {
    fn default() -> Self {
        Self {
            completion_phrases: LoopConfig::default().completion_phrases,
            require_verifier_pass: false,
            require_human_approval: false,
        }
    }
}

/// A verifier the loop should use when a concrete executor is wired in.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum VerifierSpec {
    Command { command: String },
    Skill { skill_trigger: String },
}

/// Persistence policy for loop transcripts and synthesized work memory.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryPolicy {
    pub persist_work_memory: bool,
    pub persist_markdown_archive: bool,
}

impl Default for MemoryPolicy {
    fn default() -> Self {
        Self {
            persist_work_memory: true,
            persist_markdown_archive: true,
        }
    }
}

/// Human-approval rules that should be checked before a loop runs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GuardrailPolicy {
    pub require_initial_approval: bool,
    pub approval_operations: Vec<OperationType>,
}

impl Default for GuardrailPolicy {
    fn default() -> Self {
        Self {
            require_initial_approval: false,
            approval_operations: Vec::new(),
        }
    }
}

/// Full contract for a loop-engineered workflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopSpec {
    pub id: String,
    pub name: String,
    pub objective: String,
    pub trigger: TriggerType,
    pub done: DoneCriteria,
    pub skills: Vec<String>,
    pub tools: Vec<String>,
    pub verifier: Option<VerifierSpec>,
    pub memory: MemoryPolicy,
    pub guardrails: GuardrailPolicy,
    pub loop_config: LoopConfig,
}

impl LoopSpec {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        objective: impl Into<String>,
        trigger: TriggerType,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            objective: objective.into(),
            trigger,
            done: DoneCriteria::default(),
            skills: Vec::new(),
            tools: Vec::new(),
            verifier: None,
            memory: MemoryPolicy::default(),
            guardrails: GuardrailPolicy::default(),
            loop_config: LoopConfig::default(),
        }
    }

    pub fn with_done(mut self, done: DoneCriteria) -> Self {
        self.loop_config.completion_phrases = done.completion_phrases.clone();
        self.done = done;
        self
    }

    pub fn with_skill(mut self, skill_trigger: impl Into<String>) -> Self {
        self.skills.push(skill_trigger.into());
        self
    }

    pub fn with_tool(mut self, tool_name: impl Into<String>) -> Self {
        self.tools.push(tool_name.into());
        self
    }

    pub fn with_verifier_command(mut self, command: impl Into<String>) -> Self {
        self.verifier = Some(VerifierSpec::Command {
            command: command.into(),
        });
        self
    }

    pub fn with_verifier_skill(mut self, skill_trigger: impl Into<String>) -> Self {
        self.verifier = Some(VerifierSpec::Skill {
            skill_trigger: skill_trigger.into(),
        });
        self
    }

    pub fn with_memory_policy(
        mut self,
        persist_work_memory: bool,
        persist_markdown_archive: bool,
    ) -> Self {
        self.memory = MemoryPolicy {
            persist_work_memory,
            persist_markdown_archive,
        };
        self
    }

    /// Register this loop as an active automation shell.
    ///
    /// The automation stores the trigger and emits a loop-run notification.
    /// The actual execution is handled by [`LoopRunner`] so callers can wire
    /// model, skill, tool, and verifier execution without faking workflow
    /// `ExecuteSkill` support.
    pub fn register_workflow(&self, automations: &mut AutomationService) -> Result<Workflow> {
        let workflow =
            automations.create_workflow(&self.name, &self.objective, self.trigger.clone());

        let body = serde_json::to_string(self)?;
        automations.add_step(
            &workflow.id,
            &format!("Run loop {}", self.id),
            ActionType::SendNotification {
                title: format!("Loop ready: {}", self.name),
                body,
            },
        )?;
        automations.activate_workflow(&workflow.id)?;

        Ok(automations
            .get_workflow(&workflow.id)
            .cloned()
            .unwrap_or(workflow))
    }
}

/// Source class for work Hive can select when improving itself.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum SelfWorkSource {
    PlanningIssue,
    TodoComment,
    RepoHealth,
}

impl SelfWorkSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PlanningIssue => "planning_issue",
            Self::TodoComment => "todo_comment",
            Self::RepoHealth => "repo_health",
        }
    }
}

/// One bounded item Hive can autonomously work on.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SelfWorkCandidate {
    pub source: SelfWorkSource,
    pub title: String,
    pub objective: String,
    pub evidence: String,
    pub priority: u8,
    pub path: Option<PathBuf>,
    pub line: Option<usize>,
}

/// Configuration for selecting Hive's next self-improvement task.
#[derive(Debug, Clone)]
pub struct SelfWorkConfig {
    pub repo_root: PathBuf,
    pub objective_hint: Option<String>,
    pub trigger: TriggerType,
    pub max_candidates: usize,
    pub verifier_command: String,
    pub require_human_approval: bool,
}

impl SelfWorkConfig {
    pub fn for_hive_repo(repo_root: impl AsRef<Path>) -> Self {
        Self {
            repo_root: repo_root.as_ref().to_path_buf(),
            objective_hint: None,
            trigger: TriggerType::Schedule {
                cron: "*/30 * * * *".into(),
            },
            max_candidates: 12,
            verifier_command: "cargo check -p hive_agents --lib".into(),
            require_human_approval: false,
        }
    }

    pub fn with_objective_hint(mut self, hint: impl Into<String>) -> Self {
        let hint = hint.into();
        if !hint.trim().is_empty() {
            self.objective_hint = Some(hint.trim().to_string());
        }
        self
    }

    pub fn with_verifier_command(mut self, command: impl Into<String>) -> Self {
        let command = command.into();
        if !command.trim().is_empty() {
            self.verifier_command = command.trim().to_string();
        }
        self
    }

    pub fn with_max_candidates(mut self, max_candidates: usize) -> Self {
        self.max_candidates = max_candidates.max(1);
        self
    }

    pub fn require_human_approval(mut self, require: bool) -> Self {
        self.require_human_approval = require;
        self
    }
}

/// A deterministic self-work plan Hive can run, expose over MCP, and persist.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelfWorkPlan {
    pub repo_root: PathBuf,
    pub generated_at: DateTime<Utc>,
    pub objective_hint: Option<String>,
    pub context_summary: String,
    pub candidates: Vec<SelfWorkCandidate>,
    pub selected: SelfWorkCandidate,
    pub loop_spec: LoopSpec,
}

/// Selects the next bounded task for Hive to perform on its own repository.
#[derive(Debug, Clone)]
pub struct SelfWorkPlanner {
    config: SelfWorkConfig,
}

impl SelfWorkPlanner {
    pub fn new(config: SelfWorkConfig) -> Self {
        Self { config }
    }

    pub fn plan(&self) -> std::result::Result<SelfWorkPlan, String> {
        let mut candidates = Vec::new();
        self.collect_planning_issues(&mut candidates);
        self.collect_todo_comments(&mut candidates);

        candidates.sort_by(|a, b| {
            b.priority
                .cmp(&a.priority)
                .then_with(|| a.source.cmp(&b.source))
                .then_with(|| a.evidence.cmp(&b.evidence))
                .then_with(|| a.title.cmp(&b.title))
        });
        candidates.truncate(self.config.max_candidates.max(1));

        if candidates.is_empty() {
            candidates.push(self.repo_health_candidate());
        }

        let selected = candidates[0].clone();
        let loop_spec = self.loop_spec_for(&selected);
        let context_summary = format!(
            "{} candidate(s) scanned; selected {} from {}",
            candidates.len(),
            selected.title,
            selected.source.as_str()
        );

        Ok(SelfWorkPlan {
            repo_root: self.config.repo_root.clone(),
            generated_at: Utc::now(),
            objective_hint: self.config.objective_hint.clone(),
            context_summary,
            candidates,
            selected,
            loop_spec,
        })
    }

    fn collect_planning_issues(&self, candidates: &mut Vec<SelfWorkCandidate>) {
        let issue_files = [
            self.config.repo_root.join(".planning/ISSUES.md"),
            self.config.repo_root.join("ISSUES.md"),
            self.config.repo_root.join("TODO.md"),
            self.config.repo_root.join(".hive/ISSUES.md"),
        ];

        for path in issue_files {
            let Ok(raw) = std::fs::read_to_string(&path) else {
                continue;
            };

            for (idx, line) in raw.lines().enumerate() {
                let Some(title) = parse_unchecked_markdown_task(line) else {
                    continue;
                };
                let evidence = self.evidence(&path, idx + 1);
                candidates.push(SelfWorkCandidate {
                    source: SelfWorkSource::PlanningIssue,
                    objective: self.objective_for(&title),
                    title,
                    evidence,
                    priority: 100,
                    path: Some(path.clone()),
                    line: Some(idx + 1),
                });
            }
        }
    }

    fn collect_todo_comments(&self, candidates: &mut Vec<SelfWorkCandidate>) {
        let mut files = Vec::new();
        collect_text_files(&self.config.repo_root, &mut files);
        files.sort();

        for path in files {
            if candidates.len() >= self.config.max_candidates.saturating_mul(4).max(16) {
                break;
            }

            let Ok(raw) = std::fs::read_to_string(&path) else {
                continue;
            };

            for (idx, line) in raw.lines().enumerate() {
                let Some(title) = parse_todo_comment(line) else {
                    continue;
                };
                let evidence = self.evidence(&path, idx + 1);
                candidates.push(SelfWorkCandidate {
                    source: SelfWorkSource::TodoComment,
                    objective: self.objective_for(&title),
                    title,
                    evidence,
                    priority: 50,
                    path: Some(path.clone()),
                    line: Some(idx + 1),
                });
            }
        }
    }

    fn repo_health_candidate(&self) -> SelfWorkCandidate {
        let title = "Run Hive self-health pass".to_string();
        SelfWorkCandidate {
            source: SelfWorkSource::RepoHealth,
            objective: self.objective_for(
                "Run Hive self-health pass: inspect git state, run focused verification, and record the next actionable improvement",
            ),
            evidence: "fallback: no planning issues or TODO/FIXME comments found".into(),
            priority: 10,
            path: None,
            line: None,
            title,
        }
    }

    fn loop_spec_for(&self, candidate: &SelfWorkCandidate) -> LoopSpec {
        let mut loop_config = LoopConfig {
            max_iterations: 5,
            cost_limit_usd: 1.0,
            time_limit_secs: 1800,
            completion_phrases: vec!["ready for review".into(), "self-work complete".into()],
        };

        let done = DoneCriteria {
            completion_phrases: loop_config.completion_phrases.clone(),
            require_verifier_pass: true,
            require_human_approval: self.config.require_human_approval,
        };
        loop_config.completion_phrases = done.completion_phrases.clone();

        let mut spec = LoopSpec::new(
            "hive-self-work",
            "Hive self-improvement loop",
            candidate.objective.clone(),
            self.config.trigger.clone(),
        )
        .with_done(done)
        .with_skill("superpowers:test-driven-development")
        .with_skill("superpowers:verification-before-completion")
        .with_skill("$gsd-progress")
        .with_tool("read_file")
        .with_tool("search_context")
        .with_tool("execute_command")
        .with_tool("git_status")
        .with_tool("write_file")
        .with_verifier_command(self.config.verifier_command.clone())
        .with_memory_policy(true, true);
        spec.loop_config = loop_config;
        spec
    }

    fn objective_for(&self, task: &str) -> String {
        let mut objective = format!(
            "Autonomously improve Hive by completing this bounded self-work task: {}.",
            task.trim()
        );
        if let Some(hint) = &self.config.objective_hint {
            objective.push_str(" User objective: ");
            objective.push_str(hint);
            objective.push('.');
        }
        objective.push_str(
            " Work in small test-first increments, use the existing repo context, verify locally, and stop with changes ready for review.",
        );
        objective
    }

    fn evidence(&self, path: &Path, line: usize) -> String {
        let display = path
            .strip_prefix(&self.config.repo_root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        format!("{display}:{line}")
    }
}

fn parse_unchecked_markdown_task(line: &str) -> Option<String> {
    let trimmed = line.trim();
    let rest = trimmed
        .strip_prefix("- [ ]")
        .or_else(|| trimmed.strip_prefix("* [ ]"))?
        .trim();
    non_empty_title(rest)
}

fn parse_todo_comment(line: &str) -> Option<String> {
    let marker = if let Some(idx) = line.find("TODO") {
        Some(idx + "TODO".len())
    } else {
        line.find("FIXME").map(|idx| idx + "FIXME".len())
    }?;
    let rest = line[marker..]
        .trim_start_matches(|c: char| c == ':' || c == '-' || c.is_whitespace())
        .trim();
    non_empty_title(rest)
}

fn non_empty_title(text: &str) -> Option<String> {
    let title = text.trim();
    if title.is_empty() {
        None
    } else {
        Some(title.to_string())
    }
}

fn collect_text_files(root: &Path, out: &mut Vec<PathBuf>) {
    let Ok(read_dir) = std::fs::read_dir(root) else {
        return;
    };

    let mut entries: Vec<_> = read_dir.filter_map(|entry| entry.ok()).collect();
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };

        if path.is_dir() {
            if matches!(
                name,
                ".git" | ".hive-worktrees" | "target" | "node_modules" | "dist" | "build"
            ) {
                continue;
            }
            collect_text_files(&path, out);
            continue;
        }

        if is_self_work_text_file(&path) {
            out.push(path);
        }
    }
}

fn is_self_work_text_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some(
            "rs" | "md"
                | "toml"
                | "json"
                | "yaml"
                | "yml"
                | "ts"
                | "tsx"
                | "js"
                | "jsx"
                | "py"
                | "sh"
                | "ps1"
        )
    )
}

/// One iteration's executor output.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LoopIterationOutcome {
    pub output: String,
    pub cost_usd: f64,
    pub memory_notes: Vec<String>,
}

/// A durable transcript record for one loop iteration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LoopIterationRecord {
    pub iteration: usize,
    pub output: String,
    pub cost_usd: f64,
    pub memory_notes: Vec<String>,
    pub timestamp: DateTime<Utc>,
}

/// Result from a loop verifier.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VerifierOutcome {
    pub passed: bool,
    pub summary: String,
}

/// Durable result of a loop run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopRunResult {
    pub run_id: String,
    pub spec_id: String,
    pub spec_name: String,
    pub objective: String,
    pub started_at: DateTime<Utc>,
    pub completed_at: DateTime<Utc>,
    pub status: LoopStatus,
    pub iterations: Vec<LoopIterationRecord>,
    pub verification: Option<VerifierOutcome>,
    pub approval_request_ids: Vec<String>,
    pub error: Option<String>,
    pub total_cost_usd: f64,
}

impl LoopRunResult {
    fn started(spec: &LoopSpec) -> Self {
        let now = Utc::now();
        Self {
            run_id: Uuid::new_v4().to_string(),
            spec_id: spec.id.clone(),
            spec_name: spec.name.clone(),
            objective: spec.objective.clone(),
            started_at: now,
            completed_at: now,
            status: LoopStatus::Running,
            iterations: Vec::new(),
            verification: None,
            approval_request_ids: Vec::new(),
            error: None,
            total_cost_usd: 0.0,
        }
    }

    pub fn succeeded(&self) -> bool {
        self.status == LoopStatus::Completed
            && self
                .verification
                .as_ref()
                .map(|outcome| outcome.passed)
                .unwrap_or(true)
    }

    fn finish(&mut self, status: LoopStatus) {
        self.status = status;
        self.completed_at = Utc::now();
    }
}

/// Runs a [`LoopSpec`] with caller-supplied execution and verification hooks.
#[derive(Debug, Default, Clone)]
pub struct LoopRunner {
    approval_gate: Option<Arc<ApprovalGate>>,
}

impl LoopRunner {
    pub fn new() -> Self {
        Self {
            approval_gate: None,
        }
    }

    pub fn with_approval_gate(mut self, gate: Arc<ApprovalGate>) -> Self {
        self.approval_gate = Some(gate);
        self
    }

    pub fn run<E, V>(&self, spec: &LoopSpec, mut execute: E, mut verify: V) -> LoopRunResult
    where
        E: FnMut(&LoopSpec, usize) -> std::result::Result<LoopIterationOutcome, String>,
        V: FnMut(&LoopSpec, &[LoopIterationRecord]) -> std::result::Result<VerifierOutcome, String>,
    {
        let mut result = LoopRunResult::started(spec);

        if self.check_initial_approvals(spec, &mut result) {
            result.finish(LoopStatus::Paused);
            return result;
        }

        let mut config = spec.loop_config.clone();
        config.completion_phrases.clear();

        let mut hive_loop = HiveLoop::new(config);
        hive_loop.start();

        while hive_loop.should_continue() {
            let iteration = hive_loop.iteration + 1;
            let outcome = match execute(spec, iteration) {
                Ok(outcome) => outcome,
                Err(error) => {
                    result.error = Some(error);
                    result.finish(LoopStatus::Failed);
                    return result;
                }
            };

            let record = LoopIterationRecord {
                iteration,
                output: outcome.output.clone(),
                cost_usd: outcome.cost_usd,
                memory_notes: outcome.memory_notes,
                timestamp: Utc::now(),
            };

            result.total_cost_usd += record.cost_usd;
            result.iterations.push(record);

            let loop_status = hive_loop.record_iteration(&outcome.output, outcome.cost_usd);

            if self.definition_of_done_met(spec, &mut result, &mut verify) {
                if result.status == LoopStatus::Failed {
                    return result;
                }
                result.finish(LoopStatus::Completed);
                return result;
            }

            if loop_status != LoopStatus::Running {
                result.finish(loop_status);
                return result;
            }
        }

        result.finish(hive_loop.status);
        result
    }

    fn check_initial_approvals(&self, spec: &LoopSpec, result: &mut LoopRunResult) -> bool {
        if !spec.guardrails.require_initial_approval && !spec.done.require_human_approval {
            return false;
        }

        let Some(gate) = &self.approval_gate else {
            result.error =
                Some("loop requires approval, but no approval gate is configured".into());
            return true;
        };

        let operations = if spec.guardrails.approval_operations.is_empty() {
            vec![OperationType::Custom(format!("loop:{}", spec.id))]
        } else {
            spec.guardrails.approval_operations.clone()
        };

        for operation in operations {
            if let Some(request) = gate.check_sync(&spec.id, &operation) {
                result.approval_request_ids.push(request.id);
            }
        }

        !result.approval_request_ids.is_empty()
    }

    fn definition_of_done_met<V>(
        &self,
        spec: &LoopSpec,
        result: &mut LoopRunResult,
        verify: &mut V,
    ) -> bool
    where
        V: FnMut(&LoopSpec, &[LoopIterationRecord]) -> std::result::Result<VerifierOutcome, String>,
    {
        let phrase_ready = result
            .iterations
            .last()
            .map(|record| completion_phrase_matches(&record.output, &spec.done.completion_phrases))
            .unwrap_or(false);

        if !phrase_ready {
            return false;
        }

        if spec.done.require_verifier_pass {
            match verify(spec, &result.iterations) {
                Ok(outcome) => {
                    let passed = outcome.passed;
                    result.verification = Some(outcome);
                    passed
                }
                Err(error) => {
                    result.error = Some(error);
                    result.finish(LoopStatus::Failed);
                    true
                }
            }
        } else {
            true
        }
    }
}

fn completion_phrase_matches(output: &str, phrases: &[String]) -> bool {
    if phrases.is_empty() {
        return true;
    }

    let output = output.to_lowercase();
    phrases
        .iter()
        .any(|phrase| output.contains(&phrase.to_lowercase()))
}

/// A synthesized work-memory lesson from a completed loop run.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BrainLesson {
    pub category: MemoryCategory,
    pub content: String,
    pub tags: Vec<String>,
    pub relevance_score: f64,
    pub source_run_id: String,
}

/// Result of a Brain synthesis pass.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrainSynthesis {
    pub run_id: String,
    pub generated_at: DateTime<Utc>,
    pub lessons: Vec<BrainLesson>,
    pub markdown_path: Option<PathBuf>,
}

/// Converts loop transcripts into durable work memory.
#[derive(Debug, Clone)]
pub struct BrainPass {
    max_output_chars: usize,
}

impl Default for BrainPass {
    fn default() -> Self {
        Self {
            max_output_chars: 320,
        }
    }
}

impl BrainPass {
    pub fn new(max_output_chars: usize) -> Self {
        Self { max_output_chars }
    }

    pub fn synthesize(&self, run: &LoopRunResult) -> BrainSynthesis {
        let mut lessons = Vec::new();
        let last_output = run
            .iterations
            .last()
            .map(|record| truncate(&record.output, self.max_output_chars))
            .unwrap_or_else(|| "no loop output recorded".to_string());

        let verifier = run
            .verification
            .as_ref()
            .map(|outcome| outcome.summary.as_str())
            .unwrap_or("no verifier summary");

        if run.succeeded() {
            lessons.push(BrainLesson {
                category: MemoryCategory::SuccessPattern,
                content: format!(
                    "Loop '{}' ({}) completed objective '{}' in {} iteration(s); verifier: {}; last output: {}",
                    run.spec_name,
                    run.spec_id,
                    run.objective,
                    run.iterations.len(),
                    verifier,
                    last_output
                ),
                tags: vec!["loop".into(), "success".into(), run.spec_id.clone()],
                relevance_score: 0.95,
                source_run_id: run.run_id.clone(),
            });
        } else {
            let reason = run
                .error
                .as_deref()
                .unwrap_or("loop stopped before definition of done");
            lessons.push(BrainLesson {
                category: MemoryCategory::FailurePattern,
                content: format!(
                    "Loop '{}' ({}) stopped with status {:?} after {} iteration(s); reason: {}; last output: {}",
                    run.spec_name,
                    run.spec_id,
                    run.status,
                    run.iterations.len(),
                    reason,
                    last_output
                ),
                tags: vec!["loop".into(), "failure".into(), run.spec_id.clone()],
                relevance_score: 0.9,
                source_run_id: run.run_id.clone(),
            });
        }

        for record in &run.iterations {
            for note in &record.memory_notes {
                lessons.push(BrainLesson {
                    category: MemoryCategory::CodePattern,
                    content: format!("Loop '{}' learned: {}", run.spec_id, note),
                    tags: vec!["loop".into(), "memory-note".into(), run.spec_id.clone()],
                    relevance_score: 0.85,
                    source_run_id: run.run_id.clone(),
                });
            }
        }

        BrainSynthesis {
            run_id: run.run_id.clone(),
            generated_at: Utc::now(),
            lessons,
            markdown_path: None,
        }
    }

    pub fn synthesize_and_persist(
        &self,
        run: &LoopRunResult,
        memory: &CollectiveMemory,
        archive_dir: Option<&Path>,
        date: NaiveDate,
    ) -> std::result::Result<BrainSynthesis, String> {
        let mut synthesis = self.synthesize(run);

        for lesson in &synthesis.lessons {
            let mut entry = MemoryEntry::new(lesson.category, lesson.content.clone());
            entry.tags = lesson.tags.clone();
            entry.source_run_id = Some(lesson.source_run_id.clone());
            entry.relevance_score = lesson.relevance_score;
            memory.remember(&entry)?;
        }

        if let Some(dir) = archive_dir {
            let path = write_brain_markdown(dir, date, &synthesis)?;
            synthesis.markdown_path = Some(path);
        }

        Ok(synthesis)
    }
}

fn truncate(text: &str, max_chars: usize) -> String {
    let mut chars = text.chars();
    let truncated: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{truncated}...")
    } else {
        truncated
    }
}

fn write_brain_markdown(
    archive_dir: &Path,
    date: NaiveDate,
    synthesis: &BrainSynthesis,
) -> std::result::Result<PathBuf, String> {
    std::fs::create_dir_all(archive_dir)
        .map_err(|e| format!("failed to create brain archive directory: {e}"))?;

    let path = archive_dir.join(format!("{date}.md"));
    let file_exists = path.exists();
    let mut markdown = String::new();

    if !file_exists {
        markdown.push_str(&format!("# Brain Work Memory - {date}\n\n"));
    } else {
        markdown.push('\n');
    }

    markdown.push_str(&format!(
        "Generated: {}\n\nRun: {}\n\n",
        synthesis.generated_at.to_rfc3339(),
        synthesis.run_id
    ));

    let mut grouped: BTreeMap<&'static str, Vec<&BrainLesson>> = BTreeMap::new();
    for lesson in &synthesis.lessons {
        grouped
            .entry(category_heading(lesson.category))
            .or_default()
            .push(lesson);
    }

    for (heading, lessons) in grouped {
        markdown.push_str(&format!("## {heading}\n"));
        for lesson in lessons {
            markdown.push_str(&format!(
                "- {} [relevance: {:.2}]\n",
                lesson.content, lesson.relevance_score
            ));
        }
        markdown.push('\n');
    }

    if file_exists {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .map_err(|e| format!("failed to open brain archive for append: {e}"))?;
        file.write_all(markdown.as_bytes())
            .map_err(|e| format!("failed to append brain archive: {e}"))?;
    } else {
        std::fs::write(&path, markdown.as_bytes())
            .map_err(|e| format!("failed to write brain archive: {e}"))?;
    }

    Ok(path)
}

fn category_heading(category: MemoryCategory) -> &'static str {
    match category {
        MemoryCategory::SuccessPattern => "Success Patterns",
        MemoryCategory::FailurePattern => "Failure Patterns",
        MemoryCategory::ModelInsight => "Model Insights",
        MemoryCategory::ConflictResolution => "Conflict Resolutions",
        MemoryCategory::CodePattern => "Code Patterns",
        MemoryCategory::UserPreference => "User Preferences",
        MemoryCategory::General => "General",
    }
}
