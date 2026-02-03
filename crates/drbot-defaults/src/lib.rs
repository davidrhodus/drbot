//! Smart defaults for drbot.
//!
//! Learn and apply intelligent default values.
//!
//! # Features
//!
//! - Usage-based defaults
//! - Context-aware suggestions
//! - User preference learning
//! - Default inheritance

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Smart defaults result type.
pub type Result<T> = std::result::Result<T, DefaultsError>;

/// Defaults errors.
#[derive(Debug, thiserror::Error)]
pub enum DefaultsError {
    #[error("Default not found: {0}")]
    NotFound(String),
    #[error("Invalid value: {0}")]
    InvalidValue(String),
    #[error("Conflict: {0}")]
    Conflict(String),
}

/// A smart default value.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmartDefault {
    /// Default ID.
    pub id: Uuid,
    /// Key/name.
    pub key: String,
    /// Value.
    pub value: serde_json::Value,
    /// Value type.
    pub value_type: ValueType,
    /// Source of this default.
    pub source: DefaultSource,
    /// Confidence (0-1).
    pub confidence: f32,
    /// Usage count.
    pub usage_count: u64,
    /// Override count.
    pub override_count: u64,
    /// Context.
    pub context: Option<String>,
    /// Created at.
    pub created_at: DateTime<Utc>,
    /// Updated at.
    pub updated_at: DateTime<Utc>,
}

impl SmartDefault {
    /// Create a new default.
    pub fn new(key: &str, value: serde_json::Value) -> Self {
        let value_type = match &value {
            serde_json::Value::String(_) => ValueType::String,
            serde_json::Value::Number(_) => ValueType::Number,
            serde_json::Value::Bool(_) => ValueType::Boolean,
            serde_json::Value::Array(_) => ValueType::Array,
            serde_json::Value::Object(_) => ValueType::Object,
            serde_json::Value::Null => ValueType::String,
        };

        Self {
            id: Uuid::new_v4(),
            key: key.to_string(),
            value,
            value_type,
            source: DefaultSource::System,
            confidence: 0.5,
            usage_count: 0,
            override_count: 0,
            context: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    /// Set source.
    pub fn with_source(mut self, source: DefaultSource) -> Self {
        self.source = source;
        self
    }

    /// Set context.
    pub fn with_context(mut self, context: &str) -> Self {
        self.context = Some(context.to_string());
        self
    }

    /// Record usage.
    pub fn record_usage(&mut self, was_overridden: bool) {
        self.usage_count += 1;
        if was_overridden {
            self.override_count += 1;
        }

        // Update confidence based on override rate
        let override_rate = self.override_count as f32 / self.usage_count as f32;
        self.confidence = 1.0 - override_rate;
        self.updated_at = Utc::now();
    }

    /// Get override rate.
    pub fn override_rate(&self) -> f32 {
        if self.usage_count == 0 {
            0.0
        } else {
            self.override_count as f32 / self.usage_count as f32
        }
    }
}

/// Value types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValueType {
    String,
    Number,
    Boolean,
    Array,
    Object,
}

/// Default source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DefaultSource {
    /// System-defined.
    System,
    /// User-defined.
    User,
    /// Learned from usage.
    Learned,
    /// Inherited from parent context.
    Inherited,
}

/// Default suggestion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefaultSuggestion {
    /// Key.
    pub key: String,
    /// Suggested value.
    pub value: serde_json::Value,
    /// Confidence.
    pub confidence: f32,
    /// Reason.
    pub reason: String,
}

/// Defaults configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefaultsConfig {
    /// Enable learning.
    pub learning_enabled: bool,
    /// Minimum confidence to suggest.
    pub min_confidence: f32,
    /// Maximum defaults per key.
    pub max_per_key: usize,
    /// Enable context-aware defaults.
    pub context_aware: bool,
}

impl Default for DefaultsConfig {
    fn default() -> Self {
        Self {
            learning_enabled: true,
            min_confidence: 0.6,
            max_per_key: 5,
            context_aware: true,
        }
    }
}

/// Defaults scope.
#[derive(Debug, Clone, Default)]
pub struct DefaultsScope {
    /// Scope name.
    pub name: String,
    /// Parent scope.
    pub parent: Option<String>,
    /// Values.
    pub values: HashMap<String, serde_json::Value>,
}

impl DefaultsScope {
    /// Create a new scope.
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            parent: None,
            values: HashMap::new(),
        }
    }

    /// With parent.
    pub fn with_parent(mut self, parent: &str) -> Self {
        self.parent = Some(parent.to_string());
        self
    }
}

/// Smart defaults manager.
pub struct SmartDefaultsManager {
    config: DefaultsConfig,
    defaults: Arc<RwLock<HashMap<String, Vec<SmartDefault>>>>,
    scopes: Arc<RwLock<HashMap<String, DefaultsScope>>>,
    usage_history: Arc<RwLock<Vec<UsageRecord>>>,
}

impl SmartDefaultsManager {
    /// Create a new manager.
    pub fn new(config: DefaultsConfig) -> Self {
        Self {
            config,
            defaults: Arc::new(RwLock::new(HashMap::new())),
            scopes: Arc::new(RwLock::new(HashMap::new())),
            usage_history: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Register a default.
    pub async fn register(&self, default: SmartDefault) {
        let mut defaults = self.defaults.write().await;
        let entry = defaults.entry(default.key.clone()).or_insert_with(Vec::new);

        // Check for existing default with same context
        if let Some(existing) = entry.iter_mut().find(|d| d.context == default.context) {
            *existing = default;
        } else {
            entry.push(default);
        }

        // Limit entries
        if entry.len() > self.config.max_per_key {
            entry.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());
            entry.truncate(self.config.max_per_key);
        }
    }

    /// Get default value.
    pub async fn get(&self, key: &str, context: Option<&str>) -> Option<serde_json::Value> {
        let defaults = self.defaults.read().await;

        if let Some(entries) = defaults.get(key) {
            // Try context-specific first
            if let Some(ctx) = context {
                if let Some(d) = entries.iter().find(|d| d.context.as_deref() == Some(ctx)) {
                    return Some(d.value.clone());
                }
            }

            // Fall back to highest confidence global default (no context)
            entries
                .iter()
                .filter(|d| d.confidence >= self.config.min_confidence && d.context.is_none())
                .max_by(|a, b| a.confidence.partial_cmp(&b.confidence).unwrap())
                .map(|d| d.value.clone())
        } else {
            None
        }
    }

    /// Get default with metadata.
    pub async fn get_with_meta(&self, key: &str, context: Option<&str>) -> Option<SmartDefault> {
        let defaults = self.defaults.read().await;

        if let Some(entries) = defaults.get(key) {
            if let Some(ctx) = context {
                if let Some(d) = entries.iter().find(|d| d.context.as_deref() == Some(ctx)) {
                    return Some(d.clone());
                }
            }

            entries
                .iter()
                .filter(|d| d.confidence >= self.config.min_confidence && d.context.is_none())
                .max_by(|a, b| a.confidence.partial_cmp(&b.confidence).unwrap())
                .cloned()
        } else {
            None
        }
    }

    /// Record usage of a default.
    pub async fn record_usage(&self, key: &str, value: &serde_json::Value, context: Option<&str>) {
        let mut defaults = self.defaults.write().await;

        if let Some(entries) = defaults.get_mut(key) {
            for entry in entries.iter_mut() {
                if entry.context.as_deref() == context {
                    let was_overridden = &entry.value != value;
                    entry.record_usage(was_overridden);
                    break;
                }
            }
        }

        // Record in history
        if self.config.learning_enabled {
            self.usage_history.write().await.push(UsageRecord {
                key: key.to_string(),
                value: value.clone(),
                context: context.map(String::from),
                timestamp: Utc::now(),
            });
        }
    }

    /// Learn defaults from usage history.
    pub async fn learn_from_usage(&self) -> Vec<SmartDefault> {
        if !self.config.learning_enabled {
            return Vec::new();
        }

        let history = self.usage_history.read().await;
        let mut learned = Vec::new();

        // Group by key and context
        let mut grouped: HashMap<(String, Option<String>), Vec<&UsageRecord>> = HashMap::new();
        for record in history.iter() {
            grouped
                .entry((record.key.clone(), record.context.clone()))
                .or_default()
                .push(record);
        }

        // Find common values
        for ((key, context), records) in grouped {
            if records.len() < 3 {
                continue;
            }

            // Count value occurrences
            let mut value_counts: HashMap<String, usize> = HashMap::new();
            for record in &records {
                let key = serde_json::to_string(&record.value).unwrap_or_default();
                *value_counts.entry(key).or_insert(0) += 1;
            }

            // Find most common value
            if let Some((value_str, count)) = value_counts.iter().max_by_key(|(_, c)| *c) {
                let confidence = *count as f32 / records.len() as f32;
                if confidence >= self.config.min_confidence {
                    if let Ok(value) = serde_json::from_str(value_str) {
                        let mut default =
                            SmartDefault::new(&key, value).with_source(DefaultSource::Learned);
                        default.confidence = confidence;
                        if let Some(ctx) = context {
                            default = default.with_context(&ctx);
                        }
                        learned.push(default);
                    }
                }
            }
        }

        // Register learned defaults
        for default in &learned {
            self.register(default.clone()).await;
        }

        learned
    }

    /// Get suggestions for a key.
    pub async fn suggest(&self, key: &str, context: Option<&str>) -> Vec<DefaultSuggestion> {
        let defaults = self.defaults.read().await;

        if let Some(entries) = defaults.get(key) {
            entries
                .iter()
                .filter(|d| d.confidence >= self.config.min_confidence)
                .filter(|d| {
                    context.is_none() || d.context.is_none() || d.context.as_deref() == context
                })
                .map(|d| DefaultSuggestion {
                    key: key.to_string(),
                    value: d.value.clone(),
                    confidence: d.confidence,
                    reason: format!("{:?} default, used {} times", d.source, d.usage_count),
                })
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Register a scope.
    pub async fn register_scope(&self, scope: DefaultsScope) {
        self.scopes.write().await.insert(scope.name.clone(), scope);
    }

    /// Get value with scope inheritance.
    pub async fn get_from_scope(&self, key: &str, scope_name: &str) -> Option<serde_json::Value> {
        let scopes = self.scopes.read().await;

        let mut current = scope_name;
        loop {
            if let Some(scope) = scopes.get(current) {
                if let Some(value) = scope.values.get(key) {
                    return Some(value.clone());
                }
                if let Some(parent) = &scope.parent {
                    current = parent;
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        // Fall back to defaults
        drop(scopes);
        self.get(key, Some(scope_name)).await
    }

    /// List all defaults.
    pub async fn list_all(&self) -> Vec<SmartDefault> {
        self.defaults
            .read()
            .await
            .values()
            .flatten()
            .cloned()
            .collect()
    }

    /// Get statistics.
    pub async fn stats(&self) -> DefaultsStats {
        let defaults = self.defaults.read().await;
        let history = self.usage_history.read().await;

        let total_defaults: usize = defaults.values().map(|v| v.len()).sum();
        let total_usage: u64 = defaults.values().flatten().map(|d| d.usage_count).sum();

        let avg_confidence = if total_defaults > 0 {
            defaults
                .values()
                .flatten()
                .map(|d| d.confidence)
                .sum::<f32>()
                / total_defaults as f32
        } else {
            0.0
        };

        DefaultsStats {
            total_keys: defaults.len(),
            total_defaults,
            total_usage,
            avg_confidence,
            history_size: history.len(),
        }
    }
}

/// Usage record.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct UsageRecord {
    key: String,
    value: serde_json::Value,
    context: Option<String>,
    timestamp: DateTime<Utc>,
}

/// Defaults statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefaultsStats {
    pub total_keys: usize,
    pub total_defaults: usize,
    pub total_usage: u64,
    pub avg_confidence: f32,
    pub history_size: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_register_and_get() {
        let config = DefaultsConfig {
            min_confidence: 0.4, // Lower threshold to accept default confidence
            ..Default::default()
        };
        let manager = SmartDefaultsManager::new(config);

        let default = SmartDefault::new("language", serde_json::json!("en"));
        manager.register(default).await;

        let value = manager.get("language", None).await;
        assert!(value.is_some());
        assert_eq!(value.unwrap(), serde_json::json!("en"));
    }

    #[tokio::test]
    async fn test_context_aware() {
        let config = DefaultsConfig {
            min_confidence: 0.4, // Lower threshold to accept default confidence
            ..Default::default()
        };
        let manager = SmartDefaultsManager::new(config);

        let global = SmartDefault::new("theme", serde_json::json!("light"));
        let dark_ctx =
            SmartDefault::new("theme", serde_json::json!("dark")).with_context("night_mode");

        manager.register(global).await;
        manager.register(dark_ctx).await;

        let value = manager.get("theme", Some("night_mode")).await;
        assert_eq!(value.unwrap(), serde_json::json!("dark"));

        let value = manager.get("theme", None).await;
        assert_eq!(value.unwrap(), serde_json::json!("light"));
    }

    #[tokio::test]
    async fn test_usage_recording() {
        let manager = SmartDefaultsManager::new(DefaultsConfig::default());

        let mut default = SmartDefault::new("size", serde_json::json!(10));
        default.confidence = 0.8;
        manager.register(default).await;

        // Use without override
        manager
            .record_usage("size", &serde_json::json!(10), None)
            .await;

        let d = manager.get_with_meta("size", None).await.unwrap();
        assert_eq!(d.usage_count, 1);
        assert_eq!(d.override_count, 0);
    }

    #[tokio::test]
    async fn test_scope_inheritance() {
        let manager = SmartDefaultsManager::new(DefaultsConfig::default());

        let mut parent = DefaultsScope::new("global");
        parent
            .values
            .insert("color".to_string(), serde_json::json!("blue"));

        let child = DefaultsScope::new("user").with_parent("global");

        manager.register_scope(parent).await;
        manager.register_scope(child).await;

        let value = manager.get_from_scope("color", "user").await;
        assert_eq!(value.unwrap(), serde_json::json!("blue"));
    }
}
