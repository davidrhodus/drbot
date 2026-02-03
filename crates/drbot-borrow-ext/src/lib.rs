//! Borrow trait extensions for drbot.
//!
//! This crate provides:
//! - Borrow extensions
//! - AsRef/AsMut helpers
//! - Borrow utilities

use std::borrow::{Borrow, BorrowMut};
use thiserror::Error;

/// Borrow error types.
#[derive(Error, Debug, Clone)]
pub enum BorrowError {
    #[error("Already borrowed")]
    AlreadyBorrowed,

    #[error("Already mutably borrowed")]
    AlreadyMutablyBorrowed,
}

/// Result type for borrow operations.
pub type Result<T> = std::result::Result<T, BorrowError>;

/// Borrow extension trait.
pub trait BorrowExt<T: ?Sized>: Borrow<T> {
    /// Borrow and apply function.
    fn borrow_with<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&T) -> R,
    {
        f(self.borrow())
    }
}

impl<B: Borrow<T>, T: ?Sized> BorrowExt<T> for B {}

/// BorrowMut extension trait.
pub trait BorrowMutExt<T: ?Sized>: BorrowMut<T> {
    /// Borrow mutably and apply function.
    fn borrow_mut_with<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce(&mut T) -> R,
    {
        f(self.borrow_mut())
    }
}

impl<B: BorrowMut<T>, T: ?Sized> BorrowMutExt<T> for B {}

/// AsRef extension trait.
pub trait AsRefExt<T: ?Sized> {
    /// Get reference.
    fn as_ref_ext(&self) -> &T;
}

impl<R: AsRef<T>, T: ?Sized> AsRefExt<T> for R {
    fn as_ref_ext(&self) -> &T {
        self.as_ref()
    }
}

/// Map a reference through a function.
pub fn map_ref<'a, T: ?Sized, U: ?Sized, F>(value: &'a T, f: F) -> &'a U
where
    F: FnOnce(&'a T) -> &'a U,
{
    f(value)
}

/// AsMut extension trait.
pub trait AsMutExt<T: ?Sized> {
    /// Get mutable reference.
    fn as_mut_ext(&mut self) -> &mut T;
}

impl<R: AsMut<T>, T: ?Sized> AsMutExt<T> for R {
    fn as_mut_ext(&mut self) -> &mut T {
        self.as_mut()
    }
}

/// Map a mutable reference through a function.
pub fn map_mut<'a, T: ?Sized, U: ?Sized, F>(value: &'a mut T, f: F) -> &'a mut U
where
    F: FnOnce(&'a mut T) -> &'a mut U,
{
    f(value)
}

/// Cow-like borrowed or owned.
pub enum Borrowed<'a, T: 'a + ?Sized> {
    /// Borrowed reference.
    Borrowed(&'a T),
    /// Owned copy (for Sized types).
    Owned(Box<T>),
}

impl<'a, T: ?Sized> Borrowed<'a, T> {
    /// Is borrowed.
    pub fn is_borrowed(&self) -> bool {
        matches!(self, Self::Borrowed(_))
    }

    /// Is owned.
    pub fn is_owned(&self) -> bool {
        matches!(self, Self::Owned(_))
    }

    /// Get reference.
    pub fn as_ref(&self) -> &T {
        match self {
            Self::Borrowed(r) => r,
            Self::Owned(b) => b,
        }
    }
}

impl<'a, T: Clone> Borrowed<'a, T> {
    /// Create borrowed.
    pub fn borrowed(r: &'a T) -> Self {
        Self::Borrowed(r)
    }

    /// Create owned.
    pub fn owned(value: T) -> Self {
        Self::Owned(Box::new(value))
    }

    /// Into owned.
    pub fn into_owned(self) -> T {
        match self {
            Self::Borrowed(r) => r.clone(),
            Self::Owned(b) => *b,
        }
    }

    /// To mut.
    pub fn to_mut(&mut self) -> &mut T {
        if let Self::Borrowed(r) = self {
            *self = Self::Owned(Box::new(r.clone()));
        }
        match self {
            Self::Owned(b) => b,
            _ => unreachable!(),
        }
    }
}

impl<'a, T: ?Sized> std::ops::Deref for Borrowed<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.as_ref()
    }
}

/// Transparent borrow wrapper.
#[repr(transparent)]
pub struct TransparentBorrow<T>(T);

impl<T> TransparentBorrow<T> {
    /// Create new.
    pub fn new(value: T) -> Self {
        Self(value)
    }

    /// Get inner.
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> Borrow<T> for TransparentBorrow<T> {
    fn borrow(&self) -> &T {
        &self.0
    }
}

impl<T> BorrowMut<T> for TransparentBorrow<T> {
    fn borrow_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

impl<T> AsRef<T> for TransparentBorrow<T> {
    fn as_ref(&self) -> &T {
        &self.0
    }
}

impl<T> AsMut<T> for TransparentBorrow<T> {
    fn as_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

impl<T> std::ops::Deref for TransparentBorrow<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> std::ops::DerefMut for TransparentBorrow<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// Borrow guard for tracking borrows.
pub struct BorrowGuard<'a, T> {
    value: &'a T,
}

impl<'a, T> BorrowGuard<'a, T> {
    /// Create new guard.
    pub fn new(value: &'a T) -> Self {
        Self { value }
    }

    /// Get reference.
    pub fn get(&self) -> &T {
        self.value
    }
}

impl<'a, T> std::ops::Deref for BorrowGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.value
    }
}

/// Mutable borrow guard.
pub struct BorrowMutGuard<'a, T> {
    value: &'a mut T,
}

impl<'a, T> BorrowMutGuard<'a, T> {
    /// Create new guard.
    pub fn new(value: &'a mut T) -> Self {
        Self { value }
    }

    /// Get reference.
    pub fn get(&self) -> &T {
        self.value
    }

    /// Get mutable reference.
    pub fn get_mut(&mut self) -> &mut T {
        self.value
    }
}

impl<'a, T> std::ops::Deref for BorrowMutGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.value
    }
}

impl<'a, T> std::ops::DerefMut for BorrowMutGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_borrow_ext() {
        let s = String::from("hello");
        let result = s.borrow_with(|s: &str| s.len());
        assert_eq!(result, 5);
    }

    #[test]
    fn test_borrowed() {
        let value = 42;
        let borrowed = Borrowed::borrowed(&value);
        assert!(borrowed.is_borrowed());
        assert_eq!(*borrowed, 42);
    }

    #[test]
    fn test_borrowed_to_owned() {
        let value = 42;
        let mut borrowed = Borrowed::borrowed(&value);
        *borrowed.to_mut() = 84;
        assert!(borrowed.is_owned());
        assert_eq!(*borrowed, 84);
    }

    #[test]
    fn test_transparent_borrow() {
        let tb = TransparentBorrow::new(42);
        assert_eq!(*tb, 42);
    }
}

// ============================================================================
// Kani Formal Verification Proofs
// ============================================================================

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    // ========================================================================
    // BorrowExt Trait Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_borrow_with_applies_function() {
        let value: u8 = kani::any();
        let boxed = Box::new(value);

        let result = boxed.borrow_with(|v: &u8| v.wrapping_add(1));

        kani::assert(
            result == value.wrapping_add(1),
            "borrow_with must apply function",
        );
    }

    #[kani::proof]
    fn proof_borrow_with_identity() {
        let value: u8 = kani::any();
        let boxed = Box::new(value);

        let result = boxed.borrow_with(|v: &u8| *v);

        kani::assert(result == value, "borrow_with identity must return value");
    }

    // ========================================================================
    // BorrowMutExt Trait Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_borrow_mut_with_applies_function() {
        let value: u8 = kani::any();
        let mut boxed = Box::new(value);

        let result = boxed.borrow_mut_with(|v: &mut u8| {
            *v = v.wrapping_add(1);
            *v
        });

        kani::assert(
            result == value.wrapping_add(1),
            "borrow_mut_with must apply function",
        );
    }

    #[kani::proof]
    fn proof_borrow_mut_with_modifies() {
        let value: u8 = kani::any();
        let mut boxed = Box::new(value);

        boxed.borrow_mut_with(|v: &mut u8| {
            *v = 42;
        });

        kani::assert(*boxed == 42, "borrow_mut_with must modify value");
    }

    // ========================================================================
    // Borrowed<T> Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_borrowed_is_borrowed() {
        let value: u8 = kani::any();
        let borrowed = Borrowed::borrowed(&value);

        kani::assert(
            borrowed.is_borrowed(),
            "borrowed must return is_borrowed true",
        );
        kani::assert(!borrowed.is_owned(), "borrowed must return is_owned false");
    }

    #[kani::proof]
    fn proof_borrowed_owned_is_owned() {
        let value: u8 = kani::any();
        let borrowed = Borrowed::owned(value);

        kani::assert(borrowed.is_owned(), "owned must return is_owned true");
        kani::assert(
            !borrowed.is_borrowed(),
            "owned must return is_borrowed false",
        );
    }

    #[kani::proof]
    fn proof_borrowed_as_ref_borrowed() {
        let value: u8 = kani::any();
        let borrowed = Borrowed::borrowed(&value);

        kani::assert(
            *borrowed.as_ref() == value,
            "as_ref must return correct value",
        );
    }

    #[kani::proof]
    fn proof_borrowed_as_ref_owned() {
        let value: u8 = kani::any();
        let borrowed = Borrowed::owned(value);

        kani::assert(
            *borrowed.as_ref() == value,
            "as_ref must return correct value for owned",
        );
    }

    #[kani::proof]
    fn proof_borrowed_into_owned_from_borrowed() {
        let value: u8 = kani::any();
        let borrowed = Borrowed::borrowed(&value);

        let owned = borrowed.into_owned();

        kani::assert(owned == value, "into_owned must return correct value");
    }

    #[kani::proof]
    fn proof_borrowed_into_owned_from_owned() {
        let value: u8 = kani::any();
        let borrowed = Borrowed::owned(value);

        let owned = borrowed.into_owned();

        kani::assert(owned == value, "into_owned must return correct value");
    }

    #[kani::proof]
    fn proof_borrowed_to_mut_converts_to_owned() {
        let value: u8 = kani::any();
        let mut borrowed = Borrowed::borrowed(&value);

        let _ = borrowed.to_mut();

        kani::assert(borrowed.is_owned(), "to_mut must convert to owned");
    }

    #[kani::proof]
    fn proof_borrowed_to_mut_modifiable() {
        let value: u8 = kani::any();
        let new_value: u8 = kani::any();
        let mut borrowed = Borrowed::borrowed(&value);

        *borrowed.to_mut() = new_value;

        kani::assert(
            *borrowed.as_ref() == new_value,
            "to_mut must allow modification",
        );
    }

    #[kani::proof]
    fn proof_borrowed_deref() {
        let value: u8 = kani::any();
        let borrowed = Borrowed::borrowed(&value);

        kani::assert(*borrowed == value, "deref must return correct value");
    }

    #[kani::proof]
    fn proof_borrowed_owned_deref() {
        let value: u8 = kani::any();
        let borrowed = Borrowed::owned(value);

        kani::assert(
            *borrowed == value,
            "deref must return correct value for owned",
        );
    }

    // ========================================================================
    // TransparentBorrow Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_transparent_borrow_new() {
        let value: u8 = kani::any();
        let tb = TransparentBorrow::new(value);

        kani::assert(*tb == value, "new must store value correctly");
    }

    #[kani::proof]
    fn proof_transparent_borrow_into_inner() {
        let value: u8 = kani::any();
        let tb = TransparentBorrow::new(value);

        let inner = tb.into_inner();

        kani::assert(inner == value, "into_inner must return original value");
    }

    #[kani::proof]
    fn proof_transparent_borrow_borrow() {
        let value: u8 = kani::any();
        let tb = TransparentBorrow::new(value);

        let borrowed: &u8 = tb.borrow();

        kani::assert(*borrowed == value, "borrow must return reference to value");
    }

    #[kani::proof]
    fn proof_transparent_borrow_borrow_mut() {
        let value: u8 = kani::any();
        let new_value: u8 = kani::any();
        let mut tb = TransparentBorrow::new(value);

        *tb.borrow_mut() = new_value;

        kani::assert(*tb == new_value, "borrow_mut must allow modification");
    }

    #[kani::proof]
    fn proof_transparent_borrow_as_ref() {
        let value: u8 = kani::any();
        let tb = TransparentBorrow::new(value);

        let r: &u8 = tb.as_ref();

        kani::assert(*r == value, "as_ref must return reference");
    }

    #[kani::proof]
    fn proof_transparent_borrow_as_mut() {
        let value: u8 = kani::any();
        let new_value: u8 = kani::any();
        let mut tb = TransparentBorrow::new(value);

        *tb.as_mut() = new_value;

        kani::assert(*tb == new_value, "as_mut must allow modification");
    }

    #[kani::proof]
    fn proof_transparent_borrow_deref() {
        let value: u8 = kani::any();
        let tb = TransparentBorrow::new(value);

        kani::assert(*tb == value, "deref must return value");
    }

    #[kani::proof]
    fn proof_transparent_borrow_deref_mut() {
        let value: u8 = kani::any();
        let new_value: u8 = kani::any();
        let mut tb = TransparentBorrow::new(value);

        *tb = new_value;

        kani::assert(*tb == new_value, "deref_mut must allow modification");
    }

    // ========================================================================
    // BorrowGuard Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_borrow_guard_new() {
        let value: u8 = kani::any();
        let guard = BorrowGuard::new(&value);

        kani::assert(*guard.get() == value, "new must create guard with value");
    }

    #[kani::proof]
    fn proof_borrow_guard_deref() {
        let value: u8 = kani::any();
        let guard = BorrowGuard::new(&value);

        kani::assert(*guard == value, "deref must return value");
    }

    // ========================================================================
    // BorrowMutGuard Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_borrow_mut_guard_new() {
        let mut value: u8 = kani::any();
        let guard = BorrowMutGuard::new(&mut value);

        kani::assert(*guard.get() == value, "new must create guard with value");
    }

    #[kani::proof]
    fn proof_borrow_mut_guard_get_mut() {
        let value: u8 = kani::any();
        let new_value: u8 = kani::any();
        let mut stored = value;
        let mut guard = BorrowMutGuard::new(&mut stored);

        *guard.get_mut() = new_value;

        kani::assert(*guard.get() == new_value, "get_mut must allow modification");
    }

    #[kani::proof]
    fn proof_borrow_mut_guard_deref() {
        let value: u8 = kani::any();
        let mut stored = value;
        let guard = BorrowMutGuard::new(&mut stored);

        kani::assert(*guard == value, "deref must return value");
    }

    #[kani::proof]
    fn proof_borrow_mut_guard_deref_mut() {
        let value: u8 = kani::any();
        let new_value: u8 = kani::any();
        let mut stored = value;
        let mut guard = BorrowMutGuard::new(&mut stored);

        *guard = new_value;

        kani::assert(*guard == new_value, "deref_mut must allow modification");
    }

    // ========================================================================
    // map_ref / map_mut Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_map_ref_applies_function() {
        let value: u8 = kani::any();

        let result = map_ref(&value, |v| v);

        kani::assert(*result == value, "map_ref must apply function");
    }

    #[kani::proof]
    fn proof_map_mut_applies_function() {
        let mut value: u8 = kani::any();

        let result = map_mut(&mut value, |v| v);

        kani::assert(*result == value, "map_mut must apply function");
    }
}
