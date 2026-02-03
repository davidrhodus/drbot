//! Hot configuration reload for drbot.
//!
//! This crate provides:
//! - File-based configuration watching
//! - Atomic configuration updates
//! - Change notification
//! - Validation before applying changes

use chrono::{DateTime, Utc};
use serde::{de::DeserializeOwned, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::{broadcast, RwLock};

/// Configuration error types.
#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("File not found: {0}")]
    FileNotFound(String),

    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Validation error: {0}")]
    ValidationError(String),

    #[error("Watch error: {0}")]
    WatchError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Result type for config operations.
pub type Result<T> = std::result::Result<T, ConfigError>;

/// Configuration change event.
#[derive(Debug, Clone)]
pub struct ConfigChange {
    /// Path that changed.
    pub path: PathBuf,
    /// Timestamp of change.
    pub timestamp: DateTime<Utc>,
    /// Change type.
    pub change_type: ChangeType,
}

/// Type of configuration change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeType {
    /// File was created.
    Created,
    /// File was modified.
    Modified,
    /// File was deleted.
    Deleted,
}

/// Configuration format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigFormat {
    /// JSON format.
    Json,
    /// TOML format.
    Toml,
    /// YAML format.
    Yaml,
}

impl ConfigFormat {
    /// Detect format from file extension.
    pub fn from_extension(path: &std::path::Path) -> Option<Self> {
        path.extension().and_then(|ext| match ext.to_str()? {
            "json" => Some(Self::Json),
            "toml" => Some(Self::Toml),
            "yaml" | "yml" => Some(Self::Yaml),
            _ => None,
        })
    }
}

/// Configuration parser.
pub struct ConfigParser;

impl ConfigParser {
    /// Parse configuration from string.
    pub fn parse<T: DeserializeOwned>(content: &str, format: ConfigFormat) -> Result<T> {
        match format {
            ConfigFormat::Json => {
                serde_json::from_str(content).map_err(|e| ConfigError::ParseError(e.to_string()))
            }
            ConfigFormat::Toml => {
                toml::from_str(content).map_err(|e| ConfigError::ParseError(e.to_string()))
            }
            ConfigFormat::Yaml => {
                // For now, we only support JSON and TOML
                Err(ConfigError::ParseError("YAML not supported".to_string()))
            }
        }
    }

    /// Serialize configuration to string.
    pub fn serialize<T: Serialize>(value: &T, format: ConfigFormat) -> Result<String> {
        match format {
            ConfigFormat::Json => serde_json::to_string_pretty(value)
                .map_err(|e| ConfigError::ParseError(e.to_string())),
            ConfigFormat::Toml => {
                toml::to_string_pretty(value).map_err(|e| ConfigError::ParseError(e.to_string()))
            }
            ConfigFormat::Yaml => Err(ConfigError::ParseError("YAML not supported".to_string())),
        }
    }
}

/// Configuration snapshot.
#[derive(Debug, Clone)]
pub struct ConfigSnapshot<T> {
    /// The configuration value.
    pub value: T,
    /// When the config was loaded.
    pub loaded_at: DateTime<Utc>,
    /// Source file path.
    pub source: Option<PathBuf>,
    /// Config version.
    pub version: u64,
}

/// Hot-reloadable configuration.
pub struct HotConfig<T> {
    current: Arc<RwLock<ConfigSnapshot<T>>>,
    change_tx: broadcast::Sender<ConfigChange>,
    validators: Vec<Box<dyn Fn(&T) -> Result<()> + Send + Sync>>,
}

impl<T: Clone + Send + Sync + 'static> HotConfig<T> {
    /// Create a new hot config with initial value.
    pub fn new(initial: T) -> Self {
        let (change_tx, _) = broadcast::channel(16);
        let snapshot = ConfigSnapshot {
            value: initial,
            loaded_at: Utc::now(),
            source: None,
            version: 1,
        };

        Self {
            current: Arc::new(RwLock::new(snapshot)),
            change_tx,
            validators: Vec::new(),
        }
    }

    /// Add a validator.
    pub fn add_validator<F>(&mut self, validator: F)
    where
        F: Fn(&T) -> Result<()> + Send + Sync + 'static,
    {
        self.validators.push(Box::new(validator));
    }

    /// Get the current configuration.
    pub async fn get(&self) -> T {
        self.current.read().await.value.clone()
    }

    /// Get the current snapshot.
    pub async fn snapshot(&self) -> ConfigSnapshot<T> {
        self.current.read().await.clone()
    }

    /// Update the configuration.
    pub async fn update(&self, new_value: T, source: Option<PathBuf>) -> Result<()> {
        // Run validators
        for validator in &self.validators {
            validator(&new_value)?;
        }

        // Update atomically
        let mut current = self.current.write().await;
        current.value = new_value;
        current.loaded_at = Utc::now();
        current.version += 1;

        if let Some(ref path) = source {
            current.source = Some(path.clone());

            // Notify subscribers
            let _ = self.change_tx.send(ConfigChange {
                path: path.clone(),
                timestamp: Utc::now(),
                change_type: ChangeType::Modified,
            });
        }

        Ok(())
    }

    /// Subscribe to configuration changes.
    pub fn subscribe(&self) -> broadcast::Receiver<ConfigChange> {
        self.change_tx.subscribe()
    }

    /// Get the current version.
    pub async fn version(&self) -> u64 {
        self.current.read().await.version
    }
}

impl<T: Clone + Send + Sync + DeserializeOwned + 'static> HotConfig<T> {
    /// Load configuration from a file.
    pub async fn load_file(&self, path: &std::path::Path) -> Result<()> {
        let format = ConfigFormat::from_extension(path)
            .ok_or_else(|| ConfigError::ParseError("Unknown file format".to_string()))?;

        let content = tokio::fs::read_to_string(path).await?;
        let value: T = ConfigParser::parse(&content, format)?;

        self.update(value, Some(path.to_path_buf())).await
    }

    /// Create from a file.
    pub async fn from_file(path: &std::path::Path) -> Result<Self> {
        let format = ConfigFormat::from_extension(path)
            .ok_or_else(|| ConfigError::ParseError("Unknown file format".to_string()))?;

        let content = tokio::fs::read_to_string(path).await?;
        let value: T = ConfigParser::parse(&content, format)?;

        let config = Self::new(value);
        {
            let mut current = config.current.write().await;
            current.source = Some(path.to_path_buf());
        }

        Ok(config)
    }
}

/// Configuration value that merges from multiple sources.
#[derive(Debug, Clone, Default)]
pub struct LayeredConfig {
    layers: Vec<ConfigLayer>,
}

/// A configuration layer.
#[derive(Debug, Clone)]
pub struct ConfigLayer {
    /// Layer name.
    pub name: String,
    /// Layer priority (higher = more important).
    pub priority: i32,
    /// Layer values.
    pub values: HashMap<String, serde_json::Value>,
}

impl LayeredConfig {
    /// Create a new layered config.
    pub fn new() -> Self {
        Self { layers: Vec::new() }
    }

    /// Add a layer.
    pub fn add_layer(&mut self, name: impl Into<String>, priority: i32) -> &mut ConfigLayer {
        let layer = ConfigLayer {
            name: name.into(),
            priority,
            values: HashMap::new(),
        };
        self.layers.push(layer);
        self.layers.sort_by(|a, b| a.priority.cmp(&b.priority));
        self.layers.last_mut().unwrap()
    }

    /// Get a value, merging from all layers.
    pub fn get(&self, key: &str) -> Option<&serde_json::Value> {
        // Higher priority layers override lower ones
        for layer in self.layers.iter().rev() {
            if let Some(value) = layer.values.get(key) {
                return Some(value);
            }
        }
        None
    }

    /// Get a typed value.
    pub fn get_typed<T: DeserializeOwned>(&self, key: &str) -> Option<T> {
        self.get(key)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    /// Get all keys.
    pub fn keys(&self) -> Vec<String> {
        let mut keys: Vec<String> = self
            .layers
            .iter()
            .flat_map(|l| l.values.keys().cloned())
            .collect();
        keys.sort();
        keys.dedup();
        keys
    }

    /// Merge into a single map.
    pub fn merge(&self) -> HashMap<String, serde_json::Value> {
        let mut result = HashMap::new();

        // Lower priority first
        for layer in &self.layers {
            for (key, value) in &layer.values {
                result.insert(key.clone(), value.clone());
            }
        }

        result
    }
}

impl ConfigLayer {
    /// Set a value.
    pub fn set(&mut self, key: impl Into<String>, value: impl Serialize) -> &mut Self {
        if let Ok(json_value) = serde_json::to_value(value) {
            self.values.insert(key.into(), json_value);
        }
        self
    }
}

/// Environment variable configuration source.
pub struct EnvConfig {
    prefix: Option<String>,
    separator: String,
}

impl EnvConfig {
    /// Create a new environment config.
    pub fn new() -> Self {
        Self {
            prefix: None,
            separator: "_".to_string(),
        }
    }

    /// Set prefix for environment variables.
    pub fn with_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.prefix = Some(prefix.into());
        self
    }

    /// Set separator for nested keys.
    pub fn with_separator(mut self, separator: impl Into<String>) -> Self {
        self.separator = separator.into();
        self
    }

    /// Get all matching environment variables.
    pub fn get_all(&self) -> HashMap<String, String> {
        let mut result = HashMap::new();

        for (key, value) in std::env::vars() {
            if let Some(ref prefix) = self.prefix {
                if key.starts_with(prefix) {
                    let key = key[prefix.len()..].trim_start_matches(&self.separator);
                    result.insert(key.to_lowercase(), value);
                }
            } else {
                result.insert(key.to_lowercase(), value);
            }
        }

        result
    }

    /// Get a specific value.
    pub fn get(&self, key: &str) -> Option<String> {
        let env_key = match &self.prefix {
            Some(prefix) => format!("{}{}{}", prefix, self.separator, key.to_uppercase()),
            None => key.to_uppercase(),
        };

        std::env::var(env_key).ok()
    }
}

impl Default for EnvConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// Configuration builder.
pub struct ConfigBuilder {
    layered: LayeredConfig,
}

impl ConfigBuilder {
    /// Create a new config builder.
    pub fn new() -> Self {
        Self {
            layered: LayeredConfig::new(),
        }
    }

    /// Add defaults.
    pub fn add_defaults(mut self, defaults: HashMap<String, serde_json::Value>) -> Self {
        let layer = self.layered.add_layer("defaults", 0);
        layer.values = defaults;
        self
    }

    /// Add from environment.
    pub fn add_env(mut self, env: &EnvConfig) -> Self {
        let layer = self.layered.add_layer("env", 100);
        for (key, value) in env.get_all() {
            layer.set(key, value);
        }
        self
    }

    /// Build the configuration.
    pub fn build(self) -> LayeredConfig {
        self.layered
    }
}

impl Default for ConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    struct TestConfig {
        name: String,
        value: i32,
    }

    #[test]
    fn test_config_format_detection() {
        assert_eq!(
            ConfigFormat::from_extension(std::path::Path::new("config.json")),
            Some(ConfigFormat::Json)
        );
        assert_eq!(
            ConfigFormat::from_extension(std::path::Path::new("config.toml")),
            Some(ConfigFormat::Toml)
        );
        assert_eq!(
            ConfigFormat::from_extension(std::path::Path::new("config.yaml")),
            Some(ConfigFormat::Yaml)
        );
        assert_eq!(
            ConfigFormat::from_extension(std::path::Path::new("config.txt")),
            None
        );
    }

    #[test]
    fn test_json_parse() {
        let json = r#"{"name": "test", "value": 42}"#;
        let config: TestConfig = ConfigParser::parse(json, ConfigFormat::Json).unwrap();
        assert_eq!(config.name, "test");
        assert_eq!(config.value, 42);
    }

    #[test]
    fn test_toml_parse() {
        let toml = r#"
            name = "test"
            value = 42
        "#;
        let config: TestConfig = ConfigParser::parse(toml, ConfigFormat::Toml).unwrap();
        assert_eq!(config.name, "test");
        assert_eq!(config.value, 42);
    }

    #[test]
    fn test_serialize_json() {
        let config = TestConfig {
            name: "test".to_string(),
            value: 42,
        };
        let json = ConfigParser::serialize(&config, ConfigFormat::Json).unwrap();
        assert!(json.contains("\"name\": \"test\""));
    }

    #[tokio::test]
    async fn test_hot_config() {
        let config = HotConfig::new(TestConfig {
            name: "initial".to_string(),
            value: 1,
        });

        assert_eq!(config.get().await.name, "initial");
        assert_eq!(config.version().await, 1);

        config
            .update(
                TestConfig {
                    name: "updated".to_string(),
                    value: 2,
                },
                None,
            )
            .await
            .unwrap();

        assert_eq!(config.get().await.name, "updated");
        assert_eq!(config.version().await, 2);
    }

    #[tokio::test]
    async fn test_hot_config_validator() {
        let mut config = HotConfig::new(TestConfig {
            name: "test".to_string(),
            value: 5,
        });

        config.add_validator(|c| {
            if c.value < 0 {
                Err(ConfigError::ValidationError(
                    "value must be positive".to_string(),
                ))
            } else {
                Ok(())
            }
        });

        // Valid update
        config
            .update(
                TestConfig {
                    name: "test".to_string(),
                    value: 10,
                },
                None,
            )
            .await
            .unwrap();

        // Invalid update
        let result = config
            .update(
                TestConfig {
                    name: "test".to_string(),
                    value: -1,
                },
                None,
            )
            .await;
        assert!(result.is_err());
    }

    #[test]
    fn test_layered_config() {
        let mut layered = LayeredConfig::new();

        layered.add_layer("defaults", 0).set("key1", "default1");

        layered.add_layer("overrides", 10).set("key1", "override1");

        assert_eq!(
            layered.get_typed::<String>("key1"),
            Some("override1".to_string())
        );
    }

    #[test]
    fn test_layered_config_merge() {
        let mut layered = LayeredConfig::new();

        layered
            .add_layer("defaults", 0)
            .set("key1", "value1")
            .set("key2", "value2");

        layered.add_layer("overrides", 10).set("key2", "override2");

        let merged = layered.merge();
        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn test_env_config() {
        std::env::set_var("TEST_DRBOT_KEY", "value");

        let env = EnvConfig::new().with_prefix("TEST_DRBOT");
        let value = env.get("key");
        assert_eq!(value, Some("value".to_string()));

        std::env::remove_var("TEST_DRBOT_KEY");
    }

    #[test]
    fn test_config_builder() {
        let mut defaults = HashMap::new();
        defaults.insert("key".to_string(), serde_json::json!("value"));

        let config = ConfigBuilder::new().add_defaults(defaults).build();

        assert_eq!(config.get_typed::<String>("key"), Some("value".to_string()));
    }
}
