use std::sync::Arc;

use chrono::{NaiveDate, Utc};
use hive_agents::{
    ApprovalGate, ApprovalRule, AutomationService, BrainPass, CollectiveMemory, DoneCriteria,
    GuardrailPolicy, LoopIterationOutcome, LoopRunner, LoopSpec, LoopStatus, MemoryCategory,
    OperationType, RuleTrigger, SelfWorkConfig, SelfWorkPlanner, SelfWorkSource, TriggerType,
    VerifierOutcome, VerifierSpec,
};
use tempfile::tempdir;

fn schedule_spec() -> LoopSpec {
    LoopSpec::new(
        "repo-maintenance",
        "Maintain repositories",
        "Keep the local repositories reviewed and healthy",
        TriggerType::Schedule {
            cron: "0 3 * * *".into(),
        },
    )
    .with_done(DoneCriteria {
        completion_phrases: vec!["ready for review".into()],
        require_verifier_pass: true,
        require_human_approval: false,
    })
    .with_skill("/triage")
    .with_skill("/autoreview")
    .with_tool("git")
    .with_verifier_command("cargo test -p hive_agents")
    .with_memory_policy(true, true)
}

#[test]
fn loop_spec_registers_triggered_workflow_with_loop_metadata() {
    let spec = schedule_spec();
    let mut automations = AutomationService::new();

    let workflow = spec.register_workflow(&mut automations).unwrap();

    assert_eq!(workflow.name, "Maintain repositories");
    assert!(matches!(workflow.trigger, TriggerType::Schedule { .. }));
    assert_eq!(workflow.steps.len(), 1);
    assert!(workflow.steps[0].name.contains("Run loop"));
    assert!(workflow.steps[0].name.contains("repo-maintenance"));
    assert_eq!(automations.list_workflows().len(), 1);
}

#[test]
fn runner_continues_until_definition_of_done_is_verified() {
    let spec = schedule_spec();
    let mut attempts = 0;
    let mut verifier_calls = 0;

    let result = LoopRunner::new().run(
        &spec,
        |_spec, _iteration| {
            attempts += 1;
            Ok(LoopIterationOutcome {
                output: if attempts == 1 {
                    "ready for review, but tests still failing".into()
                } else {
                    "ready for review after fixing tests".into()
                },
                cost_usd: 0.02,
                memory_notes: Vec::new(),
            })
        },
        |_spec, transcript| {
            verifier_calls += 1;
            Ok(VerifierOutcome {
                passed: transcript.len() == 2,
                summary: format!("{} iteration(s) checked", transcript.len()),
            })
        },
    );

    assert_eq!(result.status, LoopStatus::Completed);
    assert_eq!(attempts, 2);
    assert_eq!(verifier_calls, 2);
    assert_eq!(result.iterations.len(), 2);
    assert!(result.succeeded());
}

#[test]
fn runner_pauses_when_guardrail_requires_human_approval() {
    let mut spec = schedule_spec();
    spec.guardrails = GuardrailPolicy {
        require_initial_approval: true,
        approval_operations: vec![OperationType::Custom("loop:repo-maintenance".into())],
    };

    let gate = Arc::new(ApprovalGate::new(vec![ApprovalRule {
        name: "all-loops".into(),
        enabled: true,
        trigger: RuleTrigger::Always,
        priority: 100,
    }]));

    let result = LoopRunner::new().with_approval_gate(gate.clone()).run(
        &spec,
        |_spec, _iteration| panic!("guardrail should pause before executing"),
        |_spec, _transcript| panic!("guardrail should pause before verifying"),
    );

    assert_eq!(result.status, LoopStatus::Paused);
    assert_eq!(result.approval_request_ids.len(), 1);
    assert_eq!(gate.pending_count(), 1);
}

#[test]
fn brain_pass_persists_completed_runs_as_work_memory_and_markdown() {
    let spec = schedule_spec();
    let result = LoopRunner::new().run(
        &spec,
        |_spec, _iteration| {
            Ok(LoopIterationOutcome {
                output: "ready for review after using cached test context".into(),
                cost_usd: 0.03,
                memory_notes: vec!["Use cached test context before rerunning broad suites".into()],
            })
        },
        |_spec, _transcript| {
            Ok(VerifierOutcome {
                passed: true,
                summary: "tests passed".into(),
            })
        },
    );

    let memory = CollectiveMemory::in_memory().unwrap();
    let dir = tempdir().unwrap();
    let archive_dir = dir.path().join("brain");
    let date = NaiveDate::from_ymd_opt(2026, 6, 21).unwrap();

    let synthesis = BrainPass::default()
        .synthesize_and_persist(&result, &memory, Some(&archive_dir), date)
        .unwrap();

    assert!(synthesis.lessons.iter().any(|lesson| {
        lesson.category == MemoryCategory::SuccessPattern
            && lesson.content.contains("repo-maintenance")
    }));
    assert!(synthesis.lessons.iter().any(|lesson| {
        lesson.category == MemoryCategory::CodePattern
            && lesson.content.contains("cached test context")
    }));

    let recalled = memory
        .recall(
            "cached test context",
            Some(MemoryCategory::CodePattern),
            None,
            10,
        )
        .unwrap();
    assert_eq!(recalled.len(), 1);
    assert_eq!(
        recalled[0].source_run_id.as_deref(),
        Some(result.run_id.as_str())
    );

    let markdown = std::fs::read_to_string(archive_dir.join(format!("{date}.md"))).unwrap();
    assert!(markdown.contains("Success Patterns"));
    assert!(markdown.contains("cached test context"));
    assert!(markdown.contains(&Utc::now().format("%Y").to_string()));
}

#[test]
fn self_work_planner_selects_project_issue_and_builds_loop_spec() {
    let dir = tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join(".planning")).unwrap();
    std::fs::write(
        dir.path().join(".planning/ISSUES.md"),
        "- [ ] Fix provider routing regression so chat can use configured keys\n",
    )
    .unwrap();
    std::fs::create_dir_all(dir.path().join("hive/crates/hive_ai/src")).unwrap();
    std::fs::write(
        dir.path().join("hive/crates/hive_ai/src/service.rs"),
        "// TODO: add more routing regression coverage\n",
    )
    .unwrap();

    let config = SelfWorkConfig::for_hive_repo(dir.path())
        .with_objective_hint("autonomously improve Hive")
        .with_verifier_command("cargo check -p hive_agents --lib");

    let plan = SelfWorkPlanner::new(config).plan().unwrap();

    assert_eq!(plan.selected.source, SelfWorkSource::PlanningIssue);
    assert!(
        plan.selected
            .title
            .contains("Fix provider routing regression")
    );
    assert!(
        plan.loop_spec
            .objective
            .contains("Fix provider routing regression")
    );
    assert!(
        plan.loop_spec
            .skills
            .contains(&"superpowers:test-driven-development".to_string())
    );
    assert!(
        plan.loop_spec
            .tools
            .contains(&"execute_command".to_string())
    );
    assert!(plan.loop_spec.done.require_verifier_pass);
    assert!(matches!(
        plan.loop_spec.verifier,
        Some(VerifierSpec::Command { ref command }) if command == "cargo check -p hive_agents --lib"
    ));
    assert_eq!(plan.candidates.len(), 2);
}

#[test]
fn self_work_planner_falls_back_to_repo_health_when_no_tasks_are_found() {
    let dir = tempdir().unwrap();
    std::fs::write(dir.path().join("Cargo.toml"), "[workspace]\n").unwrap();

    let plan = SelfWorkPlanner::new(SelfWorkConfig::for_hive_repo(dir.path()))
        .plan()
        .unwrap();

    assert_eq!(plan.selected.source, SelfWorkSource::RepoHealth);
    assert!(plan.selected.objective.contains("self-health"));
    assert!(plan.loop_spec.objective.contains("self-health"));
    assert!(plan.loop_spec.tools.contains(&"git_status".to_string()));
}
