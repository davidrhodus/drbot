//! Function utilities for drbot.
//!
//! This crate provides:
//! - Function composition
//! - Function memoization
//! - Function adapters

use std::collections::HashMap;
use std::hash::Hash;
use thiserror::Error;

/// Function error types.
#[derive(Error, Debug, Clone)]
pub enum FnError {
    #[error("Function call failed")]
    CallFailed,
}

/// Result type for function operations.
pub type Result<T> = std::result::Result<T, FnError>;

/// Identity function.
pub fn identity<T>(x: T) -> T {
    x
}

/// Constant function.
pub fn constant<T: Clone>(value: T) -> impl Fn() -> T {
    move || value.clone()
}

/// Compose two functions.
pub fn compose<A, B, C, F, G>(f: F, g: G) -> impl Fn(A) -> C
where
    F: Fn(A) -> B,
    G: Fn(B) -> C,
{
    move |a| g(f(a))
}

/// Flip function arguments.
pub fn flip<A, B, C, F>(f: F) -> impl Fn(B, A) -> C
where
    F: Fn(A, B) -> C,
{
    move |b, a| f(a, b)
}

/// Apply function to value.
pub fn apply<A, B, F: Fn(A) -> B>(f: F, a: A) -> B {
    f(a)
}

/// Create a function that ignores its argument.
pub fn ignore<A, B, F: Fn() -> B>(f: F) -> impl Fn(A) -> B {
    move |_| f()
}

/// Create a function that always returns the same value.
pub fn always<T: Clone, A>(value: T) -> impl Fn(A) -> T {
    move |_| value.clone()
}

/// Negate a predicate.
pub fn negate<A, F>(f: F) -> impl Fn(A) -> bool
where
    F: Fn(A) -> bool,
{
    move |a| !f(a)
}

/// Memoized function.
pub struct Memoized<F, A, B>
where
    A: Hash + Eq + Clone,
    B: Clone,
{
    func: F,
    cache: HashMap<A, B>,
}

impl<F, A, B> Memoized<F, A, B>
where
    F: Fn(A) -> B,
    A: Hash + Eq + Clone,
    B: Clone,
{
    /// Create new memoized function.
    pub fn new(func: F) -> Self {
        Self {
            func,
            cache: HashMap::new(),
        }
    }

    /// Call the function (with memoization).
    pub fn call(&mut self, arg: A) -> B {
        if let Some(cached) = self.cache.get(&arg) {
            return cached.clone();
        }
        let result = (self.func)(arg.clone());
        self.cache.insert(arg, result.clone());
        result
    }

    /// Clear the cache.
    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }

    /// Get cache size.
    pub fn cache_size(&self) -> usize {
        self.cache.len()
    }
}

/// Create a memoized function.
pub fn memoize<F, A, B>(f: F) -> Memoized<F, A, B>
where
    F: Fn(A) -> B,
    A: Hash + Eq + Clone,
    B: Clone,
{
    Memoized::new(f)
}

/// Run a function once and cache the result.
pub struct Once<F, T> {
    func: Option<F>,
    result: Option<T>,
}

impl<F, T> Once<F, T>
where
    F: FnOnce() -> T,
{
    /// Create new once function.
    pub fn new(func: F) -> Self {
        Self {
            func: Some(func),
            result: None,
        }
    }

    /// Get or compute the result.
    pub fn get(&mut self) -> &T {
        if self.result.is_none() {
            if let Some(f) = self.func.take() {
                self.result = Some(f());
            }
        }
        self.result.as_ref().unwrap()
    }

    /// Check if already computed.
    pub fn is_computed(&self) -> bool {
        self.result.is_some()
    }
}

/// Times function - call n times.
pub fn times<F: FnMut(usize)>(n: usize, mut f: F) {
    for i in 0..n {
        f(i);
    }
}

/// Repeat function - call until predicate returns false.
pub fn repeat_while<F, P>(mut f: F, mut predicate: P)
where
    F: FnMut(),
    P: FnMut() -> bool,
{
    while predicate() {
        f();
    }
}

/// Tap - execute side effect and return value.
pub fn tap<T, F: FnOnce(&T)>(value: T, f: F) -> T {
    f(&value);
    value
}

/// Tap mut - execute mutable side effect and return value.
pub fn tap_mut<T, F: FnOnce(&mut T)>(mut value: T, f: F) -> T {
    f(&mut value);
    value
}

/// Also - like tap but with ownership.
pub fn also<T, F: FnOnce(&T)>(value: T, f: F) -> T {
    f(&value);
    value
}

/// Let - transform value inline.
pub fn let_in<T, U, F: FnOnce(T) -> U>(value: T, f: F) -> U {
    f(value)
}

/// Take if - take value if predicate passes.
pub fn take_if<T, F: FnOnce(&T) -> bool>(value: T, predicate: F) -> Option<T> {
    if predicate(&value) {
        Some(value)
    } else {
        None
    }
}

/// Take unless - take value unless predicate passes.
pub fn take_unless<T, F: FnOnce(&T) -> bool>(value: T, predicate: F) -> Option<T> {
    if predicate(&value) {
        None
    } else {
        Some(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identity() {
        assert_eq!(identity(42), 42);
        assert_eq!(identity("hello"), "hello");
    }

    #[test]
    fn test_constant() {
        let f = constant(42);
        assert_eq!(f(), 42);
        assert_eq!(f(), 42);
    }

    #[test]
    fn test_compose() {
        let add_one = |x: i32| x + 1;
        let double = |x: i32| x * 2;
        let composed = compose(add_one, double);
        assert_eq!(composed(5), 12); // (5 + 1) * 2
    }

    #[test]
    fn test_flip() {
        let sub = |a: i32, b: i32| a - b;
        let flipped = flip(sub);
        assert_eq!(sub(10, 3), 7);
        assert_eq!(flipped(10, 3), -7);
    }

    #[test]
    fn test_negate() {
        let is_even = |x: i32| x % 2 == 0;
        let is_odd = negate(is_even);
        assert!(is_odd(3));
        assert!(!is_odd(4));
    }

    #[test]
    fn test_memoize() {
        let mut call_count = 0;
        let expensive = |x: i32| {
            call_count += 1;
            x * 2
        };

        // Can't easily test call_count with memoize due to closures
        // Just test basic functionality
        let mut memo = memoize(|x: i32| x * 2);
        assert_eq!(memo.call(5), 10);
        assert_eq!(memo.call(5), 10);
        assert_eq!(memo.cache_size(), 1);
    }

    #[test]
    fn test_once() {
        let mut counter = 0;
        let mut once = Once::new(|| {
            counter += 1;
            42
        });

        assert!(!once.is_computed());
        assert_eq!(*once.get(), 42);
        assert!(once.is_computed());
        assert_eq!(*once.get(), 42);
    }

    #[test]
    fn test_tap() {
        let result = tap(42, |x| println!("value: {}", x));
        assert_eq!(result, 42);
    }

    #[test]
    fn test_take_if() {
        let result = take_if(42, |x| *x > 10);
        assert_eq!(result, Some(42));

        let result = take_if(5, |x| *x > 10);
        assert_eq!(result, None);
    }
}

// ============================================================================
// Kani Formal Verification Proofs
// ============================================================================

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    // ========================================================================
    // identity() Function Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_identity_preserves_value() {
        let x: i8 = kani::any();
        kani::assert(identity(x) == x, "identity preserves value");
    }

    #[kani::proof]
    fn proof_identity_idempotent() {
        let x: i8 = kani::any();
        kani::assert(identity(identity(x)) == x, "identity is idempotent");
    }

    // ========================================================================
    // constant() Function Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_constant_returns_same() {
        let value: i8 = kani::any();
        let f = constant(value);
        kani::assert(f() == value, "constant returns same value");
    }

    #[kani::proof]
    fn proof_constant_multiple_calls() {
        let value: i8 = kani::any();
        let f = constant(value);
        let first = f();
        let second = f();
        kani::assert(first == second, "constant returns same on multiple calls");
    }

    // ========================================================================
    // compose() Function Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_compose_identity_left() {
        let x: i8 = kani::any();
        let f = |a: i8| a.wrapping_add(1);
        let composed = compose(identity, f);
        kani::assert(composed(x) == f(x), "compose with identity left");
    }

    #[kani::proof]
    fn proof_compose_identity_right() {
        let x: i8 = kani::any();
        let f = |a: i8| a.wrapping_add(1);
        let composed = compose(f, identity);
        kani::assert(composed(x) == f(x), "compose with identity right");
    }

    #[kani::proof]
    fn proof_compose_application_order() {
        let x: i8 = kani::any();
        kani::assume(x < 100 && x > -100);

        let add_one = |a: i8| a.wrapping_add(1);
        let double = |a: i8| a.wrapping_mul(2);

        let composed = compose(add_one, double);
        let expected = double(add_one(x));

        kani::assert(composed(x) == expected, "compose applies f then g");
    }

    // ========================================================================
    // flip() Function Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_flip_swaps_args() {
        let a: i8 = kani::any();
        let b: i8 = kani::any();
        kani::assume(a < 64 && a > -64 && b < 64 && b > -64);

        let sub = |x: i8, y: i8| x.wrapping_sub(y);
        let flipped = flip(sub);

        kani::assert(flipped(a, b) == sub(b, a), "flip swaps arguments");
    }

    #[kani::proof]
    fn proof_flip_double_is_identity() {
        let a: i8 = kani::any();
        let b: i8 = kani::any();
        kani::assume(a < 64 && a > -64 && b < 64 && b > -64);

        let sub = |x: i8, y: i8| x.wrapping_sub(y);
        let flipped_twice = flip(flip(sub));

        kani::assert(flipped_twice(a, b) == sub(a, b), "double flip is identity");
    }

    // ========================================================================
    // apply() Function Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_apply_calls_function() {
        let x: i8 = kani::any();
        let f = |a: i8| a.wrapping_add(1);

        kani::assert(apply(f, x) == f(x), "apply calls function");
    }

    #[kani::proof]
    fn proof_apply_identity() {
        let x: i8 = kani::any();
        kani::assert(apply(identity, x) == x, "apply with identity");
    }

    // ========================================================================
    // always() Function Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_always_ignores_arg() {
        let value: i8 = kani::any();
        let arg: i8 = kani::any();

        let f = always(value);
        kani::assert(f(arg) == value, "always ignores argument");
    }

    #[kani::proof]
    fn proof_always_constant_across_args() {
        let value: i8 = kani::any();
        let arg1: i8 = kani::any();
        let arg2: i8 = kani::any();

        let f = always(value);
        kani::assert(f(arg1) == f(arg2), "always returns same for different args");
    }

    // ========================================================================
    // negate() Function Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_negate_inverts_true() {
        let f = |_: i8| true;
        let negated = negate(f);
        let x: i8 = kani::any();

        kani::assert(!negated(x), "negate inverts true to false");
    }

    #[kani::proof]
    fn proof_negate_inverts_false() {
        let f = |_: i8| false;
        let negated = negate(f);
        let x: i8 = kani::any();

        kani::assert(negated(x), "negate inverts false to true");
    }

    #[kani::proof]
    fn proof_negate_double_is_original() {
        let x: i8 = kani::any();
        let is_positive = |a: i8| a > 0;
        let double_negated = negate(negate(is_positive));

        kani::assert(
            double_negated(x) == is_positive(x),
            "double negate is original",
        );
    }

    // ========================================================================
    // tap() Function Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_tap_returns_value() {
        let value: i8 = kani::any();
        let result = tap(value, |_| {});
        kani::assert(result == value, "tap returns original value");
    }

    #[kani::proof]
    fn proof_tap_preserves_value() {
        let value: i8 = kani::any();
        let mut observed = 0i8;
        let result = tap(value, |v| observed = *v);

        kani::assert(result == value, "tap returns value unchanged");
        kani::assert(observed == value, "tap effect sees value");
    }

    // ========================================================================
    // tap_mut() Function Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_tap_mut_returns_modified() {
        let value: i8 = kani::any();
        kani::assume(value < 127);

        let result = tap_mut(value, |v| *v += 1);
        kani::assert(result == value + 1, "tap_mut returns modified value");
    }

    // ========================================================================
    // also() Function Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_also_returns_value() {
        let value: i8 = kani::any();
        let result = also(value, |_| {});
        kani::assert(result == value, "also returns original value");
    }

    // ========================================================================
    // let_in() Function Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_let_in_transforms() {
        let value: i8 = kani::any();
        kani::assume(value < 127);

        let result = let_in(value, |v| v + 1);
        kani::assert(result == value + 1, "let_in transforms value");
    }

    #[kani::proof]
    fn proof_let_in_identity() {
        let value: i8 = kani::any();
        let result = let_in(value, identity);
        kani::assert(result == value, "let_in with identity");
    }

    // ========================================================================
    // take_if() Function Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_take_if_true_some() {
        let value: i8 = kani::any();
        let result = take_if(value, |_| true);
        kani::assert(result == Some(value), "take_if true returns Some");
    }

    #[kani::proof]
    fn proof_take_if_false_none() {
        let value: i8 = kani::any();
        let result = take_if(value, |_| false);
        kani::assert(result.is_none(), "take_if false returns None");
    }

    #[kani::proof]
    fn proof_take_if_predicate() {
        let value: i8 = kani::any();
        let result = take_if(value, |v| *v > 0);

        if value > 0 {
            kani::assert(result == Some(value), "take_if positive returns Some");
        } else {
            kani::assert(result.is_none(), "take_if non-positive returns None");
        }
    }

    // ========================================================================
    // take_unless() Function Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_take_unless_true_none() {
        let value: i8 = kani::any();
        let result = take_unless(value, |_| true);
        kani::assert(result.is_none(), "take_unless true returns None");
    }

    #[kani::proof]
    fn proof_take_unless_false_some() {
        let value: i8 = kani::any();
        let result = take_unless(value, |_| false);
        kani::assert(result == Some(value), "take_unless false returns Some");
    }

    #[kani::proof]
    fn proof_take_unless_opposite_of_take_if() {
        let value: i8 = kani::any();
        let predicate = |v: &i8| *v > 0;

        let if_result = take_if(value, predicate);
        let unless_result = take_unless(value, predicate);

        // Exactly one should be Some
        kani::assert(
            if_result.is_some() != unless_result.is_some(),
            "take_if and take_unless opposite",
        );
    }

    // ========================================================================
    // Memoized Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_memoized_new_empty_cache() {
        let memo = Memoized::new(|x: i8| x);
        kani::assert(memo.cache_size() == 0, "new Memoized has empty cache");
    }

    #[kani::proof]
    fn proof_memoized_call_returns_result() {
        let mut memo = Memoized::new(|x: i8| x.wrapping_mul(2));
        let input: i8 = kani::any();

        let result = memo.call(input);
        kani::assert(
            result == input.wrapping_mul(2),
            "memoized returns correct result",
        );
    }

    #[kani::proof]
    fn proof_memoized_call_caches() {
        let mut memo = Memoized::new(|x: i8| x.wrapping_mul(2));
        let input: i8 = kani::any();

        let _ = memo.call(input);
        kani::assert(memo.cache_size() == 1, "call adds to cache");
    }

    #[kani::proof]
    fn proof_memoized_same_input_same_output() {
        let mut memo = Memoized::new(|x: i8| x.wrapping_mul(2));
        let input: i8 = kani::any();

        let first = memo.call(input);
        let second = memo.call(input);

        kani::assert(first == second, "same input gives same output");
    }

    #[kani::proof]
    fn proof_memoized_clear_cache() {
        let mut memo = Memoized::new(|x: i8| x);
        let input: i8 = kani::any();

        let _ = memo.call(input);
        kani::assert(memo.cache_size() == 1, "cache has entry");

        memo.clear_cache();
        kani::assert(memo.cache_size() == 0, "clear empties cache");
    }

    // ========================================================================
    // Once Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_once_new_not_computed() {
        let once = Once::new(|| 42i32);
        kani::assert(!once.is_computed(), "new Once not computed");
    }

    #[kani::proof]
    fn proof_once_get_computes() {
        let value: i8 = kani::any();
        let mut once = Once::new(move || value);

        let result = *once.get();

        kani::assert(once.is_computed(), "get computes Once");
        kani::assert(result == value, "get returns computed value");
    }

    #[kani::proof]
    fn proof_once_get_idempotent() {
        let mut once = Once::new(|| 42i8);

        let first = *once.get();
        let second = *once.get();

        kani::assert(first == second, "multiple get returns same");
    }

    // ========================================================================
    // memoize() Function Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_memoize_creates_memoized() {
        let memo = memoize(|x: i8| x);
        kani::assert(memo.cache_size() == 0, "memoize creates empty cache");
    }
}
