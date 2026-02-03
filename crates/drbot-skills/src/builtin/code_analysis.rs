//! Code analysis skill.

use crate::{
    ManifestInput, ManifestOutput, Result, Skill, SkillContext, SkillInput, SkillManifest,
    SkillOutput,
};
use async_trait::async_trait;

/// Code analysis skill for analyzing source code.
pub struct CodeAnalysisSkill {
    manifest: SkillManifest,
}

impl CodeAnalysisSkill {
    /// Create a new code analysis skill.
    pub fn new() -> Self {
        Self {
            manifest: SkillManifest {
                name: "code-analysis".to_string(),
                version: "1.0.0".to_string(),
                description: "Analyze source code for structure, complexity, and issues"
                    .to_string(),
                author: Some("drbot".to_string()),
                license: Some("MIT".to_string()),
                homepage: None,
                repository: None,
                tags: vec![
                    "builtin".to_string(),
                    "code".to_string(),
                    "analysis".to_string(),
                ],
                inputs: vec![
                    ManifestInput {
                        name: "code".to_string(),
                        param_type: "string".to_string(),
                        description: "Source code to analyze".to_string(),
                        required: true,
                        default: None,
                        pattern: None,
                        enum_values: Vec::new(),
                    },
                    ManifestInput {
                        name: "language".to_string(),
                        param_type: "string".to_string(),
                        description: "Programming language".to_string(),
                        required: false,
                        default: Some(serde_json::json!("auto")),
                        pattern: None,
                        enum_values: Vec::new(),
                    },
                ],
                outputs: vec![ManifestOutput {
                    name: "analysis".to_string(),
                    output_type: "object".to_string(),
                    description: "Code analysis results".to_string(),
                }],
                capabilities: Vec::new(),
                entry_point: None,
                runtime: None,
            },
        }
    }
}

impl Default for CodeAnalysisSkill {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Skill for CodeAnalysisSkill {
    fn manifest(&self) -> &SkillManifest {
        &self.manifest
    }

    async fn execute(&self, input: SkillInput, _ctx: &SkillContext) -> Result<SkillOutput> {
        let code: String = input.require("code")?;
        let language: String = input.get("language").unwrap_or_else(|| "auto".to_string());

        // Basic code analysis
        let lines: Vec<&str> = code.lines().collect();
        let total_lines = lines.len();
        let blank_lines = lines.iter().filter(|l| l.trim().is_empty()).count();
        let comment_lines = lines
            .iter()
            .filter(|l| {
                let trimmed = l.trim();
                trimmed.starts_with("//") || trimmed.starts_with("#") || trimmed.starts_with("/*")
            })
            .count();
        let code_lines = total_lines - blank_lines - comment_lines;

        // Detect language if auto
        let detected_language = if language == "auto" {
            detect_language(&code)
        } else {
            language.clone()
        };

        // Count functions/classes (simple heuristic)
        let function_count = count_functions(&code, &detected_language);

        let analysis = serde_json::json!({
            "language": detected_language,
            "metrics": {
                "total_lines": total_lines,
                "code_lines": code_lines,
                "blank_lines": blank_lines,
                "comment_lines": comment_lines,
                "functions": function_count,
            },
            "summary": format!(
                "{} lines of {} code ({} functions)",
                code_lines, detected_language, function_count
            ),
        });

        Ok(SkillOutput::new(analysis).with_text(&format!(
            "Analyzed {} lines of {} code",
            total_lines, detected_language
        )))
    }
}

/// Simple language detection.
fn detect_language(code: &str) -> String {
    // Detect Rust: fn keyword with Rust-like syntax (-> for return type, pub fn, etc.)
    if code.contains("fn ")
        && (code.contains("->") || code.contains("pub fn") || code.contains("let "))
    {
        "rust".to_string()
    } else if code.contains("def ") && code.contains(":") {
        "python".to_string()
    } else if code.contains("function") || code.contains("const ") || code.contains("=>") {
        "javascript".to_string()
    } else if code.contains("public class") || code.contains("private void") {
        "java".to_string()
    } else if code.contains("func ") && code.contains("package") {
        "go".to_string()
    } else {
        "unknown".to_string()
    }
}

/// Count functions (simple heuristic).
fn count_functions(code: &str, language: &str) -> usize {
    match language {
        "rust" => code.matches("fn ").count(),
        "python" => code.matches("def ").count(),
        "javascript" | "typescript" => {
            code.matches("function ").count() + code.matches("=> {").count()
        }
        "java" | "kotlin" => code.matches("void ").count() + code.matches("public ").count(),
        "go" => code.matches("func ").count(),
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_code_analysis_skill() {
        let skill = CodeAnalysisSkill::new();

        let input = SkillInput::new().with_param(
            "code",
            r#"
fn main() {
    println!("Hello, world!");
}

fn add(a: i32, b: i32) -> i32 {
    a + b
}
"#,
        );

        let ctx = SkillContext::new();
        let result = skill.execute(input, &ctx).await.unwrap();

        let analysis = result.data;
        assert_eq!(analysis["language"], "rust");
        assert_eq!(analysis["metrics"]["functions"], 2);
    }
}
