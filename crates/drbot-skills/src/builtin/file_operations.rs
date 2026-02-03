//! File operations skill.

use crate::{
    ManifestCapability, ManifestInput, ManifestOutput, Result, Skill, SkillContext, SkillInput,
    SkillManifest, SkillOutput,
};
use async_trait::async_trait;

/// File operations skill for reading, writing, and managing files.
pub struct FileOperationsSkill {
    manifest: SkillManifest,
}

impl FileOperationsSkill {
    /// Create a new file operations skill.
    pub fn new() -> Self {
        Self {
            manifest: SkillManifest {
                name: "file-operations".to_string(),
                version: "1.0.0".to_string(),
                description: "Read, write, and manage files".to_string(),
                author: Some("drbot".to_string()),
                license: Some("MIT".to_string()),
                homepage: None,
                repository: None,
                tags: vec!["builtin".to_string(), "files".to_string()],
                inputs: vec![
                    ManifestInput {
                        name: "operation".to_string(),
                        param_type: "string".to_string(),
                        description: "Operation to perform".to_string(),
                        required: true,
                        default: None,
                        pattern: None,
                        enum_values: vec![
                            serde_json::json!("read"),
                            serde_json::json!("write"),
                            serde_json::json!("list"),
                            serde_json::json!("delete"),
                        ],
                    },
                    ManifestInput {
                        name: "path".to_string(),
                        param_type: "string".to_string(),
                        description: "File or directory path".to_string(),
                        required: true,
                        default: None,
                        pattern: None,
                        enum_values: Vec::new(),
                    },
                    ManifestInput {
                        name: "content".to_string(),
                        param_type: "string".to_string(),
                        description: "Content to write (for write operation)".to_string(),
                        required: false,
                        default: None,
                        pattern: None,
                        enum_values: Vec::new(),
                    },
                ],
                outputs: vec![ManifestOutput {
                    name: "result".to_string(),
                    output_type: "object".to_string(),
                    description: "Operation result".to_string(),
                }],
                capabilities: vec![ManifestCapability::required("filesystem")],
                entry_point: None,
                runtime: None,
            },
        }
    }
}

impl Default for FileOperationsSkill {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Skill for FileOperationsSkill {
    fn manifest(&self) -> &SkillManifest {
        &self.manifest
    }

    async fn execute(&self, input: SkillInput, ctx: &SkillContext) -> Result<SkillOutput> {
        // Check capability
        if !ctx.has_capability("filesystem") {
            return Err(crate::SkillError::MissingCapability("filesystem".into()));
        }

        let operation: String = input.require("operation")?;
        let path: String = input.require("path")?;

        match operation.as_str() {
            "read" => {
                let content = std::fs::read_to_string(&path)
                    .map_err(|e| crate::SkillError::ExecutionFailed(e.to_string()))?;

                Ok(SkillOutput::new(serde_json::json!({
                    "content": content,
                    "path": path,
                }))
                .with_text(&format!(
                    "Read {} bytes from {}",
                    content.len(),
                    path
                )))
            }

            "write" => {
                let content: String = input.require("content")?;

                std::fs::write(&path, &content)
                    .map_err(|e| crate::SkillError::ExecutionFailed(e.to_string()))?;

                Ok(SkillOutput::new(serde_json::json!({
                    "path": path,
                    "bytes_written": content.len(),
                }))
                .with_text(&format!("Wrote {} bytes to {}", content.len(), path)))
            }

            "list" => {
                let entries: Vec<String> = std::fs::read_dir(&path)
                    .map_err(|e| crate::SkillError::ExecutionFailed(e.to_string()))?
                    .filter_map(|e| e.ok())
                    .map(|e| e.file_name().to_string_lossy().to_string())
                    .collect();

                Ok(SkillOutput::new(serde_json::json!({
                    "path": path,
                    "entries": entries,
                }))
                .with_text(&format!(
                    "Found {} entries in {}",
                    entries.len(),
                    path
                )))
            }

            "delete" => {
                let path_obj = std::path::Path::new(&path);

                if path_obj.is_dir() {
                    std::fs::remove_dir_all(&path)
                        .map_err(|e| crate::SkillError::ExecutionFailed(e.to_string()))?;
                } else {
                    std::fs::remove_file(&path)
                        .map_err(|e| crate::SkillError::ExecutionFailed(e.to_string()))?;
                }

                Ok(SkillOutput::new(serde_json::json!({
                    "path": path,
                    "deleted": true,
                }))
                .with_text(&format!("Deleted {}", path)))
            }

            _ => Err(crate::SkillError::ValidationFailed(format!(
                "Unknown operation: {}",
                operation
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_operations_skill() {
        let skill = FileOperationsSkill::new();

        assert_eq!(skill.manifest().name, "file-operations");
        assert!(skill.manifest().requires_capability("filesystem"));
    }
}
