//! Daemon control operations.

use crate::{detect_platform, DaemonError, LaunchdManager, Platform, Result, SystemdManager};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Daemon status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DaemonStatus {
    /// Daemon is running.
    Running,
    /// Daemon is stopped.
    Stopped,
    /// Daemon is not installed.
    NotInstalled,
    /// Status unknown.
    Unknown,
}

impl std::fmt::Display for DaemonStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Running => write!(f, "running"),
            Self::Stopped => write!(f, "stopped"),
            Self::NotInstalled => write!(f, "not installed"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

/// Daemon configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    /// Path to the binary.
    pub binary_path: Option<PathBuf>,
    /// Working directory.
    pub working_dir: Option<PathBuf>,
    /// Additional arguments.
    #[serde(default)]
    pub args: Vec<String>,
    /// Start automatically on system boot.
    #[serde(default)]
    pub auto_start: bool,
    /// Restart on failure.
    #[serde(default = "default_restart")]
    pub restart_on_failure: bool,
}

fn default_restart() -> bool {
    true
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            binary_path: None,
            working_dir: None,
            args: Vec::new(),
            auto_start: false,
            restart_on_failure: default_restart(),
        }
    }
}

/// Daemon information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonInfo {
    /// Daemon status.
    pub status: DaemonStatus,
    /// Platform.
    pub platform: String,
    /// Service manager.
    pub service_manager: String,
    /// Service name.
    pub service_name: String,
    /// Process ID (if running).
    pub pid: Option<u32>,
}

/// Platform-agnostic daemon manager.
pub struct DaemonManager {
    platform: Platform,
    launchd: Option<LaunchdManager>,
    systemd: Option<SystemdManager>,
}

impl DaemonManager {
    /// Create a new daemon manager.
    pub fn new() -> Result<Self> {
        let platform = detect_platform();

        let (launchd, systemd) = match platform {
            Platform::MacOS => (Some(LaunchdManager::new("com.drbot.gateway")), None),
            Platform::Linux => (None, Some(SystemdManager::new("drbot"))),
            _ => (None, None),
        };

        Ok(Self {
            platform,
            launchd,
            systemd,
        })
    }

    /// Install the daemon.
    pub fn install(&self, config: &DaemonConfig) -> Result<()> {
        match self.platform {
            Platform::MacOS => self
                .launchd
                .as_ref()
                .ok_or(DaemonError::NotSupported)?
                .install(config),
            Platform::Linux => self
                .systemd
                .as_ref()
                .ok_or(DaemonError::NotSupported)?
                .install(config),
            _ => Err(DaemonError::NotSupported),
        }
    }

    /// Uninstall the daemon.
    pub fn uninstall(&self) -> Result<()> {
        match self.platform {
            Platform::MacOS => self
                .launchd
                .as_ref()
                .ok_or(DaemonError::NotSupported)?
                .uninstall(),
            Platform::Linux => self
                .systemd
                .as_ref()
                .ok_or(DaemonError::NotSupported)?
                .uninstall(),
            _ => Err(DaemonError::NotSupported),
        }
    }

    /// Start the daemon.
    pub fn start(&self) -> Result<()> {
        match self.platform {
            Platform::MacOS => self
                .launchd
                .as_ref()
                .ok_or(DaemonError::NotSupported)?
                .start(),
            Platform::Linux => self
                .systemd
                .as_ref()
                .ok_or(DaemonError::NotSupported)?
                .start(),
            _ => Err(DaemonError::NotSupported),
        }
    }

    /// Stop the daemon.
    pub fn stop(&self) -> Result<()> {
        match self.platform {
            Platform::MacOS => self
                .launchd
                .as_ref()
                .ok_or(DaemonError::NotSupported)?
                .stop(),
            Platform::Linux => self
                .systemd
                .as_ref()
                .ok_or(DaemonError::NotSupported)?
                .stop(),
            _ => Err(DaemonError::NotSupported),
        }
    }

    /// Restart the daemon.
    pub fn restart(&self) -> Result<()> {
        match self.platform {
            Platform::MacOS => self
                .launchd
                .as_ref()
                .ok_or(DaemonError::NotSupported)?
                .restart(),
            Platform::Linux => self
                .systemd
                .as_ref()
                .ok_or(DaemonError::NotSupported)?
                .restart(),
            _ => Err(DaemonError::NotSupported),
        }
    }

    /// Get daemon status.
    pub fn status(&self) -> Result<DaemonStatus> {
        match self.platform {
            Platform::MacOS => self
                .launchd
                .as_ref()
                .ok_or(DaemonError::NotSupported)?
                .status(),
            Platform::Linux => self
                .systemd
                .as_ref()
                .ok_or(DaemonError::NotSupported)?
                .status(),
            _ => Err(DaemonError::NotSupported),
        }
    }

    /// Get daemon info.
    pub fn info(&self) -> Result<DaemonInfo> {
        let status = self.status()?;

        let service_manager = self.platform.service_manager().unwrap_or("unknown");
        let service_name = match self.platform {
            Platform::MacOS => "com.drbot.gateway",
            Platform::Linux => "drbot",
            _ => "drbot",
        };

        Ok(DaemonInfo {
            status,
            platform: format!("{:?}", self.platform),
            service_manager: service_manager.to_string(),
            service_name: service_name.to_string(),
            pid: None, // Would need to read from PID file or query service manager
        })
    }

    /// Check if the daemon is installed.
    pub fn is_installed(&self) -> bool {
        match self.platform {
            Platform::MacOS => self
                .launchd
                .as_ref()
                .map(|l| l.is_installed())
                .unwrap_or(false),
            Platform::Linux => self
                .systemd
                .as_ref()
                .map(|s| s.is_installed())
                .unwrap_or(false),
            _ => false,
        }
    }

    /// Get the current platform.
    pub fn platform(&self) -> Platform {
        self.platform
    }
}

impl Default for DaemonManager {
    fn default() -> Self {
        Self::new().expect("Failed to create daemon manager")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_daemon_config_default() {
        let config = DaemonConfig::default();
        assert!(config.restart_on_failure);
        assert!(!config.auto_start);
    }

    #[test]
    fn test_daemon_status_display() {
        assert_eq!(DaemonStatus::Running.to_string(), "running");
        assert_eq!(DaemonStatus::Stopped.to_string(), "stopped");
    }
}
