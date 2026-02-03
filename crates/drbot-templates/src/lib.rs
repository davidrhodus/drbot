//! Conversation templates and playbooks for drbot.
//!
//! Provides reusable conversation starters and multi-step guided workflows.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Templates result type.
pub type Result<T> = std::result::Result<T, TemplateError>;

/// Template errors.
#[derive(Debug, thiserror::Error)]
pub enum TemplateError {
    #[error("Template not found: {0}")]
    NotFound(String),
    #[error("Invalid template: {0}")]
    Invalid(String),
    #[error("Variable missing: {0}")]
    MissingVariable(String),
}

/// Conversation template.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Template {
    /// Template ID.
    pub id: Uuid,
    /// Template name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Category.
    pub category: String,
    /// Template type.
    pub template_type: TemplateType,
    /// System prompt.
    pub system_prompt: Option<String>,
    /// Initial messages.
    pub initial_messages: Vec<TemplateMessage>,
    /// Variables.
    pub variables: Vec<Variable>,
    /// Tags.
    pub tags: Vec<String>,
    /// Is public.
    pub is_public: bool,
    /// Author.
    pub author: Option<String>,
    /// Usage count.
    pub usage_count: u64,
    /// Rating.
    pub rating: f32,
    /// Created at.
    pub created_at: DateTime<Utc>,
    /// Updated at.
    pub updated_at: DateTime<Utc>,
}

impl Template {
    /// Create a new template.
    pub fn new(name: &str, description: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.to_string(),
            description: description.to_string(),
            category: "general".to_string(),
            template_type: TemplateType::Conversation,
            system_prompt: None,
            initial_messages: Vec::new(),
            variables: Vec::new(),
            tags: Vec::new(),
            is_public: false,
            author: None,
            usage_count: 0,
            rating: 0.0,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    /// Set system prompt.
    pub fn with_system_prompt(mut self, prompt: &str) -> Self {
        self.system_prompt = Some(prompt.to_string());
        self
    }

    /// Add a message.
    pub fn with_message(mut self, role: &str, content: &str) -> Self {
        self.initial_messages.push(TemplateMessage {
            role: role.to_string(),
            content: content.to_string(),
        });
        self
    }

    /// Add a variable.
    pub fn with_variable(mut self, variable: Variable) -> Self {
        self.variables.push(variable);
        self
    }

    /// Render the template with variables.
    pub fn render(&self, values: &HashMap<String, String>) -> Result<RenderedTemplate> {
        // Check all required variables are provided
        for var in &self.variables {
            if var.required && !values.contains_key(&var.name) && var.default.is_none() {
                return Err(TemplateError::MissingVariable(var.name.clone()));
            }
        }

        // Render messages
        let mut messages = Vec::new();
        for msg in &self.initial_messages {
            let content = self.substitute(&msg.content, values);
            messages.push(TemplateMessage {
                role: msg.role.clone(),
                content,
            });
        }

        // Render system prompt
        let system_prompt = self
            .system_prompt
            .as_ref()
            .map(|p| self.substitute(p, values));

        Ok(RenderedTemplate {
            template_id: self.id,
            system_prompt,
            messages,
        })
    }

    fn substitute(&self, text: &str, values: &HashMap<String, String>) -> String {
        let mut result = text.to_string();

        for var in &self.variables {
            let placeholder = format!("{{{{{}}}}}", var.name);
            let value = values
                .get(&var.name)
                .or(var.default.as_ref())
                .map(|s| s.as_str())
                .unwrap_or("");
            result = result.replace(&placeholder, value);
        }

        result
    }
}

/// Template type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TemplateType {
    /// Simple conversation starter.
    Conversation,
    /// Multi-step playbook.
    Playbook,
    /// Task-specific template.
    Task,
    /// System prompt template.
    System,
}

/// Template message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateMessage {
    /// Role.
    pub role: String,
    /// Content.
    pub content: String,
}

/// Variable definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Variable {
    /// Variable name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Variable type.
    pub var_type: VarType,
    /// Default value.
    pub default: Option<String>,
    /// Is required.
    pub required: bool,
    /// Options (for select type).
    pub options: Vec<String>,
}

impl Variable {
    /// Create a new variable.
    pub fn new(name: &str, description: &str) -> Self {
        Self {
            name: name.to_string(),
            description: description.to_string(),
            var_type: VarType::Text,
            default: None,
            required: true,
            options: Vec::new(),
        }
    }

    /// Set as optional with default.
    pub fn optional(mut self, default: &str) -> Self {
        self.required = false;
        self.default = Some(default.to_string());
        self
    }
}

/// Variable type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VarType {
    Text,
    Number,
    Select,
    MultiSelect,
    Boolean,
    Date,
    File,
}

/// Rendered template.
#[derive(Debug, Clone)]
pub struct RenderedTemplate {
    /// Source template ID.
    pub template_id: Uuid,
    /// Rendered system prompt.
    pub system_prompt: Option<String>,
    /// Rendered messages.
    pub messages: Vec<TemplateMessage>,
}

/// Playbook - multi-step guided workflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Playbook {
    /// Playbook ID.
    pub id: Uuid,
    /// Name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Steps.
    pub steps: Vec<PlaybookStep>,
    /// Variables.
    pub variables: Vec<Variable>,
    /// Estimated duration.
    pub estimated_minutes: Option<u32>,
    /// Created at.
    pub created_at: DateTime<Utc>,
}

/// Playbook step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaybookStep {
    /// Step ID.
    pub id: Uuid,
    /// Step number.
    pub step_number: u32,
    /// Title.
    pub title: String,
    /// Description.
    pub description: String,
    /// Prompt to use.
    pub prompt: String,
    /// Expected output.
    pub expected_output: Option<String>,
    /// Can skip.
    pub skippable: bool,
}

impl Playbook {
    /// Create a new playbook.
    pub fn new(name: &str, description: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.to_string(),
            description: description.to_string(),
            steps: Vec::new(),
            variables: Vec::new(),
            estimated_minutes: None,
            created_at: Utc::now(),
        }
    }

    /// Add a step.
    pub fn add_step(&mut self, title: &str, prompt: &str) {
        let step_number = self.steps.len() as u32 + 1;
        self.steps.push(PlaybookStep {
            id: Uuid::new_v4(),
            step_number,
            title: title.to_string(),
            description: String::new(),
            prompt: prompt.to_string(),
            expected_output: None,
            skippable: false,
        });
    }
}

/// Template library.
pub struct TemplateLibrary {
    templates: Arc<RwLock<HashMap<Uuid, Template>>>,
    playbooks: Arc<RwLock<HashMap<Uuid, Playbook>>>,
}

impl TemplateLibrary {
    /// Create a new library.
    pub fn new() -> Self {
        Self {
            templates: Arc::new(RwLock::new(HashMap::new())),
            playbooks: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Add a template.
    pub async fn add_template(&self, template: Template) -> Uuid {
        let id = template.id;
        let mut templates = self.templates.write().await;
        templates.insert(id, template);
        id
    }

    /// Get a template.
    pub async fn get_template(&self, id: Uuid) -> Option<Template> {
        let templates = self.templates.read().await;
        templates.get(&id).cloned()
    }

    /// Search templates.
    pub async fn search_templates(&self, query: &str, category: Option<&str>) -> Vec<Template> {
        let templates = self.templates.read().await;
        let query_lower = query.to_lowercase();

        templates
            .values()
            .filter(|t| {
                let matches_query = t.name.to_lowercase().contains(&query_lower)
                    || t.description.to_lowercase().contains(&query_lower)
                    || t.tags
                        .iter()
                        .any(|tag| tag.to_lowercase().contains(&query_lower));

                let matches_category = category.map_or(true, |c| t.category == c);

                matches_query && matches_category
            })
            .cloned()
            .collect()
    }

    /// Get templates by category.
    pub async fn by_category(&self, category: &str) -> Vec<Template> {
        let templates = self.templates.read().await;
        templates
            .values()
            .filter(|t| t.category == category)
            .cloned()
            .collect()
    }

    /// Add a playbook.
    pub async fn add_playbook(&self, playbook: Playbook) -> Uuid {
        let id = playbook.id;
        let mut playbooks = self.playbooks.write().await;
        playbooks.insert(id, playbook);
        id
    }

    /// Get a playbook.
    pub async fn get_playbook(&self, id: Uuid) -> Option<Playbook> {
        let playbooks = self.playbooks.read().await;
        playbooks.get(&id).cloned()
    }

    /// List playbooks.
    pub async fn list_playbooks(&self) -> Vec<Playbook> {
        let playbooks = self.playbooks.read().await;
        playbooks.values().cloned().collect()
    }

    /// Record template usage.
    pub async fn record_usage(&self, id: Uuid) {
        let mut templates = self.templates.write().await;
        if let Some(template) = templates.get_mut(&id) {
            template.usage_count += 1;
        }
    }
}

impl Default for TemplateLibrary {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_template() {
        let template = Template::new("Code Review", "Start a code review session")
            .with_system_prompt("You are a code reviewer. Be thorough but constructive.")
            .with_message("user", "Please review this {{language}} code: {{code}}")
            .with_variable(Variable::new("language", "Programming language"))
            .with_variable(Variable::new("code", "Code to review"));

        let mut values = HashMap::new();
        values.insert("language".to_string(), "Rust".to_string());
        values.insert("code".to_string(), "fn main() {}".to_string());

        let rendered = template.render(&values).unwrap();
        assert!(rendered.messages[0].content.contains("Rust"));
        assert!(rendered.messages[0].content.contains("fn main()"));
    }

    #[tokio::test]
    async fn test_library() {
        let library = TemplateLibrary::new();

        let template = Template::new("Test", "Test template");
        let id = library.add_template(template).await;

        let fetched = library.get_template(id).await;
        assert!(fetched.is_some());

        let results = library.search_templates("test", None).await;
        assert_eq!(results.len(), 1);
    }
}
