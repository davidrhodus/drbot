//! Sync manager and state.

use crate::{Config, DeviceInfo, Encrypted, EncryptionKey, Result, SyncError};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
use uuid::Uuid;

/// Sync configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConfig {
    /// Base configuration.
    #[serde(flatten)]
    pub base: Config,
    /// Device ID.
    pub device_id: Uuid,
    /// Device name.
    pub device_name: String,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            base: Config::default(),
            device_id: Uuid::new_v4(),
            device_name: hostname::get()
                .map(|h| h.to_string_lossy().to_string())
                .unwrap_or_else(|_| "Unknown Device".to_string()),
        }
    }
}

/// Sync state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyncState {
    /// Not initialized.
    Uninitialized,
    /// Ready but not syncing.
    Idle,
    /// Currently syncing.
    Syncing,
    /// Sync paused.
    Paused,
    /// Error state.
    Error,
}

/// Item to be synced.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncItem {
    /// Unique ID.
    pub id: Uuid,
    /// Item type.
    pub item_type: SyncItemType,
    /// Item data (serialized JSON).
    pub data: serde_json::Value,
    /// Version number.
    pub version: u64,
    /// Last modified timestamp.
    pub modified_at: DateTime<Utc>,
    /// Device that made the change.
    pub modified_by: Uuid,
    /// Deleted flag (soft delete).
    pub deleted: bool,
}

/// Type of syncable item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyncItemType {
    /// Session data.
    Session,
    /// Long-term memory.
    Memory,
    /// Settings.
    Settings,
    /// Persona.
    Persona,
    /// Scheduled task.
    Task,
}

/// Result of a sync operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncResult {
    /// Items pushed.
    pub pushed: usize,
    /// Items pulled.
    pub pulled: usize,
    /// Conflicts encountered.
    pub conflicts: usize,
    /// Conflicts resolved.
    pub resolved: usize,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
}

/// Sync manager.
pub struct SyncManager {
    config: SyncConfig,
    state: Arc<RwLock<SyncState>>,
    key: Option<EncryptionKey>,
    pending_items: Arc<RwLock<Vec<SyncItem>>>,
    last_sync: Arc<RwLock<Option<DateTime<Utc>>>>,
}

impl SyncManager {
    /// Create a new sync manager.
    pub async fn new(config: SyncConfig) -> Result<Self> {
        info!(device_id = %config.device_id, "Initializing sync manager");

        Ok(Self {
            config,
            state: Arc::new(RwLock::new(SyncState::Uninitialized)),
            key: None,
            pending_items: Arc::new(RwLock::new(Vec::new())),
            last_sync: Arc::new(RwLock::new(None)),
        })
    }

    /// Set encryption key.
    pub async fn set_key(&mut self, key: EncryptionKey) {
        self.key = Some(key);
        let mut state = self.state.write().await;
        *state = SyncState::Idle;
    }

    /// Generate device link info.
    pub fn generate_link_info(&self) -> crate::DeviceLinkInfo {
        crate::DeviceLinkInfo {
            device_id: self.config.device_id,
            device_name: self.config.device_name.clone(),
            code: format!("{:08X}", random_u32()),
            expires_at: Utc::now() + chrono::Duration::minutes(5),
        }
    }

    /// Start sync.
    pub async fn start(&self) -> Result<()> {
        if self.key.is_none() {
            return Err(SyncError::InvalidKey);
        }

        let mut state = self.state.write().await;
        *state = SyncState::Idle;

        // Start background sync loop
        self.start_sync_loop();

        info!("Sync started");
        Ok(())
    }

    /// Stop sync.
    pub async fn stop(&self) {
        let mut state = self.state.write().await;
        *state = SyncState::Paused;
        info!("Sync stopped");
    }

    /// Get current state.
    pub async fn state(&self) -> SyncState {
        *self.state.read().await
    }

    /// Queue an item for sync.
    pub async fn queue_item(&self, item: SyncItem) {
        let mut pending = self.pending_items.write().await;
        pending.push(item);
        debug!(pending_count = pending.len(), "Item queued for sync");
    }

    /// Perform a sync cycle.
    pub async fn sync_now(&self) -> Result<SyncResult> {
        let state = *self.state.read().await;
        if state == SyncState::Syncing {
            return Err(SyncError::Conflict("Already syncing".to_string()));
        }

        {
            let mut state = self.state.write().await;
            *state = SyncState::Syncing;
        }

        let result = self.perform_sync().await;

        {
            let mut state = self.state.write().await;
            *state = if result.is_ok() {
                SyncState::Idle
            } else {
                SyncState::Error
            };
        }

        result
    }

    /// Internal sync logic.
    async fn perform_sync(&self) -> Result<SyncResult> {
        let key = self.key.as_ref().ok_or(SyncError::InvalidKey)?;

        let pending = {
            let mut items = self.pending_items.write().await;
            std::mem::take(&mut *items)
        };

        let pushed = pending.len();

        // Encrypt and send items
        for item in &pending {
            let json = serde_json::to_vec(&item)
                .map_err(|e| SyncError::EncryptionFailed(e.to_string()))?;
            let _encrypted = key.encrypt(&json)?;
            // In production, would send to sync server here
        }

        // Pull items from server
        let pulled = 0; // Would receive from server
        let conflicts = 0;
        let resolved = 0;

        let result = SyncResult {
            pushed,
            pulled,
            conflicts,
            resolved,
            timestamp: Utc::now(),
        };

        {
            let mut last = self.last_sync.write().await;
            *last = Some(Utc::now());
        }

        info!(
            pushed = result.pushed,
            pulled = result.pulled,
            "Sync completed"
        );

        Ok(result)
    }

    /// Start background sync loop.
    fn start_sync_loop(&self) {
        let interval = self.config.base.sync_interval_secs;
        let state = self.state.clone();
        let pending = self.pending_items.clone();

        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(interval)).await;

                let current_state = *state.read().await;
                if current_state == SyncState::Paused {
                    break;
                }

                let pending_count = pending.read().await.len();
                if pending_count > 0 && current_state == SyncState::Idle {
                    debug!(pending_count, "Background sync would trigger");
                    // Would call sync_now() here
                }
            }
        });
    }

    /// Get last sync time.
    pub async fn last_sync(&self) -> Option<DateTime<Utc>> {
        *self.last_sync.read().await
    }

    /// Get pending item count.
    pub async fn pending_count(&self) -> usize {
        self.pending_items.read().await.len()
    }
}

// Generate random u32 for link codes
fn random_u32() -> u32 {
    use ring::rand::{SecureRandom, SystemRandom};
    let rng = SystemRandom::new();
    let mut bytes = [0u8; 4];
    rng.fill(&mut bytes).expect("RNG failed");
    u32::from_le_bytes(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_manager_creation() {
        let config = SyncConfig::default();
        let manager = SyncManager::new(config).await.unwrap();

        assert_eq!(manager.state().await, SyncState::Uninitialized);
    }

    #[tokio::test]
    async fn test_queue_item() {
        let config = SyncConfig::default();
        let manager = SyncManager::new(config).await.unwrap();

        let item = SyncItem {
            id: Uuid::new_v4(),
            item_type: SyncItemType::Session,
            data: serde_json::json!({"test": true}),
            version: 1,
            modified_at: Utc::now(),
            modified_by: Uuid::new_v4(),
            deleted: false,
        };

        manager.queue_item(item).await;
        assert_eq!(manager.pending_count().await, 1);
    }

    #[tokio::test]
    async fn test_set_key() {
        let config = SyncConfig::default();
        let mut manager = SyncManager::new(config).await.unwrap();

        let key = EncryptionKey::generate().unwrap();
        manager.set_key(key).await;

        assert_eq!(manager.state().await, SyncState::Idle);
    }

    #[test]
    fn test_sync_config_default() {
        let config = SyncConfig::default();
        assert!(!config.base.enabled);
    }
}
