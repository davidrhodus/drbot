//! Agent and plugin marketplace for drbot.
//!
//! Provides a registry for discovering, installing, and managing agents and plugins.
//!
//! # Features
//!
//! - Agent/plugin registry and discovery
//! - WASM sandboxed execution
//! - Community ratings and reviews
//! - Version management
//! - Creator monetization support

mod plugin;
mod ratings;
mod registry;
mod sandbox;

pub use plugin::{Plugin, PluginConfig, PluginManifest, PluginRuntime};
pub use ratings::{Rating, RatingStats, Review};
pub use registry::{ItemStatus, ItemType, MarketplaceItem, Registry, RegistryConfig};
pub use sandbox::{Sandbox, SandboxConfig, SandboxPermissions};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Marketplace result type.
pub type Result<T> = std::result::Result<T, MarketplaceError>;

/// Marketplace errors.
#[derive(Debug, thiserror::Error)]
pub enum MarketplaceError {
    #[error("Item not found: {0}")]
    NotFound(String),
    #[error("Installation failed: {0}")]
    InstallFailed(String),
    #[error("Sandbox error: {0}")]
    SandboxError(String),
    #[error("Network error: {0}")]
    NetworkError(String),
    #[error("Invalid manifest: {0}")]
    InvalidManifest(String),
    #[error("Permission denied: {0}")]
    PermissionDenied(String),
}

/// Marketplace configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplaceConfig {
    /// Registry URL.
    pub registry_url: String,
    /// Enable community plugins.
    pub allow_community: bool,
    /// Auto-update plugins.
    pub auto_update: bool,
    /// Sandbox all plugins.
    pub sandbox_enabled: bool,
    /// Cache directory.
    pub cache_dir: Option<String>,
}

impl Default for MarketplaceConfig {
    fn default() -> Self {
        Self {
            registry_url: "https://marketplace.drbot.dev".to_string(),
            allow_community: true,
            auto_update: true,
            sandbox_enabled: true,
            cache_dir: None,
        }
    }
}

/// Installed item information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledItem {
    /// Item ID.
    pub id: Uuid,
    /// Item name.
    pub name: String,
    /// Item type.
    pub item_type: ItemType,
    /// Installed version.
    pub version: String,
    /// Latest available version.
    pub latest_version: Option<String>,
    /// Installation time.
    pub installed_at: DateTime<Utc>,
    /// Last updated.
    pub updated_at: DateTime<Utc>,
    /// Is enabled.
    pub enabled: bool,
    /// Configuration.
    pub config: serde_json::Value,
}

/// Creator information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Creator {
    /// Creator ID.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Avatar URL.
    pub avatar_url: Option<String>,
    /// Is verified.
    pub verified: bool,
    /// Item count.
    pub item_count: u32,
    /// Total downloads.
    pub total_downloads: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_marketplace_config_default() {
        let config = MarketplaceConfig::default();
        assert!(config.allow_community);
        assert!(config.sandbox_enabled);
    }
}
