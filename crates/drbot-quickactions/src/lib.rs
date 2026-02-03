//! Quick actions for drbot.
//!
//! One-click common operations.
//!
//! # Features
//!
//! - Predefined actions
//! - Custom action creation
//! - Context-aware suggestions
//! - Usage analytics

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Quick actions result type.
pub type Result<T> = std::result::Result<T, ActionError>;

/// Action errors.
#[derive(Debug, thiserror::Error)]
pub enum ActionError {
    #[error("Action not found: {0}")]
    NotFound(String),
    #[error("Execution failed: {0}")]
    ExecutionFailed(String),
    #[error("Invalid parameters: {0}")]
    InvalidParams(String),
    #[error("Action disabled")]
    Disabled,
}

/// A quick action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuickAction {
    /// Action ID.
    pub id: Uuid,
    /// Action name.
    pub name: String,
    /// Display label.
    pub label: String,
    /// Description.
    pub description: String,
    /// Icon.
    pub icon: Option<String>,
    /// Category.
    pub category: String,
    /// Parameters.
    pub parameters: Vec<ActionParam>,
    /// Keyboard shortcut.
    pub shortcut: Option<String>,
    /// Enabled.
    pub enabled: bool,
    /// Usage count.
    pub usage_count: u64,
    /// Last used.
    pub last_used: Option<DateTime<Utc>>,
    /// Created at.
    pub created_at: DateTime<Utc>,
    /// Metadata.
    pub metadata: HashMap<String, serde_json::Value>,
}

impl QuickAction {
    /// Create a new action.
    pub fn new(name: &str, label: &str, category: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.to_string(),
            label: label.to_string(),
            description: String::new(),
            icon: None,
            category: category.to_string(),
            parameters: Vec::new(),
            shortcut: None,
            enabled: true,
            usage_count: 0,
            last_used: None,
            created_at: Utc::now(),
            metadata: HashMap::new(),
        }
    }

    /// Set description.
    pub fn with_description(mut self, description: &str) -> Self {
        self.description = description.to_string();
        self
    }

    /// Set icon.
    pub fn with_icon(mut self, icon: &str) -> Self {
        self.icon = Some(icon.to_string());
        self
    }

    /// Add parameter.
    pub fn with_param(mut self, param: ActionParam) -> Self {
        self.parameters.push(param);
        self
    }

    /// Set shortcut.
    pub fn with_shortcut(mut self, shortcut: &str) -> Self {
        self.shortcut = Some(shortcut.to_string());
        self
    }

    /// Record usage.
    pub fn record_usage(&mut self) {
        self.usage_count += 1;
        self.last_used = Some(Utc::now());
    }
}

/// Action parameter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionParam {
    /// Parameter name.
    pub name: String,
    /// Parameter type.
    pub param_type: ParamType,
    /// Label.
    pub label: String,
    /// Required.
    pub required: bool,
    /// Default value.
    pub default: Option<serde_json::Value>,
    /// Options (for select type).
    pub options: Option<Vec<String>>,
}

impl ActionParam {
    /// Create a new parameter.
    pub fn new(name: &str, param_type: ParamType, label: &str) -> Self {
        Self {
            name: name.to_string(),
            param_type,
            label: label.to_string(),
            required: false,
            default: None,
            options: None,
        }
    }

    /// Set as required.
    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    /// Set default value.
    pub fn with_default(mut self, default: serde_json::Value) -> Self {
        self.default = Some(default);
        self
    }

    /// Set options.
    pub fn with_options(mut self, options: Vec<String>) -> Self {
        self.options = Some(options);
        self
    }
}

/// Parameter types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParamType {
    String,
    Number,
    Boolean,
    Select,
    File,
    Date,
}

/// Action execution result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionResult {
    /// Result ID.
    pub id: Uuid,
    /// Action ID.
    pub action_id: Uuid,
    /// Success.
    pub success: bool,
    /// Output.
    pub output: serde_json::Value,
    /// Error message.
    pub error: Option<String>,
    /// Duration in ms.
    pub duration_ms: u64,
    /// Executed at.
    pub executed_at: DateTime<Utc>,
}

/// Quick actions configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuickActionConfig {
    /// Maximum recent actions.
    pub max_recent: usize,
    /// Enable suggestions.
    pub suggestions_enabled: bool,
    /// Track usage.
    pub track_usage: bool,
}

impl Default for QuickActionConfig {
    fn default() -> Self {
        Self {
            max_recent: 10,
            suggestions_enabled: true,
            track_usage: true,
        }
    }
}

/// Context for action suggestions.
#[derive(Debug, Clone, Default)]
pub struct ActionContext {
    /// Current screen/view.
    pub screen: Option<String>,
    /// Selected items.
    pub selected: Vec<String>,
    /// User preferences.
    pub preferences: HashMap<String, String>,
    /// Custom context.
    pub custom: HashMap<String, serde_json::Value>,
}

impl ActionContext {
    /// Create new context.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set screen.
    pub fn with_screen(mut self, screen: &str) -> Self {
        self.screen = Some(screen.to_string());
        self
    }

    /// Add selected item.
    pub fn with_selected(mut self, item: &str) -> Self {
        self.selected.push(item.to_string());
        self
    }
}

/// Trait for action executors.
#[async_trait]
pub trait ActionExecutor: Send + Sync {
    /// Execute an action.
    async fn execute(
        &self,
        action: &QuickAction,
        params: &HashMap<String, serde_json::Value>,
    ) -> Result<serde_json::Value>;
}

/// Quick actions manager.
pub struct QuickActionManager<E: ActionExecutor> {
    config: QuickActionConfig,
    executor: E,
    actions: Arc<RwLock<HashMap<String, QuickAction>>>,
    recent: Arc<RwLock<Vec<Uuid>>>,
    results: Arc<RwLock<Vec<ActionResult>>>,
}

impl<E: ActionExecutor> QuickActionManager<E> {
    /// Create a new manager.
    pub fn new(config: QuickActionConfig, executor: E) -> Self {
        Self {
            config,
            executor,
            actions: Arc::new(RwLock::new(HashMap::new())),
            recent: Arc::new(RwLock::new(Vec::new())),
            results: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Register an action.
    pub async fn register(&self, action: QuickAction) {
        self.actions
            .write()
            .await
            .insert(action.name.clone(), action);
    }

    /// Execute an action by name.
    pub async fn execute(
        &self,
        name: &str,
        params: &HashMap<String, serde_json::Value>,
    ) -> Result<ActionResult> {
        let mut actions = self.actions.write().await;
        let action = actions
            .get_mut(name)
            .ok_or_else(|| ActionError::NotFound(name.to_string()))?;

        if !action.enabled {
            return Err(ActionError::Disabled);
        }

        // Validate parameters
        for param in &action.parameters {
            if param.required && !params.contains_key(&param.name) && param.default.is_none() {
                return Err(ActionError::InvalidParams(format!(
                    "Missing required parameter: {}",
                    param.name
                )));
            }
        }

        let start = std::time::Instant::now();

        let output = self.executor.execute(action, params).await;
        let duration_ms = start.elapsed().as_millis() as u64;

        let (success, output, error) = match output {
            Ok(o) => (true, o, None),
            Err(e) => (false, serde_json::Value::Null, Some(e.to_string())),
        };

        // Record usage
        if self.config.track_usage {
            action.record_usage();
        }

        let result = ActionResult {
            id: Uuid::new_v4(),
            action_id: action.id,
            success,
            output,
            error,
            duration_ms,
            executed_at: Utc::now(),
        };

        // Update recent
        let mut recent = self.recent.write().await;
        recent.retain(|&id| id != action.id);
        recent.insert(0, action.id);
        recent.truncate(self.config.max_recent);

        // Store result
        self.results.write().await.push(result.clone());

        Ok(result)
    }

    /// Get action by name.
    pub async fn get(&self, name: &str) -> Option<QuickAction> {
        self.actions.read().await.get(name).cloned()
    }

    /// List all actions.
    pub async fn list_all(&self) -> Vec<QuickAction> {
        self.actions.read().await.values().cloned().collect()
    }

    /// List actions by category.
    pub async fn list_by_category(&self, category: &str) -> Vec<QuickAction> {
        self.actions
            .read()
            .await
            .values()
            .filter(|a| a.category == category)
            .cloned()
            .collect()
    }

    /// Get recent actions.
    pub async fn get_recent(&self) -> Vec<QuickAction> {
        let recent = self.recent.read().await;
        let actions = self.actions.read().await;

        recent
            .iter()
            .filter_map(|id| actions.values().find(|a| a.id == *id).cloned())
            .collect()
    }

    /// Get suggested actions for context.
    pub async fn suggest(&self, context: &ActionContext) -> Vec<QuickAction> {
        if !self.config.suggestions_enabled {
            return Vec::new();
        }

        let actions = self.actions.read().await;
        let mut suggestions: Vec<_> = actions.values().filter(|a| a.enabled).cloned().collect();

        // Sort by relevance (usage count + recency)
        suggestions.sort_by(|a, b| {
            let score_a = a.usage_count as i64 + if a.last_used.is_some() { 10 } else { 0 };
            let score_b = b.usage_count as i64 + if b.last_used.is_some() { 10 } else { 0 };
            score_b.cmp(&score_a)
        });

        // Filter by context if available
        if let Some(screen) = &context.screen {
            suggestions.retain(|a| {
                a.metadata
                    .get("screens")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().any(|s| s.as_str() == Some(screen)))
                    .unwrap_or(true)
            });
        }

        suggestions.truncate(5);
        suggestions
    }

    /// Enable/disable action.
    pub async fn set_enabled(&self, name: &str, enabled: bool) -> Result<()> {
        let mut actions = self.actions.write().await;
        let action = actions
            .get_mut(name)
            .ok_or_else(|| ActionError::NotFound(name.to_string()))?;
        action.enabled = enabled;
        Ok(())
    }

    /// Get categories.
    pub async fn get_categories(&self) -> Vec<String> {
        let actions = self.actions.read().await;
        let mut categories: Vec<_> = actions.values().map(|a| a.category.clone()).collect();
        categories.sort();
        categories.dedup();
        categories
    }

    /// Get execution history.
    pub async fn history(&self, limit: usize) -> Vec<ActionResult> {
        let results = self.results.read().await;
        results.iter().rev().take(limit).cloned().collect()
    }

    /// Get statistics.
    pub async fn stats(&self) -> QuickActionStats {
        let actions = self.actions.read().await;
        let results = self.results.read().await;

        let total_usage: u64 = actions.values().map(|a| a.usage_count).sum();
        let successful = results.iter().filter(|r| r.success).count();

        QuickActionStats {
            total_actions: actions.len(),
            enabled_actions: actions.values().filter(|a| a.enabled).count(),
            total_executions: results.len(),
            successful_executions: successful,
            total_usage,
        }
    }
}

/// Quick action statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuickActionStats {
    pub total_actions: usize,
    pub enabled_actions: usize,
    pub total_executions: usize,
    pub successful_executions: usize,
    pub total_usage: u64,
}

/// Simple action executor.
pub struct SimpleExecutor;

#[async_trait]
impl ActionExecutor for SimpleExecutor {
    async fn execute(
        &self,
        action: &QuickAction,
        params: &HashMap<String, serde_json::Value>,
    ) -> Result<serde_json::Value> {
        // Simple executor that returns action info
        Ok(serde_json::json!({
            "action": action.name,
            "params": params,
            "message": format!("Executed action: {}", action.label)
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_action_registration() {
        let manager = QuickActionManager::new(QuickActionConfig::default(), SimpleExecutor);

        let action = QuickAction::new("copy", "Copy", "edit")
            .with_description("Copy selected item")
            .with_shortcut("Ctrl+C");

        manager.register(action).await;

        let retrieved = manager.get("copy").await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().label, "Copy");
    }

    #[tokio::test]
    async fn test_action_execution() {
        let manager = QuickActionManager::new(QuickActionConfig::default(), SimpleExecutor);

        let action = QuickAction::new("test", "Test Action", "general");
        manager.register(action).await;

        let params = HashMap::new();
        let result = manager.execute("test", &params).await.unwrap();

        assert!(result.success);
    }

    #[tokio::test]
    async fn test_required_params() {
        let manager = QuickActionManager::new(QuickActionConfig::default(), SimpleExecutor);

        let action = QuickAction::new("greet", "Greet", "general")
            .with_param(ActionParam::new("name", ParamType::String, "Name").required());

        manager.register(action).await;

        let params = HashMap::new();
        let result = manager.execute("greet", &params).await;

        assert!(matches!(result, Err(ActionError::InvalidParams(_))));
    }

    #[tokio::test]
    async fn test_recent_actions() {
        let manager = QuickActionManager::new(QuickActionConfig::default(), SimpleExecutor);

        manager
            .register(QuickAction::new("action1", "Action 1", "general"))
            .await;
        manager
            .register(QuickAction::new("action2", "Action 2", "general"))
            .await;

        manager.execute("action1", &HashMap::new()).await.unwrap();
        manager.execute("action2", &HashMap::new()).await.unwrap();

        let recent = manager.get_recent().await;
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].name, "action2"); // Most recent first
    }

    #[tokio::test]
    async fn test_disable_action() {
        let manager = QuickActionManager::new(QuickActionConfig::default(), SimpleExecutor);

        manager
            .register(QuickAction::new("test", "Test", "general"))
            .await;
        manager.set_enabled("test", false).await.unwrap();

        let result = manager.execute("test", &HashMap::new()).await;
        assert!(matches!(result, Err(ActionError::Disabled)));
    }
}
