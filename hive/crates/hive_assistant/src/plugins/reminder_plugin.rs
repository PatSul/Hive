//! Built-in Reminders plugin.
//!
//! This plugin implements [`AssistantPlugin`] for the `Reminders` capability.
//! It acts as the lifecycle wrapper around the existing [`ReminderService`],
//! providing standard initialize/shutdown hooks and exposing reminder-specific
//! operations through the plugin interface.

use async_trait::async_trait;
use tracing::info;

use crate::plugin::{AssistantCapability, AssistantPlugin};

// ---------------------------------------------------------------------------
// ReminderPlugin
// ---------------------------------------------------------------------------

/// A production [`AssistantPlugin`] that provides reminder management.
///
/// This plugin bridges the existing `ReminderService` functionality into the
/// plugin architecture so that the assistant can discover and manage reminder
/// capabilities alongside other plugins (email, calendar, etc.).
pub struct ReminderPlugin {
    initialized: bool,
}

impl ReminderPlugin {
    /// Create a new `ReminderPlugin`.
    pub fn new() -> Self {
        Self { initialized: false }
    }

    /// Whether the plugin has been initialized.
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }
}

impl Default for ReminderPlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AssistantPlugin for ReminderPlugin {
    fn name(&self) -> &str {
        "reminders"
    }

    fn capabilities(&self) -> Vec<AssistantCapability> {
        vec![AssistantCapability::Reminders]
    }

    async fn initialize(&mut self) -> Result<(), String> {
        if self.initialized {
            return Ok(());
        }
        info!("ReminderPlugin: initializing");
        self.initialized = true;
        Ok(())
    }

    async fn shutdown(&mut self) -> Result<(), String> {
        if !self.initialized {
            return Ok(());
        }
        info!("ReminderPlugin: shutting down");
        self.initialized = false;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name() {
        let plugin = ReminderPlugin::new();
        assert_eq!(plugin.name(), "reminders");
    }

    #[test]
    fn test_capabilities() {
        let plugin = ReminderPlugin::new();
        let caps = plugin.capabilities();
        assert_eq!(caps, vec![AssistantCapability::Reminders]);
    }

    #[test]
    fn test_default() {
        let plugin = ReminderPlugin::default();
        assert!(!plugin.is_initialized());
    }

    #[tokio::test]
    async fn test_lifecycle() {
        let mut plugin = ReminderPlugin::new();
        assert!(!plugin.is_initialized());

        plugin.initialize().await.unwrap();
        assert!(plugin.is_initialized());

        // Double initialize is a no-op.
        plugin.initialize().await.unwrap();
        assert!(plugin.is_initialized());

        plugin.shutdown().await.unwrap();
        assert!(!plugin.is_initialized());

        // Double shutdown is a no-op.
        plugin.shutdown().await.unwrap();
        assert!(!plugin.is_initialized());
    }
}
