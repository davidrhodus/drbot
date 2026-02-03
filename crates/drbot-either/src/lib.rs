//! Either type utilities for drbot.
//!
//! This crate provides:
//! - Either type (Left or Right)
//! - Either3 type (First, Second, Third)
//! - Either combinators

use thiserror::Error;

/// Either error types.
#[derive(Error, Debug, Clone)]
pub enum EitherError {
    #[error("Expected left, got right")]
    ExpectedLeft,

    #[error("Expected right, got left")]
    ExpectedRight,
}

/// Result type for either operations.
pub type Result<T> = std::result::Result<T, EitherError>;

/// Either left or right value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Either<L, R> {
    /// Left variant.
    Left(L),
    /// Right variant.
    Right(R),
}

impl<L, R> Either<L, R> {
    /// Check if left.
    pub fn is_left(&self) -> bool {
        matches!(self, Self::Left(_))
    }

    /// Check if right.
    pub fn is_right(&self) -> bool {
        matches!(self, Self::Right(_))
    }

    /// Get left value.
    pub fn left(self) -> Option<L> {
        match self {
            Self::Left(l) => Some(l),
            Self::Right(_) => None,
        }
    }

    /// Get right value.
    pub fn right(self) -> Option<R> {
        match self {
            Self::Left(_) => None,
            Self::Right(r) => Some(r),
        }
    }

    /// Get left reference.
    pub fn left_ref(&self) -> Option<&L> {
        match self {
            Self::Left(l) => Some(l),
            Self::Right(_) => None,
        }
    }

    /// Get right reference.
    pub fn right_ref(&self) -> Option<&R> {
        match self {
            Self::Left(_) => None,
            Self::Right(r) => Some(r),
        }
    }

    /// Map left value.
    pub fn map_left<T, F: FnOnce(L) -> T>(self, f: F) -> Either<T, R> {
        match self {
            Self::Left(l) => Either::Left(f(l)),
            Self::Right(r) => Either::Right(r),
        }
    }

    /// Map right value.
    pub fn map_right<T, F: FnOnce(R) -> T>(self, f: F) -> Either<L, T> {
        match self {
            Self::Left(l) => Either::Left(l),
            Self::Right(r) => Either::Right(f(r)),
        }
    }

    /// Map both values.
    pub fn map<A, B, F: FnOnce(L) -> A, G: FnOnce(R) -> B>(self, f: F, g: G) -> Either<A, B> {
        match self {
            Self::Left(l) => Either::Left(f(l)),
            Self::Right(r) => Either::Right(g(r)),
        }
    }

    /// Flip left and right.
    pub fn flip(self) -> Either<R, L> {
        match self {
            Self::Left(l) => Either::Right(l),
            Self::Right(r) => Either::Left(r),
        }
    }

    /// Unwrap left or use default.
    pub fn left_or(self, default: L) -> L {
        match self {
            Self::Left(l) => l,
            Self::Right(_) => default,
        }
    }

    /// Unwrap right or use default.
    pub fn right_or(self, default: R) -> R {
        match self {
            Self::Left(_) => default,
            Self::Right(r) => r,
        }
    }

    /// Unwrap left or compute default.
    pub fn left_or_else<F: FnOnce(R) -> L>(self, f: F) -> L {
        match self {
            Self::Left(l) => l,
            Self::Right(r) => f(r),
        }
    }

    /// Unwrap right or compute default.
    pub fn right_or_else<F: FnOnce(L) -> R>(self, f: F) -> R {
        match self {
            Self::Left(l) => f(l),
            Self::Right(r) => r,
        }
    }

    /// Expect left.
    pub fn expect_left(self, msg: &str) -> L {
        match self {
            Self::Left(l) => l,
            Self::Right(_) => panic!("{}", msg),
        }
    }

    /// Expect right.
    pub fn expect_right(self, msg: &str) -> R {
        match self {
            Self::Left(_) => panic!("{}", msg),
            Self::Right(r) => r,
        }
    }
}

impl<T> Either<T, T> {
    /// Extract value from either variant.
    pub fn into_inner(self) -> T {
        match self {
            Self::Left(t) | Self::Right(t) => t,
        }
    }

    /// Get reference to inner value.
    pub fn as_ref(&self) -> &T {
        match self {
            Self::Left(t) | Self::Right(t) => t,
        }
    }

    /// Map both variants with same function.
    pub fn map_both<U, F: FnOnce(T) -> U>(self, f: F) -> Either<U, U> {
        match self {
            Self::Left(t) => Either::Left(f(t)),
            Self::Right(t) => Either::Right(f(t)),
        }
    }
}

impl<L: Default, R> Default for Either<L, R> {
    fn default() -> Self {
        Self::Left(L::default())
    }
}

/// Three-way either type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Either3<A, B, C> {
    /// First variant.
    First(A),
    /// Second variant.
    Second(B),
    /// Third variant.
    Third(C),
}

impl<A, B, C> Either3<A, B, C> {
    /// Check if first.
    pub fn is_first(&self) -> bool {
        matches!(self, Self::First(_))
    }

    /// Check if second.
    pub fn is_second(&self) -> bool {
        matches!(self, Self::Second(_))
    }

    /// Check if third.
    pub fn is_third(&self) -> bool {
        matches!(self, Self::Third(_))
    }

    /// Get first value.
    pub fn first(self) -> Option<A> {
        match self {
            Self::First(a) => Some(a),
            _ => None,
        }
    }

    /// Get second value.
    pub fn second(self) -> Option<B> {
        match self {
            Self::Second(b) => Some(b),
            _ => None,
        }
    }

    /// Get third value.
    pub fn third(self) -> Option<C> {
        match self {
            Self::Third(c) => Some(c),
            _ => None,
        }
    }

    /// Map first value.
    pub fn map_first<D, F: FnOnce(A) -> D>(self, f: F) -> Either3<D, B, C> {
        match self {
            Self::First(a) => Either3::First(f(a)),
            Self::Second(b) => Either3::Second(b),
            Self::Third(c) => Either3::Third(c),
        }
    }

    /// Map second value.
    pub fn map_second<D, F: FnOnce(B) -> D>(self, f: F) -> Either3<A, D, C> {
        match self {
            Self::First(a) => Either3::First(a),
            Self::Second(b) => Either3::Second(f(b)),
            Self::Third(c) => Either3::Third(c),
        }
    }

    /// Map third value.
    pub fn map_third<D, F: FnOnce(C) -> D>(self, f: F) -> Either3<A, B, D> {
        match self {
            Self::First(a) => Either3::First(a),
            Self::Second(b) => Either3::Second(b),
            Self::Third(c) => Either3::Third(f(c)),
        }
    }

    /// Get variant index (0, 1, or 2).
    pub fn variant_index(&self) -> usize {
        match self {
            Self::First(_) => 0,
            Self::Second(_) => 1,
            Self::Third(_) => 2,
        }
    }
}

impl<T> Either3<T, T, T> {
    /// Extract value from any variant.
    pub fn into_inner(self) -> T {
        match self {
            Self::First(t) | Self::Second(t) | Self::Third(t) => t,
        }
    }
}

/// Create left either.
pub fn left<L, R>(value: L) -> Either<L, R> {
    Either::Left(value)
}

/// Create right either.
pub fn right<L, R>(value: R) -> Either<L, R> {
    Either::Right(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_either_left() {
        let e: Either<i32, &str> = Either::Left(42);
        assert!(e.is_left());
        assert!(!e.is_right());
        assert_eq!(e.left(), Some(42));
        assert_eq!(e.right(), None);
    }

    #[test]
    fn test_either_right() {
        let e: Either<i32, &str> = Either::Right("hello");
        assert!(!e.is_left());
        assert!(e.is_right());
        assert_eq!(e.right(), Some("hello"));
    }

    #[test]
    fn test_either_map() {
        let e: Either<i32, &str> = Either::Left(42);
        let mapped = e.map_left(|x| x * 2);
        assert_eq!(mapped.left(), Some(84));
    }

    #[test]
    fn test_either_flip() {
        let e: Either<i32, &str> = Either::Left(42);
        let flipped = e.flip();
        assert!(flipped.is_right());
    }

    #[test]
    fn test_either_same_type() {
        let e: Either<i32, i32> = Either::Left(42);
        assert_eq!(e.into_inner(), 42);

        let e2: Either<i32, i32> = Either::Right(100);
        assert_eq!(e2.into_inner(), 100);
    }

    #[test]
    fn test_either3() {
        let e: Either3<i32, &str, f64> = Either3::Second("hello");
        assert!(!e.is_first());
        assert!(e.is_second());
        assert!(!e.is_third());
        assert_eq!(e.variant_index(), 1);
    }
}

// ============================================================================
// Kani Formal Verification Proofs
// ============================================================================

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    // ------------------------------------------------------------------------
    // Either Basic Proofs
    // ------------------------------------------------------------------------

    #[kani::proof]
    fn proof_either_left_is_left() {
        let value: u8 = kani::any();
        let e: Either<u8, u8> = Either::Left(value);

        kani::assert(e.is_left(), "Left variant is_left");
        kani::assert(!e.is_right(), "Left variant is not is_right");
    }

    #[kani::proof]
    fn proof_either_right_is_right() {
        let value: u8 = kani::any();
        let e: Either<u8, u8> = Either::Right(value);

        kani::assert(!e.is_left(), "Right variant is not is_left");
        kani::assert(e.is_right(), "Right variant is_right");
    }

    #[kani::proof]
    fn proof_either_left_extraction() {
        let value: u8 = kani::any();
        let e: Either<u8, u8> = Either::Left(value);

        kani::assert(e.left() == Some(value), "left() extracts left value");
    }

    #[kani::proof]
    fn proof_either_left_no_right() {
        let value: u8 = kani::any();
        let e: Either<u8, u8> = Either::Left(value);

        kani::assert(e.right().is_none(), "left variant has no right value");
    }

    #[kani::proof]
    fn proof_either_right_extraction() {
        let value: u8 = kani::any();
        let e: Either<u8, u8> = Either::Right(value);

        kani::assert(e.right() == Some(value), "right() extracts right value");
    }

    #[kani::proof]
    fn proof_either_right_no_left() {
        let value: u8 = kani::any();
        let e: Either<u8, u8> = Either::Right(value);

        kani::assert(e.left().is_none(), "right variant has no left value");
    }

    // ------------------------------------------------------------------------
    // Either Map Proofs
    // ------------------------------------------------------------------------

    #[kani::proof]
    fn proof_either_map_left_on_left() {
        let value: u8 = kani::any();
        let e: Either<u8, u8> = Either::Left(value);

        let mapped = e.map_left(|x| x.wrapping_add(1));

        kani::assert(mapped.is_left(), "map_left preserves Left");
        kani::assert(
            mapped.left() == Some(value.wrapping_add(1)),
            "map_left applies function",
        );
    }

    #[kani::proof]
    fn proof_either_map_left_on_right() {
        let value: u8 = kani::any();
        let e: Either<u8, u8> = Either::Right(value);

        let mapped = e.map_left(|x| x.wrapping_add(1));

        kani::assert(mapped.is_right(), "map_left preserves Right");
        kani::assert(
            mapped.right() == Some(value),
            "map_left doesn't change Right value",
        );
    }

    #[kani::proof]
    fn proof_either_map_right_on_right() {
        let value: u8 = kani::any();
        let e: Either<u8, u8> = Either::Right(value);

        let mapped = e.map_right(|x| x.wrapping_add(1));

        kani::assert(mapped.is_right(), "map_right preserves Right");
        kani::assert(
            mapped.right() == Some(value.wrapping_add(1)),
            "map_right applies function",
        );
    }

    #[kani::proof]
    fn proof_either_map_right_on_left() {
        let value: u8 = kani::any();
        let e: Either<u8, u8> = Either::Left(value);

        let mapped = e.map_right(|x| x.wrapping_add(1));

        kani::assert(mapped.is_left(), "map_right preserves Left");
        kani::assert(
            mapped.left() == Some(value),
            "map_right doesn't change Left value",
        );
    }

    // ------------------------------------------------------------------------
    // Either Flip Proofs
    // ------------------------------------------------------------------------

    #[kani::proof]
    fn proof_either_flip_left() {
        let value: u8 = kani::any();
        let e: Either<u8, u8> = Either::Left(value);

        let flipped = e.flip();

        kani::assert(flipped.is_right(), "flip Left becomes Right");
        kani::assert(flipped.right() == Some(value), "flip preserves value");
    }

    #[kani::proof]
    fn proof_either_flip_right() {
        let value: u8 = kani::any();
        let e: Either<u8, u8> = Either::Right(value);

        let flipped = e.flip();

        kani::assert(flipped.is_left(), "flip Right becomes Left");
        kani::assert(flipped.left() == Some(value), "flip preserves value");
    }

    #[kani::proof]
    fn proof_either_flip_double() {
        let value: u8 = kani::any();
        let e: Either<u8, u8> = Either::Left(value);

        let double_flip = e.flip().flip();

        kani::assert(double_flip.is_left(), "double flip restores variant");
        kani::assert(
            double_flip.left() == Some(value),
            "double flip preserves value",
        );
    }

    // ------------------------------------------------------------------------
    // Either Or Proofs
    // ------------------------------------------------------------------------

    #[kani::proof]
    fn proof_either_left_or_on_left() {
        let value: u8 = kani::any();
        let default: u8 = kani::any();
        let e: Either<u8, u8> = Either::Left(value);

        kani::assert(e.left_or(default) == value, "left_or on Left returns value");
    }

    #[kani::proof]
    fn proof_either_left_or_on_right() {
        let value: u8 = kani::any();
        let default: u8 = kani::any();
        let e: Either<u8, u8> = Either::Right(value);

        kani::assert(
            e.left_or(default) == default,
            "left_or on Right returns default",
        );
    }

    #[kani::proof]
    fn proof_either_right_or_on_right() {
        let value: u8 = kani::any();
        let default: u8 = kani::any();
        let e: Either<u8, u8> = Either::Right(value);

        kani::assert(
            e.right_or(default) == value,
            "right_or on Right returns value",
        );
    }

    #[kani::proof]
    fn proof_either_right_or_on_left() {
        let value: u8 = kani::any();
        let default: u8 = kani::any();
        let e: Either<u8, u8> = Either::Left(value);

        kani::assert(
            e.right_or(default) == default,
            "right_or on Left returns default",
        );
    }

    // ------------------------------------------------------------------------
    // Either Same Type Proofs
    // ------------------------------------------------------------------------

    #[kani::proof]
    fn proof_either_into_inner_left() {
        let value: u8 = kani::any();
        let e: Either<u8, u8> = Either::Left(value);

        kani::assert(e.into_inner() == value, "into_inner extracts Left value");
    }

    #[kani::proof]
    fn proof_either_into_inner_right() {
        let value: u8 = kani::any();
        let e: Either<u8, u8> = Either::Right(value);

        kani::assert(e.into_inner() == value, "into_inner extracts Right value");
    }

    // ------------------------------------------------------------------------
    // Either3 Proofs
    // ------------------------------------------------------------------------

    #[kani::proof]
    fn proof_either3_first_is_first() {
        let value: u8 = kani::any();
        let e: Either3<u8, u8, u8> = Either3::First(value);

        kani::assert(e.is_first(), "First is_first");
        kani::assert(!e.is_second(), "First is not is_second");
        kani::assert(!e.is_third(), "First is not is_third");
    }

    #[kani::proof]
    fn proof_either3_second_is_second() {
        let value: u8 = kani::any();
        let e: Either3<u8, u8, u8> = Either3::Second(value);

        kani::assert(!e.is_first(), "Second is not is_first");
        kani::assert(e.is_second(), "Second is_second");
        kani::assert(!e.is_third(), "Second is not is_third");
    }

    #[kani::proof]
    fn proof_either3_third_is_third() {
        let value: u8 = kani::any();
        let e: Either3<u8, u8, u8> = Either3::Third(value);

        kani::assert(!e.is_first(), "Third is not is_first");
        kani::assert(!e.is_second(), "Third is not is_second");
        kani::assert(e.is_third(), "Third is_third");
    }

    #[kani::proof]
    fn proof_either3_variant_index() {
        let value: u8 = kani::any();

        let e1: Either3<u8, u8, u8> = Either3::First(value);
        let e2: Either3<u8, u8, u8> = Either3::Second(value);
        let e3: Either3<u8, u8, u8> = Either3::Third(value);

        kani::assert(e1.variant_index() == 0, "First has index 0");
        kani::assert(e2.variant_index() == 1, "Second has index 1");
        kani::assert(e3.variant_index() == 2, "Third has index 2");
    }

    #[kani::proof]
    fn proof_either3_extraction() {
        let value: u8 = kani::any();

        let e1: Either3<u8, u8, u8> = Either3::First(value);
        let e2: Either3<u8, u8, u8> = Either3::Second(value);
        let e3: Either3<u8, u8, u8> = Either3::Third(value);

        kani::assert(e1.first() == Some(value), "first() extracts First");
        kani::assert(e2.second() == Some(value), "second() extracts Second");
        kani::assert(e3.third() == Some(value), "third() extracts Third");
    }

    #[kani::proof]
    fn proof_either3_into_inner() {
        let value: u8 = kani::any();

        let e1: Either3<u8, u8, u8> = Either3::First(value);
        let e2: Either3<u8, u8, u8> = Either3::Second(value);
        let e3: Either3<u8, u8, u8> = Either3::Third(value);

        kani::assert(e1.into_inner() == value, "into_inner extracts First");
        kani::assert(e2.into_inner() == value, "into_inner extracts Second");
        kani::assert(e3.into_inner() == value, "into_inner extracts Third");
    }

    // ------------------------------------------------------------------------
    // Helper Function Proofs
    // ------------------------------------------------------------------------

    #[kani::proof]
    fn proof_left_function() {
        let value: u8 = kani::any();
        let e: Either<u8, u8> = left(value);

        kani::assert(e.is_left(), "left() creates Left");
        kani::assert(e.left() == Some(value), "left() has correct value");
    }

    #[kani::proof]
    fn proof_right_function() {
        let value: u8 = kani::any();
        let e: Either<u8, u8> = right(value);

        kani::assert(e.is_right(), "right() creates Right");
        kani::assert(e.right() == Some(value), "right() has correct value");
    }
}
