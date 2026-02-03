//! Result extensions for drbot.
//!
//! This crate provides:
//! - Extended Result methods
//! - Result combinators
//! - Conversion utilities

use std::fmt;

/// Extension trait for Result.
pub trait ResultExt<T, E> {
    /// Convert error to string.
    fn map_err_to_string(self) -> Result<T, String>
    where
        E: fmt::Display;

    /// Unwrap with default value.
    fn unwrap_or_default_with<F>(self, f: F) -> T
    where
        F: FnOnce(E) -> T;

    /// Log error and return None.
    fn ok_or_log<F>(self, log: F) -> Option<T>
    where
        F: FnOnce(&E);

    /// Tap into Ok value.
    fn tap<F>(self, f: F) -> Self
    where
        F: FnOnce(&T);

    /// Tap into Err value.
    fn tap_err<F>(self, f: F) -> Self
    where
        F: FnOnce(&E);

    /// Convert to Option, ignoring error.
    fn ignore_err(self) -> Option<T>;

    /// Replace Ok value.
    fn replace<U>(self, value: U) -> Result<U, E>;

    /// Replace with result of function.
    fn replace_with<U, F>(self, f: F) -> Result<U, E>
    where
        F: FnOnce(T) -> U;
}

impl<T, E> ResultExt<T, E> for Result<T, E> {
    fn map_err_to_string(self) -> Result<T, String>
    where
        E: fmt::Display,
    {
        self.map_err(|e| e.to_string())
    }

    fn unwrap_or_default_with<F>(self, f: F) -> T
    where
        F: FnOnce(E) -> T,
    {
        match self {
            Ok(v) => v,
            Err(e) => f(e),
        }
    }

    fn ok_or_log<F>(self, log: F) -> Option<T>
    where
        F: FnOnce(&E),
    {
        match self {
            Ok(v) => Some(v),
            Err(ref e) => {
                log(e);
                None
            }
        }
    }

    fn tap<F>(self, f: F) -> Self
    where
        F: FnOnce(&T),
    {
        if let Ok(ref v) = self {
            f(v);
        }
        self
    }

    fn tap_err<F>(self, f: F) -> Self
    where
        F: FnOnce(&E),
    {
        if let Err(ref e) = self {
            f(e);
        }
        self
    }

    fn ignore_err(self) -> Option<T> {
        self.ok()
    }

    fn replace<U>(self, value: U) -> Result<U, E> {
        self.map(|_| value)
    }

    fn replace_with<U, F>(self, f: F) -> Result<U, E>
    where
        F: FnOnce(T) -> U,
    {
        self.map(f)
    }
}

/// Extension trait for Result<Option<T>, E>.
pub trait ResultOptionExt<T, E> {
    /// Transpose Result<Option<T>, E> to Option<Result<T, E>>.
    fn transpose_inner(self) -> Option<Result<T, E>>;

    /// Flatten Result<Option<T>, E> with default on None.
    fn unwrap_inner_or(self, default: T) -> Result<T, E>;

    /// Flatten Result<Option<T>, E> with error on None.
    fn ok_or_inner<F>(self, err: F) -> Result<T, E>
    where
        F: FnOnce() -> E;
}

impl<T, E> ResultOptionExt<T, E> for Result<Option<T>, E> {
    fn transpose_inner(self) -> Option<Result<T, E>> {
        match self {
            Ok(Some(v)) => Some(Ok(v)),
            Ok(None) => None,
            Err(e) => Some(Err(e)),
        }
    }

    fn unwrap_inner_or(self, default: T) -> Result<T, E> {
        self.map(|opt| opt.unwrap_or(default))
    }

    fn ok_or_inner<F>(self, err: F) -> Result<T, E>
    where
        F: FnOnce() -> E,
    {
        match self {
            Ok(Some(v)) => Ok(v),
            Ok(None) => Err(err()),
            Err(e) => Err(e),
        }
    }
}

/// Combine multiple results.
pub fn combine<T, E>(results: Vec<Result<T, E>>) -> Result<Vec<T>, E> {
    results.into_iter().collect()
}

/// Partition results into Ok and Err.
pub fn partition<T, E>(results: Vec<Result<T, E>>) -> (Vec<T>, Vec<E>) {
    let mut oks = Vec::new();
    let mut errs = Vec::new();

    for result in results {
        match result {
            Ok(v) => oks.push(v),
            Err(e) => errs.push(e),
        }
    }

    (oks, errs)
}

/// Try to apply function to each element, short-circuiting on first error.
pub fn try_for_each<T, E, F>(items: impl IntoIterator<Item = T>, mut f: F) -> Result<(), E>
where
    F: FnMut(T) -> Result<(), E>,
{
    for item in items {
        f(item)?;
    }
    Ok(())
}

/// Try to map function over iterator, collecting results.
pub fn try_map<T, U, E, F>(items: impl IntoIterator<Item = T>, f: F) -> Result<Vec<U>, E>
where
    F: FnMut(T) -> Result<U, E>,
{
    items.into_iter().map(f).collect()
}

/// First successful result or last error.
pub fn first_ok<T, E>(results: impl IntoIterator<Item = Result<T, E>>) -> Option<Result<T, E>> {
    let mut last_err = None;

    for result in results {
        match result {
            Ok(v) => return Some(Ok(v)),
            Err(e) => last_err = Some(Err(e)),
        }
    }

    last_err
}

/// All must succeed.
pub fn all_ok<T, E>(results: impl IntoIterator<Item = Result<T, E>>) -> Result<Vec<T>, E> {
    results.into_iter().collect()
}

/// Any must succeed.
pub fn any_ok<T, E>(results: impl IntoIterator<Item = Result<T, E>>) -> Result<T, Vec<E>> {
    let mut errors = Vec::new();

    for result in results {
        match result {
            Ok(v) => return Ok(v),
            Err(e) => errors.push(e),
        }
    }

    Err(errors)
}

/// Result type alias for string errors.
pub type StringResult<T> = Result<T, String>;

/// Create Ok result.
pub fn ok<T, E>(value: T) -> Result<T, E> {
    Ok(value)
}

/// Create Err result.
pub fn err<T, E>(error: E) -> Result<T, E> {
    Err(error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_map_err_to_string() {
        let result: Result<i32, i32> = Err(42);
        let string_result = result.map_err_to_string();
        assert_eq!(string_result.unwrap_err(), "42");
    }

    #[test]
    fn test_tap() {
        let mut tapped = false;
        let result: Result<i32, ()> = Ok(42);
        let _ = result.tap(|_| tapped = true);
        assert!(tapped);
    }

    #[test]
    fn test_combine() {
        let results = vec![Ok(1), Ok(2), Ok(3)];
        let combined: Result<Vec<i32>, ()> = combine(results);
        assert_eq!(combined.unwrap(), vec![1, 2, 3]);

        let results_with_err: Vec<Result<i32, &str>> = vec![Ok(1), Err("error"), Ok(3)];
        let combined = combine(results_with_err);
        assert!(combined.is_err());
    }

    #[test]
    fn test_partition() {
        let results: Vec<Result<i32, &str>> = vec![Ok(1), Err("a"), Ok(2), Err("b")];
        let (oks, errs) = partition(results);
        assert_eq!(oks, vec![1, 2]);
        assert_eq!(errs, vec!["a", "b"]);
    }

    #[test]
    fn test_first_ok() {
        let results: Vec<Result<i32, &str>> = vec![Err("a"), Ok(2), Err("c")];
        let first = first_ok(results);
        assert_eq!(first.unwrap().unwrap(), 2);
    }

    #[test]
    fn test_any_ok() {
        let results: Vec<Result<i32, &str>> = vec![Err("a"), Ok(2), Err("c")];
        let any = any_ok(results);
        assert_eq!(any.unwrap(), 2);

        let all_err: Vec<Result<i32, &str>> = vec![Err("a"), Err("b")];
        let none = any_ok(all_err);
        assert_eq!(none.unwrap_err(), vec!["a", "b"]);
    }
}

// ============================================================================
// Kani Formal Verification Proofs
// ============================================================================

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    // ------------------------------------------------------------------------
    // ResultExt Trait Proofs
    // ------------------------------------------------------------------------

    #[kani::proof]
    fn proof_ignore_err_ok() {
        let value: u8 = kani::any();
        let result: Result<u8, u8> = Ok(value);

        let opt = result.ignore_err();
        kani::assert(opt == Some(value), "ignore_err on Ok returns Some");
    }

    #[kani::proof]
    fn proof_ignore_err_err() {
        let error: u8 = kani::any();
        let result: Result<u8, u8> = Err(error);

        let opt = result.ignore_err();
        kani::assert(opt.is_none(), "ignore_err on Err returns None");
    }

    #[kani::proof]
    fn proof_replace_ok() {
        let value: u8 = kani::any();
        let replacement: u8 = kani::any();
        let result: Result<u8, u8> = Ok(value);

        let replaced = result.replace(replacement);
        kani::assert(replaced == Ok(replacement), "replace on Ok gives new value");
    }

    #[kani::proof]
    fn proof_replace_err() {
        let error: u8 = kani::any();
        let replacement: u8 = kani::any();
        let result: Result<u8, u8> = Err(error);

        let replaced = result.replace(replacement);
        kani::assert(replaced == Err(error), "replace on Err preserves error");
    }

    #[kani::proof]
    fn proof_replace_with_ok() {
        let value: u8 = kani::any();
        let result: Result<u8, u8> = Ok(value);

        let replaced = result.replace_with(|x| x.wrapping_add(1));
        kani::assert(
            replaced == Ok(value.wrapping_add(1)),
            "replace_with applies function",
        );
    }

    #[kani::proof]
    fn proof_unwrap_or_default_with_ok() {
        let value: u8 = kani::any();
        let result: Result<u8, u8> = Ok(value);

        let v = result.unwrap_or_default_with(|_| 0);
        kani::assert(v == value, "unwrap_or_default_with on Ok returns value");
    }

    #[kani::proof]
    fn proof_unwrap_or_default_with_err() {
        let error: u8 = kani::any();
        let result: Result<u8, u8> = Err(error);

        let v = result.unwrap_or_default_with(|e| e.wrapping_add(1));
        kani::assert(
            v == error.wrapping_add(1),
            "unwrap_or_default_with on Err uses function",
        );
    }

    // ------------------------------------------------------------------------
    // ResultOptionExt Trait Proofs
    // ------------------------------------------------------------------------

    #[kani::proof]
    fn proof_transpose_inner_some() {
        let value: u8 = kani::any();
        let result: Result<Option<u8>, u8> = Ok(Some(value));

        let opt = result.transpose_inner();
        kani::assert(opt == Some(Ok(value)), "transpose Ok(Some) -> Some(Ok)");
    }

    #[kani::proof]
    fn proof_transpose_inner_none() {
        let result: Result<Option<u8>, u8> = Ok(None);

        let opt = result.transpose_inner();
        kani::assert(opt.is_none(), "transpose Ok(None) -> None");
    }

    #[kani::proof]
    fn proof_transpose_inner_err() {
        let error: u8 = kani::any();
        let result: Result<Option<u8>, u8> = Err(error);

        let opt = result.transpose_inner();
        kani::assert(opt == Some(Err(error)), "transpose Err -> Some(Err)");
    }

    #[kani::proof]
    fn proof_unwrap_inner_or_some() {
        let value: u8 = kani::any();
        let default: u8 = kani::any();
        let result: Result<Option<u8>, u8> = Ok(Some(value));

        let v = result.unwrap_inner_or(default);
        kani::assert(v == Ok(value), "unwrap_inner_or on Some returns value");
    }

    #[kani::proof]
    fn proof_unwrap_inner_or_none() {
        let default: u8 = kani::any();
        let result: Result<Option<u8>, u8> = Ok(None);

        let v = result.unwrap_inner_or(default);
        kani::assert(v == Ok(default), "unwrap_inner_or on None returns default");
    }

    #[kani::proof]
    fn proof_unwrap_inner_or_err() {
        let error: u8 = kani::any();
        let default: u8 = kani::any();
        let result: Result<Option<u8>, u8> = Err(error);

        let v = result.unwrap_inner_or(default);
        kani::assert(v == Err(error), "unwrap_inner_or on Err returns Err");
    }

    // ------------------------------------------------------------------------
    // Utility Function Proofs
    // ------------------------------------------------------------------------

    #[kani::proof]
    fn proof_partition_empty() {
        let results: Vec<Result<u8, u8>> = vec![];
        let (oks, errs) = partition(results);

        kani::assert(oks.is_empty(), "Empty input gives empty oks");
        kani::assert(errs.is_empty(), "Empty input gives empty errs");
    }

    #[kani::proof]
    fn proof_partition_all_ok() {
        let v1: u8 = kani::any();
        let v2: u8 = kani::any();
        let results: Vec<Result<u8, u8>> = vec![Ok(v1), Ok(v2)];

        let (oks, errs) = partition(results);

        kani::assert(oks.len() == 2, "All Ok gives 2 oks");
        kani::assert(errs.is_empty(), "All Ok gives no errs");
    }

    #[kani::proof]
    fn proof_partition_all_err() {
        let e1: u8 = kani::any();
        let e2: u8 = kani::any();
        let results: Vec<Result<u8, u8>> = vec![Err(e1), Err(e2)];

        let (oks, errs) = partition(results);

        kani::assert(oks.is_empty(), "All Err gives no oks");
        kani::assert(errs.len() == 2, "All Err gives 2 errs");
    }

    #[kani::proof]
    fn proof_partition_mixed() {
        let v: u8 = kani::any();
        let e: u8 = kani::any();
        let results: Vec<Result<u8, u8>> = vec![Ok(v), Err(e)];

        let (oks, errs) = partition(results);

        kani::assert(oks.len() == 1, "One Ok gives 1 ok");
        kani::assert(errs.len() == 1, "One Err gives 1 err");
        kani::assert(oks[0] == v, "Ok value preserved");
        kani::assert(errs[0] == e, "Err value preserved");
    }

    #[kani::proof]
    fn proof_first_ok_all_err() {
        let e1: u8 = kani::any();
        let e2: u8 = kani::any();
        let results: Vec<Result<u8, u8>> = vec![Err(e1), Err(e2)];

        let first = first_ok(results);

        // Should return the last error
        kani::assert(
            first == Some(Err(e2)),
            "first_ok on all Err returns last Err",
        );
    }

    #[kani::proof]
    fn proof_first_ok_finds_first() {
        let e1: u8 = kani::any();
        let v: u8 = kani::any();
        let e2: u8 = kani::any();
        let results: Vec<Result<u8, u8>> = vec![Err(e1), Ok(v), Err(e2)];

        let first = first_ok(results);

        kani::assert(first == Some(Ok(v)), "first_ok finds first Ok");
    }

    #[kani::proof]
    fn proof_ok_function() {
        let value: u8 = kani::any();
        let result: Result<u8, u8> = ok(value);

        kani::assert(result == Ok(value), "ok() creates Ok variant");
    }

    #[kani::proof]
    fn proof_err_function() {
        let error: u8 = kani::any();
        let result: Result<u8, u8> = err(error);

        kani::assert(result == Err(error), "err() creates Err variant");
    }

    #[kani::proof]
    fn proof_combine_all_ok() {
        let v1: u8 = kani::any();
        let v2: u8 = kani::any();
        let results: Vec<Result<u8, u8>> = vec![Ok(v1), Ok(v2)];

        let combined = combine(results);

        kani::assert(combined.is_ok(), "combine all Ok is Ok");
        kani::assert(combined.unwrap().len() == 2, "combine preserves count");
    }

    #[kani::proof]
    fn proof_combine_with_err() {
        let v: u8 = kani::any();
        let e: u8 = kani::any();
        let results: Vec<Result<u8, u8>> = vec![Ok(v), Err(e)];

        let combined = combine(results);

        kani::assert(combined.is_err(), "combine with Err is Err");
    }

    #[kani::proof]
    fn proof_any_ok_finds_ok() {
        let e: u8 = kani::any();
        let v: u8 = kani::any();
        let results: Vec<Result<u8, u8>> = vec![Err(e), Ok(v)];

        let any = any_ok(results);

        kani::assert(any == Ok(v), "any_ok finds Ok value");
    }

    #[kani::proof]
    fn proof_any_ok_all_err() {
        let e1: u8 = kani::any();
        let e2: u8 = kani::any();
        let results: Vec<Result<u8, u8>> = vec![Err(e1), Err(e2)];

        let any = any_ok(results);

        kani::assert(any.is_err(), "any_ok on all Err is Err");
        kani::assert(any.unwrap_err().len() == 2, "any_ok collects all errors");
    }
}
