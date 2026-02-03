//! Process spawning and management for drbot.
//!
//! This crate provides:
//! - Async process execution
//! - Process monitoring
//! - Output capture and streaming

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;
use tracing::{debug, error, trace};

/// Process error types.
#[derive(Error, Debug)]
pub enum ProcessError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Process failed with exit code: {0}")]
    ExitCode(i32),

    #[error("Process killed by signal")]
    Signal,

    #[error("Process timed out after {0:?}")]
    Timeout(Duration),

    #[error("Process error: {0}")]
    Other(String),
}

/// Result type for process operations.
pub type Result<T> = std::result::Result<T, ProcessError>;

/// Process output.
#[derive(Debug, Clone, Default)]
pub struct Output {
    /// Standard output.
    pub stdout: String,
    /// Standard error.
    pub stderr: String,
    /// Exit code.
    pub exit_code: Option<i32>,
    /// Whether process was killed.
    pub killed: bool,
    /// Duration.
    pub duration: Duration,
}

impl Output {
    /// Check if process succeeded.
    pub fn success(&self) -> bool {
        self.exit_code == Some(0)
    }

    /// Get combined output.
    pub fn combined(&self) -> String {
        format!("{}{}", self.stdout, self.stderr)
    }

    /// Get stdout lines.
    pub fn stdout_lines(&self) -> Vec<&str> {
        self.stdout.lines().collect()
    }

    /// Get stderr lines.
    pub fn stderr_lines(&self) -> Vec<&str> {
        self.stderr.lines().collect()
    }
}

/// Process builder.
pub struct ProcessBuilder {
    program: String,
    args: Vec<String>,
    env: HashMap<String, String>,
    env_clear: bool,
    current_dir: Option<PathBuf>,
    timeout: Option<Duration>,
    stdin_data: Option<Vec<u8>>,
}

impl ProcessBuilder {
    /// Create new process builder.
    pub fn new(program: &str) -> Self {
        Self {
            program: program.to_string(),
            args: Vec::new(),
            env: HashMap::new(),
            env_clear: false,
            current_dir: None,
            timeout: None,
            stdin_data: None,
        }
    }

    /// Add argument.
    pub fn arg(mut self, arg: &str) -> Self {
        self.args.push(arg.to_string());
        self
    }

    /// Add multiple arguments.
    pub fn args(mut self, args: &[&str]) -> Self {
        self.args.extend(args.iter().map(|s| s.to_string()));
        self
    }

    /// Set environment variable.
    pub fn env(mut self, key: &str, value: &str) -> Self {
        self.env.insert(key.to_string(), value.to_string());
        self
    }

    /// Set multiple environment variables.
    pub fn envs(mut self, vars: &[(&str, &str)]) -> Self {
        for (key, value) in vars {
            self.env.insert(key.to_string(), value.to_string());
        }
        self
    }

    /// Clear all environment variables.
    pub fn env_clear(mut self) -> Self {
        self.env_clear = true;
        self
    }

    /// Set current directory.
    pub fn current_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.current_dir = Some(dir.into());
        self
    }

    /// Set timeout.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Set stdin data.
    pub fn stdin(mut self, data: impl Into<Vec<u8>>) -> Self {
        self.stdin_data = Some(data.into());
        self
    }

    /// Set stdin from string.
    pub fn stdin_str(mut self, data: &str) -> Self {
        self.stdin_data = Some(data.as_bytes().to_vec());
        self
    }

    fn build_command(&self) -> Command {
        let mut cmd = Command::new(&self.program);
        cmd.args(&self.args);

        if self.env_clear {
            cmd.env_clear();
        }
        cmd.envs(&self.env);

        if let Some(dir) = &self.current_dir {
            cmd.current_dir(dir);
        }

        cmd
    }

    /// Run and wait for completion.
    pub async fn run(&self) -> Result<Output> {
        debug!("Running: {} {:?}", self.program, self.args);
        let start = std::time::Instant::now();

        let mut cmd = self.build_command();
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        if self.stdin_data.is_some() {
            cmd.stdin(Stdio::piped());
        }

        let mut child = cmd.spawn()?;

        // Write stdin if provided
        if let Some(data) = &self.stdin_data {
            if let Some(mut stdin) = child.stdin.take() {
                use tokio::io::AsyncWriteExt;
                stdin.write_all(data).await?;
            }
        }

        // Handle timeout
        let output = if let Some(timeout) = self.timeout {
            match tokio::time::timeout(timeout, child.wait_with_output()).await {
                Ok(result) => result?,
                Err(_) => {
                    return Err(ProcessError::Timeout(timeout));
                }
            }
        } else {
            child.wait_with_output().await?
        };

        let duration = start.elapsed();

        let result = Output {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            exit_code: output.status.code(),
            killed: false,
            duration,
        };

        trace!("Process completed: {:?}", result);
        Ok(result)
    }

    /// Run and check for success.
    pub async fn run_checked(&self) -> Result<Output> {
        let output = self.run().await?;
        if !output.success() {
            return Err(ProcessError::ExitCode(output.exit_code.unwrap_or(-1)));
        }
        Ok(output)
    }

    /// Spawn process without waiting.
    pub async fn spawn(&self) -> Result<ManagedProcess> {
        debug!("Spawning: {} {:?}", self.program, self.args);

        let mut cmd = self.build_command();
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        cmd.stdin(Stdio::piped());

        let child = cmd.spawn()?;

        Ok(ManagedProcess {
            child,
            timeout: self.timeout,
        })
    }

    /// Run with streaming output.
    pub async fn run_streaming(&self) -> Result<(ManagedProcess, mpsc::Receiver<OutputLine>)> {
        let mut cmd = self.build_command();
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let mut child = cmd.spawn()?;

        let (tx, rx) = mpsc::channel(100);

        // Stream stdout
        if let Some(stdout) = child.stdout.take() {
            let tx = tx.clone();
            tokio::spawn(async move {
                let reader = BufReader::new(stdout);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    if tx.send(OutputLine::Stdout(line)).await.is_err() {
                        break;
                    }
                }
            });
        }

        // Stream stderr
        if let Some(stderr) = child.stderr.take() {
            tokio::spawn(async move {
                let reader = BufReader::new(stderr);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    if tx.send(OutputLine::Stderr(line)).await.is_err() {
                        break;
                    }
                }
            });
        }

        Ok((
            ManagedProcess {
                child,
                timeout: self.timeout,
            },
            rx,
        ))
    }
}

/// Output line type.
#[derive(Debug, Clone)]
pub enum OutputLine {
    Stdout(String),
    Stderr(String),
}

impl OutputLine {
    /// Get the line content.
    pub fn content(&self) -> &str {
        match self {
            OutputLine::Stdout(s) | OutputLine::Stderr(s) => s,
        }
    }

    /// Check if stdout.
    pub fn is_stdout(&self) -> bool {
        matches!(self, OutputLine::Stdout(_))
    }

    /// Check if stderr.
    pub fn is_stderr(&self) -> bool {
        matches!(self, OutputLine::Stderr(_))
    }
}

/// Managed child process.
pub struct ManagedProcess {
    child: Child,
    timeout: Option<Duration>,
}

impl ManagedProcess {
    /// Get process ID.
    pub fn id(&self) -> Option<u32> {
        self.child.id()
    }

    /// Check if process is still running.
    pub async fn is_running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    /// Wait for completion.
    pub async fn wait(&mut self) -> Result<Output> {
        let start = std::time::Instant::now();

        let status = if let Some(timeout) = self.timeout {
            match tokio::time::timeout(timeout, self.child.wait()).await {
                Ok(result) => result?,
                Err(_) => return Err(ProcessError::Timeout(timeout)),
            }
        } else {
            self.child.wait().await?
        };

        let duration = start.elapsed();

        Ok(Output {
            stdout: String::new(),
            stderr: String::new(),
            exit_code: status.code(),
            killed: false,
            duration,
        })
    }

    /// Kill the process.
    pub async fn kill(&mut self) -> Result<()> {
        self.child.kill().await?;
        Ok(())
    }

    /// Get stdin handle.
    pub fn stdin(&mut self) -> Option<&mut tokio::process::ChildStdin> {
        self.child.stdin.as_mut()
    }
}

/// Quick command execution.
pub async fn exec(program: &str, args: &[&str]) -> Result<Output> {
    ProcessBuilder::new(program).args(args).run().await
}

/// Quick command execution with success check.
pub async fn exec_checked(program: &str, args: &[&str]) -> Result<Output> {
    ProcessBuilder::new(program).args(args).run_checked().await
}

/// Run shell command.
pub async fn shell(cmd: &str) -> Result<Output> {
    ProcessBuilder::new("sh").args(&["-c", cmd]).run().await
}

/// Run shell command with success check.
pub async fn shell_checked(cmd: &str) -> Result<Output> {
    ProcessBuilder::new("sh")
        .args(&["-c", cmd])
        .run_checked()
        .await
}

/// Check if command exists.
pub async fn command_exists(cmd: &str) -> bool {
    ProcessBuilder::new("which")
        .arg(cmd)
        .run()
        .await
        .map(|o| o.success())
        .unwrap_or(false)
}

/// Get command output as string (trimmed).
pub async fn output(program: &str, args: &[&str]) -> Result<String> {
    let output = ProcessBuilder::new(program)
        .args(args)
        .run_checked()
        .await?;
    Ok(output.stdout.trim().to_string())
}

/// Process group for managing multiple processes.
pub struct ProcessGroup {
    processes: Vec<ManagedProcess>,
}

impl ProcessGroup {
    /// Create new process group.
    pub fn new() -> Self {
        Self {
            processes: Vec::new(),
        }
    }

    /// Add process.
    pub fn add(&mut self, process: ManagedProcess) {
        self.processes.push(process);
    }

    /// Get number of processes.
    pub fn len(&self) -> usize {
        self.processes.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.processes.is_empty()
    }

    /// Kill all processes.
    pub async fn kill_all(&mut self) {
        for process in &mut self.processes {
            if let Err(e) = process.kill().await {
                error!("Failed to kill process: {}", e);
            }
        }
    }

    /// Wait for all processes.
    pub async fn wait_all(&mut self) -> Vec<Result<Output>> {
        let mut results = Vec::new();
        for process in &mut self.processes {
            results.push(process.wait().await);
        }
        results
    }
}

impl Default for ProcessGroup {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_exec() {
        let output = exec("echo", &["hello"]).await.unwrap();
        assert!(output.success());
        assert_eq!(output.stdout.trim(), "hello");
    }

    #[tokio::test]
    async fn test_shell() {
        let output = shell("echo 'hello world'").await.unwrap();
        assert!(output.success());
        assert_eq!(output.stdout.trim(), "hello world");
    }

    #[tokio::test]
    async fn test_process_builder() {
        let output = ProcessBuilder::new("echo")
            .args(&["hello", "world"])
            .run()
            .await
            .unwrap();

        assert!(output.success());
        assert_eq!(output.stdout.trim(), "hello world");
    }

    #[tokio::test]
    async fn test_env() {
        let output = ProcessBuilder::new("sh")
            .args(&["-c", "echo $MY_VAR"])
            .env("MY_VAR", "test_value")
            .run()
            .await
            .unwrap();

        assert_eq!(output.stdout.trim(), "test_value");
    }

    #[tokio::test]
    async fn test_current_dir() {
        let output = ProcessBuilder::new("pwd")
            .current_dir("/tmp")
            .run()
            .await
            .unwrap();

        assert!(output.stdout.contains("tmp"));
    }

    #[tokio::test]
    async fn test_stdin() {
        let output = ProcessBuilder::new("cat")
            .stdin_str("hello from stdin")
            .run()
            .await
            .unwrap();

        assert_eq!(output.stdout, "hello from stdin");
    }

    #[tokio::test]
    async fn test_timeout() {
        let result = ProcessBuilder::new("sleep")
            .arg("10")
            .timeout(Duration::from_millis(100))
            .run()
            .await;

        assert!(matches!(result, Err(ProcessError::Timeout(_))));
    }

    #[tokio::test]
    async fn test_exit_code() {
        let output = exec("sh", &["-c", "exit 42"]).await.unwrap();
        assert_eq!(output.exit_code, Some(42));
        assert!(!output.success());
    }

    #[tokio::test]
    async fn test_command_exists() {
        assert!(command_exists("echo").await);
        assert!(!command_exists("nonexistent_command_12345").await);
    }

    #[tokio::test]
    async fn test_output() {
        let result = output("echo", &["hello"]).await.unwrap();
        assert_eq!(result, "hello");
    }

    #[tokio::test]
    async fn test_spawn_and_kill() {
        let mut process = ProcessBuilder::new("sleep")
            .arg("100")
            .spawn()
            .await
            .unwrap();

        assert!(process.is_running().await);
        process.kill().await.unwrap();
    }
}
