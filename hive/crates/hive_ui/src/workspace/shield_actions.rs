use gpui::*;
use hive_ui_core::{DestructiveActionKind, DestructiveConfirmation, ShieldDeleteRule};

use super::{AppConfig, AppShield, HiveWorkspace, destructive_actions};

pub(super) fn handle_shield_delete_rule(
    workspace: &mut HiveWorkspace,
    action: &ShieldDeleteRule,
    window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    let exists = workspace
        .shield_view
        .read(cx)
        .user_rules
        .iter()
        .any(|rule| rule.id == action.rule_id);
    if !exists {
        return;
    }

    let confirmation =
        DestructiveConfirmation::for_action(DestructiveActionKind::ShieldDeleteRule {
            rule_id: action.rule_id.clone(),
        });
    destructive_actions::request_confirmation(workspace, confirmation, window, cx);
}

pub(super) fn execute_confirmed_shield_delete_rule(
    workspace: &mut HiveWorkspace,
    rule_id: &str,
    cx: &mut Context<HiveWorkspace>,
) {
    let rule_id = rule_id.to_string();
    workspace.shield_view.update(cx, |view, cx| {
        view.user_rules.retain(|rule| rule.id != rule_id);
        cx.emit(hive_ui_panels::panels::shield::ShieldConfigChanged);
        cx.notify();
    });
    cx.notify();
}

pub(super) fn handle_shield_config_save(
    workspace: &mut HiveWorkspace,
    cx: &mut Context<HiveWorkspace>,
) {
    let snapshot = workspace.shield_view.read(cx).collect_shield_config();

    if cx.has_global::<AppConfig>() {
        let _ = cx.global::<AppConfig>().0.update(|config| {
            config.shield_enabled = snapshot.shield_enabled;
            config.shield.enable_secret_scan = snapshot.secret_scan_enabled;
            config.shield.enable_vulnerability_check = snapshot.vulnerability_check_enabled;
            config.shield.enable_pii_detection = snapshot.pii_detection_enabled;
            config.shield.user_rules = snapshot.user_rules.clone();
        });
    }

    if cx.has_global::<AppConfig>() {
        let config = cx.global::<AppConfig>().0.get();
        let shield = std::sync::Arc::new(hive_shield::HiveShield::new(config.shield.clone()));
        cx.set_global(AppShield(shield));
    }
}
