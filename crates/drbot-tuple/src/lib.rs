//! Tuple utilities for drbot.
//!
//! This crate provides:
//! - Tuple manipulation
//! - Tuple conversion traits
//! - Tuple iteration

use thiserror::Error;

/// Tuple error types.
#[derive(Error, Debug, Clone)]
pub enum TupleError {
    #[error("Tuple index out of bounds: {0}")]
    IndexOutOfBounds(usize),

    #[error("Tuple size mismatch")]
    SizeMismatch,
}

/// Result type for tuple operations.
pub type Result<T> = std::result::Result<T, TupleError>;

/// Trait for swapping tuple elements.
pub trait TupleSwap {
    /// The swapped type.
    type Swapped;

    /// Swap the elements.
    fn swap(self) -> Self::Swapped;
}

impl<A, B> TupleSwap for (A, B) {
    type Swapped = (B, A);

    fn swap(self) -> Self::Swapped {
        (self.1, self.0)
    }
}

impl<A, B, C> TupleSwap for (A, B, C) {
    type Swapped = (C, B, A);

    fn swap(self) -> Self::Swapped {
        (self.2, self.1, self.0)
    }
}

/// Trait for mapping over tuple elements.
pub trait TupleMap<F> {
    /// The mapped type.
    type Mapped;

    /// Map function over elements.
    fn map(self, f: F) -> Self::Mapped;
}

impl<A, B, F, R> TupleMap<F> for (A, B)
where
    F: Fn(A) -> R + Clone,
    B: Into<A>,
{
    type Mapped = (R, R);

    fn map(self, f: F) -> Self::Mapped {
        (f(self.0), f.clone()(self.1.into()))
    }
}

/// Trait for converting tuple to array.
pub trait TupleToArray<T, const N: usize> {
    /// Convert to array.
    fn to_array(self) -> [T; N];
}

impl<T> TupleToArray<T, 2> for (T, T) {
    fn to_array(self) -> [T; 2] {
        [self.0, self.1]
    }
}

impl<T> TupleToArray<T, 3> for (T, T, T) {
    fn to_array(self) -> [T; 3] {
        [self.0, self.1, self.2]
    }
}

impl<T> TupleToArray<T, 4> for (T, T, T, T) {
    fn to_array(self) -> [T; 4] {
        [self.0, self.1, self.2, self.3]
    }
}

/// Trait for getting tuple first element.
pub trait TupleFirst {
    /// First element type.
    type First;

    /// Get first element.
    fn first(self) -> Self::First;
}

impl<A, B> TupleFirst for (A, B) {
    type First = A;

    fn first(self) -> Self::First {
        self.0
    }
}

impl<A, B, C> TupleFirst for (A, B, C) {
    type First = A;

    fn first(self) -> Self::First {
        self.0
    }
}

/// Trait for getting tuple last element.
pub trait TupleLast {
    /// Last element type.
    type Last;

    /// Get last element.
    fn last(self) -> Self::Last;
}

impl<A, B> TupleLast for (A, B) {
    type Last = B;

    fn last(self) -> Self::Last {
        self.1
    }
}

impl<A, B, C> TupleLast for (A, B, C) {
    type Last = C;

    fn last(self) -> Self::Last {
        self.2
    }
}

/// Trait for prepending to tuple.
pub trait TuplePrepend<T> {
    /// The prepended type.
    type Prepended;

    /// Prepend value.
    fn prepend(self, value: T) -> Self::Prepended;
}

impl<T, A> TuplePrepend<T> for (A,) {
    type Prepended = (T, A);

    fn prepend(self, value: T) -> Self::Prepended {
        (value, self.0)
    }
}

impl<T, A, B> TuplePrepend<T> for (A, B) {
    type Prepended = (T, A, B);

    fn prepend(self, value: T) -> Self::Prepended {
        (value, self.0, self.1)
    }
}

/// Trait for appending to tuple.
pub trait TupleAppend<T> {
    /// The appended type.
    type Appended;

    /// Append value.
    fn append(self, value: T) -> Self::Appended;
}

impl<T, A> TupleAppend<T> for (A,) {
    type Appended = (A, T);

    fn append(self, value: T) -> Self::Appended {
        (self.0, value)
    }
}

impl<T, A, B> TupleAppend<T> for (A, B) {
    type Appended = (A, B, T);

    fn append(self, value: T) -> Self::Appended {
        (self.0, self.1, value)
    }
}

/// Trait for tuple length.
pub trait TupleLen {
    /// Get length.
    fn len() -> usize;
}

impl<A> TupleLen for (A,) {
    fn len() -> usize {
        1
    }
}

impl<A, B> TupleLen for (A, B) {
    fn len() -> usize {
        2
    }
}

impl<A, B, C> TupleLen for (A, B, C) {
    fn len() -> usize {
        3
    }
}

impl<A, B, C, D> TupleLen for (A, B, C, D) {
    fn len() -> usize {
        4
    }
}

/// Zip two tuples together.
pub fn zip2<A, B, C, D>(a: (A, B), b: (C, D)) -> ((A, C), (B, D)) {
    ((a.0, b.0), (a.1, b.1))
}

/// Unzip tuple of tuples.
pub fn unzip2<A, B, C, D>(t: ((A, B), (C, D))) -> ((A, C), (B, D)) {
    ((t.0 .0, t.1 .0), (t.0 .1, t.1 .1))
}

/// Create a tuple from an iterator.
pub fn from_iter_2<T, I: IntoIterator<Item = T>>(iter: I) -> Option<(T, T)> {
    let mut it = iter.into_iter();
    let a = it.next()?;
    let b = it.next()?;
    Some((a, b))
}

/// Create a tuple from an iterator.
pub fn from_iter_3<T, I: IntoIterator<Item = T>>(iter: I) -> Option<(T, T, T)> {
    let mut it = iter.into_iter();
    let a = it.next()?;
    let b = it.next()?;
    let c = it.next()?;
    Some((a, b, c))
}

/// Create a homogeneous pair.
pub fn pair<T>(a: T, b: T) -> (T, T) {
    (a, b)
}

/// Create a homogeneous triple.
pub fn triple<T>(a: T, b: T, c: T) -> (T, T, T) {
    (a, b, c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tuple_swap() {
        let t = (1, "hello");
        let swapped = t.swap();
        assert_eq!(swapped, ("hello", 1));
    }

    #[test]
    fn test_tuple_to_array() {
        let t = (1, 2, 3);
        let arr = t.to_array();
        assert_eq!(arr, [1, 2, 3]);
    }

    #[test]
    fn test_tuple_first_last() {
        let t = (1, 2, 3);
        assert_eq!(t.first(), 1);
        assert_eq!((1, 2, 3).last(), 3);
    }

    #[test]
    fn test_tuple_prepend_append() {
        let t = (2, 3);
        let prepended = t.prepend(1);
        assert_eq!(prepended, (1, 2, 3));

        let t2 = (1, 2);
        let appended = t2.append(3);
        assert_eq!(appended, (1, 2, 3));
    }

    #[test]
    fn test_tuple_len() {
        assert_eq!(<(i32, i32)>::len(), 2);
        assert_eq!(<(i32, i32, i32)>::len(), 3);
    }

    #[test]
    fn test_from_iter() {
        let v = vec![1, 2, 3];
        let t = from_iter_2(v.clone());
        assert_eq!(t, Some((1, 2)));

        let t3 = from_iter_3(v);
        assert_eq!(t3, Some((1, 2, 3)));
    }
}
