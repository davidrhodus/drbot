//! Lazy evaluation utilities for drbot.
//!
//! This crate provides:
//! - Lazy values
//! - Lazy sequences
//! - Deferred computation

use std::sync::{Arc, RwLock};
use thiserror::Error;

/// Lazy error types.
#[derive(Error, Debug)]
pub enum LazyError {
    #[error("Initialization failed: {0}")]
    InitFailed(String),

    #[error("Already initialized")]
    AlreadyInitialized,
}

/// Result type for lazy operations.
pub type Result<T> = std::result::Result<T, LazyError>;

/// Lazy value that computes on first access.
pub struct Lazy<T> {
    value: RwLock<Option<T>>,
    init: RwLock<Option<Box<dyn FnOnce() -> T + Send + Sync>>>,
}

impl<T> Lazy<T> {
    /// Create new lazy value.
    pub fn new<F: FnOnce() -> T + Send + Sync + 'static>(init: F) -> Self {
        Self {
            value: RwLock::new(None),
            init: RwLock::new(Some(Box::new(init))),
        }
    }

    /// Check if initialized.
    pub fn is_initialized(&self) -> bool {
        self.value.read().unwrap().is_some()
    }

    /// Force initialization.
    pub fn force(&self)
    where
        T: Clone,
    {
        let _ = self.get();
    }

    /// Get value, initializing if needed.
    pub fn get(&self) -> T
    where
        T: Clone,
    {
        // Check if already initialized
        {
            let read = self.value.read().unwrap();
            if let Some(ref v) = *read {
                return v.clone();
            }
        }

        // Initialize
        let mut write = self.value.write().unwrap();
        if write.is_none() {
            let init = self.init.write().unwrap().take();
            if let Some(f) = init {
                *write = Some(f());
            }
        }
        write.as_ref().unwrap().clone()
    }

    /// Check if has value and get clone.
    pub fn get_if_initialized(&self) -> Option<T>
    where
        T: Clone,
    {
        self.value.read().unwrap().clone()
    }
}

/// Lazy with fallible initialization.
pub struct LazyResult<T, E> {
    value: RwLock<Option<std::result::Result<T, E>>>,
    init: RwLock<Option<Box<dyn FnOnce() -> std::result::Result<T, E> + Send + Sync>>>,
}

impl<T: Clone, E: Clone> LazyResult<T, E> {
    /// Create new lazy result.
    pub fn new<F: FnOnce() -> std::result::Result<T, E> + Send + Sync + 'static>(init: F) -> Self {
        Self {
            value: RwLock::new(None),
            init: RwLock::new(Some(Box::new(init))),
        }
    }

    /// Check if initialized.
    pub fn is_initialized(&self) -> bool {
        self.value.read().unwrap().is_some()
    }

    /// Get value, initializing if needed.
    pub fn get(&self) -> std::result::Result<T, E> {
        // Check if already initialized
        {
            let read = self.value.read().unwrap();
            if let Some(ref v) = *read {
                return v.clone();
            }
        }

        // Initialize
        let mut write = self.value.write().unwrap();
        if write.is_none() {
            let init = self.init.write().unwrap().take();
            if let Some(f) = init {
                *write = Some(f());
            }
        }
        write.as_ref().unwrap().clone()
    }
}

/// Lazy cell (interior mutability pattern).
pub struct LazyCell<T> {
    inner: std::cell::UnsafeCell<Option<T>>,
    init: std::cell::UnsafeCell<Option<Box<dyn FnOnce() -> T>>>,
}

impl<T> LazyCell<T> {
    /// Create new lazy cell.
    pub fn new<F: FnOnce() -> T + 'static>(init: F) -> Self {
        Self {
            inner: std::cell::UnsafeCell::new(None),
            init: std::cell::UnsafeCell::new(Some(Box::new(init))),
        }
    }

    /// Get or initialize value.
    pub fn get(&self) -> &T {
        unsafe {
            let inner = &mut *self.inner.get();
            if inner.is_none() {
                let init = &mut *self.init.get();
                if let Some(f) = init.take() {
                    *inner = Some(f());
                }
            }
            inner.as_ref().unwrap()
        }
    }

    /// Check if initialized.
    pub fn is_initialized(&self) -> bool {
        unsafe { (*self.inner.get()).is_some() }
    }
}

/// Deferred computation.
pub struct Deferred<T> {
    compute: Box<dyn Fn() -> T + Send + Sync>,
}

impl<T> Deferred<T> {
    /// Create new deferred computation.
    pub fn new<F: Fn() -> T + Send + Sync + 'static>(compute: F) -> Self {
        Self {
            compute: Box::new(compute),
        }
    }

    /// Evaluate the deferred computation.
    pub fn eval(&self) -> T {
        (self.compute)()
    }

    /// Map the result.
    pub fn map<U, F: Fn(T) -> U + Send + Sync + 'static>(self, f: F) -> Deferred<U>
    where
        T: 'static,
    {
        let compute = self.compute;
        Deferred::new(move || f(compute()))
    }
}

/// Lazy sequence.
pub struct LazySeq<T> {
    generator: Arc<dyn Fn(usize) -> Option<T> + Send + Sync>,
}

impl<T> LazySeq<T> {
    /// Create from generator function.
    pub fn new<F: Fn(usize) -> Option<T> + Send + Sync + 'static>(generator: F) -> Self {
        Self {
            generator: Arc::new(generator),
        }
    }

    /// Create from vector.
    pub fn from_vec(data: Vec<T>) -> Self
    where
        T: Clone + Send + Sync + 'static,
    {
        let items = Arc::new(data);
        Self {
            generator: Arc::new(move |index| items.get(index).cloned()),
        }
    }

    /// Get element at index.
    pub fn get(&self, index: usize) -> Option<T> {
        (self.generator)(index)
    }

    /// Take first n elements.
    pub fn take(&self, n: usize) -> Vec<T> {
        (0..n).filter_map(|i| self.get(i)).collect()
    }

    /// Create iterator.
    pub fn iter(&self) -> LazySeqIter<T> {
        LazySeqIter {
            seq: self,
            index: 0,
        }
    }
}

/// Iterator for lazy sequence.
pub struct LazySeqIter<'a, T> {
    seq: &'a LazySeq<T>,
    index: usize,
}

impl<'a, T> Iterator for LazySeqIter<'a, T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        let item = self.seq.get(self.index)?;
        self.index += 1;
        Some(item)
    }
}

/// Create lazy value.
pub fn lazy<T, F: FnOnce() -> T + Send + Sync + 'static>(init: F) -> Lazy<T> {
    Lazy::new(init)
}

/// Create deferred computation.
pub fn defer<T, F: Fn() -> T + Send + Sync + 'static>(compute: F) -> Deferred<T> {
    Deferred::new(compute)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicI32, Ordering};

    #[test]
    fn test_lazy() {
        let counter = Arc::new(AtomicI32::new(0));
        let counter_clone = counter.clone();

        let lazy = Lazy::new(move || {
            counter_clone.fetch_add(1, Ordering::SeqCst);
            42
        });

        assert!(!lazy.is_initialized());
        assert_eq!(counter.load(Ordering::SeqCst), 0);

        assert_eq!(lazy.get(), 42);
        assert!(lazy.is_initialized());
        assert_eq!(counter.load(Ordering::SeqCst), 1);

        // Subsequent accesses don't reinitialize
        assert_eq!(lazy.get(), 42);
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_deferred() {
        let deferred = Deferred::new(|| 21);
        let mapped = deferred.map(|x| x * 2);

        assert_eq!(mapped.eval(), 42);
    }

    #[test]
    fn test_lazy_seq() {
        let seq = LazySeq::new(|i| if i < 5 { Some(i * 2) } else { None });

        assert_eq!(seq.get(0), Some(0));
        assert_eq!(seq.get(2), Some(4));
        assert_eq!(seq.get(5), None);

        assert_eq!(seq.take(3), vec![0, 2, 4]);
    }
}

// ============================================================================
// Kani Formal Verification Proofs
// ============================================================================

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    // ========================================================================
    // Lazy Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_lazy_not_initialized_initially() {
        let lazy = Lazy::new(|| 42i32);
        kani::assert(!lazy.is_initialized(), "Lazy not initialized initially");
    }

    #[kani::proof]
    fn proof_lazy_get_initializes() {
        let lazy = Lazy::new(|| 42i32);
        let _ = lazy.get();
        kani::assert(lazy.is_initialized(), "get initializes Lazy");
    }

    #[kani::proof]
    fn proof_lazy_get_returns_value() {
        let value: i8 = kani::any();
        let lazy = Lazy::new(move || value);

        let result = lazy.get();
        kani::assert(result == value, "get returns computed value");
    }

    #[kani::proof]
    fn proof_lazy_get_idempotent() {
        let lazy = Lazy::new(|| 42i32);

        let first = lazy.get();
        let second = lazy.get();

        kani::assert(first == second, "get returns same value");
    }

    #[kani::proof]
    fn proof_lazy_get_if_initialized_before() {
        let lazy = Lazy::new(|| 42i32);
        let result = lazy.get_if_initialized();
        kani::assert(result.is_none(), "get_if_initialized None before init");
    }

    #[kani::proof]
    fn proof_lazy_get_if_initialized_after() {
        let lazy = Lazy::new(|| 42i32);
        let _ = lazy.get();
        let result = lazy.get_if_initialized();
        kani::assert(result == Some(42), "get_if_initialized Some after init");
    }

    #[kani::proof]
    fn proof_lazy_force_initializes() {
        let lazy = Lazy::new(|| 42i32);
        lazy.force();
        kani::assert(lazy.is_initialized(), "force initializes Lazy");
    }

    // ========================================================================
    // LazyResult Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_lazy_result_not_initialized_initially() {
        let lazy: LazyResult<i32, ()> = LazyResult::new(|| Ok(42));
        kani::assert(
            !lazy.is_initialized(),
            "LazyResult not initialized initially",
        );
    }

    #[kani::proof]
    fn proof_lazy_result_get_initializes() {
        let lazy: LazyResult<i32, ()> = LazyResult::new(|| Ok(42));
        let _ = lazy.get();
        kani::assert(lazy.is_initialized(), "get initializes LazyResult");
    }

    #[kani::proof]
    fn proof_lazy_result_get_ok() {
        let value: i8 = kani::any();
        let lazy: LazyResult<i8, ()> = LazyResult::new(move || Ok(value));

        let result = lazy.get();
        kani::assert(result.is_ok(), "result is Ok");
        kani::assert(result.unwrap() == value, "result has correct value");
    }

    #[kani::proof]
    fn proof_lazy_result_get_err() {
        let lazy: LazyResult<i32, &str> = LazyResult::new(|| Err("error"));

        let result = lazy.get();
        kani::assert(result.is_err(), "result is Err");
    }

    #[kani::proof]
    fn proof_lazy_result_get_idempotent() {
        let lazy: LazyResult<i32, ()> = LazyResult::new(|| Ok(42));

        let first = lazy.get();
        let second = lazy.get();

        kani::assert(first == second, "get returns same Result");
    }

    // ========================================================================
    // LazyCell Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_lazy_cell_not_initialized_initially() {
        let cell = LazyCell::new(|| 42i32);
        kani::assert(!cell.is_initialized(), "LazyCell not initialized initially");
    }

    #[kani::proof]
    fn proof_lazy_cell_get_initializes() {
        let cell = LazyCell::new(|| 42i32);
        let _ = cell.get();
        kani::assert(cell.is_initialized(), "get initializes LazyCell");
    }

    #[kani::proof]
    fn proof_lazy_cell_get_returns_value() {
        let cell = LazyCell::new(|| 42i32);
        let result = cell.get();
        kani::assert(*result == 42, "get returns computed value");
    }

    #[kani::proof]
    fn proof_lazy_cell_get_idempotent() {
        let cell = LazyCell::new(|| 42i32);

        let first = *cell.get();
        let second = *cell.get();

        kani::assert(first == second, "get returns same value");
    }

    // ========================================================================
    // Deferred Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_deferred_eval_returns_value() {
        let value: i8 = kani::any();
        let deferred = Deferred::new(move || value);

        let result = deferred.eval();
        kani::assert(result == value, "eval returns computed value");
    }

    #[kani::proof]
    fn proof_deferred_eval_multiple_times() {
        let deferred = Deferred::new(|| 42i32);

        let first = deferred.eval();
        let second = deferred.eval();

        kani::assert(first == second, "eval returns same value each time");
    }

    #[kani::proof]
    fn proof_deferred_map_transforms() {
        let deferred = Deferred::new(|| 21i32);
        let mapped = deferred.map(|x| x * 2);

        let result = mapped.eval();
        kani::assert(result == 42, "map transforms value");
    }

    #[kani::proof]
    fn proof_deferred_map_chain() {
        let deferred = Deferred::new(|| 10i32);
        let mapped = deferred.map(|x| x + 1).map(|x| x * 2);

        let result = mapped.eval();
        kani::assert(result == 22, "map chains correctly: (10+1)*2 = 22");
    }

    // ========================================================================
    // LazySeq Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_lazy_seq_get_in_bounds() {
        let seq = LazySeq::new(|i| if i < 5 { Some(i as i32) } else { None });

        let idx: usize = kani::any();
        kani::assume(idx < 5);

        let result = seq.get(idx);
        kani::assert(result == Some(idx as i32), "get returns correct value");
    }

    #[kani::proof]
    fn proof_lazy_seq_get_out_of_bounds() {
        let seq = LazySeq::new(|i| if i < 5 { Some(i as i32) } else { None });

        let idx: usize = kani::any();
        kani::assume(idx >= 5 && idx < 100);

        let result = seq.get(idx);
        kani::assert(result.is_none(), "get returns None out of bounds");
    }

    #[kani::proof]
    fn proof_lazy_seq_from_vec_get() {
        let seq = LazySeq::from_vec(vec![10i32, 20, 30]);

        kani::assert(seq.get(0) == Some(10), "get(0) == 10");
        kani::assert(seq.get(1) == Some(20), "get(1) == 20");
        kani::assert(seq.get(2) == Some(30), "get(2) == 30");
        kani::assert(seq.get(3).is_none(), "get(3) == None");
    }

    #[kani::proof]
    fn proof_lazy_seq_take_length() {
        let seq = LazySeq::new(|i| Some(i as i32));

        let n: usize = kani::any();
        kani::assume(n <= 10);

        let taken = seq.take(n);
        kani::assert(taken.len() == n, "take returns correct length");
    }

    #[kani::proof]
    fn proof_lazy_seq_take_bounded() {
        let seq = LazySeq::new(|i| if i < 3 { Some(i as i32) } else { None });

        let taken = seq.take(10);
        kani::assert(taken.len() == 3, "take stops at None");
    }

    // ========================================================================
    // lazy() Function Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_lazy_function() {
        let l = lazy(|| 42i32);
        kani::assert(!l.is_initialized(), "lazy creates uninitialized");
        kani::assert(l.get() == 42, "lazy get works");
    }

    // ========================================================================
    // defer() Function Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_defer_function() {
        let d = defer(|| 42i32);
        kani::assert(d.eval() == 42, "defer eval works");
    }
}
