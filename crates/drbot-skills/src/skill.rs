//! Skill trait and types.

use crate::{Result, SkillContext, SkillManifest};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Skill input data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillInput {
    /// Input parameters.
    pub params: HashMap<String, serde_json::Value>,
    /// Raw text input (if any).
    pub text: Option<String>,
    /// Attachments (file paths).
    #[serde(default)]
    pub attachments: Vec<String>,
}

impl SkillInput {
    /// Create a new skill input.
    pub fn new() -> Self {
        Self {
            params: HashMap::new(),
            text: None,
            attachments: Vec::new(),
        }
    }

    /// Add a parameter.
    pub fn with_param(mut self, key: &str, value: impl Serialize) -> Self {
        self.params.insert(
            key.to_string(),
            serde_json::to_value(value).unwrap_or(serde_json::Value::Null),
        );
        self
    }

    /// Set text input.
    pub fn with_text(mut self, text: &str) -> Self {
        self.text = Some(text.to_string());
        self
    }

    /// Add an attachment.
    pub fn with_attachment(mut self, path: &str) -> Self {
        self.attachments.push(path.to_string());
        self
    }

    /// Get a parameter value.
    pub fn get<T: for<'de> Deserialize<'de>>(&self, key: &str) -> Option<T> {
        self.params
            .get(key)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    /// Get a required parameter.
    pub fn require<T: for<'de> Deserialize<'de>>(&self, key: &str) -> Result<T> {
        self.get(key).ok_or_else(|| {
            crate::SkillError::ValidationFailed(format!("Missing required parameter: {}", key))
        })
    }
}

impl Default for SkillInput {
    fn default() -> Self {
        Self::new()
    }
}

/// Skill output data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillOutput {
    /// Output data.
    pub data: serde_json::Value,
    /// Text output (for display).
    pub text: Option<String>,
    /// Output artifacts (file paths).
    #[serde(default)]
    pub artifacts: Vec<String>,
    /// Additional metadata.
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

impl SkillOutput {
    /// Create a new skill output.
    pub fn new(data: impl Serialize) -> Self {
        Self {
            data: serde_json::to_value(data).unwrap_or(serde_json::Value::Null),
            text: None,
            artifacts: Vec::new(),
            metadata: HashMap::new(),
        }
    }

    /// Create with text output.
    pub fn text(text: &str) -> Self {
        Self {
            data: serde_json::Value::Null,
            text: Some(text.to_string()),
            artifacts: Vec::new(),
            metadata: HashMap::new(),
        }
    }

    /// Add text output.
    pub fn with_text(mut self, text: &str) -> Self {
        self.text = Some(text.to_string());
        self
    }

    /// Add an artifact.
    pub fn with_artifact(mut self, path: &str) -> Self {
        self.artifacts.push(path.to_string());
        self
    }

    /// Add metadata.
    pub fn with_metadata(mut self, key: &str, value: impl Serialize) -> Self {
        self.metadata.insert(
            key.to_string(),
            serde_json::to_value(value).unwrap_or(serde_json::Value::Null),
        );
        self
    }
}

/// Skill execution result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillResult {
    /// Whether execution succeeded.
    pub success: bool,
    /// Output (if successful).
    pub output: Option<SkillOutput>,
    /// Error message (if failed).
    pub error: Option<String>,
    /// Execution time in milliseconds.
    pub execution_time_ms: u64,
}

impl SkillResult {
    /// Create a successful result.
    pub fn success(output: SkillOutput, execution_time_ms: u64) -> Self {
        Self {
            success: true,
            output: Some(output),
            error: None,
            execution_time_ms,
        }
    }

    /// Create a failed result.
    pub fn failure(error: &str, execution_time_ms: u64) -> Self {
        Self {
            success: false,
            output: None,
            error: Some(error.to_string()),
            execution_time_ms,
        }
    }
}

/// Skill metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillMetadata {
    /// Skill name.
    pub name: String,
    /// Skill version.
    pub version: String,
    /// Description.
    pub description: String,
    /// Author.
    pub author: Option<String>,
    /// Tags.
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Trait for implementing skills.
#[async_trait]
pub trait Skill: Send + Sync {
    /// Get the skill manifest.
    fn manifest(&self) -> &SkillManifest;

    /// Validate input before execution.
    fn validate_input(&self, input: &SkillInput) -> Result<()> {
        // Default: validate required parameters from manifest
        for param in &self.manifest().inputs {
            if param.required && !input.params.contains_key(&param.name) {
                return Err(crate::SkillError::ValidationFailed(format!(
                    "Missing required parameter: {}",
                    param.name
                )));
            }
        }
        Ok(())
    }

    /// Execute the skill.
    async fn execute(&self, input: SkillInput, ctx: &SkillContext) -> Result<SkillOutput>;

    /// Get skill metadata.
    fn metadata(&self) -> SkillMetadata {
        let m = self.manifest();
        SkillMetadata {
            name: m.name.clone(),
            version: m.version.clone(),
            description: m.description.clone(),
            author: m.author.clone(),
            tags: m.tags.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skill_input() {
        let input = SkillInput::new()
            .with_param("query", "test")
            .with_text("Hello");

        assert_eq!(input.get::<String>("query"), Some("test".to_string()));
        assert_eq!(input.text, Some("Hello".to_string()));
    }

    #[test]
    fn test_skill_output() {
        let output = SkillOutput::text("Result")
            .with_artifact("/tmp/output.txt")
            .with_metadata("count", 42);

        assert_eq!(output.text, Some("Result".to_string()));
        assert_eq!(output.artifacts.len(), 1);
    }

    #[test]
    fn test_skill_result() {
        let output = SkillOutput::text("Done");
        let result = SkillResult::success(output, 100);

        assert!(result.success);
        assert!(result.output.is_some());
        assert_eq!(result.execution_time_ms, 100);
    }
}
