use gpui::*;
use tracing::{info, warn};

use super::{
    data_refresh, HiveWorkspace, HistoryClearAll, HistoryClearAllCancel, HistoryClearAllConfirm,
    HistoryData, HistoryDeleteConversation, HistoryLoadConversation, HistoryRefresh, Panel,
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
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    info!("History: delete conversation {}", action.conversation_id);
    if let Ok(store) = hive_core::ConversationStore::new()
        && let Err(e) = store.delete(&action.conversation_id)
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

pub(super) fn handle_history_clear_all(
    workspace: &mut HiveWorkspace,
    _action: &HistoryClearAll,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    info!("History: clear all requested - showing confirmation");
    workspace.history_data.confirming_clear = true;
    cx.notify();
}

pub(super) fn handle_history_clear_all_confirm(
    workspace: &mut HiveWorkspace,
    _action: &HistoryClearAllConfirm,
    _window: &mut Window,
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
