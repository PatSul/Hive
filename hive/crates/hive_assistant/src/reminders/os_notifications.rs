/// Show a native OS toast notification.
///
/// On Windows, uses `winrt-notification` to display a toast in the
/// notification center. On Linux and macOS, uses `notify-rust` (which
/// talks to D-Bus/freedesktop on Linux and the macOS notification API).
pub fn show_toast(title: &str, body: &str) {
    #[cfg(windows)]
    {
        show_toast_windows(title, body);
    }
    #[cfg(not(windows))]
    {
        use notify_rust::Notification;
        if let Err(e) = Notification::new()
            .summary(title)
            .body(body)
            .appname("Hive")
            .show()
        {
            tracing::warn!("Failed to show notification: {e}");
        }
    }
}

#[cfg(windows)]
fn show_toast_windows(title: &str, body: &str) {
    use winrt_notification::{Duration, Toast};

    let result = Toast::new(Toast::POWERSHELL_APP_ID)
        .title(title)
        .text1(body)
        .duration(Duration::Short)
        .show();

    if let Err(e) = result {
        tracing::warn!("Failed to show Windows toast notification: {e}");
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use crate::reminders::os_notifications::show_toast;

    #[test]
    fn test_show_toast_does_not_panic() {
        // This test just verifies the function can be called without panicking.
        // On CI or headless environments, the toast may silently fail, which is fine.
        show_toast("Test Title", "Test Body");
    }

    #[test]
    fn test_show_toast_empty_strings() {
        show_toast("", "");
    }

    #[test]
    fn test_show_toast_unicode() {
        show_toast("Reminder", "Meeting at 3pm ");
    }
}
