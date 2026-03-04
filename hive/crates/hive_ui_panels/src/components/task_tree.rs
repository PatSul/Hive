//! Task Tree — live task breakdown state for coordinator execution.
//!
//! Tracks per-task status (Pending / Running / Completed / Failed) and
//! provides aggregate progress. Used by the agents panel to show a
//! drill-down view of running orchestration plans.

use serde::{Deserialize, Serialize};

use gpui::*;
use gpui_component::{Icon, IconName};
use hive_ui_core::{AppTheme, HiveTheme};

use hive_agents::TaskEventInfo;

// ---------------------------------------------------------------------------
// Task Display Status
// ---------------------------------------------------------------------------

/// Visual status of a single task in the tree.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum TaskDisplayStatus {
    Pending,
    Running,
    Completed,
    Failed(String),
}

// ---------------------------------------------------------------------------
// Task Display
// ---------------------------------------------------------------------------

/// Display-oriented representation of a single task.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskDisplay {
    pub id: String,
    pub description: String,
    pub persona: String,
    pub status: TaskDisplayStatus,
    pub duration_ms: Option<u64>,
    pub cost: Option<f64>,
    pub output_preview: Option<String>,
    pub expanded: bool,
}

// ---------------------------------------------------------------------------
// Task Tree State
// ---------------------------------------------------------------------------

/// Complete state for a task tree representing one coordinator plan execution.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskTreeState {
    pub title: String,
    pub plan_id: String,
    pub tasks: Vec<TaskDisplay>,
    pub collapsed: bool,
    pub total_cost: f64,
    pub elapsed_ms: u64,
}

impl TaskTreeState {
    /// Create a new tree from plan info. All tasks start as Pending.
    pub fn new(title: String, plan_id: String, tasks: Vec<TaskEventInfo>) -> Self {
        let displays = tasks
            .into_iter()
            .map(|t| TaskDisplay {
                id: t.id,
                description: t.description,
                persona: t.persona,
                status: TaskDisplayStatus::Pending,
                duration_ms: None,
                cost: None,
                output_preview: None,
                expanded: false,
            })
            .collect();

        Self {
            title,
            plan_id,
            tasks: displays,
            collapsed: false,
            total_cost: 0.0,
            elapsed_ms: 0,
        }
    }

    /// Mark a task as Running.
    pub fn mark_started(&mut self, task_id: &str) {
        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == task_id) {
            task.status = TaskDisplayStatus::Running;
        }
    }

    /// Mark a task as Completed with execution details.
    pub fn mark_completed(
        &mut self,
        task_id: &str,
        duration_ms: u64,
        cost: f64,
        output: String,
    ) {
        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == task_id) {
            task.status = TaskDisplayStatus::Completed;
            task.duration_ms = Some(duration_ms);
            task.cost = Some(cost);
            task.output_preview = if output.is_empty() {
                None
            } else {
                Some(output)
            };
            self.total_cost += cost;
        }
    }

    /// Mark a task as Failed with an error message.
    pub fn mark_failed(&mut self, task_id: &str, error: String) {
        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == task_id) {
            task.status = TaskDisplayStatus::Failed(error);
        }
    }

    /// Fraction of tasks that are done (completed + failed) out of total.
    pub fn progress(&self) -> f32 {
        if self.tasks.is_empty() {
            return 0.0;
        }
        self.tasks_done() as f32 / self.tasks.len() as f32
    }

    /// Count of tasks that are finished (completed or failed).
    pub fn tasks_done(&self) -> usize {
        self.tasks
            .iter()
            .filter(|t| {
                matches!(
                    t.status,
                    TaskDisplayStatus::Completed | TaskDisplayStatus::Failed(_)
                )
            })
            .count()
    }

    /// Toggle the collapsed state of the entire tree.
    pub fn toggle_collapse(&mut self) {
        self.collapsed = !self.collapsed;
    }

    /// Toggle the expanded state of a specific task (for showing output).
    pub fn toggle_task_expand(&mut self, task_id: &str) {
        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == task_id) {
            task.expanded = !task.expanded;
        }
    }
}

// ---------------------------------------------------------------------------
// GPUI View
// ---------------------------------------------------------------------------

pub struct TaskTreeView {
    pub state: TaskTreeState,
}

impl TaskTreeView {
    pub fn new(state: TaskTreeState) -> Self {
        Self { state }
    }
}

impl Render for TaskTreeView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = if cx.has_global::<AppTheme>() {
            cx.global::<AppTheme>().0.clone()
        } else {
            HiveTheme::dark()
        };

        // Render header
        let header = div()
            .flex()
            .items_center()
            .justify_between()
            .w_full()
            .p(theme.space_2)
            .bg(theme.bg_surface)
            .rounded(theme.radius_sm)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(theme.space_2)
                    .child(
                        Icon::new(if self.state.collapsed {
                            IconName::ChevronRight
                        } else {
                            IconName::ChevronDown
                        })
                        .size_4()
                        .text_color(theme.text_muted),
                    )
                    .child(
                        div()
                            .text_size(theme.font_size_sm)
                            .font_weight(FontWeight::BOLD)
                            .text_color(theme.text_primary)
                            .child(self.state.title.clone()),
                    ),
            )
            .child(
                div()
                    .text_size(theme.font_size_xs)
                    .text_color(theme.text_muted)
                    .child(format!("{:.1}%", self.state.progress() * 100.0)),
            );

        if self.state.collapsed {
            return div().w_full().child(header);
        }

        // Render tasks in a parallel wave list format
        let tasks_list = self.state.tasks.iter().map(|task| {
            let (icon, color) = match &task.status {
                TaskDisplayStatus::Pending => (IconName::Info, theme.text_muted),
                TaskDisplayStatus::Running => (IconName::Loader, theme.accent_cyan),
                TaskDisplayStatus::Completed => (IconName::CircleCheck, theme.accent_green),
                TaskDisplayStatus::Failed(_) => (IconName::CircleX, theme.accent_red),
            };

            let row = div()
                .flex()
                .items_center()
                .w_full()
                .p(theme.space_2)
                .gap(theme.space_3)
                .bg(theme.bg_secondary)
                .child(Icon::new(icon).size_4().text_color(color))
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .text_size(theme.font_size_sm)
                                .text_color(theme.text_primary)
                                .child(task.description.clone()),
                        )
                        .child(
                            div()
                                .flex()
                                .gap(theme.space_2)
                                .text_size(theme.font_size_xs)
                                .text_color(theme.text_muted)
                                .child(format!("@{}", task.persona))
                                .children(task.duration_ms.map(|ms| format!("{}ms", ms)))
                                .children(task.cost.map(|c| format!("${:.4}", c))),
                        ),
                );

            if task.expanded {
                if let Some(err) = match &task.status {
                    TaskDisplayStatus::Failed(e) => Some(e.clone()),
                    _ => None,
                } {
                    div()
                        .w_full()
                        .flex_col()
                        .child(row)
                        .child(
                            div()
                                .w_full()
                                .p(theme.space_2)
                                .text_size(theme.font_size_xs)
                                .text_color(theme.accent_red)
                                .bg(theme.bg_surface)
                                .child(err),
                        )
                } else if let Some(preview) = &task.output_preview {
                    div()
                        .w_full()
                        .flex_col()
                        .child(row)
                        .child(
                            div()
                                .w_full()
                                .p(theme.space_2)
                                .text_size(theme.font_size_xs)
                                .text_color(theme.text_muted)
                                .bg(theme.bg_surface)
                                .child(preview.clone()),
                        )
                } else {
                    row
                }
            } else {
                row
            }
        });

        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(theme.space_1)
            .child(header)
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .pl(theme.space_4) // Indent the subtasks
                    .gap(theme.space_1)
                    .children(tasks_list),
            )
    }
}

