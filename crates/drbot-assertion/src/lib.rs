//! Assertion and invariant checking for drbot.
//!
//! This crate provides:
//! - Runtime assertions
//! - Invariant checking
//! - Pre/post conditions
//! - Assertion collection

use std::fmt;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use thiserror::Error;

/// Assertion error types.
#[derive(Error, Debug)]
pub enum AssertionError {
    #[error("Assertion failed: {message}")]
    Failed {
        message: String,
        location: Option<Location>,
    },

    #[error("Precondition failed: {0}")]
    PreconditionFailed(String),

    #[error("Postcondition failed: {0}")]
    PostconditionFailed(String),

    #[error("Invariant violated: {0}")]
    InvariantViolated(String),
}

/// Result type for assertion operations.
pub type Result<T> = std::result::Result<T, AssertionError>;

/// Source location.
#[derive(Debug, Clone)]
pub struct Location {
    /// File name.
    pub file: String,
    /// Line number.
    pub line: u32,
    /// Column number.
    pub column: u32,
}

impl Location {
    /// Create new location.
    pub fn new(file: impl Into<String>, line: u32, column: u32) -> Self {
        Self {
            file: file.into(),
            line,
            column,
        }
    }
}

impl fmt::Display for Location {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}:{}", self.file, self.line, self.column)
    }
}

/// Assertion result.
#[derive(Debug, Clone)]
pub struct AssertionResult {
    /// Whether assertion passed.
    pub passed: bool,
    /// Assertion message.
    pub message: String,
    /// Location.
    pub location: Option<Location>,
}

impl AssertionResult {
    /// Create passed result.
    pub fn passed(message: impl Into<String>) -> Self {
        Self {
            passed: true,
            message: message.into(),
            location: None,
        }
    }

    /// Create failed result.
    pub fn failed(message: impl Into<String>) -> Self {
        Self {
            passed: false,
            message: message.into(),
            location: None,
        }
    }

    /// Set location.
    pub fn at(mut self, location: Location) -> Self {
        self.location = Some(location);
        self
    }
}

/// Assert that condition is true.
pub fn assert_that(condition: bool, message: impl Into<String>) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(AssertionError::Failed {
            message: message.into(),
            location: None,
        })
    }
}

/// Assert equality.
pub fn assert_eq<T: PartialEq + fmt::Debug>(
    left: T,
    right: T,
    message: impl Into<String>,
) -> Result<()> {
    if left == right {
        Ok(())
    } else {
        Err(AssertionError::Failed {
            message: format!("{}: {:?} != {:?}", message.into(), left, right),
            location: None,
        })
    }
}

/// Assert not equal.
pub fn assert_ne<T: PartialEq + fmt::Debug>(
    left: T,
    right: T,
    message: impl Into<String>,
) -> Result<()> {
    if left != right {
        Ok(())
    } else {
        Err(AssertionError::Failed {
            message: format!("{}: {:?} == {:?}", message.into(), left, right),
            location: None,
        })
    }
}

/// Assert less than.
pub fn assert_lt<T: PartialOrd + fmt::Debug>(
    left: T,
    right: T,
    message: impl Into<String>,
) -> Result<()> {
    if left < right {
        Ok(())
    } else {
        Err(AssertionError::Failed {
            message: format!("{}: {:?} >= {:?}", message.into(), left, right),
            location: None,
        })
    }
}

/// Assert less than or equal.
pub fn assert_le<T: PartialOrd + fmt::Debug>(
    left: T,
    right: T,
    message: impl Into<String>,
) -> Result<()> {
    if left <= right {
        Ok(())
    } else {
        Err(AssertionError::Failed {
            message: format!("{}: {:?} > {:?}", message.into(), left, right),
            location: None,
        })
    }
}

/// Assert greater than.
pub fn assert_gt<T: PartialOrd + fmt::Debug>(
    left: T,
    right: T,
    message: impl Into<String>,
) -> Result<()> {
    if left > right {
        Ok(())
    } else {
        Err(AssertionError::Failed {
            message: format!("{}: {:?} <= {:?}", message.into(), left, right),
            location: None,
        })
    }
}

/// Assert greater than or equal.
pub fn assert_ge<T: PartialOrd + fmt::Debug>(
    left: T,
    right: T,
    message: impl Into<String>,
) -> Result<()> {
    if left >= right {
        Ok(())
    } else {
        Err(AssertionError::Failed {
            message: format!("{}: {:?} < {:?}", message.into(), left, right),
            location: None,
        })
    }
}

/// Assert option is Some.
pub fn assert_some<T>(opt: Option<T>, message: impl Into<String>) -> Result<T> {
    opt.ok_or_else(|| AssertionError::Failed {
        message: format!("{}: expected Some, got None", message.into()),
        location: None,
    })
}

/// Assert option is None.
pub fn assert_none<T: fmt::Debug>(opt: Option<T>, message: impl Into<String>) -> Result<()> {
    if opt.is_none() {
        Ok(())
    } else {
        Err(AssertionError::Failed {
            message: format!("{}: expected None, got {:?}", message.into(), opt),
            location: None,
        })
    }
}

/// Assert result is Ok.
pub fn assert_ok<T, E: fmt::Debug>(
    result: std::result::Result<T, E>,
    message: impl Into<String>,
) -> Result<T> {
    result.map_err(|e| AssertionError::Failed {
        message: format!("{}: expected Ok, got Err({:?})", message.into(), e),
        location: None,
    })
}

/// Assert result is Err.
pub fn assert_err<T: fmt::Debug, E>(
    result: std::result::Result<T, E>,
    message: impl Into<String>,
) -> Result<E> {
    match result {
        Err(e) => Ok(e),
        Ok(v) => Err(AssertionError::Failed {
            message: format!("{}: expected Err, got Ok({:?})", message.into(), v),
            location: None,
        }),
    }
}

/// Precondition checker.
pub struct Precondition {
    name: String,
}

impl Precondition {
    /// Create new precondition.
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }

    /// Check condition.
    pub fn check(&self, condition: bool) -> Result<()> {
        if condition {
            Ok(())
        } else {
            Err(AssertionError::PreconditionFailed(self.name.clone()))
        }
    }

    /// Require non-null.
    pub fn require<T>(&self, value: Option<T>) -> Result<T> {
        value.ok_or_else(|| {
            AssertionError::PreconditionFailed(format!("{}: value is null", self.name))
        })
    }

    /// Require non-empty.
    pub fn require_non_empty(&self, s: &str) -> Result<()> {
        if !s.is_empty() {
            Ok(())
        } else {
            Err(AssertionError::PreconditionFailed(format!(
                "{}: string is empty",
                self.name
            )))
        }
    }

    /// Require in range.
    pub fn require_in_range<T: PartialOrd + fmt::Debug>(
        &self,
        value: T,
        min: T,
        max: T,
    ) -> Result<()> {
        if value >= min && value <= max {
            Ok(())
        } else {
            Err(AssertionError::PreconditionFailed(format!(
                "{}: {:?} not in range [{:?}, {:?}]",
                self.name, value, min, max
            )))
        }
    }
}

/// Postcondition checker.
pub struct Postcondition {
    name: String,
}

impl Postcondition {
    /// Create new postcondition.
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }

    /// Check condition.
    pub fn check(&self, condition: bool) -> Result<()> {
        if condition {
            Ok(())
        } else {
            Err(AssertionError::PostconditionFailed(self.name.clone()))
        }
    }

    /// Ensure value is valid.
    pub fn ensure<T, F: FnOnce(&T) -> bool>(&self, value: T, predicate: F) -> Result<T> {
        if predicate(&value) {
            Ok(value)
        } else {
            Err(AssertionError::PostconditionFailed(self.name.clone()))
        }
    }
}

/// Invariant checker.
pub struct Invariant {
    name: String,
    check_fn: Box<dyn Fn() -> bool + Send + Sync>,
}

impl Invariant {
    /// Create new invariant.
    pub fn new<F>(name: impl Into<String>, check: F) -> Self
    where
        F: Fn() -> bool + Send + Sync + 'static,
    {
        Self {
            name: name.into(),
            check_fn: Box::new(check),
        }
    }

    /// Check invariant.
    pub fn check(&self) -> Result<()> {
        if (self.check_fn)() {
            Ok(())
        } else {
            Err(AssertionError::InvariantViolated(self.name.clone()))
        }
    }

    /// Get name.
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Assertion collector.
pub struct AssertionCollector {
    assertions: Mutex<Vec<AssertionResult>>,
    pass_count: AtomicUsize,
    fail_count: AtomicUsize,
}

impl AssertionCollector {
    /// Create new collector.
    pub fn new() -> Self {
        Self {
            assertions: Mutex::new(Vec::new()),
            pass_count: AtomicUsize::new(0),
            fail_count: AtomicUsize::new(0),
        }
    }

    /// Add assertion.
    pub fn add(&self, result: AssertionResult) {
        if result.passed {
            self.pass_count.fetch_add(1, Ordering::SeqCst);
        } else {
            self.fail_count.fetch_add(1, Ordering::SeqCst);
        }
        self.assertions.lock().unwrap().push(result);
    }

    /// Check condition.
    pub fn check(&self, condition: bool, message: impl Into<String>) {
        let result = if condition {
            AssertionResult::passed(message)
        } else {
            AssertionResult::failed(message)
        };
        self.add(result);
    }

    /// Get pass count.
    pub fn pass_count(&self) -> usize {
        self.pass_count.load(Ordering::SeqCst)
    }

    /// Get fail count.
    pub fn fail_count(&self) -> usize {
        self.fail_count.load(Ordering::SeqCst)
    }

    /// Get total count.
    pub fn total_count(&self) -> usize {
        self.pass_count() + self.fail_count()
    }

    /// Check if all passed.
    pub fn all_passed(&self) -> bool {
        self.fail_count() == 0
    }

    /// Get all results.
    pub fn results(&self) -> Vec<AssertionResult> {
        self.assertions.lock().unwrap().clone()
    }

    /// Get failed results.
    pub fn failures(&self) -> Vec<AssertionResult> {
        self.assertions
            .lock()
            .unwrap()
            .iter()
            .filter(|r| !r.passed)
            .cloned()
            .collect()
    }

    /// Generate summary.
    pub fn summary(&self) -> String {
        format!(
            "{}/{} assertions passed",
            self.pass_count(),
            self.total_count()
        )
    }
}

impl Default for AssertionCollector {
    fn default() -> Self {
        Self::new()
    }
}

/// Contract for function with pre/postconditions.
pub struct Contract<T> {
    preconditions: Vec<Box<dyn Fn(&T) -> bool + Send + Sync>>,
    postconditions: Vec<Box<dyn Fn(&T) -> bool + Send + Sync>>,
}

impl<T> Contract<T> {
    /// Create new contract.
    pub fn new() -> Self {
        Self {
            preconditions: Vec::new(),
            postconditions: Vec::new(),
        }
    }

    /// Add precondition.
    pub fn requires<F>(mut self, condition: F) -> Self
    where
        F: Fn(&T) -> bool + Send + Sync + 'static,
    {
        self.preconditions.push(Box::new(condition));
        self
    }

    /// Add postcondition.
    pub fn ensures<F>(mut self, condition: F) -> Self
    where
        F: Fn(&T) -> bool + Send + Sync + 'static,
    {
        self.postconditions.push(Box::new(condition));
        self
    }

    /// Check preconditions.
    pub fn check_pre(&self, value: &T) -> Result<()> {
        for (i, pre) in self.preconditions.iter().enumerate() {
            if !pre(value) {
                return Err(AssertionError::PreconditionFailed(format!(
                    "Precondition {} failed",
                    i
                )));
            }
        }
        Ok(())
    }

    /// Check postconditions.
    pub fn check_post(&self, value: &T) -> Result<()> {
        for (i, post) in self.postconditions.iter().enumerate() {
            if !post(value) {
                return Err(AssertionError::PostconditionFailed(format!(
                    "Postcondition {} failed",
                    i
                )));
            }
        }
        Ok(())
    }
}

impl<T> Default for Contract<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_assert_that() {
        assert!(assert_that(true, "should pass").is_ok());
        assert!(assert_that(false, "should fail").is_err());
    }

    #[test]
    fn test_assert_eq() {
        assert!(assert_eq(5, 5, "equal").is_ok());
        assert!(assert_eq(5, 6, "not equal").is_err());
    }

    #[test]
    fn test_assert_some_none() {
        assert!(assert_some(Some(42), "some").is_ok());
        assert!(assert_some::<i32>(None, "none").is_err());
        assert!(assert_none::<i32>(None, "none").is_ok());
        assert!(assert_none(Some(42), "some").is_err());
    }

    #[test]
    fn test_precondition() {
        let pre = Precondition::new("test");
        assert!(pre.check(true).is_ok());
        assert!(pre.check(false).is_err());
        assert!(pre.require_non_empty("hello").is_ok());
        assert!(pre.require_non_empty("").is_err());
    }

    #[test]
    fn test_collector() {
        let collector = AssertionCollector::new();
        collector.check(true, "pass");
        collector.check(false, "fail");
        collector.check(true, "pass2");

        assert_eq!(collector.pass_count(), 2);
        assert_eq!(collector.fail_count(), 1);
        assert!(!collector.all_passed());
    }

    #[test]
    fn test_contract() {
        let contract = Contract::<i32>::new()
            .requires(|&x| x > 0)
            .ensures(|&x| x < 100);

        assert!(contract.check_pre(&50).is_ok());
        assert!(contract.check_pre(&-1).is_err());
        assert!(contract.check_post(&50).is_ok());
        assert!(contract.check_post(&150).is_err());
    }
}

// ============================================================================
// Kani Formal Verification Proofs
// ============================================================================

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    // ========================================================================
    // assert_that() Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_assert_that_true_ok() {
        let result = assert_that(true, "test");
        kani::assert(result.is_ok(), "true condition must succeed");
    }

    #[kani::proof]
    fn proof_assert_that_false_err() {
        let result = assert_that(false, "test");
        kani::assert(result.is_err(), "false condition must fail");
    }

    // ========================================================================
    // assert_eq() Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_assert_eq_equal_ok() {
        let a: u8 = kani::any();
        let result = assert_eq(a, a, "test");
        kani::assert(result.is_ok(), "equal values must succeed");
    }

    #[kani::proof]
    fn proof_assert_eq_not_equal_err() {
        let a: u8 = kani::any();
        let b: u8 = kani::any();
        kani::assume(a != b);

        let result = assert_eq(a, b, "test");
        kani::assert(result.is_err(), "unequal values must fail");
    }

    // ========================================================================
    // assert_ne() Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_assert_ne_not_equal_ok() {
        let a: u8 = kani::any();
        let b: u8 = kani::any();
        kani::assume(a != b);

        let result = assert_ne(a, b, "test");
        kani::assert(result.is_ok(), "unequal values must succeed");
    }

    #[kani::proof]
    fn proof_assert_ne_equal_err() {
        let a: u8 = kani::any();
        let result = assert_ne(a, a, "test");
        kani::assert(result.is_err(), "equal values must fail");
    }

    // ========================================================================
    // assert_lt() Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_assert_lt_less_ok() {
        let a: u8 = kani::any();
        let b: u8 = kani::any();
        kani::assume(a < b);

        let result = assert_lt(a, b, "test");
        kani::assert(result.is_ok(), "less than must succeed");
    }

    #[kani::proof]
    fn proof_assert_lt_ge_err() {
        let a: u8 = kani::any();
        let b: u8 = kani::any();
        kani::assume(a >= b);

        let result = assert_lt(a, b, "test");
        kani::assert(result.is_err(), "greater or equal must fail");
    }

    // ========================================================================
    // assert_le() Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_assert_le_le_ok() {
        let a: u8 = kani::any();
        let b: u8 = kani::any();
        kani::assume(a <= b);

        let result = assert_le(a, b, "test");
        kani::assert(result.is_ok(), "less or equal must succeed");
    }

    #[kani::proof]
    fn proof_assert_le_gt_err() {
        let a: u8 = kani::any();
        let b: u8 = kani::any();
        kani::assume(a > b);

        let result = assert_le(a, b, "test");
        kani::assert(result.is_err(), "greater must fail");
    }

    // ========================================================================
    // assert_gt() Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_assert_gt_greater_ok() {
        let a: u8 = kani::any();
        let b: u8 = kani::any();
        kani::assume(a > b);

        let result = assert_gt(a, b, "test");
        kani::assert(result.is_ok(), "greater than must succeed");
    }

    #[kani::proof]
    fn proof_assert_gt_le_err() {
        let a: u8 = kani::any();
        let b: u8 = kani::any();
        kani::assume(a <= b);

        let result = assert_gt(a, b, "test");
        kani::assert(result.is_err(), "less or equal must fail");
    }

    // ========================================================================
    // assert_ge() Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_assert_ge_ge_ok() {
        let a: u8 = kani::any();
        let b: u8 = kani::any();
        kani::assume(a >= b);

        let result = assert_ge(a, b, "test");
        kani::assert(result.is_ok(), "greater or equal must succeed");
    }

    #[kani::proof]
    fn proof_assert_ge_lt_err() {
        let a: u8 = kani::any();
        let b: u8 = kani::any();
        kani::assume(a < b);

        let result = assert_ge(a, b, "test");
        kani::assert(result.is_err(), "less must fail");
    }

    // ========================================================================
    // assert_some() / assert_none() Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_assert_some_some_ok() {
        let value: u8 = kani::any();
        let opt = Some(value);
        let result = assert_some(opt, "test");
        kani::assert(result.is_ok(), "Some must succeed");
        kani::assert(result.unwrap() == value, "must return value");
    }

    #[kani::proof]
    fn proof_assert_some_none_err() {
        let opt: Option<u8> = None;
        let result = assert_some(opt, "test");
        kani::assert(result.is_err(), "None must fail");
    }

    #[kani::proof]
    fn proof_assert_none_none_ok() {
        let opt: Option<u8> = None;
        let result = assert_none(opt, "test");
        kani::assert(result.is_ok(), "None must succeed");
    }

    #[kani::proof]
    fn proof_assert_none_some_err() {
        let value: u8 = kani::any();
        let opt = Some(value);
        let result = assert_none(opt, "test");
        kani::assert(result.is_err(), "Some must fail");
    }

    // ========================================================================
    // assert_ok() / assert_err() Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_assert_ok_ok_ok() {
        let value: u8 = kani::any();
        let result: std::result::Result<u8, &str> = Ok(value);
        let check = assert_ok(result, "test");
        kani::assert(check.is_ok(), "Ok must succeed");
        kani::assert(check.unwrap() == value, "must return value");
    }

    #[kani::proof]
    fn proof_assert_ok_err_err() {
        let result: std::result::Result<u8, &str> = Err("error");
        let check = assert_ok(result, "test");
        kani::assert(check.is_err(), "Err must fail");
    }

    #[kani::proof]
    fn proof_assert_err_err_ok() {
        let result: std::result::Result<u8, &str> = Err("error");
        let check = assert_err(result, "test");
        kani::assert(check.is_ok(), "Err must succeed");
    }

    #[kani::proof]
    fn proof_assert_err_ok_err() {
        let value: u8 = kani::any();
        let result: std::result::Result<u8, &str> = Ok(value);
        let check = assert_err(result, "test");
        kani::assert(check.is_err(), "Ok must fail");
    }

    // ========================================================================
    // AssertionResult Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_assertion_result_passed() {
        let result = AssertionResult::passed("test");
        kani::assert(result.passed == true, "passed must set passed to true");
        kani::assert(result.location.is_none(), "location must be None");
    }

    #[kani::proof]
    fn proof_assertion_result_failed() {
        let result = AssertionResult::failed("test");
        kani::assert(result.passed == false, "failed must set passed to false");
        kani::assert(result.location.is_none(), "location must be None");
    }

    #[kani::proof]
    fn proof_assertion_result_at_sets_location() {
        let line: u32 = kani::any();
        let column: u32 = kani::any();

        let result = AssertionResult::passed("test").at(Location::new("file.rs", line, column));

        kani::assert(result.location.is_some(), "at must set location");
        let loc = result.location.unwrap();
        kani::assert(loc.line == line, "line must match");
        kani::assert(loc.column == column, "column must match");
    }

    // ========================================================================
    // Precondition Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_precondition_check_true_ok() {
        let pre = Precondition::new("test");
        let result = pre.check(true);
        kani::assert(result.is_ok(), "true condition must succeed");
    }

    #[kani::proof]
    fn proof_precondition_check_false_err() {
        let pre = Precondition::new("test");
        let result = pre.check(false);
        kani::assert(result.is_err(), "false condition must fail");
    }

    #[kani::proof]
    fn proof_precondition_require_some_ok() {
        let value: u8 = kani::any();
        let pre = Precondition::new("test");
        let result = pre.require(Some(value));
        kani::assert(result.is_ok(), "Some must succeed");
        kani::assert(result.unwrap() == value, "must return value");
    }

    #[kani::proof]
    fn proof_precondition_require_none_err() {
        let pre = Precondition::new("test");
        let result = pre.require::<u8>(None);
        kani::assert(result.is_err(), "None must fail");
    }

    #[kani::proof]
    fn proof_precondition_require_in_range_ok() {
        let value: u8 = kani::any();
        let min: u8 = kani::any();
        let max: u8 = kani::any();
        kani::assume(min <= max);
        kani::assume(value >= min && value <= max);

        let pre = Precondition::new("test");
        let result = pre.require_in_range(value, min, max);
        kani::assert(result.is_ok(), "in-range value must succeed");
    }

    #[kani::proof]
    fn proof_precondition_require_in_range_err() {
        let value: u8 = kani::any();
        let min: u8 = kani::any();
        let max: u8 = kani::any();
        kani::assume(min <= max);
        kani::assume(value < min || value > max);

        let pre = Precondition::new("test");
        let result = pre.require_in_range(value, min, max);
        kani::assert(result.is_err(), "out-of-range value must fail");
    }

    // ========================================================================
    // Postcondition Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_postcondition_check_true_ok() {
        let post = Postcondition::new("test");
        let result = post.check(true);
        kani::assert(result.is_ok(), "true condition must succeed");
    }

    #[kani::proof]
    fn proof_postcondition_check_false_err() {
        let post = Postcondition::new("test");
        let result = post.check(false);
        kani::assert(result.is_err(), "false condition must fail");
    }

    #[kani::proof]
    fn proof_postcondition_ensure_pass() {
        let value: u8 = kani::any();
        let post = Postcondition::new("test");
        let result = post.ensure(value, |_| true);
        kani::assert(result.is_ok(), "true predicate must succeed");
        kani::assert(result.unwrap() == value, "must return value");
    }

    #[kani::proof]
    fn proof_postcondition_ensure_fail() {
        let value: u8 = kani::any();
        let post = Postcondition::new("test");
        let result = post.ensure(value, |_| false);
        kani::assert(result.is_err(), "false predicate must fail");
    }

    // ========================================================================
    // AssertionCollector Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_collector_new_empty() {
        let collector = AssertionCollector::new();
        kani::assert(collector.pass_count() == 0, "pass_count must be 0");
        kani::assert(collector.fail_count() == 0, "fail_count must be 0");
        kani::assert(collector.total_count() == 0, "total_count must be 0");
        kani::assert(collector.all_passed(), "all_passed must be true when empty");
    }

    #[kani::proof]
    fn proof_collector_default_empty() {
        let collector = AssertionCollector::default();
        kani::assert(collector.total_count() == 0, "default must be empty");
    }

    #[kani::proof]
    fn proof_collector_add_passed_increments_pass() {
        let collector = AssertionCollector::new();
        collector.add(AssertionResult::passed("test"));
        kani::assert(collector.pass_count() == 1, "pass_count must be 1");
        kani::assert(collector.fail_count() == 0, "fail_count must be 0");
    }

    #[kani::proof]
    fn proof_collector_add_failed_increments_fail() {
        let collector = AssertionCollector::new();
        collector.add(AssertionResult::failed("test"));
        kani::assert(collector.pass_count() == 0, "pass_count must be 0");
        kani::assert(collector.fail_count() == 1, "fail_count must be 1");
    }

    #[kani::proof]
    fn proof_collector_total_is_sum() {
        let collector = AssertionCollector::new();
        collector.add(AssertionResult::passed("p1"));
        collector.add(AssertionResult::failed("f1"));
        collector.add(AssertionResult::passed("p2"));

        kani::assert(
            collector.total_count() == collector.pass_count() + collector.fail_count(),
            "total must equal pass + fail",
        );
    }

    #[kani::proof]
    fn proof_collector_all_passed_false_with_failures() {
        let collector = AssertionCollector::new();
        collector.add(AssertionResult::passed("p1"));
        collector.add(AssertionResult::failed("f1"));

        kani::assert(
            !collector.all_passed(),
            "all_passed must be false with failures",
        );
    }

    #[kani::proof]
    fn proof_collector_all_passed_true_no_failures() {
        let collector = AssertionCollector::new();
        collector.add(AssertionResult::passed("p1"));
        collector.add(AssertionResult::passed("p2"));

        kani::assert(
            collector.all_passed(),
            "all_passed must be true without failures",
        );
    }
}
