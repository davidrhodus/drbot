//! Alignment utilities for drbot.
//!
//! This crate provides:
//! - Alignment checking
//! - Aligned memory operations
//! - Alignment helpers

use thiserror::Error;

/// Alignment error types.
#[derive(Error, Debug, Clone)]
pub enum AlignError {
    #[error("Not aligned")]
    NotAligned,

    #[error("Invalid alignment: {0}")]
    InvalidAlignment(usize),

    #[error("Overflow")]
    Overflow,
}

/// Result type for alignment operations.
pub type Result<T> = std::result::Result<T, AlignError>;

/// Check if value is power of two.
pub const fn is_power_of_two(n: usize) -> bool {
    n != 0 && (n & (n - 1)) == 0
}

/// Check if alignment is valid.
pub const fn is_valid_alignment(align: usize) -> bool {
    is_power_of_two(align)
}

/// Check if pointer is aligned.
pub fn is_aligned<T>(ptr: *const T) -> bool {
    let align = std::mem::align_of::<T>();
    (ptr as usize) % align == 0
}

/// Check if address is aligned to given alignment.
pub fn is_aligned_to(addr: usize, align: usize) -> bool {
    if !is_valid_alignment(align) {
        return false;
    }
    addr % align == 0
}

/// Align up to next alignment boundary.
pub fn align_up(addr: usize, align: usize) -> Result<usize> {
    if !is_valid_alignment(align) {
        return Err(AlignError::InvalidAlignment(align));
    }
    let mask = align - 1;
    addr.checked_add(mask)
        .map(|a| a & !mask)
        .ok_or(AlignError::Overflow)
}

/// Align down to previous alignment boundary.
pub fn align_down(addr: usize, align: usize) -> Result<usize> {
    if !is_valid_alignment(align) {
        return Err(AlignError::InvalidAlignment(align));
    }
    Ok(addr & !(align - 1))
}

/// Calculate padding needed for alignment.
pub fn padding_for(addr: usize, align: usize) -> Result<usize> {
    if !is_valid_alignment(align) {
        return Err(AlignError::InvalidAlignment(align));
    }
    let rem = addr % align;
    if rem == 0 {
        Ok(0)
    } else {
        Ok(align - rem)
    }
}

/// Alignment requirement of type.
pub const fn align_of<T>() -> usize {
    std::mem::align_of::<T>()
}

/// Alignment requirement of value.
pub fn align_of_val<T: ?Sized>(val: &T) -> usize {
    std::mem::align_of_val(val)
}

/// Common alignments.
pub mod alignments {
    /// Byte alignment.
    pub const BYTE: usize = 1;
    /// 2-byte alignment.
    pub const ALIGN2: usize = 2;
    /// 4-byte alignment.
    pub const ALIGN4: usize = 4;
    /// 8-byte alignment.
    pub const ALIGN8: usize = 8;
    /// 16-byte alignment.
    pub const ALIGN16: usize = 16;
    /// 32-byte alignment.
    pub const ALIGN32: usize = 32;
    /// 64-byte alignment.
    pub const ALIGN64: usize = 64;
    /// Cache line alignment (common).
    pub const CACHE_LINE: usize = 64;
    /// Page alignment (4KB).
    pub const PAGE: usize = 4096;
}

/// Aligned wrapper.
#[repr(C)]
pub struct Aligned<T, const ALIGN: usize> {
    _align: [u8; 0],
    value: T,
}

impl<T, const ALIGN: usize> Aligned<T, ALIGN> {
    /// Create new aligned value.
    pub fn new(value: T) -> Self {
        Self { _align: [], value }
    }

    /// Get reference.
    pub fn get(&self) -> &T {
        &self.value
    }

    /// Get mutable reference.
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.value
    }

    /// Into inner.
    pub fn into_inner(self) -> T {
        self.value
    }
}

impl<T, const ALIGN: usize> std::ops::Deref for Aligned<T, ALIGN> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<T, const ALIGN: usize> std::ops::DerefMut for Aligned<T, ALIGN> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}

/// 8-byte aligned type.
pub type Align8<T> = Aligned<T, 8>;

/// 16-byte aligned type.
pub type Align16<T> = Aligned<T, 16>;

/// 32-byte aligned type.
pub type Align32<T> = Aligned<T, 32>;

/// 64-byte aligned type.
pub type Align64<T> = Aligned<T, 64>;

/// Cache-line aligned type.
pub type CacheAligned<T> = Aligned<T, 64>;

/// Assert alignment at runtime.
pub fn assert_aligned<T>(ptr: *const T) -> Result<()> {
    if is_aligned(ptr) {
        Ok(())
    } else {
        Err(AlignError::NotAligned)
    }
}

/// Assert alignment to specific boundary.
pub fn assert_aligned_to(addr: usize, align: usize) -> Result<()> {
    if !is_valid_alignment(align) {
        return Err(AlignError::InvalidAlignment(align));
    }
    if is_aligned_to(addr, align) {
        Ok(())
    } else {
        Err(AlignError::NotAligned)
    }
}

/// Get next power of two.
pub fn next_power_of_two(n: usize) -> Option<usize> {
    if n == 0 {
        return Some(1);
    }
    n.checked_next_power_of_two()
}

/// Get minimum alignment for size.
pub fn min_align_for_size(size: usize) -> usize {
    if size == 0 {
        return 1;
    }
    // Find lowest set bit
    size & size.wrapping_neg()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_power_of_two() {
        assert!(is_power_of_two(1));
        assert!(is_power_of_two(2));
        assert!(is_power_of_two(4));
        assert!(!is_power_of_two(3));
        assert!(!is_power_of_two(0));
    }

    #[test]
    fn test_align_up() {
        assert_eq!(align_up(1, 4), Ok(4));
        assert_eq!(align_up(4, 4), Ok(4));
        assert_eq!(align_up(5, 4), Ok(8));
        assert_eq!(align_up(0, 4), Ok(0));
    }

    #[test]
    fn test_align_down() {
        assert_eq!(align_down(1, 4), Ok(0));
        assert_eq!(align_down(4, 4), Ok(4));
        assert_eq!(align_down(5, 4), Ok(4));
        assert_eq!(align_down(8, 4), Ok(8));
    }

    #[test]
    fn test_padding_for() {
        assert_eq!(padding_for(0, 4), Ok(0));
        assert_eq!(padding_for(1, 4), Ok(3));
        assert_eq!(padding_for(4, 4), Ok(0));
        assert_eq!(padding_for(5, 4), Ok(3));
    }

    #[test]
    fn test_is_aligned() {
        let val: u32 = 42;
        assert!(is_aligned(&val as *const u32));
    }

    #[test]
    fn test_aligned_wrapper() {
        let aligned = Aligned::<u8, 16>::new(42);
        assert_eq!(*aligned, 42);
    }
}
