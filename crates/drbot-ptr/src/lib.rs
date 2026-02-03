//! Pointer utilities for drbot.
//!
//! This crate provides:
//! - Pointer utilities
//! - Address operations
//! - Pointer comparisons

use std::ptr;
use thiserror::Error;

/// Pointer error types.
#[derive(Error, Debug, Clone)]
pub enum PtrError {
    #[error("Null pointer")]
    Null,

    #[error("Invalid pointer")]
    Invalid,

    #[error("Alignment error")]
    Alignment,
}

/// Result type for pointer operations.
pub type Result<T> = std::result::Result<T, PtrError>;

/// Check if pointer is null.
pub fn is_null<T>(ptr: *const T) -> bool {
    ptr.is_null()
}

/// Check if pointer is aligned.
pub fn is_aligned<T>(ptr: *const T) -> bool {
    (ptr as usize) % std::mem::align_of::<T>() == 0
}

/// Get address of pointer.
pub fn address<T>(ptr: *const T) -> usize {
    ptr as usize
}

/// Compare pointers.
pub fn ptr_eq<T: ?Sized>(a: *const T, b: *const T) -> bool {
    ptr::eq(a, b)
}

/// Non-null pointer wrapper.
#[derive(Debug)]
pub struct NonNull<T> {
    ptr: *mut T,
}

impl<T> NonNull<T> {
    /// Create from raw pointer.
    pub fn new(ptr: *mut T) -> Option<Self> {
        if ptr.is_null() {
            None
        } else {
            Some(Self { ptr })
        }
    }

    /// Create without null check (unsafe).
    pub const unsafe fn new_unchecked(ptr: *mut T) -> Self {
        Self { ptr }
    }

    /// As pointer.
    pub fn as_ptr(&self) -> *mut T {
        self.ptr
    }

    /// As reference.
    pub unsafe fn as_ref(&self) -> &T {
        &*self.ptr
    }

    /// As mutable reference.
    pub unsafe fn as_mut(&mut self) -> &mut T {
        &mut *self.ptr
    }

    /// Cast to different type.
    pub fn cast<U>(self) -> NonNull<U> {
        NonNull {
            ptr: self.ptr as *mut U,
        }
    }
}

impl<T> Clone for NonNull<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for NonNull<T> {}

/// Pointer offset operations.
pub trait PtrOffset {
    type Target;

    /// Offset by count.
    fn offset_by(self, count: isize) -> Self;

    /// Add offset.
    fn add_offset(self, count: usize) -> Self;

    /// Sub offset.
    fn sub_offset(self, count: usize) -> Self;
}

impl<T> PtrOffset for *const T {
    type Target = T;

    fn offset_by(self, count: isize) -> Self {
        unsafe { self.offset(count) }
    }

    fn add_offset(self, count: usize) -> Self {
        unsafe { self.add(count) }
    }

    fn sub_offset(self, count: usize) -> Self {
        unsafe { self.sub(count) }
    }
}

impl<T> PtrOffset for *mut T {
    type Target = T;

    fn offset_by(self, count: isize) -> Self {
        unsafe { self.offset(count) }
    }

    fn add_offset(self, count: usize) -> Self {
        unsafe { self.add(count) }
    }

    fn sub_offset(self, count: usize) -> Self {
        unsafe { self.sub(count) }
    }
}

/// Calculate distance between pointers.
pub fn distance<T>(from: *const T, to: *const T) -> isize {
    (to as isize - from as isize) / std::mem::size_of::<T>() as isize
}

/// Pointer range.
#[derive(Debug, Clone, Copy)]
pub struct PtrRange<T> {
    start: *const T,
    end: *const T,
}

impl<T> PtrRange<T> {
    /// Create new range.
    pub fn new(start: *const T, end: *const T) -> Self {
        Self { start, end }
    }

    /// From slice.
    pub fn from_slice(slice: &[T]) -> Self {
        let start = slice.as_ptr();
        let end = unsafe { start.add(slice.len()) };
        Self { start, end }
    }

    /// Check if contains pointer.
    pub fn contains(&self, ptr: *const T) -> bool {
        ptr >= self.start && ptr < self.end
    }

    /// Get length.
    pub fn len(&self) -> usize {
        distance(self.start, self.end) as usize
    }

    /// Is empty.
    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }

    /// Start pointer.
    pub fn start(&self) -> *const T {
        self.start
    }

    /// End pointer.
    pub fn end(&self) -> *const T {
        self.end
    }
}

/// Write to pointer.
pub unsafe fn write<T>(ptr: *mut T, value: T) {
    ptr::write(ptr, value);
}

/// Read from pointer.
pub unsafe fn read<T>(ptr: *const T) -> T {
    ptr::read(ptr)
}

/// Copy between pointers.
pub unsafe fn copy<T>(src: *const T, dst: *mut T, count: usize) {
    ptr::copy(src, dst, count);
}

/// Copy non-overlapping.
pub unsafe fn copy_nonoverlapping<T>(src: *const T, dst: *mut T, count: usize) {
    ptr::copy_nonoverlapping(src, dst, count);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_null() {
        let ptr: *const i32 = std::ptr::null();
        assert!(is_null(ptr));

        let val = 42;
        let ptr = &val as *const i32;
        assert!(!is_null(ptr));
    }

    #[test]
    fn test_is_aligned() {
        let val: u64 = 42;
        let ptr = &val as *const u64;
        assert!(is_aligned(ptr));
    }

    #[test]
    fn test_nonnull() {
        let mut val = 42;
        let ptr = NonNull::new(&mut val as *mut i32);
        assert!(ptr.is_some());

        let null_ptr: *mut i32 = std::ptr::null_mut();
        assert!(NonNull::new(null_ptr).is_none());
    }

    #[test]
    fn test_ptr_range() {
        let arr = [1, 2, 3, 4, 5];
        let range = PtrRange::from_slice(&arr);
        assert_eq!(range.len(), 5);
        assert!(range.contains(&arr[2] as *const i32));
    }

    #[test]
    fn test_distance() {
        let arr = [1i32, 2, 3, 4, 5];
        let dist = distance(&arr[0] as *const i32, &arr[3] as *const i32);
        assert_eq!(dist, 3);
    }
}
