//! Flyweight pattern for drbot.
//!
//! This crate provides:
//! - Flyweight factory for sharing objects
//! - Interning for strings and other values
//! - Memory-efficient object storage

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::{Arc, RwLock, Weak};
use thiserror::Error;

/// Flyweight error types.
#[derive(Error, Debug)]
pub enum FlyweightError {
    #[error("Flyweight not found")]
    NotFound,

    #[error("Creation failed")]
    CreationFailed,
}

/// Result type for flyweight operations.
pub type Result<T> = std::result::Result<T, FlyweightError>;

/// Flyweight factory for creating shared objects.
pub struct FlyweightFactory<K, V> {
    flyweights: RwLock<HashMap<K, Arc<V>>>,
}

impl<K: Hash + Eq + Clone, V> FlyweightFactory<K, V> {
    /// Create new factory.
    pub fn new() -> Self {
        Self {
            flyweights: RwLock::new(HashMap::new()),
        }
    }

    /// Get or create flyweight.
    pub fn get_or_create<F>(&self, key: K, creator: F) -> Arc<V>
    where
        F: FnOnce() -> V,
    {
        // Try read first
        {
            let read = self.flyweights.read().unwrap();
            if let Some(flyweight) = read.get(&key) {
                return flyweight.clone();
            }
        }

        // Create new flyweight
        let mut write = self.flyweights.write().unwrap();

        // Double-check after acquiring write lock
        if let Some(flyweight) = write.get(&key) {
            return flyweight.clone();
        }

        let flyweight = Arc::new(creator());
        write.insert(key, flyweight.clone());
        flyweight
    }

    /// Get existing flyweight.
    pub fn get(&self, key: &K) -> Option<Arc<V>> {
        self.flyweights.read().unwrap().get(key).cloned()
    }

    /// Remove flyweight.
    pub fn remove(&self, key: &K) -> Option<Arc<V>> {
        self.flyweights.write().unwrap().remove(key)
    }

    /// Get count.
    pub fn len(&self) -> usize {
        self.flyweights.read().unwrap().len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.flyweights.read().unwrap().is_empty()
    }

    /// Clear all flyweights.
    pub fn clear(&self) {
        self.flyweights.write().unwrap().clear();
    }
}

impl<K: Hash + Eq + Clone, V> Default for FlyweightFactory<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

/// String interner for deduplicating strings.
pub struct StringInterner {
    strings: RwLock<HashMap<String, Arc<str>>>,
}

impl StringInterner {
    /// Create new interner.
    pub fn new() -> Self {
        Self {
            strings: RwLock::new(HashMap::new()),
        }
    }

    /// Intern a string.
    pub fn intern(&self, s: &str) -> Arc<str> {
        // Try read first
        {
            let read = self.strings.read().unwrap();
            if let Some(interned) = read.get(s) {
                return interned.clone();
            }
        }

        // Create new interned string
        let mut write = self.strings.write().unwrap();

        // Double-check
        if let Some(interned) = write.get(s) {
            return interned.clone();
        }

        let interned: Arc<str> = Arc::from(s);
        write.insert(s.to_string(), interned.clone());
        interned
    }

    /// Check if string is interned.
    pub fn contains(&self, s: &str) -> bool {
        self.strings.read().unwrap().contains_key(s)
    }

    /// Get count of interned strings.
    pub fn len(&self) -> usize {
        self.strings.read().unwrap().len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.strings.read().unwrap().is_empty()
    }

    /// Clear all interned strings.
    pub fn clear(&self) {
        self.strings.write().unwrap().clear();
    }
}

impl Default for StringInterner {
    fn default() -> Self {
        Self::new()
    }
}

/// Weak flyweight factory (allows cleanup of unused flyweights).
pub struct WeakFlyweightFactory<K, V> {
    flyweights: RwLock<HashMap<K, Weak<V>>>,
}

impl<K: Hash + Eq + Clone, V> WeakFlyweightFactory<K, V> {
    /// Create new factory.
    pub fn new() -> Self {
        Self {
            flyweights: RwLock::new(HashMap::new()),
        }
    }

    /// Get or create flyweight.
    pub fn get_or_create<F>(&self, key: K, creator: F) -> Arc<V>
    where
        F: FnOnce() -> V,
    {
        // Try to get existing
        {
            let read = self.flyweights.read().unwrap();
            if let Some(weak) = read.get(&key) {
                if let Some(strong) = weak.upgrade() {
                    return strong;
                }
            }
        }

        // Create new
        let mut write = self.flyweights.write().unwrap();

        // Double-check with possible upgrade
        if let Some(weak) = write.get(&key) {
            if let Some(strong) = weak.upgrade() {
                return strong;
            }
        }

        let flyweight = Arc::new(creator());
        write.insert(key, Arc::downgrade(&flyweight));
        flyweight
    }

    /// Get existing flyweight.
    pub fn get(&self, key: &K) -> Option<Arc<V>> {
        self.flyweights
            .read()
            .unwrap()
            .get(key)
            .and_then(|w| w.upgrade())
    }

    /// Cleanup expired weak references.
    pub fn cleanup(&self) -> usize {
        let mut write = self.flyweights.write().unwrap();
        let before = write.len();
        write.retain(|_, weak| weak.strong_count() > 0);
        before - write.len()
    }

    /// Get count (including potentially expired).
    pub fn len(&self) -> usize {
        self.flyweights.read().unwrap().len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.flyweights.read().unwrap().is_empty()
    }
}

impl<K: Hash + Eq + Clone, V> Default for WeakFlyweightFactory<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

/// Flyweight with intrinsic and extrinsic state.
#[derive(Debug, Clone)]
pub struct Flyweight<I, E> {
    /// Shared intrinsic state.
    pub intrinsic: Arc<I>,
    /// Unique extrinsic state.
    pub extrinsic: E,
}

impl<I, E> Flyweight<I, E> {
    /// Create new flyweight.
    pub fn new(intrinsic: Arc<I>, extrinsic: E) -> Self {
        Self {
            intrinsic,
            extrinsic,
        }
    }

    /// Get intrinsic state.
    pub fn intrinsic(&self) -> &I {
        &self.intrinsic
    }

    /// Get extrinsic state.
    pub fn extrinsic(&self) -> &E {
        &self.extrinsic
    }

    /// Get mutable extrinsic state.
    pub fn extrinsic_mut(&mut self) -> &mut E {
        &mut self.extrinsic
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_flyweight_factory() {
        let factory: FlyweightFactory<String, Vec<i32>> = FlyweightFactory::new();

        let data1 = factory.get_or_create("key".to_string(), || vec![1, 2, 3]);
        let data2 = factory.get_or_create("key".to_string(), || vec![4, 5, 6]);

        // Should return same Arc
        assert!(Arc::ptr_eq(&data1, &data2));
        assert_eq!(*data1, vec![1, 2, 3]);
    }

    #[test]
    fn test_string_interner() {
        let interner = StringInterner::new();

        let s1 = interner.intern("hello");
        let s2 = interner.intern("hello");
        let s3 = interner.intern("world");

        // Same string should return same Arc
        assert!(Arc::ptr_eq(&s1, &s2));
        assert!(!Arc::ptr_eq(&s1, &s3));
    }

    #[test]
    fn test_weak_flyweight_factory() {
        let factory: WeakFlyweightFactory<String, Vec<i32>> = WeakFlyweightFactory::new();

        {
            let _data = factory.get_or_create("key".to_string(), || vec![1, 2, 3]);
            assert_eq!(factory.len(), 1);
        }

        // After dropping, cleanup should remove it
        let removed = factory.cleanup();
        assert_eq!(removed, 1);
    }

    #[test]
    fn test_flyweight_state() {
        let intrinsic = Arc::new("shared data".to_string());

        let fw1 = Flyweight::new(intrinsic.clone(), 1);
        let fw2 = Flyweight::new(intrinsic.clone(), 2);

        // Same intrinsic state
        assert!(Arc::ptr_eq(&fw1.intrinsic, &fw2.intrinsic));

        // Different extrinsic state
        assert_ne!(fw1.extrinsic, fw2.extrinsic);
    }
}
