use gpui::*;
use tracing::info;

use super::{HiveWorkspace, RoutingAddRule};

pub(super) fn handle_routing_add_rule(
    workspace: &mut HiveWorkspace,
    _action: &RoutingAddRule,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    use hive_ui_panels::panels::routing::RoutingRule;

    info!("Routing: add rule");
    workspace.routing_data.custom_rules.push(RoutingRule {
        name: "New Rule".to_string(),
        condition: "task_type == \"code\"".to_string(),
        target_model: "auto".to_string(),
        enabled: true,
    });
    cx.notify();
}
