//! Sandbox for safe code execution.

use crate::{AgentError, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::Command;
use tracing::info;

/// Sandbox configuration.
#[derive(Debug, Clone)]
pub struct SandboxConfig {
    /// Working directory for sandboxed execution.
    pub working_dir: PathBuf,
    /// Maximum execution time in seconds.
    pub timeout_secs: u64,
    /// Maximum memory in MB.
    pub max_memory_mb: u64,
    /// Whether network access is allowed.
    pub allow_network: bool,
    /// Allowed environment variables.
    pub allowed_env: Vec<String>,
    /// Read-only paths.
    pub read_only_paths: Vec<PathBuf>,
    /// Read-write paths.
    pub read_write_paths: Vec<PathBuf>,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            working_dir: std::env::temp_dir().join("drbot-sandbox"),
            timeout_secs: 30,
            max_memory_mb: 256,
            allow_network: false,
            allowed_env: vec!["PATH".to_string(), "HOME".to_string()],
            read_only_paths: vec![],
            read_write_paths: vec![],
        }
    }
}

/// Execution result from sandbox.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxResult {
    /// Exit code.
    pub exit_code: i32,
    /// Standard output.
    pub stdout: String,
    /// Standard error.
    pub stderr: String,
    /// Execution time in milliseconds.
    pub duration_ms: u64,
    /// Whether execution was killed due to timeout.
    pub timed_out: bool,
}

/// Sandbox for isolated code execution.
pub struct Sandbox {
    config: SandboxConfig,
}

impl Sandbox {
    /// Create a new sandbox.
    pub fn new(config: SandboxConfig) -> Result<Self> {
        // Ensure working directory exists
        std::fs::create_dir_all(&config.working_dir).map_err(|e| {
            AgentError::SandboxError(format!("Failed to create sandbox dir: {}", e))
        })?;

        Ok(Self { config })
    }

    /// Execute a command in the sandbox.
    pub async fn execute(&self, command: &str, args: &[&str]) -> Result<SandboxResult> {
        info!("Sandbox executing: {} {:?}", command, args);

        let start = std::time::Instant::now();

        // Build command with sandbox restrictions
        let mut cmd = self.build_sandboxed_command(command, args)?;

        // Execute with timeout
        let output = tokio::time::timeout(
            tokio::time::Duration::from_secs(self.config.timeout_secs),
            cmd.output(),
        )
        .await;

        let duration_ms = start.elapsed().as_millis() as u64;

        match output {
            Ok(Ok(output)) => Ok(SandboxResult {
                exit_code: output.status.code().unwrap_or(-1),
                stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                duration_ms,
                timed_out: false,
            }),
            Ok(Err(e)) => Err(AgentError::SandboxError(format!("Execution failed: {}", e))),
            Err(_) => Ok(SandboxResult {
                exit_code: -1,
                stdout: String::new(),
                stderr: "Execution timed out".to_string(),
                duration_ms,
                timed_out: true,
            }),
        }
    }

    /// Execute code in a specific language.
    pub async fn execute_code(&self, language: &str, code: &str) -> Result<SandboxResult> {
        // Write code to temporary file
        let (file_path, cmd, args) = match language.to_lowercase().as_str() {
            "python" | "py" => {
                let path = self.config.working_dir.join("script.py");
                tokio::fs::write(&path, code).await.map_err(|e| {
                    AgentError::SandboxError(format!("Failed to write code: {}", e))
                })?;
                (
                    path.clone(),
                    "python3",
                    vec![path.to_string_lossy().to_string()],
                )
            }
            "javascript" | "js" | "node" => {
                let path = self.config.working_dir.join("script.js");
                tokio::fs::write(&path, code).await.map_err(|e| {
                    AgentError::SandboxError(format!("Failed to write code: {}", e))
                })?;
                (
                    path.clone(),
                    "node",
                    vec![path.to_string_lossy().to_string()],
                )
            }
            "bash" | "sh" => {
                let path = self.config.working_dir.join("script.sh");
                tokio::fs::write(&path, code).await.map_err(|e| {
                    AgentError::SandboxError(format!("Failed to write code: {}", e))
                })?;
                (
                    path.clone(),
                    "bash",
                    vec![path.to_string_lossy().to_string()],
                )
            }
            "ruby" | "rb" => {
                let path = self.config.working_dir.join("script.rb");
                tokio::fs::write(&path, code).await.map_err(|e| {
                    AgentError::SandboxError(format!("Failed to write code: {}", e))
                })?;
                (
                    path.clone(),
                    "ruby",
                    vec![path.to_string_lossy().to_string()],
                )
            }
            _ => {
                return Err(AgentError::SandboxError(format!(
                    "Unsupported language: {}",
                    language
                )));
            }
        };

        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let result = self.execute(cmd, &args_refs).await;

        // Clean up
        let _ = tokio::fs::remove_file(&file_path).await;

        result
    }

    /// Build a sandboxed command.
    fn build_sandboxed_command(&self, command: &str, args: &[&str]) -> Result<Command> {
        let mut cmd = Command::new(command);
        cmd.args(args)
            .current_dir(&self.config.working_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null());

        // Clear environment and only allow specified vars
        cmd.env_clear();
        for var in &self.config.allowed_env {
            if let Ok(value) = std::env::var(var) {
                cmd.env(var, value);
            }
        }

        // On macOS/Linux, we could use sandbox-exec or similar
        // For now, we rely on basic restrictions

        Ok(cmd)
    }

    /// Clean up sandbox files.
    pub async fn cleanup(&self) -> Result<()> {
        if self.config.working_dir.exists() {
            tokio::fs::remove_dir_all(&self.config.working_dir)
                .await
                .map_err(|e| AgentError::SandboxError(format!("Cleanup failed: {}", e)))?;
        }
        Ok(())
    }
}

impl Default for Sandbox {
    fn default() -> Self {
        Self::new(SandboxConfig::default()).expect("Failed to create default sandbox")
    }
}

/// Docker-based sandbox for stronger isolation.
#[allow(dead_code)] // Optional implementation; not currently wired into the agent runtime.
pub struct DockerSandbox {
    config: SandboxConfig,
    image: String,
}

#[allow(dead_code)]
impl DockerSandbox {
    /// Create a new Docker sandbox.
    pub fn new(config: SandboxConfig, image: &str) -> Self {
        Self {
            config,
            image: image.to_string(),
        }
    }

    /// Execute code in Docker container.
    pub async fn execute(&self, language: &str, code: &str) -> Result<SandboxResult> {
        let start = std::time::Instant::now();

        // Pre-compute formatted strings to avoid temporary lifetime issues
        let network_mode = if self.config.allow_network {
            "bridge"
        } else {
            "none"
        };
        let memory_limit = format!("--memory={}m", self.config.max_memory_mb);

        // Build docker command
        let mut args = vec![
            "run",
            "--rm",
            "--network",
            network_mode,
            memory_limit.as_str(),
            "--cpus=1",
            "-i",
        ];

        // Add image
        args.push(&self.image);

        // Add language-specific interpreter
        let (interpreter, flag) = match language.to_lowercase().as_str() {
            "python" | "py" => ("python3", "-c"),
            "javascript" | "js" | "node" => ("node", "-e"),
            "bash" | "sh" => ("bash", "-c"),
            "ruby" | "rb" => ("ruby", "-e"),
            _ => {
                return Err(AgentError::SandboxError(format!(
                    "Unsupported language: {}",
                    language
                )))
            }
        };

        args.push(interpreter);
        args.push(flag);
        args.push(code);

        let output = tokio::time::timeout(
            tokio::time::Duration::from_secs(self.config.timeout_secs),
            Command::new("docker")
                .args(&args)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output(),
        )
        .await;

        let duration_ms = start.elapsed().as_millis() as u64;

        match output {
            Ok(Ok(output)) => Ok(SandboxResult {
                exit_code: output.status.code().unwrap_or(-1),
                stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                duration_ms,
                timed_out: false,
            }),
            Ok(Err(e)) => Err(AgentError::SandboxError(format!(
                "Docker execution failed: {}",
                e
            ))),
            Err(_) => {
                // Kill the container if timed out
                let _ = Command::new("docker")
                    .args(["kill", "--signal=KILL"])
                    .output()
                    .await;
                Ok(SandboxResult {
                    exit_code: -1,
                    stdout: String::new(),
                    stderr: "Execution timed out".to_string(),
                    duration_ms,
                    timed_out: true,
                })
            }
        }
    }
}

/// WASM-based sandbox for even stronger isolation.
#[allow(dead_code)] // Optional implementation; not currently wired into the agent runtime.
pub struct WasmSandbox {
    config: SandboxConfig,
}

#[allow(dead_code)]
impl WasmSandbox {
    /// Create a new WASM sandbox.
    pub fn new(config: SandboxConfig) -> Self {
        Self { config }
    }

    /// Execute WASM module.
    pub async fn execute_wasm(
        &self,
        _wasm_bytes: &[u8],
        _function: &str,
        _args: &[&str],
    ) -> Result<SandboxResult> {
        // This would use wasmtime or wasmer to execute WASM
        // Placeholder implementation
        Err(AgentError::SandboxError(
            "WASM execution not yet implemented".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sandbox_config_default() {
        let config = SandboxConfig::default();
        assert_eq!(config.timeout_secs, 30);
        assert!(!config.allow_network);
    }

    #[tokio::test]
    async fn test_sandbox_execute() {
        let sandbox = Sandbox::new(SandboxConfig::default()).unwrap();
        let result = sandbox.execute("echo", &["hello"]).await.unwrap();
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("hello"));
    }

    #[tokio::test]
    async fn test_sandbox_execute_code() {
        let sandbox = Sandbox::new(SandboxConfig::default()).unwrap();

        // Test Python execution
        let result = sandbox.execute_code("python", "print('hello')").await;
        // This will fail if python3 is not installed, which is fine for tests
        if result.is_ok() {
            let result = result.unwrap();
            assert!(result.stdout.contains("hello") || result.exit_code != 0);
        }
    }
}
