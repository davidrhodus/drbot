//! Response templates for drbot.
//!
//! Reusable templates for common response patterns.
//!
//! # Features
//!
//! - Template library
//! - Variable substitution
//! - Context-aware selection
//! - Template versioning

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Template result type.
pub type Result<T> = std::result::Result<T, TemplateError>;

/// Template errors.
#[derive(Debug, thiserror::Error)]
pub enum TemplateError {
    #[error("Template not found: {0}")]
    TemplateNotFound(String),
    #[error("Render failed: {0}")]
    RenderFailed(String),
    #[error("Missing variable: {0}")]
    MissingVariable(String),
    #[error("Invalid template: {0}")]
    InvalidTemplate(String),
}

/// A response template.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseTemplate {
    /// Template ID.
    pub id: Uuid,
    /// Template name.
    pub name: String,
    /// Template category.
    pub category: String,
    /// Template content.
    pub content: String,
    /// Variables in template.
    pub variables: Vec<TemplateVariable>,
    /// Usage conditions.
    pub conditions: Vec<TemplateCondition>,
    /// Version.
    pub version: u32,
    /// Usage count.
    pub usage_count: u64,
    /// Success rate.
    pub success_rate: f32,
    /// Created at.
    pub created_at: DateTime<Utc>,
    /// Updated at.
    pub updated_at: DateTime<Utc>,
    /// Tags.
    pub tags: Vec<String>,
}

impl ResponseTemplate {
    /// Create a new template.
    pub fn new(name: &str, category: &str, content: &str) -> Self {
        let variables = Self::extract_variables(content);
        Self {
            id: Uuid::new_v4(),
            name: name.to_string(),
            category: category.to_string(),
            content: content.to_string(),
            variables,
            conditions: Vec::new(),
            version: 1,
            usage_count: 0,
            success_rate: 0.0,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            tags: Vec::new(),
        }
    }

    fn extract_variables(content: &str) -> Vec<TemplateVariable> {
        let mut vars = Vec::new();
        let mut in_var = false;
        let mut current = String::new();

        for c in content.chars() {
            if c == '{' && !in_var {
                in_var = true;
                current.clear();
            } else if c == '}' && in_var {
                in_var = false;
                if !current.is_empty() {
                    let parts: Vec<_> = current.split(':').collect();
                    let name = parts[0].to_string();
                    let default = parts.get(1).map(|s| s.to_string());

                    if !vars.iter().any(|v: &TemplateVariable| v.name == name) {
                        vars.push(TemplateVariable {
                            name,
                            description: String::new(),
                            required: default.is_none(),
                            default,
                            var_type: VariableType::String,
                        });
                    }
                }
            } else if in_var {
                current.push(c);
            }
        }

        vars
    }

    /// Add a condition.
    pub fn with_condition(mut self, condition: TemplateCondition) -> Self {
        self.conditions.push(condition);
        self
    }

    /// Add tags.
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    /// Render the template with variables.
    pub fn render(&self, vars: &HashMap<String, String>) -> Result<String> {
        let mut result = self.content.clone();

        for var in &self.variables {
            let placeholder = format!("{{{}}}", var.name);
            let placeholder_with_default =
                format!("{{{}:{}}}", var.name, var.default.as_deref().unwrap_or(""));

            let value = vars
                .get(&var.name)
                .or(var.default.as_ref())
                .ok_or_else(|| TemplateError::MissingVariable(var.name.clone()))?;

            result = result.replace(&placeholder, value);
            result = result.replace(&placeholder_with_default, value);
        }

        Ok(result)
    }

    /// Check if template matches context.
    pub fn matches(&self, context: &TemplateContext) -> bool {
        if self.conditions.is_empty() {
            return true;
        }

        self.conditions.iter().all(|c| c.evaluate(context))
    }

    /// Record usage.
    pub fn record_usage(&mut self, success: bool) {
        self.usage_count += 1;
        let n = self.usage_count as f32;
        self.success_rate = (self.success_rate * (n - 1.0) + if success { 1.0 } else { 0.0 }) / n;
    }
}

/// Template variable.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateVariable {
    /// Variable name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Required.
    pub required: bool,
    /// Default value.
    pub default: Option<String>,
    /// Variable type.
    pub var_type: VariableType,
}

/// Variable types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VariableType {
    String,
    Number,
    Date,
    List,
    Boolean,
}

/// Template condition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateCondition {
    /// Field to check.
    pub field: String,
    /// Operator.
    pub operator: ConditionOperator,
    /// Expected value.
    pub value: String,
}

impl TemplateCondition {
    /// Create a new condition.
    pub fn new(field: &str, operator: ConditionOperator, value: &str) -> Self {
        Self {
            field: field.to_string(),
            operator,
            value: value.to_string(),
        }
    }

    /// Evaluate the condition.
    pub fn evaluate(&self, context: &TemplateContext) -> bool {
        let actual = context.get(&self.field);

        match self.operator {
            ConditionOperator::Equals => actual == Some(&self.value),
            ConditionOperator::NotEquals => actual != Some(&self.value),
            ConditionOperator::Contains => actual.map(|v| v.contains(&self.value)).unwrap_or(false),
            ConditionOperator::StartsWith => {
                actual.map(|v| v.starts_with(&self.value)).unwrap_or(false)
            }
            ConditionOperator::EndsWith => {
                actual.map(|v| v.ends_with(&self.value)).unwrap_or(false)
            }
            ConditionOperator::Exists => actual.is_some(),
            ConditionOperator::NotExists => actual.is_none(),
        }
    }
}

/// Condition operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConditionOperator {
    Equals,
    NotEquals,
    Contains,
    StartsWith,
    EndsWith,
    Exists,
    NotExists,
}

/// Template context.
#[derive(Debug, Clone, Default)]
pub struct TemplateContext {
    values: HashMap<String, String>,
}

impl TemplateContext {
    /// Create a new context.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set a value.
    pub fn set(&mut self, key: &str, value: &str) {
        self.values.insert(key.to_string(), value.to_string());
    }

    /// Get a value.
    pub fn get(&self, key: &str) -> Option<&String> {
        self.values.get(key)
    }

    /// Build context fluently.
    pub fn with(mut self, key: &str, value: &str) -> Self {
        self.set(key, value);
        self
    }
}

/// Template configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateConfig {
    /// Enable templates.
    pub enabled: bool,
    /// Strict variable checking.
    pub strict: bool,
    /// Track usage.
    pub track_usage: bool,
}

impl Default for TemplateConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            strict: false,
            track_usage: true,
        }
    }
}

/// Template manager.
pub struct TemplateManager {
    config: TemplateConfig,
    templates: Arc<RwLock<HashMap<String, Vec<ResponseTemplate>>>>,
}

impl TemplateManager {
    /// Create a new template manager.
    pub fn new(config: TemplateConfig) -> Self {
        Self {
            config,
            templates: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a template.
    pub async fn register(&self, template: ResponseTemplate) {
        let mut templates = self.templates.write().await;
        templates
            .entry(template.category.clone())
            .or_default()
            .push(template);
    }

    /// Get template by name.
    pub async fn get(&self, category: &str, name: &str) -> Option<ResponseTemplate> {
        let templates = self.templates.read().await;
        templates
            .get(category)?
            .iter()
            .find(|t| t.name == name)
            .cloned()
    }

    /// Find matching template for context.
    pub async fn find_matching(
        &self,
        category: &str,
        context: &TemplateContext,
    ) -> Option<ResponseTemplate> {
        let templates = self.templates.read().await;
        templates
            .get(category)?
            .iter()
            .filter(|t| t.matches(context))
            .max_by_key(|t| t.usage_count)
            .cloned()
    }

    /// Render a template.
    pub async fn render(
        &self,
        category: &str,
        name: &str,
        vars: &HashMap<String, String>,
    ) -> Result<String> {
        let template = self
            .get(category, name)
            .await
            .ok_or_else(|| TemplateError::TemplateNotFound(name.to_string()))?;

        let result = template.render(vars)?;

        if self.config.track_usage {
            // Would update usage in a real implementation
        }

        Ok(result)
    }

    /// Render best matching template.
    pub async fn render_best(
        &self,
        category: &str,
        context: &TemplateContext,
        vars: &HashMap<String, String>,
    ) -> Result<String> {
        let template = self
            .find_matching(category, context)
            .await
            .ok_or(TemplateError::TemplateNotFound(category.to_string()))?;

        template.render(vars)
    }

    /// List all templates.
    pub async fn list_all(&self) -> Vec<ResponseTemplate> {
        self.templates
            .read()
            .await
            .values()
            .flatten()
            .cloned()
            .collect()
    }

    /// List templates by category.
    pub async fn list_by_category(&self, category: &str) -> Vec<ResponseTemplate> {
        self.templates
            .read()
            .await
            .get(category)
            .cloned()
            .unwrap_or_default()
    }

    /// List categories.
    pub async fn list_categories(&self) -> Vec<String> {
        self.templates.read().await.keys().cloned().collect()
    }

    /// Get statistics.
    pub async fn stats(&self) -> TemplateStats {
        let templates = self.templates.read().await;

        let total: usize = templates.values().map(|v| v.len()).sum();
        let total_usage: u64 = templates.values().flatten().map(|t| t.usage_count).sum();
        let avg_success: f32 = if total > 0 {
            templates
                .values()
                .flatten()
                .map(|t| t.success_rate)
                .sum::<f32>()
                / total as f32
        } else {
            0.0
        };

        TemplateStats {
            total_templates: total,
            total_categories: templates.len(),
            total_usage,
            avg_success_rate: avg_success,
        }
    }
}

/// Template statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateStats {
    pub total_templates: usize,
    pub total_categories: usize,
    pub total_usage: u64,
    pub avg_success_rate: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_template_rendering() {
        let manager = TemplateManager::new(TemplateConfig::default());

        let template =
            ResponseTemplate::new("greeting", "general", "Hello {name}! Welcome to {service}.");

        manager.register(template).await;

        let mut vars = HashMap::new();
        vars.insert("name".to_string(), "Alice".to_string());
        vars.insert("service".to_string(), "DrBot".to_string());

        let result = manager.render("general", "greeting", &vars).await.unwrap();
        assert_eq!(result, "Hello Alice! Welcome to DrBot.");
    }

    #[tokio::test]
    async fn test_default_values() {
        let template = ResponseTemplate::new("test", "general", "Hello {name:Guest}!");

        let vars = HashMap::new();
        let result = template.render(&vars).unwrap();
        assert_eq!(result, "Hello Guest!");
    }

    #[tokio::test]
    async fn test_conditional_templates() {
        let manager = TemplateManager::new(TemplateConfig::default());

        let casual = ResponseTemplate::new("greeting", "general", "Hey {name}!").with_condition(
            TemplateCondition::new("tone", ConditionOperator::Equals, "casual"),
        );

        let formal =
            ResponseTemplate::new("greeting_formal", "general", "Dear {name},").with_condition(
                TemplateCondition::new("tone", ConditionOperator::Equals, "formal"),
            );

        manager.register(casual).await;
        manager.register(formal).await;

        let context = TemplateContext::new().with("tone", "casual");
        let template = manager.find_matching("general", &context).await.unwrap();
        assert_eq!(template.name, "greeting");
    }

    #[test]
    fn test_variable_extraction() {
        let template =
            ResponseTemplate::new("test", "cat", "Hello {name}, you have {count} messages.");
        assert_eq!(template.variables.len(), 2);
        assert!(template.variables.iter().any(|v| v.name == "name"));
        assert!(template.variables.iter().any(|v| v.name == "count"));
    }

    #[test]
    fn test_missing_variable() {
        let template = ResponseTemplate::new("test", "cat", "Hello {name}!");
        let vars = HashMap::new();
        let result = template.render(&vars);
        assert!(matches!(result, Err(TemplateError::MissingVariable(_))));
    }
}
