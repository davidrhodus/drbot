//! Skills system for drbot.
//!
//! Provides a framework for creating, discovering, and executing skills
//! that extend the bot's capabilities.
//!
//! # Features
//!
//! - Skill trait for defining capabilities
//! - Skill manifest (skill.toml) parsing
//! - Skill registry for discovery and execution
//! - Skill installation from various sources
//! - Built-in skills for common tasks

pub mod builtin;
mod context;
mod discovery;
mod installation;
mod manifest;
mod registry;
mod skill;

pub use context::{SkillConfig, SkillContext};
pub use discovery::{DiscoveredSkill, SkillDiscovery};
pub use installation::{InstallResult, InstallSource, SkillInstaller};
pub use manifest::{ManifestCapability, ManifestInput, ManifestOutput, SkillManifest};
pub use registry::{RegisteredSkill, SkillRegistry};
pub use skill::{Skill, SkillInput, SkillMetadata, SkillOutput, SkillResult};

use serde::{Deserialize, Serialize};

/// Result type for skill operations.
pub type Result<T> = std::result::Result<T, SkillError>;

/// Skill errors.
#[derive(Debug, thiserror::Error)]
pub enum SkillError {
    #[error("Skill not found: {0}")]
    NotFound(String),
    #[error("Invalid manifest: {0}")]
    InvalidManifest(String),
    #[error("Execution failed: {0}")]
    ExecutionFailed(String),
    #[error("Validation failed: {0}")]
    ValidationFailed(String),
    #[error("Installation failed: {0}")]
    InstallationFailed(String),
    #[error("Missing capability: {0}")]
    MissingCapability(String),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Skills configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillsConfig {
    /// Skills directory.
    #[serde(default = "default_skills_dir")]
    pub skills_dir: String,
    /// Enable built-in skills.
    #[serde(default = "default_builtin")]
    pub enable_builtin: bool,
    /// Enable skill discovery.
    #[serde(default = "default_discovery")]
    pub enable_discovery: bool,
    /// Auto-load skills on startup.
    #[serde(default = "default_auto_load")]
    pub auto_load: bool,
    /// Maximum skill execution time (seconds).
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

fn default_skills_dir() -> String {
    dirs::config_dir()
        .map(|p| p.join("drbot").join("skills").to_string_lossy().to_string())
        .unwrap_or_else(|| "~/.config/drbot/skills".to_string())
}

fn default_builtin() -> bool {
    true
}

fn default_discovery() -> bool {
    true
}

fn default_auto_load() -> bool {
    true
}

fn default_timeout() -> u64 {
    30
}

impl Default for SkillsConfig {
    fn default() -> Self {
        Self {
            skills_dir: default_skills_dir(),
            enable_builtin: default_builtin(),
            enable_discovery: default_discovery(),
            auto_load: default_auto_load(),
            timeout_secs: default_timeout(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skills_config_default() {
        let config = SkillsConfig::default();
        assert!(config.enable_builtin);
        assert!(config.enable_discovery);
        assert_eq!(config.timeout_secs, 30);
    }
}
