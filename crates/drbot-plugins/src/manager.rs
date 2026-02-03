//! Plugin manager for loading and managing plugins.

use crate::types::{BoxedPlugin, PluginContext, PluginEvent, PluginResponse, PluginState};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Entry for a loaded plugin.
struct PluginEntry {
    plugin: BoxedPlugin,
    context: PluginContext,
    state: PluginState,
}

/// Plugin manager for loading and executing plugins.
pub struct PluginManager {
    /// Loaded plugins.
    plugins: Arc<RwLock<HashMap<String, PluginEntry>>>,
    /// Plugin configurations.
    configs: Arc<RwLock<HashMap<String, serde_json::Value>>>,
}

impl PluginManager {
    /// Create a new plugin manager.
    pub fn new() -> Self {
        Self {
            plugins: Arc::new(RwLock::new(HashMap::new())),
            configs: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Set plugin configuration.
    pub async fn set_config(&self, plugin_name: &str, config: serde_json::Value) {
        let mut configs = self.configs.write().await;
        configs.insert(plugin_name.to_string(), config);
    }

    /// Load a plugin.
    pub async fn load(&self, plugin: BoxedPlugin) -> drbot_core::Result<()> {
        let name = plugin.metadata().name.clone();

        let mut plugins = self.plugins.write().await;

        if plugins.contains_key(&name) {
            return Err(drbot_core::Error::InvalidInput(format!(
                "Plugin '{}' is already loaded",
                name
            )));
        }

        // Get configuration if available
        let configs = self.configs.read().await;
        let config = configs
            .get(&name)
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        drop(configs);

        let context = PluginContext::new(&name).with_config(config);

        let entry = PluginEntry {
            plugin,
            context,
            state: PluginState::Loaded,
        };

        plugins.insert(name.clone(), entry);
        info!(plugin = %name, "Plugin loaded");

        Ok(())
    }

    /// Unload a plugin.
    pub async fn unload(&self, name: &str) -> drbot_core::Result<()> {
        let mut plugins = self.plugins.write().await;

        if let Some(mut entry) = plugins.remove(name) {
            // Stop if running
            if entry.state == PluginState::Running {
                if let Err(e) = entry.plugin.stop(&entry.context).await {
                    warn!(plugin = %name, error = %e, "Error stopping plugin during unload");
                }
            }
            info!(plugin = %name, "Plugin unloaded");
            Ok(())
        } else {
            Err(drbot_core::Error::NotFound(format!(
                "Plugin '{}' not found",
                name
            )))
        }
    }

    /// Initialize a plugin.
    pub async fn init(&self, name: &str) -> drbot_core::Result<()> {
        let mut plugins = self.plugins.write().await;

        let entry = plugins
            .get_mut(name)
            .ok_or_else(|| drbot_core::Error::NotFound(format!("Plugin '{}' not found", name)))?;

        entry.plugin.init(&entry.context).await?;
        info!(plugin = %name, "Plugin initialized");

        Ok(())
    }

    /// Start a plugin.
    pub async fn start(&self, name: &str) -> drbot_core::Result<()> {
        let mut plugins = self.plugins.write().await;

        let entry = plugins
            .get_mut(name)
            .ok_or_else(|| drbot_core::Error::NotFound(format!("Plugin '{}' not found", name)))?;

        if entry.state == PluginState::Running {
            return Err(drbot_core::Error::InvalidInput(format!(
                "Plugin '{}' is already running",
                name
            )));
        }

        entry.plugin.start(&entry.context).await?;
        entry.state = PluginState::Running;
        info!(plugin = %name, "Plugin started");

        Ok(())
    }

    /// Stop a plugin.
    pub async fn stop(&self, name: &str) -> drbot_core::Result<()> {
        let mut plugins = self.plugins.write().await;

        let entry = plugins
            .get_mut(name)
            .ok_or_else(|| drbot_core::Error::NotFound(format!("Plugin '{}' not found", name)))?;

        if entry.state != PluginState::Running {
            return Err(drbot_core::Error::InvalidInput(format!(
                "Plugin '{}' is not running",
                name
            )));
        }

        entry.plugin.stop(&entry.context).await?;
        entry.state = PluginState::Stopped;
        info!(plugin = %name, "Plugin stopped");

        Ok(())
    }

    /// Start all loaded plugins.
    pub async fn start_all(&self) -> Vec<(String, drbot_core::Result<()>)> {
        let plugins = self.plugins.read().await;
        let names: Vec<String> = plugins.keys().cloned().collect();
        drop(plugins);

        let mut results = Vec::new();
        for name in names {
            let result = self.start(&name).await;
            results.push((name, result));
        }
        results
    }

    /// Stop all running plugins.
    pub async fn stop_all(&self) -> Vec<(String, drbot_core::Result<()>)> {
        let plugins = self.plugins.read().await;
        let names: Vec<String> = plugins
            .iter()
            .filter(|(_, e)| e.state == PluginState::Running)
            .map(|(name, _)| name.clone())
            .collect();
        drop(plugins);

        let mut results = Vec::new();
        for name in names {
            let result = self.stop(&name).await;
            results.push((name, result));
        }
        results
    }

    /// Dispatch an event to all running plugins.
    pub async fn dispatch(&self, event: &PluginEvent) -> Vec<PluginResponse> {
        let plugins = self.plugins.read().await;
        let mut responses = Vec::new();

        for (name, entry) in plugins.iter() {
            if entry.state != PluginState::Running {
                continue;
            }

            debug!(plugin = %name, event = ?event, "Dispatching event to plugin");

            match entry.plugin.handle_event(event, &entry.context).await {
                Ok(response) => {
                    if response.handled {
                        debug!(plugin = %name, "Plugin handled event");
                    }
                    responses.push(response);
                }
                Err(e) => {
                    error!(plugin = %name, error = %e, "Plugin error handling event");
                }
            }
        }

        responses
    }

    /// Get plugin state.
    pub async fn get_state(&self, name: &str) -> Option<PluginState> {
        let plugins = self.plugins.read().await;
        plugins.get(name).map(|e| e.state)
    }

    /// List all loaded plugins.
    pub async fn list(&self) -> Vec<String> {
        let plugins = self.plugins.read().await;
        plugins.keys().cloned().collect()
    }

    /// Check if a plugin is loaded.
    pub async fn is_loaded(&self, name: &str) -> bool {
        let plugins = self.plugins.read().await;
        plugins.contains_key(name)
    }

    /// Get plugin metadata.
    pub async fn get_metadata(&self, name: &str) -> Option<crate::types::PluginMetadata> {
        let plugins = self.plugins.read().await;
        plugins.get(name).map(|e| e.plugin.metadata().clone())
    }
}

impl Default for PluginManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Plugin, PluginMetadata};
    use async_trait::async_trait;

    struct TestPlugin {
        metadata: PluginMetadata,
        init_called: std::sync::atomic::AtomicBool,
        start_called: std::sync::atomic::AtomicBool,
        stop_called: std::sync::atomic::AtomicBool,
    }

    impl TestPlugin {
        fn new(name: &str) -> Self {
            Self {
                metadata: PluginMetadata::new(name, "1.0.0"),
                init_called: std::sync::atomic::AtomicBool::new(false),
                start_called: std::sync::atomic::AtomicBool::new(false),
                stop_called: std::sync::atomic::AtomicBool::new(false),
            }
        }
    }

    #[async_trait]
    impl Plugin for TestPlugin {
        fn metadata(&self) -> &PluginMetadata {
            &self.metadata
        }

        async fn init(&mut self, _context: &PluginContext) -> drbot_core::Result<()> {
            self.init_called
                .store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        }

        async fn start(&mut self, _context: &PluginContext) -> drbot_core::Result<()> {
            self.start_called
                .store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        }

        async fn stop(&mut self, _context: &PluginContext) -> drbot_core::Result<()> {
            self.stop_called
                .store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        }

        async fn handle_event(
            &self,
            event: &PluginEvent,
            _context: &PluginContext,
        ) -> drbot_core::Result<PluginResponse> {
            match event {
                PluginEvent::Message { content, .. } => {
                    if content.starts_with("!test") {
                        Ok(PluginResponse::with_message("Test response"))
                    } else {
                        Ok(PluginResponse::unhandled())
                    }
                }
                _ => Ok(PluginResponse::unhandled()),
            }
        }
    }

    #[tokio::test]
    async fn test_load_plugin() {
        let manager = PluginManager::new();
        let plugin = Box::new(TestPlugin::new("test-plugin"));

        manager.load(plugin).await.unwrap();

        assert!(manager.is_loaded("test-plugin").await);
        assert_eq!(manager.list().await, vec!["test-plugin"]);
    }

    #[tokio::test]
    async fn test_unload_plugin() {
        let manager = PluginManager::new();
        let plugin = Box::new(TestPlugin::new("test-plugin"));

        manager.load(plugin).await.unwrap();
        manager.unload("test-plugin").await.unwrap();

        assert!(!manager.is_loaded("test-plugin").await);
    }

    #[tokio::test]
    async fn test_start_stop_plugin() {
        let manager = PluginManager::new();
        let plugin = Box::new(TestPlugin::new("test-plugin"));

        manager.load(plugin).await.unwrap();

        assert_eq!(
            manager.get_state("test-plugin").await,
            Some(PluginState::Loaded)
        );

        manager.start("test-plugin").await.unwrap();
        assert_eq!(
            manager.get_state("test-plugin").await,
            Some(PluginState::Running)
        );

        manager.stop("test-plugin").await.unwrap();
        assert_eq!(
            manager.get_state("test-plugin").await,
            Some(PluginState::Stopped)
        );
    }

    #[tokio::test]
    async fn test_dispatch_event() {
        let manager = PluginManager::new();
        let plugin = Box::new(TestPlugin::new("test-plugin"));

        manager.load(plugin).await.unwrap();
        manager.start("test-plugin").await.unwrap();

        let event = PluginEvent::Message {
            session_id: "session-1".to_string(),
            user_id: "user-1".to_string(),
            content: "!test hello".to_string(),
        };

        let responses = manager.dispatch(&event).await;
        assert_eq!(responses.len(), 1);
        assert!(responses[0].handled);
        assert_eq!(responses[0].message, Some("Test response".to_string()));
    }

    #[tokio::test]
    async fn test_get_metadata() {
        let manager = PluginManager::new();
        let plugin = Box::new(TestPlugin::new("test-plugin"));

        manager.load(plugin).await.unwrap();

        let meta = manager.get_metadata("test-plugin").await.unwrap();
        assert_eq!(meta.name, "test-plugin");
        assert_eq!(meta.version, "1.0.0");
    }
}
