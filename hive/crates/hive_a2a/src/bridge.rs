//! Bridge — type conversions between a2a-rs domain types and hive_agents types.
//!
//! Provides functions to convert back and forth between A2A protocol types
//! (Message, Artifact, TaskStatusUpdateEvent) and Hive internal types
//! (OrchestrationResult, CoordinatorResult, SwarmResult, TaskEvent).

use a2a_rs::{Artifact, Message, Part, Role, TaskState, TaskStatus, TaskStatusUpdateEvent};
use chrono::Utc;
use serde_json::{Map, Value};
use uuid::Uuid;

use hive_agents::coordinator::{CoordinatorResult, TaskEvent};
use hive_agents::hivemind::OrchestrationResult;
use hive_agents::swarm::SwarmResult;

// ---------------------------------------------------------------------------
// A2A → Hive helpers
// ---------------------------------------------------------------------------

/// Concatenate all Text parts from an A2A Message into a single string.
///
/// Non-text parts (File, Data) are silently skipped. Multiple text parts
/// are joined with a single newline.
pub fn extract_message_text(message: &Message) -> String {
    let texts: Vec<&str> = message
        .parts
        .iter()
        .filter_map(|part| match part {
            Part::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    texts.join("\n")
}

/// Extract a `"skill_id"` from message metadata, if present.
///
/// Returns `None` if metadata is absent, the key is missing, or the value
/// is not a string.
pub fn extract_skill_id(metadata: Option<&Map<String, Value>>) -> Option<String> {
    metadata
        .and_then(|m| m.get("skill_id"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Infer which Hive skill to use based on message text content.
///
/// Returns one of: `"queen"`, `"hivemind"`, `"coordinator"`, or `"single"`.
///
/// Keyword heuristics (checked in order):
/// - "teams", "swarm", "platform" → `"queen"`
/// - "architect", "design and implement", "plan and build" → `"hivemind"`
/// - "steps", "then ", "after ", "in order", "parallel" → `"coordinator"`
/// - Short messages (<100 chars) → `"single"`, longer → `"hivemind"`
pub fn infer_skill(text: &str) -> &'static str {
    let lower = text.to_lowercase();

    // Queen keywords (multi-team orchestration)
    if lower.contains("teams") || lower.contains("swarm") || lower.contains("platform") {
        return "queen";
    }

    // HiveMind keywords (multi-agent pipeline)
    if lower.contains("architect")
        || lower.contains("design and implement")
        || lower.contains("plan and build")
    {
        return "hivemind";
    }

    // Coordinator keywords (dependency-ordered tasks)
    if lower.contains("steps")
        || lower.contains("then ")
        || lower.contains("after ")
        || lower.contains("in order")
        || lower.contains("parallel")
    {
        return "coordinator";
    }

    // Fallback: short messages → single, longer → hivemind
    if text.len() < 100 {
        "single"
    } else {
        "hivemind"
    }
}

// ---------------------------------------------------------------------------
// Hive → A2A helpers
// ---------------------------------------------------------------------------

/// Convert an `OrchestrationResult` (HiveMind) into an A2A `Artifact`.
///
/// The artifact text includes the synthesized output, cost, duration, and
/// consensus score (if available).
pub fn orchestration_result_to_artifact(result: &OrchestrationResult) -> Artifact {
    let mut text = result.synthesized_output.clone();
    text.push_str(&format!(
        "\n\n---\nCost: ${:.4} | Duration: {}ms",
        result.total_cost, result.total_duration_ms
    ));
    if let Some(score) = result.consensus_score {
        text.push_str(&format!(" | Consensus: {:.0}%", score * 100.0));
    }

    Artifact {
        artifact_id: format!("hivemind-{}", result.run_id),
        name: Some("HiveMind Result".into()),
        description: Some(format!("Orchestration result for: {}", result.task)),
        parts: vec![Part::text(text)],
        metadata: None,
    }
}

/// Convert a `CoordinatorResult` into an A2A `Artifact`.
///
/// The artifact text includes a summary of each task result plus totals.
pub fn coordinator_result_to_artifact(result: &CoordinatorResult) -> Artifact {
    let mut lines = Vec::new();
    for r in &result.results {
        let status_str = if r.success { "OK" } else { "FAIL" };
        let preview = if r.output.chars().count() > 200 {
            let truncated: String = r.output.chars().take(200).collect();
            format!("{truncated}...")
        } else {
            r.output.clone()
        };
        lines.push(format!(
            "[{}] {} ({}) — ${:.4}, {}ms\n{}",
            status_str, r.task_id, r.persona, r.cost, r.duration_ms, preview
        ));
    }
    lines.push(format!(
        "\n---\nTotal: ${:.4} | Duration: {}ms | {}/{} succeeded",
        result.total_cost,
        result.total_duration_ms,
        result.results.iter().filter(|r| r.success).count(),
        result.results.len()
    ));

    Artifact {
        artifact_id: format!("coordinator-{}", Uuid::new_v4()),
        name: Some("Coordinator Result".into()),
        description: Some(format!(
            "{} tasks executed ({} succeeded)",
            result.results.len(),
            result.results.iter().filter(|r| r.success).count()
        )),
        parts: vec![Part::text(lines.join("\n\n"))],
        metadata: None,
    }
}

/// Convert a `SwarmResult` (Queen) into an A2A `Artifact`.
///
/// The artifact text includes the synthesized output, team count, cost, and
/// duration.
pub fn swarm_result_to_artifact(result: &SwarmResult) -> Artifact {
    let mut text = result.synthesized_output.clone();
    text.push_str(&format!(
        "\n\n---\nGoal: {} | Teams: {} | Cost: ${:.4} | Duration: {}ms | Learnings: {}",
        result.goal,
        result.team_results.len(),
        result.total_cost,
        result.total_duration_ms,
        result.learnings_recorded,
    ));

    Artifact {
        artifact_id: format!("swarm-{}", result.run_id),
        name: Some("Queen Swarm Result".into()),
        description: Some(format!("Swarm result for: {}", result.goal)),
        parts: vec![Part::text(text)],
        metadata: None,
    }
}

/// Convert a coordinator `TaskEvent` to an A2A `TaskStatusUpdateEvent`.
///
/// Maps each variant:
/// - `PlanCreated` → `Working` (not final)
/// - `TaskStarted` → `Working` (not final)
/// - `TaskProgress` → `Working` (not final)
/// - `TaskCompleted` → `Working` (not final, since other tasks may remain)
/// - `TaskFailed` → `Working` (not final, since other tasks may remain)
/// - `AllComplete` → `Completed` or `Failed` depending on failure count (final)
pub fn task_event_to_status_update(
    a2a_task_id: &str,
    context_id: &str,
    event: &TaskEvent,
) -> Option<TaskStatusUpdateEvent> {
    let (state, is_final, message_text) = match event {
        TaskEvent::PlanCreated { plan_id, tasks } => (
            TaskState::Working,
            false,
            format!("Plan '{}' created with {} tasks", plan_id, tasks.len()),
        ),
        TaskEvent::TaskStarted {
            task_id,
            description,
            persona,
        } => (
            TaskState::Working,
            false,
            format!("Started task '{}' ({}): {}", task_id, persona, description),
        ),
        TaskEvent::TaskProgress {
            task_id,
            progress,
            message,
        } => (
            TaskState::Working,
            false,
            format!("[{}] {:.0}%: {}", task_id, progress * 100.0, message),
        ),
        TaskEvent::TaskCompleted {
            task_id,
            duration_ms,
            cost,
            output_preview,
        } => (
            TaskState::Working,
            false,
            format!(
                "Completed '{}' (${:.4}, {}ms): {}",
                task_id, cost, duration_ms, output_preview
            ),
        ),
        TaskEvent::TaskApprovalPending {
            task_id,
            request_id,
            operation,
            rule,
        } => (
            TaskState::Working,
            false,
            format!(
                "Task '{}' awaiting approval (req {}, op: {}, rule: {})",
                task_id, request_id, operation, rule
            ),
        ),
        TaskEvent::TaskDenied { task_id, reason } => (
            TaskState::Working,
            false,
            format!(
                "Task '{}' denied: {}",
                task_id,
                reason.as_deref().unwrap_or("no reason given")
            ),
        ),
        TaskEvent::TaskFailed { task_id, error } => (
            TaskState::Working,
            false,
            format!("Task '{}' failed: {}", task_id, error),
        ),
        TaskEvent::AllComplete {
            total_cost,
            total_duration_ms,
            success_count,
            failure_count,
        } => {
            let state = if *failure_count > 0 {
                TaskState::Failed
            } else {
                TaskState::Completed
            };
            (
                state,
                true,
                format!(
                    "All complete: {}/{} succeeded, ${:.4}, {}ms",
                    success_count,
                    success_count + failure_count,
                    total_cost,
                    total_duration_ms
                ),
            )
        }
    };

    let msg_id = Uuid::new_v4().to_string();
    let status_message = Message {
        role: Role::Agent,
        parts: vec![Part::text(message_text)],
        metadata: None,
        reference_task_ids: None,
        message_id: msg_id,
        task_id: Some(a2a_task_id.to_string()),
        context_id: Some(context_id.to_string()),
        kind: "message".to_string(),
    };

    Some(TaskStatusUpdateEvent {
        task_id: a2a_task_id.to_string(),
        context_id: context_id.to_string(),
        kind: "status-update".to_string(),
        status: TaskStatus {
            state,
            message: Some(status_message),
            timestamp: Some(Utc::now()),
        },
        final_: is_final,
        metadata: None,
    })
}

/// Extract text content from an A2A `Artifact`.
///
/// Concatenates all Text parts with newlines, mirroring `extract_message_text`.
pub fn artifact_to_text(artifact: &Artifact) -> String {
    let texts: Vec<&str> = artifact
        .parts
        .iter()
        .filter_map(|part| match part {
            Part::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    texts.join("\n")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use hive_agents::coordinator::{CoordinatorResult, TaskEvent, TaskPlan, TaskResult};
    use hive_agents::hivemind::{OrchestrationResult, OrchestrationStatus};
    use hive_agents::personas::PersonaKind;
    use hive_agents::swarm::{
        OrchestrationMode, SwarmPlan, SwarmResult, SwarmStatus, TeamObjective,
    };

    // -- extract_message_text -----------------------------------------------

    #[test]
    fn test_extract_text_from_message_single_part() {
        let msg = Message::user_text("Hello world".into(), "msg-1".into());
        assert_eq!(extract_message_text(&msg), "Hello world");
    }

    #[test]
    fn test_extract_text_from_message_multi_part() {
        let msg = Message {
            role: Role::User,
            parts: vec![
                Part::text("Part one".into()),
                Part::text("Part two".into()),
                Part::data(Map::new()), // non-text part
                Part::text("Part three".into()),
            ],
            metadata: None,
            reference_task_ids: None,
            message_id: "msg-2".into(),
            task_id: None,
            context_id: None,
            kind: "message".into(),
        };
        assert_eq!(extract_message_text(&msg), "Part one\nPart two\nPart three");
    }

    #[test]
    fn test_extract_text_from_message_no_text_parts() {
        let msg = Message {
            role: Role::Agent,
            parts: vec![Part::data(Map::new())],
            metadata: None,
            reference_task_ids: None,
            message_id: "msg-3".into(),
            task_id: None,
            context_id: None,
            kind: "message".into(),
        };
        assert_eq!(extract_message_text(&msg), "");
    }

    // -- extract_skill_id ---------------------------------------------------

    #[test]
    fn test_extract_skill_from_metadata_present() {
        let mut meta = Map::new();
        meta.insert("skill_id".into(), Value::String("hivemind".into()));
        assert_eq!(extract_skill_id(Some(&meta)), Some("hivemind".to_string()));
    }

    #[test]
    fn test_extract_skill_from_metadata_absent() {
        let meta = Map::new();
        assert_eq!(extract_skill_id(Some(&meta)), None);
    }

    #[test]
    fn test_extract_skill_from_metadata_none() {
        assert_eq!(extract_skill_id(None), None);
    }

    #[test]
    fn test_extract_skill_from_metadata_wrong_type() {
        let mut meta = Map::new();
        meta.insert("skill_id".into(), Value::Number(42.into()));
        assert_eq!(extract_skill_id(Some(&meta)), None);
    }

    // -- infer_skill --------------------------------------------------------

    #[test]
    fn test_infer_skill_queen_keywords() {
        assert_eq!(infer_skill("Deploy to multiple teams"), "queen");
        assert_eq!(infer_skill("Use swarm to build this"), "queen");
        assert_eq!(infer_skill("Build the platform"), "queen");
    }

    #[test]
    fn test_infer_skill_hivemind_keywords() {
        assert_eq!(infer_skill("Architect a new auth system"), "hivemind");
        assert_eq!(infer_skill("Design and implement the API"), "hivemind");
        assert_eq!(infer_skill("Plan and build the service"), "hivemind");
    }

    #[test]
    fn test_infer_skill_coordinator_keywords() {
        assert_eq!(infer_skill("Follow these steps carefully"), "coordinator");
        assert_eq!(infer_skill("Do X then do Y"), "coordinator");
        assert_eq!(infer_skill("After refactoring, run tests"), "coordinator");
        assert_eq!(infer_skill("Execute tasks in order"), "coordinator");
        assert_eq!(infer_skill("Run A and B in parallel"), "coordinator");
    }

    #[test]
    fn test_infer_skill_short_default() {
        assert_eq!(infer_skill("Fix the bug"), "single");
        assert_eq!(infer_skill("Hello"), "single");
    }

    #[test]
    fn test_infer_skill_long_default() {
        let long_msg = "a".repeat(150);
        assert_eq!(infer_skill(&long_msg), "hivemind");
    }

    // -- orchestration_result_to_artifact -----------------------------------

    #[test]
    fn test_orchestration_result_to_artifact() {
        let result = OrchestrationResult {
            run_id: "run-42".into(),
            task: "Build auth module".into(),
            status: OrchestrationStatus::Complete,
            agent_outputs: vec![],
            synthesized_output: "Auth module implemented successfully.".into(),
            total_cost: 0.0523,
            total_duration_ms: 12345,
            consensus_score: Some(0.92),
        };

        let artifact = orchestration_result_to_artifact(&result);

        assert_eq!(artifact.artifact_id, "hivemind-run-42");
        assert_eq!(artifact.name, Some("HiveMind Result".into()));
        assert!(artifact
            .description
            .as_ref()
            .unwrap()
            .contains("Build auth module"));

        let text = artifact_to_text(&artifact);
        assert!(text.contains("Auth module implemented successfully."));
        assert!(text.contains("$0.0523"));
        assert!(text.contains("12345ms"));
        assert!(text.contains("Consensus: 92%"));
    }

    #[test]
    fn test_orchestration_result_to_artifact_no_consensus() {
        let result = OrchestrationResult {
            run_id: "run-99".into(),
            task: "Simple task".into(),
            status: OrchestrationStatus::Complete,
            agent_outputs: vec![],
            synthesized_output: "Done.".into(),
            total_cost: 0.01,
            total_duration_ms: 500,
            consensus_score: None,
        };

        let artifact = orchestration_result_to_artifact(&result);
        let text = artifact_to_text(&artifact);
        assert!(text.contains("Done."));
        assert!(!text.contains("Consensus"));
    }

    // -- coordinator_result_to_artifact -------------------------------------

    #[test]
    fn test_coordinator_result_to_artifact() {
        let result = CoordinatorResult {
            plan: TaskPlan { tasks: vec![] },
            results: vec![
                TaskResult {
                    task_id: "t1".into(),
                    persona: PersonaKind::Investigate,
                    output: "Investigation complete.".into(),
                    cost: 0.02,
                    duration_ms: 1000,
                    success: true,
                    error: None,
                },
                TaskResult {
                    task_id: "t2".into(),
                    persona: PersonaKind::Implement,
                    output: "Code written.".into(),
                    cost: 0.03,
                    duration_ms: 2000,
                    success: true,
                    error: None,
                },
            ],
            total_cost: 0.05,
            total_duration_ms: 3000,
            spec_updates: vec![],
        };

        let artifact = coordinator_result_to_artifact(&result);
        assert!(artifact.artifact_id.starts_with("coordinator-"));
        assert_eq!(artifact.name, Some("Coordinator Result".into()));

        let text = artifact_to_text(&artifact);
        assert!(text.contains("[OK] t1"));
        assert!(text.contains("[OK] t2"));
        assert!(text.contains("2/2 succeeded"));
        assert!(text.contains("$0.0500"));
    }

    // -- swarm_result_to_artifact -------------------------------------------

    #[test]
    fn test_swarm_result_to_artifact() {
        let result = SwarmResult {
            run_id: "swarm-7".into(),
            goal: "Build entire platform".into(),
            status: SwarmStatus::Complete,
            plan: SwarmPlan {
                teams: vec![TeamObjective {
                    id: "team-1".into(),
                    name: "Backend".into(),
                    description: "Build backend".into(),
                    dependencies: vec![],
                    orchestration_mode: OrchestrationMode::HiveMind,
                    scope_paths: vec![],
                    priority: 0,
                    preferred_model: None,
                }],
            },
            team_results: vec![],
            synthesized_output: "Platform built successfully.".into(),
            total_cost: 1.50,
            total_duration_ms: 60000,
            learnings_recorded: 5,
        };

        let artifact = swarm_result_to_artifact(&result);
        assert_eq!(artifact.artifact_id, "swarm-swarm-7");
        assert_eq!(artifact.name, Some("Queen Swarm Result".into()));

        let text = artifact_to_text(&artifact);
        assert!(text.contains("Platform built successfully."));
        assert!(text.contains("$1.5000"));
        assert!(text.contains("60000ms"));
        assert!(text.contains("Learnings: 5"));
    }

    // -- task_event_to_status_update ----------------------------------------

    #[test]
    fn test_task_event_plan_created() {
        let event = TaskEvent::PlanCreated {
            plan_id: "plan-1".into(),
            tasks: vec![],
        };
        let update = task_event_to_status_update("a2a-task-1", "ctx-1", &event).unwrap();
        assert_eq!(update.task_id, "a2a-task-1");
        assert_eq!(update.context_id, "ctx-1");
        assert_eq!(update.status.state, TaskState::Working);
        assert!(!update.final_);
    }

    #[test]
    fn test_task_event_task_started() {
        let event = TaskEvent::TaskStarted {
            task_id: "t1".into(),
            description: "Investigate".into(),
            persona: "Investigate".into(),
        };
        let update = task_event_to_status_update("a2a-1", "ctx-1", &event).unwrap();
        assert_eq!(update.status.state, TaskState::Working);
        assert!(!update.final_);

        let msg = update.status.message.unwrap();
        let text = extract_message_text(&msg);
        assert!(text.contains("Started task 't1'"));
    }

    #[test]
    fn test_task_event_task_progress() {
        let event = TaskEvent::TaskProgress {
            task_id: "t1".into(),
            progress: 0.5,
            message: "Halfway done".into(),
        };
        let update = task_event_to_status_update("a2a-1", "ctx-1", &event).unwrap();
        assert_eq!(update.status.state, TaskState::Working);
        assert!(!update.final_);

        let msg = update.status.message.unwrap();
        let text = extract_message_text(&msg);
        assert!(text.contains("50%"));
        assert!(text.contains("Halfway done"));
    }

    #[test]
    fn test_task_event_task_completed() {
        let event = TaskEvent::TaskCompleted {
            task_id: "t1".into(),
            duration_ms: 1000,
            cost: 0.05,
            output_preview: "All good".into(),
        };
        let update = task_event_to_status_update("a2a-1", "ctx-1", &event).unwrap();
        assert_eq!(update.status.state, TaskState::Working);
        assert!(!update.final_);
    }

    #[test]
    fn test_task_event_task_failed() {
        let event = TaskEvent::TaskFailed {
            task_id: "t1".into(),
            error: "Out of memory".into(),
        };
        let update = task_event_to_status_update("a2a-1", "ctx-1", &event).unwrap();
        assert_eq!(update.status.state, TaskState::Working);
        assert!(!update.final_);

        let msg = update.status.message.unwrap();
        let text = extract_message_text(&msg);
        assert!(text.contains("failed"));
        assert!(text.contains("Out of memory"));
    }

    #[test]
    fn test_task_event_all_complete_success() {
        let event = TaskEvent::AllComplete {
            total_cost: 0.10,
            total_duration_ms: 5000,
            success_count: 3,
            failure_count: 0,
        };
        let update = task_event_to_status_update("a2a-1", "ctx-1", &event).unwrap();
        assert_eq!(update.status.state, TaskState::Completed);
        assert!(update.final_);
    }

    #[test]
    fn test_task_event_all_complete_with_failures() {
        let event = TaskEvent::AllComplete {
            total_cost: 0.10,
            total_duration_ms: 5000,
            success_count: 2,
            failure_count: 1,
        };
        let update = task_event_to_status_update("a2a-1", "ctx-1", &event).unwrap();
        assert_eq!(update.status.state, TaskState::Failed);
        assert!(update.final_);
    }

    // -- artifact_to_text ---------------------------------------------------

    #[test]
    fn test_artifact_to_text_single_part() {
        let artifact = Artifact {
            artifact_id: "art-1".into(),
            name: None,
            description: None,
            parts: vec![Part::text("Hello from artifact".into())],
            metadata: None,
        };
        assert_eq!(artifact_to_text(&artifact), "Hello from artifact");
    }

    #[test]
    fn test_artifact_to_text_multi_part() {
        let artifact = Artifact {
            artifact_id: "art-2".into(),
            name: None,
            description: None,
            parts: vec![
                Part::text("Line one".into()),
                Part::data(Map::new()),
                Part::text("Line two".into()),
            ],
            metadata: None,
        };
        assert_eq!(artifact_to_text(&artifact), "Line one\nLine two");
    }

    #[test]
    fn test_artifact_to_text_empty() {
        let artifact = Artifact {
            artifact_id: "art-3".into(),
            name: None,
            description: None,
            parts: vec![],
            metadata: None,
        };
        assert_eq!(artifact_to_text(&artifact), "");
    }

    // -- round-trip test ----------------------------------------------------

    #[test]
    fn test_round_trip_text_through_artifact() {
        let original = "Some analysis output from HiveMind.";
        let artifact = Artifact {
            artifact_id: "round-1".into(),
            name: None,
            description: None,
            parts: vec![Part::text(original.into())],
            metadata: None,
        };
        let extracted = artifact_to_text(&artifact);
        assert_eq!(extracted, original);
    }
}
