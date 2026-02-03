//! Data transformation skill.

use crate::{
    ManifestInput, ManifestOutput, Result, Skill, SkillContext, SkillInput, SkillManifest,
    SkillOutput,
};
use async_trait::async_trait;

/// Data transformation skill for converting and manipulating data.
pub struct DataTransformSkill {
    manifest: SkillManifest,
}

impl DataTransformSkill {
    /// Create a new data transformation skill.
    pub fn new() -> Self {
        Self {
            manifest: SkillManifest {
                name: "data-transform".to_string(),
                version: "1.0.0".to_string(),
                description: "Transform data between formats and apply operations".to_string(),
                author: Some("drbot".to_string()),
                license: Some("MIT".to_string()),
                homepage: None,
                repository: None,
                tags: vec![
                    "builtin".to_string(),
                    "data".to_string(),
                    "transform".to_string(),
                ],
                inputs: vec![
                    ManifestInput {
                        name: "operation".to_string(),
                        param_type: "string".to_string(),
                        description: "Transformation operation".to_string(),
                        required: true,
                        default: None,
                        pattern: None,
                        enum_values: vec![
                            serde_json::json!("json_to_yaml"),
                            serde_json::json!("yaml_to_json"),
                            serde_json::json!("csv_to_json"),
                            serde_json::json!("flatten"),
                            serde_json::json!("filter"),
                            serde_json::json!("map"),
                        ],
                    },
                    ManifestInput {
                        name: "data".to_string(),
                        param_type: "string".to_string(),
                        description: "Input data".to_string(),
                        required: true,
                        default: None,
                        pattern: None,
                        enum_values: Vec::new(),
                    },
                    ManifestInput {
                        name: "options".to_string(),
                        param_type: "object".to_string(),
                        description: "Operation-specific options".to_string(),
                        required: false,
                        default: Some(serde_json::json!({})),
                        pattern: None,
                        enum_values: Vec::new(),
                    },
                ],
                outputs: vec![ManifestOutput {
                    name: "result".to_string(),
                    output_type: "string".to_string(),
                    description: "Transformed data".to_string(),
                }],
                capabilities: Vec::new(),
                entry_point: None,
                runtime: None,
            },
        }
    }
}

impl Default for DataTransformSkill {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Skill for DataTransformSkill {
    fn manifest(&self) -> &SkillManifest {
        &self.manifest
    }

    async fn execute(&self, input: SkillInput, _ctx: &SkillContext) -> Result<SkillOutput> {
        let operation: String = input.require("operation")?;
        let data: String = input.require("data")?;

        match operation.as_str() {
            "json_to_yaml" => {
                let value: serde_json::Value = serde_json::from_str(&data)
                    .map_err(|e| crate::SkillError::ExecutionFailed(e.to_string()))?;

                // Simple YAML-like output (not full YAML)
                let yaml = json_to_simple_yaml(&value, 0);

                Ok(SkillOutput::new(serde_json::json!({ "yaml": yaml })).with_text(&yaml))
            }

            "csv_to_json" => {
                let lines: Vec<&str> = data.lines().collect();
                if lines.is_empty() {
                    return Ok(SkillOutput::new(serde_json::json!([])));
                }

                let headers: Vec<&str> = lines[0].split(',').map(|s| s.trim()).collect();
                let mut rows = Vec::new();

                for line in &lines[1..] {
                    let values: Vec<&str> = line.split(',').map(|s| s.trim()).collect();
                    let mut row = serde_json::Map::new();

                    for (i, header) in headers.iter().enumerate() {
                        let value = values.get(i).unwrap_or(&"");
                        row.insert(header.to_string(), serde_json::json!(value));
                    }

                    rows.push(serde_json::Value::Object(row));
                }

                let json = serde_json::to_string_pretty(&rows)
                    .map_err(|e| crate::SkillError::ExecutionFailed(e.to_string()))?;

                Ok(SkillOutput::new(serde_json::json!({ "json": rows })).with_text(&json))
            }

            "flatten" => {
                let value: serde_json::Value = serde_json::from_str(&data)
                    .map_err(|e| crate::SkillError::ExecutionFailed(e.to_string()))?;

                let flattened = flatten_json(&value, "");

                let json = serde_json::to_string_pretty(&flattened)
                    .map_err(|e| crate::SkillError::ExecutionFailed(e.to_string()))?;

                Ok(SkillOutput::new(flattened).with_text(&json))
            }

            "filter" => {
                let value: serde_json::Value = serde_json::from_str(&data)
                    .map_err(|e| crate::SkillError::ExecutionFailed(e.to_string()))?;

                let options: serde_json::Value =
                    input.get("options").unwrap_or(serde_json::json!({}));
                let key = options.get("key").and_then(|v| v.as_str()).unwrap_or("");
                let value_filter = options.get("value").and_then(|v| v.as_str());

                if let Some(arr) = value.as_array() {
                    let filtered: Vec<&serde_json::Value> = arr
                        .iter()
                        .filter(|item| {
                            if let Some(obj) = item.as_object() {
                                if let Some(v) = obj.get(key) {
                                    if let Some(filter_val) = value_filter {
                                        return v.as_str() == Some(filter_val)
                                            || v.to_string() == filter_val;
                                    }
                                    return true;
                                }
                            }
                            false
                        })
                        .collect();

                    let json = serde_json::to_string_pretty(&filtered)
                        .map_err(|e| crate::SkillError::ExecutionFailed(e.to_string()))?;

                    Ok(SkillOutput::new(serde_json::json!(filtered))
                        .with_text(&format!("Filtered to {} items", filtered.len())))
                } else {
                    Err(crate::SkillError::ExecutionFailed(
                        "Data must be an array for filter operation".into(),
                    ))
                }
            }

            _ => Err(crate::SkillError::ValidationFailed(format!(
                "Unknown operation: {}",
                operation
            ))),
        }
    }
}

/// Simple JSON to YAML-like conversion.
fn json_to_simple_yaml(value: &serde_json::Value, indent: usize) -> String {
    let prefix = "  ".repeat(indent);

    match value {
        serde_json::Value::Null => "null".to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::String(s) => {
            if s.contains('\n') {
                format!("|\n{}{}", prefix, s.replace('\n', &format!("\n{}", prefix)))
            } else {
                s.clone()
            }
        }
        serde_json::Value::Array(arr) => {
            let items: Vec<String> = arr
                .iter()
                .map(|v| format!("{}- {}", prefix, json_to_simple_yaml(v, indent + 1)))
                .collect();
            items.join("\n")
        }
        serde_json::Value::Object(obj) => {
            let items: Vec<String> = obj
                .iter()
                .map(|(k, v)| {
                    if v.is_object() || v.is_array() {
                        format!("{}{}:\n{}", prefix, k, json_to_simple_yaml(v, indent + 1))
                    } else {
                        format!("{}{}: {}", prefix, k, json_to_simple_yaml(v, indent))
                    }
                })
                .collect();
            items.join("\n")
        }
    }
}

/// Flatten a nested JSON object.
fn flatten_json(value: &serde_json::Value, prefix: &str) -> serde_json::Value {
    let mut result = serde_json::Map::new();

    match value {
        serde_json::Value::Object(obj) => {
            for (k, v) in obj {
                let new_key = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{}.{}", prefix, k)
                };

                if v.is_object() {
                    if let serde_json::Value::Object(flat) = flatten_json(v, &new_key) {
                        for (fk, fv) in flat {
                            result.insert(fk, fv);
                        }
                    }
                } else {
                    result.insert(new_key, v.clone());
                }
            }
        }
        _ => {
            result.insert(prefix.to_string(), value.clone());
        }
    }

    serde_json::Value::Object(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_data_transform_csv_to_json() {
        let skill = DataTransformSkill::new();

        let input = SkillInput::new()
            .with_param("operation", "csv_to_json")
            .with_param("data", "name,age\nAlice,30\nBob,25");

        let ctx = SkillContext::new();
        let result = skill.execute(input, &ctx).await.unwrap();

        let json = &result.data["json"];
        assert!(json.is_array());
        assert_eq!(json.as_array().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn test_data_transform_flatten() {
        let skill = DataTransformSkill::new();

        let input = SkillInput::new()
            .with_param("operation", "flatten")
            .with_param("data", r#"{"a": {"b": 1, "c": 2}}"#);

        let ctx = SkillContext::new();
        let result = skill.execute(input, &ctx).await.unwrap();

        assert_eq!(result.data["a.b"], 1);
        assert_eq!(result.data["a.c"], 2);
    }
}
