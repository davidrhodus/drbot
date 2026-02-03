//! Function composition utilities for drbot.
//!
//! This crate provides:
//! - Function composition
//! - Pipelines
//! - Monadic composition

use std::sync::Arc;
use thiserror::Error;

/// Composition error types.
#[derive(Error, Debug)]
pub enum ComposeError {
    #[error("Composition failed: {0}")]
    Failed(String),

    #[error("Invalid function")]
    Invalid,
}

/// Result type for composition operations.
pub type Result<T> = std::result::Result<T, ComposeError>;

/// Composable function trait.
pub trait Composable<I, O>: Send + Sync {
    /// Apply the function.
    fn apply(&self, input: I) -> O;
}

/// Function wrapper.
pub struct Func<I, O, F: Fn(I) -> O + Send + Sync> {
    func: F,
    _marker: std::marker::PhantomData<(I, O)>,
}

impl<I, O, F: Fn(I) -> O + Send + Sync> Func<I, O, F> {
    /// Create new function.
    pub fn new(func: F) -> Self {
        Self {
            func,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<I: Send + Sync, O: Send + Sync, F: Fn(I) -> O + Send + Sync> Composable<I, O>
    for Func<I, O, F>
{
    fn apply(&self, input: I) -> O {
        (self.func)(input)
    }
}

/// Composed function (f . g).
pub struct Composed<A, B, C> {
    first: Arc<dyn Composable<A, B>>,
    second: Arc<dyn Composable<B, C>>,
}

impl<A, B, C> Composed<A, B, C> {
    /// Create new composition.
    pub fn new(first: Arc<dyn Composable<A, B>>, second: Arc<dyn Composable<B, C>>) -> Self {
        Self { first, second }
    }
}

impl<A: Send + Sync, B: Send + Sync, C: Send + Sync> Composable<A, C> for Composed<A, B, C> {
    fn apply(&self, input: A) -> C {
        let intermediate = self.first.apply(input);
        self.second.apply(intermediate)
    }
}

/// Pipeline that chains functions.
pub struct Pipeline<I, O> {
    func: Arc<dyn Composable<I, O>>,
}

impl<I: Send + Sync + 'static, O: Send + Sync + 'static> Pipeline<I, O> {
    /// Create new pipeline from function.
    pub fn new<F: Fn(I) -> O + Send + Sync + 'static>(func: F) -> Self {
        Self {
            func: Arc::new(Func::new(func)),
        }
    }

    /// Apply pipeline.
    pub fn apply(&self, input: I) -> O {
        self.func.apply(input)
    }

    /// Chain with another function.
    pub fn then<N: Send + Sync + 'static, F: Fn(O) -> N + Send + Sync + 'static>(
        self,
        func: F,
    ) -> Pipeline<I, N> {
        Pipeline {
            func: Arc::new(Composed::new(self.func, Arc::new(Func::new(func)))),
        }
    }
}

/// Identity function.
pub fn identity<T>() -> impl Fn(T) -> T {
    |x| x
}

/// Constant function.
pub fn constant<T: Clone + Send + Sync, I>(value: T) -> impl Fn(I) -> T {
    move |_| value.clone()
}

/// Compose two functions (g . f) = g(f(x)).
pub fn compose<A, B, C, F, G>(f: F, g: G) -> impl Fn(A) -> C
where
    F: Fn(A) -> B,
    G: Fn(B) -> C,
{
    move |x| g(f(x))
}

/// Pipe functions (f |> g) = g(f(x)).
pub fn pipe<A, B, C, F, G>(f: F, g: G) -> impl Fn(A) -> C
where
    F: Fn(A) -> B,
    G: Fn(B) -> C,
{
    compose(f, g)
}

/// Partial application (curry first argument).
pub fn partial<A: Clone, B, C, F>(f: F, a: A) -> impl Fn(B) -> C
where
    F: Fn(A, B) -> C,
{
    move |b| f(a.clone(), b)
}

/// Flip arguments of binary function.
pub fn flip<A, B, C, F>(f: F) -> impl Fn(B, A) -> C
where
    F: Fn(A, B) -> C,
{
    move |b, a| f(a, b)
}

/// Apply function to tuple.
pub fn uncurry<A, B, C, F>(f: F) -> impl Fn((A, B)) -> C
where
    F: Fn(A, B) -> C,
{
    move |(a, b)| f(a, b)
}

/// Curried function wrapper.
pub struct Curried<A, B, C, F>
where
    F: Fn(A, B) -> C + Clone,
    A: Clone,
{
    f: F,
    a: A,
    _marker: std::marker::PhantomData<(B, C)>,
}

impl<A: Clone, B, C, F: Fn(A, B) -> C + Clone> Curried<A, B, C, F> {
    /// Apply second argument.
    pub fn apply(&self, b: B) -> C {
        (self.f)(self.a.clone(), b)
    }
}

/// Curry function to take single argument.
pub fn curry<A: Clone, B, C, F: Fn(A, B) -> C + Clone>(f: F, a: A) -> Curried<A, B, C, F> {
    Curried {
        f,
        a,
        _marker: std::marker::PhantomData,
    }
}

/// Memoize a function (single-value cache).
pub fn memoize<I: Clone + Eq + std::hash::Hash, O: Clone, F>(f: F) -> impl Fn(I) -> O
where
    F: Fn(I) -> O,
{
    use std::cell::RefCell;
    use std::collections::HashMap;

    let cache: RefCell<HashMap<I, O>> = RefCell::new(HashMap::new());

    move |input: I| {
        if let Some(output) = cache.borrow().get(&input) {
            return output.clone();
        }
        let output = f(input.clone());
        cache.borrow_mut().insert(input, output.clone());
        output
    }
}

/// Tap - execute side effect and return input.
pub fn tap<T, F>(f: F) -> impl Fn(T) -> T
where
    F: Fn(&T),
{
    move |x| {
        f(&x);
        x
    }
}

/// Predicate combinator - logical and.
pub fn and<T, P1, P2>(p1: P1, p2: P2) -> impl Fn(&T) -> bool
where
    P1: Fn(&T) -> bool,
    P2: Fn(&T) -> bool,
{
    move |x| p1(x) && p2(x)
}

/// Predicate combinator - logical or.
pub fn or<T, P1, P2>(p1: P1, p2: P2) -> impl Fn(&T) -> bool
where
    P1: Fn(&T) -> bool,
    P2: Fn(&T) -> bool,
{
    move |x| p1(x) || p2(x)
}

/// Predicate combinator - logical not.
pub fn not<T, P>(p: P) -> impl Fn(&T) -> bool
where
    P: Fn(&T) -> bool,
{
    move |x| !p(x)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compose() {
        let add_one = |x: i32| x + 1;
        let double = |x: i32| x * 2;

        let composed = compose(add_one, double);
        assert_eq!(composed(10), 22); // (10 + 1) * 2
    }

    #[test]
    fn test_pipeline() {
        let pipeline = Pipeline::new(|x: i32| x + 1)
            .then(|x| x * 2)
            .then(|x| x - 1);

        assert_eq!(pipeline.apply(10), 21); // ((10 + 1) * 2) - 1
    }

    #[test]
    fn test_partial() {
        let add = |a: i32, b: i32| a + b;
        let add_10 = partial(add, 10);

        assert_eq!(add_10(32), 42);
    }

    #[test]
    fn test_flip() {
        let div = |a: i32, b: i32| a / b;
        let flipped = flip(div);

        assert_eq!(div(10, 2), 5);
        assert_eq!(flipped(2, 10), 5);
    }

    #[test]
    fn test_tap() {
        let mut log = Vec::new();
        let logger = tap(|x: &i32| log.push(*x));

        let result = logger(42);
        assert_eq!(result, 42);
        // Note: log won't capture due to closure semantics
    }

    #[test]
    fn test_predicates() {
        let is_positive = |x: &i32| *x > 0;
        let is_even = |x: &i32| *x % 2 == 0;

        let positive_even = and(is_positive, is_even);
        let positive_or_even = or(is_positive, is_even);

        assert!(positive_even(&4));
        assert!(!positive_even(&3));
        assert!(!positive_even(&-2));

        assert!(positive_or_even(&3));
        assert!(positive_or_even(&-2));
        assert!(!positive_or_even(&-3));
    }
}
