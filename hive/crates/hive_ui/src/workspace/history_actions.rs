use gpui::*;
use hive_ui_core::{DestructiveActionKind, DestructiveConfirmation};
use tracing::{info, warn};

use super::{
    HistoryClearAll, HistoryClearAllCancel, HistoryClearAllConfirm, HistoryData,
    HistoryDeleteConversation, HistoryLoadConversation, HistoryRefresh, HistorySetSearchQuery,
    HiveWorkspace, Panel, data_refresh, destructive_actions,
};

pub(super) fn handle_history_load(
    workspace: &mut HiveWorkspace,
    action: &HistoryLoadConversation,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    info!("History: load conversation {}", action.conversation_id);
    let result = workspace.chat_service.update(cx, |svc, _cx| {
        svc.load_conversation(&action.conversation_id)
    });
    match result {
        Ok(()) => {
            workspace.cached_chat_data.markdown_cache.clear();
            workspace.sidebar.active_panel = Panel::Chat;
            workspace.session_dirty = true;
        }
        Err(e) => warn!("History: failed to load conversation: {e}"),
    }
    cx.notify();
}

pub(super) fn handle_history_delete(
    workspace: &mut HiveWorkspace,
    action: &HistoryDeleteConversation,
    window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    let confirmation =
        DestructiveConfirmation::for_action(DestructiveActionKind::HistoryDeleteConversation {
            conversation_id: action.conversation_id.clone(),
        });
    destructive_actions::request_confirmation(workspace, confirmation, window, cx);
}

pub(super) fn execute_confirmed_history_delete(
    workspace: &mut HiveWorkspace,
    conversation_id: &str,
    cx: &mut Context<HiveWorkspace>,
) {
    info!("History: delete conversation {conversation_id}");
    if let Ok(store) = hive_core::ConversationStore::new()
        && let Err(e) = store.delete(conversation_id)
    {
        warn!("History: failed to delete conversation: {e}");
    }
    data_refresh::refresh_history(workspace);
    cx.notify();
}

pub(super) fn handle_history_refresh(
    workspace: &mut HiveWorkspace,
    _action: &HistoryRefresh,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    data_refresh::refresh_history(workspace);
    cx.notify();
}

pub(super) fn handle_history_set_search_query(
    workspace: &mut HiveWorkspace,
    action: &HistorySetSearchQuery,
    window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    workspace.history_data.search_query = action.query.clone();
    if workspace.history_search_input.read(cx).value() != action.query {
        workspace.history_search_input.update(cx, |input, cx| {
            input.set_value(action.query.clone(), window, cx);
        });
    }
    cx.notify();
}

pub(super) fn handle_history_clear_all(
    workspace: &mut HiveWorkspace,
    _action: &HistoryClearAll,
    window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    info!("History: clear all requested");
    let confirmation =
        DestructiveConfirmation::for_action(DestructiveActionKind::HistoryClearAll {
            conversation_count: workspace.history_data.conversations.len(),
        });
    destructive_actions::request_confirmation(workspace, confirmation, window, cx);
}

pub(super) fn handle_history_clear_all_confirm(
    workspace: &mut HiveWorkspace,
    _action: &HistoryClearAllConfirm,
    window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    handle_history_clear_all(workspace, &HistoryClearAll, window, cx);
}

pub(super) fn execute_confirmed_history_clear_all(
    workspace: &mut HiveWorkspace,
    cx: &mut Context<HiveWorkspace>,
) {
    info!("History: clear all confirmed - deleting all conversations");

    if let Ok(store) = hive_core::ConversationStore::new() {
        match store.delete_all() {
            Ok(count) => info!("History: deleted {count} conversation files"),
            Err(e) => warn!("History: failed to delete conversation files: {e}"),
        }
    }

    if let Ok(db) = hive_core::persistence::Database::open() {
        match db.clear_all_conversations() {
            Ok(count) => info!("History: deleted {count} conversations from database"),
            Err(e) => warn!("History: failed to clear conversations from database: {e}"),
        }
    }

    workspace.chat_service.update(cx, |svc, _cx| {
        svc.new_conversation();
    });
    workspace.cached_chat_data.markdown_cache.clear();

    workspace.history_data = HistoryData::empty();
    workspace.session_dirty = true;
    cx.notify();
}

pub(super) fn handle_history_clear_all_cancel(
    workspace: &mut HiveWorkspace,
    _action: &HistoryClearAllCancel,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    info!("History: clear all cancelled");
    workspace.history_data.confirming_clear = false;
    cx.notify();
}
