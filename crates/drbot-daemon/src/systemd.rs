//! Linux systemd integration.

use crate::{DaemonConfig, DaemonError, DaemonStatus, Result};
use std::path::PathBuf;
use std::process::Command;

/// Systemd manager for Linux.
pub struct SystemdManager {
    /// Service name.
    service_name: String,
    /// Unit file path.
    unit_path: PathBuf,
    /// Use user services (systemd --user).
    user_mode: bool,
}

impl SystemdManager {
    /// Create a new systemd manager.
    pub fn new(service_name: &str) -> Self {
        Self::with_mode(service_name, true)
    }

    /// Create with explicit user mode setting.
    pub fn with_mode(service_name: &str, user_mode: bool) -> Self {
        let unit_path = if user_mode {
            dirs::config_dir()
                .unwrap_or_else(|| PathBuf::from("/tmp"))
                .join("systemd")
                .join("user")
                .join(format!("{}.service", service_name))
        } else {
            PathBuf::from("/etc/systemd/system").join(format!("{}.service", service_name))
        };

        Self {
            service_name: service_name.to_string(),
            unit_path,
            user_mode,
        }
    }

    /// Generate unit file content.
    fn generate_unit(&self, config: &DaemonConfig) -> String {
        let binary_path = config
            .binary_path
            .as_ref()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| {
                std::env::current_exe()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|_| "/usr/local/bin/drbot".to_string())
            });

        let mut exec_start = binary_path;
        exec_start.push_str(" gateway");
        for arg in &config.args {
            exec_start.push(' ');
            exec_start.push_str(arg);
        }

        let restart = if config.restart_on_failure {
            "on-failure"
        } else {
            "no"
        };

        let workdir = config
            .working_dir
            .as_ref()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "/tmp".to_string());

        format!(
            r#"[Unit]
Description=drbot Gateway Server
After=network.target

[Service]
Type=simple
ExecStart={exec_start}
WorkingDirectory={workdir}
Restart={restart}
RestartSec=5

[Install]
WantedBy=default.target
"#,
            exec_start = exec_start,
            workdir = workdir,
            restart = restart,
        )
    }

    /// Run systemctl command.
    fn systemctl(&self, args: &[&str]) -> Result<std::process::Output> {
        let mut cmd = Command::new("systemctl");

        if self.user_mode {
            cmd.arg("--user");
        }

        cmd.args(args);

        cmd.output().map_err(|e| DaemonError::IoError(e))
    }

    /// Install the daemon.
    pub fn install(&self, config: &DaemonConfig) -> Result<()> {
        if self.unit_path.exists() {
            return Err(DaemonError::AlreadyInstalled);
        }

        // Create parent directory
        if let Some(parent) = self.unit_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Write unit file
        let unit_content = self.generate_unit(config);
        std::fs::write(&self.unit_path, unit_content)?;

        // Reload systemd
        self.systemctl(&["daemon-reload"])?;

        // Enable if auto-start
        if config.auto_start {
            self.systemctl(&["enable", &self.service_name])?;
        }

        tracing::info!(
            unit_path = %self.unit_path.display(),
            "Installed systemd service"
        );

        Ok(())
    }

    /// Uninstall the daemon.
    pub fn uninstall(&self) -> Result<()> {
        if !self.unit_path.exists() {
            return Err(DaemonError::NotInstalled);
        }

        // Stop and disable first
        let _ = self.stop();
        let _ = self.systemctl(&["disable", &self.service_name]);

        // Remove unit file
        std::fs::remove_file(&self.unit_path)?;

        // Reload systemd
        self.systemctl(&["daemon-reload"])?;

        tracing::info!(
            unit_path = %self.unit_path.display(),
            "Uninstalled systemd service"
        );

        Ok(())
    }

    /// Start the daemon.
    pub fn start(&self) -> Result<()> {
        if !self.unit_path.exists() {
            return Err(DaemonError::NotInstalled);
        }

        let output = self.systemctl(&["start", &self.service_name])?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(DaemonError::StartFailed(stderr.to_string()));
        }

        tracing::info!(service = %self.service_name, "Started systemd service");
        Ok(())
    }

    /// Stop the daemon.
    pub fn stop(&self) -> Result<()> {
        if !self.unit_path.exists() {
            return Err(DaemonError::NotInstalled);
        }

        let output = self.systemctl(&["stop", &self.service_name])?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(DaemonError::StopFailed(stderr.to_string()));
        }

        tracing::info!(service = %self.service_name, "Stopped systemd service");
        Ok(())
    }

    /// Restart the daemon.
    pub fn restart(&self) -> Result<()> {
        if !self.unit_path.exists() {
            return Err(DaemonError::NotInstalled);
        }

        let output = self.systemctl(&["restart", &self.service_name])?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(DaemonError::StartFailed(stderr.to_string()));
        }

        tracing::info!(service = %self.service_name, "Restarted systemd service");
        Ok(())
    }

    /// Get daemon status.
    pub fn status(&self) -> Result<DaemonStatus> {
        if !self.unit_path.exists() {
            return Ok(DaemonStatus::NotInstalled);
        }

        let output = self.systemctl(&["is-active", &self.service_name])?;
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();

        match stdout.as_str() {
            "active" => Ok(DaemonStatus::Running),
            "inactive" | "failed" => Ok(DaemonStatus::Stopped),
            _ => Ok(DaemonStatus::Unknown),
        }
    }

    /// Check if installed.
    pub fn is_installed(&self) -> bool {
        self.unit_path.exists()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_systemd_manager() {
        let manager = SystemdManager::new("drbot-test");
        assert!(!manager.is_installed());
    }
}
