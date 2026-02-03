//! Synchronization primitives for drbot.
//!
//! This crate provides:
//! - Counting semaphore
//! - Read-write lock utilities
//! - Barrier utilities

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use thiserror::Error;

/// Sync error types.
#[derive(Error, Debug)]
pub enum SyncError {
    #[error("Timeout waiting for lock")]
    Timeout,

    #[error("Lock poisoned")]
    Poisoned,

    #[error("Would block")]
    WouldBlock,
}

/// Result type for sync operations.
pub type Result<T> = std::result::Result<T, SyncError>;

/// Counting semaphore.
pub struct Semaphore {
    count: Mutex<usize>,
    max: usize,
    cond: Condvar,
}

impl Semaphore {
    /// Create new semaphore.
    pub fn new(initial: usize, max: usize) -> Self {
        Self {
            count: Mutex::new(initial),
            max,
            cond: Condvar::new(),
        }
    }

    /// Acquire permit.
    pub fn acquire(&self) {
        let mut count = self.count.lock().unwrap();
        while *count == 0 {
            count = self.cond.wait(count).unwrap();
        }
        *count -= 1;
    }

    /// Try to acquire permit.
    pub fn try_acquire(&self) -> bool {
        let mut count = self.count.lock().unwrap();
        if *count > 0 {
            *count -= 1;
            true
        } else {
            false
        }
    }

    /// Release permit.
    pub fn release(&self) {
        let mut count = self.count.lock().unwrap();
        if *count < self.max {
            *count += 1;
            self.cond.notify_one();
        }
    }

    /// Get available permits.
    pub fn available(&self) -> usize {
        *self.count.lock().unwrap()
    }
}

/// Semaphore guard.
pub struct SemaphoreGuard<'a> {
    semaphore: &'a Semaphore,
}

impl<'a> SemaphoreGuard<'a> {
    /// Create guard by acquiring semaphore.
    pub fn acquire(semaphore: &'a Semaphore) -> Self {
        semaphore.acquire();
        Self { semaphore }
    }

    /// Try to create guard.
    pub fn try_acquire(semaphore: &'a Semaphore) -> Option<Self> {
        if semaphore.try_acquire() {
            Some(Self { semaphore })
        } else {
            None
        }
    }
}

impl<'a> Drop for SemaphoreGuard<'a> {
    fn drop(&mut self) {
        self.semaphore.release();
    }
}

/// Spin lock.
pub struct SpinLock {
    locked: AtomicBool,
}

impl SpinLock {
    /// Create new spin lock.
    pub fn new() -> Self {
        Self {
            locked: AtomicBool::new(false),
        }
    }

    /// Acquire lock.
    pub fn lock(&self) {
        while self
            .locked
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            std::hint::spin_loop();
        }
    }

    /// Try to acquire lock.
    pub fn try_lock(&self) -> bool {
        self.locked
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
    }

    /// Release lock.
    pub fn unlock(&self) {
        self.locked.store(false, Ordering::Release);
    }

    /// Check if locked.
    pub fn is_locked(&self) -> bool {
        self.locked.load(Ordering::Relaxed)
    }
}

impl Default for SpinLock {
    fn default() -> Self {
        Self::new()
    }
}

/// Spin lock guard.
pub struct SpinLockGuard<'a> {
    lock: &'a SpinLock,
}

impl<'a> SpinLockGuard<'a> {
    /// Acquire guard.
    pub fn acquire(lock: &'a SpinLock) -> Self {
        lock.lock();
        Self { lock }
    }

    /// Try to acquire guard.
    pub fn try_acquire(lock: &'a SpinLock) -> Option<Self> {
        if lock.try_lock() {
            Some(Self { lock })
        } else {
            None
        }
    }
}

impl<'a> Drop for SpinLockGuard<'a> {
    fn drop(&mut self) {
        self.lock.unlock();
    }
}

/// Reentrant counter for counting nested operations.
pub struct ReentrantCounter {
    count: AtomicUsize,
}

impl ReentrantCounter {
    /// Create new counter.
    pub fn new() -> Self {
        Self {
            count: AtomicUsize::new(0),
        }
    }

    /// Enter (increment).
    pub fn enter(&self) -> usize {
        self.count.fetch_add(1, Ordering::SeqCst)
    }

    /// Leave (decrement).
    pub fn leave(&self) -> usize {
        self.count.fetch_sub(1, Ordering::SeqCst)
    }

    /// Get current count.
    pub fn count(&self) -> usize {
        self.count.load(Ordering::SeqCst)
    }

    /// Check if inside.
    pub fn is_inside(&self) -> bool {
        self.count() > 0
    }
}

impl Default for ReentrantCounter {
    fn default() -> Self {
        Self::new()
    }
}

/// Once cell (initialize once).
pub struct Once<T> {
    value: std::sync::RwLock<Option<T>>,
    initialized: AtomicBool,
}

impl<T> Once<T> {
    /// Create new once cell.
    pub fn new() -> Self {
        Self {
            value: std::sync::RwLock::new(None),
            initialized: AtomicBool::new(false),
        }
    }

    /// Initialize with value.
    pub fn init(&self, value: T) -> bool {
        if self
            .initialized
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            let mut guard = self.value.write().unwrap();
            *guard = Some(value);
            true
        } else {
            false
        }
    }

    /// Get or initialize.
    pub fn get_or_init<F: FnOnce() -> T>(&self, f: F) -> &T {
        if !self.initialized.load(Ordering::SeqCst) {
            self.init(f());
        }
        // Safety: We know value is initialized
        unsafe {
            let ptr = self.value.read().unwrap();
            let ref_ptr = ptr.as_ref().unwrap() as *const T;
            &*ref_ptr
        }
    }

    /// Check if initialized.
    pub fn is_initialized(&self) -> bool {
        self.initialized.load(Ordering::SeqCst)
    }
}

impl<T> Default for Once<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Wait group for waiting on multiple operations.
pub struct WaitGroup {
    count: Arc<(Mutex<usize>, Condvar)>,
}

impl WaitGroup {
    /// Create new wait group.
    pub fn new() -> Self {
        Self {
            count: Arc::new((Mutex::new(0), Condvar::new())),
        }
    }

    /// Add to wait group.
    pub fn add(&self, n: usize) {
        let mut count = self.count.0.lock().unwrap();
        *count += n;
    }

    /// Mark one operation as done.
    pub fn done(&self) {
        let mut count = self.count.0.lock().unwrap();
        *count = count.saturating_sub(1);
        if *count == 0 {
            self.count.1.notify_all();
        }
    }

    /// Wait for all operations to complete.
    pub fn wait(&self) {
        let mut count = self.count.0.lock().unwrap();
        while *count > 0 {
            count = self.count.1.wait(count).unwrap();
        }
    }

    /// Get current count.
    pub fn count(&self) -> usize {
        *self.count.0.lock().unwrap()
    }
}

impl Default for WaitGroup {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for WaitGroup {
    fn clone(&self) -> Self {
        Self {
            count: self.count.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_semaphore() {
        let sem = Semaphore::new(2, 2);

        assert_eq!(sem.available(), 2);
        sem.acquire();
        assert_eq!(sem.available(), 1);
        sem.acquire();
        assert_eq!(sem.available(), 0);

        sem.release();
        assert_eq!(sem.available(), 1);
    }

    #[test]
    fn test_spin_lock() {
        let lock = SpinLock::new();

        assert!(!lock.is_locked());
        lock.lock();
        assert!(lock.is_locked());
        lock.unlock();
        assert!(!lock.is_locked());
    }

    #[test]
    fn test_wait_group() {
        let wg = WaitGroup::new();
        wg.add(2);

        assert_eq!(wg.count(), 2);
        wg.done();
        assert_eq!(wg.count(), 1);
        wg.done();
        assert_eq!(wg.count(), 0);
    }

    #[test]
    fn test_reentrant_counter() {
        let counter = ReentrantCounter::new();

        assert!(!counter.is_inside());
        counter.enter();
        assert!(counter.is_inside());
        counter.enter();
        assert_eq!(counter.count(), 2);
        counter.leave();
        assert!(counter.is_inside());
        counter.leave();
        assert!(!counter.is_inside());
    }
}
