//! Feature flag management for drbot.
//!
//! This crate provides:
//! - Feature flag definitions
//! - Percentage rollouts
//! - User targeting
//! - A/B testing support

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

/// Feature flag error types.
#[derive(Error, Debug)]
pub enum FeatureFlagError {
    #[error("Flag not found: {0}")]
    FlagNotFound(String),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("Storage error: {0}")]
    StorageError(String),
}

/// Result type for feature flag operations.
pub type Result<T> = std::result::Result<T, FeatureFlagError>;

/// Feature flag state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FlagState {
    /// Flag is enabled.
    Enabled,
    /// Flag is disabled.
    Disabled,
    /// Flag uses rules for evaluation.
    Conditional,
}

/// Targeting rule type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TargetingRule {
    /// Target specific users.
    UserIds(HashSet<String>),
    /// Target percentage of users.
    Percentage(u8),
    /// Target by attribute.
    Attribute {
        key: String,
        values: HashSet<String>,
    },
    /// Target by environment.
    Environment(String),
    /// Combine rules with AND.
    All(Vec<TargetingRule>),
    /// Combine rules with OR.
    Any(Vec<TargetingRule>),
}

impl TargetingRule {
    /// Evaluate rule against context.
    pub fn evaluate(&self, context: &EvaluationContext) -> bool {
        match self {
            TargetingRule::UserIds(ids) => context
                .user_id
                .as_ref()
                .map(|id| ids.contains(id))
                .unwrap_or(false),
            TargetingRule::Percentage(pct) => {
                // Consistent hashing for user
                if let Some(user_id) = &context.user_id {
                    let hash = Self::hash_user(user_id);
                    (hash % 100) < *pct as u64
                } else {
                    false
                }
            }
            TargetingRule::Attribute { key, values } => context
                .attributes
                .get(key)
                .map(|v| values.contains(v))
                .unwrap_or(false),
            TargetingRule::Environment(env) => context.environment.as_ref() == Some(env),
            TargetingRule::All(rules) => rules.iter().all(|r| r.evaluate(context)),
            TargetingRule::Any(rules) => rules.iter().any(|r| r.evaluate(context)),
        }
    }

    fn hash_user(user_id: &str) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        user_id.hash(&mut hasher);
        hasher.finish()
    }
}

/// Feature flag definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureFlag {
    /// Flag key.
    pub key: String,
    /// Flag name.
    pub name: String,
    /// Description.
    pub description: Option<String>,
    /// Current state.
    pub state: FlagState,
    /// Targeting rules (when state is Conditional).
    pub rules: Vec<TargetingRule>,
    /// Default value when no rules match.
    pub default_value: bool,
    /// Flag variants for A/B testing.
    pub variants: HashMap<String, serde_json::Value>,
    /// Tags.
    pub tags: Vec<String>,
    /// Created at.
    pub created_at: DateTime<Utc>,
    /// Updated at.
    pub updated_at: DateTime<Utc>,
    /// Created by.
    pub created_by: Option<String>,
}

impl FeatureFlag {
    /// Create a new feature flag.
    pub fn new(key: impl Into<String>, name: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            key: key.into(),
            name: name.into(),
            description: None,
            state: FlagState::Disabled,
            rules: Vec::new(),
            default_value: false,
            variants: HashMap::new(),
            tags: Vec::new(),
            created_at: now,
            updated_at: now,
            created_by: None,
        }
    }

    /// Set state.
    pub fn with_state(mut self, state: FlagState) -> Self {
        self.state = state;
        self
    }

    /// Add rule.
    pub fn with_rule(mut self, rule: TargetingRule) -> Self {
        self.rules.push(rule);
        self
    }

    /// Set default value.
    pub fn with_default(mut self, value: bool) -> Self {
        self.default_value = value;
        self
    }

    /// Add variant.
    pub fn with_variant(mut self, name: impl Into<String>, value: serde_json::Value) -> Self {
        self.variants.insert(name.into(), value);
        self
    }

    /// Evaluate flag for context.
    pub fn evaluate(&self, context: &EvaluationContext) -> bool {
        match self.state {
            FlagState::Enabled => true,
            FlagState::Disabled => false,
            FlagState::Conditional => {
                for rule in &self.rules {
                    if rule.evaluate(context) {
                        return true;
                    }
                }
                self.default_value
            }
        }
    }
}

/// Evaluation context.
#[derive(Debug, Clone, Default)]
pub struct EvaluationContext {
    /// User ID.
    pub user_id: Option<String>,
    /// Environment.
    pub environment: Option<String>,
    /// Custom attributes.
    pub attributes: HashMap<String, String>,
}

impl EvaluationContext {
    /// Create a new context.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set user ID.
    pub fn with_user_id(mut self, user_id: impl Into<String>) -> Self {
        self.user_id = Some(user_id.into());
        self
    }

    /// Set environment.
    pub fn with_environment(mut self, env: impl Into<String>) -> Self {
        self.environment = Some(env.into());
        self
    }

    /// Add attribute.
    pub fn with_attribute(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.attributes.insert(key.into(), value.into());
        self
    }
}

/// Feature flag storage trait.
#[async_trait]
pub trait FlagStorage: Send + Sync {
    /// Get a flag by key.
    async fn get(&self, key: &str) -> Result<Option<FeatureFlag>>;

    /// Save a flag.
    async fn save(&self, flag: FeatureFlag) -> Result<()>;

    /// Delete a flag.
    async fn delete(&self, key: &str) -> Result<()>;

    /// List all flags.
    async fn list(&self) -> Result<Vec<FeatureFlag>>;

    /// List flags by tag.
    async fn list_by_tag(&self, tag: &str) -> Result<Vec<FeatureFlag>>;
}

/// In-memory flag storage.
pub struct InMemoryFlagStorage {
    flags: RwLock<HashMap<String, FeatureFlag>>,
}

impl InMemoryFlagStorage {
    /// Create new storage.
    pub fn new() -> Self {
        Self {
            flags: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for InMemoryFlagStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl FlagStorage for InMemoryFlagStorage {
    async fn get(&self, key: &str) -> Result<Option<FeatureFlag>> {
        let flags = self.flags.read().await;
        Ok(flags.get(key).cloned())
    }

    async fn save(&self, flag: FeatureFlag) -> Result<()> {
        let mut flags = self.flags.write().await;
        flags.insert(flag.key.clone(), flag);
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<()> {
        let mut flags = self.flags.write().await;
        flags.remove(key);
        Ok(())
    }

    async fn list(&self) -> Result<Vec<FeatureFlag>> {
        let flags = self.flags.read().await;
        Ok(flags.values().cloned().collect())
    }

    async fn list_by_tag(&self, tag: &str) -> Result<Vec<FeatureFlag>> {
        let flags = self.flags.read().await;
        Ok(flags
            .values()
            .filter(|f| f.tags.contains(&tag.to_string()))
            .cloned()
            .collect())
    }
}

/// Feature flag change event.
#[derive(Debug, Clone)]
pub enum FlagChangeEvent {
    /// Flag was created.
    Created(FeatureFlag),
    /// Flag was updated.
    Updated(FeatureFlag),
    /// Flag was deleted.
    Deleted(String),
}

/// Feature flag service.
pub struct FeatureFlagService<S: FlagStorage> {
    storage: Arc<S>,
    cache: RwLock<HashMap<String, FeatureFlag>>,
    change_tx: broadcast::Sender<FlagChangeEvent>,
}

impl<S: FlagStorage> FeatureFlagService<S> {
    /// Create a new service.
    pub fn new(storage: Arc<S>) -> Self {
        let (change_tx, _) = broadcast::channel(100);
        Self {
            storage,
            cache: RwLock::new(HashMap::new()),
            change_tx,
        }
    }

    /// Subscribe to changes.
    pub fn subscribe(&self) -> broadcast::Receiver<FlagChangeEvent> {
        self.change_tx.subscribe()
    }

    /// Check if flag is enabled.
    pub async fn is_enabled(&self, key: &str, context: &EvaluationContext) -> bool {
        // Check cache first
        {
            let cache = self.cache.read().await;
            if let Some(flag) = cache.get(key) {
                return flag.evaluate(context);
            }
        }

        // Load from storage
        if let Ok(Some(flag)) = self.storage.get(key).await {
            let result = flag.evaluate(context);

            // Cache it
            {
                let mut cache = self.cache.write().await;
                cache.insert(key.to_string(), flag);
            }

            return result;
        }

        false // Default to disabled if not found
    }

    /// Get variant value.
    pub async fn get_variant(&self, key: &str, variant: &str) -> Option<serde_json::Value> {
        if let Ok(Some(flag)) = self.storage.get(key).await {
            return flag.variants.get(variant).cloned();
        }
        None
    }

    /// Create or update a flag.
    pub async fn save(&self, flag: FeatureFlag) -> Result<()> {
        let is_new = self.storage.get(&flag.key).await?.is_none();
        self.storage.save(flag.clone()).await?;

        // Update cache
        {
            let mut cache = self.cache.write().await;
            cache.insert(flag.key.clone(), flag.clone());
        }

        // Emit event
        let event = if is_new {
            FlagChangeEvent::Created(flag)
        } else {
            FlagChangeEvent::Updated(flag)
        };
        let _ = self.change_tx.send(event);

        Ok(())
    }

    /// Delete a flag.
    pub async fn delete(&self, key: &str) -> Result<()> {
        self.storage.delete(key).await?;

        // Remove from cache
        {
            let mut cache = self.cache.write().await;
            cache.remove(key);
        }

        let _ = self
            .change_tx
            .send(FlagChangeEvent::Deleted(key.to_string()));
        Ok(())
    }

    /// List all flags.
    pub async fn list(&self) -> Result<Vec<FeatureFlag>> {
        self.storage.list().await
    }

    /// Invalidate cache.
    pub async fn invalidate_cache(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_flag_creation() {
        let flag = FeatureFlag::new("new_feature", "New Feature").with_state(FlagState::Enabled);

        assert_eq!(flag.key, "new_feature");
        assert_eq!(flag.state, FlagState::Enabled);
    }

    #[test]
    fn test_evaluate_enabled() {
        let flag = FeatureFlag::new("test", "Test").with_state(FlagState::Enabled);

        let context = EvaluationContext::new();
        assert!(flag.evaluate(&context));
    }

    #[test]
    fn test_evaluate_disabled() {
        let flag = FeatureFlag::new("test", "Test").with_state(FlagState::Disabled);

        let context = EvaluationContext::new();
        assert!(!flag.evaluate(&context));
    }

    #[test]
    fn test_user_targeting() {
        let mut user_ids = HashSet::new();
        user_ids.insert("user-123".to_string());

        let flag = FeatureFlag::new("test", "Test")
            .with_state(FlagState::Conditional)
            .with_rule(TargetingRule::UserIds(user_ids));

        let matching = EvaluationContext::new().with_user_id("user-123");
        let non_matching = EvaluationContext::new().with_user_id("user-456");

        assert!(flag.evaluate(&matching));
        assert!(!flag.evaluate(&non_matching));
    }

    #[test]
    fn test_percentage_rollout() {
        let flag = FeatureFlag::new("test", "Test")
            .with_state(FlagState::Conditional)
            .with_rule(TargetingRule::Percentage(50));

        // With 50% rollout, roughly half should be enabled
        let mut enabled = 0;
        for i in 0..100 {
            let context = EvaluationContext::new().with_user_id(format!("user-{}", i));
            if flag.evaluate(&context) {
                enabled += 1;
            }
        }

        // Should be roughly 50% (allow some variance)
        assert!(enabled > 30 && enabled < 70);
    }

    #[test]
    fn test_attribute_targeting() {
        let mut values = HashSet::new();
        values.insert("premium".to_string());

        let flag = FeatureFlag::new("test", "Test")
            .with_state(FlagState::Conditional)
            .with_rule(TargetingRule::Attribute {
                key: "plan".to_string(),
                values,
            });

        let premium = EvaluationContext::new().with_attribute("plan", "premium");
        let free = EvaluationContext::new().with_attribute("plan", "free");

        assert!(flag.evaluate(&premium));
        assert!(!flag.evaluate(&free));
    }

    #[test]
    fn test_environment_targeting() {
        let flag = FeatureFlag::new("test", "Test")
            .with_state(FlagState::Conditional)
            .with_rule(TargetingRule::Environment("production".to_string()));

        let prod = EvaluationContext::new().with_environment("production");
        let dev = EvaluationContext::new().with_environment("development");

        assert!(flag.evaluate(&prod));
        assert!(!flag.evaluate(&dev));
    }

    #[tokio::test]
    async fn test_flag_service() {
        let storage = Arc::new(InMemoryFlagStorage::new());
        let service = FeatureFlagService::new(storage);

        let flag = FeatureFlag::new("test", "Test").with_state(FlagState::Enabled);

        service.save(flag).await.unwrap();

        let context = EvaluationContext::new();
        assert!(service.is_enabled("test", &context).await);
    }

    #[tokio::test]
    async fn test_flag_delete() {
        let storage = Arc::new(InMemoryFlagStorage::new());
        let service = FeatureFlagService::new(storage);

        let flag = FeatureFlag::new("test", "Test").with_state(FlagState::Enabled);

        service.save(flag).await.unwrap();
        service.delete("test").await.unwrap();

        let context = EvaluationContext::new();
        assert!(!service.is_enabled("test", &context).await);
    }

    #[test]
    fn test_variants() {
        let flag = FeatureFlag::new("test", "Test")
            .with_variant("control", serde_json::json!({"color": "blue"}))
            .with_variant("treatment", serde_json::json!({"color": "green"}));

        assert_eq!(flag.variants.len(), 2);
    }
}
