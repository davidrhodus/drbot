//! Closure utilities for drbot.
//!
//! This crate provides:
//! - Closure boxing
//! - Closure composition
//! - Callable traits

use std::sync::Arc;
use thiserror::Error;

/// Closure error types.
#[derive(Error, Debug, Clone)]
pub enum ClosureError {
    #[error("Closure execution failed")]
    ExecutionFailed,
}

/// Result type for closure operations.
pub type Result<T> = std::result::Result<T, ClosureError>;

/// A boxed callable.
pub type BoxFn<A, B> = Box<dyn Fn(A) -> B + Send + Sync>;

/// A boxed mutable callable.
pub type BoxFnMut<A, B> = Box<dyn FnMut(A) -> B + Send>;

/// A boxed once callable.
pub type BoxFnOnce<A, B> = Box<dyn FnOnce(A) -> B + Send>;

/// An Arc callable.
pub type ArcFn<A, B> = Arc<dyn Fn(A) -> B + Send + Sync>;

/// Create boxed function.
pub fn boxed<A, B, F>(f: F) -> BoxFn<A, B>
where
    F: Fn(A) -> B + Send + Sync + 'static,
{
    Box::new(f)
}

/// Create Arc function.
pub fn arc_fn<A, B, F>(f: F) -> ArcFn<A, B>
where
    F: Fn(A) -> B + Send + Sync + 'static,
{
    Arc::new(f)
}

/// A stored closure that can be called multiple times.
pub struct StoredClosure<A, B> {
    inner: BoxFn<A, B>,
}

impl<A, B> StoredClosure<A, B> {
    /// Create from function.
    pub fn new<F>(f: F) -> Self
    where
        F: Fn(A) -> B + Send + Sync + 'static,
    {
        Self { inner: Box::new(f) }
    }

    /// Call the closure.
    pub fn call(&self, arg: A) -> B {
        (self.inner)(arg)
    }
}

/// A closure that can only be called once.
pub struct OnceClosure<A, B> {
    inner: Option<BoxFnOnce<A, B>>,
}

impl<A, B> OnceClosure<A, B> {
    /// Create from function.
    pub fn new<F>(f: F) -> Self
    where
        F: FnOnce(A) -> B + Send + 'static,
    {
        Self {
            inner: Some(Box::new(f)),
        }
    }

    /// Call the closure (consumes it).
    pub fn call(mut self, arg: A) -> Option<B> {
        self.inner.take().map(|f| f(arg))
    }

    /// Check if not yet called.
    pub fn is_pending(&self) -> bool {
        self.inner.is_some()
    }
}

/// A closure with state.
pub struct StatefulClosure<S, A, B, F>
where
    F: Fn(&mut S, A) -> B,
{
    state: S,
    func: F,
    _marker: std::marker::PhantomData<(A, B)>,
}

impl<S, A, B, F> StatefulClosure<S, A, B, F>
where
    F: Fn(&mut S, A) -> B,
{
    /// Create with initial state.
    pub fn new(state: S, func: F) -> Self {
        Self {
            state,
            func,
            _marker: std::marker::PhantomData,
        }
    }

    /// Call with argument.
    pub fn call(&mut self, arg: A) -> B {
        (self.func)(&mut self.state, arg)
    }

    /// Get state reference.
    pub fn state(&self) -> &S {
        &self.state
    }

    /// Get mutable state reference.
    pub fn state_mut(&mut self) -> &mut S {
        &mut self.state
    }
}

/// A counter closure.
pub struct Counter {
    count: usize,
}

impl Counter {
    /// Create new counter.
    pub fn new() -> Self {
        Self { count: 0 }
    }

    /// Create with initial value.
    pub fn with_value(value: usize) -> Self {
        Self { count: value }
    }

    /// Increment and return new value.
    pub fn increment(&mut self) -> usize {
        self.count += 1;
        self.count
    }

    /// Get current count.
    pub fn get(&self) -> usize {
        self.count
    }

    /// Reset counter.
    pub fn reset(&mut self) {
        self.count = 0;
    }
}

impl Default for Counter {
    fn default() -> Self {
        Self::new()
    }
}

/// A closure that tracks calls.
pub struct TrackedClosure<A, B, F>
where
    F: Fn(A) -> B,
{
    func: F,
    call_count: usize,
    _marker: std::marker::PhantomData<(A, B)>,
}

impl<A, B, F> TrackedClosure<A, B, F>
where
    F: Fn(A) -> B,
{
    /// Create new tracked closure.
    pub fn new(func: F) -> Self {
        Self {
            func,
            call_count: 0,
            _marker: std::marker::PhantomData,
        }
    }

    /// Call the closure.
    pub fn call(&mut self, arg: A) -> B {
        self.call_count += 1;
        (self.func)(arg)
    }

    /// Get call count.
    pub fn call_count(&self) -> usize {
        self.call_count
    }

    /// Reset call count.
    pub fn reset_count(&mut self) {
        self.call_count = 0;
    }
}

/// Create a closure that returns incrementing values.
pub fn incrementing(start: usize) -> impl FnMut() -> usize {
    let mut current = start;
    move || {
        let value = current;
        current += 1;
        value
    }
}

/// Create a closure that alternates between values.
pub fn alternating<T: Clone>(a: T, b: T) -> impl FnMut() -> T {
    let mut use_first = true;
    move || {
        let result = if use_first { a.clone() } else { b.clone() };
        use_first = !use_first;
        result
    }
}

/// Create a closure that cycles through values.
pub fn cycling<T: Clone>(values: Vec<T>) -> impl FnMut() -> T {
    let mut index = 0;
    let len = values.len();
    move || {
        let value = values[index % len].clone();
        index += 1;
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stored_closure() {
        let closure = StoredClosure::new(|x: i32| x * 2);
        assert_eq!(closure.call(5), 10);
        assert_eq!(closure.call(10), 20);
    }

    #[test]
    fn test_once_closure() {
        let closure = OnceClosure::new(|x: i32| x * 2);
        assert!(closure.is_pending());
        let result = closure.call(5);
        assert_eq!(result, Some(10));
    }

    #[test]
    fn test_stateful_closure() {
        let mut closure = StatefulClosure::new(0, |state: &mut i32, x: i32| {
            *state += x;
            *state
        });

        assert_eq!(closure.call(5), 5);
        assert_eq!(closure.call(3), 8);
        assert_eq!(*closure.state(), 8);
    }

    #[test]
    fn test_counter() {
        let mut counter = Counter::new();
        assert_eq!(counter.get(), 0);
        assert_eq!(counter.increment(), 1);
        assert_eq!(counter.increment(), 2);
    }

    #[test]
    fn test_tracked_closure() {
        let mut closure = TrackedClosure::new(|x: i32| x * 2);
        assert_eq!(closure.call_count(), 0);

        closure.call(5);
        closure.call(10);
        assert_eq!(closure.call_count(), 2);
    }

    #[test]
    fn test_incrementing() {
        let mut inc = incrementing(0);
        assert_eq!(inc(), 0);
        assert_eq!(inc(), 1);
        assert_eq!(inc(), 2);
    }

    #[test]
    fn test_alternating() {
        let mut alt = alternating(true, false);
        assert!(alt());
        assert!(!alt());
        assert!(alt());
    }

    #[test]
    fn test_cycling() {
        let mut cyc = cycling(vec![1, 2, 3]);
        assert_eq!(cyc(), 1);
        assert_eq!(cyc(), 2);
        assert_eq!(cyc(), 3);
        assert_eq!(cyc(), 1);
    }
}
