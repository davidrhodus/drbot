//! Main sandbox implementation.

use crate::executor::{CodeExecution, ExecutionResult};
use crate::limits::{FilesystemPolicy, NetworkPolicy, ResourceLimits};
use crate::runtime::{Language, Runtime};
use crate::{Result, SandboxError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
use uuid::Uuid;

/// Sandbox configuration.
#[derive(Debug, Clone)]
pub struct SandboxConfig {
    /// Resource limits.
    pub limits: ResourceLimits,
    /// Network policy.
    pub network_policy: NetworkPolicy,
    /// Filesystem policy.
    pub filesystem_policy: FilesystemPolicy,
    /// Sandbox root directory.
    pub sandbox_root: String,
    /// Whether to cleanup after execution.
    pub auto_cleanup: bool,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        let sandbox_root = std::env::temp_dir()
            .join("drbot-sandbox")
            .to_string_lossy()
            .to_string();

        Self {
            limits: ResourceLimits::default(),
            network_policy: NetworkPolicy::deny_all(),
            filesystem_policy: FilesystemPolicy::temp_only(sandbox_root.clone()),
            sandbox_root,
            auto_cleanup: true,
        }
    }
}

impl SandboxConfig {
    /// Create a secure config (strict limits, no network).
    pub fn secure() -> Self {
        Self {
            limits: ResourceLimits::strict(),
            network_policy: NetworkPolicy::deny_all(),
            filesystem_policy: FilesystemPolicy::deny_all(),
            ..Default::default()
        }
    }

    /// Create a permissive config (for trusted code).
    pub fn permissive() -> Self {
        Self {
            limits: ResourceLimits::relaxed(),
            network_policy: NetworkPolicy::allow_all(),
            filesystem_policy: FilesystemPolicy::default(),
            ..Default::default()
        }
    }

    /// Set resource limits.
    pub fn with_limits(mut self, limits: ResourceLimits) -> Self {
        self.limits = limits;
        self
    }

    /// Set network policy.
    pub fn with_network_policy(mut self, policy: NetworkPolicy) -> Self {
        self.network_policy = policy;
        self
    }

    /// Set filesystem policy.
    pub fn with_filesystem_policy(mut self, policy: FilesystemPolicy) -> Self {
        self.filesystem_policy = policy;
        self
    }
}

/// Sandbox state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SandboxState {
    /// Ready to execute code.
    Ready,
    /// Currently executing.
    Executing,
    /// Cleanup needed.
    Dirty,
    /// Error state.
    Error,
}

/// Secure sandbox for code execution.
pub struct Sandbox {
    id: String,
    config: SandboxConfig,
    state: Arc<RwLock<SandboxState>>,
    runtimes: HashMap<Language, Runtime>,
    working_dir: String,
}

impl Sandbox {
    /// Create a new sandbox.
    pub async fn new(config: SandboxConfig) -> Result<Self> {
        let id = Uuid::new_v4().to_string();
        let working_dir = format!("{}/{}", config.sandbox_root, &id[..8]);

        // Create sandbox directory
        tokio::fs::create_dir_all(&working_dir).await.map_err(|e| {
            SandboxError::CreationFailed(format!("Failed to create sandbox dir: {}", e))
        })?;

        info!("Created sandbox {} at {}", &id[..8], working_dir);

        let mut sandbox = Self {
            id,
            config,
            state: Arc::new(RwLock::new(SandboxState::Ready)),
            runtimes: HashMap::new(),
            working_dir,
        };

        // Initialize available runtimes
        sandbox.init_runtimes().await;

        Ok(sandbox)
    }

    /// Initialize available runtimes.
    async fn init_runtimes(&mut self) {
        let languages = [
            Language::Python,
            Language::JavaScript,
            Language::TypeScript,
            Language::Ruby,
            Language::Shell,
        ];

        for lang in languages {
            let mut runtime = Runtime::new(lang);
            if runtime.check_availability().await {
                debug!(
                    "Runtime available: {} ({})",
                    lang,
                    runtime.version.as_deref().unwrap_or("unknown version")
                );
                self.runtimes.insert(lang, runtime);
            }
        }

        info!("{} runtimes available", self.runtimes.len());
    }

    /// Get sandbox ID.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Get current state.
    pub fn state(&self) -> SandboxState {
        // Use try_read to avoid async in non-async context
        *self.state.try_read().unwrap_or_else(|_| {
            // If we can't read, return a default
            panic!("Failed to read sandbox state")
        })
    }

    /// Get available languages.
    pub fn available_languages(&self) -> Vec<Language> {
        self.runtimes.keys().cloned().collect()
    }

    /// Check if a language is available.
    pub fn is_language_available(&self, language: Language) -> bool {
        self.runtimes.contains_key(&language)
    }

    /// Execute code.
    pub async fn execute(&self, language: Language, code: &str) -> Result<ExecutionResult> {
        // Check state
        {
            let mut state = self.state.write().await;
            if *state != SandboxState::Ready {
                return Err(SandboxError::ExecutionFailed(
                    "Sandbox not in ready state".into(),
                ));
            }
            *state = SandboxState::Executing;
        }

        // Get runtime
        let runtime = self.runtimes.get(&language).ok_or_else(|| {
            SandboxError::UnsupportedLanguage(format!("{} not available", language))
        })?;

        // Create execution
        let execution = CodeExecution::new(language, code)
            .with_limits(self.config.limits.clone())
            .with_working_dir(&self.working_dir);

        // Execute
        let result = execution.execute(runtime).await;

        // Update state
        {
            let mut state = self.state.write().await;
            *state = if self.config.auto_cleanup {
                SandboxState::Ready
            } else {
                SandboxState::Dirty
            };
        }

        result
    }

    /// Execute code with custom limits.
    pub async fn execute_with_limits(
        &self,
        language: Language,
        code: &str,
        limits: ResourceLimits,
    ) -> Result<ExecutionResult> {
        // Check state
        {
            let mut state = self.state.write().await;
            if *state != SandboxState::Ready {
                return Err(SandboxError::ExecutionFailed(
                    "Sandbox not in ready state".into(),
                ));
            }
            *state = SandboxState::Executing;
        }

        // Get runtime
        let runtime = self.runtimes.get(&language).ok_or_else(|| {
            SandboxError::UnsupportedLanguage(format!("{} not available", language))
        })?;

        // Create execution with custom limits
        let execution = CodeExecution::new(language, code)
            .with_limits(limits)
            .with_working_dir(&self.working_dir);

        // Execute
        let result = execution.execute(runtime).await;

        // Update state
        {
            let mut state = self.state.write().await;
            *state = if self.config.auto_cleanup {
                SandboxState::Ready
            } else {
                SandboxState::Dirty
            };
        }

        result
    }

    /// Cleanup the sandbox.
    pub async fn cleanup(&self) -> Result<()> {
        let mut state = self.state.write().await;

        if *state == SandboxState::Executing {
            warn!("Cannot cleanup while executing");
            return Err(SandboxError::ExecutionFailed(
                "Execution in progress".into(),
            ));
        }

        // Remove working directory
        if let Err(e) = tokio::fs::remove_dir_all(&self.working_dir).await {
            warn!("Failed to cleanup sandbox dir: {}", e);
        }

        *state = SandboxState::Ready;
        info!("Sandbox {} cleaned up", &self.id[..8]);

        Ok(())
    }

    /// Get the working directory.
    pub fn working_dir(&self) -> &str {
        &self.working_dir
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        // Attempt cleanup on drop
        if self.config.auto_cleanup {
            let working_dir = self.working_dir.clone();
            tokio::spawn(async move {
                let _ = tokio::fs::remove_dir_all(&working_dir).await;
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_sandbox_creation() {
        let sandbox = Sandbox::new(SandboxConfig::default()).await.unwrap();
        assert_eq!(sandbox.state(), SandboxState::Ready);
    }

    #[tokio::test]
    async fn test_sandbox_config_builders() {
        let secure = SandboxConfig::secure();
        assert!(!secure.network_policy.allow_network);

        let permissive = SandboxConfig::permissive();
        assert!(permissive.network_policy.allow_network);
    }
}
