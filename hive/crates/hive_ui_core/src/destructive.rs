/// Shared model for human-triggered destructive UI actions.
///
/// Workspace code owns execution, but the copy and acknowledgement rules live
/// in `hive_ui_core` so panels and tests can share one contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DestructiveActionKind {
    FilesDeleteEntry {
        target_path: String,
        is_directory: bool,
    },
    HistoryDeleteConversation {
        conversation_id: String,
    },
    HistoryClearAll {
        conversation_count: usize,
    },
    LogsClear {
        entries: usize,
    },
    CostsResetToday,
    CostsClearHistory,
    ReviewDiscardAll {
        changed_files: usize,
    },
    ReviewBranchDelete {
        branch_name: String,
    },
    ReviewGitflowFinish {
        kind: String,
        name: String,
    },
    PromptLibraryDelete {
        prompt_id: String,
    },
    ShieldDeleteRule {
        rule_id: String,
    },
    TokenLaunchDeploy {
        chain: String,
        token_symbol: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DestructiveConfirmation {
    pub action: DestructiveActionKind,
    pub title: String,
    pub body: String,
    pub details: Vec<String>,
    pub confirm_label: String,
    pub cancel_label: String,
    pub acknowledgement_phrase: Option<String>,
}

impl DestructiveConfirmation {
    pub fn for_action(action: DestructiveActionKind) -> Self {
        let cancel_label = "Cancel".to_string();

        match &action {
            DestructiveActionKind::FilesDeleteEntry {
                target_path,
                is_directory,
            } => {
                let noun = if *is_directory { "folder" } else { "file" };
                Self {
                    action: action.clone(),
                    title: format!("Delete {noun}?"),
                    body: format!("This {noun} will be permanently deleted and cannot be undone."),
                    details: vec![format!("Target: {target_path}")],
                    confirm_label: format!("Delete {noun}"),
                    cancel_label,
                    acknowledgement_phrase: None,
                }
            }
            DestructiveActionKind::HistoryDeleteConversation { conversation_id } => Self {
                action: action.clone(),
                title: "Delete conversation?".into(),
                body: "This conversation will be removed from history and cannot be undone.".into(),
                details: vec![format!("Conversation: {conversation_id}")],
                confirm_label: "Delete conversation".into(),
                cancel_label,
                acknowledgement_phrase: None,
            },
            DestructiveActionKind::HistoryClearAll { conversation_count } => Self {
                action: action.clone(),
                title: "Clear all conversations?".into(),
                body: "All saved conversations will be deleted and cannot be undone.".into(),
                details: vec![format!("Conversations affected: {conversation_count}")],
                confirm_label: "Clear conversations".into(),
                cancel_label,
                acknowledgement_phrase: None,
            },
            DestructiveActionKind::LogsClear { entries } => Self {
                action: action.clone(),
                title: "Clear logs?".into(),
                body: "Visible and persisted log entries will be cleared.".into(),
                details: vec![format!("Entries affected: {entries}")],
                confirm_label: "Clear logs".into(),
                cancel_label,
                acknowledgement_phrase: None,
            },
            DestructiveActionKind::CostsResetToday => Self {
                action: action.clone(),
                title: "Reset today's cost totals?".into(),
                body: "Today's usage and cost totals will be reset for the local tracker.".into(),
                details: vec!["This does not affect provider billing records.".into()],
                confirm_label: "Reset today".into(),
                cancel_label,
                acknowledgement_phrase: None,
            },
            DestructiveActionKind::CostsClearHistory => Self {
                action: action.clone(),
                title: "Clear cost history?".into(),
                body: "All locally tracked cost history will be cleared.".into(),
                details: vec!["This cannot be reconstructed from Hive after clearing.".into()],
                confirm_label: "Clear cost history".into(),
                cancel_label,
                acknowledgement_phrase: None,
            },
            DestructiveActionKind::ReviewDiscardAll { changed_files } => Self {
                action: action.clone(),
                title: "Discard working tree changes?".into(),
                body: "Tracked file changes will be discarded from the working tree.".into(),
                details: vec![
                    format!("Changed files affected: {changed_files}"),
                    "Untracked files may require separate cleanup.".into(),
                ],
                confirm_label: "Discard changes".into(),
                cancel_label,
                acknowledgement_phrase: None,
            },
            DestructiveActionKind::ReviewBranchDelete { branch_name } => Self {
                action: action.clone(),
                title: "Delete branch?".into(),
                body: "The selected local branch will be deleted.".into(),
                details: vec![format!("Branch: {branch_name}")],
                confirm_label: "Delete branch".into(),
                cancel_label,
                acknowledgement_phrase: None,
            },
            DestructiveActionKind::ReviewGitflowFinish { kind, name } => Self {
                action: action.clone(),
                title: "Finish Gitflow branch?".into(),
                body: "Hive will run the Gitflow finish operation for this branch.".into(),
                details: vec![format!("Type: {kind}"), format!("Name: {name}")],
                confirm_label: "Finish branch".into(),
                cancel_label,
                acknowledgement_phrase: None,
            },
            DestructiveActionKind::PromptLibraryDelete { prompt_id } => Self {
                action: action.clone(),
                title: "Delete prompt?".into(),
                body: "This prompt template will be removed from the library.".into(),
                details: vec![format!("Prompt: {prompt_id}")],
                confirm_label: "Delete prompt".into(),
                cancel_label,
                acknowledgement_phrase: None,
            },
            DestructiveActionKind::ShieldDeleteRule { rule_id } => Self {
                action: action.clone(),
                title: "Delete Shield rule?".into(),
                body: "This custom Shield rule will stop blocking matching content.".into(),
                details: vec![format!("Rule: {rule_id}")],
                confirm_label: "Delete rule".into(),
                cancel_label,
                acknowledgement_phrase: None,
            },
            DestructiveActionKind::TokenLaunchDeploy {
                chain,
                token_symbol,
            } => {
                let symbol = token_symbol.trim().to_uppercase();
                let acknowledgement_phrase = format!("DEPLOY {symbol}");
                Self {
                    action: action.clone(),
                    title: "Deploy token?".into(),
                    body: "This can create an on-chain asset and may spend real funds. Confirm the network, wallet, and token details before continuing.".into(),
                    details: vec![format!("Network: {chain}"), format!("Symbol: {symbol}")],
                    confirm_label: "Deploy token".into(),
                    cancel_label,
                    acknowledgement_phrase: Some(acknowledgement_phrase),
                }
            }
        }
    }

    pub fn can_confirm(&self, typed_acknowledgement: &str) -> bool {
        match &self.acknowledgement_phrase {
            Some(required) => typed_acknowledgement == required,
            None => true,
        }
    }
}
