//! Skill registry for discovery and execution.

use crate::{Result, Skill, SkillContext, SkillError, SkillInput, SkillManifest, SkillResult};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// A registered skill.
pub struct RegisteredSkill {
    /// The skill implementation.
    pub skill: Arc<dyn Skill>,
    /// Whether the skill is enabled.
    pub enabled: bool,
    /// Execution count.
    pub execution_count: u64,
}

/// Skill registry.
pub struct SkillRegistry {
    /// Registered skills by name.
    skills: Arc<RwLock<HashMap<String, RegisteredSkill>>>,
    /// Default context.
    default_context: SkillContext,
}

impl SkillRegistry {
    /// Create a new registry.
    pub fn new() -> Self {
        Self {
            skills: Arc::new(RwLock::new(HashMap::new())),
            default_context: SkillContext::default(),
        }
    }

    /// Create with a custom context.
    pub fn with_context(context: SkillContext) -> Self {
        Self {
            skills: Arc::new(RwLock::new(HashMap::new())),
            default_context: context,
        }
    }

    /// Register a skill.
    pub async fn register(&self, skill: Arc<dyn Skill>) -> Result<()> {
        let name = skill.manifest().name.clone();

        let mut skills = self.skills.write().await;
        skills.insert(
            name.clone(),
            RegisteredSkill {
                skill,
                enabled: true,
                execution_count: 0,
            },
        );

        tracing::info!(skill = %name, "Registered skill");
        Ok(())
    }

    /// Unregister a skill.
    pub async fn unregister(&self, name: &str) -> Result<()> {
        let mut skills = self.skills.write().await;
        if skills.remove(name).is_some() {
            tracing::info!(skill = %name, "Unregistered skill");
            Ok(())
        } else {
            Err(SkillError::NotFound(name.to_string()))
        }
    }

    /// Enable a skill.
    pub async fn enable(&self, name: &str) -> Result<()> {
        let mut skills = self.skills.write().await;
        if let Some(skill) = skills.get_mut(name) {
            skill.enabled = true;
            Ok(())
        } else {
            Err(SkillError::NotFound(name.to_string()))
        }
    }

    /// Disable a skill.
    pub async fn disable(&self, name: &str) -> Result<()> {
        let mut skills = self.skills.write().await;
        if let Some(skill) = skills.get_mut(name) {
            skill.enabled = false;
            Ok(())
        } else {
            Err(SkillError::NotFound(name.to_string()))
        }
    }

    /// Get a skill by name.
    pub async fn get(&self, name: &str) -> Option<Arc<dyn Skill>> {
        let skills = self.skills.read().await;
        skills
            .get(name)
            .filter(|s| s.enabled)
            .map(|s| s.skill.clone())
    }

    /// List all registered skills.
    pub async fn list(&self) -> Vec<SkillManifest> {
        let skills = self.skills.read().await;
        skills
            .values()
            .map(|s| s.skill.manifest().clone())
            .collect()
    }

    /// List enabled skills.
    pub async fn list_enabled(&self) -> Vec<SkillManifest> {
        let skills = self.skills.read().await;
        skills
            .values()
            .filter(|s| s.enabled)
            .map(|s| s.skill.manifest().clone())
            .collect()
    }

    /// Execute a skill.
    pub async fn execute(
        &self,
        name: &str,
        input: SkillInput,
        context: Option<&SkillContext>,
    ) -> Result<SkillResult> {
        let ctx = context.unwrap_or(&self.default_context);
        let start = std::time::Instant::now();

        // Get the skill
        let skill = {
            let skills = self.skills.read().await;
            skills
                .get(name)
                .filter(|s| s.enabled)
                .map(|s| s.skill.clone())
                .ok_or_else(|| SkillError::NotFound(name.to_string()))?
        };

        // Validate input
        skill.validate_input(&input)?;

        // Execute
        match skill.execute(input, ctx).await {
            Ok(output) => {
                let elapsed = start.elapsed().as_millis() as u64;

                // Update execution count
                {
                    let mut skills = self.skills.write().await;
                    if let Some(s) = skills.get_mut(name) {
                        s.execution_count += 1;
                    }
                }

                Ok(SkillResult::success(output, elapsed))
            }
            Err(e) => {
                let elapsed = start.elapsed().as_millis() as u64;
                Ok(SkillResult::failure(&e.to_string(), elapsed))
            }
        }
    }

    /// Search skills by tag.
    pub async fn search_by_tag(&self, tag: &str) -> Vec<SkillManifest> {
        let skills = self.skills.read().await;
        skills
            .values()
            .filter(|s| s.enabled && s.skill.manifest().tags.contains(&tag.to_string()))
            .map(|s| s.skill.manifest().clone())
            .collect()
    }

    /// Get skill count.
    pub async fn count(&self) -> usize {
        let skills = self.skills.read().await;
        skills.len()
    }
}

impl Default for SkillRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test skill for registry tests
    struct TestSkill {
        manifest: SkillManifest,
    }

    impl TestSkill {
        fn new(name: &str) -> Self {
            Self {
                manifest: SkillManifest {
                    name: name.to_string(),
                    version: "1.0.0".to_string(),
                    description: "Test skill".to_string(),
                    author: None,
                    license: None,
                    homepage: None,
                    repository: None,
                    tags: vec!["test".to_string()],
                    inputs: Vec::new(),
                    outputs: Vec::new(),
                    capabilities: Vec::new(),
                    entry_point: None,
                    runtime: None,
                },
            }
        }
    }

    #[async_trait::async_trait]
    impl Skill for TestSkill {
        fn manifest(&self) -> &SkillManifest {
            &self.manifest
        }

        async fn execute(
            &self,
            _input: SkillInput,
            _ctx: &SkillContext,
        ) -> Result<crate::SkillOutput> {
            Ok(crate::SkillOutput::text("Test output"))
        }
    }

    #[tokio::test]
    async fn test_registry() {
        let registry = SkillRegistry::new();

        let skill = Arc::new(TestSkill::new("test-skill"));
        registry.register(skill).await.unwrap();

        assert_eq!(registry.count().await, 1);

        let skills = registry.list().await;
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "test-skill");

        let result = registry
            .execute("test-skill", SkillInput::new(), None)
            .await
            .unwrap();
        assert!(result.success);

        registry.unregister("test-skill").await.unwrap();
        assert_eq!(registry.count().await, 0);
    }
}
