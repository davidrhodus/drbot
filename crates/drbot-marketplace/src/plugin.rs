//! Plugin system for marketplace items.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use crate::{MarketplaceError, Result};

/// Plugin manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    /// Plugin ID.
    pub id: String,
    /// Plugin name.
    pub name: String,
    /// Version.
    pub version: String,
    /// Description.
    pub description: String,
    /// Author.
    pub author: String,
    /// License.
    pub license: String,
    /// Entry point.
    pub entry: String,
    /// Required permissions.
    pub permissions: Vec<String>,
    /// Dependencies.
    pub dependencies: HashMap<String, String>,
    /// Configuration schema.
    pub config_schema: Option<serde_json::Value>,
    /// Hooks to register.
    pub hooks: Vec<HookRegistration>,
    /// Commands to register.
    pub commands: Vec<CommandRegistration>,
}

/// Hook registration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookRegistration {
    /// Hook name.
    pub name: String,
    /// Hook type.
    pub hook_type: HookType,
    /// Priority.
    pub priority: i32,
}

/// Hook types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookType {
    /// Before message processing.
    BeforeMessage,
    /// After message processing.
    AfterMessage,
    /// Before response.
    BeforeResponse,
    /// After response.
    AfterResponse,
    /// On session start.
    SessionStart,
    /// On session end.
    SessionEnd,
}

/// Command registration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandRegistration {
    /// Command name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Arguments.
    pub arguments: Vec<CommandArgument>,
}

/// Command argument.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandArgument {
    /// Argument name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Is required.
    pub required: bool,
    /// Default value.
    pub default: Option<serde_json::Value>,
}

/// Plugin configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginConfig {
    /// Plugin ID.
    pub plugin_id: String,
    /// Configuration values.
    pub values: HashMap<String, serde_json::Value>,
    /// Is enabled.
    pub enabled: bool,
}

/// Plugin state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginState {
    /// Not loaded.
    Unloaded,
    /// Loading.
    Loading,
    /// Loaded and ready.
    Ready,
    /// Error state.
    Error,
    /// Disabled.
    Disabled,
}

/// Plugin instance.
pub struct Plugin {
    /// Plugin ID.
    pub id: Uuid,
    /// Manifest.
    pub manifest: PluginManifest,
    /// Configuration.
    pub config: PluginConfig,
    /// Current state.
    pub state: PluginState,
    /// Loaded at.
    pub loaded_at: Option<DateTime<Utc>>,
    /// Error message if in error state.
    pub error: Option<String>,
}

impl Plugin {
    /// Create a new plugin from manifest.
    pub fn new(manifest: PluginManifest) -> Self {
        let config = PluginConfig {
            plugin_id: manifest.id.clone(),
            values: HashMap::new(),
            enabled: true,
        };

        Self {
            id: Uuid::new_v4(),
            manifest,
            config,
            state: PluginState::Unloaded,
            loaded_at: None,
            error: None,
        }
    }

    /// Load the plugin.
    pub async fn load(&mut self) -> Result<()> {
        self.state = PluginState::Loading;

        // In a real implementation, this would load WASM or native code
        self.state = PluginState::Ready;
        self.loaded_at = Some(Utc::now());

        Ok(())
    }

    /// Unload the plugin.
    pub async fn unload(&mut self) {
        self.state = PluginState::Unloaded;
        self.loaded_at = None;
    }

    /// Enable the plugin.
    pub fn enable(&mut self) {
        self.config.enabled = true;
        if self.state == PluginState::Disabled {
            self.state = PluginState::Ready;
        }
    }

    /// Disable the plugin.
    pub fn disable(&mut self) {
        self.config.enabled = false;
        if self.state == PluginState::Ready {
            self.state = PluginState::Disabled;
        }
    }

    /// Set configuration value.
    pub fn set_config(&mut self, key: &str, value: serde_json::Value) {
        self.config.values.insert(key.to_string(), value);
    }

    /// Get configuration value.
    pub fn get_config(&self, key: &str) -> Option<&serde_json::Value> {
        self.config.values.get(key)
    }
}

/// Plugin execution context.
pub struct PluginContext {
    /// Session ID.
    pub session_id: String,
    /// User ID.
    pub user_id: Option<String>,
    /// Channel.
    pub channel: Option<String>,
    /// Variables.
    pub variables: HashMap<String, serde_json::Value>,
}

/// Plugin runtime trait.
#[async_trait]
pub trait PluginRuntime: Send + Sync {
    /// Initialize the runtime.
    async fn init(&mut self) -> Result<()>;

    /// Load a plugin.
    async fn load_plugin(&mut self, manifest: &PluginManifest, code: &[u8]) -> Result<String>;

    /// Unload a plugin.
    async fn unload_plugin(&mut self, plugin_id: &str) -> Result<()>;

    /// Call a hook.
    async fn call_hook(
        &self,
        plugin_id: &str,
        hook: HookType,
        context: &PluginContext,
        data: serde_json::Value,
    ) -> Result<serde_json::Value>;

    /// Execute a command.
    async fn execute_command(
        &self,
        plugin_id: &str,
        command: &str,
        args: HashMap<String, serde_json::Value>,
        context: &PluginContext,
    ) -> Result<serde_json::Value>;
}

/// Simple in-process plugin runtime (for testing).
pub struct InProcessRuntime {
    plugins: HashMap<String, Plugin>,
}

impl InProcessRuntime {
    /// Create a new in-process runtime.
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
        }
    }
}

impl Default for InProcessRuntime {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PluginRuntime for InProcessRuntime {
    async fn init(&mut self) -> Result<()> {
        Ok(())
    }

    async fn load_plugin(&mut self, manifest: &PluginManifest, _code: &[u8]) -> Result<String> {
        let mut plugin = Plugin::new(manifest.clone());
        plugin.load().await?;
        let id = plugin.manifest.id.clone();
        self.plugins.insert(id.clone(), plugin);
        Ok(id)
    }

    async fn unload_plugin(&mut self, plugin_id: &str) -> Result<()> {
        if let Some(mut plugin) = self.plugins.remove(plugin_id) {
            plugin.unload().await;
        }
        Ok(())
    }

    async fn call_hook(
        &self,
        plugin_id: &str,
        _hook: HookType,
        _context: &PluginContext,
        data: serde_json::Value,
    ) -> Result<serde_json::Value> {
        if !self.plugins.contains_key(plugin_id) {
            return Err(MarketplaceError::NotFound(plugin_id.to_string()));
        }
        // In a real implementation, this would call the plugin's hook handler
        Ok(data)
    }

    async fn execute_command(
        &self,
        plugin_id: &str,
        _command: &str,
        _args: HashMap<String, serde_json::Value>,
        _context: &PluginContext,
    ) -> Result<serde_json::Value> {
        if !self.plugins.contains_key(plugin_id) {
            return Err(MarketplaceError::NotFound(plugin_id.to_string()));
        }
        // In a real implementation, this would execute the command
        Ok(serde_json::Value::Null)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_manifest() {
        let manifest = PluginManifest {
            id: "test-plugin".to_string(),
            name: "Test Plugin".to_string(),
            version: "1.0.0".to_string(),
            description: "A test plugin".to_string(),
            author: "Test".to_string(),
            license: "MIT".to_string(),
            entry: "main.wasm".to_string(),
            permissions: vec!["read".to_string()],
            dependencies: HashMap::new(),
            config_schema: None,
            hooks: Vec::new(),
            commands: Vec::new(),
        };

        let plugin = Plugin::new(manifest);
        assert_eq!(plugin.state, PluginState::Unloaded);
    }

    #[tokio::test]
    async fn test_plugin_lifecycle() {
        let manifest = PluginManifest {
            id: "test".to_string(),
            name: "Test".to_string(),
            version: "1.0.0".to_string(),
            description: "Test".to_string(),
            author: "Test".to_string(),
            license: "MIT".to_string(),
            entry: "main.wasm".to_string(),
            permissions: Vec::new(),
            dependencies: HashMap::new(),
            config_schema: None,
            hooks: Vec::new(),
            commands: Vec::new(),
        };

        let mut plugin = Plugin::new(manifest);
        assert_eq!(plugin.state, PluginState::Unloaded);

        plugin.load().await.unwrap();
        assert_eq!(plugin.state, PluginState::Ready);

        plugin.disable();
        assert_eq!(plugin.state, PluginState::Disabled);

        plugin.enable();
        assert_eq!(plugin.state, PluginState::Ready);

        plugin.unload().await;
        assert_eq!(plugin.state, PluginState::Unloaded);
    }
}
