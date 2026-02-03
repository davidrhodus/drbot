//! Atomic type extensions for drbot.
//!
//! This crate provides:
//! - Atomic extensions
//! - Atomic utilities
//! - Lock-free patterns

use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, AtomicUsize, Ordering};
use thiserror::Error;

/// Atomic extension error types.
#[derive(Error, Debug, Clone)]
pub enum AtomicError {
    #[error("Compare-exchange failed")]
    CompareExchangeFailed,
}

/// Result type for atomic operations.
pub type Result<T> = std::result::Result<T, AtomicError>;

/// Default ordering.
pub const DEFAULT_ORDER: Ordering = Ordering::SeqCst;

/// Atomic usize extensions.
pub trait AtomicUsizeExt {
    /// Increment and get new value.
    fn inc(&self) -> usize;

    /// Decrement and get new value.
    fn dec(&self) -> usize;

    /// Add and get new value.
    fn add_get(&self, n: usize) -> usize;

    /// Get and set.
    fn get_set(&self, value: usize) -> usize;

    /// Update with function.
    fn update<F: Fn(usize) -> usize>(&self, f: F) -> usize;
}

impl AtomicUsizeExt for AtomicUsize {
    fn inc(&self) -> usize {
        self.fetch_add(1, DEFAULT_ORDER).wrapping_add(1)
    }

    fn dec(&self) -> usize {
        self.fetch_sub(1, DEFAULT_ORDER).wrapping_sub(1)
    }

    fn add_get(&self, n: usize) -> usize {
        self.fetch_add(n, DEFAULT_ORDER).wrapping_add(n)
    }

    fn get_set(&self, value: usize) -> usize {
        self.swap(value, DEFAULT_ORDER)
    }

    fn update<F: Fn(usize) -> usize>(&self, f: F) -> usize {
        let mut current = self.load(Ordering::Relaxed);
        loop {
            let new = f(current);
            match self.compare_exchange_weak(current, new, DEFAULT_ORDER, Ordering::Relaxed) {
                Ok(_) => return new,
                Err(c) => current = c,
            }
        }
    }
}

/// Atomic i64 extensions.
pub trait AtomicI64Ext {
    /// Increment and get new value.
    fn inc(&self) -> i64;

    /// Decrement and get new value.
    fn dec(&self) -> i64;

    /// Get and set.
    fn get_set(&self, value: i64) -> i64;

    /// Update with function.
    fn update<F: Fn(i64) -> i64>(&self, f: F) -> i64;
}

impl AtomicI64Ext for AtomicI64 {
    fn inc(&self) -> i64 {
        self.fetch_add(1, DEFAULT_ORDER).wrapping_add(1)
    }

    fn dec(&self) -> i64 {
        self.fetch_sub(1, DEFAULT_ORDER).wrapping_sub(1)
    }

    fn get_set(&self, value: i64) -> i64 {
        self.swap(value, DEFAULT_ORDER)
    }

    fn update<F: Fn(i64) -> i64>(&self, f: F) -> i64 {
        let mut current = self.load(Ordering::Relaxed);
        loop {
            let new = f(current);
            match self.compare_exchange_weak(current, new, DEFAULT_ORDER, Ordering::Relaxed) {
                Ok(_) => return new,
                Err(c) => current = c,
            }
        }
    }
}

/// Atomic u64 extensions.
pub trait AtomicU64Ext {
    /// Increment and get new value.
    fn inc(&self) -> u64;

    /// Decrement and get new value.
    fn dec(&self) -> u64;

    /// Get and set.
    fn get_set(&self, value: u64) -> u64;

    /// Max with current.
    fn fetch_max_get(&self, value: u64) -> u64;

    /// Min with current.
    fn fetch_min_get(&self, value: u64) -> u64;
}

impl AtomicU64Ext for AtomicU64 {
    fn inc(&self) -> u64 {
        self.fetch_add(1, DEFAULT_ORDER).wrapping_add(1)
    }

    fn dec(&self) -> u64 {
        self.fetch_sub(1, DEFAULT_ORDER).wrapping_sub(1)
    }

    fn get_set(&self, value: u64) -> u64 {
        self.swap(value, DEFAULT_ORDER)
    }

    fn fetch_max_get(&self, value: u64) -> u64 {
        self.fetch_max(value, DEFAULT_ORDER).max(value)
    }

    fn fetch_min_get(&self, value: u64) -> u64 {
        self.fetch_min(value, DEFAULT_ORDER).min(value)
    }
}

/// Atomic bool extensions.
pub trait AtomicBoolExt {
    /// Toggle and return new value.
    fn toggle(&self) -> bool;

    /// Set if false, return if was set.
    fn try_set(&self) -> bool;

    /// Clear if true, return if was cleared.
    fn try_clear(&self) -> bool;
}

impl AtomicBoolExt for AtomicBool {
    fn toggle(&self) -> bool {
        let old = self.fetch_xor(true, DEFAULT_ORDER);
        !old
    }

    fn try_set(&self) -> bool {
        self.compare_exchange(false, true, DEFAULT_ORDER, Ordering::Relaxed)
            .is_ok()
    }

    fn try_clear(&self) -> bool {
        self.compare_exchange(true, false, DEFAULT_ORDER, Ordering::Relaxed)
            .is_ok()
    }
}

/// Atomic option.
#[derive(Debug)]
pub struct AtomicOption<T> {
    ptr: std::sync::atomic::AtomicPtr<T>,
}

impl<T> AtomicOption<T> {
    /// Create empty.
    pub fn new() -> Self {
        Self {
            ptr: std::sync::atomic::AtomicPtr::new(std::ptr::null_mut()),
        }
    }

    /// Create with value.
    pub fn with(value: T) -> Self {
        Self {
            ptr: std::sync::atomic::AtomicPtr::new(Box::into_raw(Box::new(value))),
        }
    }

    /// Take value.
    pub fn take(&self) -> Option<Box<T>> {
        let ptr = self.ptr.swap(std::ptr::null_mut(), DEFAULT_ORDER);
        if ptr.is_null() {
            None
        } else {
            Some(unsafe { Box::from_raw(ptr) })
        }
    }

    /// Store value.
    pub fn store(&self, value: T) -> Option<Box<T>> {
        let new_ptr = Box::into_raw(Box::new(value));
        let old_ptr = self.ptr.swap(new_ptr, DEFAULT_ORDER);
        if old_ptr.is_null() {
            None
        } else {
            Some(unsafe { Box::from_raw(old_ptr) })
        }
    }

    /// Is some.
    pub fn is_some(&self) -> bool {
        !self.ptr.load(Ordering::Relaxed).is_null()
    }

    /// Is none.
    pub fn is_none(&self) -> bool {
        self.ptr.load(Ordering::Relaxed).is_null()
    }
}

impl<T> Default for AtomicOption<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Drop for AtomicOption<T> {
    fn drop(&mut self) {
        let ptr = *self.ptr.get_mut();
        if !ptr.is_null() {
            unsafe { drop(Box::from_raw(ptr)) };
        }
    }
}

/// Atomic ID generator.
#[derive(Debug)]
pub struct AtomicIdGenerator {
    next: AtomicU64,
}

impl AtomicIdGenerator {
    /// Create new.
    pub const fn new() -> Self {
        Self {
            next: AtomicU64::new(1),
        }
    }

    /// Generate next ID.
    pub fn next(&self) -> u64 {
        self.next.fetch_add(1, DEFAULT_ORDER)
    }

    /// Peek next ID without incrementing.
    pub fn peek(&self) -> u64 {
        self.next.load(Ordering::Relaxed)
    }
}

impl Default for AtomicIdGenerator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_atomic_usize_ext() {
        let a = AtomicUsize::new(5);
        assert_eq!(a.inc(), 6);
        assert_eq!(a.dec(), 5);
        assert_eq!(a.add_get(10), 15);
    }

    #[test]
    fn test_atomic_bool_ext() {
        let a = AtomicBool::new(false);
        assert!(a.try_set());
        assert!(!a.try_set()); // Already set
        assert!(!a.toggle()); // Was true, now false, returns new value (false)
    }

    #[test]
    fn test_atomic_option() {
        let opt: AtomicOption<i32> = AtomicOption::new();
        assert!(opt.is_none());

        opt.store(42);
        assert!(opt.is_some());

        let val = opt.take();
        assert_eq!(*val.unwrap(), 42);
        assert!(opt.is_none());
    }

    #[test]
    fn test_atomic_id() {
        let gen = AtomicIdGenerator::new();
        assert_eq!(gen.next(), 1);
        assert_eq!(gen.next(), 2);
        assert_eq!(gen.next(), 3);
    }
}

// ============================================================================
// Kani Formal Verification Proofs
// ============================================================================

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    // ========================================================================
    // AtomicUsizeExt Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_atomic_usize_inc() {
        let value: u8 = kani::any();
        let a = AtomicUsize::new(value as usize);

        let result = a.inc();
        kani::assert(
            result == (value as usize).wrapping_add(1),
            "inc returns new value",
        );
    }

    #[kani::proof]
    fn proof_atomic_usize_dec() {
        let value: u8 = kani::any();
        kani::assume(value > 0);
        let a = AtomicUsize::new(value as usize);

        let result = a.dec();
        kani::assert(result == (value as usize) - 1, "dec returns new value");
    }

    #[kani::proof]
    fn proof_atomic_usize_add_get() {
        let value: u8 = kani::any();
        let add: u8 = kani::any();
        kani::assume((value as u16) + (add as u16) < 256);

        let a = AtomicUsize::new(value as usize);
        let result = a.add_get(add as usize);

        kani::assert(
            result == (value as usize) + (add as usize),
            "add_get returns sum",
        );
    }

    #[kani::proof]
    fn proof_atomic_usize_get_set() {
        let old_val: u8 = kani::any();
        let new_val: u8 = kani::any();

        let a = AtomicUsize::new(old_val as usize);
        let returned = a.get_set(new_val as usize);

        kani::assert(returned == old_val as usize, "get_set returns old value");
        kani::assert(
            a.load(Ordering::Relaxed) == new_val as usize,
            "get_set stores new value",
        );
    }

    #[kani::proof]
    fn proof_atomic_usize_update() {
        let value: u8 = kani::any();
        kani::assume(value < 200);

        let a = AtomicUsize::new(value as usize);
        let result = a.update(|x| x + 10);

        kani::assert(result == (value as usize) + 10, "update applies function");
    }

    // ========================================================================
    // AtomicI64Ext Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_atomic_i64_inc() {
        let value: i8 = kani::any();
        let a = AtomicI64::new(value as i64);

        let result = a.inc();
        kani::assert(
            result == (value as i64).wrapping_add(1),
            "inc returns new value",
        );
    }

    #[kani::proof]
    fn proof_atomic_i64_dec() {
        let value: i8 = kani::any();
        let a = AtomicI64::new(value as i64);

        let result = a.dec();
        kani::assert(
            result == (value as i64).wrapping_sub(1),
            "dec returns new value",
        );
    }

    #[kani::proof]
    fn proof_atomic_i64_get_set() {
        let old_val: i8 = kani::any();
        let new_val: i8 = kani::any();

        let a = AtomicI64::new(old_val as i64);
        let returned = a.get_set(new_val as i64);

        kani::assert(returned == old_val as i64, "get_set returns old value");
    }

    #[kani::proof]
    fn proof_atomic_i64_update() {
        let value: i8 = kani::any();
        kani::assume(value < 100);

        let a = AtomicI64::new(value as i64);
        let result = a.update(|x| x * 2);

        kani::assert(result == (value as i64) * 2, "update applies function");
    }

    // ========================================================================
    // AtomicU64Ext Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_atomic_u64_inc() {
        let value: u8 = kani::any();
        let a = AtomicU64::new(value as u64);

        let result = a.inc();
        kani::assert(result == (value as u64) + 1, "inc returns new value");
    }

    #[kani::proof]
    fn proof_atomic_u64_dec() {
        let value: u8 = kani::any();
        kani::assume(value > 0);
        let a = AtomicU64::new(value as u64);

        let result = a.dec();
        kani::assert(result == (value as u64) - 1, "dec returns new value");
    }

    #[kani::proof]
    fn proof_atomic_u64_get_set() {
        let old_val: u8 = kani::any();
        let new_val: u8 = kani::any();

        let a = AtomicU64::new(old_val as u64);
        let returned = a.get_set(new_val as u64);

        kani::assert(returned == old_val as u64, "get_set returns old value");
    }

    #[kani::proof]
    fn proof_atomic_u64_fetch_max_get() {
        let current: u8 = kani::any();
        let new_val: u8 = kani::any();

        let a = AtomicU64::new(current as u64);
        let result = a.fetch_max_get(new_val as u64);

        let expected = (current as u64).max(new_val as u64);
        kani::assert(result == expected, "fetch_max_get returns max");
    }

    #[kani::proof]
    fn proof_atomic_u64_fetch_min_get() {
        let current: u8 = kani::any();
        let new_val: u8 = kani::any();

        let a = AtomicU64::new(current as u64);
        let result = a.fetch_min_get(new_val as u64);

        let expected = (current as u64).min(new_val as u64);
        kani::assert(result == expected, "fetch_min_get returns min");
    }

    // ========================================================================
    // AtomicBoolExt Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_atomic_bool_toggle_false() {
        let a = AtomicBool::new(false);
        let result = a.toggle();

        kani::assert(result == true, "toggle false gives true");
        kani::assert(a.load(Ordering::Relaxed) == true, "value is now true");
    }

    #[kani::proof]
    fn proof_atomic_bool_toggle_true() {
        let a = AtomicBool::new(true);
        let result = a.toggle();

        kani::assert(result == false, "toggle true gives false");
        kani::assert(a.load(Ordering::Relaxed) == false, "value is now false");
    }

    #[kani::proof]
    fn proof_atomic_bool_try_set_false() {
        let a = AtomicBool::new(false);
        let result = a.try_set();

        kani::assert(result == true, "try_set on false succeeds");
        kani::assert(a.load(Ordering::Relaxed) == true, "value is now true");
    }

    #[kani::proof]
    fn proof_atomic_bool_try_set_true() {
        let a = AtomicBool::new(true);
        let result = a.try_set();

        kani::assert(result == false, "try_set on true fails");
    }

    #[kani::proof]
    fn proof_atomic_bool_try_clear_true() {
        let a = AtomicBool::new(true);
        let result = a.try_clear();

        kani::assert(result == true, "try_clear on true succeeds");
        kani::assert(a.load(Ordering::Relaxed) == false, "value is now false");
    }

    #[kani::proof]
    fn proof_atomic_bool_try_clear_false() {
        let a = AtomicBool::new(false);
        let result = a.try_clear();

        kani::assert(result == false, "try_clear on false fails");
    }

    // ========================================================================
    // AtomicOption Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_atomic_option_new_is_none() {
        let opt: AtomicOption<i32> = AtomicOption::new();

        kani::assert(opt.is_none(), "new is None");
        kani::assert(!opt.is_some(), "new is not Some");
    }

    #[kani::proof]
    fn proof_atomic_option_default_is_none() {
        let opt: AtomicOption<i32> = AtomicOption::default();

        kani::assert(opt.is_none(), "default is None");
    }

    #[kani::proof]
    fn proof_atomic_option_with_is_some() {
        let opt: AtomicOption<i32> = AtomicOption::with(42);

        kani::assert(opt.is_some(), "with is Some");
        kani::assert(!opt.is_none(), "with is not None");
    }

    #[kani::proof]
    fn proof_atomic_option_store_on_empty() {
        let opt: AtomicOption<i32> = AtomicOption::new();
        let old = opt.store(42);

        kani::assert(old.is_none(), "store on empty returns None");
        kani::assert(opt.is_some(), "opt is now Some");
    }

    #[kani::proof]
    fn proof_atomic_option_store_on_existing() {
        let opt: AtomicOption<i32> = AtomicOption::with(10);
        let old = opt.store(42);

        kani::assert(old.is_some(), "store on existing returns Some");
        kani::assert(*old.unwrap() == 10, "returns old value");
    }

    #[kani::proof]
    fn proof_atomic_option_take_empty() {
        let opt: AtomicOption<i32> = AtomicOption::new();
        let taken = opt.take();

        kani::assert(taken.is_none(), "take empty returns None");
    }

    #[kani::proof]
    fn proof_atomic_option_take_existing() {
        let opt: AtomicOption<i32> = AtomicOption::with(42);
        let taken = opt.take();

        kani::assert(taken.is_some(), "take existing returns Some");
        kani::assert(*taken.unwrap() == 42, "take returns value");
        kani::assert(opt.is_none(), "opt is now None");
    }

    // ========================================================================
    // AtomicIdGenerator Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_atomic_id_generator_new() {
        let gen = AtomicIdGenerator::new();

        kani::assert(gen.peek() == 1, "new generator starts at 1");
    }

    #[kani::proof]
    fn proof_atomic_id_generator_default() {
        let gen = AtomicIdGenerator::default();

        kani::assert(gen.peek() == 1, "default generator starts at 1");
    }

    #[kani::proof]
    fn proof_atomic_id_generator_next() {
        let gen = AtomicIdGenerator::new();

        let id1 = gen.next();
        let id2 = gen.next();

        kani::assert(id1 == 1, "first id is 1");
        kani::assert(id2 == 2, "second id is 2");
        kani::assert(id1 != id2, "ids are unique");
    }

    #[kani::proof]
    fn proof_atomic_id_generator_peek_no_change() {
        let gen = AtomicIdGenerator::new();

        let p1 = gen.peek();
        let p2 = gen.peek();

        kani::assert(p1 == p2, "peek doesn't change state");
    }
}
