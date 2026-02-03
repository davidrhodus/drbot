//! Selective sync for drbot.
//!
//! Fine-grained data synchronization control.
//!
//! # Features
//!
//! - Per-item sync preferences
//! - Device-aware sync
//! - Bandwidth management
//! - Conflict resolution
//! - Offline support

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Selective sync result type.
pub type Result<T> = std::result::Result<T, SyncError>;

/// Sync errors.
#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error("Sync conflict: {0}")]
    Conflict(String),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Quota exceeded: {0}")]
    QuotaExceeded(String),
    #[error("Sync failed: {0}")]
    Failed(String),
}

/// Syncable item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncItem {
    /// Item ID.
    pub id: Uuid,
    /// Item type.
    pub item_type: String,
    /// Name.
    pub name: String,
    /// Size (bytes).
    pub size: u64,
    /// Content hash.
    pub hash: String,
    /// Version.
    pub version: u64,
    /// Last modified.
    pub modified_at: DateTime<Utc>,
    /// Created at.
    pub created_at: DateTime<Utc>,
    /// Metadata.
    pub metadata: HashMap<String, String>,
}

impl SyncItem {
    /// Create a new sync item.
    pub fn new(item_type: &str, name: &str, size: u64, hash: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            item_type: item_type.to_string(),
            name: name.to_string(),
            size,
            hash: hash.to_string(),
            version: 1,
            modified_at: Utc::now(),
            created_at: Utc::now(),
            metadata: HashMap::new(),
        }
    }
}

/// Sync preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncPreference {
    /// Always sync.
    Always,
    /// Sync on WiFi only.
    WifiOnly,
    /// Manual sync.
    Manual,
    /// Never sync.
    Never,
    /// Sync if smaller than threshold.
    SizeLimit,
}

/// Sync rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncRule {
    /// Rule ID.
    pub id: Uuid,
    /// Name.
    pub name: String,
    /// Item type pattern (glob).
    pub item_type_pattern: String,
    /// Preference.
    pub preference: SyncPreference,
    /// Size limit (bytes).
    pub size_limit: Option<u64>,
    /// Device filter.
    pub device_filter: Option<Vec<String>>,
    /// Priority.
    pub priority: i32,
    /// Enabled.
    pub enabled: bool,
}

impl SyncRule {
    /// Create a new rule.
    pub fn new(name: &str, item_type_pattern: &str, preference: SyncPreference) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.to_string(),
            item_type_pattern: item_type_pattern.to_string(),
            preference,
            size_limit: None,
            device_filter: None,
            priority: 100,
            enabled: true,
        }
    }

    /// Check if rule matches item type.
    pub fn matches(&self, item_type: &str) -> bool {
        if self.item_type_pattern == "*" {
            return true;
        }
        if self.item_type_pattern.ends_with('*') {
            let prefix = &self.item_type_pattern[..self.item_type_pattern.len() - 1];
            return item_type.starts_with(prefix);
        }
        self.item_type_pattern == item_type
    }
}

/// Device info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    /// Device ID.
    pub id: String,
    /// Name.
    pub name: String,
    /// Device type.
    pub device_type: DeviceType,
    /// Last seen.
    pub last_seen: DateTime<Utc>,
    /// Storage quota (bytes).
    pub storage_quota: u64,
    /// Storage used (bytes).
    pub storage_used: u64,
    /// Online.
    pub online: bool,
}

/// Device types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceType {
    Desktop,
    Laptop,
    Phone,
    Tablet,
    Server,
    Other,
}

/// Sync status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncStatus {
    Synced,
    Pending,
    Syncing,
    Conflict,
    Error,
    Excluded,
}

/// Item sync state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemSyncState {
    /// Item ID.
    pub item_id: Uuid,
    /// Device ID.
    pub device_id: String,
    /// Status.
    pub status: SyncStatus,
    /// Local version.
    pub local_version: u64,
    /// Remote version.
    pub remote_version: u64,
    /// Last synced.
    pub last_synced: Option<DateTime<Utc>>,
    /// Error message.
    pub error: Option<String>,
}

/// Conflict.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConflict {
    /// Conflict ID.
    pub id: Uuid,
    /// Item.
    pub item: SyncItem,
    /// Local version.
    pub local: SyncItem,
    /// Remote version.
    pub remote: SyncItem,
    /// Detected at.
    pub detected_at: DateTime<Utc>,
}

/// Conflict resolution strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictResolution {
    KeepLocal,
    KeepRemote,
    KeepBoth,
    KeepNewest,
    Manual,
}

/// Sync configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConfig {
    /// Enable sync.
    pub enabled: bool,
    /// Default preference.
    pub default_preference: SyncPreference,
    /// Default conflict resolution.
    pub conflict_resolution: ConflictResolution,
    /// Max concurrent syncs.
    pub max_concurrent: usize,
    /// Bandwidth limit (bytes/sec, 0 = unlimited).
    pub bandwidth_limit: u64,
    /// Sync interval (seconds).
    pub sync_interval: u64,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            default_preference: SyncPreference::Always,
            conflict_resolution: ConflictResolution::KeepNewest,
            max_concurrent: 5,
            bandwidth_limit: 0,
            sync_interval: 60,
        }
    }
}

/// Sync operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncOperation {
    /// Operation ID.
    pub id: Uuid,
    /// Item ID.
    pub item_id: Uuid,
    /// Operation type.
    pub op_type: OperationType,
    /// Source device.
    pub source: String,
    /// Target device.
    pub target: String,
    /// Status.
    pub status: OperationStatus,
    /// Progress (0-100).
    pub progress: u8,
    /// Started at.
    pub started_at: DateTime<Utc>,
    /// Completed at.
    pub completed_at: Option<DateTime<Utc>>,
}

/// Operation types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationType {
    Upload,
    Download,
    Delete,
    Rename,
}

/// Operation status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
}

/// Trait for sync backends.
#[async_trait]
pub trait SyncBackend: Send + Sync {
    /// List remote items.
    async fn list(&self) -> Result<Vec<SyncItem>>;
    /// Get item.
    async fn get(&self, id: Uuid) -> Result<SyncItem>;
    /// Upload item.
    async fn upload(&self, item: &SyncItem, data: &[u8]) -> Result<()>;
    /// Download item.
    async fn download(&self, id: Uuid) -> Result<Vec<u8>>;
    /// Delete item.
    async fn delete(&self, id: Uuid) -> Result<()>;
}

/// Selective sync engine.
pub struct SelectiveSync<B: SyncBackend> {
    config: SyncConfig,
    backend: B,
    rules: Arc<RwLock<Vec<SyncRule>>>,
    items: Arc<RwLock<HashMap<Uuid, SyncItem>>>,
    states: Arc<RwLock<HashMap<(Uuid, String), ItemSyncState>>>,
    devices: Arc<RwLock<HashMap<String, Device>>>,
    conflicts: Arc<RwLock<Vec<SyncConflict>>>,
    operations: Arc<RwLock<Vec<SyncOperation>>>,
    current_device: String,
}

impl<B: SyncBackend> SelectiveSync<B> {
    /// Create a new sync engine.
    pub fn new(config: SyncConfig, backend: B, device_id: &str) -> Self {
        Self {
            config,
            backend,
            rules: Arc::new(RwLock::new(Vec::new())),
            items: Arc::new(RwLock::new(HashMap::new())),
            states: Arc::new(RwLock::new(HashMap::new())),
            devices: Arc::new(RwLock::new(HashMap::new())),
            conflicts: Arc::new(RwLock::new(Vec::new())),
            operations: Arc::new(RwLock::new(Vec::new())),
            current_device: device_id.to_string(),
        }
    }

    /// Add sync rule.
    pub async fn add_rule(&self, rule: SyncRule) {
        self.rules.write().await.push(rule);
    }

    /// Get preference for item.
    pub async fn get_preference(&self, item: &SyncItem) -> SyncPreference {
        let rules = self.rules.read().await;
        let mut matching: Vec<_> = rules
            .iter()
            .filter(|r| r.enabled && r.matches(&item.item_type))
            .collect();
        matching.sort_by_key(|r| r.priority);

        if let Some(rule) = matching.first() {
            // Check size limit
            if rule.preference == SyncPreference::SizeLimit {
                if let Some(limit) = rule.size_limit {
                    if item.size <= limit {
                        return SyncPreference::Always;
                    } else {
                        return SyncPreference::Manual;
                    }
                }
            }
            rule.preference
        } else {
            self.config.default_preference
        }
    }

    /// Check if item should sync.
    pub async fn should_sync(&self, item: &SyncItem, wifi_available: bool) -> bool {
        match self.get_preference(item).await {
            SyncPreference::Always => true,
            SyncPreference::WifiOnly => wifi_available,
            SyncPreference::Manual => false,
            SyncPreference::Never => false,
            SyncPreference::SizeLimit => true, // Already evaluated in get_preference
        }
    }

    /// Add local item.
    pub async fn add_item(&self, item: SyncItem) -> Result<()> {
        let id = item.id;
        self.items.write().await.insert(id, item);

        // Create sync state
        let state = ItemSyncState {
            item_id: id,
            device_id: self.current_device.clone(),
            status: SyncStatus::Pending,
            local_version: 1,
            remote_version: 0,
            last_synced: None,
            error: None,
        };
        self.states
            .write()
            .await
            .insert((id, self.current_device.clone()), state);

        Ok(())
    }

    /// Sync item.
    pub async fn sync_item(&self, id: Uuid) -> Result<()> {
        let item = self
            .items
            .read()
            .await
            .get(&id)
            .cloned()
            .ok_or(SyncError::NotFound(id.to_string()))?;

        let state_key = (id, self.current_device.clone());
        let state = self.states.read().await.get(&state_key).cloned();

        // Create operation
        let operation = SyncOperation {
            id: Uuid::new_v4(),
            item_id: id,
            op_type: OperationType::Upload,
            source: self.current_device.clone(),
            target: "remote".to_string(),
            status: OperationStatus::Running,
            progress: 0,
            started_at: Utc::now(),
            completed_at: None,
        };
        self.operations.write().await.push(operation.clone());

        // Check for conflicts
        if let Some(state) = &state {
            if state.remote_version > state.local_version {
                // Potential conflict
                match self.config.conflict_resolution {
                    ConflictResolution::KeepLocal => {
                        // Continue with upload
                    }
                    ConflictResolution::KeepRemote => {
                        // Download instead
                        let _data = self.backend.download(id).await?;
                        return Ok(());
                    }
                    ConflictResolution::KeepNewest => {
                        let remote = self.backend.get(id).await?;
                        if remote.modified_at > item.modified_at {
                            return Ok(()); // Keep remote
                        }
                    }
                    _ => {}
                }
            }
        }

        // Simulate upload (in real impl, would get actual data)
        self.backend.upload(&item, &[]).await?;

        // Update state
        if let Some(mut state) = self.states.write().await.get_mut(&state_key) {
            state.status = SyncStatus::Synced;
            state.remote_version = state.local_version;
            state.last_synced = Some(Utc::now());
        }

        // Update operation
        let mut ops = self.operations.write().await;
        if let Some(op) = ops.iter_mut().find(|o| o.id == operation.id) {
            op.status = OperationStatus::Completed;
            op.progress = 100;
            op.completed_at = Some(Utc::now());
        }

        Ok(())
    }

    /// Get pending items.
    pub async fn get_pending(&self) -> Vec<SyncItem> {
        let items = self.items.read().await;
        let states = self.states.read().await;

        items
            .values()
            .filter(|item| {
                let key = (item.id, self.current_device.clone());
                states
                    .get(&key)
                    .map(|s| s.status == SyncStatus::Pending)
                    .unwrap_or(true)
            })
            .cloned()
            .collect()
    }

    /// Get conflicts.
    pub async fn get_conflicts(&self) -> Vec<SyncConflict> {
        self.conflicts.read().await.clone()
    }

    /// Resolve conflict.
    pub async fn resolve_conflict(
        &self,
        conflict_id: Uuid,
        resolution: ConflictResolution,
    ) -> Result<()> {
        let mut conflicts = self.conflicts.write().await;
        if let Some(pos) = conflicts.iter().position(|c| c.id == conflict_id) {
            let conflict = conflicts.remove(pos);

            match resolution {
                ConflictResolution::KeepLocal => {
                    self.backend.upload(&conflict.local, &[]).await?;
                }
                ConflictResolution::KeepRemote => {
                    self.items
                        .write()
                        .await
                        .insert(conflict.item.id, conflict.remote);
                }
                ConflictResolution::KeepBoth => {
                    // Create copy with new ID
                    let mut copy = conflict.local.clone();
                    copy.id = Uuid::new_v4();
                    copy.name = format!("{} (conflict)", copy.name);
                    self.items.write().await.insert(copy.id, copy);
                }
                _ => {}
            }
        }

        Ok(())
    }

    /// Register device.
    pub async fn register_device(&self, device: Device) {
        self.devices.write().await.insert(device.id.clone(), device);
    }

    /// Get sync status summary.
    pub async fn status(&self) -> SyncStatusSummary {
        let items = self.items.read().await;
        let states = self.states.read().await;
        let conflicts = self.conflicts.read().await;

        let mut synced = 0;
        let mut pending = 0;
        let mut errors = 0;

        for item in items.values() {
            let key = (item.id, self.current_device.clone());
            if let Some(state) = states.get(&key) {
                match state.status {
                    SyncStatus::Synced => synced += 1,
                    SyncStatus::Pending | SyncStatus::Syncing => pending += 1,
                    SyncStatus::Error => errors += 1,
                    _ => {}
                }
            }
        }

        SyncStatusSummary {
            total_items: items.len(),
            synced,
            pending,
            conflicts: conflicts.len(),
            errors,
        }
    }

    /// Get excluded items.
    pub async fn get_excluded(&self) -> Vec<SyncItem> {
        let items = self.items.read().await;
        let mut excluded = Vec::new();

        for item in items.values() {
            if self.get_preference(item).await == SyncPreference::Never {
                excluded.push(item.clone());
            }
        }

        excluded
    }
}

/// Sync status summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncStatusSummary {
    pub total_items: usize,
    pub synced: usize,
    pub pending: usize,
    pub conflicts: usize,
    pub errors: usize,
}

/// Mock sync backend for testing.
pub struct MockBackend {
    items: Arc<RwLock<HashMap<Uuid, SyncItem>>>,
}

impl MockBackend {
    pub fn new() -> Self {
        Self {
            items: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl Default for MockBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SyncBackend for MockBackend {
    async fn list(&self) -> Result<Vec<SyncItem>> {
        Ok(self.items.read().await.values().cloned().collect())
    }

    async fn get(&self, id: Uuid) -> Result<SyncItem> {
        self.items
            .read()
            .await
            .get(&id)
            .cloned()
            .ok_or(SyncError::NotFound(id.to_string()))
    }

    async fn upload(&self, item: &SyncItem, _data: &[u8]) -> Result<()> {
        self.items.write().await.insert(item.id, item.clone());
        Ok(())
    }

    async fn download(&self, _id: Uuid) -> Result<Vec<u8>> {
        Ok(Vec::new())
    }

    async fn delete(&self, id: Uuid) -> Result<()> {
        self.items.write().await.remove(&id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_add_item() {
        let sync = SelectiveSync::new(SyncConfig::default(), MockBackend::new(), "device-1");
        let item = SyncItem::new("document", "test.txt", 100, "abc123");

        sync.add_item(item).await.unwrap();

        let pending = sync.get_pending().await;
        assert_eq!(pending.len(), 1);
    }

    #[tokio::test]
    async fn test_sync_item() {
        let sync = SelectiveSync::new(SyncConfig::default(), MockBackend::new(), "device-1");
        let item = SyncItem::new("document", "test.txt", 100, "abc123");
        let id = item.id;

        sync.add_item(item).await.unwrap();
        sync.sync_item(id).await.unwrap();

        let status = sync.status().await;
        assert_eq!(status.synced, 1);
        assert_eq!(status.pending, 0);
    }

    #[tokio::test]
    async fn test_sync_rules() {
        let sync = SelectiveSync::new(SyncConfig::default(), MockBackend::new(), "device-1");

        sync.add_rule(SyncRule::new("Videos", "video/*", SyncPreference::WifiOnly))
            .await;
        sync.add_rule(SyncRule::new("No Temp", "temp/*", SyncPreference::Never))
            .await;

        let video = SyncItem::new("video/mp4", "movie.mp4", 1_000_000, "hash");
        let temp = SyncItem::new("temp/cache", "cache.tmp", 100, "hash");
        let doc = SyncItem::new("document", "doc.txt", 100, "hash");

        assert_eq!(sync.get_preference(&video).await, SyncPreference::WifiOnly);
        assert_eq!(sync.get_preference(&temp).await, SyncPreference::Never);
        assert_eq!(sync.get_preference(&doc).await, SyncPreference::Always);
    }

    #[tokio::test]
    async fn test_should_sync() {
        let sync = SelectiveSync::new(SyncConfig::default(), MockBackend::new(), "device-1");
        sync.add_rule(SyncRule::new("Videos", "video/*", SyncPreference::WifiOnly))
            .await;

        let video = SyncItem::new("video/mp4", "movie.mp4", 1_000_000, "hash");

        assert!(!sync.should_sync(&video, false).await);
        assert!(sync.should_sync(&video, true).await);
    }

    #[tokio::test]
    async fn test_size_limit_rule() {
        let sync = SelectiveSync::new(SyncConfig::default(), MockBackend::new(), "device-1");

        let mut rule = SyncRule::new("Large Files", "document/*", SyncPreference::SizeLimit);
        rule.size_limit = Some(1_000_000); // 1MB limit
        sync.add_rule(rule).await;

        let small = SyncItem::new("document/pdf", "small.pdf", 500_000, "hash");
        let large = SyncItem::new("document/pdf", "large.pdf", 5_000_000, "hash");

        assert_eq!(sync.get_preference(&small).await, SyncPreference::Always);
        assert_eq!(sync.get_preference(&large).await, SyncPreference::Manual);
    }

    #[tokio::test]
    async fn test_status_summary() {
        let sync = SelectiveSync::new(SyncConfig::default(), MockBackend::new(), "device-1");

        let item1 = SyncItem::new("doc", "a.txt", 100, "h1");
        let item2 = SyncItem::new("doc", "b.txt", 100, "h2");
        let id1 = item1.id;

        sync.add_item(item1).await.unwrap();
        sync.add_item(item2).await.unwrap();
        sync.sync_item(id1).await.unwrap();

        let status = sync.status().await;
        assert_eq!(status.total_items, 2);
        assert_eq!(status.synced, 1);
        assert_eq!(status.pending, 1);
    }
}
