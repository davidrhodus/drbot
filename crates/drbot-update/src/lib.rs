//! Self-update system for drbot.
//!
//! This crate provides functionality for checking for updates,
//! downloading new versions, and performing self-replacement.

mod channel;
mod checker;
mod downloader;
mod installer;
mod manifest;
mod rollback;

pub use channel::*;
pub use checker::*;
pub use downloader::*;
pub use installer::*;
pub use manifest::*;
pub use rollback::*;

use serde::{Deserialize, Serialize};

/// Update system errors.
#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    #[error("No update available")]
    NoUpdateAvailable,
    #[error("Update already in progress")]
    UpdateInProgress,
    #[error("Download failed: {0}")]
    DownloadFailed(String),
    #[error("Checksum verification failed")]
    ChecksumMismatch,
    #[error("Installation failed: {0}")]
    InstallationFailed(String),
    #[error("Rollback failed: {0}")]
    RollbackFailed(String),
    #[error("No rollback available")]
    NoRollbackAvailable,
    #[error("Network error: {0}")]
    NetworkError(#[from] reqwest::Error),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),
    #[error("Invalid version: {0}")]
    InvalidVersion(String),
    #[error("Unsupported platform")]
    UnsupportedPlatform,
}

/// Result type for update operations.
pub type Result<T> = std::result::Result<T, UpdateError>;

/// Current version information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionInfo {
    /// Current version.
    pub current: String,
    /// Release channel.
    pub channel: ReleaseChannel,
    /// Build date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build_date: Option<String>,
    /// Git commit hash.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
}

impl VersionInfo {
    /// Create version info for the current build.
    pub fn current() -> Self {
        Self {
            current: env!("CARGO_PKG_VERSION").to_string(),
            channel: ReleaseChannel::Stable,
            build_date: option_env!("BUILD_DATE").map(String::from),
            commit: option_env!("GIT_COMMIT").map(String::from),
        }
    }
}

/// Update check result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCheckResult {
    /// Whether an update is available.
    pub available: bool,
    /// Current version.
    pub current_version: String,
    /// Latest version (if different).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_version: Option<String>,
    /// Release notes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_notes: Option<String>,
    /// Download size in bytes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub download_size: Option<u64>,
    /// Release channel.
    pub channel: ReleaseChannel,
}

impl UpdateCheckResult {
    /// Create a result indicating no update is available.
    pub fn no_update(current: &str, channel: ReleaseChannel) -> Self {
        Self {
            available: false,
            current_version: current.to_string(),
            latest_version: None,
            release_notes: None,
            download_size: None,
            channel,
        }
    }

    /// Create a result indicating an update is available.
    pub fn update_available(
        current: &str,
        latest: &str,
        channel: ReleaseChannel,
        release_notes: Option<String>,
        download_size: Option<u64>,
    ) -> Self {
        Self {
            available: true,
            current_version: current.to_string(),
            latest_version: Some(latest.to_string()),
            release_notes,
            download_size,
            channel,
        }
    }
}

/// Update progress callback.
pub type ProgressCallback = Box<dyn Fn(UpdateProgress) + Send + Sync>;

/// Update progress information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateProgress {
    /// Current stage.
    pub stage: UpdateStage,
    /// Progress percentage (0-100).
    pub percent: u8,
    /// Bytes downloaded (for download stage).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes_downloaded: Option<u64>,
    /// Total bytes (for download stage).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_bytes: Option<u64>,
    /// Status message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Update stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateStage {
    /// Checking for updates.
    Checking,
    /// Downloading update.
    Downloading,
    /// Verifying download.
    Verifying,
    /// Backing up current version.
    BackingUp,
    /// Installing update.
    Installing,
    /// Finalizing.
    Finalizing,
    /// Complete.
    Complete,
    /// Failed.
    Failed,
}

impl std::fmt::Display for UpdateStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Checking => write!(f, "Checking for updates"),
            Self::Downloading => write!(f, "Downloading update"),
            Self::Verifying => write!(f, "Verifying download"),
            Self::BackingUp => write!(f, "Backing up current version"),
            Self::Installing => write!(f, "Installing update"),
            Self::Finalizing => write!(f, "Finalizing"),
            Self::Complete => write!(f, "Update complete"),
            Self::Failed => write!(f, "Update failed"),
        }
    }
}

/// Update manager configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateConfig {
    /// Release channel.
    #[serde(default)]
    pub channel: ReleaseChannel,
    /// Auto-check for updates.
    #[serde(default = "default_auto_check")]
    pub auto_check: bool,
    /// Check interval in hours.
    #[serde(default = "default_check_interval")]
    pub check_interval_hours: u32,
    /// Custom manifest URL (overrides channel default).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest_url: Option<String>,
    /// Keep N previous versions for rollback.
    #[serde(default = "default_rollback_count")]
    pub rollback_versions: u32,
}

fn default_auto_check() -> bool {
    true
}

fn default_check_interval() -> u32 {
    24
}

fn default_rollback_count() -> u32 {
    2
}

impl Default for UpdateConfig {
    fn default() -> Self {
        Self {
            channel: ReleaseChannel::default(),
            auto_check: default_auto_check(),
            check_interval_hours: default_check_interval(),
            manifest_url: None,
            rollback_versions: default_rollback_count(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_info() {
        let info = VersionInfo::current();
        assert!(!info.current.is_empty());
    }

    #[test]
    fn test_update_check_result() {
        let result = UpdateCheckResult::no_update("1.0.0", ReleaseChannel::Stable);
        assert!(!result.available);
        assert_eq!(result.current_version, "1.0.0");
    }

    #[test]
    fn test_update_config_default() {
        let config = UpdateConfig::default();
        assert!(config.auto_check);
        assert_eq!(config.check_interval_hours, 24);
    }
}
