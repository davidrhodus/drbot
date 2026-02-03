//! Identity and equality utilities for drbot.
//!
//! This crate provides:
//! - Identity checking
//! - Pointer equality
//! - Reference identity

use std::ptr;
use thiserror::Error;

/// Identity error types.
#[derive(Error, Debug, Clone)]
pub enum IdentityError {
    #[error("Identity mismatch")]
    Mismatch,

    #[error("Null reference")]
    NullRef,
}

/// Result type for identity operations.
pub type Result<T> = std::result::Result<T, IdentityError>;

/// Check pointer equality.
pub fn ptr_eq<T: ?Sized>(a: &T, b: &T) -> bool {
    ptr::eq(a, b)
}

/// Check if same address.
pub fn same_address<T>(a: &T, b: &T) -> bool {
    ptr::eq(a, b)
}

/// Identity wrapper.
#[derive(Debug)]
pub struct Identity<T> {
    value: T,
}

impl<T> Identity<T> {
    /// Create new identity wrapper.
    pub fn new(value: T) -> Self {
        Self { value }
    }

    /// Get reference.
    pub fn get(&self) -> &T {
        &self.value
    }

    /// Get mutable reference.
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.value
    }

    /// Into inner value.
    pub fn into_inner(self) -> T {
        self.value
    }

    /// Check if same identity.
    pub fn same_as(&self, other: &Identity<T>) -> bool {
        ptr_eq(self, other)
    }
}

impl<T: PartialEq> PartialEq for Identity<T> {
    fn eq(&self, other: &Self) -> bool {
        // Identity equality is pointer equality
        ptr_eq(self, other)
    }
}

impl<T: Eq> Eq for Identity<T> {}

/// Identity by reference.
#[derive(Debug)]
pub struct IdentityRef<'a, T> {
    value: &'a T,
}

impl<'a, T> IdentityRef<'a, T> {
    /// Create new identity reference.
    pub fn new(value: &'a T) -> Self {
        Self { value }
    }

    /// Get reference.
    pub fn get(&self) -> &T {
        self.value
    }

    /// Check same identity.
    pub fn same_as(&self, other: &IdentityRef<T>) -> bool {
        ptr_eq(self.value, other.value)
    }
}

impl<T> PartialEq for IdentityRef<'_, T> {
    fn eq(&self, other: &Self) -> bool {
        ptr_eq(self.value, other.value)
    }
}

impl<T> Eq for IdentityRef<'_, T> {}

impl<T> Clone for IdentityRef<'_, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for IdentityRef<'_, T> {}

/// Get address of value.
pub fn address_of<T: ?Sized>(value: &T) -> usize {
    value as *const T as *const () as usize
}

/// Identity trait.
pub trait HasIdentity {
    /// Get identity key.
    fn identity_key(&self) -> usize {
        address_of(self)
    }

    /// Check same identity.
    fn same_identity<T: HasIdentity>(&self, other: &T) -> bool {
        self.identity_key() == other.identity_key()
    }
}

impl<T> HasIdentity for T {}

/// Identified value with explicit ID.
#[derive(Debug, Clone)]
pub struct Identified<T> {
    id: u64,
    value: T,
}

impl<T> Identified<T> {
    /// Create with ID.
    pub fn new(id: u64, value: T) -> Self {
        Self { id, value }
    }

    /// Get ID.
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Get value.
    pub fn value(&self) -> &T {
        &self.value
    }

    /// Get mutable value.
    pub fn value_mut(&mut self) -> &mut T {
        &mut self.value
    }

    /// Into value.
    pub fn into_value(self) -> T {
        self.value
    }
}

impl<T> PartialEq for Identified<T> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<T> Eq for Identified<T> {}

impl<T> std::hash::Hash for Identified<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

/// ID generator.
#[derive(Debug)]
pub struct IdGenerator {
    next: std::sync::atomic::AtomicU64,
}

impl IdGenerator {
    /// Create new generator.
    pub const fn new() -> Self {
        Self {
            next: std::sync::atomic::AtomicU64::new(1),
        }
    }

    /// Generate next ID.
    pub fn next(&self) -> u64 {
        self.next.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }

    /// Get current value.
    pub fn current(&self) -> u64 {
        self.next.load(std::sync::atomic::Ordering::Relaxed)
    }
}

impl Default for IdGenerator {
    fn default() -> Self {
        Self::new()
    }
}

/// Global ID generator.
static GLOBAL_ID: IdGenerator = IdGenerator::new();

/// Generate global unique ID.
pub fn next_id() -> u64 {
    GLOBAL_ID.next()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ptr_eq() {
        let a = 42;
        let b = 42;
        assert!(ptr_eq(&a, &a));
        assert!(!ptr_eq(&a, &b));
    }

    #[test]
    fn test_identity() {
        let a = Identity::new(42);
        let b = Identity::new(42);
        assert!(!a.same_as(&b));
        assert!(a.same_as(&a));
    }

    #[test]
    fn test_identity_ref() {
        let val = 42;
        let r1 = IdentityRef::new(&val);
        let r2 = IdentityRef::new(&val);
        assert!(r1.same_as(&r2));
    }

    #[test]
    fn test_identified() {
        let a = Identified::new(1, "hello");
        let b = Identified::new(1, "world");
        assert_eq!(a, b); // Same ID

        let c = Identified::new(2, "hello");
        assert_ne!(a, c); // Different ID
    }

    #[test]
    fn test_id_generator() {
        let gen = IdGenerator::new();
        let id1 = gen.next();
        let id2 = gen.next();
        assert_ne!(id1, id2);
    }
}
