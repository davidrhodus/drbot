//! Option utilities for drbot.
//!
//! This crate provides:
//! - Option extension methods
//! - Option combinators
//! - Optional value handling

use thiserror::Error;

/// Option-related errors.
#[derive(Error, Debug, Clone)]
pub enum OptionError {
    #[error("Value is None")]
    IsNone,

    #[error("Expected None but got Some")]
    ExpectedNone,
}

/// Result type for option operations.
pub type Result<T> = std::result::Result<T, OptionError>;

/// Option extension trait.
pub trait OptionExt<T> {
    /// Convert to Result with custom error.
    fn ok_or_err<E>(self, err: E) -> std::result::Result<T, E>;

    /// Convert to Result with lazy error.
    fn ok_or_else_err<E, F: FnOnce() -> E>(self, f: F) -> std::result::Result<T, E>;

    /// Check if None.
    fn is_none_or<F: FnOnce(&T) -> bool>(&self, f: F) -> bool;

    /// Check if Some and matches predicate.
    fn is_some_and_matches<F: FnOnce(&T) -> bool>(&self, f: F) -> bool;

    /// Tap into the value if Some.
    fn tap<F: FnOnce(&T)>(self, f: F) -> Self;

    /// Tap into the value if None.
    fn tap_none<F: FnOnce()>(self, f: F) -> Self;

    /// Get or compute default.
    fn get_or_compute<F: FnOnce() -> T>(&self, f: F) -> T
    where
        T: Clone;

    /// Zip with another Option.
    fn zip_with<U, R, F: FnOnce(T, U) -> R>(self, other: Option<U>, f: F) -> Option<R>;

    /// Filter and map in one step.
    fn filter_map_<U, F: FnOnce(&T) -> Option<U>>(self, f: F) -> Option<U>;

    /// Replace value if Some.
    fn replace_if_some<F: FnOnce(T) -> T>(self, f: F) -> Option<T>;

    /// Ensure Some, panicking with message if None.
    fn expect_some(self, msg: &str) -> T;
}

impl<T> OptionExt<T> for Option<T> {
    fn ok_or_err<E>(self, err: E) -> std::result::Result<T, E> {
        self.ok_or(err)
    }

    fn ok_or_else_err<E, F: FnOnce() -> E>(self, f: F) -> std::result::Result<T, E> {
        self.ok_or_else(f)
    }

    fn is_none_or<F: FnOnce(&T) -> bool>(&self, f: F) -> bool {
        match self {
            None => true,
            Some(v) => f(v),
        }
    }

    fn is_some_and_matches<F: FnOnce(&T) -> bool>(&self, f: F) -> bool {
        match self {
            Some(v) => f(v),
            None => false,
        }
    }

    fn tap<F: FnOnce(&T)>(self, f: F) -> Self {
        if let Some(ref v) = self {
            f(v);
        }
        self
    }

    fn tap_none<F: FnOnce()>(self, f: F) -> Self {
        if self.is_none() {
            f();
        }
        self
    }

    fn get_or_compute<F: FnOnce() -> T>(&self, f: F) -> T
    where
        T: Clone,
    {
        match self {
            Some(v) => v.clone(),
            None => f(),
        }
    }

    fn zip_with<U, R, F: FnOnce(T, U) -> R>(self, other: Option<U>, f: F) -> Option<R> {
        match (self, other) {
            (Some(a), Some(b)) => Some(f(a, b)),
            _ => None,
        }
    }

    fn filter_map_<U, F: FnOnce(&T) -> Option<U>>(self, f: F) -> Option<U> {
        self.as_ref().and_then(f)
    }

    fn replace_if_some<F: FnOnce(T) -> T>(self, f: F) -> Option<T> {
        self.map(f)
    }

    fn expect_some(self, msg: &str) -> T {
        self.expect(msg)
    }
}

/// Optional value wrapper with additional methods.
pub struct Optional<T>(Option<T>);

impl<T> Optional<T> {
    /// Create from Option.
    pub fn from_option(opt: Option<T>) -> Self {
        Self(opt)
    }

    /// Create Some.
    pub fn some(value: T) -> Self {
        Self(Some(value))
    }

    /// Create None.
    pub fn none() -> Self {
        Self(None)
    }

    /// Into inner Option.
    pub fn into_option(self) -> Option<T> {
        self.0
    }

    /// Check if Some.
    pub fn is_some(&self) -> bool {
        self.0.is_some()
    }

    /// Check if None.
    pub fn is_none(&self) -> bool {
        self.0.is_none()
    }

    /// Get reference.
    pub fn as_ref(&self) -> Option<&T> {
        self.0.as_ref()
    }

    /// Get mutable reference.
    pub fn as_mut(&mut self) -> Option<&mut T> {
        self.0.as_mut()
    }

    /// Map value.
    pub fn map<U, F: FnOnce(T) -> U>(self, f: F) -> Optional<U> {
        Optional(self.0.map(f))
    }

    /// Filter value.
    pub fn filter<F: FnOnce(&T) -> bool>(self, predicate: F) -> Self {
        Self(self.0.filter(predicate))
    }

    /// Get or default.
    pub fn unwrap_or(self, default: T) -> T {
        self.0.unwrap_or(default)
    }

    /// Get or compute default.
    pub fn unwrap_or_else<F: FnOnce() -> T>(self, f: F) -> T {
        self.0.unwrap_or_else(f)
    }

    /// Get or panic.
    pub fn unwrap(self) -> T {
        self.0.unwrap()
    }

    /// Chain with another Optional.
    pub fn or(self, other: Optional<T>) -> Self {
        Self(self.0.or(other.0))
    }

    /// Chain with lazy computation.
    pub fn or_else<F: FnOnce() -> Optional<T>>(self, f: F) -> Self {
        Self(self.0.or_else(|| f().0))
    }
}

impl<T> From<Option<T>> for Optional<T> {
    fn from(opt: Option<T>) -> Self {
        Self(opt)
    }
}

impl<T> From<Optional<T>> for Option<T> {
    fn from(opt: Optional<T>) -> Self {
        opt.0
    }
}

impl<T: Default> Optional<T> {
    /// Get or default.
    pub fn unwrap_or_default(self) -> T {
        self.0.unwrap_or_default()
    }
}

/// Combine multiple Options.
pub struct OptionCombinator;

impl OptionCombinator {
    /// All must be Some.
    pub fn all<T, I>(options: I) -> Option<Vec<T>>
    where
        I: IntoIterator<Item = Option<T>>,
    {
        options.into_iter().collect()
    }

    /// At least one must be Some (returns first Some).
    pub fn any<T, I>(options: I) -> Option<T>
    where
        I: IntoIterator<Item = Option<T>>,
    {
        for opt in options {
            if opt.is_some() {
                return opt;
            }
        }
        None
    }

    /// First Some or None.
    pub fn first_some<T>(options: &[Option<T>]) -> Option<&T> {
        for opt in options {
            if let Some(v) = opt {
                return Some(v);
            }
        }
        None
    }

    /// Count Some values.
    pub fn count_some<T, I>(options: I) -> usize
    where
        I: IntoIterator<Item = Option<T>>,
    {
        options.into_iter().filter(|o| o.is_some()).count()
    }

    /// Get all Some values.
    pub fn flatten<T, I>(options: I) -> Vec<T>
    where
        I: IntoIterator<Item = Option<T>>,
    {
        options.into_iter().flatten().collect()
    }
}

/// Try operations on Options.
pub struct TryOption;

impl TryOption {
    /// Try to get value, returning Result.
    pub fn get<T>(opt: Option<T>) -> Result<T> {
        opt.ok_or(OptionError::IsNone)
    }

    /// Try to ensure None.
    pub fn ensure_none<T>(opt: Option<T>) -> Result<()> {
        match opt {
            None => Ok(()),
            Some(_) => Err(OptionError::ExpectedNone),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_none_or() {
        let some_five: Option<i32> = Some(5);
        let none: Option<i32> = None;

        assert!(some_five.is_none_or(|&x| x > 3));
        assert!(!some_five.is_none_or(|&x| x > 10));
        assert!(none.is_none_or(|&x| x > 10));
    }

    #[test]
    fn test_tap() {
        let mut tapped = false;
        let _ = Some(5).tap(|_| tapped = true);
        assert!(tapped);

        tapped = false;
        let _: Option<i32> = None.tap(|_| tapped = true);
        assert!(!tapped);
    }

    #[test]
    fn test_zip_with() {
        let a = Some(2);
        let b = Some(3);
        let result = a.zip_with(b, |x, y| x * y);
        assert_eq!(result, Some(6));

        let c: Option<i32> = None;
        let result2 = c.zip_with(Some(3), |x, y| x * y);
        assert_eq!(result2, None);
    }

    #[test]
    fn test_optional() {
        let opt = Optional::some(5);
        assert!(opt.is_some());

        let mapped = opt.map(|x| x * 2);
        assert_eq!(mapped.unwrap(), 10);
    }

    #[test]
    fn test_combinator_all() {
        let options = vec![Some(1), Some(2), Some(3)];
        assert_eq!(OptionCombinator::all(options), Some(vec![1, 2, 3]));

        let with_none = vec![Some(1), None, Some(3)];
        assert_eq!(OptionCombinator::all(with_none), None);
    }

    #[test]
    fn test_combinator_any() {
        let options = vec![None, Some(2), Some(3)];
        assert_eq!(OptionCombinator::any(options), Some(2));

        let all_none: Vec<Option<i32>> = vec![None, None];
        assert_eq!(OptionCombinator::any(all_none), None);
    }

    #[test]
    fn test_flatten() {
        let options = vec![Some(1), None, Some(3), None, Some(5)];
        assert_eq!(OptionCombinator::flatten(options), vec![1, 3, 5]);
    }

    #[test]
    fn test_try_option() {
        assert!(TryOption::get(Some(5)).is_ok());
        assert!(TryOption::get::<i32>(None).is_err());

        assert!(TryOption::ensure_none::<i32>(None).is_ok());
        assert!(TryOption::ensure_none(Some(5)).is_err());
    }
}
