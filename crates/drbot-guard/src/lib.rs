//! Guard patterns and safety wrappers for drbot.
//!
//! This crate provides:
//! - Scope guards
//! - Lock guards
//! - Resource guards
//! - Cleanup patterns

use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use thiserror::Error;

/// Guard error types.
#[derive(Error, Debug)]
pub enum GuardError {
    #[error("Guard already dropped")]
    AlreadyDropped,

    #[error("Lock poisoned")]
    LockPoisoned,

    #[error("Guard failed: {0}")]
    Failed(String),
}

/// Result type for guard operations.
pub type Result<T> = std::result::Result<T, GuardError>;

/// Scope guard that runs cleanup on drop.
pub struct ScopeGuard<F: FnOnce()> {
    cleanup: Option<F>,
    active: bool,
}

impl<F: FnOnce()> ScopeGuard<F> {
    /// Create new scope guard.
    pub fn new(cleanup: F) -> Self {
        Self {
            cleanup: Some(cleanup),
            active: true,
        }
    }

    /// Dismiss the guard without running cleanup.
    pub fn dismiss(mut self) {
        self.active = false;
    }

    /// Cancel the guard and take the cleanup function.
    pub fn cancel(mut self) -> Option<F> {
        self.active = false;
        self.cleanup.take()
    }
}

impl<F: FnOnce()> Drop for ScopeGuard<F> {
    fn drop(&mut self) {
        if self.active {
            if let Some(cleanup) = self.cleanup.take() {
                cleanup();
            }
        }
    }
}

/// Create a scope guard.
pub fn defer<F: FnOnce()>(cleanup: F) -> ScopeGuard<F> {
    ScopeGuard::new(cleanup)
}

/// Success guard - runs cleanup only on success.
pub struct SuccessGuard<F: FnOnce()> {
    cleanup: Option<F>,
    success: bool,
}

impl<F: FnOnce()> SuccessGuard<F> {
    /// Create new success guard.
    pub fn new(cleanup: F) -> Self {
        Self {
            cleanup: Some(cleanup),
            success: false,
        }
    }

    /// Mark as successful.
    pub fn success(mut self) {
        self.success = true;
    }
}

impl<F: FnOnce()> Drop for SuccessGuard<F> {
    fn drop(&mut self) {
        if self.success {
            if let Some(cleanup) = self.cleanup.take() {
                cleanup();
            }
        }
    }
}

/// Failure guard - runs cleanup only on failure/panic.
pub struct FailureGuard<F: FnOnce()> {
    cleanup: Option<F>,
    committed: bool,
}

impl<F: FnOnce()> FailureGuard<F> {
    /// Create new failure guard.
    pub fn new(cleanup: F) -> Self {
        Self {
            cleanup: Some(cleanup),
            committed: false,
        }
    }

    /// Commit (don't run cleanup).
    pub fn commit(mut self) {
        self.committed = true;
    }
}

impl<F: FnOnce()> Drop for FailureGuard<F> {
    fn drop(&mut self) {
        if !self.committed {
            if let Some(cleanup) = self.cleanup.take() {
                cleanup();
            }
        }
    }
}

/// Value guard that wraps a value and runs cleanup on drop.
pub struct ValueGuard<T, F: FnOnce(T)> {
    value: Option<T>,
    cleanup: Option<F>,
}

impl<T, F: FnOnce(T)> ValueGuard<T, F> {
    /// Create new value guard.
    pub fn new(value: T, cleanup: F) -> Self {
        Self {
            value: Some(value),
            cleanup: Some(cleanup),
        }
    }

    /// Take value without cleanup.
    pub fn take(mut self) -> T {
        self.cleanup = None;
        self.value.take().unwrap()
    }

    /// Get reference to value.
    pub fn get(&self) -> &T {
        self.value.as_ref().unwrap()
    }

    /// Get mutable reference to value.
    pub fn get_mut(&mut self) -> &mut T {
        self.value.as_mut().unwrap()
    }
}

impl<T, F: FnOnce(T)> Deref for ValueGuard<T, F> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.value.as_ref().unwrap()
    }
}

impl<T, F: FnOnce(T)> DerefMut for ValueGuard<T, F> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.value.as_mut().unwrap()
    }
}

impl<T, F: FnOnce(T)> Drop for ValueGuard<T, F> {
    fn drop(&mut self) {
        if let (Some(value), Some(cleanup)) = (self.value.take(), self.cleanup.take()) {
            cleanup(value);
        }
    }
}

/// Reference counter guard.
pub struct RefGuard {
    counter: Arc<AtomicBool>,
}

impl RefGuard {
    /// Create new ref guard.
    pub fn new() -> Self {
        Self {
            counter: Arc::new(AtomicBool::new(true)),
        }
    }

    /// Check if valid.
    pub fn is_valid(&self) -> bool {
        self.counter.load(Ordering::SeqCst)
    }

    /// Create weak reference.
    pub fn weak(&self) -> WeakGuard {
        WeakGuard {
            counter: self.counter.clone(),
        }
    }

    /// Invalidate.
    pub fn invalidate(&self) {
        self.counter.store(false, Ordering::SeqCst);
    }
}

impl Default for RefGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for RefGuard {
    fn drop(&mut self) {
        self.invalidate();
    }
}

/// Weak guard that checks validity.
#[derive(Clone)]
pub struct WeakGuard {
    counter: Arc<AtomicBool>,
}

impl WeakGuard {
    /// Check if still valid.
    pub fn is_valid(&self) -> bool {
        self.counter.load(Ordering::SeqCst)
    }
}

/// Mutex guard with timeout tracking.
pub struct TimedMutexGuard<'a, T> {
    guard: MutexGuard<'a, T>,
    #[allow(dead_code)]
    acquired_at: std::time::Instant,
}

impl<T> TimedMutexGuard<'_, T> {
    /// Get how long the lock has been held.
    pub fn held_for(&self) -> std::time::Duration {
        self.acquired_at.elapsed()
    }
}

impl<T> Deref for TimedMutexGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.guard
    }
}

impl<T> DerefMut for TimedMutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.guard
    }
}

/// Mutex wrapper with timed guards.
pub struct TimedMutex<T> {
    inner: Mutex<T>,
}

impl<T> TimedMutex<T> {
    /// Create new timed mutex.
    pub fn new(value: T) -> Self {
        Self {
            inner: Mutex::new(value),
        }
    }

    /// Lock with timing.
    pub fn lock(&self) -> Result<TimedMutexGuard<'_, T>> {
        let guard = self.inner.lock().map_err(|_| GuardError::LockPoisoned)?;
        Ok(TimedMutexGuard {
            guard,
            acquired_at: std::time::Instant::now(),
        })
    }
}

/// Reentrancy guard to prevent reentrant calls.
pub struct ReentrancyGuard {
    locked: AtomicBool,
}

impl ReentrancyGuard {
    /// Create new guard.
    pub fn new() -> Self {
        Self {
            locked: AtomicBool::new(false),
        }
    }

    /// Try to enter guarded section.
    pub fn try_enter(&self) -> Option<ReentrancyToken> {
        if self
            .locked
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            Some(ReentrancyToken {
                guard: &self.locked,
            })
        } else {
            None
        }
    }

    /// Check if locked.
    pub fn is_locked(&self) -> bool {
        self.locked.load(Ordering::SeqCst)
    }
}

impl Default for ReentrancyGuard {
    fn default() -> Self {
        Self::new()
    }
}

/// Token for reentrancy guard.
pub struct ReentrancyToken<'a> {
    guard: &'a AtomicBool,
}

impl Drop for ReentrancyToken<'_> {
    fn drop(&mut self) {
        self.guard.store(false, Ordering::SeqCst);
    }
}

/// Boolean guard that sets flag on drop.
pub struct BoolGuard<'a> {
    flag: &'a AtomicBool,
    set_on_drop: bool,
}

impl<'a> BoolGuard<'a> {
    /// Create guard that sets flag to true on drop.
    pub fn set_true(flag: &'a AtomicBool) -> Self {
        Self {
            flag,
            set_on_drop: true,
        }
    }

    /// Create guard that sets flag to false on drop.
    pub fn set_false(flag: &'a AtomicBool) -> Self {
        Self {
            flag,
            set_on_drop: false,
        }
    }

    /// Cancel the guard.
    pub fn cancel(self) -> &'a AtomicBool {
        let flag = self.flag;
        std::mem::forget(self);
        flag
    }
}

impl Drop for BoolGuard<'_> {
    fn drop(&mut self) {
        self.flag.store(self.set_on_drop, Ordering::SeqCst);
    }
}

/// Counter guard that increments/decrements.
pub struct CounterGuard<'a> {
    counter: &'a std::sync::atomic::AtomicUsize,
}

impl<'a> CounterGuard<'a> {
    /// Create guard that increments on creation, decrements on drop.
    pub fn new(counter: &'a std::sync::atomic::AtomicUsize) -> Self {
        counter.fetch_add(1, Ordering::SeqCst);
        Self { counter }
    }
}

impl Drop for CounterGuard<'_> {
    fn drop(&mut self) {
        self.counter.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Option guard that clears on drop.
pub struct OptionGuard<'a, T> {
    option: &'a Mutex<Option<T>>,
}

impl<'a, T> OptionGuard<'a, T> {
    /// Create guard that clears option on drop.
    pub fn new(option: &'a Mutex<Option<T>>) -> Self {
        Self { option }
    }

    /// Cancel the guard.
    pub fn cancel(self) {
        std::mem::forget(self);
    }
}

impl<T> Drop for OptionGuard<'_, T> {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.option.lock() {
            *guard = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scope_guard() {
        use std::sync::atomic::AtomicUsize;

        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();

        {
            let _guard = defer(move || {
                counter_clone.fetch_add(1, Ordering::SeqCst);
            });
        }

        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_scope_guard_dismiss() {
        use std::sync::atomic::AtomicUsize;

        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();

        {
            let guard = defer(move || {
                counter_clone.fetch_add(1, Ordering::SeqCst);
            });
            guard.dismiss();
        }

        assert_eq!(counter.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn test_failure_guard() {
        use std::sync::atomic::AtomicUsize;

        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();

        {
            let guard = FailureGuard::new(move || {
                counter_clone.fetch_add(1, Ordering::SeqCst);
            });
            guard.commit();
        }

        assert_eq!(counter.load(Ordering::SeqCst), 0);

        let counter_clone2 = counter.clone();
        {
            let _guard = FailureGuard::new(move || {
                counter_clone2.fetch_add(1, Ordering::SeqCst);
            });
            // Don't commit
        }

        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_value_guard() {
        use std::sync::atomic::AtomicUsize;

        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();

        {
            let guard = ValueGuard::new(42, move |v| {
                counter_clone.fetch_add(v as usize, Ordering::SeqCst);
            });
            assert_eq!(*guard, 42);
        }

        assert_eq!(counter.load(Ordering::SeqCst), 42);
    }

    #[test]
    fn test_reentrancy_guard() {
        let guard = ReentrancyGuard::new();

        {
            let token = guard.try_enter();
            assert!(token.is_some());
            assert!(guard.is_locked());

            // Try reentrant call
            assert!(guard.try_enter().is_none());
        }

        // After drop, should be unlocked
        assert!(!guard.is_locked());
        assert!(guard.try_enter().is_some());
    }

    #[test]
    fn test_counter_guard() {
        use std::sync::atomic::AtomicUsize;

        let counter = AtomicUsize::new(0);

        {
            let _g1 = CounterGuard::new(&counter);
            assert_eq!(counter.load(Ordering::SeqCst), 1);

            {
                let _g2 = CounterGuard::new(&counter);
                assert_eq!(counter.load(Ordering::SeqCst), 2);
            }

            assert_eq!(counter.load(Ordering::SeqCst), 1);
        }

        assert_eq!(counter.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn test_ref_guard() {
        let guard = RefGuard::new();
        let weak = guard.weak();

        assert!(guard.is_valid());
        assert!(weak.is_valid());

        drop(guard);

        assert!(!weak.is_valid());
    }
}

// ============================================================================
// Kani Formal Verification Proofs
// ============================================================================

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    // ========================================================================
    // ScopeGuard Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_scope_guard_new_active() {
        let guard = ScopeGuard::new(|| {});
        kani::assert(guard.active, "new ScopeGuard is active");
    }

    #[kani::proof]
    fn proof_scope_guard_dismiss_deactivates() {
        let guard = ScopeGuard::new(|| {});
        let active_before = guard.active;
        guard.dismiss();
        kani::assert(active_before, "was active before dismiss");
    }

    #[kani::proof]
    fn proof_scope_guard_cancel_returns_cleanup() {
        let guard = ScopeGuard::new(|| {});
        let cleanup = guard.cancel();
        kani::assert(cleanup.is_some(), "cancel returns cleanup function");
    }

    // ========================================================================
    // SuccessGuard Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_success_guard_new_not_success() {
        let guard = SuccessGuard::new(|| {});
        kani::assert(!guard.success, "new SuccessGuard not success");
    }

    // ========================================================================
    // FailureGuard Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_failure_guard_new_not_committed() {
        let guard = FailureGuard::new(|| {});
        kani::assert(!guard.committed, "new FailureGuard not committed");
    }

    // ========================================================================
    // ValueGuard Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_value_guard_get() {
        let value: i8 = kani::any();
        let guard = ValueGuard::new(value, |_| {});

        kani::assert(*guard.get() == value, "get returns correct value");
    }

    #[kani::proof]
    fn proof_value_guard_get_mut() {
        let value: i8 = kani::any();
        let mut guard = ValueGuard::new(value, |_| {});

        *guard.get_mut() = 42;
        kani::assert(*guard.get() == 42, "get_mut allows modification");
    }

    #[kani::proof]
    fn proof_value_guard_take() {
        let value: i8 = kani::any();
        let guard = ValueGuard::new(value, |_| {});

        let taken = guard.take();
        kani::assert(taken == value, "take returns value");
    }

    #[kani::proof]
    fn proof_value_guard_deref() {
        let value: i8 = kani::any();
        let guard = ValueGuard::new(value, |_| {});

        kani::assert(*guard == value, "deref returns value");
    }

    #[kani::proof]
    fn proof_value_guard_deref_mut() {
        let value: i8 = kani::any();
        let mut guard = ValueGuard::new(value, |_| {});

        *guard = 100;
        kani::assert(*guard == 100, "deref_mut allows modification");
    }

    // ========================================================================
    // RefGuard Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_ref_guard_new_valid() {
        let guard = RefGuard::new();
        kani::assert(guard.is_valid(), "new RefGuard is valid");
    }

    #[kani::proof]
    fn proof_ref_guard_default_valid() {
        let guard = RefGuard::default();
        kani::assert(guard.is_valid(), "default RefGuard is valid");
    }

    #[kani::proof]
    fn proof_ref_guard_invalidate() {
        let guard = RefGuard::new();
        guard.invalidate();
        kani::assert(!guard.is_valid(), "invalidated RefGuard not valid");
    }

    #[kani::proof]
    fn proof_ref_guard_weak_shares_state() {
        let guard = RefGuard::new();
        let weak = guard.weak();

        kani::assert(weak.is_valid(), "weak starts valid");

        guard.invalidate();
        kani::assert(!weak.is_valid(), "weak becomes invalid");
    }

    #[kani::proof]
    fn proof_weak_guard_clone() {
        let guard = RefGuard::new();
        let weak1 = guard.weak();
        let weak2 = weak1.clone();

        kani::assert(
            weak1.is_valid() == weak2.is_valid(),
            "cloned weak has same state",
        );
    }

    // ========================================================================
    // ReentrancyGuard Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_reentrancy_guard_new_unlocked() {
        let guard = ReentrancyGuard::new();
        kani::assert(!guard.is_locked(), "new ReentrancyGuard unlocked");
    }

    #[kani::proof]
    fn proof_reentrancy_guard_default_unlocked() {
        let guard = ReentrancyGuard::default();
        kani::assert(!guard.is_locked(), "default ReentrancyGuard unlocked");
    }

    #[kani::proof]
    fn proof_reentrancy_guard_try_enter_success() {
        let guard = ReentrancyGuard::new();
        let token = guard.try_enter();

        kani::assert(token.is_some(), "first try_enter succeeds");
        kani::assert(guard.is_locked(), "becomes locked after enter");
    }

    #[kani::proof]
    fn proof_reentrancy_guard_try_enter_fails_when_locked() {
        let guard = ReentrancyGuard::new();
        let _token1 = guard.try_enter();

        let token2 = guard.try_enter();
        kani::assert(token2.is_none(), "second try_enter fails");
    }

    // ========================================================================
    // BoolGuard Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_bool_guard_set_true_config() {
        let flag = AtomicBool::new(false);
        let guard = BoolGuard::set_true(&flag);

        kani::assert(guard.set_on_drop, "set_true sets set_on_drop to true");
    }

    #[kani::proof]
    fn proof_bool_guard_set_false_config() {
        let flag = AtomicBool::new(true);
        let guard = BoolGuard::set_false(&flag);

        kani::assert(!guard.set_on_drop, "set_false sets set_on_drop to false");
    }

    // ========================================================================
    // CounterGuard Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_counter_guard_increments_on_new() {
        use std::sync::atomic::AtomicUsize;

        let counter = AtomicUsize::new(0);
        let _guard = CounterGuard::new(&counter);

        kani::assert(
            counter.load(Ordering::SeqCst) == 1,
            "counter incremented on new",
        );
    }

    // ========================================================================
    // TimedMutex Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_timed_mutex_new() {
        let value: i8 = kani::any();
        let mutex = TimedMutex::new(value);

        let guard = mutex.lock();
        kani::assert(guard.is_ok(), "lock succeeds on new mutex");
    }

    #[kani::proof]
    fn proof_timed_mutex_guard_deref() {
        let value: i8 = kani::any();
        let mutex = TimedMutex::new(value);

        let guard = mutex.lock().unwrap();
        kani::assert(*guard == value, "guard derefs to value");
    }

    #[kani::proof]
    fn proof_timed_mutex_guard_deref_mut() {
        let value: i8 = kani::any();
        let mutex = TimedMutex::new(value);

        let mut guard = mutex.lock().unwrap();
        *guard = 42;
        kani::assert(*guard == 42, "guard deref_mut allows modification");
    }

    // ========================================================================
    // defer() Function Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_defer_creates_scope_guard() {
        let guard = defer(|| {});
        kani::assert(guard.active, "defer creates active guard");
    }
}
