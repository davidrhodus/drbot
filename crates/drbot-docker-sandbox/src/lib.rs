//! Docker container sandboxing for drbot.
//!
//! This crate provides secure code execution in isolated Docker containers.
//!
//! # Features
//!
//! - Container lifecycle management (create, execute, destroy)
//! - Resource limits (memory, CPU, PIDs, disk)
//! - Network isolation (none, bridge, filtered)
//! - Volume mounting with read-only support
//! - Pre-warmed container pools for fast execution
//! - Session-to-container mapping
//!
//! # Example
//!
//! ```rust,no_run
//! use drbot_docker_sandbox::{SandboxManager, SandboxConfig, ExecuteRequest};
//!
//! async fn example() {
//!     let config = SandboxConfig::default();
//!     let manager = SandboxManager::new(config).await.unwrap();
//!
//!     let request = ExecuteRequest::new("python", "print('Hello, World!')");
//!     let result = manager.execute("session1", request).await.unwrap();
//!
//!     println!("Output: {}", result.stdout);
//! }
//! ```

mod container;
mod image;
mod manager;
mod network;
mod pool;
mod session;
mod volume;

pub use container::{ContainerInfo, ContainerLimits, ContainerStatus, SandboxContainer};
pub use image::{ImageConfig, ImageManager, ImagePullProgress};
pub use manager::{ExecuteRequest, ExecuteResult, SandboxManager};
pub use network::{NetworkConfig, NetworkMode};
pub use pool::{ContainerPool, PoolConfig, PoolStatus};
pub use session::{SessionManager, SessionSandbox};
pub use volume::{MountMode, VolumeMount};

use serde::{Deserialize, Serialize};

/// Result type for sandbox operations.
pub type Result<T> = std::result::Result<T, SandboxError>;

/// Sandbox errors.
#[derive(Debug, thiserror::Error)]
pub enum SandboxError {
    #[error("Docker connection failed: {0}")]
    ConnectionFailed(String),
    #[error("Container creation failed: {0}")]
    ContainerCreationFailed(String),
    #[error("Execution failed: {0}")]
    ExecutionFailed(String),
    #[error("Timeout exceeded")]
    Timeout,
    #[error("Resource limit exceeded: {0}")]
    ResourceLimitExceeded(String),
    #[error("Image not found: {0}")]
    ImageNotFound(String),
    #[error("Image pull failed: {0}")]
    ImagePullFailed(String),
    #[error("Container not found: {0}")]
    ContainerNotFound(String),
    #[error("Session not found: {0}")]
    SessionNotFound(String),
    #[error("Pool exhausted")]
    PoolExhausted,
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
    #[error("Docker error: {0}")]
    DockerError(String),
}

/// Global sandbox configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    /// Docker socket path (default: unix:///var/run/docker.sock).
    #[serde(default = "default_docker_socket")]
    pub docker_socket: String,
    /// Default container image.
    #[serde(default = "default_image")]
    pub default_image: String,
    /// Default resource limits.
    #[serde(default)]
    pub default_limits: ContainerLimits,
    /// Default network mode.
    #[serde(default)]
    pub network_mode: NetworkMode,
    /// Enable container pooling.
    #[serde(default = "default_pooling")]
    pub enable_pooling: bool,
    /// Pool configuration.
    #[serde(default)]
    pub pool_config: PoolConfig,
    /// Default execution timeout (seconds).
    #[serde(default = "default_timeout")]
    pub default_timeout_secs: u64,
    /// Working directory inside containers.
    #[serde(default = "default_workdir")]
    pub workdir: String,
    /// Enable audit logging.
    #[serde(default)]
    pub audit_logging: bool,
}

fn default_docker_socket() -> String {
    "unix:///var/run/docker.sock".to_string()
}

fn default_image() -> String {
    "python:3.11-slim".to_string()
}

fn default_pooling() -> bool {
    true
}

fn default_timeout() -> u64 {
    30
}

fn default_workdir() -> String {
    "/workspace".to_string()
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            docker_socket: default_docker_socket(),
            default_image: default_image(),
            default_limits: ContainerLimits::default(),
            network_mode: NetworkMode::default(),
            enable_pooling: default_pooling(),
            pool_config: PoolConfig::default(),
            default_timeout_secs: default_timeout(),
            workdir: default_workdir(),
            audit_logging: false,
        }
    }
}

/// Supported programming languages with their configurations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanguageConfig {
    /// Language name.
    pub name: String,
    /// Docker image to use.
    pub image: String,
    /// Command to run code.
    pub run_command: Vec<String>,
    /// File extension.
    pub extension: String,
    /// REPL command (if supported).
    pub repl_command: Option<Vec<String>>,
}

impl LanguageConfig {
    /// Get configuration for Python.
    pub fn python() -> Self {
        Self {
            name: "python".to_string(),
            image: "python:3.11-slim".to_string(),
            run_command: vec!["python".to_string(), "-c".to_string()],
            extension: ".py".to_string(),
            repl_command: Some(vec!["python".to_string()]),
        }
    }

    /// Get configuration for Node.js.
    pub fn nodejs() -> Self {
        Self {
            name: "nodejs".to_string(),
            image: "node:20-slim".to_string(),
            run_command: vec!["node".to_string(), "-e".to_string()],
            extension: ".js".to_string(),
            repl_command: Some(vec!["node".to_string()]),
        }
    }

    /// Get configuration for Rust.
    pub fn rust() -> Self {
        Self {
            name: "rust".to_string(),
            image: "rust:slim".to_string(),
            run_command: vec![
                "sh".to_string(),
                "-c".to_string(),
                "echo \"$1\" > /tmp/main.rs && rustc /tmp/main.rs -o /tmp/main && /tmp/main"
                    .to_string(),
                "--".to_string(),
            ],
            extension: ".rs".to_string(),
            repl_command: None,
        }
    }

    /// Get configuration for Bash.
    pub fn bash() -> Self {
        Self {
            name: "bash".to_string(),
            image: "alpine:latest".to_string(),
            run_command: vec!["sh".to_string(), "-c".to_string()],
            extension: ".sh".to_string(),
            repl_command: Some(vec!["sh".to_string()]),
        }
    }

    /// Get language config by name.
    pub fn by_name(name: &str) -> Option<Self> {
        match name.to_lowercase().as_str() {
            "python" | "py" => Some(Self::python()),
            "javascript" | "js" | "node" | "nodejs" => Some(Self::nodejs()),
            "rust" | "rs" => Some(Self::rust()),
            "bash" | "sh" | "shell" => Some(Self::bash()),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sandbox_config_default() {
        let config = SandboxConfig::default();
        assert_eq!(config.default_timeout_secs, 30);
        assert!(config.enable_pooling);
    }

    #[test]
    fn test_language_config() {
        let python = LanguageConfig::by_name("python");
        assert!(python.is_some());
        assert_eq!(python.unwrap().name, "python");

        let unknown = LanguageConfig::by_name("unknown");
        assert!(unknown.is_none());
    }
}
