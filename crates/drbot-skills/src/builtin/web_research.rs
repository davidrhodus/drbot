//! Web research skill.

use crate::{
    ManifestCapability, ManifestInput, ManifestOutput, Result, Skill, SkillContext, SkillInput,
    SkillManifest, SkillOutput,
};
use async_trait::async_trait;

/// Web research skill for searching and summarizing web content.
pub struct WebResearchSkill {
    manifest: SkillManifest,
}

impl WebResearchSkill {
    /// Create a new web research skill.
    pub fn new() -> Self {
        Self {
            manifest: SkillManifest {
                name: "web-research".to_string(),
                version: "1.0.0".to_string(),
                description: "Search the web and summarize findings".to_string(),
                author: Some("drbot".to_string()),
                license: Some("MIT".to_string()),
                homepage: None,
                repository: None,
                tags: vec![
                    "builtin".to_string(),
                    "web".to_string(),
                    "search".to_string(),
                ],
                inputs: vec![
                    ManifestInput {
                        name: "query".to_string(),
                        param_type: "string".to_string(),
                        description: "Search query".to_string(),
                        required: true,
                        default: None,
                        pattern: None,
                        enum_values: Vec::new(),
                    },
                    ManifestInput {
                        name: "max_results".to_string(),
                        param_type: "number".to_string(),
                        description: "Maximum number of results".to_string(),
                        required: false,
                        default: Some(serde_json::json!(10)),
                        pattern: None,
                        enum_values: Vec::new(),
                    },
                ],
                outputs: vec![ManifestOutput {
                    name: "results".to_string(),
                    output_type: "array".to_string(),
                    description: "Search results with summaries".to_string(),
                }],
                capabilities: vec![ManifestCapability::required("network")],
                entry_point: None,
                runtime: None,
            },
        }
    }
}

impl Default for WebResearchSkill {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Skill for WebResearchSkill {
    fn manifest(&self) -> &SkillManifest {
        &self.manifest
    }

    async fn execute(&self, input: SkillInput, ctx: &SkillContext) -> Result<SkillOutput> {
        // Check capability
        if !ctx.has_capability("network") {
            return Err(crate::SkillError::MissingCapability("network".into()));
        }

        let query: String = input.require("query")?;
        let max_results: u32 = input.get("max_results").unwrap_or(10);

        // In a real implementation, this would perform web search
        // For now, return a placeholder result

        let results = serde_json::json!([
            {
                "title": format!("Search results for: {}", query),
                "url": "https://example.com",
                "snippet": format!("This is a placeholder result for query: {}", query),
            }
        ]);

        Ok(SkillOutput::new(results)
            .with_text(&format!("Found results for: {}", query))
            .with_metadata("query", &query)
            .with_metadata("max_results", max_results))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_web_research_skill() {
        let skill = WebResearchSkill::new();

        assert_eq!(skill.manifest().name, "web-research");
        assert!(skill.manifest().requires_capability("network"));

        let input = SkillInput::new().with_param("query", "test search");

        let ctx = SkillContext::new().with_capability("network");

        let result = skill.execute(input, &ctx).await.unwrap();
        assert!(result.text.is_some());
    }
}
