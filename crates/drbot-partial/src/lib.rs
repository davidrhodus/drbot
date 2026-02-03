//! Partial application utilities for drbot.
//!
//! This crate provides:
//! - Partial application
//! - Placeholder arguments
//! - Argument reordering

use thiserror::Error;

/// Partial error types.
#[derive(Error, Debug, Clone)]
pub enum PartialError {
    #[error("Missing argument")]
    MissingArgument,
}

/// Result type for partial operations.
pub type Result<T> = std::result::Result<T, PartialError>;

/// Create partial with first argument.
pub fn partial1<A, B, C, F>(f: F, a: A) -> impl Fn(B) -> C
where
    F: Fn(A, B) -> C,
    A: Clone,
{
    move |b| f(a.clone(), b)
}

/// Create partial with second argument.
pub fn partial2<A, B, C, F>(f: F, b: B) -> impl Fn(A) -> C
where
    F: Fn(A, B) -> C,
    B: Clone,
{
    move |a| f(a, b.clone())
}

/// Partial application for 3-arg functions.
pub fn partial1_3<A, B, C, D, F>(f: F, a: A) -> impl Fn(B, C) -> D
where
    F: Fn(A, B, C) -> D,
    A: Clone,
{
    move |b, c| f(a.clone(), b, c)
}

/// Partial with first two of 3 args.
pub fn partial12_3<A, B, C, D, F>(f: F, a: A, b: B) -> impl Fn(C) -> D
where
    F: Fn(A, B, C) -> D,
    A: Clone,
    B: Clone,
{
    move |c| f(a.clone(), b.clone(), c)
}

/// Partial with first and third of 3 args.
pub fn partial13_3<A, B, C, D, F>(f: F, a: A, c: C) -> impl Fn(B) -> D
where
    F: Fn(A, B, C) -> D,
    A: Clone,
    C: Clone,
{
    move |b| f(a.clone(), b, c.clone())
}

/// Reorder arguments.
pub fn reorder21<A, B, C, F>(f: F) -> impl Fn(B, A) -> C
where
    F: Fn(A, B) -> C,
{
    move |b, a| f(a, b)
}

/// Reorder 3 arguments.
pub fn reorder321<A, B, C, D, F>(f: F) -> impl Fn(C, B, A) -> D
where
    F: Fn(A, B, C) -> D,
{
    move |c, b, a| f(a, b, c)
}

/// Reorder 3 arguments (rotate left).
pub fn reorder231<A, B, C, D, F>(f: F) -> impl Fn(B, C, A) -> D
where
    F: Fn(A, B, C) -> D,
{
    move |b, c, a| f(a, b, c)
}

/// Partial application with stored state (2-arg to 1-arg).
pub struct Applied1Of2<A, B, C, F>
where
    F: Fn(A, B) -> C,
{
    first: A,
    func: F,
    _marker: std::marker::PhantomData<(B, C)>,
}

impl<A: Clone, B, C, F: Fn(A, B) -> C> Applied1Of2<A, B, C, F> {
    /// Create with first argument.
    pub fn new(func: F, first: A) -> Self {
        Self {
            first,
            func,
            _marker: std::marker::PhantomData,
        }
    }

    /// Get first argument.
    pub fn first(&self) -> &A {
        &self.first
    }

    /// Call with second argument.
    pub fn call(&self, second: B) -> C {
        (self.func)(self.first.clone(), second)
    }
}

/// Apply first argument of 2-arg function.
pub fn apply1of2<A, B, C, F>(func: F, first: A) -> Applied1Of2<A, B, C, F>
where
    A: Clone,
    F: Fn(A, B) -> C,
{
    Applied1Of2::new(func, first)
}

/// Partial application with two stored arguments (3-arg to 1-arg).
pub struct Applied2Of3<A, B, C, D, F>
where
    F: Fn(A, B, C) -> D,
{
    first: A,
    second: B,
    func: F,
    _marker: std::marker::PhantomData<(C, D)>,
}

impl<A: Clone, B: Clone, C, D, F: Fn(A, B, C) -> D> Applied2Of3<A, B, C, D, F> {
    /// Create with first two arguments.
    pub fn new(func: F, first: A, second: B) -> Self {
        Self {
            first,
            second,
            func,
            _marker: std::marker::PhantomData,
        }
    }

    /// Call with third argument.
    pub fn call(&self, third: C) -> D {
        (self.func)(self.first.clone(), self.second.clone(), third)
    }
}

/// Apply first two arguments of 3-arg function.
pub fn apply2of3<A, B, C, D, F>(func: F, first: A, second: B) -> Applied2Of3<A, B, C, D, F>
where
    A: Clone,
    B: Clone,
    F: Fn(A, B, C) -> D,
{
    Applied2Of3::new(func, first, second)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_partial1() {
        let sub = |a: i32, b: i32| a - b;
        let sub_from_10 = partial1(sub, 10);
        assert_eq!(sub_from_10(3), 7);
    }

    #[test]
    fn test_partial2() {
        let sub = |a: i32, b: i32| a - b;
        let sub_3 = partial2(sub, 3);
        assert_eq!(sub_3(10), 7);
    }

    #[test]
    fn test_applied1of2() {
        let sub = |a: i32, b: i32| a - b;
        let applied = apply1of2(sub, 10);
        assert_eq!(applied.call(3), 7);
    }

    #[test]
    fn test_applied2of3() {
        let f = |a: i32, b: i32, c: i32| a + b + c;
        let applied = apply2of3(f, 1, 2);
        assert_eq!(applied.call(3), 6);
    }

    #[test]
    fn test_reorder21() {
        let sub = |a: i32, b: i32| a - b;
        let reordered = reorder21(sub);
        assert_eq!(sub(10, 3), 7);
        assert_eq!(reordered(10, 3), -7);
    }

    #[test]
    fn test_partial12_3() {
        let f = |a: i32, b: i32, c: i32| a + b + c;
        let partial = partial12_3(f, 1, 2);
        assert_eq!(partial(3), 6);
    }
}
