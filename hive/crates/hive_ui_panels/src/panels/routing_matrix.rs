//! Routing Matrix panel.
//!
//! Edits `HiveConfig.routing_policy`: a per-category model routing policy with
//! an allow-list and quality floor per task category, plus a global
//! cost-vs-quality preference. Mirrors the stateful pattern of
//! [`crate::panels::settings::SettingsView`]: a GPUI [`Entity`] implementing
//! [`Render`], loading from `AppConfig` on construction and emitting a
//! [`RoutingMatrixSaved`] event when the user clicks Save. The workspace
//! subscribes to that event to persist via `ConfigManager` and live-apply the
//! policy to the running router.

use gpui::*;
use gpui_component::input::{Input, InputState};

use hive_ai::model_registry::MODEL_REGISTRY;
use hive_ai::routing::CapabilityTaskType;
use hive_core::config::{CategoryPolicy, RoutingPolicy};
use hive_ui_core::{AppConfig, AppTheme, HiveTheme};

/// The twelve task categories, in display order. The display string is the key
/// persisted in [`CategoryPolicy::category`].
const CATEGORIES: [CapabilityTaskType; 12] = [
    CapabilityTaskType::Coding,
    CapabilityTaskType::Reasoning,
    CapabilityTaskType::CreativeWriting,
    CapabilityTaskType::Math,
    CapabilityTaskType::InstructionFollowing,
    CapabilityTaskType::Translation,
    CapabilityTaskType::Summarization,
    CapabilityTaskType::DataAnalysis,
    CapabilityTaskType::ToolUse,
    CapabilityTaskType::Agentic,
    CapabilityTaskType::Vision,
    CapabilityTaskType::GeneralChat,
];

/// The selectable quality floors. Empty string = no floor.
const FLOORS: [(&str, &str); 5] = [
    ("", "None"),
    ("free", "Free"),
    ("budget", "Budget"),
    ("mid", "Mid"),
    ("premium", "Premium"),
];

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

/// Emitted when the user clicks Save. The workspace subscribes to this and
/// persists the assembled [`RoutingPolicy`] to `AppConfig`, then live-applies
/// it to the running router.
#[derive(Debug, Clone)]
pub struct RoutingMatrixSaved;

// ---------------------------------------------------------------------------
// Per-category UI state
// ---------------------------------------------------------------------------

/// In-memory editing state for a single task category.
struct CategoryRow {
    /// The `CapabilityTaskType` this row edits.
    task: CapabilityTaskType,
    /// Allowed model-id substrings selected via toggle chips. Empty = all.
    allow: Vec<String>,
    /// Selected floor key: "" | "free" | "budget" | "mid" | "premium".
    floor: String,
}

// ---------------------------------------------------------------------------
// RoutingMatrixView
// ---------------------------------------------------------------------------

/// Interactive Routing Matrix panel backed by GPUI state.
///
/// Pre-populates from the loaded `routing_policy`, lets the user toggle which
/// models each category may use and set a quality floor, edit the global
/// cost-aggressiveness, and Save.
pub struct RoutingMatrixView {
    theme: HiveTheme,

    /// Numeric input bound to `routing_policy.cost_aggressiveness` (0.0..=1.0).
    cost_input: Entity<InputState>,

    /// Per-category editing rows, one per [`CATEGORIES`] entry.
    rows: Vec<CategoryRow>,

    /// Available model ids (provider-grouped), honoring `project_models`.
    available_models: Vec<ModelChoice>,

    /// Escalation pool carried through unchanged from the loaded policy (not
    /// edited in this v1 panel, but preserved so Save never drops it).
    escalation_pool: Vec<String>,

    /// Transient status line shown after a Save.
    status: Option<String>,

    /// Focus handle so dispatched actions reach this view.
    focus_handle: FocusHandle,
}

/// A single selectable model in the allow-list chips.
#[derive(Clone)]
struct ModelChoice {
    /// The model id (persisted as an allow-list substring).
    id: String,
    /// Human-readable name for the chip label.
    name: String,
    /// Provider, used to group chips.
    provider: String,
}

impl EventEmitter<RoutingMatrixSaved> for RoutingMatrixView {}

impl RoutingMatrixView {
    /// Build the view, loading the current `routing_policy` from `AppConfig`.
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let cfg = if cx.has_global::<AppConfig>() {
            cx.global::<AppConfig>().0.get()
        } else {
            hive_core::HiveConfig::default()
        };

        let policy = cfg.routing_policy.clone();

        // Build the available-model list from the static registry, honoring the
        // curated project model set the same way the model selector does: when
        // `project_models` is non-empty, restrict to those ids.
        let project_models: Vec<String> = cfg.project_models.clone();
        let available_models = build_available_models(&project_models);

        // Pre-populate one row per category from the loaded policy.
        let rows = CATEGORIES
            .iter()
            .map(|task| {
                let key = task.to_string();
                let existing = policy.categories.iter().find(|c| c.category == key);
                CategoryRow {
                    task: *task,
                    allow: existing.map(|c| c.allow.clone()).unwrap_or_default(),
                    floor: existing.map(|c| c.floor.clone()).unwrap_or_default(),
                }
            })
            .collect();

        let cost_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_placeholder("0.0 - 1.0", window, cx);
            state.set_value(format!("{:.2}", policy.cost_aggressiveness), window, cx);
            state
        });

        let theme = if cx.has_global::<AppTheme>() {
            cx.global::<AppTheme>().0.clone()
        } else {
            HiveTheme::dark()
        };

        Self {
            theme,
            cost_input,
            rows,
            available_models,
            escalation_pool: policy.escalation_pool,
            status: None,
            focus_handle: cx.focus_handle(),
        }
    }

    /// Return the focus handle so the workspace can focus this view.
    pub fn focus_handle(&self) -> &FocusHandle {
        &self.focus_handle
    }

    /// Replace the cached theme and re-render.
    pub fn set_theme(&mut self, theme: HiveTheme, cx: &mut Context<Self>) {
        self.theme = theme;
        cx.notify();
    }

    /// Assemble the current UI state into a [`RoutingPolicy`].
    ///
    /// Only categories that actually constrain routing (non-empty allow-list or
    /// a floor) are persisted, keeping the stored policy minimal.
    pub fn collect_policy(&self, cx: &App) -> RoutingPolicy {
        let cost_aggressiveness = self
            .cost_input
            .read(cx)
            .value()
            .trim()
            .parse::<f32>()
            .unwrap_or(0.0)
            .clamp(0.0, 1.0);

        let categories = self
            .rows
            .iter()
            .filter(|row| !row.allow.is_empty() || !row.floor.is_empty())
            .map(|row| CategoryPolicy {
                category: row.task.to_string(),
                allow: row.allow.clone(),
                floor: row.floor.clone(),
            })
            .collect();

        RoutingPolicy {
            categories,
            cost_aggressiveness,
            escalation_pool: self.escalation_pool.clone(),
        }
    }

    /// Toggle a model id in a category's allow-list.
    fn toggle_allow(&mut self, row_index: usize, model_id: &str, cx: &mut Context<Self>) {
        if let Some(row) = self.rows.get_mut(row_index) {
            if let Some(pos) = row.allow.iter().position(|m| m == model_id) {
                row.allow.remove(pos);
            } else {
                row.allow.push(model_id.to_string());
            }
            self.status = None;
            cx.notify();
        }
    }

    /// Set the floor for a category.
    fn set_floor(&mut self, row_index: usize, floor: &str, cx: &mut Context<Self>) {
        if let Some(row) = self.rows.get_mut(row_index) {
            row.floor = floor.to_string();
            self.status = None;
            cx.notify();
        }
    }

    /// Snap the cost-aggressiveness input to a preset value.
    fn set_cost(&mut self, value: f32, window: &mut Window, cx: &mut Context<Self>) {
        self.cost_input.update(cx, |state, cx| {
            state.set_value(format!("{value:.2}"), window, cx);
        });
        self.status = None;
        cx.notify();
    }

    /// Emit the save event so the workspace persists and live-applies.
    fn save(&mut self, cx: &mut Context<Self>) {
        self.status = Some("Routing policy saved and applied.".to_string());
        cx.emit(RoutingMatrixSaved);
        cx.notify();
    }
}

// ---------------------------------------------------------------------------
// Available-model list
// ---------------------------------------------------------------------------

/// Build the selectable model list from the static registry.
///
/// When `project_models` is non-empty, only those ids are offered (matching the
/// model selector's curation behavior); otherwise the full registry is shown.
fn build_available_models(project_models: &[String]) -> Vec<ModelChoice> {
    MODEL_REGISTRY
        .iter()
        .filter(|m| project_models.is_empty() || project_models.iter().any(|p| p == &m.id))
        .map(|m| ModelChoice {
            id: m.id.clone(),
            name: m.name.clone(),
            provider: m.provider.clone(),
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Render
// ---------------------------------------------------------------------------

impl Render for RoutingMatrixView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.theme;
        let entity = cx.entity().clone();

        let cost_value = self
            .cost_input
            .read(cx)
            .value()
            .trim()
            .parse::<f32>()
            .unwrap_or(0.0)
            .clamp(0.0, 1.0);

        let mut root = div()
            .id("routing-matrix-scroll")
            .track_focus(&self.focus_handle)
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.0))
            .size_full()
            .p(theme.space_4)
            .gap(theme.space_4)
            .overflow_y_scroll()
            .child(header(theme))
            .child(cost_card(&entity, &self.cost_input, cost_value, theme));

        for (index, row) in self.rows.iter().enumerate() {
            root = root.child(category_card(
                &entity,
                index,
                row,
                &self.available_models,
                theme,
            ));
        }

        root = root.child(save_bar(&entity, self.status.as_deref(), theme));

        root
    }
}

// ---------------------------------------------------------------------------
// Header
// ---------------------------------------------------------------------------

fn header(theme: &HiveTheme) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap(theme.space_1)
        .child(
            div()
                .text_size(theme.font_size_2xl)
                .text_color(theme.text_primary)
                .font_weight(FontWeight::BOLD)
                .child("Routing Matrix"),
        )
        .child(
            div()
                .text_size(theme.font_size_sm)
                .text_color(theme.text_muted)
                .child(
                    "Pick which models each task category may use, set a quality floor, and a \
                     cost-vs-quality preference. Empty allow-list = all models.",
                ),
        )
}

// ---------------------------------------------------------------------------
// Cost-aggressiveness card
// ---------------------------------------------------------------------------

fn cost_card(
    entity: &Entity<RoutingMatrixView>,
    cost_input: &Entity<InputState>,
    cost_value: f32,
    theme: &HiveTheme,
) -> impl IntoElement {
    let presets = [("Quality", 0.0_f32), ("Balanced", 0.5), ("Thrift", 1.0)];

    let mut preset_row = div().flex().flex_row().flex_wrap().gap(theme.space_2);
    for (label, value) in presets {
        let active = (cost_value - value).abs() < 0.01;
        let entity = entity.clone();
        preset_row = preset_row.child(
            div()
                .id(SharedString::from(format!("cost-preset-{label}")))
                .cursor_pointer()
                .px(theme.space_3)
                .py(theme.space_2)
                .rounded(theme.radius_sm)
                .text_size(theme.font_size_sm)
                .text_color(if active {
                    theme.text_on_accent
                } else {
                    theme.text_primary
                })
                .bg(if active {
                    theme.accent_aqua
                } else {
                    theme.bg_tertiary
                })
                .hover(|s| s.opacity(0.9))
                .on_mouse_down(MouseButton::Left, move |_ev, window, cx| {
                    entity.update(cx, |this, cx| this.set_cost(value, window, cx));
                })
                .child(format!("{label} ({value:.1})")),
        );
    }

    card(theme)
        .child(section_title("Cost vs Quality", theme))
        .child(section_desc(
            "0.0 favors quality (best model); 1.0 favors thrift (cheapest acceptable model).",
            theme,
        ))
        .child(separator(theme))
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .gap(theme.space_4)
                .child(
                    div()
                        .text_size(theme.font_size_base)
                        .text_color(theme.text_secondary)
                        .child("Cost Aggressiveness"),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(theme.space_2)
                        .child(
                            div()
                                .min_w(px(90.0))
                                .child(Input::new(cost_input).appearance(true).cleanable(false)),
                        )
                        .child(
                            div()
                                .text_size(theme.font_size_sm)
                                .text_color(theme.accent_cyan)
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(format!("{cost_value:.2}")),
                        ),
                ),
        )
        .child(preset_row)
}

// ---------------------------------------------------------------------------
// Per-category card
// ---------------------------------------------------------------------------

fn category_card(
    entity: &Entity<RoutingMatrixView>,
    row_index: usize,
    row: &CategoryRow,
    available_models: &[ModelChoice],
    theme: &HiveTheme,
) -> impl IntoElement {
    let label = capitalize_words(&row.task.to_string());

    // Floor selector buttons.
    let mut floor_row = div()
        .flex()
        .flex_row()
        .flex_wrap()
        .items_center()
        .gap(theme.space_2);
    floor_row = floor_row.child(
        div()
            .text_size(theme.font_size_xs)
            .text_color(theme.text_muted)
            .child("Floor:"),
    );
    for (value, floor_label) in FLOORS {
        let active = row.floor == value;
        let entity = entity.clone();
        let value_owned = value.to_string();
        floor_row = floor_row.child(
            div()
                .id(SharedString::from(format!(
                    "floor-{row_index}-{floor_label}"
                )))
                .cursor_pointer()
                .px(theme.space_2)
                .py(theme.space_1)
                .rounded(theme.radius_sm)
                .text_size(theme.font_size_xs)
                .text_color(if active {
                    theme.text_on_accent
                } else {
                    theme.text_primary
                })
                .bg(if active {
                    theme.accent_aqua
                } else {
                    theme.bg_tertiary
                })
                .hover(|s| s.opacity(0.9))
                .on_mouse_down(MouseButton::Left, move |_ev, _window, cx| {
                    entity.update(cx, |this, cx| this.set_floor(row_index, &value_owned, cx));
                })
                .child(floor_label),
        );
    }

    // Allow-list model chips.
    let mut chips = div().flex().flex_row().flex_wrap().gap(theme.space_2);
    if available_models.is_empty() {
        chips = chips.child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .child("No models available."),
        );
    } else {
        for model in available_models {
            let active = row.allow.iter().any(|m| m == &model.id);
            let entity = entity.clone();
            let model_id = model.id.clone();
            chips = chips.child(
                div()
                    .id(SharedString::from(format!(
                        "allow-{row_index}-{}",
                        model.id
                    )))
                    .cursor_pointer()
                    .px(theme.space_2)
                    .py(theme.space_1)
                    .rounded(theme.radius_sm)
                    .border_1()
                    .border_color(if active {
                        theme.accent_cyan
                    } else {
                        theme.border
                    })
                    .text_size(theme.font_size_xs)
                    .text_color(if active {
                        theme.accent_cyan
                    } else {
                        theme.text_secondary
                    })
                    .bg(if active {
                        theme.bg_tertiary
                    } else {
                        theme.bg_primary
                    })
                    .hover(|s| s.bg(theme.bg_tertiary))
                    .on_mouse_down(MouseButton::Left, move |_ev, _window, cx| {
                        entity.update(cx, |this, cx| this.toggle_allow(row_index, &model_id, cx));
                    })
                    .child(format!("{} · {}", model.provider, model.name)),
            );
        }
    }

    let allow_summary = if row.allow.is_empty() {
        "All models allowed".to_string()
    } else {
        format!("{} model(s) allowed", row.allow.len())
    };

    card(theme)
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .gap(theme.space_2)
                .child(section_title(&label, theme))
                .child(
                    div()
                        .text_size(theme.font_size_xs)
                        .text_color(theme.text_muted)
                        .child(allow_summary),
                ),
        )
        .child(floor_row)
        .child(separator(theme))
        .child(chips)
}

// ---------------------------------------------------------------------------
// Save bar
// ---------------------------------------------------------------------------

fn save_bar(
    entity: &Entity<RoutingMatrixView>,
    status: Option<&str>,
    theme: &HiveTheme,
) -> impl IntoElement {
    let entity = entity.clone();

    let mut bar = div()
        .flex()
        .flex_row()
        .items_center()
        .gap(theme.space_3)
        .child(
            div()
                .id("routing-matrix-save")
                .cursor_pointer()
                .px(theme.space_4)
                .py(theme.space_2)
                .rounded(theme.radius_sm)
                .bg(theme.accent_cyan)
                .text_size(theme.font_size_sm)
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.text_on_accent)
                .hover(|s| s.opacity(0.9))
                .on_mouse_down(MouseButton::Left, move |_ev, _window, cx| {
                    entity.update(cx, |this, cx| this.save(cx));
                })
                .child("Save Routing Policy"),
        );

    if let Some(status) = status {
        bar = bar.child(
            div()
                .text_size(theme.font_size_sm)
                .text_color(theme.accent_green)
                .child(status.to_string()),
        );
    }

    bar
}

// ---------------------------------------------------------------------------
// Shared card helpers (mirrors settings.rs / routing.rs styling)
// ---------------------------------------------------------------------------

fn card(theme: &HiveTheme) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(theme.space_2)
        .p(theme.space_4)
        .rounded(theme.radius_md)
        .bg(theme.bg_surface)
        .border_1()
        .border_color(theme.border)
}

fn section_title(text: &str, theme: &HiveTheme) -> impl IntoElement {
    div()
        .text_size(theme.font_size_lg)
        .text_color(theme.text_primary)
        .font_weight(FontWeight::SEMIBOLD)
        .child(text.to_string())
}

fn section_desc(text: &str, theme: &HiveTheme) -> impl IntoElement {
    div()
        .text_size(theme.font_size_sm)
        .text_color(theme.text_muted)
        .child(text.to_string())
}

fn separator(theme: &HiveTheme) -> impl IntoElement {
    div().w_full().h(px(1.0)).bg(theme.border)
}

/// Title-case each whitespace-separated word in a category display string,
/// e.g. "creative writing" -> "Creative Writing".
fn capitalize_words(s: &str) -> String {
    s.split(' ')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
