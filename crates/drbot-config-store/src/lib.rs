//! Configuration storage for drbot.
//!
//! This crate provides:
//! - Hierarchical configuration
//! - Environment overrides
//! - Configuration versioning
//! - Change notifications

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::{broadcast, RwLock};

/// Config store error types.
#[derive(Error, Debug)]
pub enum ConfigStoreError {
    #[error("Key not found: {0}")]
    KeyNotFound(String),

    #[error("Invalid value: {0}")]
    InvalidValue(String),

    #[error("Version conflict")]
    VersionConflict,

    #[error("Storage error: {0}")]
    StorageError(String),
}

/// Result type for config operations.
pub type Result<T> = std::result::Result<T, ConfigStoreError>;

/// Configuration value.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigValue {
    /// The value.
    pub value: serde_json::Value,
    /// Value type hint.
    pub value_type: ValueType,
    /// Description.
    pub description: Option<String>,
    /// Default value.
    pub default: Option<serde_json::Value>,
    /// Whether this is sensitive.
    pub sensitive: bool,
    /// Version number.
    pub version: u64,
    /// Last updated.
    pub updated_at: DateTime<Utc>,
    /// Updated by.
    pub updated_by: Option<String>,
}

impl ConfigValue {
    /// Create a new config value.
    pub fn new(value: serde_json::Value) -> Self {
        Self {
            value,
            value_type: ValueType::String,
            description: None,
            default: None,
            sensitive: false,
            version: 1,
            updated_at: Utc::now(),
            updated_by: None,
        }
    }

    /// Set value type.
    pub fn with_type(mut self, value_type: ValueType) -> Self {
        self.value_type = value_type;
        self
    }

    /// Set description.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Set default.
    pub fn with_default(mut self, default: serde_json::Value) -> Self {
        self.default = Some(default);
        self
    }

    /// Mark as sensitive.
    pub fn sensitive(mut self) -> Self {
        self.sensitive = true;
        self
    }

    /// Get as string.
    pub fn as_string(&self) -> Option<String> {
        self.value.as_str().map(|s| s.to_string())
    }

    /// Get as i64.
    pub fn as_i64(&self) -> Option<i64> {
        self.value.as_i64()
    }

    /// Get as f64.
    pub fn as_f64(&self) -> Option<f64> {
        self.value.as_f64()
    }

    /// Get as bool.
    pub fn as_bool(&self) -> Option<bool> {
        self.value.as_bool()
    }
}

/// Value type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ValueType {
    String,
    Integer,
    Float,
    Boolean,
    Json,
    List,
}

/// Configuration namespace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Namespace {
    /// Namespace name.
    pub name: String,
    /// Description.
    pub description: Option<String>,
    /// Parent namespace.
    pub parent: Option<String>,
    /// Created at.
    pub created_at: DateTime<Utc>,
}

impl Namespace {
    /// Create a new namespace.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: None,
            parent: None,
            created_at: Utc::now(),
        }
    }

    /// Set parent.
    pub fn with_parent(mut self, parent: impl Into<String>) -> Self {
        self.parent = Some(parent.into());
        self
    }
}

/// Config change event.
#[derive(Debug, Clone)]
pub struct ConfigChange {
    /// Namespace.
    pub namespace: String,
    /// Key.
    pub key: String,
    /// Old value.
    pub old_value: Option<ConfigValue>,
    /// New value.
    pub new_value: Option<ConfigValue>,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
}

/// Config store trait.
#[async_trait]
pub trait ConfigStore: Send + Sync {
    /// Get a value.
    async fn get(&self, namespace: &str, key: &str) -> Result<Option<ConfigValue>>;

    /// Set a value.
    async fn set(&self, namespace: &str, key: &str, value: ConfigValue) -> Result<()>;

    /// Delete a value.
    async fn delete(&self, namespace: &str, key: &str) -> Result<()>;

    /// List keys in namespace.
    async fn list_keys(&self, namespace: &str) -> Result<Vec<String>>;

    /// Get all values in namespace.
    async fn get_all(&self, namespace: &str) -> Result<HashMap<String, ConfigValue>>;

    /// Create namespace.
    async fn create_namespace(&self, namespace: Namespace) -> Result<()>;

    /// List namespaces.
    async fn list_namespaces(&self) -> Result<Vec<Namespace>>;
}

/// In-memory config store.
pub struct InMemoryConfigStore {
    values: RwLock<HashMap<String, HashMap<String, ConfigValue>>>,
    namespaces: RwLock<HashMap<String, Namespace>>,
}

impl InMemoryConfigStore {
    /// Create new store.
    pub fn new() -> Self {
        Self {
            values: RwLock::new(HashMap::new()),
            namespaces: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for InMemoryConfigStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ConfigStore for InMemoryConfigStore {
    async fn get(&self, namespace: &str, key: &str) -> Result<Option<ConfigValue>> {
        let values = self.values.read().await;
        Ok(values.get(namespace).and_then(|ns| ns.get(key)).cloned())
    }

    async fn set(&self, namespace: &str, key: &str, mut value: ConfigValue) -> Result<()> {
        let mut values = self.values.write().await;
        let ns = values.entry(namespace.to_string()).or_default();

        // Increment version if exists
        if let Some(existing) = ns.get(key) {
            value.version = existing.version + 1;
        }
        value.updated_at = Utc::now();

        ns.insert(key.to_string(), value);
        Ok(())
    }

    async fn delete(&self, namespace: &str, key: &str) -> Result<()> {
        let mut values = self.values.write().await;
        if let Some(ns) = values.get_mut(namespace) {
            ns.remove(key);
        }
        Ok(())
    }

    async fn list_keys(&self, namespace: &str) -> Result<Vec<String>> {
        let values = self.values.read().await;
        Ok(values
            .get(namespace)
            .map(|ns| ns.keys().cloned().collect())
            .unwrap_or_default())
    }

    async fn get_all(&self, namespace: &str) -> Result<HashMap<String, ConfigValue>> {
        let values = self.values.read().await;
        Ok(values.get(namespace).cloned().unwrap_or_default())
    }

    async fn create_namespace(&self, namespace: Namespace) -> Result<()> {
        let mut namespaces = self.namespaces.write().await;
        namespaces.insert(namespace.name.clone(), namespace);
        Ok(())
    }

    async fn list_namespaces(&self) -> Result<Vec<Namespace>> {
        let namespaces = self.namespaces.read().await;
        Ok(namespaces.values().cloned().collect())
    }
}

/// Layered config store (with overrides).
pub struct LayeredConfigStore {
    layers: Vec<Arc<dyn ConfigStore>>,
}

impl LayeredConfigStore {
    /// Create new layered store.
    pub fn new() -> Self {
        Self { layers: Vec::new() }
    }

    /// Add a layer (later layers override earlier).
    pub fn add_layer(mut self, store: Arc<dyn ConfigStore>) -> Self {
        self.layers.push(store);
        self
    }
}

impl Default for LayeredConfigStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ConfigStore for LayeredConfigStore {
    async fn get(&self, namespace: &str, key: &str) -> Result<Option<ConfigValue>> {
        // Check layers in reverse order (last added wins)
        for layer in self.layers.iter().rev() {
            if let Some(value) = layer.get(namespace, key).await? {
                return Ok(Some(value));
            }
        }
        Ok(None)
    }

    async fn set(&self, namespace: &str, key: &str, value: ConfigValue) -> Result<()> {
        // Set in the last layer
        if let Some(layer) = self.layers.last() {
            layer.set(namespace, key, value).await
        } else {
            Err(ConfigStoreError::StorageError(
                "No layers configured".to_string(),
            ))
        }
    }

    async fn delete(&self, namespace: &str, key: &str) -> Result<()> {
        if let Some(layer) = self.layers.last() {
            layer.delete(namespace, key).await
        } else {
            Ok(())
        }
    }

    async fn list_keys(&self, namespace: &str) -> Result<Vec<String>> {
        let mut keys = std::collections::HashSet::new();
        for layer in &self.layers {
            for key in layer.list_keys(namespace).await? {
                keys.insert(key);
            }
        }
        Ok(keys.into_iter().collect())
    }

    async fn get_all(&self, namespace: &str) -> Result<HashMap<String, ConfigValue>> {
        let mut result = HashMap::new();
        for layer in &self.layers {
            for (key, value) in layer.get_all(namespace).await? {
                result.insert(key, value);
            }
        }
        Ok(result)
    }

    async fn create_namespace(&self, namespace: Namespace) -> Result<()> {
        if let Some(layer) = self.layers.last() {
            layer.create_namespace(namespace).await
        } else {
            Err(ConfigStoreError::StorageError(
                "No layers configured".to_string(),
            ))
        }
    }

    async fn list_namespaces(&self) -> Result<Vec<Namespace>> {
        let mut namespaces = HashMap::new();
        for layer in &self.layers {
            for ns in layer.list_namespaces().await? {
                namespaces.insert(ns.name.clone(), ns);
            }
        }
        Ok(namespaces.into_values().collect())
    }
}

/// Config service with caching and notifications.
pub struct ConfigService<S: ConfigStore> {
    store: Arc<S>,
    cache: RwLock<HashMap<String, HashMap<String, ConfigValue>>>,
    change_tx: broadcast::Sender<ConfigChange>,
}

impl<S: ConfigStore> ConfigService<S> {
    /// Create new service.
    pub fn new(store: Arc<S>) -> Self {
        let (change_tx, _) = broadcast::channel(100);
        Self {
            store,
            cache: RwLock::new(HashMap::new()),
            change_tx,
        }
    }

    /// Subscribe to changes.
    pub fn subscribe(&self) -> broadcast::Receiver<ConfigChange> {
        self.change_tx.subscribe()
    }

    /// Get a value.
    pub async fn get(&self, namespace: &str, key: &str) -> Result<Option<ConfigValue>> {
        // Check cache
        {
            let cache = self.cache.read().await;
            if let Some(ns) = cache.get(namespace) {
                if let Some(value) = ns.get(key) {
                    return Ok(Some(value.clone()));
                }
            }
        }

        // Load from store
        let value = self.store.get(namespace, key).await?;

        // Cache it
        if let Some(ref v) = value {
            let mut cache = self.cache.write().await;
            cache
                .entry(namespace.to_string())
                .or_default()
                .insert(key.to_string(), v.clone());
        }

        Ok(value)
    }

    /// Get string value.
    pub async fn get_string(&self, namespace: &str, key: &str) -> Option<String> {
        self.get(namespace, key)
            .await
            .ok()
            .flatten()
            .and_then(|v| v.as_string())
    }

    /// Get int value.
    pub async fn get_int(&self, namespace: &str, key: &str) -> Option<i64> {
        self.get(namespace, key)
            .await
            .ok()
            .flatten()
            .and_then(|v| v.as_i64())
    }

    /// Get bool value.
    pub async fn get_bool(&self, namespace: &str, key: &str) -> Option<bool> {
        self.get(namespace, key)
            .await
            .ok()
            .flatten()
            .and_then(|v| v.as_bool())
    }

    /// Set a value.
    pub async fn set(&self, namespace: &str, key: &str, value: ConfigValue) -> Result<()> {
        let old_value = self.store.get(namespace, key).await?;
        self.store.set(namespace, key, value.clone()).await?;

        // Update cache
        {
            let mut cache = self.cache.write().await;
            cache
                .entry(namespace.to_string())
                .or_default()
                .insert(key.to_string(), value.clone());
        }

        // Emit change
        let _ = self.change_tx.send(ConfigChange {
            namespace: namespace.to_string(),
            key: key.to_string(),
            old_value,
            new_value: Some(value),
            timestamp: Utc::now(),
        });

        Ok(())
    }

    /// Invalidate cache.
    pub async fn invalidate(&self, namespace: &str) {
        let mut cache = self.cache.write().await;
        cache.remove(namespace);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_value() {
        let value = ConfigValue::new(serde_json::json!("test"))
            .with_type(ValueType::String)
            .with_description("Test config");

        assert_eq!(value.as_string(), Some("test".to_string()));
    }

    #[tokio::test]
    async fn test_in_memory_store() {
        let store = InMemoryConfigStore::new();

        let value = ConfigValue::new(serde_json::json!("hello"));
        store.set("app", "greeting", value).await.unwrap();

        let retrieved = store.get("app", "greeting").await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().as_string(), Some("hello".to_string()));
    }

    #[tokio::test]
    async fn test_version_increment() {
        let store = InMemoryConfigStore::new();

        store
            .set("app", "key", ConfigValue::new(serde_json::json!("v1")))
            .await
            .unwrap();
        store
            .set("app", "key", ConfigValue::new(serde_json::json!("v2")))
            .await
            .unwrap();

        let value = store.get("app", "key").await.unwrap().unwrap();
        assert_eq!(value.version, 2);
    }

    #[tokio::test]
    async fn test_layered_store() {
        let base = Arc::new(InMemoryConfigStore::new());
        let overlay = Arc::new(InMemoryConfigStore::new());

        base.set("app", "key", ConfigValue::new(serde_json::json!("base")))
            .await
            .unwrap();
        overlay
            .set("app", "key", ConfigValue::new(serde_json::json!("overlay")))
            .await
            .unwrap();

        let layered = LayeredConfigStore::new().add_layer(base).add_layer(overlay);

        let value = layered.get("app", "key").await.unwrap().unwrap();
        assert_eq!(value.as_string(), Some("overlay".to_string()));
    }

    #[tokio::test]
    async fn test_config_service() {
        let store = Arc::new(InMemoryConfigStore::new());
        let service = ConfigService::new(store);

        service
            .set("app", "name", ConfigValue::new(serde_json::json!("MyApp")))
            .await
            .unwrap();

        let name = service.get_string("app", "name").await;
        assert_eq!(name, Some("MyApp".to_string()));
    }

    #[tokio::test]
    async fn test_list_keys() {
        let store = InMemoryConfigStore::new();

        store
            .set("app", "key1", ConfigValue::new(serde_json::json!("v1")))
            .await
            .unwrap();
        store
            .set("app", "key2", ConfigValue::new(serde_json::json!("v2")))
            .await
            .unwrap();

        let keys = store.list_keys("app").await.unwrap();
        assert_eq!(keys.len(), 2);
    }

    #[tokio::test]
    async fn test_namespaces() {
        let store = InMemoryConfigStore::new();

        store
            .create_namespace(Namespace::new("production"))
            .await
            .unwrap();
        store
            .create_namespace(Namespace::new("staging"))
            .await
            .unwrap();

        let namespaces = store.list_namespaces().await.unwrap();
        assert_eq!(namespaces.len(), 2);
    }
}
