//! Plugin types and definitions.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Plugin metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginMetadata {
    /// Plugin name (unique identifier).
    pub name: String,
    /// Plugin version.
    pub version: String,
    /// Plugin description.
    pub description: String,
    /// Plugin author.
    pub author: Option<String>,
    /// Plugin homepage/repository.
    pub homepage: Option<String>,
    /// Required drbot version.
    pub drbot_version: Option<String>,
    /// Plugin capabilities.
    pub capabilities: Vec<PluginCapability>,
}

impl PluginMetadata {
    /// Create new plugin metadata.
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            description: String::new(),
            author: None,
            homepage: None,
            drbot_version: None,
            capabilities: vec![],
        }
    }

    /// Set description.
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    /// Set author.
    pub fn with_author(mut self, author: impl Into<String>) -> Self {
        self.author = Some(author.into());
        self
    }

    /// Add a capability.
    pub fn with_capability(mut self, cap: PluginCapability) -> Self {
        self.capabilities.push(cap);
        self
    }
}

/// Plugin capability flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PluginCapability {
    /// Can process messages.
    MessageHandler,
    /// Can generate commands.
    CommandProvider,
    /// Can provide tools for AI.
    ToolProvider,
    /// Can access network.
    NetworkAccess,
    /// Can access filesystem.
    FileSystemAccess,
    /// Can spawn processes.
    ProcessSpawn,
}

/// Plugin state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PluginState {
    /// Plugin is loaded but not started.
    Loaded,
    /// Plugin is running.
    Running,
    /// Plugin is stopped.
    Stopped,
    /// Plugin encountered an error.
    Error,
}

/// Plugin context for interacting with the system.
pub struct PluginContext {
    /// Plugin name.
    pub plugin_name: String,
    /// Configuration for this plugin.
    pub config: serde_json::Value,
    /// Shared data store.
    data: Arc<tokio::sync::RwLock<serde_json::Map<String, serde_json::Value>>>,
}

impl PluginContext {
    /// Create a new plugin context.
    pub fn new(plugin_name: impl Into<String>) -> Self {
        Self {
            plugin_name: plugin_name.into(),
            config: serde_json::Value::Null,
            data: Arc::new(tokio::sync::RwLock::new(serde_json::Map::new())),
        }
    }

    /// Create with configuration.
    pub fn with_config(mut self, config: serde_json::Value) -> Self {
        self.config = config;
        self
    }

    /// Get a value from the data store.
    pub async fn get(&self, key: &str) -> Option<serde_json::Value> {
        let data = self.data.read().await;
        data.get(key).cloned()
    }

    /// Set a value in the data store.
    pub async fn set(&self, key: impl Into<String>, value: serde_json::Value) {
        let mut data = self.data.write().await;
        data.insert(key.into(), value);
    }

    /// Remove a value from the data store.
    pub async fn remove(&self, key: &str) -> Option<serde_json::Value> {
        let mut data = self.data.write().await;
        data.remove(key)
    }

    /// Log a message.
    pub fn log(&self, level: &str, message: &str) {
        match level {
            "error" => tracing::error!(plugin = %self.plugin_name, "{}", message),
            "warn" => tracing::warn!(plugin = %self.plugin_name, "{}", message),
            "info" => tracing::info!(plugin = %self.plugin_name, "{}", message),
            "debug" => tracing::debug!(plugin = %self.plugin_name, "{}", message),
            _ => tracing::trace!(plugin = %self.plugin_name, "{}", message),
        }
    }
}

/// Plugin event types.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PluginEvent {
    /// Message received.
    Message {
        /// Session ID.
        session_id: String,
        /// User ID.
        user_id: String,
        /// Message content.
        content: String,
    },
    /// Command invoked.
    Command {
        /// Command name.
        name: String,
        /// Arguments.
        args: Vec<String>,
    },
    /// Timer fired.
    Timer {
        /// Timer ID.
        id: String,
    },
    /// Custom event.
    Custom {
        /// Event type.
        event_type: String,
        /// Event data.
        data: serde_json::Value,
    },
}

/// Plugin response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginResponse {
    /// Whether the event was handled.
    pub handled: bool,
    /// Response message (if any).
    pub message: Option<String>,
    /// Response data (if any).
    pub data: Option<serde_json::Value>,
}

impl PluginResponse {
    /// Create an unhandled response.
    pub fn unhandled() -> Self {
        Self {
            handled: false,
            message: None,
            data: None,
        }
    }

    /// Create a handled response.
    pub fn handled() -> Self {
        Self {
            handled: true,
            message: None,
            data: None,
        }
    }

    /// Create a response with a message.
    pub fn with_message(message: impl Into<String>) -> Self {
        Self {
            handled: true,
            message: Some(message.into()),
            data: None,
        }
    }

    /// Create a response with data.
    pub fn with_data(data: serde_json::Value) -> Self {
        Self {
            handled: true,
            message: None,
            data: Some(data),
        }
    }
}

/// Plugin trait that all plugins must implement.
#[async_trait]
pub trait Plugin: Send + Sync {
    /// Get plugin metadata.
    fn metadata(&self) -> &PluginMetadata;

    /// Initialize the plugin.
    async fn init(&mut self, context: &PluginContext) -> drbot_core::Result<()> {
        let _ = context;
        Ok(())
    }

    /// Start the plugin.
    async fn start(&mut self, context: &PluginContext) -> drbot_core::Result<()> {
        let _ = context;
        Ok(())
    }

    /// Stop the plugin.
    async fn stop(&mut self, context: &PluginContext) -> drbot_core::Result<()> {
        let _ = context;
        Ok(())
    }

    /// Handle an event.
    async fn handle_event(
        &self,
        event: &PluginEvent,
        context: &PluginContext,
    ) -> drbot_core::Result<PluginResponse> {
        let _ = (event, context);
        Ok(PluginResponse::unhandled())
    }
}

/// Boxed plugin type.
pub type BoxedPlugin = Box<dyn Plugin>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_metadata() {
        let meta = PluginMetadata::new("test-plugin", "1.0.0")
            .with_description("A test plugin")
            .with_author("Test Author")
            .with_capability(PluginCapability::MessageHandler);

        assert_eq!(meta.name, "test-plugin");
        assert_eq!(meta.version, "1.0.0");
        assert_eq!(meta.author, Some("Test Author".to_string()));
        assert!(meta
            .capabilities
            .contains(&PluginCapability::MessageHandler));
    }

    #[tokio::test]
    async fn test_plugin_context() {
        let ctx = PluginContext::new("test-plugin");

        ctx.set("key1", serde_json::json!("value1")).await;
        assert_eq!(ctx.get("key1").await, Some(serde_json::json!("value1")));

        ctx.remove("key1").await;
        assert_eq!(ctx.get("key1").await, None);
    }

    #[test]
    fn test_plugin_response() {
        let resp = PluginResponse::unhandled();
        assert!(!resp.handled);

        let resp = PluginResponse::handled();
        assert!(resp.handled);

        let resp = PluginResponse::with_message("Hello");
        assert!(resp.handled);
        assert_eq!(resp.message, Some("Hello".to_string()));
    }
}
