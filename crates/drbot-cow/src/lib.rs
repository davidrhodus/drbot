//! Copy-on-write utilities for drbot.
//!
//! This crate provides:
//! - Cow-like containers
//! - Lazy cloning
//! - Copy-on-write patterns

use std::borrow::Borrow;
use std::ops::Deref;
use thiserror::Error;

/// Cow error types.
#[derive(Error, Debug, Clone)]
pub enum CowError {
    #[error("Cannot convert borrowed to owned")]
    CannotConvert,
}

/// Result type for Cow operations.
pub type Result<T> = std::result::Result<T, CowError>;

/// A clone-on-write smart pointer.
#[derive(Debug)]
pub enum CowBox<'a, T: Clone> {
    /// Borrowed data.
    Borrowed(&'a T),
    /// Owned data.
    Owned(Box<T>),
}

impl<'a, T: Clone> CowBox<'a, T> {
    /// Create borrowed variant.
    pub fn borrowed(value: &'a T) -> Self {
        Self::Borrowed(value)
    }

    /// Create owned variant.
    pub fn owned(value: T) -> Self {
        Self::Owned(Box::new(value))
    }

    /// Check if borrowed.
    pub fn is_borrowed(&self) -> bool {
        matches!(self, Self::Borrowed(_))
    }

    /// Check if owned.
    pub fn is_owned(&self) -> bool {
        matches!(self, Self::Owned(_))
    }

    /// Get mutable reference (clones if borrowed).
    pub fn to_mut(&mut self) -> &mut T {
        match self {
            Self::Borrowed(b) => {
                *self = Self::Owned(Box::new((*b).clone()));
                match self {
                    Self::Owned(o) => o,
                    _ => unreachable!(),
                }
            }
            Self::Owned(o) => o,
        }
    }

    /// Convert to owned.
    pub fn into_owned(self) -> T {
        match self {
            Self::Borrowed(b) => b.clone(),
            Self::Owned(o) => *o,
        }
    }
}

impl<T: Clone> Deref for CowBox<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Borrowed(b) => b,
            Self::Owned(o) => o,
        }
    }
}

impl<T: Clone> Clone for CowBox<'_, T> {
    fn clone(&self) -> Self {
        match self {
            Self::Borrowed(b) => Self::Borrowed(*b),
            Self::Owned(o) => Self::Owned(o.clone()),
        }
    }
}

/// A copy-on-write string.
#[derive(Debug, Clone)]
pub enum CowStr<'a> {
    /// Borrowed string slice.
    Borrowed(&'a str),
    /// Owned String.
    Owned(String),
}

impl<'a> CowStr<'a> {
    /// Create from borrowed.
    pub fn borrowed(s: &'a str) -> Self {
        Self::Borrowed(s)
    }

    /// Create from owned.
    pub fn owned(s: String) -> Self {
        Self::Owned(s)
    }

    /// Check if borrowed.
    pub fn is_borrowed(&self) -> bool {
        matches!(self, Self::Borrowed(_))
    }

    /// Check if owned.
    pub fn is_owned(&self) -> bool {
        matches!(self, Self::Owned(_))
    }

    /// Get mutable string (converts to owned if borrowed).
    pub fn to_mut(&mut self) -> &mut String {
        match self {
            Self::Borrowed(s) => {
                *self = Self::Owned(s.to_string());
                match self {
                    Self::Owned(o) => o,
                    _ => unreachable!(),
                }
            }
            Self::Owned(o) => o,
        }
    }

    /// Convert to owned String.
    pub fn into_owned(self) -> String {
        match self {
            Self::Borrowed(s) => s.to_string(),
            Self::Owned(s) => s,
        }
    }

    /// Get string length.
    pub fn len(&self) -> usize {
        self.as_str().len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.as_str().is_empty()
    }

    /// Get as str slice.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Borrowed(s) => s,
            Self::Owned(s) => s,
        }
    }
}

impl Deref for CowStr<'_> {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl<'a> From<&'a str> for CowStr<'a> {
    fn from(s: &'a str) -> Self {
        Self::Borrowed(s)
    }
}

impl From<String> for CowStr<'_> {
    fn from(s: String) -> Self {
        Self::Owned(s)
    }
}

/// A copy-on-write vector.
#[derive(Debug)]
pub enum CowVec<'a, T: Clone> {
    /// Borrowed slice.
    Borrowed(&'a [T]),
    /// Owned vector.
    Owned(Vec<T>),
}

impl<'a, T: Clone> CowVec<'a, T> {
    /// Create from borrowed slice.
    pub fn borrowed(slice: &'a [T]) -> Self {
        Self::Borrowed(slice)
    }

    /// Create from owned vector.
    pub fn owned(vec: Vec<T>) -> Self {
        Self::Owned(vec)
    }

    /// Check if borrowed.
    pub fn is_borrowed(&self) -> bool {
        matches!(self, Self::Borrowed(_))
    }

    /// Check if owned.
    pub fn is_owned(&self) -> bool {
        matches!(self, Self::Owned(_))
    }

    /// Get mutable vector (converts to owned if borrowed).
    pub fn to_mut(&mut self) -> &mut Vec<T> {
        match self {
            Self::Borrowed(s) => {
                *self = Self::Owned(s.to_vec());
                match self {
                    Self::Owned(o) => o,
                    _ => unreachable!(),
                }
            }
            Self::Owned(o) => o,
        }
    }

    /// Convert to owned vector.
    pub fn into_owned(self) -> Vec<T> {
        match self {
            Self::Borrowed(s) => s.to_vec(),
            Self::Owned(v) => v,
        }
    }

    /// Get length.
    pub fn len(&self) -> usize {
        self.as_slice().len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.as_slice().is_empty()
    }

    /// Get as slice.
    pub fn as_slice(&self) -> &[T] {
        match self {
            Self::Borrowed(s) => s,
            Self::Owned(v) => v,
        }
    }

    /// Push item (converts to owned).
    pub fn push(&mut self, item: T) {
        self.to_mut().push(item);
    }

    /// Pop item (converts to owned).
    pub fn pop(&mut self) -> Option<T> {
        self.to_mut().pop()
    }
}

impl<T: Clone> Deref for CowVec<'_, T> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl<T: Clone> Clone for CowVec<'_, T> {
    fn clone(&self) -> Self {
        match self {
            Self::Borrowed(s) => Self::Borrowed(*s),
            Self::Owned(v) => Self::Owned(v.clone()),
        }
    }
}

/// Lazy clone wrapper.
pub struct LazyClone<T: Clone> {
    value: T,
    cloned: std::cell::Cell<bool>,
}

impl<T: Clone> LazyClone<T> {
    /// Create new lazy clone wrapper.
    pub fn new(value: T) -> Self {
        Self {
            value,
            cloned: std::cell::Cell::new(false),
        }
    }

    /// Check if clone has occurred.
    pub fn was_cloned(&self) -> bool {
        self.cloned.get()
    }

    /// Get reference.
    pub fn get(&self) -> &T {
        &self.value
    }

    /// Clone the value (marks as cloned).
    pub fn clone_value(&self) -> T {
        self.cloned.set(true);
        self.value.clone()
    }

    /// Take ownership without cloning.
    pub fn into_inner(self) -> T {
        self.value
    }
}

impl<T: Clone> Deref for LazyClone<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<T: Clone> Borrow<T> for LazyClone<T> {
    fn borrow(&self) -> &T {
        &self.value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cow_box() {
        let value = 42;
        let mut cow = CowBox::borrowed(&value);
        assert!(cow.is_borrowed());
        assert_eq!(*cow, 42);

        *cow.to_mut() = 100;
        assert!(cow.is_owned());
        assert_eq!(*cow, 100);
    }

    #[test]
    fn test_cow_str() {
        let s = "hello";
        let mut cow = CowStr::borrowed(s);
        assert!(cow.is_borrowed());
        assert_eq!(cow.as_str(), "hello");

        cow.to_mut().push_str(" world");
        assert!(cow.is_owned());
        assert_eq!(cow.as_str(), "hello world");
    }

    #[test]
    fn test_cow_vec() {
        let arr = [1, 2, 3];
        let mut cow = CowVec::borrowed(&arr);
        assert!(cow.is_borrowed());
        assert_eq!(cow.len(), 3);

        cow.push(4);
        assert!(cow.is_owned());
        assert_eq!(cow.as_slice(), &[1, 2, 3, 4]);
    }

    #[test]
    fn test_lazy_clone() {
        let lazy = LazyClone::new(42);
        assert!(!lazy.was_cloned());
        assert_eq!(*lazy, 42);

        let cloned = lazy.clone_value();
        assert!(lazy.was_cloned());
        assert_eq!(cloned, 42);
    }

    #[test]
    fn test_cow_str_into_owned() {
        let s = "hello";
        let cow: CowStr = CowStr::borrowed(s);
        let owned = cow.into_owned();
        assert_eq!(owned, "hello");
    }
}
