//! Raw pointer utilities for drbot.
//!
//! This crate provides:
//! - Raw pointer helpers
//! - Unsafe pointer operations
//! - Pointer safety wrappers

use std::ptr;
use thiserror::Error;

/// Raw pointer error types.
#[derive(Error, Debug, Clone)]
pub enum RawPtrError {
    #[error("Null pointer")]
    Null,

    #[error("Dangling pointer")]
    Dangling,

    #[error("Misaligned pointer")]
    Misaligned,

    #[error("Out of bounds")]
    OutOfBounds,
}

/// Result type for raw pointer operations.
pub type Result<T> = std::result::Result<T, RawPtrError>;

/// Create null pointer.
pub fn null<T>() -> *const T {
    ptr::null()
}

/// Create null mutable pointer.
pub fn null_mut<T>() -> *mut T {
    ptr::null_mut()
}

/// Check null.
pub fn check_null<T>(ptr: *const T) -> Result<()> {
    if ptr.is_null() {
        Err(RawPtrError::Null)
    } else {
        Ok(())
    }
}

/// Check alignment.
pub fn check_aligned<T>(ptr: *const T) -> Result<()> {
    if (ptr as usize) % std::mem::align_of::<T>() != 0 {
        Err(RawPtrError::Misaligned)
    } else {
        Ok(())
    }
}

/// Safe raw pointer wrapper.
#[derive(Debug)]
pub struct SafePtr<T> {
    ptr: *mut T,
}

impl<T> SafePtr<T> {
    /// Create from pointer, checking null.
    pub fn new(ptr: *mut T) -> Result<Self> {
        if ptr.is_null() {
            Err(RawPtrError::Null)
        } else {
            Ok(Self { ptr })
        }
    }

    /// Create without check (unsafe).
    pub unsafe fn new_unchecked(ptr: *mut T) -> Self {
        Self { ptr }
    }

    /// Get raw pointer.
    pub fn as_ptr(&self) -> *mut T {
        self.ptr
    }

    /// Read value (unsafe).
    pub unsafe fn read(&self) -> T {
        ptr::read(self.ptr)
    }

    /// Write value (unsafe).
    pub unsafe fn write(&self, value: T) {
        ptr::write(self.ptr, value);
    }

    /// As reference (unsafe).
    pub unsafe fn as_ref(&self) -> &T {
        &*self.ptr
    }

    /// As mutable reference (unsafe).
    pub unsafe fn as_mut(&mut self) -> &mut T {
        &mut *self.ptr
    }

    /// Offset (unsafe).
    pub unsafe fn offset(&self, count: isize) -> Self {
        Self {
            ptr: self.ptr.offset(count),
        }
    }
}

impl<T> Clone for SafePtr<T> {
    fn clone(&self) -> Self {
        Self { ptr: self.ptr }
    }
}

impl<T> Copy for SafePtr<T> {}

/// Const raw pointer wrapper.
#[derive(Debug, Clone, Copy)]
pub struct ConstPtr<T> {
    ptr: *const T,
}

impl<T> ConstPtr<T> {
    /// Create from pointer.
    pub fn new(ptr: *const T) -> Result<Self> {
        if ptr.is_null() {
            Err(RawPtrError::Null)
        } else {
            Ok(Self { ptr })
        }
    }

    /// Create without check.
    pub unsafe fn new_unchecked(ptr: *const T) -> Self {
        Self { ptr }
    }

    /// Get raw pointer.
    pub fn as_ptr(&self) -> *const T {
        self.ptr
    }

    /// Read value (unsafe).
    pub unsafe fn read(&self) -> T {
        ptr::read(self.ptr)
    }

    /// As reference (unsafe).
    pub unsafe fn as_ref(&self) -> &T {
        &*self.ptr
    }

    /// Offset (unsafe).
    pub unsafe fn offset(&self, count: isize) -> Self {
        Self {
            ptr: self.ptr.offset(count),
        }
    }
}

/// Swap pointers.
pub unsafe fn swap<T>(a: *mut T, b: *mut T) {
    ptr::swap(a, b);
}

/// Replace at pointer.
pub unsafe fn replace<T>(dst: *mut T, src: T) -> T {
    ptr::replace(dst, src)
}

/// Copy memory.
pub unsafe fn copy_memory<T>(src: *const T, dst: *mut T, count: usize) {
    ptr::copy(src, dst, count);
}

/// Write bytes.
pub unsafe fn write_bytes<T>(dst: *mut T, val: u8, count: usize) {
    ptr::write_bytes(dst, val, count);
}

/// Zero memory.
pub unsafe fn zero_memory<T>(dst: *mut T, count: usize) {
    write_bytes(dst, 0, count);
}

/// Pointer diff.
pub fn ptr_diff<T>(from: *const T, to: *const T) -> isize {
    (to as isize - from as isize) / std::mem::size_of::<T>() as isize
}

/// Check if pointers overlap.
pub fn pointers_overlap<T>(a: *const T, a_len: usize, b: *const T, b_len: usize) -> bool {
    let a_end = unsafe { a.add(a_len) };
    let b_end = unsafe { b.add(b_len) };
    a < b_end && b < a_end
}

/// Slice from raw parts.
pub unsafe fn slice_from_raw<T>(ptr: *const T, len: usize) -> &'static [T] {
    std::slice::from_raw_parts(ptr, len)
}

/// Mutable slice from raw parts.
pub unsafe fn slice_from_raw_mut<T>(ptr: *mut T, len: usize) -> &'static mut [T] {
    std::slice::from_raw_parts_mut(ptr, len)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_null() {
        let ptr: *const i32 = null();
        assert!(ptr.is_null());
        assert!(check_null(ptr).is_err());
    }

    #[test]
    fn test_safe_ptr() {
        let mut val = 42;
        let ptr = SafePtr::new(&mut val as *mut i32).unwrap();
        unsafe {
            assert_eq!(ptr.read(), 42);
            ptr.write(84);
            assert_eq!(ptr.read(), 84);
        }
    }

    #[test]
    fn test_const_ptr() {
        let val = 42;
        let ptr = ConstPtr::new(&val as *const i32).unwrap();
        unsafe {
            assert_eq!(ptr.read(), 42);
        }
    }

    #[test]
    fn test_ptr_diff() {
        let arr = [1i32, 2, 3, 4, 5];
        let diff = ptr_diff(&arr[1], &arr[4]);
        assert_eq!(diff, 3);
    }

    #[test]
    fn test_overlap() {
        let arr = [1, 2, 3, 4, 5];
        assert!(pointers_overlap(&arr[0], 3, &arr[2], 2));
        assert!(!pointers_overlap(&arr[0], 2, &arr[3], 2));
    }
}
