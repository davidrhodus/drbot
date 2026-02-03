//! Snapshot utilities for drbot.
//!
//! This crate provides:
//! - Value snapshots
//! - Snapshot comparison
//! - Snapshot restoration

use thiserror::Error;

/// Snapshot error types.
#[derive(Error, Debug, Clone)]
pub enum SnapshotError {
    #[error("Snapshot not found")]
    NotFound,

    #[error("Invalid snapshot")]
    Invalid,

    #[error("Restore failed: {0}")]
    RestoreFailed(String),
}

/// Result type for snapshot operations.
pub type Result<T> = std::result::Result<T, SnapshotError>;

/// A snapshot of a value.
#[derive(Debug, Clone)]
pub struct Snapshot<T> {
    id: u64,
    value: T,
    timestamp: u64,
    label: Option<String>,
}

impl<T: Clone> Snapshot<T> {
    /// Create new snapshot.
    pub fn new(id: u64, value: T, timestamp: u64) -> Self {
        Self {
            id,
            value,
            timestamp,
            label: None,
        }
    }

    /// Create with label.
    pub fn with_label(id: u64, value: T, timestamp: u64, label: &str) -> Self {
        Self {
            id,
            value,
            timestamp,
            label: Some(label.to_string()),
        }
    }

    /// Get ID.
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Get timestamp.
    pub fn timestamp(&self) -> u64 {
        self.timestamp
    }

    /// Get label.
    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    /// Get value reference.
    pub fn value(&self) -> &T {
        &self.value
    }

    /// Restore value.
    pub fn restore(&self) -> T {
        self.value.clone()
    }
}

/// Snapshotable trait.
pub trait Snapshotable: Clone {
    /// Take snapshot.
    fn snapshot(&self) -> Self {
        self.clone()
    }

    /// Restore from snapshot.
    fn restore_from(&mut self, snapshot: &Self) {
        *self = snapshot.clone();
    }
}

impl<T: Clone> Snapshotable for T {}

/// Snapshot manager.
#[derive(Debug)]
pub struct SnapshotManager<T: Clone> {
    snapshots: Vec<Snapshot<T>>,
    next_id: u64,
    max_snapshots: Option<usize>,
}

impl<T: Clone> SnapshotManager<T> {
    /// Create new manager.
    pub fn new() -> Self {
        Self {
            snapshots: Vec::new(),
            next_id: 1,
            max_snapshots: None,
        }
    }

    /// Create with max snapshots.
    pub fn with_max(max: usize) -> Self {
        Self {
            snapshots: Vec::new(),
            next_id: 1,
            max_snapshots: Some(max),
        }
    }

    /// Take snapshot.
    pub fn take(&mut self, value: &T, timestamp: u64) -> u64 {
        let id = self.next_id;
        self.next_id += 1;

        if let Some(max) = self.max_snapshots {
            if self.snapshots.len() >= max {
                self.snapshots.remove(0);
            }
        }

        self.snapshots
            .push(Snapshot::new(id, value.clone(), timestamp));
        id
    }

    /// Take labeled snapshot.
    pub fn take_labeled(&mut self, value: &T, timestamp: u64, label: &str) -> u64 {
        let id = self.next_id;
        self.next_id += 1;

        if let Some(max) = self.max_snapshots {
            if self.snapshots.len() >= max {
                self.snapshots.remove(0);
            }
        }

        self.snapshots
            .push(Snapshot::with_label(id, value.clone(), timestamp, label));
        id
    }

    /// Get snapshot by ID.
    pub fn get(&self, id: u64) -> Option<&Snapshot<T>> {
        self.snapshots.iter().find(|s| s.id == id)
    }

    /// Get latest snapshot.
    pub fn latest(&self) -> Option<&Snapshot<T>> {
        self.snapshots.last()
    }

    /// Get snapshot by label.
    pub fn get_by_label(&self, label: &str) -> Option<&Snapshot<T>> {
        self.snapshots
            .iter()
            .find(|s| s.label.as_deref() == Some(label))
    }

    /// Restore from snapshot ID.
    pub fn restore(&self, id: u64) -> Result<T> {
        self.get(id)
            .map(|s| s.restore())
            .ok_or(SnapshotError::NotFound)
    }

    /// Delete snapshot.
    pub fn delete(&mut self, id: u64) -> bool {
        if let Some(pos) = self.snapshots.iter().position(|s| s.id == id) {
            self.snapshots.remove(pos);
            true
        } else {
            false
        }
    }

    /// Clear all snapshots.
    pub fn clear(&mut self) {
        self.snapshots.clear();
    }

    /// Count snapshots.
    pub fn count(&self) -> usize {
        self.snapshots.len()
    }

    /// List all snapshots.
    pub fn list(&self) -> &[Snapshot<T>] {
        &self.snapshots
    }
}

impl<T: Clone> Default for SnapshotManager<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Diff between snapshots.
#[derive(Debug, Clone)]
pub struct SnapshotDiff<T> {
    /// Before snapshot.
    pub before: T,
    /// After snapshot.
    pub after: T,
}

impl<T: Clone + PartialEq> SnapshotDiff<T> {
    /// Create diff.
    pub fn new(before: T, after: T) -> Self {
        Self { before, after }
    }

    /// Check if changed.
    pub fn changed(&self) -> bool {
        self.before != self.after
    }
}

/// Auto-snapshot wrapper.
#[derive(Debug)]
pub struct AutoSnapshot<T: Clone> {
    current: T,
    snapshots: Vec<T>,
    max_snapshots: usize,
}

impl<T: Clone> AutoSnapshot<T> {
    /// Create new.
    pub fn new(value: T, max_snapshots: usize) -> Self {
        Self {
            current: value,
            snapshots: Vec::new(),
            max_snapshots,
        }
    }

    /// Get current value.
    pub fn get(&self) -> &T {
        &self.current
    }

    /// Update value, automatically creating snapshot.
    pub fn update(&mut self, value: T) {
        if self.snapshots.len() >= self.max_snapshots {
            self.snapshots.remove(0);
        }
        self.snapshots.push(self.current.clone());
        self.current = value;
    }

    /// Undo to previous snapshot.
    pub fn undo(&mut self) -> bool {
        if let Some(prev) = self.snapshots.pop() {
            self.current = prev;
            true
        } else {
            false
        }
    }

    /// Get snapshot count.
    pub fn snapshot_count(&self) -> usize {
        self.snapshots.len()
    }

    /// Can undo.
    pub fn can_undo(&self) -> bool {
        !self.snapshots.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snapshot() {
        let s = Snapshot::new(1, "hello".to_string(), 1000);
        assert_eq!(s.id(), 1);
        assert_eq!(s.value(), "hello");
        assert_eq!(s.restore(), "hello");
    }

    #[test]
    fn test_manager() {
        let mut manager: SnapshotManager<i32> = SnapshotManager::new();
        let id1 = manager.take(&42, 1000);
        let id2 = manager.take(&84, 2000);

        assert_eq!(manager.count(), 2);
        assert_eq!(manager.restore(id1).unwrap(), 42);
        assert_eq!(manager.restore(id2).unwrap(), 84);
    }

    #[test]
    fn test_labeled() {
        let mut manager: SnapshotManager<String> = SnapshotManager::new();
        manager.take_labeled(&"v1".to_string(), 1000, "version1");

        let s = manager.get_by_label("version1").unwrap();
        assert_eq!(s.value(), "v1");
    }

    #[test]
    fn test_auto_snapshot() {
        let mut auto = AutoSnapshot::new(1, 5);
        auto.update(2);
        auto.update(3);

        assert_eq!(*auto.get(), 3);
        assert!(auto.undo());
        assert_eq!(*auto.get(), 2);
    }
}
