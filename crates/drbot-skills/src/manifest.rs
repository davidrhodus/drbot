//! Skill manifest (skill.toml) parsing.

use crate::{Result, SkillError};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Skill manifest (skill.toml).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillManifest {
    /// Skill name.
    pub name: String,
    /// Skill version.
    pub version: String,
    /// Description.
    pub description: String,
    /// Author.
    pub author: Option<String>,
    /// License.
    pub license: Option<String>,
    /// Homepage URL.
    pub homepage: Option<String>,
    /// Repository URL.
    pub repository: Option<String>,
    /// Tags for categorization.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Input parameters.
    #[serde(default)]
    pub inputs: Vec<ManifestInput>,
    /// Output specification.
    #[serde(default)]
    pub outputs: Vec<ManifestOutput>,
    /// Required capabilities.
    #[serde(default)]
    pub capabilities: Vec<ManifestCapability>,
    /// Entry point (for external skills).
    pub entry_point: Option<String>,
    /// Runtime (python, node, etc.).
    pub runtime: Option<String>,
}

impl SkillManifest {
    /// Load a manifest from a file.
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        Self::from_toml(&content)
    }

    /// Parse a manifest from TOML.
    pub fn from_toml(toml: &str) -> Result<Self> {
        toml::from_str(toml).map_err(|e| SkillError::InvalidManifest(e.to_string()))
    }

    /// Validate the manifest.
    pub fn validate(&self) -> Result<()> {
        if self.name.is_empty() {
            return Err(SkillError::InvalidManifest("Name is required".into()));
        }
        if self.version.is_empty() {
            return Err(SkillError::InvalidManifest("Version is required".into()));
        }
        if self.description.is_empty() {
            return Err(SkillError::InvalidManifest(
                "Description is required".into(),
            ));
        }
        Ok(())
    }

    /// Check if this skill requires a specific capability.
    pub fn requires_capability(&self, name: &str) -> bool {
        self.capabilities.iter().any(|c| c.name == name)
    }

    /// Get required input names.
    pub fn required_inputs(&self) -> Vec<&str> {
        self.inputs
            .iter()
            .filter(|i| i.required)
            .map(|i| i.name.as_str())
            .collect()
    }
}

/// Input parameter specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestInput {
    /// Parameter name.
    pub name: String,
    /// Parameter type (string, number, boolean, array, object).
    #[serde(rename = "type")]
    pub param_type: String,
    /// Description.
    pub description: String,
    /// Whether this parameter is required.
    #[serde(default)]
    pub required: bool,
    /// Default value.
    pub default: Option<serde_json::Value>,
    /// Validation pattern (regex).
    pub pattern: Option<String>,
    /// Allowed values (enum).
    #[serde(default)]
    pub enum_values: Vec<serde_json::Value>,
}

/// Output specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestOutput {
    /// Output name.
    pub name: String,
    /// Output type.
    #[serde(rename = "type")]
    pub output_type: String,
    /// Description.
    pub description: String,
}

/// Required capability.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestCapability {
    /// Capability name (e.g., "network", "filesystem", "browser").
    pub name: String,
    /// Why this capability is needed.
    pub reason: Option<String>,
    /// Whether this capability is optional.
    #[serde(default)]
    pub optional: bool,
}

impl ManifestCapability {
    /// Create a required capability.
    pub fn required(name: &str) -> Self {
        Self {
            name: name.to_string(),
            reason: None,
            optional: false,
        }
    }

    /// Create an optional capability.
    pub fn optional(name: &str) -> Self {
        Self {
            name: name.to_string(),
            reason: None,
            optional: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manifest_parse() {
        let toml = r#"
            name = "test-skill"
            version = "1.0.0"
            description = "A test skill"
            tags = ["test", "example"]

            [[inputs]]
            name = "query"
            type = "string"
            description = "Search query"
            required = true

            [[outputs]]
            name = "results"
            type = "array"
            description = "Search results"

            [[capabilities]]
            name = "network"
            reason = "Needs to fetch data"
        "#;

        let manifest = SkillManifest::from_toml(toml).unwrap();

        assert_eq!(manifest.name, "test-skill");
        assert_eq!(manifest.version, "1.0.0");
        assert_eq!(manifest.inputs.len(), 1);
        assert_eq!(manifest.outputs.len(), 1);
        assert!(manifest.requires_capability("network"));
        assert!(manifest.validate().is_ok());
    }

    #[test]
    fn test_manifest_validation() {
        let invalid = SkillManifest {
            name: String::new(),
            version: "1.0.0".to_string(),
            description: "Test".to_string(),
            author: None,
            license: None,
            homepage: None,
            repository: None,
            tags: Vec::new(),
            inputs: Vec::new(),
            outputs: Vec::new(),
            capabilities: Vec::new(),
            entry_point: None,
            runtime: None,
        };

        assert!(invalid.validate().is_err());
    }
}
