//! Type-keyed map for drbot.
//!
//! This crate provides:
//! - Map keyed by type
//! - Type-safe value storage
//! - Extension traits

use std::any::{Any, TypeId};
use std::collections::HashMap;
use thiserror::Error;

/// Type map error types.
#[derive(Error, Debug, Clone)]
pub enum TypeMapError {
    #[error("No value for type: {0}")]
    NotFound(String),

    #[error("Type mismatch")]
    TypeMismatch,
}

/// Result type for type map operations.
pub type Result<T> = std::result::Result<T, TypeMapError>;

/// Map keyed by TypeId.
pub struct TypeMap {
    map: HashMap<TypeId, Box<dyn Any + Send + Sync>>,
}

impl TypeMap {
    /// Create new type map.
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }

    /// Insert value.
    pub fn insert<T: Send + Sync + 'static>(&mut self, value: T) -> Option<T> {
        self.map
            .insert(TypeId::of::<T>(), Box::new(value))
            .and_then(|old| old.downcast().ok().map(|b| *b))
    }

    /// Get reference to value.
    pub fn get<T: 'static>(&self) -> Option<&T> {
        self.map
            .get(&TypeId::of::<T>())
            .and_then(|v| v.downcast_ref())
    }

    /// Get mutable reference to value.
    pub fn get_mut<T: 'static>(&mut self) -> Option<&mut T> {
        self.map
            .get_mut(&TypeId::of::<T>())
            .and_then(|v| v.downcast_mut())
    }

    /// Remove value.
    pub fn remove<T: 'static>(&mut self) -> Option<T> {
        self.map
            .remove(&TypeId::of::<T>())
            .and_then(|v| v.downcast().ok().map(|b| *b))
    }

    /// Check if contains type.
    pub fn contains<T: 'static>(&self) -> bool {
        self.map.contains_key(&TypeId::of::<T>())
    }

    /// Get or insert default.
    pub fn get_or_insert<T: Default + Send + Sync + 'static>(&mut self) -> &mut T {
        self.get_or_insert_with(T::default)
    }

    /// Get or insert with function.
    pub fn get_or_insert_with<T: Send + Sync + 'static, F: FnOnce() -> T>(
        &mut self,
        f: F,
    ) -> &mut T {
        let entry = self
            .map
            .entry(TypeId::of::<T>())
            .or_insert_with(|| Box::new(f()));
        entry.downcast_mut().unwrap()
    }

    /// Get length.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Clear all values.
    pub fn clear(&mut self) {
        self.map.clear();
    }
}

impl Default for TypeMap {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for TypeMap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TypeMap")
            .field("len", &self.map.len())
            .finish()
    }
}

/// Cloneable type map entry.
struct CloneableEntry {
    value: Box<dyn Any + Send + Sync>,
    clone_fn: fn(&Box<dyn Any + Send + Sync>) -> Box<dyn Any + Send + Sync>,
}

fn make_clone_fn<T: Clone + Send + Sync + 'static>(
) -> fn(&Box<dyn Any + Send + Sync>) -> Box<dyn Any + Send + Sync> {
    |boxed| {
        let value = boxed.downcast_ref::<T>().unwrap();
        Box::new(value.clone())
    }
}

/// Cloneable type map (requires Clone on values).
pub struct CloneableTypeMap {
    map: HashMap<TypeId, CloneableEntry>,
}

impl CloneableTypeMap {
    /// Create new cloneable type map.
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }

    /// Insert value.
    pub fn insert<T: Clone + Send + Sync + 'static>(&mut self, value: T) {
        self.map.insert(
            TypeId::of::<T>(),
            CloneableEntry {
                value: Box::new(value),
                clone_fn: make_clone_fn::<T>(),
            },
        );
    }

    /// Get reference.
    pub fn get<T: 'static>(&self) -> Option<&T> {
        self.map
            .get(&TypeId::of::<T>())
            .and_then(|entry| entry.value.downcast_ref())
    }

    /// Get mutable reference.
    pub fn get_mut<T: 'static>(&mut self) -> Option<&mut T> {
        self.map
            .get_mut(&TypeId::of::<T>())
            .and_then(|entry| entry.value.downcast_mut())
    }

    /// Check if contains.
    pub fn contains<T: 'static>(&self) -> bool {
        self.map.contains_key(&TypeId::of::<T>())
    }

    /// Get length.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

impl Clone for CloneableTypeMap {
    fn clone(&self) -> Self {
        Self {
            map: self
                .map
                .iter()
                .map(|(k, entry)| {
                    let cloned_value = (entry.clone_fn)(&entry.value);
                    (
                        *k,
                        CloneableEntry {
                            value: cloned_value,
                            clone_fn: entry.clone_fn,
                        },
                    )
                })
                .collect(),
        }
    }
}

impl Default for CloneableTypeMap {
    fn default() -> Self {
        Self::new()
    }
}

/// Extension map - stores one value per type as extensions.
pub struct Extensions {
    inner: TypeMap,
}

impl Extensions {
    /// Create new extensions.
    pub fn new() -> Self {
        Self {
            inner: TypeMap::new(),
        }
    }

    /// Insert extension.
    pub fn insert<T: Send + Sync + 'static>(&mut self, value: T) {
        self.inner.insert(value);
    }

    /// Get extension.
    pub fn get<T: 'static>(&self) -> Option<&T> {
        self.inner.get()
    }

    /// Get mutable extension.
    pub fn get_mut<T: 'static>(&mut self) -> Option<&mut T> {
        self.inner.get_mut()
    }

    /// Remove extension.
    pub fn remove<T: 'static>(&mut self) -> Option<T> {
        self.inner.remove()
    }

    /// Get or insert.
    pub fn get_or_insert<T: Default + Send + Sync + 'static>(&mut self) -> &mut T {
        self.inner.get_or_insert()
    }

    /// Check if has extension.
    pub fn has<T: 'static>(&self) -> bool {
        self.inner.contains::<T>()
    }
}

impl Default for Extensions {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_map_insert_get() {
        let mut map = TypeMap::new();

        map.insert(42i32);
        map.insert("hello".to_string());
        map.insert(3.14f64);

        assert_eq!(map.get::<i32>(), Some(&42));
        assert_eq!(map.get::<String>(), Some(&"hello".to_string()));
        assert_eq!(map.get::<f64>(), Some(&3.14));
        assert_eq!(map.get::<u64>(), None);
    }

    #[test]
    fn test_type_map_remove() {
        let mut map = TypeMap::new();
        map.insert(42i32);

        let removed = map.remove::<i32>();
        assert_eq!(removed, Some(42));
        assert!(!map.contains::<i32>());
    }

    #[test]
    fn test_type_map_get_or_insert() {
        let mut map = TypeMap::new();

        let value = map.get_or_insert::<Vec<i32>>();
        value.push(1);
        value.push(2);

        assert_eq!(map.get::<Vec<i32>>(), Some(&vec![1, 2]));
    }

    #[test]
    fn test_cloneable_type_map() {
        let mut map = CloneableTypeMap::new();
        map.insert(42i32);
        map.insert("hello".to_string());

        let cloned = map.clone();
        assert_eq!(cloned.get::<i32>(), Some(&42));
        assert_eq!(cloned.get::<String>(), Some(&"hello".to_string()));
    }

    #[test]
    fn test_extensions() {
        let mut ext = Extensions::new();

        ext.insert(42i32);
        assert!(ext.has::<i32>());
        assert_eq!(ext.get::<i32>(), Some(&42));

        ext.remove::<i32>();
        assert!(!ext.has::<i32>());
    }
}
