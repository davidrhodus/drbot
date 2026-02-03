//! Daemon/service management for drbot.
//!
//! This crate provides platform-specific daemon installation and management.
//!
//! # Features
//!
//! - macOS launchd integration
//! - Linux systemd integration
//! - PID file management
//! - Start/stop/status operations

mod control;
mod launchd;
mod pidfile;
mod platform;
mod systemd;

pub use control::{DaemonConfig, DaemonInfo, DaemonManager, DaemonStatus};
pub use launchd::LaunchdManager;
pub use pidfile::{PidFile, PidFileError};
pub use platform::{detect_platform, Platform};
pub use systemd::SystemdManager;

use serde::{Deserialize, Serialize};

/// Result type for daemon operations.
pub type Result<T> = std::result::Result<T, DaemonError>;

/// Daemon errors.
#[derive(Debug, thiserror::Error)]
pub enum DaemonError {
    #[error("Not supported on this platform")]
    NotSupported,
    #[error("Installation failed: {0}")]
    InstallationFailed(String),
    #[error("Already installed")]
    AlreadyInstalled,
    #[error("Not installed")]
    NotInstalled,
    #[error("Failed to start: {0}")]
    StartFailed(String),
    #[error("Failed to stop: {0}")]
    StopFailed(String),
    #[error("Command failed: {0}")]
    CommandFailed(String),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_platform() {
        let platform = detect_platform();
        // Should detect something
        assert!(matches!(
            platform,
            Platform::MacOS | Platform::Linux | Platform::Windows | Platform::Unknown
        ));
    }
}
