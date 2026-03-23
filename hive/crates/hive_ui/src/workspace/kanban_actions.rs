use gpui::*;
use tracing::info;

use super::{data_refresh, HiveWorkspace, KanbanAddTask};

pub(super) fn handle_kanban_add_task(
    workspace: &mut HiveWorkspace,
    _action: &KanbanAddTask,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    use hive_ui_panels::panels::kanban::{KanbanTask, Priority};

    info!("Kanban: add task");
    let task = KanbanTask {
        id: workspace
            .kanban_data
            .columns
            .iter()
            .map(|column| column.tasks.len() as u64)
            .sum::<u64>()
            + 1,
        title: "New Task".to_string(),
        description: String::new(),
        priority: Priority::Medium,
        created_at: chrono::Utc::now().format("%Y-%m-%d %H:%M").to_string(),
        assigned_model: None,
    };
    workspace.kanban_data.columns[0].tasks.push(task);
    data_refresh::save_kanban_data(workspace);
    cx.notify();
}
