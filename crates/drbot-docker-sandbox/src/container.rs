//! Container lifecycle management.

use crate::{Result, SandboxError};
use bollard::container::{
    Config, CreateContainerOptions, LogsOptions, RemoveContainerOptions, StartContainerOptions,
    StopContainerOptions, WaitContainerOptions,
};
use bollard::exec::{CreateExecOptions, StartExecResults};
use bollard::Docker;
use chrono::{DateTime, Utc};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Resource limits for containers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerLimits {
    /// Memory limit in bytes.
    #[serde(default = "default_memory")]
    pub memory_bytes: u64,
    /// CPU quota (100000 = 1 CPU).
    #[serde(default = "default_cpu")]
    pub cpu_quota: i64,
    /// Maximum number of PIDs.
    #[serde(default = "default_pids")]
    pub pids_limit: i64,
    /// Disk quota in bytes (requires special setup).
    pub disk_bytes: Option<u64>,
    /// Read-only root filesystem.
    #[serde(default)]
    pub readonly_rootfs: bool,
    /// Disable networking.
    #[serde(default)]
    pub no_network: bool,
    /// Drop all capabilities.
    #[serde(default = "default_drop_caps")]
    pub drop_all_caps: bool,
}

fn default_memory() -> u64 {
    256 * 1024 * 1024 // 256 MB
}

fn default_cpu() -> i64 {
    50000 // 0.5 CPU
}

fn default_pids() -> i64 {
    100
}

fn default_drop_caps() -> bool {
    true
}

impl Default for ContainerLimits {
    fn default() -> Self {
        Self {
            memory_bytes: default_memory(),
            cpu_quota: default_cpu(),
            pids_limit: default_pids(),
            disk_bytes: None,
            readonly_rootfs: false,
            no_network: false,
            drop_all_caps: default_drop_caps(),
        }
    }
}

/// Container status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContainerStatus {
    /// Container is being created.
    Creating,
    /// Container is running.
    Running,
    /// Container is paused.
    Paused,
    /// Container has stopped.
    Stopped,
    /// Container is being removed.
    Removing,
    /// Container encountered an error.
    Error,
}

/// Container information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerInfo {
    /// Container ID.
    pub id: String,
    /// Container name.
    pub name: String,
    /// Image used.
    pub image: String,
    /// Current status.
    pub status: ContainerStatus,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Last used time.
    pub last_used: Option<DateTime<Utc>>,
    /// Number of executions.
    pub execution_count: u64,
}

/// A managed sandbox container.
pub struct SandboxContainer {
    /// Docker client.
    docker: Arc<Docker>,
    /// Container ID.
    id: String,
    /// Container name.
    name: String,
    /// Image used.
    image: String,
    /// Current status.
    status: Arc<RwLock<ContainerStatus>>,
    /// Resource limits.
    limits: ContainerLimits,
    /// Creation time.
    created_at: DateTime<Utc>,
    /// Last used time.
    last_used: Arc<RwLock<Option<DateTime<Utc>>>>,
    /// Execution count.
    execution_count: Arc<RwLock<u64>>,
    /// Working directory.
    workdir: String,
}

impl SandboxContainer {
    /// Create a new sandbox container.
    pub async fn create(
        docker: Arc<Docker>,
        image: &str,
        limits: ContainerLimits,
        workdir: &str,
    ) -> Result<Self> {
        let name = format!("drbot-sandbox-{}", Uuid::new_v4());

        let mut host_config = bollard::models::HostConfig {
            memory: Some(limits.memory_bytes as i64),
            cpu_quota: Some(limits.cpu_quota),
            pids_limit: Some(limits.pids_limit),
            readonly_rootfs: Some(limits.readonly_rootfs),
            ..Default::default()
        };

        if limits.no_network {
            host_config.network_mode = Some("none".to_string());
        }

        if limits.drop_all_caps {
            host_config.cap_drop = Some(vec!["ALL".to_string()]);
        }

        // Security options
        host_config.security_opt = Some(vec!["no-new-privileges:true".to_string()]);

        let config = Config {
            image: Some(image.to_string()),
            hostname: Some("sandbox".to_string()),
            working_dir: Some(workdir.to_string()),
            host_config: Some(host_config),
            tty: Some(false),
            attach_stdin: Some(false),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            // Keep container running with a simple command
            cmd: Some(vec!["sleep".to_string(), "infinity".to_string()]),
            ..Default::default()
        };

        let options = CreateContainerOptions {
            name: name.clone(),
            platform: None,
        };

        let response = docker
            .create_container(Some(options), config)
            .await
            .map_err(|e| SandboxError::ContainerCreationFailed(e.to_string()))?;

        let container = Self {
            docker,
            id: response.id,
            name,
            image: image.to_string(),
            status: Arc::new(RwLock::new(ContainerStatus::Creating)),
            limits,
            created_at: Utc::now(),
            last_used: Arc::new(RwLock::new(None)),
            execution_count: Arc::new(RwLock::new(0)),
            workdir: workdir.to_string(),
        };

        Ok(container)
    }

    /// Start the container.
    pub async fn start(&self) -> Result<()> {
        self.docker
            .start_container(&self.id, None::<StartContainerOptions<String>>)
            .await
            .map_err(|e| SandboxError::DockerError(e.to_string()))?;

        let mut status = self.status.write().await;
        *status = ContainerStatus::Running;

        Ok(())
    }

    /// Stop the container.
    pub async fn stop(&self) -> Result<()> {
        let options = StopContainerOptions { t: 5 };

        self.docker
            .stop_container(&self.id, Some(options))
            .await
            .map_err(|e| SandboxError::DockerError(e.to_string()))?;

        let mut status = self.status.write().await;
        *status = ContainerStatus::Stopped;

        Ok(())
    }

    /// Remove the container.
    pub async fn remove(&self) -> Result<()> {
        {
            let mut status = self.status.write().await;
            *status = ContainerStatus::Removing;
        }

        let options = RemoveContainerOptions {
            force: true,
            ..Default::default()
        };

        self.docker
            .remove_container(&self.id, Some(options))
            .await
            .map_err(|e| SandboxError::DockerError(e.to_string()))?;

        Ok(())
    }

    /// Execute a command in the container.
    pub async fn execute(
        &self,
        command: Vec<String>,
        timeout_secs: u64,
    ) -> Result<ExecutionOutput> {
        // Update last used time
        {
            let mut last_used = self.last_used.write().await;
            *last_used = Some(Utc::now());
        }

        // Increment execution count
        {
            let mut count = self.execution_count.write().await;
            *count += 1;
        }

        let exec_options = CreateExecOptions {
            cmd: Some(command),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            working_dir: Some(self.workdir.clone()),
            ..Default::default()
        };

        let exec = self
            .docker
            .create_exec(&self.id, exec_options)
            .await
            .map_err(|e| SandboxError::ExecutionFailed(e.to_string()))?;

        let start_result = self
            .docker
            .start_exec(&exec.id, None)
            .await
            .map_err(|e| SandboxError::ExecutionFailed(e.to_string()))?;

        let mut stdout = String::new();
        let mut stderr = String::new();

        if let StartExecResults::Attached { mut output, .. } = start_result {
            let timeout = tokio::time::Duration::from_secs(timeout_secs);
            let deadline = tokio::time::Instant::now() + timeout;

            loop {
                let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
                if remaining.is_zero() {
                    return Err(SandboxError::Timeout);
                }

                match tokio::time::timeout(remaining, output.next()).await {
                    Ok(Some(Ok(chunk))) => {
                        let data = chunk.into_bytes();
                        let text = String::from_utf8_lossy(&data);
                        // Note: bollard combines stdout/stderr with prefixes
                        stdout.push_str(&text);
                    }
                    Ok(Some(Err(e))) => {
                        return Err(SandboxError::ExecutionFailed(e.to_string()));
                    }
                    Ok(None) => break,
                    Err(_) => return Err(SandboxError::Timeout),
                }
            }
        }

        // Get exit code
        let inspect = self
            .docker
            .inspect_exec(&exec.id)
            .await
            .map_err(|e| SandboxError::ExecutionFailed(e.to_string()))?;

        let exit_code = inspect.exit_code.unwrap_or(-1);

        Ok(ExecutionOutput {
            stdout,
            stderr,
            exit_code,
        })
    }

    /// Get container ID.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Get container name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get container status.
    pub async fn status(&self) -> ContainerStatus {
        *self.status.read().await
    }

    /// Get container info.
    pub async fn info(&self) -> ContainerInfo {
        ContainerInfo {
            id: self.id.clone(),
            name: self.name.clone(),
            image: self.image.clone(),
            status: *self.status.read().await,
            created_at: self.created_at,
            last_used: *self.last_used.read().await,
            execution_count: *self.execution_count.read().await,
        }
    }

    /// Check if container is healthy.
    pub async fn is_healthy(&self) -> bool {
        let status = *self.status.read().await;
        status == ContainerStatus::Running
    }

    /// Reset the container state (for reuse from pool).
    pub async fn reset(&self) -> Result<()> {
        // Execute cleanup command
        let _ = self
            .execute(
                vec![
                    "sh".to_string(),
                    "-c".to_string(),
                    format!("rm -rf {}/*", self.workdir),
                ],
                5,
            )
            .await;

        Ok(())
    }
}

impl Drop for SandboxContainer {
    fn drop(&mut self) {
        // Note: Actual cleanup should be done explicitly via remove()
        // This is just for logging
        tracing::debug!(container_id = %self.id, "Sandbox container dropped");
    }
}

/// Output from container execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionOutput {
    /// Standard output.
    pub stdout: String,
    /// Standard error.
    pub stderr: String,
    /// Exit code.
    pub exit_code: i64,
}

impl ExecutionOutput {
    /// Check if execution was successful.
    pub fn is_success(&self) -> bool {
        self.exit_code == 0
    }

    /// Get combined output.
    pub fn combined(&self) -> String {
        if self.stderr.is_empty() {
            self.stdout.clone()
        } else if self.stdout.is_empty() {
            self.stderr.clone()
        } else {
            format!("{}\n{}", self.stdout, self.stderr)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_container_limits_default() {
        let limits = ContainerLimits::default();
        assert_eq!(limits.memory_bytes, 256 * 1024 * 1024);
        assert_eq!(limits.cpu_quota, 50000);
        assert!(limits.drop_all_caps);
    }

    #[test]
    fn test_execution_output() {
        let output = ExecutionOutput {
            stdout: "Hello".to_string(),
            stderr: String::new(),
            exit_code: 0,
        };
        assert!(output.is_success());
        assert_eq!(output.combined(), "Hello");
    }
}
