use std::collections::HashSet;

use gpui::App;
use tracing::warn;

use super::{AgentsRunWorkflow, AppAutomation, AppSpecs, HiveWorkspace};

impl HiveWorkspace {
    pub(super) fn make_workflow_for_run(
        &self,
        action: &AgentsRunWorkflow,
        cx: &App,
    ) -> Option<hive_agents::automation::Workflow> {
        if !cx.has_global::<AppAutomation>() {
            return None;
        }

        let requested_id = if action.workflow_id.trim().is_empty() {
            hive_agents::automation::BUILTIN_DOGFOOD_WORKFLOW_ID.to_string()
        } else {
            action.workflow_id.clone()
        };

        let automation = &cx.global::<AppAutomation>().0;
        let workflow = automation
            .clone_workflow(&requested_id)
            .or_else(|| {
                automation.clone_workflow(hive_agents::automation::BUILTIN_DOGFOOD_WORKFLOW_ID)
            })
            .or_else(|| Some(Self::fallback_workflow(&requested_id)));

        let Some(mut workflow) = workflow else {
            warn!("Agents: unable to resolve workflow '{requested_id}' for planned execution");
            return None;
        };

        let instruction = action.instruction.trim();
        if !instruction.is_empty() {
            let planned_steps = self.workflow_steps_from_instruction(
                instruction,
                &action.source,
                &action.source_id,
                cx,
            );
            if !planned_steps.is_empty() {
                workflow.steps = planned_steps;
                workflow.name = if action.source.is_empty() {
                    "Planned Workflow".to_string()
                } else if action.source_id.is_empty() {
                    format!("Planned Workflow ({})", action.source)
                } else {
                    format!("Planned Workflow ({}:{})", action.source, action.source_id)
                };
                workflow.description = format!(
                    "Planned execution for {} {}",
                    if action.source.is_empty() {
                        "manual action"
                    } else {
                        action.source.as_str()
                    },
                    if action.source_id.is_empty() {
                        "request"
                    } else {
                        action.source_id.as_str()
                    }
                );
            }
        }

        if workflow.steps.is_empty() {
            workflow.steps = self.fallback_workflow_steps();
        }

        Some(workflow)
    }

    fn workflow_steps_from_instruction(
        &self,
        instruction: &str,
        source: &str,
        source_id: &str,
        cx: &App,
    ) -> Vec<hive_agents::automation::WorkflowStep> {
        let explicit = Self::extract_explicit_commands(instruction);
        let mut commands = if explicit.is_empty() {
            self.extract_keyword_commands(instruction)
                .into_iter()
                .chain(self.extract_source_commands(source, source_id, cx))
                .collect::<Vec<_>>()
        } else {
            explicit
        };

        commands = Self::dedupe_preserve_order(commands);
        if commands.is_empty() {
            commands = self.fallback_workflow_commands();
        }

        commands
            .into_iter()
            .enumerate()
            .map(|(idx, command)| hive_agents::automation::WorkflowStep {
                id: format!("runtime:{idx}"),
                name: format!("Run command {idx}"),
                action: hive_agents::automation::ActionType::RunCommand { command },
                conditions: Vec::new(),
                timeout_secs: Some(900),
                retry_count: 0,
            })
            .collect()
    }

    fn extract_explicit_commands(instruction: &str) -> Vec<String> {
        let mut commands = Vec::new();
        let mut in_fence = false;

        for line in instruction.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            if line.starts_with("```") {
                in_fence = !in_fence;
                continue;
            }

            if in_fence {
                Self::add_command_if_valid(line, &mut commands);
                continue;
            }

            let mut remaining = line;
            while let Some(start) = remaining.find('`') {
                let after = &remaining[start + 1..];
                let Some(end) = after.find('`') else {
                    break;
                };
                let candidate = &after[..end];
                Self::add_command_if_valid(candidate, &mut commands);
                remaining = &after[end + 1..];
            }

            if let Some((prefix, rest)) = line.split_once(':') {
                let normalized = prefix.trim().to_ascii_lowercase();
                if matches!(
                    normalized.as_str(),
                    "run" | "command" | "run command" | "execute"
                ) {
                    Self::add_command_if_valid(rest, &mut commands);
                    continue;
                }
            }

            Self::add_command_if_valid(line, &mut commands);
        }

        commands
    }

    fn extract_keyword_commands(&self, instruction: &str) -> Vec<String> {
        let lower = instruction.to_lowercase();
        let mut commands = Vec::new();

        if lower.contains("build") {
            commands.push("cargo check --quiet".to_string());
        }

        if lower.contains("test") {
            commands.push("cargo test --quiet -p hive_app".to_string());
        }

        if lower.contains("lint") || lower.contains("format") {
            commands.push("cargo fmt --check".to_string());
            commands.push("cargo clippy --all-targets -- -D warnings".to_string());
        }

        if lower.contains("release") {
            commands.push("cargo build --release".to_string());
        }

        if lower.contains("docs") {
            commands.push("cargo doc --no-deps --all-features".to_string());
        }

        if lower.contains("status") {
            commands.push("git status --short".to_string());
        }

        if lower.contains("diff") {
            commands.push("git diff --stat".to_string());
        }

        Self::dedupe_preserve_order(commands)
    }

    fn extract_source_commands(&self, source: &str, source_id: &str, cx: &App) -> Vec<String> {
        let source = source.to_lowercase();
        let mut commands = Vec::new();

        if source == "spec"
            && !source_id.is_empty()
            && cx.has_global::<AppSpecs>()
            && let Some(spec) = cx.global::<AppSpecs>().0.specs.get(source_id)
        {
            if spec.entry_count() == 0 || spec.checked_count() < spec.entry_count() {
                commands.push("cargo check --quiet".to_string());
            }
            commands.push("cargo test --quiet -p hive_app".to_string());
        }

        if source == "kanban-task" && !source_id.is_empty() {
            let task_id: u64 = source_id.parse().unwrap_or(0);
            if task_id > 0 {
                for col in &self.kanban_data.columns {
                    if let Some(task) = col.tasks.iter().find(|task| task.id == task_id) {
                        let title = task.title.to_lowercase();
                        let desc = task.description.to_lowercase();
                        if title.contains("build") || desc.contains("build") {
                            commands.push("cargo check --quiet".to_string());
                        }
                        if title.contains("test") || desc.contains("test") {
                            commands.push("cargo test --quiet -p hive_app".to_string());
                        }
                        if title.contains("lint") || desc.contains("lint") {
                            commands.push("cargo fmt --check".to_string());
                            commands.push("cargo clippy --all-targets -- -D warnings".to_string());
                        }
                        break;
                    }
                }
            }
        }

        Self::dedupe_preserve_order(commands)
    }

    fn add_command_if_valid(raw: &str, out: &mut Vec<String>) {
        let Some(command) = Self::normalize_command(raw) else {
            return;
        };
        out.push(command);
    }

    fn normalize_command(raw: &str) -> Option<String> {
        let command = raw
            .trim()
            .trim_matches(['"', '\'', '`'])
            .trim_end_matches(';')
            .trim();
        if command.is_empty() || !Self::is_command_like(command) {
            return None;
        }
        Some(command.to_string())
    }

    fn is_command_like(text: &str) -> bool {
        let lower = text.to_lowercase();
        const PREFIXES: [&str; 11] = [
            "cargo ",
            "git ",
            "npm ",
            "pnpm ",
            "yarn ",
            "make ",
            "python ",
            "pytest ",
            "cargo.exe ",
            "./",
            "bash ",
        ];
        PREFIXES.iter().any(|prefix| lower.starts_with(prefix))
            || lower == "cargo"
            || lower == "git"
    }

    fn dedupe_preserve_order(commands: Vec<String>) -> Vec<String> {
        let mut seen = HashSet::new();
        commands
            .into_iter()
            .filter(|command| seen.insert(command.clone()))
            .collect()
    }

    fn fallback_workflow(workflow_id: &str) -> hive_agents::automation::Workflow {
        Self::fallback_workflow_with_id(workflow_id)
    }

    fn fallback_workflow_with_id(workflow_id: &str) -> hive_agents::automation::Workflow {
        let now = chrono::Utc::now();
        hive_agents::automation::Workflow {
            id: workflow_id.to_string(),
            name: "Local Build Check".to_string(),
            description: "Fallback local validation loop.".to_string(),
            trigger: hive_agents::automation::TriggerType::ManualTrigger,
            steps: Self::fallback_workflow_steps_static(),
            status: hive_agents::automation::WorkflowStatus::Active,
            created_at: now,
            updated_at: now,
            last_run: None,
            run_count: 0,
        }
    }

    fn fallback_workflow_steps(&self) -> Vec<hive_agents::automation::WorkflowStep> {
        Self::fallback_workflow_steps_static()
    }

    fn fallback_workflow_steps_static() -> Vec<hive_agents::automation::WorkflowStep> {
        vec![
            hive_agents::automation::WorkflowStep {
                id: "fallback:check".to_string(),
                name: "Cargo check".to_string(),
                action: hive_agents::automation::ActionType::RunCommand {
                    command: "cargo check --quiet".to_string(),
                },
                conditions: Vec::new(),
                timeout_secs: Some(900),
                retry_count: 0,
            },
            hive_agents::automation::WorkflowStep {
                id: "fallback:test".to_string(),
                name: "Cargo test".to_string(),
                action: hive_agents::automation::ActionType::RunCommand {
                    command: "cargo test --quiet -p hive_app".to_string(),
                },
                conditions: Vec::new(),
                timeout_secs: Some(1200),
                retry_count: 0,
            },
            hive_agents::automation::WorkflowStep {
                id: "fallback:status".to_string(),
                name: "Git status".to_string(),
                action: hive_agents::automation::ActionType::RunCommand {
                    command: "git status --short".to_string(),
                },
                conditions: Vec::new(),
                timeout_secs: Some(120),
                retry_count: 0,
            },
            hive_agents::automation::WorkflowStep {
                id: "fallback:diff".to_string(),
                name: "Git diff".to_string(),
                action: hive_agents::automation::ActionType::RunCommand {
                    command: "git diff --stat".to_string(),
                },
                conditions: Vec::new(),
                timeout_secs: Some(120),
                retry_count: 0,
            },
        ]
    }

    fn fallback_workflow_commands(&self) -> Vec<String> {
        Self::fallback_workflow_steps_static()
            .into_iter()
            .filter_map(|step| match step.action {
                hive_agents::automation::ActionType::RunCommand { command } => Some(command),
                _ => None,
            })
            .collect()
    }
}
