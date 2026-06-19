use hive_ui_core::{HistorySetSearchQuery, LogsSetSearchQuery};

#[test]
fn history_and_logs_search_actions_carry_query_text() {
    let history = HistorySetSearchQuery {
        query: "auth".into(),
    };
    let logs = LogsSetSearchQuery {
        query: "error".into(),
    };

    assert_eq!(history.query, "auth");
    assert_eq!(logs.query, "error");
}
