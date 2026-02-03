//! Memory utilities for drbot.
//!
//! This crate provides:
//! - Memory operations
//! - Swap/replace utilities
//! - Memory manipulation helpers

use std::mem;
use thiserror::Error;

/// Memory error types.
#[derive(Error, Debug, Clone)]
pub enum MemError {
    #[error("Size mismatch")]
    SizeMismatch,

    #[error("Alignment error")]
    AlignmentError,
}

/// Result type for memory operations.
pub type Result<T> = std::result::Result<T, MemError>;

/// Swap two values.
pub fn swap<T>(a: &mut T, b: &mut T) {
    mem::swap(a, b);
}

/// Take a value, replacing with default.
pub fn take<T: Default>(dest: &mut T) -> T {
    mem::take(dest)
}

/// Replace a value.
pub fn replace<T>(dest: &mut T, src: T) -> T {
    mem::replace(dest, src)
}

/// Drop a value.
pub fn drop_value<T>(value: T) {
    drop(value);
}

/// Forget a value (leak).
pub fn forget<T>(value: T) {
    mem::forget(value);
}

/// Size of type.
pub const fn size_of<T>() -> usize {
    mem::size_of::<T>()
}

/// Size of value.
pub fn size_of_val<T: ?Sized>(val: &T) -> usize {
    mem::size_of_val(val)
}

/// Alignment of type.
pub const fn align_of<T>() -> usize {
    mem::align_of::<T>()
}

/// Alignment of value.
pub fn align_of_val<T: ?Sized>(val: &T) -> usize {
    mem::align_of_val(val)
}

/// Check if type needs drop.
pub const fn needs_drop<T>() -> bool {
    mem::needs_drop::<T>()
}

/// Transmute between types (unsafe).
pub unsafe fn transmute_copy<S, D>(src: &S) -> D {
    mem::transmute_copy(src)
}

/// Zero-sized type check.
pub const fn is_zst<T>() -> bool {
    size_of::<T>() == 0
}

/// Memory region wrapper.
#[derive(Debug)]
pub struct MemRegion {
    ptr: *mut u8,
    size: usize,
}

impl MemRegion {
    /// Create from raw parts.
    pub unsafe fn from_raw(ptr: *mut u8, size: usize) -> Self {
        Self { ptr, size }
    }

    /// Get pointer.
    pub fn as_ptr(&self) -> *mut u8 {
        self.ptr
    }

    /// Get size.
    pub fn size(&self) -> usize {
        self.size
    }

    /// Is null.
    pub fn is_null(&self) -> bool {
        self.ptr.is_null()
    }

    /// As slice.
    pub unsafe fn as_slice(&self) -> &[u8] {
        std::slice::from_raw_parts(self.ptr, self.size)
    }

    /// As mutable slice.
    pub unsafe fn as_mut_slice(&mut self) -> &mut [u8] {
        std::slice::from_raw_parts_mut(self.ptr, self.size)
    }
}

/// Manual drop wrapper.
pub struct ManualDrop<T> {
    value: mem::ManuallyDrop<T>,
}

impl<T> ManualDrop<T> {
    /// Create new.
    pub fn new(value: T) -> Self {
        Self {
            value: mem::ManuallyDrop::new(value),
        }
    }

    /// Get reference.
    pub fn get(&self) -> &T {
        &self.value
    }

    /// Get mutable reference.
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.value
    }

    /// Take inner value.
    pub fn take(mut self) -> T {
        unsafe { mem::ManuallyDrop::take(&mut self.value) }
    }

    /// Drop inner value.
    pub unsafe fn drop_inner(&mut self) {
        mem::ManuallyDrop::drop(&mut self.value);
    }
}

impl<T> std::ops::Deref for ManualDrop<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<T> std::ops::DerefMut for ManualDrop<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}

/// Maybe uninitialized memory.
pub struct MaybeUninit<T> {
    inner: mem::MaybeUninit<T>,
}

impl<T> MaybeUninit<T> {
    /// Create uninitialized.
    pub fn uninit() -> Self {
        Self {
            inner: mem::MaybeUninit::uninit(),
        }
    }

    /// Create zeroed.
    pub fn zeroed() -> Self {
        Self {
            inner: mem::MaybeUninit::zeroed(),
        }
    }

    /// Create with value.
    pub fn new(value: T) -> Self {
        Self {
            inner: mem::MaybeUninit::new(value),
        }
    }

    /// Write value.
    pub fn write(&mut self, value: T) -> &mut T {
        self.inner.write(value)
    }

    /// Assume initialized.
    pub unsafe fn assume_init(self) -> T {
        self.inner.assume_init()
    }

    /// Get pointer.
    pub fn as_ptr(&self) -> *const T {
        self.inner.as_ptr()
    }

    /// Get mutable pointer.
    pub fn as_mut_ptr(&mut self) -> *mut T {
        self.inner.as_mut_ptr()
    }
}

impl<T> Default for MaybeUninit<T> {
    fn default() -> Self {
        Self::uninit()
    }
}

/// Zero out memory.
pub fn zero<T>(value: &mut T) {
    let ptr = value as *mut T as *mut u8;
    let size = size_of::<T>();
    unsafe {
        std::ptr::write_bytes(ptr, 0, size);
    }
}

/// Copy from slice.
pub fn copy_from_slice<T: Copy>(dest: &mut [T], src: &[T]) {
    dest.copy_from_slice(src);
}

/// Clone from slice.
pub fn clone_from_slice<T: Clone>(dest: &mut [T], src: &[T]) {
    dest.clone_from_slice(src);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_swap() {
        let mut a = 1;
        let mut b = 2;
        swap(&mut a, &mut b);
        assert_eq!(a, 2);
        assert_eq!(b, 1);
    }

    #[test]
    fn test_take() {
        let mut val = 42;
        let taken = take(&mut val);
        assert_eq!(taken, 42);
        assert_eq!(val, 0);
    }

    #[test]
    fn test_replace() {
        let mut val = 42;
        let old = replace(&mut val, 84);
        assert_eq!(old, 42);
        assert_eq!(val, 84);
    }

    #[test]
    fn test_size_of() {
        assert_eq!(size_of::<i32>(), 4);
        assert!(is_zst::<()>());
    }

    #[test]
    fn test_manual_drop() {
        let md = ManualDrop::new(42);
        assert_eq!(*md.get(), 42);
        let val = md.take();
        assert_eq!(val, 42);
    }

    #[test]
    fn test_maybe_uninit() {
        let mut mu = MaybeUninit::<i32>::uninit();
        mu.write(42);
        let val = unsafe { mu.assume_init() };
        assert_eq!(val, 42);
    }
}
