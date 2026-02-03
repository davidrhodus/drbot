//! Reference utilities for drbot.
//!
//! This crate provides:
//! - Reference utilities
//! - Reference comparisons
//! - Reference transformations

use std::ptr;
use thiserror::Error;

/// Reference error types.
#[derive(Error, Debug, Clone)]
pub enum RefError {
    #[error("Invalid reference")]
    Invalid,

    #[error("Reference expired")]
    Expired,
}

/// Result type for reference operations.
pub type Result<T> = std::result::Result<T, RefError>;

/// Check if references point to same location.
pub fn ref_eq<T: ?Sized>(a: &T, b: &T) -> bool {
    ptr::eq(a, b)
}

/// Get address of reference.
pub fn ref_addr<T: ?Sized>(r: &T) -> usize {
    r as *const T as *const () as usize
}

/// Reference extension trait.
pub trait RefExt {
    /// Get address.
    fn addr(&self) -> usize;

    /// Check if same as other.
    fn same_as<T: ?Sized>(&self, other: &T) -> bool;
}

impl<T: ?Sized> RefExt for T {
    fn addr(&self) -> usize {
        ref_addr(self)
    }

    fn same_as<U: ?Sized>(&self, other: &U) -> bool {
        ref_addr(self) == ref_addr(other)
    }
}

/// Optional reference.
#[derive(Debug, Clone, Copy)]
pub enum OptRef<'a, T> {
    /// Some reference.
    Some(&'a T),
    /// No reference.
    None,
}

impl<'a, T> OptRef<'a, T> {
    /// Create some.
    pub fn some(r: &'a T) -> Self {
        Self::Some(r)
    }

    /// Create none.
    pub fn none() -> Self {
        Self::None
    }

    /// From Option.
    pub fn from_option(opt: Option<&'a T>) -> Self {
        match opt {
            Some(r) => Self::Some(r),
            None => Self::None,
        }
    }

    /// To Option.
    pub fn to_option(self) -> Option<&'a T> {
        match self {
            Self::Some(r) => Some(r),
            Self::None => None,
        }
    }

    /// Is some.
    pub fn is_some(&self) -> bool {
        matches!(self, Self::Some(_))
    }

    /// Is none.
    pub fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }

    /// Unwrap.
    pub fn unwrap(self) -> &'a T {
        match self {
            Self::Some(r) => r,
            Self::None => panic!("OptRef is None"),
        }
    }

    /// Map.
    pub fn map<U, F: FnOnce(&'a T) -> U>(self, f: F) -> Option<U> {
        match self {
            Self::Some(r) => Some(f(r)),
            Self::None => None,
        }
    }
}

impl<'a, T> From<Option<&'a T>> for OptRef<'a, T> {
    fn from(opt: Option<&'a T>) -> Self {
        Self::from_option(opt)
    }
}

impl<'a, T> From<OptRef<'a, T>> for Option<&'a T> {
    fn from(opt: OptRef<'a, T>) -> Self {
        opt.to_option()
    }
}

/// Reference pair.
#[derive(Debug, Clone, Copy)]
pub struct RefPair<'a, T, U> {
    pub first: &'a T,
    pub second: &'a U,
}

impl<'a, T, U> RefPair<'a, T, U> {
    /// Create new pair.
    pub fn new(first: &'a T, second: &'a U) -> Self {
        Self { first, second }
    }

    /// Swap pair.
    pub fn swap(self) -> RefPair<'a, U, T> {
        RefPair {
            first: self.second,
            second: self.first,
        }
    }
}

/// Reference with metadata.
#[derive(Debug, Clone, Copy)]
pub struct RefMeta<'a, T, M> {
    pub reference: &'a T,
    pub metadata: M,
}

impl<'a, T, M> RefMeta<'a, T, M> {
    /// Create new.
    pub fn new(reference: &'a T, metadata: M) -> Self {
        Self {
            reference,
            metadata,
        }
    }
}

/// Convert reference to option.
pub fn ref_to_option<T>(r: &T) -> Option<&T> {
    Some(r)
}

/// Convert mutable reference to option.
pub fn ref_mut_to_option<T>(r: &mut T) -> Option<&mut T> {
    Some(r)
}

/// Reborrow reference.
pub fn reborrow<T>(r: &T) -> &T {
    r
}

/// Reborrow mutable.
pub fn reborrow_mut<T>(r: &mut T) -> &mut T {
    r
}

/// Split reference at index.
pub fn split_at<T>(slice: &[T], mid: usize) -> (&[T], &[T]) {
    slice.split_at(mid)
}

/// Split mutable at index.
pub fn split_at_mut<T>(slice: &mut [T], mid: usize) -> (&mut [T], &mut [T]) {
    slice.split_at_mut(mid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ref_eq() {
        let a = 42;
        let b = 42;
        assert!(ref_eq(&a, &a));
        assert!(!ref_eq(&a, &b));
    }

    #[test]
    fn test_ref_ext() {
        let a = 42;
        assert!(a.same_as(&a));
        assert!(a.addr() > 0);
    }

    #[test]
    fn test_opt_ref() {
        let val = 42;
        let opt = OptRef::some(&val);
        assert!(opt.is_some());
        assert_eq!(*opt.unwrap(), 42);

        let none: OptRef<i32> = OptRef::none();
        assert!(none.is_none());
    }

    #[test]
    fn test_ref_pair() {
        let a = 1;
        let b = "hello";
        let pair = RefPair::new(&a, &b);
        assert_eq!(*pair.first, 1);
        assert_eq!(*pair.second, "hello");
    }
}
