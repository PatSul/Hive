use std::path::PathBuf;

use chrono::Utc;
use gpui::*;
use tracing::warn;

use hive_core::config::HiveConfig;

use super::{
    format_network_relative_time, parse_etime, status_sync, AppAiService, AppApprovalGate,
    AppConfig, AppDatabase, AppLearning, AppShield, AppSpecs, CostData, HiveWorkspace,
    HistoryData, ObserveAgentRow, ObserveRunRow, ObserveRuntimeData, ObserveSafetyData,
    ObserveSafetyEvent, ObserveSpendData, RoutingData,
};

pub(super) fn refresh_history(workspace: &mut HiveWorkspace) {
    workspace.history_data = load_history_data();
}

pub(super) fn load_history_data() -> HistoryData {
    match hive_core::ConversationStore::new() {
        Ok(store) => {
            let summaries = store.list_summaries().unwrap_or_default();
            HistoryData::from_summaries(summaries)
        }
        Err(_) => HistoryData::empty(),
    }
}

pub(super) fn refresh_learning_data(workspace: &mut HiveWorkspace, cx: &App) {
    use hive_ui_panels::panels::learning::*;

    if !cx.has_global::<AppLearning>() {
        return;
    }
    let learning = &cx.global::<AppLearning>().0;

    let log_entries = learning
        .learning_log(20)
        .unwrap_or_default()
        .into_iter()
        .map(|e| LogEntryDisplay {
            event_type: e.event_type,
            description: e.description,
            timestamp: e.timestamp,
        })
        .collect();

    let preferences = learning
        .all_preferences()
        .unwrap_or_default()
        .into_iter()
        .map(|(key, value, confidence)| PreferenceDisplay {
            key,
            value,
            confidence,
        })
        .collect();

    let routing_insights = learning
        .routing_learner
        .current_adjustments()
        .into_iter()
        .map(|adj| RoutingInsightDisplay {
            task_type: adj.task_type,
            from_tier: adj.from_tier,
            to_tier: adj.to_tier,
            confidence: adj.confidence,
        })
        .collect();

    let eval = learning.self_evaluator.evaluate().ok();

    workspace.learning_data = LearningPanelData {
        metrics: QualityMetrics {
            overall_quality: eval.as_ref().map_or(0.0, |e| e.overall_quality),
            trend: eval
                .as_ref()
                .map_or("Stable".into(), |e| format!("{:?}", e.trend)),
            total_interactions: learning.interaction_count(),
            correction_rate: eval.as_ref().map_or(0.0, |e| e.correction_rate),
            regeneration_rate: eval.as_ref().map_or(0.0, |e| e.regeneration_rate),
            cost_efficiency: eval.as_ref().map_or(0.0, |e| e.cost_per_quality_point),
        },
        log_entries,
        preferences,
        prompt_suggestions: Vec::new(),
        routing_insights,
        weak_areas: eval.as_ref().map_or(Vec::new(), |e| e.weak_areas.clone()),
        best_model: eval.as_ref().and_then(|e| e.best_model.clone()),
        worst_model: eval.as_ref().and_then(|e| e.worst_model.clone()),
    };
}

pub(super) fn refresh_activity_data(workspace: &mut HiveWorkspace, cx: &App) {
    let mut filter = workspace.activity_data.filter.clone();
    if filter.limit == 0 {
        filter.limit = 100;
    }
    filter.search = if workspace.activity_data.search_query.trim().is_empty() {
        None
    } else {
        Some(workspace.activity_data.search_query.trim().to_string())
    };

    let pending_approvals = cx
        .has_global::<AppApprovalGate>()
        .then(|| cx.global::<AppApprovalGate>().0.pending_requests())
        .unwrap_or_default();

    let log_path = HiveConfig::base_dir()
        .map(|dir| dir.join("activity.db"))
        .unwrap_or_else(|_| PathBuf::from("activity.db"));

    let (entries, cost_summary) = if let Ok(log) = hive_agents::ActivityLog::open(&log_path) {
        let entries = log.query(&filter).unwrap_or_default();
        let since = Utc::now() - chrono::Duration::hours(24);
        let cost_summary = log.cost_summary(None, since).unwrap_or_default();
        (entries, cost_summary)
    } else {
        (Vec::new(), hive_agents::CostSummary::default())
    };

    workspace.activity_data.filter = filter;
    workspace.activity_data.entries = entries;
    workspace.activity_data.pending_approvals = pending_approvals;
    workspace.activity_data.cost_summary = cost_summary;
    workspace.activity_data.runtime = ObserveRuntimeData {
        status_label: workspace.monitor_data.status.label().into(),
        active_agents: workspace.monitor_data.active_agents.len(),
        active_streams: workspace.monitor_data.active_streams,
        online_providers: workspace
            .monitor_data
            .providers
            .iter()
            .filter(|provider| provider.online)
            .count(),
        total_providers: workspace.monitor_data.providers.len(),
        request_queue_length: workspace.monitor_data.request_queue_length,
        current_run_id: workspace.monitor_data.current_run_id.clone(),
        agents: workspace
            .monitor_data
            .active_agents
            .iter()
            .take(4)
            .map(|agent| ObserveAgentRow {
                role: agent.role.clone(),
                status: agent.status.label().into(),
                phase: agent.phase.clone(),
                model: agent.model.clone(),
                started_at: format_network_relative_time(agent.started_at),
            })
            .collect(),
        recent_runs: workspace
            .monitor_data
            .run_history
            .iter()
            .take(4)
            .map(|run| ObserveRunRow {
                id: run.id.clone(),
                summary: run.task_summary.clone(),
                status: run.status.label().into(),
                started_at: run.started_at.clone(),
                cost_usd: run.cost,
            })
            .collect(),
    };
    workspace.activity_data.spend = ObserveSpendData {
        quality_score: workspace.learning_data.metrics.overall_quality,
        quality_trend: workspace.learning_data.metrics.trend.clone(),
        cost_efficiency: workspace.learning_data.metrics.cost_efficiency,
        best_model: workspace.learning_data.best_model.clone(),
        worst_model: workspace.learning_data.worst_model.clone(),
        weak_areas: workspace.learning_data.weak_areas.clone(),
    };
    workspace.activity_data.safety = ObserveSafetyData {
        shield_enabled: workspace.shield_data.shield_enabled,
        pii_detections: workspace.shield_data.pii_detections,
        secrets_blocked: workspace.shield_data.secrets_blocked,
        threats_caught: workspace.shield_data.threats_caught,
        recent_events: workspace
            .shield_data
            .recent_events
            .iter()
            .take(4)
            .map(|event| ObserveSafetyEvent {
                timestamp: event.timestamp.clone(),
                event_type: event.event_type.clone(),
                severity: event.severity.clone(),
                detail: event.detail.clone(),
            })
            .collect(),
    };
}

pub(super) fn refresh_specs_data(workspace: &mut HiveWorkspace, cx: &App) {
    use hive_ui_panels::panels::specs::SpecSummary;

    if cx.has_global::<AppSpecs>() {
        let manager = &cx.global::<AppSpecs>().0;
        workspace.specs_data.specs = manager
            .specs
            .values()
            .map(|spec| SpecSummary {
                id: spec.id.clone(),
                title: spec.title.clone(),
                status: format!("{:?}", spec.status),
                entries_total: spec.entry_count(),
                entries_checked: spec.checked_count(),
                updated_at: spec.updated_at.format("%Y-%m-%d %H:%M").to_string(),
            })
            .collect();
    }
}

pub(super) fn refresh_shield_data(
    workspace: &mut HiveWorkspace,
    cx: &mut Context<HiveWorkspace>,
) {
    if cx.has_global::<AppShield>() {
        let shield = &cx.global::<AppShield>().0;
        workspace.shield_data.enabled = true;
        workspace.shield_data.pii_detections = shield.pii_detection_count();
        workspace.shield_data.secrets_blocked = shield.secrets_blocked_count();
        workspace.shield_data.threats_caught = shield.threats_caught_count();
    }
    if cx.has_global::<AppConfig>() {
        let cfg = cx.global::<AppConfig>().0.get();
        workspace.shield_data.shield_enabled = cfg.shield_enabled;
        workspace.shield_data.secret_scan_enabled = cfg.shield.enable_secret_scan;
        workspace.shield_data.vulnerability_check_enabled = cfg.shield.enable_vulnerability_check;
        workspace.shield_data.pii_detection_enabled = cfg.shield.enable_pii_detection;
        workspace.shield_data.user_rules = cfg.shield.user_rules.clone();
    }
    workspace.shield_view.update(cx, |view, _cx| {
        view.update_from_data(&workspace.shield_data);
    });
}

pub(super) fn refresh_routing_data(workspace: &mut HiveWorkspace, cx: &App) {
    if cx.has_global::<AppAiService>() {
        workspace.routing_data = RoutingData::from_router(cx.global::<AppAiService>().0.router());
    }
    // Restore persisted custom rules (from_router starts with an empty list).
    let saved_rules = load_routing_rules();
    if !saved_rules.is_empty() {
        workspace.routing_data.custom_rules = saved_rules;
    }
}

pub(super) fn refresh_monitor_data(workspace: &mut HiveWorkspace, cx: &App) {
    use hive_ui_panels::panels::monitor::ProviderStatus;

    workspace.monitor_data.resources = status_sync::gather_system_resources(workspace);

    if cx.has_global::<AppConfig>() {
        let config = cx.global::<AppConfig>().0.get();
        let mut providers: Vec<ProviderStatus> = Vec::new();

        let has_anthropic = config.anthropic_api_key.as_ref().is_some_and(|k| !k.is_empty());
        providers.push(ProviderStatus::new(
            "Anthropic",
            has_anthropic,
            if has_anthropic { Some(0) } else { None },
        ));

        let has_openai = config.openai_api_key.as_ref().is_some_and(|k| !k.is_empty());
        providers.push(ProviderStatus::new(
            "OpenAI",
            has_openai,
            if has_openai { Some(0) } else { None },
        ));

        let has_google = config.google_api_key.as_ref().is_some_and(|k| !k.is_empty());
        providers.push(ProviderStatus::new(
            "Google Gemini",
            has_google,
            if has_google { Some(0) } else { None },
        ));

        let has_openrouter = config.openrouter_api_key.as_ref().is_some_and(|k| !k.is_empty());
        providers.push(ProviderStatus::new(
            "OpenRouter",
            has_openrouter,
            if has_openrouter { Some(0) } else { None },
        ));

        let has_groq = config.groq_api_key.as_ref().is_some_and(|k| !k.is_empty());
        providers.push(ProviderStatus::new(
            "Groq",
            has_groq,
            if has_groq { Some(0) } else { None },
        ));

        let has_ollama = !config.ollama_url.is_empty();
        providers.push(ProviderStatus::new(
            "Ollama (local)",
            has_ollama,
            if has_ollama { Some(0) } else { None },
        ));

        let has_lmstudio = !config.lmstudio_url.is_empty();
        providers.push(ProviderStatus::new(
            "LM Studio",
            has_lmstudio,
            if has_lmstudio { Some(0) } else { None },
        ));

        if config
            .local_provider_url
            .as_ref()
            .is_some_and(|url| !url.is_empty())
        {
            providers.push(ProviderStatus::new("Custom Local", true, Some(0)));
        }

        workspace.monitor_data.providers = providers;
    }

    workspace.monitor_data.background_tasks = workspace
        .agents_data
        .active_runs
        .iter()
        .filter(|r| r.is_active() && r.has_task_detail())
        .map(|r| {
            use hive_ui_panels::components::task_tree::TaskTreeState;
            TaskTreeState {
                title: r.spec_title.clone(),
                plan_id: r.id.clone(),
                tasks: r.tasks.clone(),
                collapsed: false,
                total_cost: r.cost,
                elapsed_ms: 0,
            }
        })
        .collect::<Vec<_>>();
    workspace
        .monitor_data
        .background_tasks
        .extend(workspace.swarm_task_trees.iter().cloned());

    if let Ok(output) = std::process::Command::new("ps")
        .args(["-o", "etime=", "-p", &std::process::id().to_string()])
        .output()
        && output.status.success()
    {
        let raw = String::from_utf8_lossy(&output.stdout).trim().to_string();
        workspace.monitor_data.uptime_secs = parse_etime(&raw);
    }
}

pub(super) fn refresh_logs_data(workspace: &mut HiveWorkspace, cx: &App) {
    if !workspace.logs_data.entries.is_empty() {
        return;
    }
    if !cx.has_global::<AppDatabase>() {
        return;
    }
    let db = &cx.global::<AppDatabase>().0;
    match db.recent_logs(500, 0) {
        Ok(rows) => {
            use hive_ui_panels::panels::logs::LogLevel;
            for row in rows.into_iter().rev() {
                let level = LogLevel::from_str_lossy(&row.level);
                workspace.logs_data.add_entry(level, row.source, row.message);
            }
        }
        Err(e) => {
            warn!("Failed to load persisted logs: {e}");
        }
    }
}

pub(super) fn refresh_kanban_data(workspace: &mut HiveWorkspace) {
    let path = match hive_core::config::HiveConfig::base_dir() {
        Ok(d) => d.join("kanban.json"),
        Err(_) => return,
    };
    if !path.exists() {
        return;
    }
    match std::fs::read_to_string(&path) {
        Ok(json) => match serde_json::from_str::<hive_ui_panels::panels::kanban::KanbanData>(&json)
        {
            Ok(data) => {
                workspace.kanban_data = data;
            }
            Err(e) => {
                warn!("Failed to parse kanban.json: {e}");
            }
        },
        Err(e) => {
            warn!("Failed to read kanban.json: {e}");
        }
    }
}

pub(super) fn save_kanban_data(workspace: &HiveWorkspace) {
    let path = match hive_core::config::HiveConfig::base_dir() {
        Ok(d) => d.join("kanban.json"),
        Err(e) => {
            warn!("Cannot save kanban: {e}");
            return;
        }
    };
    match serde_json::to_string_pretty(&workspace.kanban_data) {
        Ok(json) => {
            if let Err(e) = std::fs::write(&path, json) {
                warn!("Failed to write kanban.json: {e}");
            }
        }
        Err(e) => {
            warn!("Failed to serialize kanban data: {e}");
        }
    }
}

pub(super) fn save_routing_rules(workspace: &HiveWorkspace) {
    let path = match hive_core::config::HiveConfig::base_dir() {
        Ok(d) => d.join("routing_rules.json"),
        Err(e) => {
            warn!("Cannot save routing rules: {e}");
            return;
        }
    };
    match serde_json::to_string_pretty(&workspace.routing_data.custom_rules) {
        Ok(json) => {
            if let Err(e) = std::fs::write(&path, json) {
                warn!("Failed to write routing_rules.json: {e}");
            }
        }
        Err(e) => {
            warn!("Failed to serialize routing rules: {e}");
        }
    }
}

pub(super) fn load_routing_rules() -> Vec<hive_ui_panels::panels::routing::RoutingRule> {
    let path = match hive_core::config::HiveConfig::base_dir() {
        Ok(d) => d.join("routing_rules.json"),
        Err(_) => return Vec::new(),
    };
    if !path.exists() {
        return Vec::new();
    }
    match std::fs::read_to_string(&path) {
        Ok(json) => serde_json::from_str(&json).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

pub(super) fn refresh_cost_data(workspace: &mut HiveWorkspace, cx: &App) {
    workspace.cost_data = if cx.has_global::<AppAiService>() {
        CostData::from_tracker(cx.global::<AppAiService>().0.cost_tracker())
    } else {
        CostData::empty()
    };
}
