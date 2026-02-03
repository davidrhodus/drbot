//! Matcher utilities for drbot.
//!
//! This crate provides:
//! - Matcher trait and implementations
//! - Composite matchers (and, or, not)
//! - Predicate-based matching

use std::sync::Arc;
use thiserror::Error;

/// Matcher error types.
#[derive(Error, Debug)]
pub enum MatcherError {
    #[error("Match failed")]
    MatchFailed,

    #[error("Invalid input")]
    InvalidInput,
}

/// Result type for matcher operations.
pub type Result<T> = std::result::Result<T, MatcherError>;

/// Matcher trait.
pub trait Matcher<T: ?Sized>: Send + Sync {
    /// Check if value matches.
    fn matches(&self, value: &T) -> bool;
}

/// Always matches.
pub struct AlwaysMatcher;

impl<T> Matcher<T> for AlwaysMatcher {
    fn matches(&self, _value: &T) -> bool {
        true
    }
}

/// Never matches.
pub struct NeverMatcher;

impl<T> Matcher<T> for NeverMatcher {
    fn matches(&self, _value: &T) -> bool {
        false
    }
}

/// Equality matcher.
pub struct EqualsMatcher<T: PartialEq + Send + Sync> {
    expected: T,
}

impl<T: PartialEq + Send + Sync> EqualsMatcher<T> {
    /// Create new equals matcher.
    pub fn new(expected: T) -> Self {
        Self { expected }
    }
}

impl<T: PartialEq + Send + Sync> Matcher<T> for EqualsMatcher<T> {
    fn matches(&self, value: &T) -> bool {
        value == &self.expected
    }
}

/// Predicate matcher.
pub struct PredicateMatcher<T, F: Fn(&T) -> bool + Send + Sync> {
    predicate: F,
    _marker: std::marker::PhantomData<fn() -> T>,
}

impl<T, F: Fn(&T) -> bool + Send + Sync> PredicateMatcher<T, F> {
    /// Create new predicate matcher.
    pub fn new(predicate: F) -> Self {
        Self {
            predicate,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<T, F: Fn(&T) -> bool + Send + Sync> Matcher<T> for PredicateMatcher<T, F> {
    fn matches(&self, value: &T) -> bool {
        (self.predicate)(value)
    }
}

/// Not matcher (negation).
pub struct NotMatcher<T> {
    inner: Arc<dyn Matcher<T>>,
}

impl<T> NotMatcher<T> {
    /// Create new not matcher.
    pub fn new(inner: Arc<dyn Matcher<T>>) -> Self {
        Self { inner }
    }
}

impl<T> Matcher<T> for NotMatcher<T> {
    fn matches(&self, value: &T) -> bool {
        !self.inner.matches(value)
    }
}

/// And matcher (all must match).
pub struct AndMatcher<T> {
    matchers: Vec<Arc<dyn Matcher<T>>>,
}

impl<T> AndMatcher<T> {
    /// Create new and matcher.
    pub fn new(matchers: Vec<Arc<dyn Matcher<T>>>) -> Self {
        Self { matchers }
    }
}

impl<T> Matcher<T> for AndMatcher<T> {
    fn matches(&self, value: &T) -> bool {
        self.matchers.iter().all(|m| m.matches(value))
    }
}

/// Or matcher (any must match).
pub struct OrMatcher<T> {
    matchers: Vec<Arc<dyn Matcher<T>>>,
}

impl<T> OrMatcher<T> {
    /// Create new or matcher.
    pub fn new(matchers: Vec<Arc<dyn Matcher<T>>>) -> Self {
        Self { matchers }
    }
}

impl<T> Matcher<T> for OrMatcher<T> {
    fn matches(&self, value: &T) -> bool {
        self.matchers.iter().any(|m| m.matches(value))
    }
}

/// Range matcher for ordered types.
pub struct RangeMatcher<T: Ord + Send + Sync> {
    min: Option<T>,
    max: Option<T>,
    inclusive_min: bool,
    inclusive_max: bool,
}

impl<T: Ord + Send + Sync> RangeMatcher<T> {
    /// Create matcher for range [min, max].
    pub fn between(min: T, max: T) -> Self {
        Self {
            min: Some(min),
            max: Some(max),
            inclusive_min: true,
            inclusive_max: true,
        }
    }

    /// Create matcher for range (min, max).
    pub fn between_exclusive(min: T, max: T) -> Self {
        Self {
            min: Some(min),
            max: Some(max),
            inclusive_min: false,
            inclusive_max: false,
        }
    }

    /// Create matcher for >= value.
    pub fn at_least(min: T) -> Self {
        Self {
            min: Some(min),
            max: None,
            inclusive_min: true,
            inclusive_max: true,
        }
    }

    /// Create matcher for <= value.
    pub fn at_most(max: T) -> Self {
        Self {
            min: None,
            max: Some(max),
            inclusive_min: true,
            inclusive_max: true,
        }
    }
}

impl<T: Ord + Send + Sync> Matcher<T> for RangeMatcher<T> {
    fn matches(&self, value: &T) -> bool {
        let min_ok = match &self.min {
            Some(min) => {
                if self.inclusive_min {
                    value >= min
                } else {
                    value > min
                }
            }
            None => true,
        };

        let max_ok = match &self.max {
            Some(max) => {
                if self.inclusive_max {
                    value <= max
                } else {
                    value < max
                }
            }
            None => true,
        };

        min_ok && max_ok
    }
}

/// String contains matcher.
pub struct ContainsMatcher {
    substring: String,
    case_sensitive: bool,
}

impl ContainsMatcher {
    /// Create new contains matcher.
    pub fn new(substring: &str) -> Self {
        Self {
            substring: substring.to_string(),
            case_sensitive: true,
        }
    }

    /// Create case-insensitive matcher.
    pub fn case_insensitive(substring: &str) -> Self {
        Self {
            substring: substring.to_lowercase(),
            case_sensitive: false,
        }
    }
}

impl Matcher<String> for ContainsMatcher {
    fn matches(&self, value: &String) -> bool {
        if self.case_sensitive {
            value.contains(&self.substring)
        } else {
            value.to_lowercase().contains(&self.substring)
        }
    }
}

impl Matcher<str> for ContainsMatcher {
    fn matches(&self, value: &str) -> bool {
        if self.case_sensitive {
            value.contains(&self.substring)
        } else {
            value.to_lowercase().contains(&self.substring)
        }
    }
}

/// Builder for composite matchers.
pub struct MatcherBuilder<T: 'static> {
    matchers: Vec<Arc<dyn Matcher<T>>>,
}

impl<T: 'static> MatcherBuilder<T> {
    /// Create new builder.
    pub fn new() -> Self {
        Self {
            matchers: Vec::new(),
        }
    }

    /// Add matcher.
    pub fn add(mut self, matcher: Arc<dyn Matcher<T>>) -> Self {
        self.matchers.push(matcher);
        self
    }

    /// Build AND matcher.
    pub fn build_and(self) -> AndMatcher<T> {
        AndMatcher::new(self.matchers)
    }

    /// Build OR matcher.
    pub fn build_or(self) -> OrMatcher<T> {
        OrMatcher::new(self.matchers)
    }
}

impl<T: 'static> Default for MatcherBuilder<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper functions.
pub fn equals<T: PartialEq + Send + Sync + 'static>(expected: T) -> Arc<dyn Matcher<T>> {
    Arc::new(EqualsMatcher::new(expected))
}

pub fn predicate<T: 'static, F>(f: F) -> Arc<dyn Matcher<T>>
where
    F: Fn(&T) -> bool + Send + Sync + 'static,
{
    Arc::new(PredicateMatcher::new(f))
}

pub fn not<T: 'static>(matcher: Arc<dyn Matcher<T>>) -> Arc<dyn Matcher<T>> {
    Arc::new(NotMatcher::new(matcher))
}

pub fn all<T: 'static>(matchers: Vec<Arc<dyn Matcher<T>>>) -> Arc<dyn Matcher<T>> {
    Arc::new(AndMatcher::new(matchers))
}

pub fn any<T: 'static>(matchers: Vec<Arc<dyn Matcher<T>>>) -> Arc<dyn Matcher<T>> {
    Arc::new(OrMatcher::new(matchers))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_equals_matcher() {
        let matcher = EqualsMatcher::new(42);
        assert!(matcher.matches(&42));
        assert!(!matcher.matches(&41));
    }

    #[test]
    fn test_predicate_matcher() {
        let matcher = PredicateMatcher::new(|x: &i32| *x > 10);
        assert!(matcher.matches(&15));
        assert!(!matcher.matches(&5));
    }

    #[test]
    fn test_and_matcher() {
        let m1: Arc<dyn Matcher<i32>> = predicate(|x: &i32| *x > 0);
        let m2: Arc<dyn Matcher<i32>> = predicate(|x: &i32| *x < 100);
        let and = AndMatcher::new(vec![m1, m2]);

        assert!(and.matches(&50));
        assert!(!and.matches(&-5));
        assert!(!and.matches(&150));
    }

    #[test]
    fn test_or_matcher() {
        let m1: Arc<dyn Matcher<i32>> = equals(1);
        let m2: Arc<dyn Matcher<i32>> = equals(2);
        let or = OrMatcher::new(vec![m1, m2]);

        assert!(or.matches(&1));
        assert!(or.matches(&2));
        assert!(!or.matches(&3));
    }

    #[test]
    fn test_range_matcher() {
        let matcher = RangeMatcher::between(10, 20);
        assert!(matcher.matches(&15));
        assert!(matcher.matches(&10));
        assert!(matcher.matches(&20));
        assert!(!matcher.matches(&5));
        assert!(!matcher.matches(&25));
    }

    #[test]
    fn test_contains_matcher() {
        let matcher = ContainsMatcher::new("world");
        assert!(matcher.matches(&"hello world".to_string()));
        assert!(!matcher.matches(&"hello".to_string()));

        let ci_matcher = ContainsMatcher::case_insensitive("WORLD");
        assert!(ci_matcher.matches(&"hello world".to_string()));
    }
}
