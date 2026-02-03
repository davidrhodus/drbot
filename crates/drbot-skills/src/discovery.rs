//! Skill discovery from directories.

use crate::{Result, SkillError, SkillManifest};
use std::path::{Path, PathBuf};

/// A discovered skill.
#[derive(Debug, Clone)]
pub struct DiscoveredSkill {
    /// Skill directory path.
    pub path: PathBuf,
    /// Skill manifest.
    pub manifest: SkillManifest,
}

/// Skill discovery.
pub struct SkillDiscovery {
    /// Search paths.
    search_paths: Vec<PathBuf>,
}

impl SkillDiscovery {
    /// Create a new discovery with default paths.
    pub fn new() -> Self {
        let mut paths = Vec::new();

        // Add default paths
        if let Some(config_dir) = dirs::config_dir() {
            paths.push(config_dir.join("drbot").join("skills"));
        }

        if let Some(data_dir) = dirs::data_dir() {
            paths.push(data_dir.join("drbot").join("skills"));
        }

        Self {
            search_paths: paths,
        }
    }

    /// Add a search path.
    pub fn add_path(&mut self, path: impl AsRef<Path>) {
        self.search_paths.push(path.as_ref().to_path_buf());
    }

    /// Discover skills in all search paths.
    pub fn discover(&self) -> Vec<DiscoveredSkill> {
        let mut skills = Vec::new();

        for search_path in &self.search_paths {
            if !search_path.exists() {
                continue;
            }

            if let Ok(entries) = std::fs::read_dir(search_path) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        if let Ok(skill) = self.discover_in_dir(&path) {
                            skills.push(skill);
                        }
                    }
                }
            }
        }

        skills
    }

    /// Discover a skill in a directory.
    pub fn discover_in_dir(&self, path: &Path) -> Result<DiscoveredSkill> {
        let manifest_path = path.join("skill.toml");

        if !manifest_path.exists() {
            return Err(SkillError::InvalidManifest(format!(
                "No skill.toml found in {}",
                path.display()
            )));
        }

        let manifest = SkillManifest::from_file(&manifest_path)?;
        manifest.validate()?;

        Ok(DiscoveredSkill {
            path: path.to_path_buf(),
            manifest,
        })
    }

    /// Get all search paths.
    pub fn search_paths(&self) -> &[PathBuf] {
        &self.search_paths
    }
}

impl Default for SkillDiscovery {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_skill_discovery() {
        let temp_dir = TempDir::new().unwrap();
        let skill_dir = temp_dir.path().join("test-skill");
        fs::create_dir(&skill_dir).unwrap();

        let manifest = r#"
            name = "test-skill"
            version = "1.0.0"
            description = "A test skill"
        "#;
        fs::write(skill_dir.join("skill.toml"), manifest).unwrap();

        let mut discovery = SkillDiscovery::new();
        discovery.add_path(temp_dir.path());

        let skills = discovery.discover();
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].manifest.name, "test-skill");
    }
}
