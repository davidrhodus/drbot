//! Type identification utilities for drbot.
//!
//! This crate provides:
//! - Type ID wrappers
//! - Type comparison utilities
//! - Runtime type information

use std::any::TypeId;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use thiserror::Error;

/// Type ID error types.
#[derive(Error, Debug, Clone)]
pub enum TypeIdError {
    #[error("Type mismatch: expected {expected}, found {found}")]
    Mismatch { expected: String, found: String },

    #[error("Unknown type ID")]
    Unknown,
}

/// Result type for type ID operations.
pub type Result<T> = std::result::Result<T, TypeIdError>;

/// Type identifier wrapper.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeIdent(TypeId);

impl TypeIdent {
    /// Get TypeIdent for a type.
    pub fn of<T: 'static>() -> Self {
        Self(TypeId::of::<T>())
    }

    /// Get inner TypeId.
    pub fn inner(&self) -> TypeId {
        self.0
    }

    /// Check if matches type.
    pub fn is<T: 'static>(&self) -> bool {
        self.0 == TypeId::of::<T>()
    }
}

impl std::fmt::Debug for TypeIdent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TypeIdent({:?})", self.0)
    }
}

impl From<TypeId> for TypeIdent {
    fn from(id: TypeId) -> Self {
        Self(id)
    }
}

/// Type info with name.
#[derive(Debug, Clone)]
pub struct TypeInfo {
    pub id: TypeIdent,
    pub name: String,
    pub size: usize,
    pub align: usize,
}

impl TypeInfo {
    /// Get info for a type.
    pub fn of<T: 'static>() -> Self {
        Self {
            id: TypeIdent::of::<T>(),
            name: std::any::type_name::<T>().to_string(),
            size: std::mem::size_of::<T>(),
            align: std::mem::align_of::<T>(),
        }
    }

    /// Check if matches type.
    pub fn is<T: 'static>(&self) -> bool {
        self.id.is::<T>()
    }

    /// Get short name (without module path).
    pub fn short_name(&self) -> &str {
        self.name.rsplit("::").next().unwrap_or(&self.name)
    }
}

impl PartialEq for TypeInfo {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for TypeInfo {}

impl Hash for TypeInfo {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

/// Type registry.
pub struct TypeRegistry {
    types: HashMap<TypeIdent, TypeInfo>,
    by_name: HashMap<String, TypeIdent>,
}

impl TypeRegistry {
    /// Create new registry.
    pub fn new() -> Self {
        Self {
            types: HashMap::new(),
            by_name: HashMap::new(),
        }
    }

    /// Register a type.
    pub fn register<T: 'static>(&mut self) {
        let info = TypeInfo::of::<T>();
        self.by_name.insert(info.name.clone(), info.id);
        self.types.insert(info.id, info);
    }

    /// Get type info by ID.
    pub fn get(&self, id: TypeIdent) -> Option<&TypeInfo> {
        self.types.get(&id)
    }

    /// Get type info by name.
    pub fn get_by_name(&self, name: &str) -> Option<&TypeInfo> {
        self.by_name.get(name).and_then(|id| self.types.get(id))
    }

    /// Check if type is registered.
    pub fn contains<T: 'static>(&self) -> bool {
        self.types.contains_key(&TypeIdent::of::<T>())
    }

    /// Get all registered types.
    pub fn all(&self) -> impl Iterator<Item = &TypeInfo> {
        self.types.values()
    }

    /// Get count.
    pub fn len(&self) -> usize {
        self.types.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.types.is_empty()
    }
}

impl Default for TypeRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Check if two values have same type.
pub fn same_type<T: 'static, U: 'static>(_a: &T, _b: &U) -> bool {
    TypeId::of::<T>() == TypeId::of::<U>()
}

/// Get type name.
pub fn type_name<T: 'static>() -> &'static str {
    std::any::type_name::<T>()
}

/// Get short type name.
pub fn short_type_name<T: 'static>() -> &'static str {
    let name = std::any::type_name::<T>();
    let base = name.split('<').next().unwrap_or(name);
    base.rsplit("::").next().unwrap_or(base)
}

/// Type guard for runtime type checking.
pub struct TypeGuard {
    expected: TypeIdent,
    expected_name: String,
}

impl TypeGuard {
    /// Create guard for type.
    pub fn for_type<T: 'static>() -> Self {
        Self {
            expected: TypeIdent::of::<T>(),
            expected_name: type_name::<T>().to_string(),
        }
    }

    /// Check if value matches.
    pub fn check<T: 'static>(&self, _value: &T) -> Result<()> {
        if TypeIdent::of::<T>() == self.expected {
            Ok(())
        } else {
            Err(TypeIdError::Mismatch {
                expected: self.expected_name.clone(),
                found: type_name::<T>().to_string(),
            })
        }
    }

    /// Check type ID directly.
    pub fn check_id(&self, id: TypeIdent) -> bool {
        id == self.expected
    }
}

/// Type assertion.
pub fn assert_type<T: 'static, U: 'static>() {
    assert!(
        TypeId::of::<T>() == TypeId::of::<U>(),
        "Type mismatch: expected {}, got {}",
        type_name::<T>(),
        type_name::<U>()
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_ident() {
        let id1 = TypeIdent::of::<i32>();
        let id2 = TypeIdent::of::<i32>();
        let id3 = TypeIdent::of::<String>();

        assert_eq!(id1, id2);
        assert_ne!(id1, id3);
        assert!(id1.is::<i32>());
        assert!(!id1.is::<String>());
    }

    #[test]
    fn test_type_info() {
        let info = TypeInfo::of::<u64>();

        assert!(info.is::<u64>());
        assert_eq!(info.size, 8);
        assert!(info.name.contains("u64"));
    }

    #[test]
    fn test_type_registry() {
        let mut registry = TypeRegistry::new();
        registry.register::<i32>();
        registry.register::<String>();

        assert!(registry.contains::<i32>());
        assert!(registry.contains::<String>());
        assert!(!registry.contains::<f64>());
        assert_eq!(registry.len(), 2);
    }

    #[test]
    fn test_same_type() {
        let a = 42i32;
        let b = 100i32;
        let c = "hello";

        assert!(same_type(&a, &b));
        assert!(!same_type(&a, &c));
    }

    #[test]
    fn test_type_guard() {
        let guard = TypeGuard::for_type::<i32>();

        assert!(guard.check(&42i32).is_ok());
        assert!(guard.check(&"hello").is_err());
    }

    #[test]
    fn test_short_type_name() {
        let name = short_type_name::<Vec<String>>();
        // Should be something like "Vec" or "Vec<String>"
        assert!(name.contains("Vec"));
    }
}
