//! Weak reference utilities for drbot.
//!
//! This crate provides:
//! - Weak reference helpers
//! - Weak collections
//! - Upgrade utilities

use std::rc::{Rc, Weak as RcWeak};
use std::sync::{Arc, Weak as ArcWeak};
use thiserror::Error;

/// Weak error types.
#[derive(Error, Debug, Clone)]
pub enum WeakError {
    #[error("Reference dropped")]
    Dropped,
}

/// Result type for weak operations.
pub type Result<T> = std::result::Result<T, WeakError>;

/// Create weak from Rc.
pub fn weak_rc<T>(rc: &Rc<T>) -> RcWeak<T> {
    Rc::downgrade(rc)
}

/// Create weak from Arc.
pub fn weak_arc<T>(arc: &Arc<T>) -> ArcWeak<T> {
    Arc::downgrade(arc)
}

/// Try upgrade Rc weak.
pub fn upgrade_rc<T>(weak: &RcWeak<T>) -> Result<Rc<T>> {
    weak.upgrade().ok_or(WeakError::Dropped)
}

/// Try upgrade Arc weak.
pub fn upgrade_arc<T>(weak: &ArcWeak<T>) -> Result<Arc<T>> {
    weak.upgrade().ok_or(WeakError::Dropped)
}

/// Weak Rc extension trait.
pub trait RcWeakExt<T> {
    /// Try upgrade or return error.
    fn try_upgrade(&self) -> Result<Rc<T>>;

    /// Is alive.
    fn is_alive(&self) -> bool;

    /// With upgraded value.
    fn with<F, R>(&self, f: F) -> Result<R>
    where
        F: FnOnce(&T) -> R;
}

impl<T> RcWeakExt<T> for RcWeak<T> {
    fn try_upgrade(&self) -> Result<Rc<T>> {
        self.upgrade().ok_or(WeakError::Dropped)
    }

    fn is_alive(&self) -> bool {
        self.strong_count() > 0
    }

    fn with<F, R>(&self, f: F) -> Result<R>
    where
        F: FnOnce(&T) -> R,
    {
        self.upgrade().map(|rc| f(&rc)).ok_or(WeakError::Dropped)
    }
}

/// Weak Arc extension trait.
pub trait ArcWeakExt<T> {
    /// Try upgrade or return error.
    fn try_upgrade(&self) -> Result<Arc<T>>;

    /// Is alive.
    fn is_alive(&self) -> bool;

    /// With upgraded value.
    fn with<F, R>(&self, f: F) -> Result<R>
    where
        F: FnOnce(&T) -> R;
}

impl<T> ArcWeakExt<T> for ArcWeak<T> {
    fn try_upgrade(&self) -> Result<Arc<T>> {
        self.upgrade().ok_or(WeakError::Dropped)
    }

    fn is_alive(&self) -> bool {
        self.strong_count() > 0
    }

    fn with<F, R>(&self, f: F) -> Result<R>
    where
        F: FnOnce(&T) -> R,
    {
        self.upgrade().map(|arc| f(&arc)).ok_or(WeakError::Dropped)
    }
}

/// Weak list (local, non-thread-safe).
pub struct WeakList<T> {
    items: Vec<RcWeak<T>>,
}

impl<T> WeakList<T> {
    /// Create new.
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }

    /// Add weak reference.
    pub fn push(&mut self, item: &Rc<T>) {
        self.items.push(Rc::downgrade(item));
    }

    /// Clean up dead references.
    pub fn cleanup(&mut self) {
        self.items.retain(|w| w.strong_count() > 0);
    }

    /// Get all alive items.
    pub fn alive(&self) -> Vec<Rc<T>> {
        self.items.iter().filter_map(|w| w.upgrade()).collect()
    }

    /// Count alive items.
    pub fn alive_count(&self) -> usize {
        self.items.iter().filter(|w| w.strong_count() > 0).count()
    }

    /// Total count (including dead).
    pub fn total_count(&self) -> usize {
        self.items.len()
    }

    /// Is empty.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Clear all.
    pub fn clear(&mut self) {
        self.items.clear();
    }

    /// Iterate alive items.
    pub fn iter_alive(&self) -> impl Iterator<Item = Rc<T>> + '_ {
        self.items.iter().filter_map(|w| w.upgrade())
    }
}

impl<T> Default for WeakList<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Thread-safe weak list.
pub struct SyncWeakList<T> {
    items: std::sync::Mutex<Vec<ArcWeak<T>>>,
}

impl<T> SyncWeakList<T> {
    /// Create new.
    pub fn new() -> Self {
        Self {
            items: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// Add weak reference.
    pub fn push(&self, item: &Arc<T>) {
        let mut items = self.items.lock().unwrap();
        items.push(Arc::downgrade(item));
    }

    /// Clean up dead references.
    pub fn cleanup(&self) {
        let mut items = self.items.lock().unwrap();
        items.retain(|w| w.strong_count() > 0);
    }

    /// Get all alive items.
    pub fn alive(&self) -> Vec<Arc<T>> {
        let items = self.items.lock().unwrap();
        items.iter().filter_map(|w| w.upgrade()).collect()
    }

    /// Count alive items.
    pub fn alive_count(&self) -> usize {
        let items = self.items.lock().unwrap();
        items.iter().filter(|w| w.strong_count() > 0).count()
    }

    /// Clear all.
    pub fn clear(&self) {
        let mut items = self.items.lock().unwrap();
        items.clear();
    }
}

impl<T> Default for SyncWeakList<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Weak map key.
pub struct WeakKey<T> {
    weak: ArcWeak<T>,
    hash: u64,
}

impl<T> WeakKey<T> {
    /// Create from Arc.
    pub fn new(arc: &Arc<T>) -> Self
    where
        T: std::hash::Hash,
    {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        Arc::as_ptr(arc).hash(&mut hasher);
        Self {
            weak: Arc::downgrade(arc),
            hash: hasher.finish(),
        }
    }

    /// Try upgrade.
    pub fn upgrade(&self) -> Option<Arc<T>> {
        self.weak.upgrade()
    }

    /// Is alive.
    pub fn is_alive(&self) -> bool {
        self.weak.strong_count() > 0
    }
}

impl<T> Clone for WeakKey<T> {
    fn clone(&self) -> Self {
        Self {
            weak: self.weak.clone(),
            hash: self.hash,
        }
    }
}

impl<T> std::hash::Hash for WeakKey<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.hash.hash(state);
    }
}

impl<T> PartialEq for WeakKey<T> {
    fn eq(&self, other: &Self) -> bool {
        self.weak.ptr_eq(&other.weak)
    }
}

impl<T> Eq for WeakKey<T> {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_weak_ext() {
        let rc = Rc::new(42);
        let weak = weak_rc(&rc);
        assert!(weak.is_alive());

        let upgraded = weak.try_upgrade().unwrap();
        assert_eq!(*upgraded, 42);

        drop(rc);
        drop(upgraded);
        assert!(!weak.is_alive());
    }

    #[test]
    fn test_weak_list() {
        let mut list = WeakList::new();
        let rc1 = Rc::new(1);
        let rc2 = Rc::new(2);

        list.push(&rc1);
        list.push(&rc2);
        assert_eq!(list.alive_count(), 2);

        drop(rc1);
        assert_eq!(list.alive_count(), 1);

        list.cleanup();
        assert_eq!(list.total_count(), 1);
    }

    #[test]
    fn test_weak_with() {
        let rc = Rc::new(42);
        let weak = Rc::downgrade(&rc);

        let result = weak.with(|v| *v * 2).unwrap();
        assert_eq!(result, 84);

        drop(rc);
        assert!(weak.with(|_| ()).is_err());
    }

    #[test]
    fn test_sync_weak_list() {
        let list = SyncWeakList::new();
        let arc = Arc::new(42);

        list.push(&arc);
        assert_eq!(list.alive_count(), 1);

        drop(arc);
        assert_eq!(list.alive_count(), 0);
    }
}
