//! Scoped resources for drbot.
//!
//! This crate provides:
//! - Scoped values
//! - Context propagation
//! - Scope guards

use std::cell::RefCell;
use std::sync::{Arc, RwLock};
use thiserror::Error;

/// Scope error types.
#[derive(Error, Debug, Clone)]
pub enum ScopeError {
    #[error("No active scope")]
    NoActiveScope,

    #[error("Scope already exists")]
    AlreadyExists,

    #[error("Value not found in scope")]
    NotFound,
}

/// Result type for scope operations.
pub type Result<T> = std::result::Result<T, ScopeError>;

/// Thread-local scoped value.
pub struct ScopedValue<T: 'static> {
    default: Option<T>,
    _marker: std::marker::PhantomData<T>,
}

impl<T: Clone + 'static> ScopedValue<T> {
    /// Create new scoped value with no default.
    pub const fn new() -> Self {
        Self {
            default: None,
            _marker: std::marker::PhantomData,
        }
    }

    /// Create with default value.
    pub const fn with_default(default: T) -> Self {
        Self {
            default: Some(default),
            _marker: std::marker::PhantomData,
        }
    }
}

thread_local! {
    static SCOPE_STACK: RefCell<Vec<Box<dyn std::any::Any>>> = const { RefCell::new(Vec::new()) };
}

/// Scope guard that restores previous value on drop.
pub struct ScopeGuard<T: 'static> {
    previous: Option<T>,
    _marker: std::marker::PhantomData<T>,
}

impl<T: 'static> Drop for ScopeGuard<T> {
    fn drop(&mut self) {
        SCOPE_STACK.with(|stack| {
            stack.borrow_mut().pop();
        });
    }
}

/// Enter scope with value.
pub fn enter_scope<T: Clone + 'static>(value: T) -> ScopeGuard<T> {
    SCOPE_STACK.with(|stack| {
        stack.borrow_mut().push(Box::new(value));
    });
    ScopeGuard {
        previous: None,
        _marker: std::marker::PhantomData,
    }
}

/// Get current scope value.
pub fn get_scope<T: Clone + 'static>() -> Option<T> {
    SCOPE_STACK.with(|stack| {
        let stack = stack.borrow();
        for item in stack.iter().rev() {
            if let Some(value) = item.downcast_ref::<T>() {
                return Some(value.clone());
            }
        }
        None
    })
}

/// Execute function within scope.
pub fn with_scope<T: Clone + 'static, R, F>(value: T, f: F) -> R
where
    F: FnOnce() -> R,
{
    let _guard = enter_scope(value);
    f()
}

/// Named scope for hierarchical contexts.
pub struct NamedScope {
    values: RwLock<std::collections::HashMap<String, Box<dyn std::any::Any + Send + Sync>>>,
    parent: Option<Arc<NamedScope>>,
}

impl NamedScope {
    /// Create new root scope.
    pub fn new() -> Self {
        Self {
            values: RwLock::new(std::collections::HashMap::new()),
            parent: None,
        }
    }

    /// Create child scope.
    pub fn child(parent: Arc<NamedScope>) -> Self {
        Self {
            values: RwLock::new(std::collections::HashMap::new()),
            parent: Some(parent),
        }
    }

    /// Set value in scope.
    pub fn set<T: Send + Sync + 'static>(&self, key: impl Into<String>, value: T) {
        let mut values = self.values.write().unwrap();
        values.insert(key.into(), Box::new(value));
    }

    /// Get value from scope (checks parents).
    pub fn get<T: Clone + Send + Sync + 'static>(&self, key: &str) -> Option<T> {
        // Check current scope
        {
            let values = self.values.read().unwrap();
            if let Some(value) = values.get(key) {
                if let Some(v) = value.downcast_ref::<T>() {
                    return Some(v.clone());
                }
            }
        }

        // Check parent
        if let Some(ref parent) = self.parent {
            return parent.get(key);
        }

        None
    }

    /// Check if key exists.
    pub fn contains(&self, key: &str) -> bool {
        {
            let values = self.values.read().unwrap();
            if values.contains_key(key) {
                return true;
            }
        }

        if let Some(ref parent) = self.parent {
            return parent.contains(key);
        }

        false
    }

    /// Remove value from current scope.
    pub fn remove(&self, key: &str) -> bool {
        let mut values = self.values.write().unwrap();
        values.remove(key).is_some()
    }
}

impl Default for NamedScope {
    fn default() -> Self {
        Self::new()
    }
}

/// Shared named scope.
pub type SharedScope = Arc<NamedScope>;

/// Create shared scope.
pub fn scope() -> SharedScope {
    Arc::new(NamedScope::new())
}

/// Create child scope.
pub fn child_scope(parent: &SharedScope) -> SharedScope {
    Arc::new(NamedScope::child(parent.clone()))
}

/// Scope builder for creating scopes with multiple values.
pub struct ScopeBuilder {
    values: std::collections::HashMap<String, Box<dyn std::any::Any + Send + Sync>>,
    parent: Option<SharedScope>,
}

impl ScopeBuilder {
    /// Create new builder.
    pub fn new() -> Self {
        Self {
            values: std::collections::HashMap::new(),
            parent: None,
        }
    }

    /// Set parent scope.
    pub fn parent(mut self, parent: SharedScope) -> Self {
        self.parent = Some(parent);
        self
    }

    /// Add value.
    pub fn with<T: Send + Sync + 'static>(mut self, key: impl Into<String>, value: T) -> Self {
        self.values.insert(key.into(), Box::new(value));
        self
    }

    /// Build scope.
    pub fn build(self) -> SharedScope {
        let scope = match self.parent {
            Some(parent) => NamedScope::child(parent),
            None => NamedScope::new(),
        };

        {
            let mut values = scope.values.write().unwrap();
            *values = self.values;
        }

        Arc::new(scope)
    }
}

impl Default for ScopeBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Run function in scope context.
pub fn in_scope<R, F>(scope: &SharedScope, f: F) -> R
where
    F: FnOnce(&SharedScope) -> R,
{
    f(scope)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scoped_value() {
        assert!(get_scope::<i32>().is_none());

        let result = with_scope(42, || get_scope::<i32>());

        assert_eq!(result, Some(42));
        assert!(get_scope::<i32>().is_none());
    }

    #[test]
    fn test_nested_scopes() {
        let result = with_scope(1, || {
            let outer = get_scope::<i32>();
            let inner = with_scope(2, || get_scope::<i32>());
            (outer, inner)
        });

        assert_eq!(result, (Some(1), Some(2)));
    }

    #[test]
    fn test_named_scope() {
        let scope = scope();
        scope.set("name", "test".to_string());
        scope.set("count", 42i32);

        assert_eq!(scope.get::<String>("name"), Some("test".to_string()));
        assert_eq!(scope.get::<i32>("count"), Some(42));
        assert!(scope.get::<i32>("missing").is_none());
    }

    #[test]
    fn test_scope_inheritance() {
        let parent = scope();
        parent.set("parent_val", 1);

        let child = child_scope(&parent);
        child.set("child_val", 2);

        assert_eq!(child.get::<i32>("parent_val"), Some(1));
        assert_eq!(child.get::<i32>("child_val"), Some(2));
        assert!(parent.get::<i32>("child_val").is_none());
    }

    #[test]
    fn test_scope_builder() {
        let scope = ScopeBuilder::new()
            .with("a", 1)
            .with("b", "hello".to_string())
            .build();

        assert_eq!(scope.get::<i32>("a"), Some(1));
        assert_eq!(scope.get::<String>("b"), Some("hello".to_string()));
    }
}
