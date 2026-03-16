use gpui::prelude::*;
use hive_ui_core::HiveTheme;

/// Data for the budget gauge.
pub struct BudgetGaugeData {
    pub current_usd: f64,
    pub limit_usd: Option<f64>,
    pub warning_pct: f64,
}

impl Default for BudgetGaugeData {
    fn default() -> Self {
        Self {
            current_usd: 0.0,
            limit_usd: None,
            warning_pct: 0.8,
        }
    }
}

/// Horizontal budget gauge bar.
pub struct BudgetGauge;

impl BudgetGauge {
    pub fn render(data: &BudgetGaugeData, theme: &HiveTheme) -> impl IntoElement {
        use gpui::{div, rgb, relative};

        let limit = data.limit_usd.unwrap_or(0.0);
        if limit <= 0.0 {
            return div().id("budget-gauge-empty");
        }

        let pct = (data.current_usd / limit).min(1.0);
        let bar_color = if pct >= 0.95 {
            rgb(0xEF4444) // red
        } else if pct >= data.warning_pct {
            rgb(0xF59E0B) // amber
        } else {
            theme.accent_cyan
        };

        div()
            .id("budget-gauge")
            .flex()
            .flex_col()
            .gap(theme.space_1)
            .px(theme.space_4)
            .py(theme.space_2)
            .child(
                div()
                    .flex()
                    .justify_between()
                    .text_size(theme.font_size_xs)
                    .child(
                        div()
                            .text_color(theme.text_primary)
                            .child(format!("${:.2} / ${:.2}", data.current_usd, limit)),
                    )
                    .child(
                        div()
                            .text_color(theme.text_muted)
                            .child(format!("{:.0}%", pct * 100.0)),
                    ),
            )
            .child(
                div()
                    .h(gpui::px(8.0))
                    .w_full()
                    .rounded(theme.radius_full)
                    .bg(theme.bg_surface)
                    .child(
                        div()
                            .h_full()
                            .rounded(theme.radius_full)
                            .bg(bar_color)
                            .w(relative(pct as f32)),
                    ),
            )
    }
}
