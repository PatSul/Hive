use gpui::prelude::*;
use gpui_component::IconName;
use hive_agents::activity::notification::Notification;
use hive_ui_core::HiveTheme;

/// Data for the notification tray display.
pub struct NotificationTrayData {
    pub notifications: Vec<Notification>,
    pub unread_count: usize,
    pub expanded: bool,
}

impl Default for NotificationTrayData {
    fn default() -> Self {
        Self {
            notifications: Vec::new(),
            unread_count: 0,
            expanded: false,
        }
    }
}

/// Notification tray rendered in the statusbar.
pub struct NotificationTray;

impl NotificationTray {
    pub fn render(data: &NotificationTrayData, theme: &HiveTheme) -> impl IntoElement {
        use gpui::div;

        let mut bell = div()
            .id("notification-bell")
            .flex()
            .items_center()
            .gap(theme.space_1)
            .px(theme.space_2)
            .py(theme.space_1)
            .rounded(theme.radius_md)
            .cursor_pointer()
            .hover(|s| s.bg(theme.bg_surface))
            .child(
                gpui_component::Icon::new(IconName::Bell)
                    .size_4()
                    .text_color(if data.unread_count > 0 {
                        theme.accent_cyan
                    } else {
                        theme.text_muted
                    }),
            );

        if data.unread_count > 0 {
            bell = bell.child(
                div()
                    .px(theme.space_1)
                    .rounded(theme.radius_full)
                    .bg(theme.accent_cyan)
                    .text_size(theme.font_size_xs)
                    .text_color(theme.text_on_accent)
                    .child(format!("{}", data.unread_count)),
            );
        }

        bell
    }
}
