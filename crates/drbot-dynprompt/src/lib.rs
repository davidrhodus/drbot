//! Dynamic system prompts for drbot.
//!
//! Adapt system prompts based on context and user.
//!
//! # Features
//!
//! - Context-aware prompt selection
//! - User preference integration
//! - A/B testing support
//! - Prompt versioning

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Dynamic prompt result type.
pub type Result<T> = std::result::Result<T, PromptError>;

/// Prompt errors.
#[derive(Debug, thiserror::Error)]
pub enum PromptError {
    #[error("Prompt not found: {0}")]
    PromptNotFound(String),
    #[error("No matching prompt for context")]
    NoMatch,
    #[error("Template error: {0}")]
    TemplateError(String),
    #[error("Condition parse error: {0}")]
    ConditionError(String),
}

/// A dynamic system prompt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DynamicPrompt {
    /// Prompt ID.
    pub id: Uuid,
    /// Prompt name.
    pub name: String,
    /// Prompt template.
    pub template: String,
    /// Variables in the template.
    pub variables: Vec<String>,
    /// Conditions for when to use this prompt.
    pub conditions: Vec<PromptCondition>,
    /// Priority (higher = preferred).
    pub priority: i32,
    /// Version.
    pub version: u32,
    /// Whether this is the default.
    pub is_default: bool,
    /// Created at.
    pub created_at: DateTime<Utc>,
    /// Updated at.
    pub updated_at: DateTime<Utc>,
    /// Metadata.
    pub metadata: HashMap<String, serde_json::Value>,
}

impl DynamicPrompt {
    /// Create a new dynamic prompt.
    pub fn new(name: &str, template: &str) -> Self {
        let variables = Self::extract_variables(template);
        Self {
            id: Uuid::new_v4(),
            name: name.to_string(),
            template: template.to_string(),
            variables,
            conditions: Vec::new(),
            priority: 0,
            version: 1,
            is_default: false,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            metadata: HashMap::new(),
        }
    }

    /// Extract variables from template.
    fn extract_variables(template: &str) -> Vec<String> {
        let mut vars = Vec::new();
        let mut in_var = false;
        let mut current = String::new();

        for c in template.chars() {
            if c == '{' && !in_var {
                in_var = true;
                current.clear();
            } else if c == '}' && in_var {
                in_var = false;
                if !current.is_empty() && !vars.contains(&current) {
                    vars.push(current.clone());
                }
            } else if in_var {
                current.push(c);
            }
        }

        vars
    }

    /// Add a condition.
    pub fn with_condition(mut self, condition: PromptCondition) -> Self {
        self.conditions.push(condition);
        self
    }

    /// Set priority.
    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    /// Mark as default.
    pub fn as_default(mut self) -> Self {
        self.is_default = true;
        self
    }

    /// Render the template with variables.
    pub fn render(&self, context: &PromptContext) -> Result<String> {
        let mut result = self.template.clone();

        for var in &self.variables {
            let placeholder = format!("{{{}}}", var);
            let value = context.variables.get(var).map(|v| v.as_str()).unwrap_or("");
            result = result.replace(&placeholder, value);
        }

        Ok(result)
    }

    /// Check if this prompt matches the context.
    pub fn matches(&self, context: &PromptContext) -> bool {
        if self.conditions.is_empty() {
            return self.is_default;
        }

        self.conditions.iter().all(|c| c.evaluate(context))
    }
}

/// Conditions for prompt selection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptCondition {
    /// Condition type.
    pub condition_type: ConditionType,
    /// Field to check.
    pub field: String,
    /// Expected value or pattern.
    pub value: String,
    /// Negate the condition.
    pub negate: bool,
}

impl PromptCondition {
    /// Create a new condition.
    pub fn new(condition_type: ConditionType, field: &str, value: &str) -> Self {
        Self {
            condition_type,
            field: field.to_string(),
            value: value.to_string(),
            negate: false,
        }
    }

    /// Negate the condition.
    pub fn not(mut self) -> Self {
        self.negate = true;
        self
    }

    /// Evaluate the condition against a context.
    pub fn evaluate(&self, context: &PromptContext) -> bool {
        let result = match self.condition_type {
            ConditionType::Equals => context
                .variables
                .get(&self.field)
                .map(|v| v == &self.value)
                .unwrap_or(false),
            ConditionType::Contains => context
                .variables
                .get(&self.field)
                .map(|v| v.contains(&self.value))
                .unwrap_or(false),
            ConditionType::StartsWith => context
                .variables
                .get(&self.field)
                .map(|v| v.starts_with(&self.value))
                .unwrap_or(false),
            ConditionType::EndsWith => context
                .variables
                .get(&self.field)
                .map(|v| v.ends_with(&self.value))
                .unwrap_or(false),
            ConditionType::Exists => context.variables.contains_key(&self.field),
            ConditionType::InList => {
                let values: Vec<_> = self.value.split(',').map(|s| s.trim()).collect();
                context
                    .variables
                    .get(&self.field)
                    .map(|v| values.contains(&v.as_str()))
                    .unwrap_or(false)
            }
            ConditionType::GreaterThan => context
                .variables
                .get(&self.field)
                .and_then(|v| v.parse::<f64>().ok())
                .map(|v| v > self.value.parse::<f64>().unwrap_or(0.0))
                .unwrap_or(false),
            ConditionType::LessThan => context
                .variables
                .get(&self.field)
                .and_then(|v| v.parse::<f64>().ok())
                .map(|v| v < self.value.parse::<f64>().unwrap_or(0.0))
                .unwrap_or(false),
        };

        if self.negate {
            !result
        } else {
            result
        }
    }
}

/// Condition types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConditionType {
    Equals,
    Contains,
    StartsWith,
    EndsWith,
    Exists,
    InList,
    GreaterThan,
    LessThan,
}

/// Context for prompt selection and rendering.
#[derive(Debug, Clone, Default)]
pub struct PromptContext {
    /// Variables for template rendering and condition evaluation.
    pub variables: HashMap<String, String>,
    /// User ID.
    pub user_id: Option<String>,
    /// Channel ID.
    pub channel_id: Option<String>,
    /// Task type.
    pub task_type: Option<String>,
    /// Additional metadata.
    pub metadata: HashMap<String, serde_json::Value>,
}

impl PromptContext {
    /// Create a new context.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set a variable.
    pub fn with_var(mut self, key: &str, value: &str) -> Self {
        self.variables.insert(key.to_string(), value.to_string());
        self
    }

    /// Set user ID.
    pub fn with_user(mut self, user_id: &str) -> Self {
        self.user_id = Some(user_id.to_string());
        self.variables
            .insert("user_id".to_string(), user_id.to_string());
        self
    }

    /// Set channel ID.
    pub fn with_channel(mut self, channel_id: &str) -> Self {
        self.channel_id = Some(channel_id.to_string());
        self.variables
            .insert("channel_id".to_string(), channel_id.to_string());
        self
    }

    /// Set task type.
    pub fn with_task(mut self, task_type: &str) -> Self {
        self.task_type = Some(task_type.to_string());
        self.variables
            .insert("task_type".to_string(), task_type.to_string());
        self
    }
}

/// A/B test configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ABTest {
    /// Test ID.
    pub id: Uuid,
    /// Test name.
    pub name: String,
    /// Variant A prompt ID.
    pub variant_a: Uuid,
    /// Variant B prompt ID.
    pub variant_b: Uuid,
    /// Traffic percentage for variant B (0-100).
    pub variant_b_percentage: u8,
    /// Whether test is active.
    pub active: bool,
    /// Metrics collected.
    pub metrics: ABMetrics,
}

/// A/B test metrics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ABMetrics {
    /// Variant A usage count.
    pub variant_a_count: u64,
    /// Variant B usage count.
    pub variant_b_count: u64,
    /// Variant A success rate.
    pub variant_a_success: f32,
    /// Variant B success rate.
    pub variant_b_success: f32,
}

/// Prompt configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DynPromptConfig {
    /// Enable dynamic prompts.
    pub enabled: bool,
    /// Cache rendered prompts.
    pub cache_rendered: bool,
    /// Enable A/B testing.
    pub ab_testing: bool,
    /// Default prompt name.
    pub default_prompt: String,
}

impl Default for DynPromptConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            cache_rendered: true,
            ab_testing: false,
            default_prompt: "default".to_string(),
        }
    }
}

/// Trait for prompt sources.
#[async_trait]
pub trait PromptSource: Send + Sync {
    /// Load prompts from source.
    async fn load(&self) -> Result<Vec<DynamicPrompt>>;

    /// Save a prompt.
    async fn save(&self, prompt: &DynamicPrompt) -> Result<()>;
}

/// Dynamic prompt manager.
pub struct DynPromptManager {
    config: DynPromptConfig,
    prompts: Arc<RwLock<HashMap<String, Vec<DynamicPrompt>>>>,
    ab_tests: Arc<RwLock<HashMap<Uuid, ABTest>>>,
    cache: Arc<RwLock<HashMap<String, String>>>,
}

impl DynPromptManager {
    /// Create a new prompt manager.
    pub fn new(config: DynPromptConfig) -> Self {
        Self {
            config,
            prompts: Arc::new(RwLock::new(HashMap::new())),
            ab_tests: Arc::new(RwLock::new(HashMap::new())),
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a prompt.
    pub async fn register(&self, prompt: DynamicPrompt) {
        let mut prompts = self.prompts.write().await;
        prompts.entry(prompt.name.clone()).or_default().push(prompt);
    }

    /// Get the best prompt for a context.
    pub async fn get_prompt(&self, name: &str, context: &PromptContext) -> Result<String> {
        if !self.config.enabled {
            return Err(PromptError::NoMatch);
        }

        // Check cache first
        if self.config.cache_rendered {
            let cache_key = format!("{}:{:?}", name, context.variables);
            let cache = self.cache.read().await;
            if let Some(cached) = cache.get(&cache_key) {
                return Ok(cached.clone());
            }
        }

        let prompts = self.prompts.read().await;
        let prompt_versions = prompts
            .get(name)
            .ok_or_else(|| PromptError::PromptNotFound(name.to_string()))?;

        // Find matching prompt with highest priority
        let matching: Vec<_> = prompt_versions
            .iter()
            .filter(|p| p.matches(context))
            .collect();

        let selected = matching
            .iter()
            .max_by_key(|p| p.priority)
            .ok_or(PromptError::NoMatch)?;

        let rendered = selected.render(context)?;

        // Cache the result
        if self.config.cache_rendered {
            let cache_key = format!("{}:{:?}", name, context.variables);
            self.cache.write().await.insert(cache_key, rendered.clone());
        }

        Ok(rendered)
    }

    /// Get or default - returns default if no match.
    pub async fn get_or_default(
        &self,
        name: &str,
        context: &PromptContext,
        default: &str,
    ) -> String {
        self.get_prompt(name, context)
            .await
            .unwrap_or_else(|_| default.to_string())
    }

    /// List all registered prompts.
    pub async fn list_prompts(&self) -> Vec<DynamicPrompt> {
        self.prompts
            .read()
            .await
            .values()
            .flatten()
            .cloned()
            .collect()
    }

    /// Clear the cache.
    pub async fn clear_cache(&self) {
        self.cache.write().await.clear();
    }

    /// Register an A/B test.
    pub async fn register_ab_test(&self, test: ABTest) {
        self.ab_tests.write().await.insert(test.id, test);
    }

    /// Select prompt variant for A/B test.
    pub async fn select_variant(&self, test_id: Uuid, user_id: &str) -> Result<Uuid> {
        let tests = self.ab_tests.read().await;
        let test = tests
            .get(&test_id)
            .ok_or(PromptError::PromptNotFound(test_id.to_string()))?;

        if !test.active {
            return Ok(test.variant_a);
        }

        // Simple hash-based assignment
        let hash: u64 = user_id
            .bytes()
            .fold(0u64, |acc, b| acc.wrapping_add(b as u64).wrapping_mul(31));
        let percentage = (hash % 100) as u8;

        if percentage < test.variant_b_percentage {
            Ok(test.variant_b)
        } else {
            Ok(test.variant_a)
        }
    }

    /// Record A/B test result.
    pub async fn record_ab_result(&self, test_id: Uuid, variant_b: bool, success: bool) {
        let mut tests = self.ab_tests.write().await;
        if let Some(test) = tests.get_mut(&test_id) {
            if variant_b {
                test.metrics.variant_b_count += 1;
                if success {
                    let total = test.metrics.variant_b_count;
                    let successes = (test.metrics.variant_b_success * (total - 1) as f32) + 1.0;
                    test.metrics.variant_b_success = successes / total as f32;
                }
            } else {
                test.metrics.variant_a_count += 1;
                if success {
                    let total = test.metrics.variant_a_count;
                    let successes = (test.metrics.variant_a_success * (total - 1) as f32) + 1.0;
                    test.metrics.variant_a_success = successes / total as f32;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_dynamic_prompt() {
        let manager = DynPromptManager::new(DynPromptConfig::default());

        let prompt =
            DynamicPrompt::new("greeting", "Hello {name}! Welcome to {service}.").as_default();

        manager.register(prompt).await;

        let context = PromptContext::new()
            .with_var("name", "Alice")
            .with_var("service", "DrBot");

        let result = manager.get_prompt("greeting", &context).await.unwrap();
        assert_eq!(result, "Hello Alice! Welcome to DrBot.");
    }

    #[tokio::test]
    async fn test_conditional_prompt() {
        let manager = DynPromptManager::new(DynPromptConfig::default());

        let default_prompt = DynamicPrompt::new("assistant", "I am your assistant.")
            .as_default()
            .with_priority(0);

        let coding_prompt = DynamicPrompt::new(
            "assistant",
            "I am your coding assistant. Let me help with {language}.",
        )
        .with_condition(PromptCondition::new(
            ConditionType::Equals,
            "task_type",
            "coding",
        ))
        .with_priority(10);

        manager.register(default_prompt).await;
        manager.register(coding_prompt).await;

        // Default context
        let context = PromptContext::new();
        let result = manager.get_prompt("assistant", &context).await.unwrap();
        assert_eq!(result, "I am your assistant.");

        // Coding context
        let context = PromptContext::new()
            .with_task("coding")
            .with_var("language", "Rust");
        let result = manager.get_prompt("assistant", &context).await.unwrap();
        assert_eq!(result, "I am your coding assistant. Let me help with Rust.");
    }

    #[test]
    fn test_variable_extraction() {
        let prompt = DynamicPrompt::new("test", "Hello {name}, your {item} is ready!");
        assert!(prompt.variables.contains(&"name".to_string()));
        assert!(prompt.variables.contains(&"item".to_string()));
    }

    #[test]
    fn test_condition_evaluation() {
        let context = PromptContext::new()
            .with_var("role", "admin")
            .with_var("count", "10");

        let eq = PromptCondition::new(ConditionType::Equals, "role", "admin");
        assert!(eq.evaluate(&context));

        let gt = PromptCondition::new(ConditionType::GreaterThan, "count", "5");
        assert!(gt.evaluate(&context));

        let lt = PromptCondition::new(ConditionType::LessThan, "count", "20");
        assert!(lt.evaluate(&context));
    }
}
