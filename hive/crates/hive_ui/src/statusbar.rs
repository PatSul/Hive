use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::{Icon, IconName};

use hive_ui_core::{HiveTheme, SwitchToSettings, TriggerAppUpdate};
use hive_ui_panels::components::notification_tray::{NotificationTray, NotificationTrayData};

/// Status bar at the bottom of the window.
/// Shows connectivity, model, privacy mode, project scope, cost, and version.
pub struct StatusBar {
    pub connectivity: ConnectivityDisplay,
    pub current_model: String,
    pub cortex_state: String,
    pub cortex_changes_applied: u32,
    pub cortex_auto_apply_enabled: bool,
    pub privacy_mode: bool,
    pub active_project: String,
    pub total_cost: f64,
    pub version: String,
    /// If set, a newer version is available for download/install.
    pub update_available: Option<String>,
    /// Notification tray state for agent/system notifications.
    pub notification_tray: NotificationTrayData,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectivityDisplay {
    Online,
    LocalOnly,
    Offline,
}

impl ConnectivityDisplay {
    fn label(self) -> &'static str {
        match self {
            Self::Online => "Online",
            Self::LocalOnly => "Local Only",
            Self::Offline => "Offline",
        }
    }

    fn color(self, theme: &HiveTheme) -> Hsla {
        match self {
            Self::Online => theme.accent_green,
            Self::LocalOnly => theme.accent_yellow,
            Self::Offline => theme.accent_red,
        }
    }
}

impl Default for StatusBar {
    fn default() -> Self {
        Self {
            connectivity: ConnectivityDisplay::Offline,
            current_model: "Select Model".into(),
            cortex_state: "idle".into(),
            cortex_changes_applied: 0,
            cortex_auto_apply_enabled: true,
            privacy_mode: false,
            active_project: "No project".into(),
            total_cost: 0.0,
            version: env!("CARGO_PKG_VERSION").into(),
            update_available: None,
            notification_tray: NotificationTrayData::default(),
        }
    }
}

impl StatusBar {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn render(&self, theme: &HiveTheme) -> impl IntoElement {
        let conn_color = self.connectivity.color(theme);
        let conn_label = self.connectivity.label();
        let model = if self.current_model.trim().is_empty()
            || self.current_model == "(no model)"
            || self.current_model == "Select Model"
        {
            "Select Model".to_string()
        } else {
            self.current_model.clone()
        };
        let cortex_state = self.cortex_state.trim();
        let cortex_label = if !self.cortex_auto_apply_enabled {
            "Cortex: paused".to_string()
        } else if cortex_state.is_empty() {
            "Cortex: idle".to_string()
        } else if self.cortex_changes_applied > 0 {
            format!("Cortex: {cortex_state} ({})", self.cortex_changes_applied)
        } else {
            format!("Cortex: {cortex_state}")
        };
        let cortex_color = if !self.cortex_auto_apply_enabled {
            theme.accent_yellow
        } else {
            match cortex_state {
                "processing" => theme.accent_yellow,
                "applied" => theme.accent_green,
                "paused" => theme.accent_yellow,
                _ => theme.text_secondary,
            }
        };
        let cost_str = format!("${:.2}", self.total_cost);
        let privacy = if self.privacy_mode {
            "Private Mode"
        } else {
            "Cloud Mode"
        };
        let privacy_icon = if self.privacy_mode {
            IconName::EyeOff
        } else {
            IconName::Eye
        };
        let project = self.active_project.clone();
        let version = format!("v{}", self.version);
        let update_version = self.update_available.clone();

        div()
            .flex()
            .items_center()
            .justify_between()
            .w_full()
            .h(px(32.0))
            .bg(theme.bg_secondary)
            .border_t_1()
            .border_color(theme.border)
            .px(theme.space_2)
            .text_color(theme.text_muted)
            .child(
                // Left summary area
                div()
                    .flex()
                    .items_center()
                    .gap(theme.space_2)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(theme.space_1)
                            .px(theme.space_2)
                            .py(px(2.0))
                            .rounded(theme.radius_sm)
                            .bg(theme.bg_surface)
                            .child(
                                div()
                                    .w(px(8.0))
                                    .h(px(8.0))
                                    .rounded(theme.radius_full)
                                    .bg(conn_color),
                            )
                            .child(
                                div()
                                    .text_size(theme.font_size_xs)
                                    .text_color(theme.text_secondary)
                                    .child(conn_label),
                            ),
                    )
                    .child(
                        div()
                            .id("status-model")
                            .px(theme.space_2)
                            .py(px(2.0))
                            .rounded(theme.radius_sm)
                            .bg(theme.bg_tertiary)
                            .text_color(theme.accent_cyan)
                            .text_size(theme.font_size_xs)
                            .font_weight(FontWeight::SEMIBOLD)
                            .cursor_pointer()
                            .on_mouse_down(MouseButton::Left, |_event, window, cx| {
                                window.dispatch_action(Box::new(SwitchToSettings), cx);
                            })
                            .child(model),
                    )
                    .child(
                        div()
                            .px(theme.space_2)
                            .py(px(2.0))
                            .rounded(theme.radius_sm)
                            .bg(theme.bg_surface)
                            .text_size(theme.font_size_xs)
                            .text_color(theme.text_muted)
                            .overflow_hidden()
                            .max_w(px(230.0))
                            .child(format!("Project: {project}")),
                    )
                    .child(
                        div()
                            .px(theme.space_2)
                            .py(px(2.0))
                            .rounded(theme.radius_sm)
                            .bg(theme.bg_surface)
                            .text_size(theme.font_size_xs)
                            .text_color(cortex_color)
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(cortex_label),
                    ),
            )
            .child(
                // Right controls
                div()
                    .flex()
                    .items_center()
                    .gap(theme.space_2)
                    // Notification tray bell
                    .child(NotificationTray::render(&self.notification_tray, theme))
                    // Update badge (only visible when update is available)
                    .when(update_version.is_some(), |el: Div| {
                        let new_ver = update_version.unwrap_or_default();
                        el.child(
                            div()
                                .id("update-badge")
                                .flex()
                                .items_center()
                                .gap(theme.space_1)
                                .px(theme.space_2)
                                .py(px(2.0))
                                .rounded(theme.radius_sm)
                                .bg(theme.accent_yellow)
                                .text_color(hsla(0.0, 0.0, 0.1, 1.0))
                                .text_size(theme.font_size_xs)
                                .font_weight(FontWeight::BOLD)
                                .cursor_pointer()
                                .on_mouse_down(MouseButton::Left, |_event, window, cx| {
                                    window.dispatch_action(Box::new(TriggerAppUpdate), cx);
                                })
                                .child(Icon::new(IconName::ArrowUp).size_3p5())
                                .child(format!("Update v{new_ver}")),
                        )
                    })
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(theme.space_1)
                            .px(theme.space_2)
                            .py(px(2.0))
                            .rounded(theme.radius_sm)
                            .bg(theme.bg_surface)
                            .text_size(theme.font_size_xs)
                            .child(Icon::new(privacy_icon).size_3p5())
                            .child(div().text_size(theme.font_size_xs).child(privacy)),
                    )
                    .child(
                        div()
                            .px(theme.space_2)
                            .py(px(2.0))
                            .rounded(theme.radius_sm)
                            .bg(theme.bg_surface)
                            .text_color(theme.accent_green)
                            .text_size(theme.font_size_xs)
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(cost_str),
                    )
                    .child(
                        div()
                            .px(theme.space_2)
                            .py(px(2.0))
                            .rounded(theme.radius_sm)
                            .bg(theme.bg_surface)
                            .text_size(theme.font_size_xs)
                            .text_color(theme.text_secondary)
                            .font_weight(FontWeight::MEDIUM)
                            .child(version),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn theme() -> HiveTheme {
        HiveTheme::dark()
    }

    // ---- ConnectivityDisplay::label ----

    #[test]
    fn online_label() {
        assert_eq!(ConnectivityDisplay::Online.label(), "Online");
    }

    #[test]
    fn local_only_label() {
        assert_eq!(ConnectivityDisplay::LocalOnly.label(), "Local Only");
    }

    #[test]
    fn offline_label() {
        assert_eq!(ConnectivityDisplay::Offline.label(), "Offline");
    }

    // ---- ConnectivityDisplay::color ----

    #[test]
    fn online_color_is_green() {
        let t = theme();
        assert_eq!(ConnectivityDisplay::Online.color(&t), t.accent_green);
    }

    #[test]
    fn local_only_color_is_yellow() {
        let t = theme();
        assert_eq!(ConnectivityDisplay::LocalOnly.color(&t), t.accent_yellow);
    }

    #[test]
    fn offline_color_is_red() {
        let t = theme();
        assert_eq!(ConnectivityDisplay::Offline.color(&t), t.accent_red);
    }

    // ---- StatusBar field mutations ----

    #[test]
    fn statusbar_default_connectivity_is_offline() {
        let bar = StatusBar::new();
        assert_eq!(bar.connectivity, ConnectivityDisplay::Offline);
    }

    #[test]
    fn statusbar_default_model_is_select_model() {
        let bar = StatusBar::new();
        assert_eq!(bar.current_model, "Select Model");
    }

    #[test]
    fn statusbar_default_cortex_state_is_idle() {
        let bar = StatusBar::new();
        assert_eq!(bar.cortex_state, "idle");
        assert_eq!(bar.cortex_changes_applied, 0);
        assert!(bar.cortex_auto_apply_enabled);
    }
}
