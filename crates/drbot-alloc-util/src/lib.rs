//! Allocation utilities for drbot.
//!
//! This crate provides:
//! - Allocation helpers
//! - Memory allocation wrappers
//! - Allocation tracking

use std::alloc::{self, Layout};
use thiserror::Error;

/// Allocation error types.
#[derive(Error, Debug, Clone)]
pub enum AllocError {
    #[error("Allocation failed")]
    AllocationFailed,

    #[error("Invalid layout")]
    InvalidLayout,

    #[error("Size overflow")]
    SizeOverflow,
}

/// Result type for allocation operations.
pub type Result<T> = std::result::Result<T, AllocError>;

/// Allocate memory.
pub unsafe fn allocate(layout: Layout) -> Result<*mut u8> {
    let ptr = alloc::alloc(layout);
    if ptr.is_null() {
        Err(AllocError::AllocationFailed)
    } else {
        Ok(ptr)
    }
}

/// Allocate zeroed memory.
pub unsafe fn allocate_zeroed(layout: Layout) -> Result<*mut u8> {
    let ptr = alloc::alloc_zeroed(layout);
    if ptr.is_null() {
        Err(AllocError::AllocationFailed)
    } else {
        Ok(ptr)
    }
}

/// Deallocate memory.
pub unsafe fn deallocate(ptr: *mut u8, layout: Layout) {
    alloc::dealloc(ptr, layout);
}

/// Reallocate memory.
pub unsafe fn reallocate(ptr: *mut u8, old_layout: Layout, new_size: usize) -> Result<*mut u8> {
    let new_ptr = alloc::realloc(ptr, old_layout, new_size);
    if new_ptr.is_null() {
        Err(AllocError::AllocationFailed)
    } else {
        Ok(new_ptr)
    }
}

/// Create layout for type.
pub fn layout_of<T>() -> Layout {
    Layout::new::<T>()
}

/// Create layout for array.
pub fn layout_array<T>(n: usize) -> Result<Layout> {
    Layout::array::<T>(n).map_err(|_| AllocError::InvalidLayout)
}

/// Allocate type.
pub fn alloc_type<T>() -> Result<*mut T> {
    let layout = layout_of::<T>();
    unsafe { allocate(layout).map(|ptr| ptr as *mut T) }
}

/// Allocate array.
pub fn alloc_array<T>(count: usize) -> Result<*mut T> {
    let layout = layout_array::<T>(count)?;
    unsafe { allocate(layout).map(|ptr| ptr as *mut T) }
}

/// Deallocate type.
pub unsafe fn dealloc_type<T>(ptr: *mut T) {
    let layout = layout_of::<T>();
    deallocate(ptr as *mut u8, layout);
}

/// Deallocate array.
pub unsafe fn dealloc_array<T>(ptr: *mut T, count: usize) {
    if let Ok(layout) = layout_array::<T>(count) {
        deallocate(ptr as *mut u8, layout);
    }
}

/// Allocation wrapper with automatic cleanup.
pub struct Allocation {
    ptr: *mut u8,
    layout: Layout,
}

impl Allocation {
    /// Create new allocation.
    pub fn new(layout: Layout) -> Result<Self> {
        let ptr = unsafe { allocate(layout)? };
        Ok(Self { ptr, layout })
    }

    /// Create zeroed allocation.
    pub fn zeroed(layout: Layout) -> Result<Self> {
        let ptr = unsafe { allocate_zeroed(layout)? };
        Ok(Self { ptr, layout })
    }

    /// Get pointer.
    pub fn as_ptr(&self) -> *mut u8 {
        self.ptr
    }

    /// Get layout.
    pub fn layout(&self) -> Layout {
        self.layout
    }

    /// Get size.
    pub fn size(&self) -> usize {
        self.layout.size()
    }

    /// Leak allocation (don't deallocate on drop).
    pub fn leak(self) -> *mut u8 {
        let ptr = self.ptr;
        std::mem::forget(self);
        ptr
    }
}

impl Drop for Allocation {
    fn drop(&mut self) {
        unsafe {
            deallocate(self.ptr, self.layout);
        }
    }
}

/// Typed allocation.
pub struct TypedAllocation<T> {
    ptr: *mut T,
}

impl<T> TypedAllocation<T> {
    /// Create new.
    pub fn new() -> Result<Self> {
        let ptr = alloc_type::<T>()?;
        Ok(Self { ptr })
    }

    /// Get pointer.
    pub fn as_ptr(&self) -> *mut T {
        self.ptr
    }

    /// Write value.
    pub fn write(&self, value: T) {
        unsafe {
            std::ptr::write(self.ptr, value);
        }
    }

    /// Read value.
    pub unsafe fn read(&self) -> T {
        std::ptr::read(self.ptr)
    }

    /// As reference.
    pub unsafe fn as_ref(&self) -> &T {
        &*self.ptr
    }

    /// As mutable reference.
    pub unsafe fn as_mut(&mut self) -> &mut T {
        &mut *self.ptr
    }

    /// Leak allocation.
    pub fn leak(self) -> *mut T {
        let ptr = self.ptr;
        std::mem::forget(self);
        ptr
    }
}

impl<T> Drop for TypedAllocation<T> {
    fn drop(&mut self) {
        unsafe {
            dealloc_type(self.ptr);
        }
    }
}

/// Array allocation.
pub struct ArrayAllocation<T> {
    ptr: *mut T,
    count: usize,
}

impl<T> ArrayAllocation<T> {
    /// Create new.
    pub fn new(count: usize) -> Result<Self> {
        let ptr = alloc_array::<T>(count)?;
        Ok(Self { ptr, count })
    }

    /// Get pointer.
    pub fn as_ptr(&self) -> *mut T {
        self.ptr
    }

    /// Get count.
    pub fn count(&self) -> usize {
        self.count
    }

    /// As slice.
    pub unsafe fn as_slice(&self) -> &[T] {
        std::slice::from_raw_parts(self.ptr, self.count)
    }

    /// As mutable slice.
    pub unsafe fn as_mut_slice(&mut self) -> &mut [T] {
        std::slice::from_raw_parts_mut(self.ptr, self.count)
    }

    /// Leak allocation.
    pub fn leak(self) -> *mut T {
        let ptr = self.ptr;
        std::mem::forget(self);
        ptr
    }
}

impl<T> Drop for ArrayAllocation<T> {
    fn drop(&mut self) {
        unsafe {
            dealloc_array(self.ptr, self.count);
        }
    }
}

/// Allocation counter for tracking.
#[derive(Debug, Default)]
pub struct AllocCounter {
    allocations: std::sync::atomic::AtomicUsize,
    deallocations: std::sync::atomic::AtomicUsize,
    bytes_allocated: std::sync::atomic::AtomicUsize,
}

impl AllocCounter {
    /// Create new counter.
    pub const fn new() -> Self {
        Self {
            allocations: std::sync::atomic::AtomicUsize::new(0),
            deallocations: std::sync::atomic::AtomicUsize::new(0),
            bytes_allocated: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// Record allocation.
    pub fn record_alloc(&self, size: usize) {
        self.allocations
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.bytes_allocated
            .fetch_add(size, std::sync::atomic::Ordering::Relaxed);
    }

    /// Record deallocation.
    pub fn record_dealloc(&self, size: usize) {
        self.deallocations
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.bytes_allocated
            .fetch_sub(size, std::sync::atomic::Ordering::Relaxed);
    }

    /// Get allocation count.
    pub fn allocations(&self) -> usize {
        self.allocations.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Get deallocation count.
    pub fn deallocations(&self) -> usize {
        self.deallocations
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Get bytes currently allocated.
    pub fn bytes_allocated(&self) -> usize {
        self.bytes_allocated
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Get live allocations.
    pub fn live_allocations(&self) -> usize {
        self.allocations().saturating_sub(self.deallocations())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allocation() {
        let layout = Layout::new::<i32>();
        let alloc = Allocation::new(layout).unwrap();
        assert!(!alloc.as_ptr().is_null());
        assert_eq!(alloc.size(), 4);
    }

    #[test]
    fn test_typed_allocation() {
        let alloc = TypedAllocation::<i32>::new().unwrap();
        alloc.write(42);
        unsafe {
            assert_eq!(alloc.read(), 42);
        }
    }

    #[test]
    fn test_array_allocation() {
        let alloc = ArrayAllocation::<i32>::new(10).unwrap();
        assert_eq!(alloc.count(), 10);
    }

    #[test]
    fn test_alloc_counter() {
        let counter = AllocCounter::new();
        counter.record_alloc(100);
        counter.record_alloc(200);
        counter.record_dealloc(100);
        assert_eq!(counter.allocations(), 2);
        assert_eq!(counter.deallocations(), 1);
        assert_eq!(counter.live_allocations(), 1);
    }
}
