use hive_ui_panels::panels::learning::{LearningPanelData, QualityMetrics};

#[test]
fn learning_panel_data_empty() {
    let data = LearningPanelData::empty();
    assert!(data.log_entries.is_empty());
    assert!(data.preferences.is_empty());
    assert!(data.prompt_suggestions.is_empty());
    assert!(data.routing_insights.is_empty());
    assert!(data.weak_areas.is_empty());
    assert!(data.best_model.is_none());
    assert!(data.worst_model.is_none());
}

#[test]
fn learning_panel_data_sample() {
    let data = LearningPanelData::sample();
    assert!(!data.log_entries.is_empty());
    assert!(!data.preferences.is_empty());
    assert!(!data.routing_insights.is_empty());
    assert!(data.best_model.is_some());
    assert!(data.worst_model.is_some());
}

#[test]
fn quality_metrics_empty() {
    let m = QualityMetrics::empty();
    assert_eq!(m.overall_quality, 0.0);
    assert_eq!(m.trend, "Stable");
    assert_eq!(m.total_interactions, 0);
}
