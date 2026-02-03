//! Replica and copy management for drbot.
//!
//! This crate provides:
//! - Replica management
//! - Copy-on-write
//! - Versioned copies

use std::sync::Arc;
use thiserror::Error;

/// Replica error types.
#[derive(Error, Debug, Clone)]
pub enum ReplicaError {
    #[error("Replica not found")]
    NotFound,

    #[error("Replica conflict")]
    Conflict,
}

/// Result type for replica operations.
pub type Result<T> = std::result::Result<T, ReplicaError>;

/// A replica of a value.
#[derive(Debug, Clone)]
pub struct Replica<T> {
    id: u64,
    value: T,
    version: u64,
}

impl<T: Clone> Replica<T> {
    /// Create new replica.
    pub fn new(id: u64, value: T) -> Self {
        Self {
            id,
            value,
            version: 1,
        }
    }

    /// Get ID.
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Get version.
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Get value.
    pub fn get(&self) -> &T {
        &self.value
    }

    /// Get mutable value, incrementing version.
    pub fn get_mut(&mut self) -> &mut T {
        self.version += 1;
        &mut self.value
    }

    /// Update value.
    pub fn update(&mut self, value: T) {
        self.value = value;
        self.version += 1;
    }

    /// Fork replica.
    pub fn fork(&self, new_id: u64) -> Self {
        Self {
            id: new_id,
            value: self.value.clone(),
            version: 1,
        }
    }

    /// Check if same version.
    pub fn same_version(&self, other: &Replica<T>) -> bool {
        self.version == other.version
    }
}

/// Replica set for managing multiple copies.
#[derive(Debug)]
pub struct ReplicaSet<T> {
    replicas: Vec<Replica<T>>,
    next_id: u64,
}

impl<T: Clone> ReplicaSet<T> {
    /// Create new set.
    pub fn new() -> Self {
        Self {
            replicas: Vec::new(),
            next_id: 1,
        }
    }

    /// Add replica.
    pub fn add(&mut self, value: T) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.replicas.push(Replica::new(id, value));
        id
    }

    /// Get replica by ID.
    pub fn get(&self, id: u64) -> Option<&Replica<T>> {
        self.replicas.iter().find(|r| r.id == id)
    }

    /// Get mutable replica.
    pub fn get_mut(&mut self, id: u64) -> Option<&mut Replica<T>> {
        self.replicas.iter_mut().find(|r| r.id == id)
    }

    /// Remove replica.
    pub fn remove(&mut self, id: u64) -> Option<Replica<T>> {
        if let Some(pos) = self.replicas.iter().position(|r| r.id == id) {
            Some(self.replicas.remove(pos))
        } else {
            None
        }
    }

    /// Count replicas.
    pub fn count(&self) -> usize {
        self.replicas.len()
    }

    /// Iterator over replicas.
    pub fn iter(&self) -> impl Iterator<Item = &Replica<T>> {
        self.replicas.iter()
    }
}

impl<T: Clone> Default for ReplicaSet<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Copy-on-write value.
#[derive(Debug)]
pub struct Cow<T: Clone> {
    inner: Arc<T>,
}

impl<T: Clone> Cow<T> {
    /// Create new.
    pub fn new(value: T) -> Self {
        Self {
            inner: Arc::new(value),
        }
    }

    /// Get reference.
    pub fn get(&self) -> &T {
        &self.inner
    }

    /// Get mutable, cloning if shared.
    pub fn get_mut(&mut self) -> &mut T {
        Arc::make_mut(&mut self.inner)
    }

    /// Check if unique.
    pub fn is_unique(&self) -> bool {
        Arc::strong_count(&self.inner) == 1
    }

    /// Into inner.
    pub fn into_inner(self) -> T {
        Arc::try_unwrap(self.inner).unwrap_or_else(|arc| (*arc).clone())
    }
}

impl<T: Clone> Clone for Cow<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

/// Versioned value.
#[derive(Debug, Clone)]
pub struct Versioned<T> {
    value: T,
    version: u64,
}

impl<T> Versioned<T> {
    /// Create new.
    pub fn new(value: T) -> Self {
        Self { value, version: 1 }
    }

    /// Get value.
    pub fn get(&self) -> &T {
        &self.value
    }

    /// Get version.
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Update value.
    pub fn update(&mut self, value: T) {
        self.value = value;
        self.version += 1;
    }

    /// Update with function.
    pub fn modify<F: FnOnce(&mut T)>(&mut self, f: F) {
        f(&mut self.value);
        self.version += 1;
    }

    /// Into value.
    pub fn into_inner(self) -> T {
        self.value
    }
}

/// Version history.
#[derive(Debug)]
pub struct VersionHistory<T: Clone> {
    versions: Vec<T>,
    max_versions: usize,
}

impl<T: Clone> VersionHistory<T> {
    /// Create new.
    pub fn new(initial: T, max_versions: usize) -> Self {
        Self {
            versions: vec![initial],
            max_versions,
        }
    }

    /// Get current.
    pub fn current(&self) -> &T {
        self.versions.last().unwrap()
    }

    /// Push new version.
    pub fn push(&mut self, value: T) {
        if self.versions.len() >= self.max_versions {
            self.versions.remove(0);
        }
        self.versions.push(value);
    }

    /// Get version by index.
    pub fn get(&self, index: usize) -> Option<&T> {
        self.versions.get(index)
    }

    /// Version count.
    pub fn version_count(&self) -> usize {
        self.versions.len()
    }

    /// Rollback to version.
    pub fn rollback(&mut self, index: usize) -> bool {
        if index < self.versions.len() {
            self.versions.truncate(index + 1);
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_replica() {
        let mut r = Replica::new(1, "hello".to_string());
        assert_eq!(r.version(), 1);

        r.update("world".to_string());
        assert_eq!(r.version(), 2);
        assert_eq!(r.get(), "world");
    }

    #[test]
    fn test_replica_set() {
        let mut set: ReplicaSet<i32> = ReplicaSet::new();
        let id1 = set.add(42);
        let id2 = set.add(84);

        assert_eq!(set.count(), 2);
        assert_eq!(set.get(id1).unwrap().get(), &42);
        assert_eq!(set.get(id2).unwrap().get(), &84);
    }

    #[test]
    fn test_cow() {
        let mut cow1 = Cow::new(vec![1, 2, 3]);
        let cow2 = cow1.clone();

        assert!(!cow1.is_unique());

        cow1.get_mut().push(4);
        assert_eq!(cow1.get(), &vec![1, 2, 3, 4]);
        assert_eq!(cow2.get(), &vec![1, 2, 3]);
    }

    #[test]
    fn test_versioned() {
        let mut v = Versioned::new(10);
        assert_eq!(v.version(), 1);

        v.update(20);
        assert_eq!(v.version(), 2);
        assert_eq!(*v.get(), 20);
    }

    #[test]
    fn test_version_history() {
        let mut history = VersionHistory::new(1, 5);
        history.push(2);
        history.push(3);

        assert_eq!(*history.current(), 3);
        assert_eq!(history.version_count(), 3);

        history.rollback(1);
        assert_eq!(*history.current(), 2);
    }
}
