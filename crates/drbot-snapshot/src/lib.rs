//! Session snapshot management for drbot.
//!
//! Save and restore session state.
//!
//! # Features
//!
//! - Session snapshots
//! - State serialization
//! - Quick restore
//! - Snapshot versioning
//! - Diff between snapshots

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Snapshot result type.
pub type Result<T> = std::result::Result<T, SnapshotError>;

/// Snapshot errors.
#[derive(Debug, thiserror::Error)]
pub enum SnapshotError {
    #[error("Snapshot not found: {0}")]
    NotFound(String),
    #[error("Serialization error: {0}")]
    SerializationError(String),
    #[error("Storage error: {0}")]
    StorageError(String),
    #[error("Restore failed: {0}")]
    RestoreFailed(String),
    #[error("Version mismatch: {0}")]
    VersionMismatch(String),
}

/// Session snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    /// Snapshot ID.
    pub id: Uuid,
    /// Snapshot name.
    pub name: String,
    /// Description.
    pub description: Option<String>,
    /// Session ID.
    pub session_id: String,
    /// User ID.
    pub user_id: String,
    /// State data.
    pub state: SessionState,
    /// Schema version.
    pub schema_version: u32,
    /// Created at.
    pub created_at: DateTime<Utc>,
    /// Size in bytes.
    pub size_bytes: usize,
    /// Tags.
    pub tags: Vec<String>,
    /// Parent snapshot (for incremental).
    pub parent_id: Option<Uuid>,
    /// Metadata.
    pub metadata: HashMap<String, String>,
}

impl Snapshot {
    /// Create a new snapshot.
    pub fn new(name: &str, session_id: &str, user_id: &str, state: SessionState) -> Self {
        let json = serde_json::to_string(&state).unwrap_or_default();
        Self {
            id: Uuid::new_v4(),
            name: name.to_string(),
            description: None,
            session_id: session_id.to_string(),
            user_id: user_id.to_string(),
            state,
            schema_version: 1,
            created_at: Utc::now(),
            size_bytes: json.len(),
            tags: Vec::new(),
            parent_id: None,
            metadata: HashMap::new(),
        }
    }

    /// Set description.
    pub fn with_description(mut self, desc: &str) -> Self {
        self.description = Some(desc.to_string());
        self
    }

    /// Add tag.
    pub fn with_tag(mut self, tag: &str) -> Self {
        self.tags.push(tag.to_string());
        self
    }

    /// Set parent.
    pub fn with_parent(mut self, parent_id: Uuid) -> Self {
        self.parent_id = Some(parent_id);
        self
    }
}

/// Session state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionState {
    /// Conversation messages.
    pub messages: Vec<Message>,
    /// Context variables.
    pub context: HashMap<String, serde_json::Value>,
    /// Active tools/plugins.
    pub active_tools: Vec<String>,
    /// Model configuration.
    pub model_config: ModelConfig,
    /// User preferences.
    pub preferences: HashMap<String, serde_json::Value>,
    /// Working directory.
    pub working_directory: Option<String>,
    /// Open files.
    pub open_files: Vec<String>,
    /// Custom state.
    pub custom: HashMap<String, serde_json::Value>,
}

impl Default for SessionState {
    fn default() -> Self {
        Self {
            messages: Vec::new(),
            context: HashMap::new(),
            active_tools: Vec::new(),
            model_config: ModelConfig::default(),
            preferences: HashMap::new(),
            working_directory: None,
            open_files: Vec::new(),
            custom: HashMap::new(),
        }
    }
}

/// Message in session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Message ID.
    pub id: Uuid,
    /// Role.
    pub role: String,
    /// Content.
    pub content: String,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
    /// Metadata.
    pub metadata: HashMap<String, serde_json::Value>,
}

impl Message {
    /// Create a new message.
    pub fn new(role: &str, content: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            role: role.to_string(),
            content: content.to_string(),
            timestamp: Utc::now(),
            metadata: HashMap::new(),
        }
    }
}

/// Model configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    /// Model ID.
    pub model_id: String,
    /// Temperature.
    pub temperature: f32,
    /// Max tokens.
    pub max_tokens: Option<usize>,
    /// System prompt.
    pub system_prompt: Option<String>,
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            model_id: "default".to_string(),
            temperature: 0.7,
            max_tokens: None,
            system_prompt: None,
        }
    }
}

/// Snapshot diff.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotDiff {
    /// Base snapshot ID.
    pub base_id: Uuid,
    /// Target snapshot ID.
    pub target_id: Uuid,
    /// Added messages.
    pub messages_added: Vec<Message>,
    /// Removed messages.
    pub messages_removed: Vec<Uuid>,
    /// Context changes.
    pub context_changes: Vec<ContextChange>,
    /// Config changes.
    pub config_changed: bool,
    /// Summary.
    pub summary: String,
}

/// Context change.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextChange {
    /// Key.
    pub key: String,
    /// Old value.
    pub old_value: Option<serde_json::Value>,
    /// New value.
    pub new_value: Option<serde_json::Value>,
    /// Change type.
    pub change_type: ChangeType,
}

/// Change type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeType {
    Added,
    Modified,
    Removed,
}

/// Snapshot query.
#[derive(Debug, Clone, Default)]
pub struct SnapshotQuery {
    /// Session ID filter.
    pub session_id: Option<String>,
    /// User ID filter.
    pub user_id: Option<String>,
    /// Tag filter.
    pub tags: Vec<String>,
    /// Date range start.
    pub after: Option<DateTime<Utc>>,
    /// Date range end.
    pub before: Option<DateTime<Utc>>,
    /// Limit.
    pub limit: Option<usize>,
}

impl SnapshotQuery {
    /// Create a new query.
    pub fn new() -> Self {
        Self::default()
    }

    /// Filter by session.
    pub fn for_session(mut self, session_id: &str) -> Self {
        self.session_id = Some(session_id.to_string());
        self
    }

    /// Filter by user.
    pub fn for_user(mut self, user_id: &str) -> Self {
        self.user_id = Some(user_id.to_string());
        self
    }

    /// Filter by tag.
    pub fn with_tag(mut self, tag: &str) -> Self {
        self.tags.push(tag.to_string());
        self
    }

    /// Set limit.
    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }
}

/// Snapshot manager.
pub struct SnapshotManager {
    snapshots: Arc<RwLock<HashMap<Uuid, Snapshot>>>,
    auto_snapshot_interval: Option<std::time::Duration>,
}

impl SnapshotManager {
    /// Create a new snapshot manager.
    pub fn new() -> Self {
        Self {
            snapshots: Arc::new(RwLock::new(HashMap::new())),
            auto_snapshot_interval: None,
        }
    }

    /// Enable auto-snapshots.
    pub fn with_auto_snapshot(mut self, interval: std::time::Duration) -> Self {
        self.auto_snapshot_interval = Some(interval);
        self
    }

    /// Create a snapshot.
    pub async fn create(&self, snapshot: Snapshot) -> Result<Uuid> {
        let id = snapshot.id;
        self.snapshots.write().await.insert(id, snapshot);
        Ok(id)
    }

    /// Get a snapshot.
    pub async fn get(&self, id: Uuid) -> Option<Snapshot> {
        self.snapshots.read().await.get(&id).cloned()
    }

    /// Delete a snapshot.
    pub async fn delete(&self, id: Uuid) -> Option<Snapshot> {
        self.snapshots.write().await.remove(&id)
    }

    /// List snapshots.
    pub async fn list(&self, query: SnapshotQuery) -> Vec<Snapshot> {
        let snapshots = self.snapshots.read().await;

        let mut results: Vec<_> = snapshots
            .values()
            .filter(|s| {
                if let Some(ref session_id) = query.session_id {
                    if s.session_id != *session_id {
                        return false;
                    }
                }
                if let Some(ref user_id) = query.user_id {
                    if s.user_id != *user_id {
                        return false;
                    }
                }
                if !query.tags.is_empty() {
                    if !query.tags.iter().all(|t| s.tags.contains(t)) {
                        return false;
                    }
                }
                if let Some(after) = query.after {
                    if s.created_at < after {
                        return false;
                    }
                }
                if let Some(before) = query.before {
                    if s.created_at > before {
                        return false;
                    }
                }
                true
            })
            .cloned()
            .collect();

        // Sort by creation time (newest first)
        results.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        if let Some(limit) = query.limit {
            results.truncate(limit);
        }

        results
    }

    /// Get latest snapshot for a session.
    pub async fn latest(&self, session_id: &str) -> Option<Snapshot> {
        self.list(SnapshotQuery::new().for_session(session_id).limit(1))
            .await
            .into_iter()
            .next()
    }

    /// Diff two snapshots.
    pub async fn diff(&self, base_id: Uuid, target_id: Uuid) -> Result<SnapshotDiff> {
        let snapshots = self.snapshots.read().await;

        let base = snapshots
            .get(&base_id)
            .ok_or_else(|| SnapshotError::NotFound(base_id.to_string()))?;
        let target = snapshots
            .get(&target_id)
            .ok_or_else(|| SnapshotError::NotFound(target_id.to_string()))?;

        let base_msg_ids: std::collections::HashSet<_> =
            base.state.messages.iter().map(|m| m.id).collect();
        let target_msg_ids: std::collections::HashSet<_> =
            target.state.messages.iter().map(|m| m.id).collect();

        let messages_added: Vec<_> = target
            .state
            .messages
            .iter()
            .filter(|m| !base_msg_ids.contains(&m.id))
            .cloned()
            .collect();

        let messages_removed: Vec<_> = base
            .state
            .messages
            .iter()
            .filter(|m| !target_msg_ids.contains(&m.id))
            .map(|m| m.id)
            .collect();

        let mut context_changes = Vec::new();

        // Find added/modified
        for (key, new_value) in &target.state.context {
            match base.state.context.get(key) {
                Some(old_value) if old_value != new_value => {
                    context_changes.push(ContextChange {
                        key: key.clone(),
                        old_value: Some(old_value.clone()),
                        new_value: Some(new_value.clone()),
                        change_type: ChangeType::Modified,
                    });
                }
                None => {
                    context_changes.push(ContextChange {
                        key: key.clone(),
                        old_value: None,
                        new_value: Some(new_value.clone()),
                        change_type: ChangeType::Added,
                    });
                }
                _ => {}
            }
        }

        // Find removed
        for key in base.state.context.keys() {
            if !target.state.context.contains_key(key) {
                context_changes.push(ContextChange {
                    key: key.clone(),
                    old_value: base.state.context.get(key).cloned(),
                    new_value: None,
                    change_type: ChangeType::Removed,
                });
            }
        }

        let config_changed = base.state.model_config.model_id != target.state.model_config.model_id
            || base.state.model_config.temperature != target.state.model_config.temperature;

        let summary = format!(
            "+{} -{} messages, {} context changes{}",
            messages_added.len(),
            messages_removed.len(),
            context_changes.len(),
            if config_changed {
                ", config changed"
            } else {
                ""
            }
        );

        Ok(SnapshotDiff {
            base_id,
            target_id,
            messages_added,
            messages_removed,
            context_changes,
            config_changed,
            summary,
        })
    }

    /// Create incremental snapshot.
    pub async fn create_incremental(
        &self,
        name: &str,
        session_id: &str,
        user_id: &str,
        state: SessionState,
    ) -> Result<Uuid> {
        let parent = self.latest(session_id).await;

        let mut snapshot = Snapshot::new(name, session_id, user_id, state);
        if let Some(parent_snapshot) = parent {
            snapshot = snapshot.with_parent(parent_snapshot.id);
        }

        self.create(snapshot).await
    }

    /// Restore session from snapshot.
    pub async fn restore(&self, id: Uuid) -> Result<SessionState> {
        let snapshot = self
            .get(id)
            .await
            .ok_or_else(|| SnapshotError::NotFound(id.to_string()))?;

        Ok(snapshot.state)
    }

    /// Get snapshot chain (incremental history).
    pub async fn get_chain(&self, id: Uuid) -> Vec<Snapshot> {
        let mut chain = Vec::new();
        let mut current_id = Some(id);

        while let Some(id) = current_id {
            if let Some(snapshot) = self.get(id).await {
                current_id = snapshot.parent_id;
                chain.push(snapshot);
            } else {
                break;
            }
        }

        chain.reverse();
        chain
    }

    /// Get total snapshot count.
    pub async fn count(&self) -> usize {
        self.snapshots.read().await.len()
    }

    /// Get total storage size.
    pub async fn total_size(&self) -> usize {
        self.snapshots
            .read()
            .await
            .values()
            .map(|s| s.size_bytes)
            .sum()
    }

    /// Clear old snapshots.
    pub async fn cleanup(&self, keep_last: usize, session_id: &str) {
        let to_remove: Vec<_> = self
            .list(SnapshotQuery::new().for_session(session_id))
            .await
            .into_iter()
            .skip(keep_last)
            .map(|s| s.id)
            .collect();

        for id in to_remove {
            self.delete(id).await;
        }
    }
}

impl Default for SnapshotManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_snapshot() {
        let manager = SnapshotManager::new();

        let state = SessionState {
            messages: vec![Message::new("user", "Hello")],
            ..Default::default()
        };

        let snapshot = Snapshot::new("Test Snapshot", "session-1", "user-1", state);
        let id = manager.create(snapshot).await.unwrap();

        let retrieved = manager.get(id).await.unwrap();
        assert_eq!(retrieved.name, "Test Snapshot");
        assert_eq!(retrieved.state.messages.len(), 1);
    }

    #[tokio::test]
    async fn test_list_snapshots() {
        let manager = SnapshotManager::new();

        for i in 0..5 {
            let state = SessionState::default();
            let snapshot = Snapshot::new(&format!("Snapshot {}", i), "session-1", "user-1", state);
            manager.create(snapshot).await.unwrap();
        }

        let all = manager.list(SnapshotQuery::new()).await;
        assert_eq!(all.len(), 5);

        let limited = manager.list(SnapshotQuery::new().limit(3)).await;
        assert_eq!(limited.len(), 3);
    }

    #[tokio::test]
    async fn test_snapshot_diff() {
        let manager = SnapshotManager::new();

        // Create a shared message that will appear in both states
        let shared_msg = Message::new("user", "Hello");
        let new_msg = Message::new("assistant", "Hi there!");

        let state1 = SessionState {
            messages: vec![shared_msg.clone()],
            context: [("key1".to_string(), serde_json::json!("value1"))]
                .into_iter()
                .collect(),
            ..Default::default()
        };

        let state2 = SessionState {
            messages: vec![shared_msg.clone(), new_msg],
            context: [
                ("key1".to_string(), serde_json::json!("modified")),
                ("key2".to_string(), serde_json::json!("new")),
            ]
            .into_iter()
            .collect(),
            ..Default::default()
        };

        let snap1 = Snapshot::new("Snap 1", "s1", "u1", state1);
        let snap2 = Snapshot::new("Snap 2", "s1", "u1", state2);

        let id1 = manager.create(snap1).await.unwrap();
        let id2 = manager.create(snap2).await.unwrap();

        let diff = manager.diff(id1, id2).await.unwrap();
        assert_eq!(diff.messages_added.len(), 1);
        assert_eq!(diff.context_changes.len(), 2);
    }

    #[tokio::test]
    async fn test_incremental_snapshots() {
        let manager = SnapshotManager::new();

        let state1 = SessionState::default();
        let id1 = manager
            .create_incremental("Snap 1", "session-1", "user-1", state1)
            .await
            .unwrap();

        let state2 = SessionState::default();
        let id2 = manager
            .create_incremental("Snap 2", "session-1", "user-1", state2)
            .await
            .unwrap();

        let snap2 = manager.get(id2).await.unwrap();
        assert_eq!(snap2.parent_id, Some(id1));

        let chain = manager.get_chain(id2).await;
        assert_eq!(chain.len(), 2);
    }
}
