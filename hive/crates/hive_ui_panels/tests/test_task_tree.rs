use hive_agents::TaskEventInfo;
use hive_ui_panels::components::task_tree::{TaskDisplayStatus, TaskTreeState};

fn sample_event_infos() -> Vec<TaskEventInfo> {
    vec![
        TaskEventInfo {
            id: "task-1".into(),
            description: "Investigate auth module".into(),
            persona: "Investigate".into(),
            dependencies: vec![],
            model_override: None,
        },
        TaskEventInfo {
            id: "task-2".into(),
            description: "Implement login flow".into(),
            persona: "Implement".into(),
            dependencies: vec!["task-1".into()],
            model_override: None,
        },
        TaskEventInfo {
            id: "task-3".into(),
            description: "Verify login tests".into(),
            persona: "Verify".into(),
            dependencies: vec!["task-2".into()],
            model_override: None,
        },
        TaskEventInfo {
            id: "task-4".into(),
            description: "Review code quality".into(),
            persona: "Code Review".into(),
            dependencies: vec!["task-2".into()],
            model_override: None,
        },
    ]
}

#[test]
fn task_tree_new_creates_pending_tasks() {
    let tree = TaskTreeState::new("Test Plan".into(), "plan-1".into(), sample_event_infos());
    assert_eq!(tree.tasks.len(), 4);
    assert_eq!(tree.title, "Test Plan");
    assert_eq!(tree.plan_id, "plan-1");
    assert!(!tree.collapsed);
    assert_eq!(tree.total_cost, 0.0);
    assert_eq!(tree.elapsed_ms, 0);

    for task in &tree.tasks {
        assert_eq!(task.status, TaskDisplayStatus::Pending);
        assert!(task.duration_ms.is_none());
        assert!(task.cost.is_none());
        assert!(task.output_preview.is_none());
        assert!(!task.expanded);
        assert!(task.model_override.is_none());
    }
}

#[test]
fn task_tree_initial_progress_is_zero() {
    let tree = TaskTreeState::new("Plan".into(), "p1".into(), sample_event_infos());
    assert_eq!(tree.progress(), 0.0);
    assert_eq!(tree.tasks_done(), 0);
}

#[test]
fn task_tree_empty_progress_is_zero() {
    let tree = TaskTreeState::new("Empty".into(), "p0".into(), vec![]);
    assert_eq!(tree.progress(), 0.0);
    assert_eq!(tree.tasks_done(), 0);
}

#[test]
fn task_tree_mark_started() {
    let mut tree = TaskTreeState::new("Plan".into(), "p1".into(), sample_event_infos());
    tree.mark_started("task-1");

    assert_eq!(tree.tasks[0].status, TaskDisplayStatus::Running);
    assert_eq!(tree.tasks[1].status, TaskDisplayStatus::Pending);
    // Running does not count as done.
    assert_eq!(tree.tasks_done(), 0);
    assert_eq!(tree.progress(), 0.0);
}

#[test]
fn task_tree_mark_completed() {
    let mut tree = TaskTreeState::new("Plan".into(), "p1".into(), sample_event_infos());
    tree.mark_started("task-1");
    tree.mark_completed("task-1", 1500, 0.03, "Analysis complete".into());

    assert_eq!(tree.tasks[0].status, TaskDisplayStatus::Completed);
    assert_eq!(tree.tasks[0].duration_ms, Some(1500));
    assert_eq!(tree.tasks[0].cost, Some(0.03));
    assert_eq!(
        tree.tasks[0].output_preview,
        Some("Analysis complete".into())
    );
    assert_eq!(tree.total_cost, 0.03);
    assert_eq!(tree.tasks_done(), 1);
    assert_eq!(tree.progress(), 0.25); // 1 of 4
}

#[test]
fn task_tree_mark_completed_empty_output() {
    let mut tree = TaskTreeState::new("Plan".into(), "p1".into(), sample_event_infos());
    tree.mark_completed("task-1", 500, 0.01, String::new());
    assert!(tree.tasks[0].output_preview.is_none());
}

#[test]
fn task_tree_mark_failed() {
    let mut tree = TaskTreeState::new("Plan".into(), "p1".into(), sample_event_infos());
    tree.mark_failed("task-2", "timeout exceeded".into());

    assert_eq!(
        tree.tasks[1].status,
        TaskDisplayStatus::Failed("timeout exceeded".into())
    );
    assert_eq!(tree.tasks_done(), 1); // failed counts as done
}

#[test]
fn task_tree_failed_counts_as_done() {
    let infos = vec![
        TaskEventInfo {
            id: "t1".into(),
            description: "Task 1".into(),
            persona: "Implement".into(),
            dependencies: vec![],
            model_override: None,
        },
        TaskEventInfo {
            id: "t2".into(),
            description: "Task 2".into(),
            persona: "Verify".into(),
            dependencies: vec![],
            model_override: None,
        },
    ];
    let mut tree = TaskTreeState::new("Plan".into(), "p1".into(), infos);
    tree.mark_failed("t1", "error".into());
    assert_eq!(tree.tasks_done(), 1);
    assert_eq!(tree.progress(), 0.5);
}

#[test]
fn task_tree_full_progress() {
    let mut tree = TaskTreeState::new("Plan".into(), "p1".into(), sample_event_infos());
    tree.mark_completed("task-1", 100, 0.01, "done".into());
    tree.mark_completed("task-2", 200, 0.02, "done".into());
    tree.mark_completed("task-3", 300, 0.03, "done".into());
    tree.mark_failed("task-4", "review issue".into());

    assert_eq!(tree.tasks_done(), 4);
    assert_eq!(tree.progress(), 1.0);
    assert!((tree.total_cost - 0.06).abs() < f64::EPSILON);
}

#[test]
fn task_tree_toggle_collapse() {
    let mut tree = TaskTreeState::new("Plan".into(), "p1".into(), sample_event_infos());
    assert!(!tree.collapsed);
    tree.toggle_collapse();
    assert!(tree.collapsed);
    tree.toggle_collapse();
    assert!(!tree.collapsed);
}

#[test]
fn task_tree_toggle_task_expand() {
    let mut tree = TaskTreeState::new("Plan".into(), "p1".into(), sample_event_infos());
    assert!(!tree.tasks[0].expanded);
    tree.toggle_task_expand("task-1");
    assert!(tree.tasks[0].expanded);
    assert!(!tree.tasks[1].expanded); // other tasks unaffected
    tree.toggle_task_expand("task-1");
    assert!(!tree.tasks[0].expanded);
}

#[test]
fn task_tree_mark_nonexistent_task_is_noop() {
    let mut tree = TaskTreeState::new("Plan".into(), "p1".into(), sample_event_infos());
    // These should not panic.
    tree.mark_started("nonexistent");
    tree.mark_completed("nonexistent", 100, 0.01, "output".into());
    tree.mark_failed("nonexistent", "error".into());
    tree.toggle_task_expand("nonexistent");
    assert_eq!(tree.tasks_done(), 0);
}

#[test]
fn task_tree_serialization_roundtrip() {
    let mut tree = TaskTreeState::new("Plan".into(), "p1".into(), sample_event_infos());
    tree.mark_completed("task-1", 1000, 0.05, "done".into());
    tree.mark_failed("task-2", "oops".into());

    let json = serde_json::to_string(&tree).unwrap();
    let deserialized: TaskTreeState = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.tasks.len(), 4);
    assert_eq!(deserialized.tasks[0].status, TaskDisplayStatus::Completed);
    assert_eq!(
        deserialized.tasks[1].status,
        TaskDisplayStatus::Failed("oops".into())
    );
    assert_eq!(deserialized.total_cost, 0.05);
}

#[test]
fn task_display_preserves_persona() {
    let tree = TaskTreeState::new("Plan".into(), "p1".into(), sample_event_infos());
    assert_eq!(tree.tasks[0].persona, "Investigate");
    assert_eq!(tree.tasks[1].persona, "Implement");
    assert_eq!(tree.tasks[2].persona, "Verify");
    assert_eq!(tree.tasks[3].persona, "Code Review");
}

#[test]
fn task_tree_cost_accumulates_correctly() {
    let mut tree = TaskTreeState::new("Plan".into(), "p1".into(), sample_event_infos());
    tree.mark_completed("task-1", 100, 0.10, "a".into());
    tree.mark_completed("task-2", 200, 0.20, "b".into());
    assert!((tree.total_cost - 0.30).abs() < 1e-10);
}

#[test]
fn task_tree_model_override() {
    let mut tree = TaskTreeState::new("Plan".into(), "p1".into(), sample_event_infos());

    // Initially no overrides
    assert!(tree.model_overrides().is_empty());

    // Pin a model to task-1
    tree.set_model_override("task-1", Some("claude-opus-4".into()));
    assert_eq!(tree.tasks[0].model_override, Some("claude-opus-4".into()));

    // Overrides list returns (id, model) pair
    let overrides = tree.model_overrides();
    assert_eq!(overrides.len(), 1);
    assert_eq!(overrides[0], ("task-1".into(), "claude-opus-4".into()));

    // Clear override
    tree.set_model_override("task-1", None);
    assert!(tree.tasks[0].model_override.is_none());
    assert!(tree.model_overrides().is_empty());
}
