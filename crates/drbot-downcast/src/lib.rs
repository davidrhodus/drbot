//! Downcasting utilities for drbot.
//!
//! This crate provides:
//! - Safe downcast helpers
//! - Downcast chains
//! - Error handling for failed casts

use std::any::{Any, TypeId};
use thiserror::Error;

/// Downcast error types.
#[derive(Error, Debug, Clone)]
pub enum DowncastError {
    #[error("Cannot downcast from {from} to {to}")]
    Failed { from: String, to: String },

    #[error("Null value")]
    Null,
}

/// Result type for downcast operations.
pub type Result<T> = std::result::Result<T, DowncastError>;

/// Downcast trait for trait objects.
pub trait Downcast: Any {
    /// Downcast to concrete type.
    fn downcast_ref<T: Any>(&self) -> Option<&T>;

    /// Downcast to mutable concrete type.
    fn downcast_mut<T: Any>(&mut self) -> Option<&mut T>;

    /// Check if is type.
    fn is<T: Any>(&self) -> bool;

    /// Get type ID.
    fn type_id(&self) -> TypeId;
}

impl<A: Any> Downcast for A {
    fn downcast_ref<T: Any>(&self) -> Option<&T> {
        if TypeId::of::<A>() == TypeId::of::<T>() {
            // Safety: we just checked the type IDs match
            Some(unsafe { &*(self as *const A as *const T) })
        } else {
            None
        }
    }

    fn downcast_mut<T: Any>(&mut self) -> Option<&mut T> {
        if TypeId::of::<A>() == TypeId::of::<T>() {
            // Safety: we just checked the type IDs match
            Some(unsafe { &mut *(self as *mut A as *mut T) })
        } else {
            None
        }
    }

    fn is<T: Any>(&self) -> bool {
        TypeId::of::<A>() == TypeId::of::<T>()
    }

    fn type_id(&self) -> TypeId {
        TypeId::of::<A>()
    }
}

/// Safe downcast from boxed Any.
pub fn downcast_box<T: 'static>(boxed: Box<dyn Any>) -> Result<T> {
    boxed
        .downcast()
        .map(|b| *b)
        .map_err(|_| DowncastError::Failed {
            from: "unknown".to_string(),
            to: std::any::type_name::<T>().to_string(),
        })
}

/// Safe downcast from boxed Any + Send.
pub fn downcast_box_send<T: 'static>(boxed: Box<dyn Any + Send>) -> Result<T> {
    boxed
        .downcast()
        .map(|b| *b)
        .map_err(|_| DowncastError::Failed {
            from: "unknown".to_string(),
            to: std::any::type_name::<T>().to_string(),
        })
}

/// Safe downcast from boxed Any + Send + Sync.
pub fn downcast_box_sync<T: 'static>(boxed: Box<dyn Any + Send + Sync>) -> Result<T> {
    boxed
        .downcast()
        .map(|b| *b)
        .map_err(|_| DowncastError::Failed {
            from: "unknown".to_string(),
            to: std::any::type_name::<T>().to_string(),
        })
}

/// Downcast chain for trying multiple types.
pub struct DowncastChain<'a> {
    value: &'a dyn Any,
}

impl<'a> DowncastChain<'a> {
    /// Create new chain.
    pub fn new(value: &'a dyn Any) -> Self {
        Self { value }
    }

    /// Try to downcast to type.
    pub fn try_as<T: 'static>(&self) -> Option<&'a T> {
        self.value.downcast_ref()
    }

    /// Try type, or continue.
    pub fn or_try<T: 'static, F, R>(&self, f: F) -> Option<R>
    where
        F: FnOnce(&T) -> R,
    {
        self.try_as::<T>().map(f)
    }
}

/// Downcast or default.
pub fn downcast_or<T: 'static + Clone>(value: &dyn Any, default: T) -> T {
    value.downcast_ref::<T>().cloned().unwrap_or(default)
}

/// Downcast or compute default.
pub fn downcast_or_else<T: 'static + Clone, F: FnOnce() -> T>(value: &dyn Any, f: F) -> T {
    value.downcast_ref::<T>().cloned().unwrap_or_else(f)
}

/// Multi-type matcher.
pub struct TypeMatcher<'a, R> {
    value: &'a dyn Any,
    result: Option<R>,
}

impl<'a, R> TypeMatcher<'a, R> {
    /// Create new matcher.
    pub fn new(value: &'a dyn Any) -> Self {
        Self {
            value,
            result: None,
        }
    }

    /// Match type.
    pub fn case<T: 'static, F: FnOnce(&T) -> R>(mut self, f: F) -> Self {
        if self.result.is_none() {
            if let Some(v) = self.value.downcast_ref::<T>() {
                self.result = Some(f(v));
            }
        }
        self
    }

    /// Default case.
    pub fn default<F: FnOnce() -> R>(self, f: F) -> R {
        self.result.unwrap_or_else(f)
    }

    /// Get result.
    pub fn result(self) -> Option<R> {
        self.result
    }
}

/// Match against multiple types.
pub fn type_match<'a, R>(value: &'a dyn Any) -> TypeMatcher<'a, R> {
    TypeMatcher::new(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_downcast_trait() {
        let value = 42i32;

        assert!(value.is::<i32>());
        assert_eq!(value.downcast_ref::<i32>(), Some(&42));
        assert!(value.downcast_ref::<String>().is_none());
    }

    #[test]
    fn test_downcast_box() {
        let boxed: Box<dyn Any> = Box::new(42i32);
        let result = downcast_box::<i32>(boxed);

        assert_eq!(result.ok(), Some(42));
    }

    #[test]
    fn test_downcast_chain() {
        let value: Box<dyn Any> = Box::new(42i32);
        let chain = DowncastChain::new(&*value);

        assert_eq!(chain.try_as::<i32>(), Some(&42));
        assert_eq!(chain.try_as::<String>(), None);
    }

    #[test]
    fn test_downcast_or() {
        let value: Box<dyn Any> = Box::new(42i32);

        assert_eq!(downcast_or::<i32>(&*value, 0), 42);
        assert_eq!(downcast_or::<String>(&*value, "default".into()), "default");
    }

    #[test]
    fn test_type_matcher() {
        let value: Box<dyn Any> = Box::new(42i32);

        let result = type_match(&*value)
            .case::<String, _>(|s| format!("string: {}", s))
            .case::<i32, _>(|n| format!("int: {}", n))
            .default(|| "unknown".to_string());

        assert_eq!(result, "int: 42");
    }

    #[test]
    fn test_type_matcher_default() {
        let value: Box<dyn Any> = Box::new(3.14f64);

        let result = type_match(&*value)
            .case::<String, _>(|_| "string")
            .case::<i32, _>(|_| "int")
            .default(|| "unknown");

        assert_eq!(result, "unknown");
    }
}
