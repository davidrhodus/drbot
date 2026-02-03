//! Prototype pattern utilities for drbot.
//!
//! This crate provides:
//! - Prototype trait for cloning objects
//! - Prototype registry
//! - Deep clone utilities

use std::collections::HashMap;
use thiserror::Error;

/// Prototype error types.
#[derive(Error, Debug)]
pub enum PrototypeError {
    #[error("Prototype not found: {0}")]
    NotFound(String),

    #[error("Clone failed: {0}")]
    CloneFailed(String),
}

/// Result type for prototype operations.
pub type Result<T> = std::result::Result<T, PrototypeError>;

/// Prototype trait for objects that can be cloned.
pub trait Prototype: Send + Sync {
    /// Clone this prototype.
    fn clone_prototype(&self) -> Box<dyn Prototype>;
}

// Blanket implementation for Clone + Send + Sync types
impl<T: Clone + Send + Sync + 'static> Prototype for T {
    fn clone_prototype(&self) -> Box<dyn Prototype> {
        Box::new(self.clone())
    }
}

/// Prototype registry for managing prototypes.
pub struct PrototypeRegistry<T: Clone> {
    prototypes: HashMap<String, T>,
}

impl<T: Clone> PrototypeRegistry<T> {
    /// Create new registry.
    pub fn new() -> Self {
        Self {
            prototypes: HashMap::new(),
        }
    }

    /// Register a prototype.
    pub fn register(&mut self, name: impl Into<String>, prototype: T) {
        self.prototypes.insert(name.into(), prototype);
    }

    /// Get prototype reference.
    pub fn get(&self, name: &str) -> Option<&T> {
        self.prototypes.get(name)
    }

    /// Clone a prototype.
    pub fn clone_prototype(&self, name: &str) -> Result<T> {
        self.prototypes
            .get(name)
            .cloned()
            .ok_or_else(|| PrototypeError::NotFound(name.to_string()))
    }

    /// Remove prototype.
    pub fn remove(&mut self, name: &str) -> Option<T> {
        self.prototypes.remove(name)
    }

    /// List prototype names.
    pub fn names(&self) -> Vec<&str> {
        self.prototypes.keys().map(|s| s.as_str()).collect()
    }

    /// Check if prototype exists.
    pub fn contains(&self, name: &str) -> bool {
        self.prototypes.contains_key(name)
    }

    /// Get count.
    pub fn len(&self) -> usize {
        self.prototypes.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.prototypes.is_empty()
    }
}

impl<T: Clone> Default for PrototypeRegistry<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Deep clone trait for types needing special clone logic.
pub trait DeepClone {
    /// Perform deep clone.
    fn deep_clone(&self) -> Self;
}

// Default implementation for Clone types
impl<T: Clone> DeepClone for T {
    fn deep_clone(&self) -> Self {
        self.clone()
    }
}

/// Prototype with customization support.
#[derive(Debug, Clone)]
pub struct CustomizablePrototype<T: Clone, C: Clone> {
    base: T,
    customizations: Vec<C>,
}

impl<T: Clone, C: Clone> CustomizablePrototype<T, C> {
    /// Create new customizable prototype.
    pub fn new(base: T) -> Self {
        Self {
            base,
            customizations: Vec::new(),
        }
    }

    /// Add customization.
    pub fn customize(&mut self, customization: C) {
        self.customizations.push(customization);
    }

    /// Get base prototype.
    pub fn base(&self) -> &T {
        &self.base
    }

    /// Get customizations.
    pub fn customizations(&self) -> &[C] {
        &self.customizations
    }

    /// Clone with additional customization.
    pub fn clone_with(&self, customization: C) -> Self {
        let mut cloned = self.clone();
        cloned.customize(customization);
        cloned
    }
}

/// Lazy prototype that creates on first access.
pub struct LazyPrototype<T> {
    creator: Box<dyn Fn() -> T + Send + Sync>,
    cached: std::sync::RwLock<Option<T>>,
}

impl<T: Clone + Send + Sync> LazyPrototype<T> {
    /// Create new lazy prototype.
    pub fn new<F>(creator: F) -> Self
    where
        F: Fn() -> T + Send + Sync + 'static,
    {
        Self {
            creator: Box::new(creator),
            cached: std::sync::RwLock::new(None),
        }
    }

    /// Get or create the prototype.
    pub fn get(&self) -> T {
        {
            let read = self.cached.read().unwrap();
            if let Some(ref value) = *read {
                return value.clone();
            }
        }

        let mut write = self.cached.write().unwrap();
        if write.is_none() {
            *write = Some((self.creator)());
        }
        write.as_ref().unwrap().clone()
    }

    /// Clone the prototype.
    pub fn clone_instance(&self) -> T {
        self.get()
    }

    /// Reset cached prototype.
    pub fn reset(&self) {
        let mut write = self.cached.write().unwrap();
        *write = None;
    }
}

/// Versioned prototype with history.
#[derive(Debug, Clone)]
pub struct VersionedPrototype<T: Clone> {
    current: T,
    history: Vec<T>,
    max_history: usize,
}

impl<T: Clone> VersionedPrototype<T> {
    /// Create new versioned prototype.
    pub fn new(initial: T, max_history: usize) -> Self {
        Self {
            current: initial,
            history: Vec::new(),
            max_history,
        }
    }

    /// Get current version.
    pub fn current(&self) -> &T {
        &self.current
    }

    /// Update to new version.
    pub fn update(&mut self, new_value: T) {
        let old = std::mem::replace(&mut self.current, new_value);
        self.history.push(old);
        while self.history.len() > self.max_history {
            self.history.remove(0);
        }
    }

    /// Clone current version.
    pub fn clone_current(&self) -> T {
        self.current.clone()
    }

    /// Get version from history.
    pub fn get_version(&self, index: usize) -> Option<&T> {
        self.history.get(index)
    }

    /// Clone version from history.
    pub fn clone_version(&self, index: usize) -> Option<T> {
        self.history.get(index).cloned()
    }

    /// Get history length.
    pub fn history_len(&self) -> usize {
        self.history.len()
    }

    /// Revert to previous version.
    pub fn revert(&mut self) -> bool {
        if let Some(prev) = self.history.pop() {
            self.current = prev;
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
    fn test_prototype_registry() {
        let mut registry = PrototypeRegistry::new();

        registry.register(
            "default_user",
            User {
                name: "Default".to_string(),
                age: 0,
            },
        );

        let user = registry.clone_prototype("default_user").unwrap();
        assert_eq!(user.name, "Default");
        assert_eq!(user.age, 0);
    }

    #[derive(Clone, Debug, PartialEq)]
    struct User {
        name: String,
        age: u32,
    }

    #[test]
    fn test_customizable_prototype() {
        let mut proto = CustomizablePrototype::new(vec![1, 2, 3]);
        proto.customize("add_four");

        let cloned = proto.clone_with("add_five");
        assert_eq!(cloned.customizations().len(), 2);
    }

    #[test]
    fn test_versioned_prototype() {
        let mut proto = VersionedPrototype::new("v1".to_string(), 3);

        proto.update("v2".to_string());
        proto.update("v3".to_string());

        assert_eq!(proto.current(), "v3");
        assert_eq!(proto.history_len(), 2);

        proto.revert();
        assert_eq!(proto.current(), "v2");
    }

    #[test]
    fn test_lazy_prototype() {
        let counter = std::sync::Arc::new(std::sync::atomic::AtomicI32::new(0));
        let counter_clone = counter.clone();

        let lazy = LazyPrototype::new(move || {
            counter_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            vec![1, 2, 3]
        });

        let a = lazy.get();
        let b = lazy.get();

        assert_eq!(a, b);
        assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 1);
    }
}
