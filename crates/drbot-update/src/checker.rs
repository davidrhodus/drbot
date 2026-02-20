//! Update checking functionality.

use crate::{ReleaseChannel, Result, UpdateCheckResult, UpdateConfig, UpdateError, UpdateManifest};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Update checker.
pub struct UpdateChecker {
    config: UpdateConfig,
    client: reqwest::Client,
    last_check: Arc<RwLock<Option<chrono::DateTime<chrono::Utc>>>>,
    cached_manifest: Arc<RwLock<Option<UpdateManifest>>>,
}

impl UpdateChecker {
    /// Create a new update checker.
    pub fn new(config: UpdateConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
            last_check: Arc::new(RwLock::new(None)),
            cached_manifest: Arc::new(RwLock::new(None)),
        }
    }

    /// Check for updates.
    pub async fn check(&self) -> Result<UpdateCheckResult> {
        let current_version = env!("CARGO_PKG_VERSION");

        // Fetch manifest
        let manifest = self.fetch_manifest().await?;

        // Check if current platform is supported
        if manifest.binary_for_current_platform().is_none() {
            return Err(UpdateError::UnsupportedPlatform);
        }

        // Check if update is available
        if !manifest.needs_update(current_version) {
            return Ok(UpdateCheckResult::no_update(
                current_version,
                self.config.channel,
            ));
        }

        // Check if current version is supported for upgrade
        if !manifest.is_version_supported(current_version) {
            return Err(UpdateError::InvalidVersion(format!(
                "Version {} is too old for upgrade. Minimum required: {}",
                current_version,
                manifest.min_version.as_deref().unwrap_or("unknown")
            )));
        }

        // Get download size
        let download_size = manifest.binary_for_current_platform().map(|b| b.size);

        Ok(UpdateCheckResult::update_available(
            current_version,
            &manifest.latest_version,
            self.config.channel,
            manifest.release_notes.clone(),
            download_size,
        ))
    }

    /// Fetch the update manifest.
    pub async fn fetch_manifest(&self) -> Result<UpdateManifest> {
        let url = self
            .config
            .manifest_url
            .as_deref()
            .unwrap_or_else(|| self.config.channel.manifest_url());

        let response = self.client.get(url).send().await?;

        if !response.status().is_success() {
            return Err(UpdateError::NetworkError(reqwest::Error::from(
                response.error_for_status().unwrap_err(),
            )));
        }

        let manifest: UpdateManifest = response.json().await?;

        // Update cache
        *self.cached_manifest.write().await = Some(manifest.clone());
        *self.last_check.write().await = Some(chrono::Utc::now());

        Ok(manifest)
    }

    /// Get the cached manifest if available.
    pub async fn cached_manifest(&self) -> Option<UpdateManifest> {
        self.cached_manifest.read().await.clone()
    }

    /// Check if we should auto-check for updates.
    pub async fn should_auto_check(&self) -> bool {
        if !self.config.auto_check {
            return false;
        }

        let last = self.last_check.read().await;
        match *last {
            Some(time) => {
                let elapsed = chrono::Utc::now() - time;
                elapsed.num_hours() >= self.config.check_interval_hours as i64
            }
            None => true,
        }
    }

    /// Get the release channel.
    pub fn channel(&self) -> ReleaseChannel {
        self.config.channel
    }

    /// Set the release channel.
    pub fn set_channel(&mut self, channel: ReleaseChannel) {
        self.config.channel = channel;
        // Clear cached manifest when channel changes
        let cached = self.cached_manifest.clone();
        tokio::spawn(async move {
            *cached.write().await = None;
        });
    }
}

/// Get the state file path for storing update check state.
pub fn state_file_path() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("drbot")
        .join("update-state.json")
}

/// Update check state.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UpdateCheckState {
    /// Last check timestamp.
    pub last_check: Option<chrono::DateTime<chrono::Utc>>,
    /// Last check result.
    pub last_result: Option<UpdateCheckResult>,
    /// Ignored version (user chose to skip).
    pub ignored_version: Option<String>,
}

impl Default for UpdateCheckState {
    fn default() -> Self {
        Self {
            last_check: None,
            last_result: None,
            ignored_version: None,
        }
    }
}

impl UpdateCheckState {
    /// Load state from file.
    pub fn load() -> Result<Self> {
        let path = state_file_path();
        if !path.exists() {
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(&path)?;
        let state: Self = serde_json::from_str(&content)?;
        Ok(state)
    }

    /// Save state to file.
    pub fn save(&self) -> Result<()> {
        let path = state_file_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, content)?;
        Ok(())
    }

    /// Check if a version is ignored.
    pub fn is_version_ignored(&self, version: &str) -> bool {
        self.ignored_version.as_deref() == Some(version)
    }

    /// Ignore a version.
    pub fn ignore_version(&mut self, version: &str) {
        self.ignored_version = Some(version.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_update_check_state_default() {
        let state = UpdateCheckState::default();
        assert!(state.last_check.is_none());
        assert!(state.ignored_version.is_none());
    }

    #[test]
    fn test_ignore_version() {
        let mut state = UpdateCheckState::default();
        state.ignore_version("1.2.0");
        assert!(state.is_version_ignored("1.2.0"));
        assert!(!state.is_version_ignored("1.3.0"));
    }

    #[tokio::test]
    async fn test_update_checker_should_auto_check() {
        let config = UpdateConfig {
            auto_check: true,
            check_interval_hours: 24,
            ..Default::default()
        };
        let checker = UpdateChecker::new(config);
        assert!(checker.should_auto_check().await);
    }
}
