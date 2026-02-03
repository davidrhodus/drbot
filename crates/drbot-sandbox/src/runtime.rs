//! Language runtimes for code execution.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Supported programming languages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Language {
    Python,
    JavaScript,
    TypeScript,
    Rust,
    Go,
    Ruby,
    Shell,
    Sql,
}

impl Language {
    /// Get the file extension for this language.
    pub fn extension(&self) -> &'static str {
        match self {
            Language::Python => "py",
            Language::JavaScript => "js",
            Language::TypeScript => "ts",
            Language::Rust => "rs",
            Language::Go => "go",
            Language::Ruby => "rb",
            Language::Shell => "sh",
            Language::Sql => "sql",
        }
    }

    /// Get the default command to run this language.
    pub fn command(&self) -> &'static str {
        match self {
            Language::Python => "python3",
            Language::JavaScript => "node",
            Language::TypeScript => "npx ts-node",
            Language::Rust => "rustc",
            Language::Go => "go run",
            Language::Ruby => "ruby",
            Language::Shell => "bash",
            Language::Sql => "sqlite3",
        }
    }

    /// Parse language from string.
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "python" | "py" | "python3" => Some(Language::Python),
            "javascript" | "js" | "node" => Some(Language::JavaScript),
            "typescript" | "ts" => Some(Language::TypeScript),
            "rust" | "rs" => Some(Language::Rust),
            "go" | "golang" => Some(Language::Go),
            "ruby" | "rb" => Some(Language::Ruby),
            "shell" | "sh" | "bash" => Some(Language::Shell),
            "sql" | "sqlite" => Some(Language::Sql),
            _ => None,
        }
    }

    /// Check if this language requires compilation.
    pub fn requires_compilation(&self) -> bool {
        matches!(self, Language::Rust | Language::Go)
    }
}

impl std::fmt::Display for Language {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Language::Python => write!(f, "Python"),
            Language::JavaScript => write!(f, "JavaScript"),
            Language::TypeScript => write!(f, "TypeScript"),
            Language::Rust => write!(f, "Rust"),
            Language::Go => write!(f, "Go"),
            Language::Ruby => write!(f, "Ruby"),
            Language::Shell => write!(f, "Shell"),
            Language::Sql => write!(f, "SQL"),
        }
    }
}

/// Runtime configuration for a language.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    /// Command to execute code.
    pub command: String,
    /// Command arguments.
    pub args: Vec<String>,
    /// Environment variables.
    pub env: HashMap<String, String>,
    /// Working directory.
    pub working_dir: Option<String>,
    /// Whether to use a REPL.
    pub use_repl: bool,
}

impl RuntimeConfig {
    /// Create a new runtime config.
    pub fn new(command: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            args: Vec::new(),
            env: HashMap::new(),
            working_dir: None,
            use_repl: false,
        }
    }

    /// Add an argument.
    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    /// Set environment variable.
    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }

    /// Set working directory.
    pub fn working_dir(mut self, dir: impl Into<String>) -> Self {
        self.working_dir = Some(dir.into());
        self
    }
}

/// A language runtime for executing code.
#[derive(Debug, Clone)]
pub struct Runtime {
    /// The language this runtime supports.
    pub language: Language,
    /// Runtime configuration.
    pub config: RuntimeConfig,
    /// Whether this runtime is available.
    pub available: bool,
    /// Version string if available.
    pub version: Option<String>,
}

impl Runtime {
    /// Create a new runtime.
    pub fn new(language: Language) -> Self {
        let config = RuntimeConfig::new(language.command());

        Self {
            language,
            config,
            available: false,
            version: None,
        }
    }

    /// Create with custom config.
    pub fn with_config(language: Language, config: RuntimeConfig) -> Self {
        Self {
            language,
            config,
            available: false,
            version: None,
        }
    }

    /// Check if the runtime is available on this system.
    pub async fn check_availability(&mut self) -> bool {
        let command = &self.config.command;
        let parts: Vec<&str> = command.split_whitespace().collect();
        let program = parts.first().copied().unwrap_or(command.as_str());

        match std::process::Command::new(program)
            .arg("--version")
            .output()
        {
            Ok(output) => {
                if output.status.success() {
                    self.available = true;
                    self.version = String::from_utf8(output.stdout)
                        .ok()
                        .map(|s| s.lines().next().unwrap_or("").to_string());
                    true
                } else {
                    self.available = false;
                    false
                }
            }
            Err(_) => {
                self.available = false;
                false
            }
        }
    }

    /// Get the command to run code from a file.
    pub fn run_file_command(&self, file_path: &str) -> Vec<String> {
        let mut parts: Vec<String> = self
            .config
            .command
            .split_whitespace()
            .map(|s| s.to_string())
            .collect();

        parts.extend(self.config.args.iter().cloned());
        parts.push(file_path.to_string());

        parts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_language_extension() {
        assert_eq!(Language::Python.extension(), "py");
        assert_eq!(Language::Rust.extension(), "rs");
    }

    #[test]
    fn test_language_from_str() {
        assert_eq!(Language::from_str("python"), Some(Language::Python));
        assert_eq!(Language::from_str("js"), Some(Language::JavaScript));
        assert_eq!(Language::from_str("unknown"), None);
    }

    #[test]
    fn test_runtime_config_builder() {
        let config = RuntimeConfig::new("python3")
            .arg("-u")
            .env("PYTHONPATH", "/app")
            .working_dir("/tmp");

        assert_eq!(config.command, "python3");
        assert_eq!(config.args, vec!["-u"]);
        assert_eq!(config.env.get("PYTHONPATH"), Some(&"/app".to_string()));
    }

    #[test]
    fn test_runtime_run_file_command() {
        let runtime = Runtime::new(Language::Python);
        let cmd = runtime.run_file_command("/tmp/test.py");
        assert_eq!(cmd, vec!["python3", "/tmp/test.py"]);
    }
}
