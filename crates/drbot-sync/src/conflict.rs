//! Conflict resolution for sync.

use crate::SyncItem;
use serde::{Deserialize, Serialize};

/// Merge strategy for conflicts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MergeStrategy {
    /// Last write wins.
    LastWriteWins,
    /// First write wins (pessimistic).
    FirstWriteWins,
    /// Keep both versions.
    KeepBoth,
    /// Manual resolution required.
    Manual,
    /// Custom merge function.
    Custom,
}

impl Default for MergeStrategy {
    fn default() -> Self {
        Self::LastWriteWins
    }
}

/// Result of conflict resolution.
#[derive(Debug, Clone)]
pub enum ConflictResolution {
    /// Use local version.
    UseLocal,
    /// Use remote version.
    UseRemote,
    /// Use merged version.
    Merged(SyncItem),
    /// Keep both versions.
    Both(SyncItem, SyncItem),
    /// Requires manual resolution.
    Manual,
}

/// Conflict resolver.
pub struct ConflictResolver {
    strategy: MergeStrategy,
}

impl ConflictResolver {
    /// Create a new resolver.
    pub fn new(strategy: MergeStrategy) -> Self {
        Self { strategy }
    }

    /// Resolve a conflict between local and remote items.
    pub fn resolve(&self, local: &SyncItem, remote: &SyncItem) -> ConflictResolution {
        match self.strategy {
            MergeStrategy::LastWriteWins => {
                if local.modified_at >= remote.modified_at {
                    ConflictResolution::UseLocal
                } else {
                    ConflictResolution::UseRemote
                }
            }
            MergeStrategy::FirstWriteWins => {
                if local.modified_at <= remote.modified_at {
                    ConflictResolution::UseLocal
                } else {
                    ConflictResolution::UseRemote
                }
            }
            MergeStrategy::KeepBoth => ConflictResolution::Both(local.clone(), remote.clone()),
            MergeStrategy::Manual => ConflictResolution::Manual,
            MergeStrategy::Custom => {
                // Custom merge would be implemented here
                // For now, fall back to last write wins
                if local.modified_at >= remote.modified_at {
                    ConflictResolution::UseLocal
                } else {
                    ConflictResolution::UseRemote
                }
            }
        }
    }

    /// Check if items are in conflict.
    pub fn is_conflict(&self, local: &SyncItem, remote: &SyncItem) -> bool {
        // Same ID but different versions from different devices
        local.id == remote.id
            && local.version != remote.version
            && local.modified_by != remote.modified_by
    }

    /// Try to merge JSON objects.
    pub fn merge_json(
        &self,
        local: &serde_json::Value,
        remote: &serde_json::Value,
    ) -> serde_json::Value {
        match (local, remote) {
            (serde_json::Value::Object(l), serde_json::Value::Object(r)) => {
                let mut merged = l.clone();
                for (key, value) in r {
                    if let Some(local_value) = merged.get(key) {
                        // Recursively merge if both are objects
                        merged.insert(key.clone(), self.merge_json(local_value, value));
                    } else {
                        merged.insert(key.clone(), value.clone());
                    }
                }
                serde_json::Value::Object(merged)
            }
            // For non-objects, use strategy
            _ => match self.strategy {
                MergeStrategy::LastWriteWins | MergeStrategy::Custom => remote.clone(),
                MergeStrategy::FirstWriteWins => local.clone(),
                _ => remote.clone(),
            },
        }
    }
}

impl Default for ConflictResolver {
    fn default() -> Self {
        Self::new(MergeStrategy::LastWriteWins)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    fn test_item(version: u64) -> SyncItem {
        SyncItem {
            id: Uuid::new_v4(),
            item_type: crate::sync::SyncItemType::Session,
            data: serde_json::json!({}),
            version,
            modified_at: Utc::now(),
            modified_by: Uuid::new_v4(),
            deleted: false,
        }
    }

    #[test]
    fn test_last_write_wins() {
        let resolver = ConflictResolver::new(MergeStrategy::LastWriteWins);

        let mut local = test_item(1);
        let mut remote = test_item(2);

        // Make remote newer
        remote.modified_at = Utc::now() + chrono::Duration::seconds(1);
        local.id = remote.id; // Same item

        let resolution = resolver.resolve(&local, &remote);
        assert!(matches!(resolution, ConflictResolution::UseRemote));
    }

    #[test]
    fn test_keep_both() {
        let resolver = ConflictResolver::new(MergeStrategy::KeepBoth);

        let local = test_item(1);
        let remote = test_item(2);

        let resolution = resolver.resolve(&local, &remote);
        assert!(matches!(resolution, ConflictResolution::Both(_, _)));
    }

    #[test]
    fn test_merge_json() {
        let resolver = ConflictResolver::default();

        let local = serde_json::json!({
            "name": "Local",
            "shared": "from_local",
            "local_only": true
        });

        let remote = serde_json::json!({
            "name": "Remote",
            "shared": "from_remote",
            "remote_only": true
        });

        let merged = resolver.merge_json(&local, &remote);

        // Remote wins for conflicts, but local_only is preserved
        assert_eq!(merged["name"], "Remote");
        assert_eq!(merged["shared"], "from_remote");
        assert_eq!(merged["local_only"], true);
        assert_eq!(merged["remote_only"], true);
    }

    #[test]
    fn test_is_conflict() {
        let resolver = ConflictResolver::default();

        let mut local = test_item(1);
        let mut remote = test_item(2);
        remote.id = local.id; // Same ID

        assert!(resolver.is_conflict(&local, &remote));

        // Same device, not a conflict
        remote.modified_by = local.modified_by;
        assert!(!resolver.is_conflict(&local, &remote));
    }
}
