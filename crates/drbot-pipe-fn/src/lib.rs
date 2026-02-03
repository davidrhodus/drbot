//! Function piping utilities for drbot.
//!
//! This crate provides:
//! - Value piping
//! - Function pipelines
//! - Chained transformations

use thiserror::Error;

/// Pipe error types.
#[derive(Error, Debug, Clone)]
pub enum PipeError {
    #[error("Pipeline failed")]
    PipelineFailed,
}

/// Result type for pipe operations.
pub type Result<T> = std::result::Result<T, PipeError>;

/// Extension trait for piping values through functions.
pub trait Pipe: Sized {
    /// Pipe value through function.
    fn pipe<B, F: FnOnce(Self) -> B>(self, f: F) -> B {
        f(self)
    }

    /// Pipe value through function if condition is true.
    fn pipe_if<F: FnOnce(Self) -> Self>(self, cond: bool, f: F) -> Self {
        if cond {
            f(self)
        } else {
            self
        }
    }

    /// Pipe value through function returning Option.
    fn pipe_opt<B, F: FnOnce(Self) -> Option<B>>(self, f: F) -> Option<B> {
        f(self)
    }

    /// Pipe value through function returning Result.
    fn pipe_res<B, E, F: FnOnce(Self) -> std::result::Result<B, E>>(
        self,
        f: F,
    ) -> std::result::Result<B, E> {
        f(self)
    }

    /// Pipe reference through function.
    fn pipe_ref<B, F: FnOnce(&Self) -> B>(&self, f: F) -> B {
        f(self)
    }

    /// Pipe mutable reference through function.
    fn pipe_mut<B, F: FnOnce(&mut Self) -> B>(&mut self, f: F) -> B {
        f(self)
    }

    /// Also execute side effect.
    fn also<F: FnOnce(&Self)>(self, f: F) -> Self {
        f(&self);
        self
    }

    /// Also execute mutable side effect.
    fn also_mut<F: FnOnce(&mut Self)>(mut self, f: F) -> Self {
        f(&mut self);
        self
    }
}

impl<T> Pipe for T {}

/// A function pipeline.
pub struct Pipeline<T> {
    value: T,
}

impl<T> Pipeline<T> {
    /// Create new pipeline.
    pub fn new(value: T) -> Self {
        Self { value }
    }

    /// Add transformation.
    pub fn then<U, F: FnOnce(T) -> U>(self, f: F) -> Pipeline<U> {
        Pipeline {
            value: f(self.value),
        }
    }

    /// Add conditional transformation.
    pub fn then_if<F: FnOnce(T) -> T>(self, cond: bool, f: F) -> Self {
        if cond {
            Pipeline {
                value: f(self.value),
            }
        } else {
            self
        }
    }

    /// Add transformation returning Option.
    pub fn then_opt<U, F: FnOnce(T) -> Option<U>>(self, f: F) -> Option<Pipeline<U>> {
        f(self.value).map(|value| Pipeline { value })
    }

    /// Inspect value without consuming.
    pub fn inspect<F: FnOnce(&T)>(self, f: F) -> Self {
        f(&self.value);
        self
    }

    /// Get final value.
    pub fn finish(self) -> T {
        self.value
    }
}

/// Create a pipeline from value.
pub fn pipe<T>(value: T) -> Pipeline<T> {
    Pipeline::new(value)
}

/// Chain multiple functions together.
pub fn chain<A, B, C, F, G>(f: F, g: G) -> impl Fn(A) -> C
where
    F: Fn(A) -> B,
    G: Fn(B) -> C,
{
    move |a| g(f(a))
}

/// Chain three functions.
pub fn chain3<A, B, C, D, F, G, H>(f: F, g: G, h: H) -> impl Fn(A) -> D
where
    F: Fn(A) -> B,
    G: Fn(B) -> C,
    H: Fn(C) -> D,
{
    move |a| h(g(f(a)))
}

/// Chain four functions.
pub fn chain4<A, B, C, D, E, F, G, H, I>(f: F, g: G, h: H, i: I) -> impl Fn(A) -> E
where
    F: Fn(A) -> B,
    G: Fn(B) -> C,
    H: Fn(C) -> D,
    I: Fn(D) -> E,
{
    move |a| i(h(g(f(a))))
}

/// Flow - left to right function composition (alias for chain).
pub fn flow<A, B, C, F, G>(f: F, g: G) -> impl Fn(A) -> C
where
    F: Fn(A) -> B,
    G: Fn(B) -> C,
{
    chain(f, g)
}

/// Compose - right to left function composition.
pub fn compose<A, B, C, F, G>(f: F, g: G) -> impl Fn(A) -> C
where
    F: Fn(B) -> C,
    G: Fn(A) -> B,
{
    move |a| f(g(a))
}

/// Build a function chain starting with identity.
pub fn identity<T>(x: T) -> T {
    x
}

/// Create an identity chain.
pub fn identity_chain<T>() -> impl Fn(T) -> T {
    identity
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pipe() {
        let result = 5.pipe(|x| x * 2).pipe(|x| x + 1);
        assert_eq!(result, 11);
    }

    #[test]
    fn test_pipe_if() {
        let result = 5.pipe_if(true, |x| x * 2);
        assert_eq!(result, 10);

        let result = 5.pipe_if(false, |x| x * 2);
        assert_eq!(result, 5);
    }

    #[test]
    fn test_pipe_ref() {
        let vec = vec![1, 2, 3];
        let len = vec.pipe_ref(|v| v.len());
        assert_eq!(len, 3);
    }

    #[test]
    fn test_also() {
        let mut side_effect = 0;
        let result = 42.also(|x| {
            side_effect = *x;
        });
        assert_eq!(result, 42);
        assert_eq!(side_effect, 42);
    }

    #[test]
    fn test_pipeline() {
        let result = pipe(5)
            .then(|x| x * 2)
            .then(|x| x + 1)
            .inspect(|x| println!("value: {}", x))
            .finish();
        assert_eq!(result, 11);
    }

    #[test]
    fn test_chain() {
        let double = |x: i32| x * 2;
        let add_one = |x: i32| x + 1;
        let chained = chain(double, add_one);
        assert_eq!(chained(5), 11);
    }

    #[test]
    fn test_compose() {
        let double = |x: i32| x * 2;
        let add_one = |x: i32| x + 1;
        let composed = compose(add_one, double);
        assert_eq!(composed(5), 11); // double(5) = 10, then add_one(10) = 11
    }
}
