//! Mocking utilities for drbot testing.
//!
//! This crate provides:
//! - Mock call tracking
//! - Expectation setting
//! - Return value configuration
//! - Verification

use std::any::Any;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use thiserror::Error;

/// Mock error types.
#[derive(Error, Debug)]
pub enum MockError {
    #[error("Unexpected call: {0}")]
    UnexpectedCall(String),

    #[error("Missing return value for: {0}")]
    MissingReturnValue(String),

    #[error("Verification failed: {0}")]
    VerificationFailed(String),

    #[error("Wrong number of calls: expected {expected}, got {actual}")]
    WrongCallCount { expected: usize, actual: usize },
}

/// Result type for mock operations.
pub type Result<T> = std::result::Result<T, MockError>;

/// Call record.
#[derive(Debug, Clone)]
pub struct CallRecord {
    /// Method name.
    pub method: String,
    /// Call arguments (as strings).
    pub args: Vec<String>,
    /// Call timestamp.
    pub timestamp: std::time::Instant,
}

impl CallRecord {
    /// Create new call record.
    pub fn new(method: impl Into<String>) -> Self {
        Self {
            method: method.into(),
            args: Vec::new(),
            timestamp: std::time::Instant::now(),
        }
    }

    /// Add argument.
    pub fn arg(mut self, arg: impl ToString) -> Self {
        self.args.push(arg.to_string());
        self
    }
}

/// Call tracker.
pub struct CallTracker {
    calls: Mutex<Vec<CallRecord>>,
}

impl CallTracker {
    /// Create new tracker.
    pub fn new() -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
        }
    }

    /// Record a call.
    pub fn record(&self, call: CallRecord) {
        self.calls.lock().unwrap().push(call);
    }

    /// Record simple call.
    pub fn record_call(&self, method: &str) {
        self.record(CallRecord::new(method));
    }

    /// Get call count.
    pub fn call_count(&self) -> usize {
        self.calls.lock().unwrap().len()
    }

    /// Get calls for method.
    pub fn calls_for(&self, method: &str) -> Vec<CallRecord> {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .filter(|c| c.method == method)
            .cloned()
            .collect()
    }

    /// Get call count for method.
    pub fn call_count_for(&self, method: &str) -> usize {
        self.calls_for(method).len()
    }

    /// Check if method was called.
    pub fn was_called(&self, method: &str) -> bool {
        self.call_count_for(method) > 0
    }

    /// Verify call count.
    pub fn verify_call_count(&self, method: &str, expected: usize) -> Result<()> {
        let actual = self.call_count_for(method);
        if actual == expected {
            Ok(())
        } else {
            Err(MockError::WrongCallCount { expected, actual })
        }
    }

    /// Get all calls.
    pub fn all_calls(&self) -> Vec<CallRecord> {
        self.calls.lock().unwrap().clone()
    }

    /// Clear all calls.
    pub fn clear(&self) {
        self.calls.lock().unwrap().clear();
    }
}

impl Default for CallTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Return value queue.
pub struct ReturnQueue<T> {
    values: Mutex<VecDeque<T>>,
    default: Mutex<Option<T>>,
}

impl<T: Clone> ReturnQueue<T> {
    /// Create new queue.
    pub fn new() -> Self {
        Self {
            values: Mutex::new(VecDeque::new()),
            default: Mutex::new(None),
        }
    }

    /// Add return value.
    pub fn returns(&self, value: T) {
        self.values.lock().unwrap().push_back(value);
    }

    /// Add multiple return values.
    pub fn returns_many(&self, values: impl IntoIterator<Item = T>) {
        let mut queue = self.values.lock().unwrap();
        for value in values {
            queue.push_back(value);
        }
    }

    /// Set default return value.
    pub fn returns_default(&self, value: T) {
        *self.default.lock().unwrap() = Some(value);
    }

    /// Get next return value.
    pub fn next(&self) -> Option<T> {
        self.values
            .lock()
            .unwrap()
            .pop_front()
            .or_else(|| self.default.lock().unwrap().clone())
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.values.lock().unwrap().is_empty()
    }
}

impl<T: Clone> Default for ReturnQueue<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Expectation for mock calls.
pub struct Expectation {
    method: String,
    min_calls: Option<usize>,
    max_calls: Option<usize>,
    actual_calls: AtomicUsize,
}

impl Expectation {
    /// Create new expectation.
    pub fn new(method: impl Into<String>) -> Self {
        Self {
            method: method.into(),
            min_calls: None,
            max_calls: None,
            actual_calls: AtomicUsize::new(0),
        }
    }

    /// Expect at least n calls.
    pub fn at_least(mut self, n: usize) -> Self {
        self.min_calls = Some(n);
        self
    }

    /// Expect at most n calls.
    pub fn at_most(mut self, n: usize) -> Self {
        self.max_calls = Some(n);
        self
    }

    /// Expect exactly n calls.
    pub fn times(mut self, n: usize) -> Self {
        self.min_calls = Some(n);
        self.max_calls = Some(n);
        self
    }

    /// Expect exactly once.
    pub fn once(self) -> Self {
        self.times(1)
    }

    /// Expect never called.
    pub fn never(self) -> Self {
        self.times(0)
    }

    /// Record a call.
    pub fn record_call(&self) {
        self.actual_calls.fetch_add(1, Ordering::SeqCst);
    }

    /// Get method name.
    pub fn method(&self) -> &str {
        &self.method
    }

    /// Verify expectation.
    pub fn verify(&self) -> Result<()> {
        let actual = self.actual_calls.load(Ordering::SeqCst);

        if let Some(min) = self.min_calls {
            if actual < min {
                return Err(MockError::VerificationFailed(format!(
                    "{}: expected at least {} calls, got {}",
                    self.method, min, actual
                )));
            }
        }

        if let Some(max) = self.max_calls {
            if actual > max {
                return Err(MockError::VerificationFailed(format!(
                    "{}: expected at most {} calls, got {}",
                    self.method, max, actual
                )));
            }
        }

        Ok(())
    }
}

/// Mock context for managing expectations.
pub struct MockContext {
    expectations: Mutex<Vec<Expectation>>,
    tracker: CallTracker,
}

impl MockContext {
    /// Create new context.
    pub fn new() -> Self {
        Self {
            expectations: Mutex::new(Vec::new()),
            tracker: CallTracker::new(),
        }
    }

    /// Add expectation.
    pub fn expect(&self, method: impl Into<String>) -> &Self {
        self.expectations
            .lock()
            .unwrap()
            .push(Expectation::new(method));
        self
    }

    /// Record call.
    pub fn call(&self, method: &str) {
        self.tracker.record_call(method);

        let expectations = self.expectations.lock().unwrap();
        for exp in expectations.iter() {
            if exp.method() == method {
                exp.record_call();
            }
        }
    }

    /// Get call tracker.
    pub fn tracker(&self) -> &CallTracker {
        &self.tracker
    }

    /// Verify all expectations.
    pub fn verify(&self) -> Result<()> {
        let expectations = self.expectations.lock().unwrap();
        for exp in expectations.iter() {
            exp.verify()?;
        }
        Ok(())
    }
}

impl Default for MockContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Type-erased mock value.
pub struct AnyValue(Box<dyn Any + Send + Sync>);

impl AnyValue {
    /// Create new value.
    pub fn new<T: Any + Send + Sync>(value: T) -> Self {
        Self(Box::new(value))
    }

    /// Downcast to type.
    pub fn downcast<T: Any>(self) -> Option<T> {
        self.0.downcast::<T>().ok().map(|b| *b)
    }

    /// Downcast ref.
    pub fn downcast_ref<T: Any>(&self) -> Option<&T> {
        self.0.downcast_ref()
    }
}

/// Simple mock function.
pub struct MockFn<R> {
    tracker: Arc<CallTracker>,
    returns: Arc<ReturnQueue<R>>,
}

impl<R: Clone + Send + Sync + 'static> MockFn<R> {
    /// Create new mock function.
    pub fn new() -> Self {
        Self {
            tracker: Arc::new(CallTracker::new()),
            returns: Arc::new(ReturnQueue::new()),
        }
    }

    /// Configure return value.
    pub fn returns(&self, value: R) -> &Self {
        self.returns.returns(value);
        self
    }

    /// Configure default return value.
    pub fn returns_default(&self, value: R) -> &Self {
        self.returns.returns_default(value);
        self
    }

    /// Call the mock.
    pub fn call(&self) -> Option<R> {
        self.tracker.record_call("call");
        self.returns.next()
    }

    /// Get call count.
    pub fn call_count(&self) -> usize {
        self.tracker.call_count()
    }

    /// Verify call count.
    pub fn verify_called(&self, times: usize) -> Result<()> {
        self.tracker.verify_call_count("call", times)
    }

    /// Check if was called.
    pub fn was_called(&self) -> bool {
        self.tracker.was_called("call")
    }
}

impl<R: Clone + Send + Sync + 'static> Default for MockFn<R> {
    fn default() -> Self {
        Self::new()
    }
}

impl<R: Clone + Send + Sync + 'static> Clone for MockFn<R> {
    fn clone(&self) -> Self {
        Self {
            tracker: self.tracker.clone(),
            returns: self.returns.clone(),
        }
    }
}

/// Spy that wraps a real implementation.
pub struct Spy<T> {
    inner: T,
    tracker: CallTracker,
}

impl<T> Spy<T> {
    /// Create new spy.
    pub fn new(inner: T) -> Self {
        Self {
            inner,
            tracker: CallTracker::new(),
        }
    }

    /// Get inner reference.
    pub fn inner(&self) -> &T {
        &self.inner
    }

    /// Get inner mutable reference.
    pub fn inner_mut(&mut self) -> &mut T {
        &mut self.inner
    }

    /// Record a call.
    pub fn record(&self, method: &str) {
        self.tracker.record_call(method);
    }

    /// Get tracker.
    pub fn tracker(&self) -> &CallTracker {
        &self.tracker
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_call_tracker() {
        let tracker = CallTracker::new();

        tracker.record_call("foo");
        tracker.record_call("bar");
        tracker.record_call("foo");

        assert_eq!(tracker.call_count(), 3);
        assert_eq!(tracker.call_count_for("foo"), 2);
        assert_eq!(tracker.call_count_for("bar"), 1);
        assert!(tracker.was_called("foo"));
        assert!(!tracker.was_called("baz"));
    }

    #[test]
    fn test_return_queue() {
        let queue: ReturnQueue<i32> = ReturnQueue::new();

        queue.returns(1);
        queue.returns(2);
        queue.returns_default(0);

        assert_eq!(queue.next(), Some(1));
        assert_eq!(queue.next(), Some(2));
        assert_eq!(queue.next(), Some(0));
        assert_eq!(queue.next(), Some(0));
    }

    #[test]
    fn test_expectation() {
        let exp = Expectation::new("test").at_least(2).at_most(5);

        exp.record_call();
        exp.record_call();
        exp.record_call();

        assert!(exp.verify().is_ok());
    }

    #[test]
    fn test_expectation_fail() {
        let exp = Expectation::new("test").times(2);

        exp.record_call();

        assert!(exp.verify().is_err());
    }

    #[test]
    fn test_mock_fn() {
        let mock: MockFn<i32> = MockFn::new();
        mock.returns(42);
        mock.returns(43);

        assert_eq!(mock.call(), Some(42));
        assert_eq!(mock.call(), Some(43));
        assert_eq!(mock.call_count(), 2);
    }
}
