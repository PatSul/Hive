use gpui::*;

use super::{AppConfig, AppShield, HiveWorkspace};

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
