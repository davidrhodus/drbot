//! Currying utilities for drbot.
//!
//! This crate provides:
//! - Function currying
//! - Partial application helpers
//! - Argument binding

use thiserror::Error;

/// Curry error types.
#[derive(Error, Debug, Clone)]
pub enum CurryError {
    #[error("Invalid arity")]
    InvalidArity,
}

/// Result type for curry operations.
pub type Result<T> = std::result::Result<T, CurryError>;

/// A curried 2-arg function.
pub struct Curry2<A, B, C, F>
where
    F: Fn(A, B) -> C,
{
    func: F,
    _marker: std::marker::PhantomData<(A, B, C)>,
}

impl<A, B, C, F> Curry2<A, B, C, F>
where
    F: Fn(A, B) -> C + Clone,
    A: Clone,
{
    /// Create curried function.
    pub fn new(func: F) -> Self {
        Self {
            func,
            _marker: std::marker::PhantomData,
        }
    }

    /// Apply first argument.
    pub fn apply(&self, a: A) -> Curry2Applied<A, B, C, F> {
        Curry2Applied {
            func: self.func.clone(),
            first: a,
            _marker: std::marker::PhantomData,
        }
    }

    /// Call with both arguments.
    pub fn call(&self, a: A, b: B) -> C {
        (self.func)(a, b)
    }
}

/// A curried 2-arg function with first argument applied.
pub struct Curry2Applied<A, B, C, F>
where
    F: Fn(A, B) -> C,
{
    func: F,
    first: A,
    _marker: std::marker::PhantomData<(B, C)>,
}

impl<A, B, C, F> Curry2Applied<A, B, C, F>
where
    F: Fn(A, B) -> C,
    A: Clone,
{
    /// Call with second argument.
    pub fn call(&self, b: B) -> C {
        (self.func)(self.first.clone(), b)
    }
}

/// Create curry2 wrapper.
pub fn curry2<A, B, C, F>(f: F) -> Curry2<A, B, C, F>
where
    F: Fn(A, B) -> C + Clone,
    A: Clone,
{
    Curry2::new(f)
}

/// Bind first argument.
pub fn bind1<A, B, C, F>(f: F, a: A) -> impl Fn(B) -> C
where
    F: Fn(A, B) -> C,
    A: Clone,
{
    move |b| f(a.clone(), b)
}

/// Bind second argument.
pub fn bind2<A, B, C, F>(f: F, b: B) -> impl Fn(A) -> C
where
    F: Fn(A, B) -> C,
    B: Clone,
{
    move |a| f(a, b.clone())
}

/// Bind first argument of 3-arg function.
pub fn bind1_3<A, B, C, D, F>(f: F, a: A) -> impl Fn(B, C) -> D
where
    F: Fn(A, B, C) -> D,
    A: Clone,
{
    move |b, c| f(a.clone(), b, c)
}

/// Bind first two arguments of 3-arg function.
pub fn bind12_3<A, B, C, D, F>(f: F, a: A, b: B) -> impl Fn(C) -> D
where
    F: Fn(A, B, C) -> D,
    A: Clone,
    B: Clone,
{
    move |c| f(a.clone(), b.clone(), c)
}

/// Apply arguments from tuple.
pub fn apply_tuple2<A, B, C, F>(f: F, args: (A, B)) -> C
where
    F: Fn(A, B) -> C,
{
    f(args.0, args.1)
}

/// Apply arguments from tuple.
pub fn apply_tuple3<A, B, C, D, F>(f: F, args: (A, B, C)) -> D
where
    F: Fn(A, B, C) -> D,
{
    f(args.0, args.1, args.2)
}

/// Spread tuple arguments.
pub fn spread2<A, B, C, F>(f: F) -> impl Fn((A, B)) -> C
where
    F: Fn(A, B) -> C,
{
    move |(a, b)| f(a, b)
}

/// Spread tuple arguments.
pub fn spread3<A, B, C, D, F>(f: F) -> impl Fn((A, B, C)) -> D
where
    F: Fn(A, B, C) -> D,
{
    move |(a, b, c)| f(a, b, c)
}

/// Gather arguments into tuple.
pub fn gather2<A, B, C, F>(f: F) -> impl Fn(A, B) -> C
where
    F: Fn((A, B)) -> C,
{
    move |a, b| f((a, b))
}

/// Uncurry - convert curried to regular.
pub fn uncurry2<A, B, C, F>(curry: &Curry2<A, B, C, F>) -> impl Fn(A, B) -> C + '_
where
    F: Fn(A, B) -> C + Clone,
    A: Clone,
{
    move |a, b| curry.call(a, b)
}

/// Flip function arguments.
pub fn flip<A, B, C, F>(f: F) -> impl Fn(B, A) -> C
where
    F: Fn(A, B) -> C,
{
    move |b, a| f(a, b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_curry2() {
        let add = |a: i32, b: i32| a + b;
        let curried = curry2(add);
        let add5 = curried.apply(5);
        assert_eq!(add5.call(3), 8);
    }

    #[test]
    fn test_bind1() {
        let sub = |a: i32, b: i32| a - b;
        let sub_from_10 = bind1(sub, 10);
        assert_eq!(sub_from_10(3), 7);
    }

    #[test]
    fn test_bind2() {
        let sub = |a: i32, b: i32| a - b;
        let sub_3 = bind2(sub, 3);
        assert_eq!(sub_3(10), 7);
    }

    #[test]
    fn test_spread2() {
        let add = |a: i32, b: i32| a + b;
        let spread_add = spread2(add);
        assert_eq!(spread_add((5, 3)), 8);
    }

    #[test]
    fn test_apply_tuple() {
        let add = |a: i32, b: i32| a + b;
        assert_eq!(apply_tuple2(add, (5, 3)), 8);
    }

    #[test]
    fn test_flip() {
        let sub = |a: i32, b: i32| a - b;
        let flipped = flip(sub);
        assert_eq!(sub(10, 3), 7);
        assert_eq!(flipped(10, 3), -7);
    }
}
