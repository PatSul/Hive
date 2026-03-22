use gpui::*;
use tracing::info;

use super::{HiveWorkspace, RoutingAddRule, data_refresh};

pub(super) fn handle_routing_add_rule(
    workspace: &mut HiveWorkspace,
    _action: &RoutingAddRule,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    use hive_ui_panels::panels::routing::RoutingRule;

    let rule_number = workspace.routing_data.custom_rules.len() + 1;
    let name = format!("Rule {rule_number}");
    info!("Routing: add rule '{name}'");

    workspace.routing_data.custom_rules.push(RoutingRule {
        name,
        condition: String::new(),
        target_model: "auto".to_string(),
        enabled: true,
    });
    data_refresh::save_routing_rules(workspace);
    cx.notify();
}
