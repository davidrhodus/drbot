//! macOS launchd integration.

use crate::{DaemonConfig, DaemonError, DaemonStatus, Result};
use std::path::PathBuf;
use std::process::Command;

/// Launchd manager for macOS.
pub struct LaunchdManager {
    /// Service label.
    label: String,
    /// Plist path.
    plist_path: PathBuf,
}

impl LaunchdManager {
    /// Create a new launchd manager.
    pub fn new(label: &str) -> Self {
        let plist_name = format!("{}.plist", label);
        let plist_path = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("Library")
            .join("LaunchAgents")
            .join(&plist_name);

        Self {
            label: label.to_string(),
            plist_path,
        }
    }

    /// Generate plist content.
    fn generate_plist(&self, config: &DaemonConfig) -> String {
        let binary_path = config
            .binary_path
            .as_ref()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| {
                std::env::current_exe()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|_| "drbot".to_string())
            });

        let mut args_xml = String::new();
        args_xml.push_str(&format!("        <string>{}</string>\n", binary_path));
        args_xml.push_str("        <string>gateway</string>\n");

        for arg in &config.args {
            args_xml.push_str(&format!("        <string>{}</string>\n", arg));
        }

        let run_at_load = if config.auto_start { "true" } else { "false" };
        let keep_alive = if config.restart_on_failure {
            "true"
        } else {
            "false"
        };

        let log_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("Library")
            .join("Logs")
            .join("drbot");

        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{label}</string>
    <key>ProgramArguments</key>
    <array>
{args}    </array>
    <key>RunAtLoad</key>
    <{run_at_load}/>
    <key>KeepAlive</key>
    <{keep_alive}/>
    <key>StandardOutPath</key>
    <string>{log_dir}/drbot.log</string>
    <key>StandardErrorPath</key>
    <string>{log_dir}/drbot.error.log</string>
    <key>WorkingDirectory</key>
    <string>{workdir}</string>
</dict>
</plist>
"#,
            label = self.label,
            args = args_xml,
            run_at_load = run_at_load,
            keep_alive = keep_alive,
            log_dir = log_dir.display(),
            workdir = config
                .working_dir
                .as_ref()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| "/tmp".to_string()),
        )
    }

    /// Install the daemon.
    pub fn install(&self, config: &DaemonConfig) -> Result<()> {
        if self.plist_path.exists() {
            return Err(DaemonError::AlreadyInstalled);
        }

        // Create parent directory
        if let Some(parent) = self.plist_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Create log directory
        let log_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("Library")
            .join("Logs")
            .join("drbot");
        std::fs::create_dir_all(&log_dir)?;

        // Write plist
        let plist_content = self.generate_plist(config);
        std::fs::write(&self.plist_path, plist_content)?;

        tracing::info!(
            plist_path = %self.plist_path.display(),
            "Installed launchd service"
        );

        Ok(())
    }

    /// Uninstall the daemon.
    pub fn uninstall(&self) -> Result<()> {
        if !self.plist_path.exists() {
            return Err(DaemonError::NotInstalled);
        }

        // Stop first if running
        let _ = self.stop();

        // Remove plist
        std::fs::remove_file(&self.plist_path)?;

        tracing::info!(
            plist_path = %self.plist_path.display(),
            "Uninstalled launchd service"
        );

        Ok(())
    }

    /// Start the daemon.
    pub fn start(&self) -> Result<()> {
        if !self.plist_path.exists() {
            return Err(DaemonError::NotInstalled);
        }

        let output = Command::new("launchctl")
            .args(["load", &self.plist_path.to_string_lossy()])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(DaemonError::StartFailed(stderr.to_string()));
        }

        tracing::info!(label = %self.label, "Started launchd service");
        Ok(())
    }

    /// Stop the daemon.
    pub fn stop(&self) -> Result<()> {
        if !self.plist_path.exists() {
            return Err(DaemonError::NotInstalled);
        }

        let output = Command::new("launchctl")
            .args(["unload", &self.plist_path.to_string_lossy()])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(DaemonError::StopFailed(stderr.to_string()));
        }

        tracing::info!(label = %self.label, "Stopped launchd service");
        Ok(())
    }

    /// Restart the daemon.
    pub fn restart(&self) -> Result<()> {
        self.stop()?;
        self.start()
    }

    /// Get daemon status.
    pub fn status(&self) -> Result<DaemonStatus> {
        if !self.plist_path.exists() {
            return Ok(DaemonStatus::NotInstalled);
        }

        let output = Command::new("launchctl")
            .args(["list", &self.label])
            .output()?;

        if output.status.success() {
            // Parse output to check PID
            let stdout = String::from_utf8_lossy(&output.stdout);
            if stdout.contains("-\t0\t") {
                Ok(DaemonStatus::Stopped)
            } else {
                Ok(DaemonStatus::Running)
            }
        } else {
            Ok(DaemonStatus::Stopped)
        }
    }

    /// Check if installed.
    pub fn is_installed(&self) -> bool {
        self.plist_path.exists()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_launchd_manager() {
        let manager = LaunchdManager::new("com.drbot.test");
        assert!(!manager.is_installed());
    }
}
