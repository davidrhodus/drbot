//! Code execution handling.

use crate::limits::ResourceLimits;
use crate::runtime::{Language, Runtime};
use crate::{Result, SandboxError};
use serde::{Deserialize, Serialize};
use std::process::Stdio;
use std::time::Instant;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tracing::{debug, error, info};
use uuid::Uuid;

/// Result of code execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    /// Execution ID.
    pub id: String,
    /// Exit status.
    pub status: ExecutionStatus,
    /// Standard output.
    pub stdout: String,
    /// Standard error.
    pub stderr: String,
    /// Exit code.
    pub exit_code: Option<i32>,
    /// Execution time in milliseconds.
    pub duration_ms: u64,
    /// Memory used in bytes (if available).
    pub memory_bytes: Option<u64>,
}

/// Execution status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutionStatus {
    /// Completed successfully.
    Success,
    /// Completed with error.
    Error,
    /// Timed out.
    Timeout,
    /// Killed due to resource limits.
    Killed,
    /// Execution was cancelled.
    Cancelled,
}

/// A pending code execution.
#[derive(Debug)]
pub struct CodeExecution {
    /// Execution ID.
    pub id: String,
    /// Language being executed.
    pub language: Language,
    /// Source code.
    pub code: String,
    /// Resource limits.
    pub limits: ResourceLimits,
    /// Working directory.
    pub working_dir: String,
}

impl CodeExecution {
    /// Create a new code execution.
    pub fn new(language: Language, code: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            language,
            code: code.into(),
            limits: ResourceLimits::default(),
            working_dir: std::env::temp_dir().to_string_lossy().to_string(),
        }
    }

    /// Set resource limits.
    pub fn with_limits(mut self, limits: ResourceLimits) -> Self {
        self.limits = limits;
        self
    }

    /// Set working directory.
    pub fn with_working_dir(mut self, dir: impl Into<String>) -> Self {
        self.working_dir = dir.into();
        self
    }

    /// Execute the code.
    pub async fn execute(&self, runtime: &Runtime) -> Result<ExecutionResult> {
        let start = Instant::now();

        // Write code to temp file
        let file_path = format!(
            "{}/drbot_exec_{}.{}",
            self.working_dir,
            &self.id[..8],
            self.language.extension()
        );

        tokio::fs::write(&file_path, &self.code)
            .await
            .map_err(|e| SandboxError::ExecutionFailed(format!("Failed to write code: {}", e)))?;

        // Build command
        let cmd_parts = runtime.run_file_command(&file_path);
        let program = &cmd_parts[0];
        let args = &cmd_parts[1..];

        debug!("Executing: {} {:?}", program, args);

        // Spawn process
        let mut child = Command::new(program)
            .args(args)
            .current_dir(&self.working_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| {
                SandboxError::ExecutionFailed(format!("Failed to spawn process: {}", e))
            })?;

        // Set up timeout
        let timeout = std::time::Duration::from_millis(self.limits.timeout_ms);

        // Capture output with timeout
        let result = tokio::time::timeout(timeout, async {
            let stdout = child.stdout.take().unwrap();
            let stderr = child.stderr.take().unwrap();

            let mut stdout_reader = BufReader::new(stdout).lines();
            let mut stderr_reader = BufReader::new(stderr).lines();

            let mut stdout_buf = String::new();
            let mut stderr_buf = String::new();

            loop {
                tokio::select! {
                    line = stdout_reader.next_line() => {
                        match line {
                            Ok(Some(l)) => {
                                if stdout_buf.len() + l.len() < self.limits.max_output_bytes as usize {
                                    stdout_buf.push_str(&l);
                                    stdout_buf.push('\n');
                                }
                            }
                            Ok(None) => break,
                            Err(e) => {
                                error!("Error reading stdout: {}", e);
                                break;
                            }
                        }
                    }
                    line = stderr_reader.next_line() => {
                        match line {
                            Ok(Some(l)) => {
                                if stderr_buf.len() + l.len() < self.limits.max_output_bytes as usize {
                                    stderr_buf.push_str(&l);
                                    stderr_buf.push('\n');
                                }
                            }
                            Ok(None) => {}
                            Err(e) => {
                                error!("Error reading stderr: {}", e);
                            }
                        }
                    }
                }
            }

            let exit_status = child.wait().await;
            (stdout_buf, stderr_buf, exit_status)
        })
        .await;

        // Cleanup temp file
        let _ = tokio::fs::remove_file(&file_path).await;

        let duration_ms = start.elapsed().as_millis() as u64;

        match result {
            Ok((stdout, stderr, exit_status)) => {
                let exit_code = exit_status.ok().and_then(|s| s.code());
                let status = if exit_code == Some(0) {
                    ExecutionStatus::Success
                } else {
                    ExecutionStatus::Error
                };

                info!(
                    "Execution {} completed in {}ms with status {:?}",
                    &self.id[..8],
                    duration_ms,
                    status
                );

                Ok(ExecutionResult {
                    id: self.id.clone(),
                    status,
                    stdout,
                    stderr,
                    exit_code,
                    duration_ms,
                    memory_bytes: None,
                })
            }
            Err(_) => {
                // Timeout - kill the process
                error!(
                    "Execution {} timed out after {}ms",
                    &self.id[..8],
                    self.limits.timeout_ms
                );

                Ok(ExecutionResult {
                    id: self.id.clone(),
                    status: ExecutionStatus::Timeout,
                    stdout: String::new(),
                    stderr: "Execution timed out".to_string(),
                    exit_code: None,
                    duration_ms,
                    memory_bytes: None,
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_code_execution_new() {
        let exec = CodeExecution::new(Language::Python, "print('hello')");
        assert_eq!(exec.language, Language::Python);
        assert_eq!(exec.code, "print('hello')");
    }

    #[test]
    fn test_execution_status() {
        assert_eq!(ExecutionStatus::Success, ExecutionStatus::Success);
        assert_ne!(ExecutionStatus::Success, ExecutionStatus::Error);
    }
}
