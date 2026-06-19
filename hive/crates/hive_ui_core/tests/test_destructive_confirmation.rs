use hive_ui_core::destructive::{DestructiveActionKind, DestructiveConfirmation};

#[test]
fn normal_destructive_confirmation_does_not_require_typed_acknowledgement() {
    let confirmation =
        DestructiveConfirmation::for_action(DestructiveActionKind::LogsClear { entries: 42 });

    assert_eq!(confirmation.confirm_label, "Clear logs");
    assert!(confirmation.acknowledgement_phrase.is_none());
    assert!(confirmation.can_confirm(""));
}

#[test]
fn token_deploy_requires_exact_typed_acknowledgement() {
    let confirmation =
        DestructiveConfirmation::for_action(DestructiveActionKind::TokenLaunchDeploy {
            chain: "Ethereum Mainnet".into(),
            token_symbol: "HIVE".into(),
        });

    assert_eq!(confirmation.confirm_label, "Deploy token");
    assert_eq!(
        confirmation.acknowledgement_phrase.as_deref(),
        Some("DEPLOY HIVE")
    );
    assert!(!confirmation.can_confirm(""));
    assert!(!confirmation.can_confirm("deploy hive"));
    assert!(confirmation.can_confirm("DEPLOY HIVE"));
}

#[test]
fn confirmation_copy_names_the_destructive_target() {
    let confirmation =
        DestructiveConfirmation::for_action(DestructiveActionKind::FilesDeleteEntry {
            target_path: "H:\\WORK\\AG\\Hive\\temp".into(),
            is_directory: true,
        });

    assert!(confirmation.title.contains("Delete folder"));
    assert!(
        confirmation
            .details
            .iter()
            .any(|line| line.contains("temp"))
    );
    assert!(confirmation.body.contains("cannot be undone"));
}

#[test]
fn shield_rule_delete_names_the_rule_target() {
    let confirmation =
        DestructiveConfirmation::for_action(DestructiveActionKind::ShieldDeleteRule {
            rule_id: "rule-123".into(),
        });

    assert_eq!(confirmation.confirm_label, "Delete rule");
    assert!(confirmation.title.contains("Delete Shield rule"));
    assert!(
        confirmation
            .details
            .iter()
            .any(|line| line.contains("rule-123"))
    );
}
