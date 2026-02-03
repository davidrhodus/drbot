//! Skill execution context.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Skill execution context.
#[derive(Debug, Clone)]
pub struct SkillContext {
    /// Skill configuration.
    pub config: SkillConfig,
    /// Available capabilities.
    pub capabilities: Vec<String>,
    /// Environment variables.
    pub env: HashMap<String, String>,
    /// Working directory.
    pub workdir: Option<PathBuf>,
    /// Session ID (if applicable).
    pub session_id: Option<String>,
    /// User ID (if applicable).
    pub user_id: Option<String>,
}

impl SkillContext {
    /// Create a new context.
    pub fn new() -> Self {
        Self {
            config: SkillConfig::default(),
            capabilities: Vec::new(),
            env: HashMap::new(),
            workdir: None,
            session_id: None,
            user_id: None,
        }
    }

    /// Add a capability.
    pub fn with_capability(mut self, capability: &str) -> Self {
        self.capabilities.push(capability.to_string());
        self
    }

    /// Set working directory.
    pub fn with_workdir(mut self, workdir: impl Into<PathBuf>) -> Self {
        self.workdir = Some(workdir.into());
        self
    }

    /// Set session ID.
    pub fn with_session(mut self, session_id: &str) -> Self {
        self.session_id = Some(session_id.to_string());
        self
    }

    /// Set user ID.
    pub fn with_user(mut self, user_id: &str) -> Self {
        self.user_id = Some(user_id.to_string());
        self
    }

    /// Set environment variable.
    pub fn with_env(mut self, key: &str, value: &str) -> Self {
        self.env.insert(key.to_string(), value.to_string());
        self
    }

    /// Check if a capability is available.
    pub fn has_capability(&self, name: &str) -> bool {
        self.capabilities.iter().any(|c| c == name)
    }

    /// Get the working directory.
    pub fn working_dir(&self) -> PathBuf {
        self.workdir
            .clone()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
    }
}

impl Default for SkillContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Skill configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillConfig {
    /// Timeout for skill execution (seconds).
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    /// Maximum output size (bytes).
    #[serde(default = "default_max_output")]
    pub max_output_bytes: usize,
    /// Allow network access.
    #[serde(default)]
    pub allow_network: bool,
    /// Allow filesystem access.
    #[serde(default)]
    pub allow_filesystem: bool,
    /// Sandboxed execution.
    #[serde(default)]
    pub sandboxed: bool,
}

fn default_timeout() -> u64 {
    30
}

fn default_max_output() -> usize {
    1024 * 1024 // 1 MB
}

impl Default for SkillConfig {
    fn default() -> Self {
        Self {
            timeout_secs: default_timeout(),
            max_output_bytes: default_max_output(),
            allow_network: false,
            allow_filesystem: false,
            sandboxed: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skill_context() {
        let ctx = SkillContext::new()
            .with_capability("network")
            .with_capability("browser")
            .with_session("session123")
            .with_user("user456");

        assert!(ctx.has_capability("network"));
        assert!(ctx.has_capability("browser"));
        assert!(!ctx.has_capability("filesystem"));
        assert_eq!(ctx.session_id, Some("session123".to_string()));
    }

    #[test]
    fn test_skill_config_default() {
        let config = SkillConfig::default();
        assert_eq!(config.timeout_secs, 30);
        assert!(!config.allow_network);
        assert!(!config.sandboxed);
    }
}
