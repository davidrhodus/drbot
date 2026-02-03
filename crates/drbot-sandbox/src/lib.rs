//! Secure code execution sandbox for drbot.
//!
//! Provides isolated execution environments for running user code safely.
//!
//! # Features
//!
//! - Process isolation with resource limits
//! - Timeout enforcement
//! - Network and filesystem restrictions
//! - Language-specific runtimes
//! - Output capture and streaming
//!
//! # Example
//!
//! ```rust,no_run
//! use drbot_sandbox::{Sandbox, SandboxConfig, Language};
//!
//! async fn example() {
//!     let sandbox = Sandbox::new(SandboxConfig::default()).await.unwrap();
//!
//!     let result = sandbox.execute(
//!         Language::Python,
//!         "print('Hello, World!')",
//!     ).await.unwrap();
//!
//!     println!("Output: {}", result.stdout);
//! }
//! ```

mod executor;
mod limits;
mod runtime;
mod sandbox;

pub use executor::{CodeExecution, ExecutionResult, ExecutionStatus};
pub use limits::{FilesystemPolicy, NetworkPolicy, ResourceLimits};
pub use runtime::{Language, Runtime, RuntimeConfig};
pub use sandbox::{Sandbox, SandboxConfig, SandboxState};

/// Result type for sandbox operations.
pub type Result<T> = std::result::Result<T, SandboxError>;

/// Sandbox errors.
#[derive(Debug, thiserror::Error)]
pub enum SandboxError {
    #[error("Execution timed out")]
    Timeout,
    #[error("Memory limit exceeded")]
    MemoryLimitExceeded,
    #[error("Execution failed: {0}")]
    ExecutionFailed(String),
    #[error("Language not supported: {0}")]
    UnsupportedLanguage(String),
    #[error("Sandbox creation failed: {0}")]
    CreationFailed(String),
    #[error("Permission denied: {0}")]
    PermissionDenied(String),
    #[error("Resource exhausted: {0}")]
    ResourceExhausted(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_sandbox_basic() {
        let sandbox = Sandbox::new(SandboxConfig::default()).await.unwrap();
        assert_eq!(sandbox.state(), SandboxState::Ready);
    }
}
