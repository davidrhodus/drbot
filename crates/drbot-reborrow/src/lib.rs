//! Reborrowing utilities for drbot.
//!
//! This crate provides:
//! - Reborrow helpers
//! - Reference splitting
//! - Multi-borrow patterns

use thiserror::Error;

/// Reborrow error types.
#[derive(Error, Debug, Clone)]
pub enum ReborrowError {
    #[error("Cannot reborrow")]
    CannotReborrow,
}

/// Result type for reborrow operations.
pub type Result<T> = std::result::Result<T, ReborrowError>;

/// Reborrow trait for types that support reborrowing.
pub trait Reborrow {
    /// The borrowed type.
    type Borrowed<'a>
    where
        Self: 'a;

    /// Reborrow as shorter lifetime.
    fn reborrow(&mut self) -> Self::Borrowed<'_>;
}

impl<T> Reborrow for &mut T {
    type Borrowed<'a>
        = &'a mut T
    where
        Self: 'a;

    fn reborrow(&mut self) -> Self::Borrowed<'_> {
        *self
    }
}

/// Split a mutable reference into two parts.
pub fn split_mut<T, U, F>(value: &mut T, f: F) -> (&mut U, &mut T)
where
    F: FnOnce(&mut T) -> &mut U,
{
    let ptr = value as *mut T;
    let part = f(value);
    // SAFETY: This is sound only if f returns a disjoint part.
    // The caller must ensure this invariant.
    unsafe { (part, &mut *ptr) }
}

/// Reference pair with both shared and unique access.
pub struct RefPair<'a, T, U> {
    first: &'a T,
    second: &'a U,
}

impl<'a, T, U> RefPair<'a, T, U> {
    /// Create new pair.
    pub fn new(first: &'a T, second: &'a U) -> Self {
        Self { first, second }
    }

    /// Get first.
    pub fn first(&self) -> &T {
        self.first
    }

    /// Get second.
    pub fn second(&self) -> &U {
        self.second
    }

    /// Into tuple.
    pub fn into_parts(self) -> (&'a T, &'a U) {
        (self.first, self.second)
    }
}

/// Mutable reference pair.
pub struct MutPair<'a, T, U> {
    first: &'a mut T,
    second: &'a mut U,
}

impl<'a, T, U> MutPair<'a, T, U> {
    /// Create new pair from disjoint references.
    pub fn new(first: &'a mut T, second: &'a mut U) -> Self {
        Self { first, second }
    }

    /// Get first.
    pub fn first(&self) -> &T {
        self.first
    }

    /// Get first mutable.
    pub fn first_mut(&mut self) -> &mut T {
        self.first
    }

    /// Get second.
    pub fn second(&self) -> &U {
        self.second
    }

    /// Get second mutable.
    pub fn second_mut(&mut self) -> &mut U {
        self.second
    }

    /// Into tuple.
    pub fn into_parts(self) -> (&'a mut T, &'a mut U) {
        (self.first, self.second)
    }
}

/// Split a slice into two mutable parts.
pub fn split_slice<T>(slice: &mut [T], mid: usize) -> (&mut [T], &mut [T]) {
    slice.split_at_mut(mid)
}

/// Split a slice into three parts.
pub fn split_slice3<T>(
    slice: &mut [T],
    first_end: usize,
    second_end: usize,
) -> (&mut [T], &mut [T], &mut [T]) {
    let (first, rest) = slice.split_at_mut(first_end);
    let (second, third) = rest.split_at_mut(second_end - first_end);
    (first, second, third)
}

/// Reborrow wrapper that tracks borrows.
pub struct ReborrowCell<T> {
    value: T,
}

impl<T> ReborrowCell<T> {
    /// Create new.
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

    /// Reborrow.
    pub fn reborrow(&mut self) -> &mut T {
        &mut self.value
    }

    /// Into inner.
    pub fn into_inner(self) -> T {
        self.value
    }
}

impl<T> std::ops::Deref for ReborrowCell<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<T> std::ops::DerefMut for ReborrowCell<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}

/// Multi-borrow pattern for struct fields.
pub struct FieldBorrow<'a, A, B> {
    a: &'a mut A,
    b: &'a mut B,
}

impl<'a, A, B> FieldBorrow<'a, A, B> {
    /// Create from disjoint field references.
    pub fn new(a: &'a mut A, b: &'a mut B) -> Self {
        Self { a, b }
    }

    /// Get A.
    pub fn a(&mut self) -> &mut A {
        self.a
    }

    /// Get B.
    pub fn b(&mut self) -> &mut B {
        self.b
    }

    /// Get both.
    pub fn both(&mut self) -> (&mut A, &mut B) {
        (self.a, self.b)
    }
}

/// Index-based multi-borrow for arrays/vectors.
pub fn get_two_mut<T>(slice: &mut [T], i: usize, j: usize) -> Option<(&mut T, &mut T)> {
    if i == j || i >= slice.len() || j >= slice.len() {
        return None;
    }

    let ptr = slice.as_mut_ptr();
    unsafe {
        let a = &mut *ptr.add(i);
        let b = &mut *ptr.add(j);
        Some((a, b))
    }
}

/// Get three mutable references from a slice.
pub fn get_three_mut<T>(
    slice: &mut [T],
    i: usize,
    j: usize,
    k: usize,
) -> Option<(&mut T, &mut T, &mut T)> {
    if i == j || j == k || i == k {
        return None;
    }
    if i >= slice.len() || j >= slice.len() || k >= slice.len() {
        return None;
    }

    let ptr = slice.as_mut_ptr();
    unsafe {
        let a = &mut *ptr.add(i);
        let b = &mut *ptr.add(j);
        let c = &mut *ptr.add(k);
        Some((a, b, c))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reborrow() {
        let mut value = 42;
        let mut r = &mut value;
        let rb = r.reborrow();
        *rb = 84;
        assert_eq!(value, 84);
    }

    #[test]
    fn test_ref_pair() {
        let a = 1;
        let b = 2;
        let pair = RefPair::new(&a, &b);
        assert_eq!(*pair.first(), 1);
        assert_eq!(*pair.second(), 2);
    }

    #[test]
    fn test_split_slice() {
        let mut arr = [1, 2, 3, 4, 5];
        let (left, right) = split_slice(&mut arr, 2);
        assert_eq!(left, &[1, 2]);
        assert_eq!(right, &[3, 4, 5]);
    }

    #[test]
    fn test_get_two_mut() {
        let mut arr = [1, 2, 3, 4, 5];
        let (a, b) = get_two_mut(&mut arr, 1, 3).unwrap();
        *a = 10;
        *b = 20;
        assert_eq!(arr, [1, 10, 3, 20, 5]);
    }

    #[test]
    fn test_get_two_mut_same_index() {
        let mut arr = [1, 2, 3];
        assert!(get_two_mut(&mut arr, 1, 1).is_none());
    }
}
