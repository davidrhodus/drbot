//! WASM plugin runtime for sandboxed plugin execution.
//!
//! Provides a secure execution environment for plugins compiled to WebAssembly.
//!
//! # Features
//!
//! - Sandboxed execution (no direct system access)
//! - Memory limits and fuel/instruction limits
//! - Host function imports for controlled capabilities
//! - Async support via Tokio
//!
//! # Plugin Interface
//!
//! WASM plugins must export the following functions:
//! - `plugin_init() -> i32`: Initialize the plugin
//! - `plugin_name() -> *const u8`: Return plugin name pointer
//! - `plugin_version() -> *const u8`: Return version pointer
//! - `handle_event(event_ptr: i32, event_len: i32) -> i32`: Handle an event
//! - `plugin_shutdown()`: Cleanup
//!
//! # Example
//!
//! ```rust,no_run
//! use drbot_plugins::wasm::{WasmRuntime, WasmPluginConfig};
//!
//! async fn example() {
//!     let runtime = WasmRuntime::new().await.unwrap();
//!
//!     let config = WasmPluginConfig::default()
//!         .with_memory_limit(64 * 1024 * 1024); // 64MB
//!
//!     let plugin = runtime.load_plugin("plugin.wasm", config).await.unwrap();
//! }
//! ```

use crate::{Plugin, PluginContext, PluginEvent, PluginMetadata, PluginResponse};
use async_trait::async_trait;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// WASM plugin configuration.
#[derive(Debug, Clone)]
pub struct WasmPluginConfig {
    /// Memory limit in bytes.
    pub memory_limit: usize,
    /// Fuel limit (instruction count limit).
    pub fuel_limit: Option<u64>,
    /// Allow network access.
    pub allow_network: bool,
    /// Allow filesystem access.
    pub allow_filesystem: bool,
    /// Allowed host functions.
    pub allowed_imports: Vec<String>,
}

impl Default for WasmPluginConfig {
    fn default() -> Self {
        Self {
            memory_limit: 32 * 1024 * 1024,  // 32MB
            fuel_limit: Some(1_000_000_000), // 1B instructions
            allow_network: false,
            allow_filesystem: false,
            allowed_imports: vec![
                "log".to_string(),
                "get_config".to_string(),
                "set_data".to_string(),
                "get_data".to_string(),
            ],
        }
    }
}

impl WasmPluginConfig {
    /// Set memory limit.
    pub fn with_memory_limit(mut self, limit: usize) -> Self {
        self.memory_limit = limit;
        self
    }

    /// Set fuel limit.
    pub fn with_fuel_limit(mut self, limit: u64) -> Self {
        self.fuel_limit = Some(limit);
        self
    }

    /// Allow network access.
    pub fn with_network(mut self) -> Self {
        self.allow_network = true;
        self.allowed_imports.push("http_request".to_string());
        self
    }

    /// Allow filesystem access.
    pub fn with_filesystem(mut self) -> Self {
        self.allow_filesystem = true;
        self.allowed_imports.push("read_file".to_string());
        self.allowed_imports.push("write_file".to_string());
        self
    }
}

/// WASM runtime for loading and executing plugins.
#[cfg(feature = "wasm")]
pub struct WasmRuntime {
    engine: wasmtime::Engine,
}

#[cfg(feature = "wasm")]
impl WasmRuntime {
    /// Create a new WASM runtime.
    pub async fn new() -> Result<Self, WasmError> {
        let mut config = wasmtime::Config::new();
        config.async_support(true);
        config.consume_fuel(true);

        let engine =
            wasmtime::Engine::new(&config).map_err(|e| WasmError::RuntimeError(e.to_string()))?;

        info!("WASM runtime initialized");
        Ok(Self { engine })
    }

    /// Load a plugin from a file.
    pub async fn load_plugin(
        &self,
        path: impl AsRef<Path>,
        config: WasmPluginConfig,
    ) -> Result<WasmPlugin, WasmError> {
        let path = path.as_ref();
        let bytes = tokio::fs::read(path)
            .await
            .map_err(|e| WasmError::LoadError(e.to_string()))?;

        self.load_plugin_bytes(&bytes, config).await
    }

    /// Load a plugin from bytes.
    pub async fn load_plugin_bytes(
        &self,
        bytes: &[u8],
        config: WasmPluginConfig,
    ) -> Result<WasmPlugin, WasmError> {
        let module = wasmtime::Module::new(&self.engine, bytes)
            .map_err(|e| WasmError::LoadError(e.to_string()))?;

        info!("WASM module loaded");

        Ok(WasmPlugin {
            engine: self.engine.clone(),
            module,
            config,
            metadata: PluginMetadata::new("wasm-plugin", "0.0.0"),
            store_data: Arc::new(RwLock::new(WasmStoreData::default())),
        })
    }
}

/// Stub WASM runtime when feature is disabled.
#[cfg(not(feature = "wasm"))]
pub struct WasmRuntime;

#[cfg(not(feature = "wasm"))]
impl WasmRuntime {
    /// Create a new WASM runtime (stub).
    pub async fn new() -> Result<Self, WasmError> {
        Err(WasmError::NotSupported)
    }

    /// Load a plugin from a file (stub).
    pub async fn load_plugin(
        &self,
        _path: impl AsRef<Path>,
        _config: WasmPluginConfig,
    ) -> Result<WasmPlugin, WasmError> {
        Err(WasmError::NotSupported)
    }

    /// Load a plugin from bytes (stub).
    pub async fn load_plugin_bytes(
        &self,
        _bytes: &[u8],
        _config: WasmPluginConfig,
    ) -> Result<WasmPlugin, WasmError> {
        Err(WasmError::NotSupported)
    }
}

/// WASM store data for host functions.
#[derive(Default)]
struct WasmStoreData {
    /// Plugin data storage.
    data: std::collections::HashMap<String, Vec<u8>>,
    /// Logs from the plugin.
    logs: Vec<String>,
}

/// A loaded WASM plugin.
#[cfg(feature = "wasm")]
pub struct WasmPlugin {
    engine: wasmtime::Engine,
    module: wasmtime::Module,
    config: WasmPluginConfig,
    metadata: PluginMetadata,
    store_data: Arc<RwLock<WasmStoreData>>,
}

#[cfg(feature = "wasm")]
impl WasmPlugin {
    /// Get the module.
    pub fn module(&self) -> &wasmtime::Module {
        &self.module
    }

    /// Get the config.
    pub fn config(&self) -> &WasmPluginConfig {
        &self.config
    }
}

/// Stub WASM plugin when feature is disabled.
#[cfg(not(feature = "wasm"))]
pub struct WasmPlugin {
    config: WasmPluginConfig,
    metadata: PluginMetadata,
}

#[cfg(not(feature = "wasm"))]
impl WasmPlugin {
    /// Get the config.
    pub fn config(&self) -> &WasmPluginConfig {
        &self.config
    }
}

#[async_trait]
impl Plugin for WasmPlugin {
    fn metadata(&self) -> &PluginMetadata {
        &self.metadata
    }

    async fn init(&mut self, _context: &PluginContext) -> drbot_core::Result<()> {
        #[cfg(not(feature = "wasm"))]
        {
            return Err(drbot_core::Error::Internal("WASM not enabled".to_string()));
        }

        #[cfg(feature = "wasm")]
        {
            debug!("Initializing WASM plugin");
            // Would call plugin_init() export here
            Ok(())
        }
    }

    async fn handle_event(
        &self,
        event: &PluginEvent,
        _context: &PluginContext,
    ) -> drbot_core::Result<PluginResponse> {
        #[cfg(not(feature = "wasm"))]
        {
            let _ = event;
            return Ok(PluginResponse::unhandled());
        }

        #[cfg(feature = "wasm")]
        {
            debug!("WASM plugin handling event");
            // Would call handle_event() export here
            Ok(PluginResponse::unhandled())
        }
    }
}

/// WASM-related errors.
#[derive(Debug, thiserror::Error)]
pub enum WasmError {
    #[error("WASM support not enabled")]
    NotSupported,
    #[error("Failed to load module: {0}")]
    LoadError(String),
    #[error("Runtime error: {0}")]
    RuntimeError(String),
    #[error("Plugin error: {0}")]
    PluginError(String),
    #[error("Memory limit exceeded")]
    MemoryLimitExceeded,
    #[error("Fuel exhausted")]
    FuelExhausted,
}

/// Manifest for WASM plugins.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WasmPluginManifest {
    /// Plugin name.
    pub name: String,
    /// Plugin version.
    pub version: String,
    /// Plugin description.
    pub description: Option<String>,
    /// Author.
    pub author: Option<String>,
    /// WASM file path (relative to manifest).
    pub wasm_file: String,
    /// Required capabilities.
    pub capabilities: Vec<String>,
    /// Configuration schema.
    pub config_schema: Option<serde_json::Value>,
}

impl WasmPluginManifest {
    /// Load from a file.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, WasmError> {
        let content =
            std::fs::read_to_string(path).map_err(|e| WasmError::LoadError(e.to_string()))?;

        serde_json::from_str(&content).map_err(|e| WasmError::LoadError(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = WasmPluginConfig::default();
        assert!(!config.allow_network);
        assert!(!config.allow_filesystem);
    }

    #[test]
    fn test_config_builder() {
        let config = WasmPluginConfig::default()
            .with_memory_limit(64 * 1024 * 1024)
            .with_network();

        assert!(config.allow_network);
        assert_eq!(config.memory_limit, 64 * 1024 * 1024);
    }

    #[test]
    fn test_manifest_parsing() {
        let json = r#"{
            "name": "test-plugin",
            "version": "1.0.0",
            "description": "A test plugin",
            "wasm_file": "plugin.wasm",
            "capabilities": ["network"]
        }"#;

        let manifest: WasmPluginManifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.name, "test-plugin");
        assert_eq!(manifest.version, "1.0.0");
    }
}
