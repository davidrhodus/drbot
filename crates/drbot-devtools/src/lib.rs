//! Developer tools integration for drbot.
//!
//! Provides integration with development workflows, IDEs, and developer tools.
//!
//! # Features
//!
//! - Git repository context (current branch, status, recent commits)
//! - Project structure analysis
//! - Language/framework detection
//! - Error log parsing
//! - IDE integration (clipboard, selections)
//! - Code search and navigation
//!
//! # Example
//!
//! ```rust,no_run
//! use drbot_devtools::{DevContext, GitInfo, ProjectInfo};
//!
//! async fn example() {
//!     let ctx = DevContext::from_path("/path/to/project").await.unwrap();
//!
//!     if let Some(git) = ctx.git() {
//!         println!("Branch: {}", git.branch);
//!     }
//!
//!     println!("Detected: {:?}", ctx.project().languages);
//! }
//! ```

mod context;
mod errors;
mod git;
mod project;

pub use context::{DevContext, DevContextConfig};
pub use errors::{ErrorEntry, ErrorLog, ErrorSource};
pub use git::{FileStatus, GitCommit, GitInfo, GitStatus};
pub use project::{BuildSystem, Framework, Language, ProjectInfo};

use serde::{Deserialize, Serialize};

/// Result type for devtools operations.
pub type Result<T> = std::result::Result<T, DevToolsError>;

/// Developer tools errors.
#[derive(Debug, thiserror::Error)]
pub enum DevToolsError {
    #[error("Not a git repository")]
    NotGitRepo,
    #[error("Git command failed: {0}")]
    GitError(String),
    #[error("Path not found: {0}")]
    PathNotFound(String),
    #[error("Parse error: {0}")]
    ParseError(String),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Developer tools configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevToolsConfig {
    /// Include git information.
    pub include_git: bool,
    /// Include project analysis.
    pub include_project: bool,
    /// Maximum recent commits to include.
    pub max_commits: usize,
    /// Include file tree.
    pub include_file_tree: bool,
    /// Maximum tree depth.
    pub max_tree_depth: usize,
    /// Patterns to ignore.
    pub ignore_patterns: Vec<String>,
}

impl Default for DevToolsConfig {
    fn default() -> Self {
        Self {
            include_git: true,
            include_project: true,
            max_commits: 10,
            include_file_tree: true,
            max_tree_depth: 4,
            ignore_patterns: vec![
                "node_modules".to_string(),
                ".git".to_string(),
                "target".to_string(),
                "__pycache__".to_string(),
                ".venv".to_string(),
                "dist".to_string(),
                "build".to_string(),
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = DevToolsConfig::default();
        assert!(config.include_git);
        assert!(config.include_project);
    }
}
