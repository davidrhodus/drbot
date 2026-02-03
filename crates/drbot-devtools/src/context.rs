//! Developer context aggregation.

use crate::{DevToolsConfig, DevToolsError, ErrorLog, GitInfo, ProjectInfo, Result};
use std::path::{Path, PathBuf};
use tracing::info;

/// Configuration for developer context.
#[derive(Debug, Clone, Default)]
pub struct DevContextConfig {
    /// Base configuration.
    pub config: DevToolsConfig,
}

/// Aggregated developer context for a project.
#[derive(Debug, Clone)]
pub struct DevContext {
    /// Root path of the project.
    pub root: PathBuf,
    /// Git repository information.
    git_info: Option<GitInfo>,
    /// Project information.
    project_info: ProjectInfo,
    /// Recent error logs.
    error_log: Option<ErrorLog>,
    /// Configuration used.
    config: DevToolsConfig,
}

impl DevContext {
    /// Create context from a path.
    pub async fn from_path(path: impl AsRef<Path>) -> Result<Self> {
        Self::with_config(path, DevToolsConfig::default()).await
    }

    /// Create context with custom configuration.
    pub async fn with_config(path: impl AsRef<Path>, config: DevToolsConfig) -> Result<Self> {
        let root = path.as_ref().to_path_buf();

        if !root.exists() {
            return Err(DevToolsError::PathNotFound(root.display().to_string()));
        }

        // Get git info if enabled and available
        let git_info = if config.include_git {
            crate::git::get_git_info(&root).await.ok()
        } else {
            None
        };

        // Get project info
        let project_info = if config.include_project {
            crate::project::analyze_project(&root, &config).await?
        } else {
            ProjectInfo::default()
        };

        info!(path = %root.display(), "Analyzed developer context");

        Ok(Self {
            root,
            git_info,
            project_info,
            error_log: None,
            config,
        })
    }

    /// Get git information.
    pub fn git(&self) -> Option<&GitInfo> {
        self.git_info.as_ref()
    }

    /// Get project information.
    pub fn project(&self) -> &ProjectInfo {
        &self.project_info
    }

    /// Get error log.
    pub fn errors(&self) -> Option<&ErrorLog> {
        self.error_log.as_ref()
    }

    /// Refresh the context.
    pub async fn refresh(&mut self) -> Result<()> {
        let new = Self::with_config(&self.root, self.config.clone()).await?;
        *self = new;
        Ok(())
    }

    /// Format as a prompt-friendly string.
    pub fn to_prompt(&self) -> String {
        let mut parts = Vec::new();

        parts.push(format!("Project: {}", self.root.display()));

        if let Some(git) = &self.git_info {
            parts.push(format!(
                "Git: {} branch, {} files changed",
                git.branch,
                git.status.changed_count()
            ));

            if !git.recent_commits.is_empty() {
                let commits: Vec<&str> = git
                    .recent_commits
                    .iter()
                    .take(3)
                    .map(|c| c.message.as_str())
                    .collect();
                parts.push(format!("Recent: {}", commits.join("; ")));
            }
        }

        if !self.project_info.languages.is_empty() {
            let langs: Vec<String> = self
                .project_info
                .languages
                .iter()
                .map(|l| format!("{:?}", l))
                .collect();
            parts.push(format!("Languages: {}", langs.join(", ")));
        }

        if !self.project_info.frameworks.is_empty() {
            let frameworks: Vec<String> = self
                .project_info
                .frameworks
                .iter()
                .map(|f| format!("{:?}", f))
                .collect();
            parts.push(format!("Frameworks: {}", frameworks.join(", ")));
        }

        parts.join("\n")
    }

    /// Get a summary of the project structure.
    pub fn structure_summary(&self) -> String {
        let mut lines = Vec::new();

        lines.push(format!("Root: {}", self.root.display()));

        if let Some(readme) = &self.project_info.readme_path {
            lines.push(format!("README: {}", readme.display()));
        }

        if !self.project_info.entry_points.is_empty() {
            lines.push(format!(
                "Entry points: {}",
                self.project_info
                    .entry_points
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }

        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_context_creation() {
        // Test with current directory
        let result = DevContext::from_path(".").await;
        assert!(result.is_ok() || matches!(result, Err(DevToolsError::PathNotFound(_))));
    }
}
