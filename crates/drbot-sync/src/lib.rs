//! Encrypted cross-device synchronization for drbot.
//!
//! Provides secure, end-to-end encrypted sync of:
//! - Session history
//! - Long-term memories
//! - Settings and preferences
//! - Personas
//!
//! # Features
//!
//! - End-to-end encryption using AES-256-GCM
//! - Device linking via QR code or manual key entry
//! - Conflict resolution for concurrent edits
//! - Incremental sync to minimize bandwidth
//! - Offline-first with eventual consistency
//!
//! # Example
//!
//! ```rust,no_run
//! use drbot_sync::{SyncManager, SyncConfig, DeviceInfo};
//!
//! async fn example() {
//!     let config = SyncConfig::default();
//!     let manager = SyncManager::new(config).await.unwrap();
//!
//!     // Generate device linking info
//!     let link_info = manager.generate_link_info();
//!     println!("Link code: {}", link_info.code);
//!
//!     // Start sync
//!     manager.start().await.unwrap();
//! }
//! ```

mod conflict;
mod crypto;
mod device;
mod handoff;
mod sync;

pub use conflict::{ConflictResolution, ConflictResolver, MergeStrategy};
pub use crypto::{derive_key, Encrypted, EncryptionKey};
pub use device::{DeviceInfo, DeviceLinkInfo, DeviceRegistry};
pub use handoff::{
    ActiveConversation, ConversationSnapshot, Device, DeviceCapabilities, DeviceType, HandoffEvent,
    HandoffManager, HandoffMessage, HandoffRequest, Platform,
};
pub use sync::{SyncConfig, SyncItem, SyncManager, SyncResult, SyncState};

use serde::{Deserialize, Serialize};

/// Result type for sync operations.
pub type Result<T> = std::result::Result<T, SyncError>;

/// Sync errors.
#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error("Encryption failed: {0}")]
    EncryptionFailed(String),
    #[error("Decryption failed: {0}")]
    DecryptionFailed(String),
    #[error("Invalid key")]
    InvalidKey,
    #[error("Device not linked")]
    DeviceNotLinked,
    #[error("Sync conflict: {0}")]
    Conflict(String),
    #[error("Network error: {0}")]
    NetworkError(String),
    #[error("Storage error: {0}")]
    StorageError(String),
}

/// Sync configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Enable sync.
    pub enabled: bool,
    /// Sync server URL (or None for local-only).
    pub server_url: Option<String>,
    /// Sync interval in seconds.
    pub sync_interval_secs: u64,
    /// Maximum items per sync.
    pub max_items_per_sync: usize,
    /// Conflict resolution strategy.
    pub conflict_strategy: MergeStrategy,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: false,
            server_url: None,
            sync_interval_secs: 60,
            max_items_per_sync: 100,
            conflict_strategy: MergeStrategy::LastWriteWins,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert!(!config.enabled);
    }
}
