//! Uninitialized value utilities for drbot.
//!
//! This crate provides:
//! - MaybeUninit helpers
//! - Safe uninitialized memory handling
//! - Initialization tracking

use std::mem::MaybeUninit;
use thiserror::Error;

/// Uninit error types.
#[derive(Error, Debug, Clone)]
pub enum UninitError {
    #[error("Value not initialized")]
    NotInitialized,

    #[error("Already initialized")]
    AlreadyInitialized,
}

/// Result type for uninit operations.
pub type Result<T> = std::result::Result<T, UninitError>;

/// Create uninitialized value.
pub fn uninit<T>() -> MaybeUninit<T> {
    MaybeUninit::uninit()
}

/// Create uninitialized array.
pub fn uninit_array<T, const N: usize>() -> [MaybeUninit<T>; N] {
    // SAFETY: An uninitialized array of MaybeUninit is valid.
    unsafe { MaybeUninit::<[MaybeUninit<T>; N]>::uninit().assume_init() }
}

/// Create zeroed value.
pub fn zeroed<T>() -> MaybeUninit<T> {
    MaybeUninit::zeroed()
}

/// Tracked uninitialized value.
#[derive(Debug)]
pub struct TrackedUninit<T> {
    value: MaybeUninit<T>,
    initialized: bool,
}

impl<T> TrackedUninit<T> {
    /// Create new uninitialized.
    pub fn uninit() -> Self {
        Self {
            value: MaybeUninit::uninit(),
            initialized: false,
        }
    }

    /// Create initialized.
    pub fn new(value: T) -> Self {
        Self {
            value: MaybeUninit::new(value),
            initialized: true,
        }
    }

    /// Initialize.
    pub fn init(&mut self, value: T) -> Result<()> {
        if self.initialized {
            return Err(UninitError::AlreadyInitialized);
        }
        self.value.write(value);
        self.initialized = true;
        Ok(())
    }

    /// Get reference.
    pub fn get(&self) -> Result<&T> {
        if !self.initialized {
            return Err(UninitError::NotInitialized);
        }
        // SAFETY: We checked that it's initialized.
        Ok(unsafe { self.value.assume_init_ref() })
    }

    /// Get mutable reference.
    pub fn get_mut(&mut self) -> Result<&mut T> {
        if !self.initialized {
            return Err(UninitError::NotInitialized);
        }
        // SAFETY: We checked that it's initialized.
        Ok(unsafe { self.value.assume_init_mut() })
    }

    /// Is initialized.
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// Take value.
    pub fn take(&mut self) -> Result<T> {
        if !self.initialized {
            return Err(UninitError::NotInitialized);
        }
        self.initialized = false;
        // SAFETY: We checked that it's initialized.
        Ok(unsafe { std::ptr::read(self.value.as_ptr()) })
    }

    /// Into inner, panics if not initialized.
    pub fn into_inner(mut self) -> Result<T> {
        if !self.initialized {
            return Err(UninitError::NotInitialized);
        }
        // Mark as uninitialized to prevent drop
        self.initialized = false;
        // SAFETY: We checked that it was initialized.
        Ok(unsafe { std::ptr::read(self.value.as_ptr()) })
    }
}

impl<T> Drop for TrackedUninit<T> {
    fn drop(&mut self) {
        if self.initialized {
            // SAFETY: Value is initialized.
            unsafe { std::ptr::drop_in_place(self.value.as_mut_ptr()) };
        }
    }
}

impl<T> Default for TrackedUninit<T> {
    fn default() -> Self {
        Self::uninit()
    }
}

/// Uninitialized buffer.
pub struct UninitBuffer<T, const N: usize> {
    buffer: [MaybeUninit<T>; N],
    len: usize,
}

impl<T, const N: usize> UninitBuffer<T, N> {
    /// Create new buffer.
    pub fn new() -> Self {
        Self {
            buffer: uninit_array(),
            len: 0,
        }
    }

    /// Push value.
    pub fn push(&mut self, value: T) -> Result<()> {
        if self.len >= N {
            return Err(UninitError::AlreadyInitialized);
        }
        self.buffer[self.len].write(value);
        self.len += 1;
        Ok(())
    }

    /// Get length.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Is empty.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Is full.
    pub fn is_full(&self) -> bool {
        self.len >= N
    }

    /// Get slice.
    pub fn as_slice(&self) -> &[T] {
        // SAFETY: First len elements are initialized.
        unsafe { std::slice::from_raw_parts(self.buffer.as_ptr() as *const T, self.len) }
    }

    /// Clear.
    pub fn clear(&mut self) {
        for i in 0..self.len {
            // SAFETY: Element is initialized.
            unsafe { std::ptr::drop_in_place(self.buffer[i].as_mut_ptr()) };
        }
        self.len = 0;
    }
}

impl<T, const N: usize> Drop for UninitBuffer<T, N> {
    fn drop(&mut self) {
        self.clear();
    }
}

impl<T, const N: usize> Default for UninitBuffer<T, N> {
    fn default() -> Self {
        Self::new()
    }
}

/// Write to uninitialized memory.
pub fn write_uninit<T>(uninit: &mut MaybeUninit<T>, value: T) -> &mut T {
    uninit.write(value)
}

/// Create box with uninitialized memory.
pub fn box_uninit<T>() -> Box<MaybeUninit<T>> {
    Box::new(MaybeUninit::uninit())
}

/// Initialize box.
pub fn init_box<T>(mut uninit: Box<MaybeUninit<T>>, value: T) -> Box<T> {
    uninit.write(value);
    // SAFETY: We just initialized it.
    unsafe { Box::from_raw(Box::into_raw(uninit) as *mut T) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tracked_uninit() {
        let mut t: TrackedUninit<i32> = TrackedUninit::uninit();
        assert!(!t.is_initialized());

        t.init(42).unwrap();
        assert!(t.is_initialized());
        assert_eq!(*t.get().unwrap(), 42);
    }

    #[test]
    fn test_tracked_new() {
        let t = TrackedUninit::new(42);
        assert!(t.is_initialized());
        assert_eq!(t.into_inner().unwrap(), 42);
    }

    #[test]
    fn test_uninit_buffer() {
        let mut buf: UninitBuffer<i32, 10> = UninitBuffer::new();
        buf.push(1).unwrap();
        buf.push(2).unwrap();
        buf.push(3).unwrap();

        assert_eq!(buf.len(), 3);
        assert_eq!(buf.as_slice(), &[1, 2, 3]);
    }

    #[test]
    fn test_init_box() {
        let uninit = box_uninit::<String>();
        let boxed = init_box(uninit, "hello".to_string());
        assert_eq!(*boxed, "hello");
    }
}
