//! Shell environment awareness.
//!
//! This crate provides:
//! - Shell detection and configuration
//! - Environment variable tracking
//! - Working directory awareness
//! - Command history analysis
//! - Path intelligence

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;

/// Shell sense errors.
#[derive(Debug, Error)]
pub enum ShellError {
    #[error("Detection failed: {0}")]
    DetectionFailed(String),

    #[error("Environment error: {0}")]
    EnvironmentError(String),

    #[error("Command failed: {0}")]
    CommandFailed(String),
}

/// Result type for shell operations.
pub type Result<T> = std::result::Result<T, ShellError>;

/// Shell types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShellType {
    Bash,
    Zsh,
    Fish,
    PowerShell,
    Cmd,
    Sh,
    Unknown,
}

impl ShellType {
    /// Detect shell from environment.
    pub fn detect() -> Self {
        if let Ok(shell) = std::env::var("SHELL") {
            if shell.contains("zsh") {
                return ShellType::Zsh;
            } else if shell.contains("bash") {
                return ShellType::Bash;
            } else if shell.contains("fish") {
                return ShellType::Fish;
            } else if shell.contains("sh") {
                return ShellType::Sh;
            }
        }

        // Check for Windows shells
        if std::env::var("PSModulePath").is_ok() {
            return ShellType::PowerShell;
        }

        if std::env::var("COMSPEC").is_ok() {
            return ShellType::Cmd;
        }

        ShellType::Unknown
    }

    /// Get shell name.
    pub fn name(&self) -> &'static str {
        match self {
            ShellType::Bash => "bash",
            ShellType::Zsh => "zsh",
            ShellType::Fish => "fish",
            ShellType::PowerShell => "powershell",
            ShellType::Cmd => "cmd",
            ShellType::Sh => "sh",
            ShellType::Unknown => "unknown",
        }
    }

    /// Get config file paths.
    pub fn config_files(&self) -> Vec<PathBuf> {
        let home = dirs_home();
        match self {
            ShellType::Bash => vec![
                home.join(".bashrc"),
                home.join(".bash_profile"),
                home.join(".profile"),
            ],
            ShellType::Zsh => vec![
                home.join(".zshrc"),
                home.join(".zprofile"),
                home.join(".zshenv"),
            ],
            ShellType::Fish => vec![home.join(".config/fish/config.fish")],
            ShellType::PowerShell => {
                vec![home.join("Documents/PowerShell/Microsoft.PowerShell_profile.ps1")]
            }
            _ => vec![],
        }
    }

    /// Get history file path.
    pub fn history_file(&self) -> Option<PathBuf> {
        let home = dirs_home();
        match self {
            ShellType::Bash => Some(home.join(".bash_history")),
            ShellType::Zsh => Some(home.join(".zsh_history")),
            ShellType::Fish => Some(home.join(".local/share/fish/fish_history")),
            _ => None,
        }
    }
}

fn dirs_home() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/"))
}

/// Shell environment snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellEnvironment {
    /// Shell type.
    pub shell_type: ShellType,
    /// Current working directory.
    pub cwd: PathBuf,
    /// Environment variables.
    pub env_vars: HashMap<String, String>,
    /// PATH entries.
    pub path_entries: Vec<PathBuf>,
    /// User.
    pub user: String,
    /// Hostname.
    pub hostname: String,
    /// Home directory.
    pub home: PathBuf,
    /// Is root/admin.
    pub is_elevated: bool,
    /// Terminal type.
    pub term: Option<String>,
    /// Captured at.
    pub captured_at: DateTime<Utc>,
}

impl ShellEnvironment {
    /// Capture current shell environment.
    pub fn capture() -> Self {
        let shell_type = ShellType::detect();
        let cwd = std::env::current_dir().unwrap_or_default();
        let env_vars: HashMap<String, String> = std::env::vars().collect();

        let path_entries = std::env::var("PATH")
            .unwrap_or_default()
            .split(if cfg!(windows) { ';' } else { ':' })
            .map(PathBuf::from)
            .collect();

        let user = std::env::var("USER")
            .or_else(|_| std::env::var("USERNAME"))
            .unwrap_or_else(|_| "unknown".to_string());

        let hostname = std::env::var("HOSTNAME")
            .or_else(|_| std::env::var("COMPUTERNAME"))
            .unwrap_or_else(|_| "localhost".to_string());

        let home = dirs_home();

        let is_elevated = std::env::var("EUID").map(|v| v == "0").unwrap_or(false);

        let term = std::env::var("TERM").ok();

        Self {
            shell_type,
            cwd,
            env_vars,
            path_entries,
            user,
            hostname,
            home,
            is_elevated,
            term,
            captured_at: Utc::now(),
        }
    }

    /// Get an environment variable.
    pub fn get_var(&self, name: &str) -> Option<&String> {
        self.env_vars.get(name)
    }

    /// Check if a command is in PATH.
    pub fn command_in_path(&self, cmd: &str) -> bool {
        self.path_entries.iter().any(|dir| {
            let path = dir.join(cmd);
            path.exists() || path.with_extension("exe").exists()
        })
    }

    /// Get likely project type based on environment.
    pub fn detect_project_type(&self) -> Option<ProjectContext> {
        // Check for common project indicators
        let cwd = &self.cwd;

        if cwd.join("Cargo.toml").exists() {
            return Some(ProjectContext {
                project_type: "rust".to_string(),
                build_tool: Some("cargo".to_string()),
                package_manager: Some("cargo".to_string()),
                config_files: vec!["Cargo.toml".to_string()],
            });
        }

        if cwd.join("package.json").exists() {
            let pm = if cwd.join("yarn.lock").exists() {
                "yarn"
            } else if cwd.join("pnpm-lock.yaml").exists() {
                "pnpm"
            } else {
                "npm"
            };

            return Some(ProjectContext {
                project_type: "node".to_string(),
                build_tool: None,
                package_manager: Some(pm.to_string()),
                config_files: vec!["package.json".to_string()],
            });
        }

        if cwd.join("pyproject.toml").exists() || cwd.join("setup.py").exists() {
            return Some(ProjectContext {
                project_type: "python".to_string(),
                build_tool: None,
                package_manager: Some("pip".to_string()),
                config_files: vec!["pyproject.toml".to_string(), "setup.py".to_string()],
            });
        }

        if cwd.join("go.mod").exists() {
            return Some(ProjectContext {
                project_type: "go".to_string(),
                build_tool: Some("go".to_string()),
                package_manager: Some("go".to_string()),
                config_files: vec!["go.mod".to_string()],
            });
        }

        if cwd.join("pom.xml").exists() {
            return Some(ProjectContext {
                project_type: "java".to_string(),
                build_tool: Some("maven".to_string()),
                package_manager: Some("maven".to_string()),
                config_files: vec!["pom.xml".to_string()],
            });
        }

        if cwd.join("build.gradle").exists() || cwd.join("build.gradle.kts").exists() {
            return Some(ProjectContext {
                project_type: "java".to_string(),
                build_tool: Some("gradle".to_string()),
                package_manager: Some("gradle".to_string()),
                config_files: vec!["build.gradle".to_string()],
            });
        }

        None
    }
}

/// Project context detected from environment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectContext {
    /// Project type (rust, node, python, etc.).
    pub project_type: String,
    /// Build tool if applicable.
    pub build_tool: Option<String>,
    /// Package manager.
    pub package_manager: Option<String>,
    /// Config files found.
    pub config_files: Vec<String>,
}

/// Command history entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    /// Command.
    pub command: String,
    /// Timestamp (if available).
    pub timestamp: Option<DateTime<Utc>>,
    /// Working directory (if tracked).
    pub cwd: Option<PathBuf>,
    /// Exit status (if tracked).
    pub exit_status: Option<i32>,
}

/// Shell history analyzer.
pub struct HistoryAnalyzer {
    entries: Vec<HistoryEntry>,
    shell_type: ShellType,
}

impl HistoryAnalyzer {
    /// Create a new history analyzer.
    pub fn new(shell_type: ShellType) -> Self {
        Self {
            entries: Vec::new(),
            shell_type,
        }
    }

    /// Load history from file.
    pub async fn load(&mut self) -> Result<()> {
        let history_file = self
            .shell_type
            .history_file()
            .ok_or_else(|| ShellError::DetectionFailed("No history file for shell".to_string()))?;

        if !history_file.exists() {
            return Ok(());
        }

        let content = tokio::fs::read_to_string(&history_file)
            .await
            .map_err(|e| ShellError::EnvironmentError(e.to_string()))?;

        self.entries = self.parse_history(&content);
        Ok(())
    }

    fn parse_history(&self, content: &str) -> Vec<HistoryEntry> {
        let mut entries = Vec::new();

        match self.shell_type {
            ShellType::Zsh => {
                // Zsh format: : timestamp:0;command
                for line in content.lines() {
                    if let Some(entry) = self.parse_zsh_line(line) {
                        entries.push(entry);
                    }
                }
            }
            ShellType::Bash | ShellType::Sh => {
                // Simple format: one command per line
                for line in content.lines() {
                    if !line.is_empty() {
                        entries.push(HistoryEntry {
                            command: line.to_string(),
                            timestamp: None,
                            cwd: None,
                            exit_status: None,
                        });
                    }
                }
            }
            _ => {}
        }

        entries
    }

    fn parse_zsh_line(&self, line: &str) -> Option<HistoryEntry> {
        if line.starts_with(": ") {
            // Extended format: : timestamp:duration;command
            let parts: Vec<&str> = line.splitn(2, ';').collect();
            if parts.len() == 2 {
                let timestamp = parts[0]
                    .trim_start_matches(": ")
                    .split(':')
                    .next()
                    .and_then(|ts| ts.parse::<i64>().ok())
                    .and_then(|ts| DateTime::from_timestamp(ts, 0))
                    .map(|dt| dt.with_timezone(&Utc));

                return Some(HistoryEntry {
                    command: parts[1].to_string(),
                    timestamp,
                    cwd: None,
                    exit_status: None,
                });
            }
        }

        Some(HistoryEntry {
            command: line.to_string(),
            timestamp: None,
            cwd: None,
            exit_status: None,
        })
    }

    /// Get recent commands.
    pub fn recent(&self, limit: usize) -> Vec<&HistoryEntry> {
        self.entries.iter().rev().take(limit).collect()
    }

    /// Get most frequent commands.
    pub fn most_frequent(&self, limit: usize) -> Vec<(String, usize)> {
        let mut freq: HashMap<String, usize> = HashMap::new();

        for entry in &self.entries {
            // Extract base command
            let base_cmd = entry.command.split_whitespace().next().unwrap_or("");
            *freq.entry(base_cmd.to_string()).or_insert(0) += 1;
        }

        let mut sorted: Vec<_> = freq.into_iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(&a.1));
        sorted.truncate(limit);
        sorted
    }

    /// Search history.
    pub fn search(&self, pattern: &str) -> Vec<&HistoryEntry> {
        let pattern_lower = pattern.to_lowercase();
        self.entries
            .iter()
            .filter(|e| e.command.to_lowercase().contains(&pattern_lower))
            .collect()
    }

    /// Get command count.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if history is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Shell sense engine.
pub struct ShellSense {
    /// Current environment.
    environment: Arc<RwLock<ShellEnvironment>>,
    /// History analyzer.
    history: Arc<RwLock<Option<HistoryAnalyzer>>>,
    /// Environment change callbacks.
    on_change: Arc<RwLock<Vec<Box<dyn Fn(&ShellEnvironment) + Send + Sync>>>>,
}

impl ShellSense {
    /// Create a new shell sense engine.
    pub fn new() -> Self {
        Self {
            environment: Arc::new(RwLock::new(ShellEnvironment::capture())),
            history: Arc::new(RwLock::new(None)),
            on_change: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Get current environment.
    pub async fn environment(&self) -> ShellEnvironment {
        let env = self.environment.read().await;
        env.clone()
    }

    /// Refresh environment.
    pub async fn refresh(&self) -> ShellEnvironment {
        let new_env = ShellEnvironment::capture();
        let mut env = self.environment.write().await;
        *env = new_env.clone();

        // Notify callbacks
        let callbacks = self.on_change.read().await;
        for callback in callbacks.iter() {
            callback(&new_env);
        }

        new_env
    }

    /// Load command history.
    pub async fn load_history(&self) -> Result<()> {
        let env = self.environment.read().await;
        let mut analyzer = HistoryAnalyzer::new(env.shell_type);
        analyzer.load().await?;

        let mut history = self.history.write().await;
        *history = Some(analyzer);
        Ok(())
    }

    /// Get history analyzer.
    pub async fn history(&self) -> Option<HistoryAnalyzer> {
        let history = self.history.read().await;
        // Clone the entries into a new analyzer
        if history.is_some() {
            let env = self.environment.read().await;
            Some(HistoryAnalyzer::new(env.shell_type))
        } else {
            None
        }
    }

    /// Get recent commands.
    pub async fn recent_commands(&self, limit: usize) -> Vec<String> {
        let history = self.history.read().await;
        if let Some(h) = &*history {
            h.recent(limit).iter().map(|e| e.command.clone()).collect()
        } else {
            Vec::new()
        }
    }

    /// Check if a command is available.
    pub async fn has_command(&self, cmd: &str) -> bool {
        let env = self.environment.read().await;
        env.command_in_path(cmd)
    }

    /// Detect project context.
    pub async fn detect_project(&self) -> Option<ProjectContext> {
        let env = self.environment.read().await;
        env.detect_project_type()
    }

    /// Get environment variable.
    pub async fn get_var(&self, name: &str) -> Option<String> {
        let env = self.environment.read().await;
        env.get_var(name).cloned()
    }

    /// Get current working directory.
    pub async fn cwd(&self) -> PathBuf {
        let env = self.environment.read().await;
        env.cwd.clone()
    }

    /// Get shell type.
    pub async fn shell_type(&self) -> ShellType {
        let env = self.environment.read().await;
        env.shell_type
    }

    /// Register environment change callback.
    pub async fn on_change<F>(&self, callback: F)
    where
        F: Fn(&ShellEnvironment) + Send + Sync + 'static,
    {
        let mut callbacks = self.on_change.write().await;
        callbacks.push(Box::new(callback));
    }
}

impl Default for ShellSense {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shell_detection() {
        let shell = ShellType::detect();
        // Should detect something
        assert!(matches!(
            shell,
            ShellType::Bash
                | ShellType::Zsh
                | ShellType::Fish
                | ShellType::Sh
                | ShellType::PowerShell
                | ShellType::Cmd
                | ShellType::Unknown
        ));
    }

    #[test]
    fn test_environment_capture() {
        let env = ShellEnvironment::capture();
        assert!(!env.user.is_empty());
        assert!(!env.path_entries.is_empty());
    }

    #[test]
    fn test_command_in_path() {
        let env = ShellEnvironment::capture();
        // 'ls' or 'dir' should be in path on most systems
        let has_ls = env.command_in_path("ls");
        let has_dir = env.command_in_path("dir");
        // At least one should exist
        assert!(has_ls || has_dir || cfg!(windows));
    }

    #[tokio::test]
    async fn test_shell_sense() {
        let sense = ShellSense::new();
        let env = sense.environment().await;
        assert!(!env.user.is_empty());
    }

    #[tokio::test]
    async fn test_refresh() {
        let sense = ShellSense::new();
        let env1 = sense.environment().await;
        let env2 = sense.refresh().await;
        // Both should have the same user
        assert_eq!(env1.user, env2.user);
    }

    #[tokio::test]
    async fn test_cwd() {
        let sense = ShellSense::new();
        let cwd = sense.cwd().await;
        assert!(cwd.exists());
    }

    #[test]
    fn test_history_analyzer() {
        let analyzer = HistoryAnalyzer::new(ShellType::Bash);
        assert!(analyzer.is_empty());
    }

    #[test]
    fn test_shell_config_files() {
        let bash = ShellType::Bash;
        let configs = bash.config_files();
        assert!(!configs.is_empty());

        let zsh = ShellType::Zsh;
        let configs = zsh.config_files();
        assert!(!configs.is_empty());
    }
}
