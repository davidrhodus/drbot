//! Main sandbox manager orchestrating all sandbox operations.

use crate::{
    ContainerLimits, ContainerPool, ImageManager, LanguageConfig, NetworkMode, PoolConfig, Result,
    SandboxConfig, SandboxError, SessionManager, VolumeMount,
};
use bollard::Docker;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{info, warn};

/// Request to execute code in a sandbox.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteRequest {
    /// Language to execute.
    pub language: String,
    /// Code to execute.
    pub code: String,
    /// Custom image (overrides language default).
    pub image: Option<String>,
    /// Custom timeout (seconds).
    pub timeout_secs: Option<u64>,
    /// Custom resource limits.
    pub limits: Option<ContainerLimits>,
    /// Environment variables.
    #[serde(default)]
    pub env: Vec<(String, String)>,
    /// Volume mounts.
    #[serde(default)]
    pub volumes: Vec<VolumeMount>,
    /// Working directory.
    pub workdir: Option<String>,
}

impl ExecuteRequest {
    /// Create a new execute request.
    pub fn new(language: &str, code: &str) -> Self {
        Self {
            language: language.to_string(),
            code: code.to_string(),
            image: None,
            timeout_secs: None,
            limits: None,
            env: Vec::new(),
            volumes: Vec::new(),
            workdir: None,
        }
    }

    /// Set custom image.
    pub fn with_image(mut self, image: &str) -> Self {
        self.image = Some(image.to_string());
        self
    }

    /// Set custom timeout.
    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = Some(secs);
        self
    }

    /// Set custom limits.
    pub fn with_limits(mut self, limits: ContainerLimits) -> Self {
        self.limits = Some(limits);
        self
    }

    /// Add environment variable.
    pub fn with_env(mut self, key: &str, value: &str) -> Self {
        self.env.push((key.to_string(), value.to_string()));
        self
    }

    /// Add volume mount.
    pub fn with_volume(mut self, mount: VolumeMount) -> Self {
        self.volumes.push(mount);
        self
    }
}

/// Result of code execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteResult {
    /// Standard output.
    pub stdout: String,
    /// Standard error.
    pub stderr: String,
    /// Exit code.
    pub exit_code: i64,
    /// Execution time in milliseconds.
    pub execution_time_ms: u64,
    /// Whether execution was successful.
    pub success: bool,
    /// Error message (if failed before execution).
    pub error: Option<String>,
    /// Resource usage information.
    pub resource_usage: Option<ResourceUsage>,
}

impl ExecuteResult {
    /// Create a successful result.
    pub fn success(stdout: String, stderr: String, exit_code: i64, execution_time_ms: u64) -> Self {
        Self {
            stdout,
            stderr,
            exit_code,
            execution_time_ms,
            success: exit_code == 0,
            error: None,
            resource_usage: None,
        }
    }

    /// Create an error result.
    pub fn error(message: &str) -> Self {
        Self {
            stdout: String::new(),
            stderr: String::new(),
            exit_code: -1,
            execution_time_ms: 0,
            success: false,
            error: Some(message.to_string()),
            resource_usage: None,
        }
    }

    /// Get combined output.
    pub fn combined_output(&self) -> String {
        if self.stderr.is_empty() {
            self.stdout.clone()
        } else if self.stdout.is_empty() {
            self.stderr.clone()
        } else {
            format!("{}\n{}", self.stdout, self.stderr)
        }
    }
}

/// Resource usage information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceUsage {
    /// Memory used in bytes.
    pub memory_bytes: Option<u64>,
    /// CPU time in milliseconds.
    pub cpu_time_ms: Option<u64>,
}

/// Main sandbox manager.
pub struct SandboxManager {
    /// Configuration.
    config: SandboxConfig,
    /// Docker client.
    docker: Arc<Docker>,
    /// Image manager.
    image_manager: Arc<ImageManager>,
    /// Container pool.
    pool: Arc<ContainerPool>,
    /// Session manager.
    session_manager: Arc<SessionManager>,
}

impl SandboxManager {
    /// Create a new sandbox manager.
    pub async fn new(config: SandboxConfig) -> Result<Self> {
        // Connect to Docker
        let docker = Docker::connect_with_socket_defaults()
            .map_err(|e| SandboxError::ConnectionFailed(e.to_string()))?;

        let docker = Arc::new(docker);

        // Create image manager
        let image_manager = Arc::new(ImageManager::new(docker.clone()));

        // Create container pool
        let pool = Arc::new(ContainerPool::new(
            docker.clone(),
            config.pool_config.clone(),
            &config.default_image,
            config.default_limits.clone(),
            &config.workdir,
        ));

        // Create session manager
        let session_manager = Arc::new(SessionManager::new(
            pool.clone(),
            config.default_timeout_secs * 10, // Session timeout = 10x execution timeout
        ));

        let manager = Self {
            config,
            docker,
            image_manager,
            pool,
            session_manager,
        };

        // Warm up pool if enabled
        if manager.config.enable_pooling {
            if let Err(e) = manager.pool.warm_up().await {
                warn!(error = %e, "Failed to warm up container pool");
            }
        }

        info!("Sandbox manager initialized");
        Ok(manager)
    }

    /// Execute code for a session.
    pub async fn execute(
        &self,
        session_id: &str,
        request: ExecuteRequest,
    ) -> Result<ExecuteResult> {
        let start_time = std::time::Instant::now();

        // Get language configuration
        let lang_config = LanguageConfig::by_name(&request.language).ok_or_else(|| {
            SandboxError::InvalidConfig(format!("Unknown language: {}", request.language))
        })?;

        // Determine image to use
        let image = request.image.as_ref().unwrap_or(&lang_config.image);

        // Ensure image is available
        self.image_manager.ensure_available(image).await?;

        // Get or create container for session
        let container = self
            .session_manager
            .get_or_create(session_id, Some(image))
            .await?;

        // Mark session as executing
        self.session_manager.set_executing(session_id, true).await;

        // Build command
        let mut command = lang_config.run_command.clone();
        command.push(request.code.clone());

        // Execute with timeout
        let timeout = request
            .timeout_secs
            .unwrap_or(self.config.default_timeout_secs);

        let result = container.execute(command, timeout).await;

        // Mark session as not executing
        self.session_manager.set_executing(session_id, false).await;

        let execution_time_ms = start_time.elapsed().as_millis() as u64;

        match result {
            Ok(output) => {
                if self.config.audit_logging {
                    info!(
                        session_id = %session_id,
                        language = %request.language,
                        exit_code = output.exit_code,
                        execution_time_ms = execution_time_ms,
                        "Code execution completed"
                    );
                }

                Ok(ExecuteResult::success(
                    output.stdout,
                    output.stderr,
                    output.exit_code,
                    execution_time_ms,
                ))
            }
            Err(e) => {
                if self.config.audit_logging {
                    warn!(
                        session_id = %session_id,
                        language = %request.language,
                        error = %e,
                        "Code execution failed"
                    );
                }

                Ok(ExecuteResult::error(&e.to_string()))
            }
        }
    }

    /// Execute code without a session (one-shot).
    pub async fn execute_oneshot(&self, request: ExecuteRequest) -> Result<ExecuteResult> {
        let session_id = format!("oneshot-{}", uuid::Uuid::new_v4());
        let result = self.execute(&session_id, request).await;

        // Clean up the temporary session
        let _ = self.session_manager.end_session(&session_id).await;

        result
    }

    /// Get session status.
    pub async fn session_status(&self, session_id: &str) -> Option<crate::session::SessionSandbox> {
        self.session_manager.get_session_info(session_id).await
    }

    /// List active sessions.
    pub async fn list_sessions(&self) -> Vec<crate::session::SessionSandbox> {
        self.session_manager.list_sessions().await
    }

    /// Reset a session's container.
    pub async fn reset_session(&self, session_id: &str) -> Result<()> {
        self.session_manager.reset_session(session_id).await
    }

    /// End a session.
    pub async fn end_session(&self, session_id: &str) -> Result<()> {
        self.session_manager.end_session(session_id).await
    }

    /// Get pool status.
    pub async fn pool_status(&self) -> crate::pool::PoolStatus {
        self.pool.status().await
    }

    /// Run cleanup tasks.
    pub async fn cleanup(&self) -> (usize, usize, usize) {
        let sessions = self.session_manager.cleanup_timeouts().await;
        let idle = self.pool.cleanup_idle().await;
        let overused = self.pool.cleanup_overused().await;

        (sessions, idle, overused)
    }

    /// Shutdown the sandbox manager.
    pub async fn shutdown(&self) -> Result<()> {
        info!("Shutting down sandbox manager");

        self.session_manager.shutdown().await?;
        self.pool.shutdown().await?;

        info!("Sandbox manager shutdown complete");
        Ok(())
    }

    /// Check if Docker is available.
    pub async fn is_docker_available(&self) -> bool {
        self.docker.ping().await.is_ok()
    }

    /// Get supported languages.
    pub fn supported_languages(&self) -> Vec<&'static str> {
        vec!["python", "javascript", "rust", "bash"]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execute_request() {
        let request = ExecuteRequest::new("python", "print('hello')")
            .with_timeout(10)
            .with_env("MY_VAR", "value");

        assert_eq!(request.language, "python");
        assert_eq!(request.code, "print('hello')");
        assert_eq!(request.timeout_secs, Some(10));
        assert_eq!(request.env.len(), 1);
    }

    #[test]
    fn test_execute_result() {
        let result = ExecuteResult::success("output".into(), String::new(), 0, 100);
        assert!(result.success);
        assert_eq!(result.combined_output(), "output");

        let error = ExecuteResult::error("failed");
        assert!(!error.success);
        assert_eq!(error.error, Some("failed".to_string()));
    }
}
