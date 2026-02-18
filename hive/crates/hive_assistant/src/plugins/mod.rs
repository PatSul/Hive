//! Built-in assistant plugin implementations.
//!
//! Each plugin implements [`AssistantPlugin`] and provides a focused set of
//! capabilities.  The [`PluginRegistry`] discovers, initializes, and shuts
//! down all registered plugins.

mod reminder_plugin;

pub use reminder_plugin::ReminderPlugin;

use std::collections::HashMap;

use crate::plugin::{AssistantCapability, AssistantPlugin};

// ---------------------------------------------------------------------------
// PluginRegistry
// ---------------------------------------------------------------------------

/// Registry that manages assistant plugin lifecycle.
///
/// Plugins are registered by name.  `initialize_all` and `shutdown_all` drive
/// the lifecycle in bulk; individual plugins can be queried by name or by
/// capability.
pub struct PluginRegistry {
    plugins: Vec<Box<dyn AssistantPlugin>>,
}

impl PluginRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            plugins: Vec::new(),
        }
    }

    /// Create a registry pre-loaded with all built-in plugins.
    pub fn with_defaults() -> Self {
        let mut registry = Self::new();
        registry.register(Box::new(ReminderPlugin::new()));
        registry
    }

    /// Register a plugin.
    pub fn register(&mut self, plugin: Box<dyn AssistantPlugin>) {
        self.plugins.push(plugin);
    }

    /// Number of registered plugins.
    pub fn len(&self) -> usize {
        self.plugins.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty()
    }

    /// Return the names of all registered plugins.
    pub fn plugin_names(&self) -> Vec<&str> {
        self.plugins.iter().map(|p| p.name()).collect()
    }

    /// Return a map of capability -> list of plugin names that provide it.
    pub fn capability_map(&self) -> HashMap<AssistantCapability, Vec<&str>> {
        let mut map: HashMap<AssistantCapability, Vec<&str>> = HashMap::new();
        for plugin in &self.plugins {
            for cap in plugin.capabilities() {
                map.entry(cap).or_default().push(plugin.name());
            }
        }
        map
    }

    /// Find all plugins that provide a given capability.
    pub fn plugins_for(&self, cap: &AssistantCapability) -> Vec<&str> {
        self.plugins
            .iter()
            .filter(|p| p.capabilities().contains(cap))
            .map(|p| p.name())
            .collect()
    }

    /// Initialize all registered plugins. Returns errors keyed by plugin name.
    pub async fn initialize_all(&mut self) -> HashMap<String, Result<(), String>> {
        let mut results = HashMap::new();
        for plugin in &mut self.plugins {
            let name = plugin.name().to_string();
            let result = plugin.initialize().await;
            results.insert(name, result);
        }
        results
    }

    /// Shut down all registered plugins. Returns errors keyed by plugin name.
    pub async fn shutdown_all(&mut self) -> HashMap<String, Result<(), String>> {
        let mut results = HashMap::new();
        for plugin in &mut self.plugins {
            let name = plugin.name().to_string();
            let result = plugin.shutdown().await;
            results.insert(name, result);
        }
        results
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::AssistantCapability;

    #[test]
    fn test_empty_registry() {
        let registry = PluginRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
        assert!(registry.plugin_names().is_empty());
    }

    #[test]
    fn test_with_defaults() {
        let registry = PluginRegistry::with_defaults();
        assert!(!registry.is_empty());
        assert!(registry.plugin_names().contains(&"reminders"));
    }

    #[test]
    fn test_capability_map() {
        let registry = PluginRegistry::with_defaults();
        let map = registry.capability_map();
        assert!(map.contains_key(&AssistantCapability::Reminders));
        let reminder_providers = &map[&AssistantCapability::Reminders];
        assert!(reminder_providers.contains(&"reminders"));
    }

    #[test]
    fn test_plugins_for_capability() {
        let registry = PluginRegistry::with_defaults();
        let names = registry.plugins_for(&AssistantCapability::Reminders);
        assert!(names.contains(&"reminders"));

        // No plugins provide Email by default.
        let email_names = registry.plugins_for(&AssistantCapability::Email);
        assert!(email_names.is_empty());
    }

    #[tokio::test]
    async fn test_initialize_all() {
        let mut registry = PluginRegistry::with_defaults();
        let results = registry.initialize_all().await;
        for (name, result) in &results {
            assert!(result.is_ok(), "Plugin {name} failed to initialize: {result:?}");
        }
    }

    #[tokio::test]
    async fn test_shutdown_all() {
        let mut registry = PluginRegistry::with_defaults();
        registry.initialize_all().await;
        let results = registry.shutdown_all().await;
        for (name, result) in &results {
            assert!(result.is_ok(), "Plugin {name} failed to shut down: {result:?}");
        }
    }
}
