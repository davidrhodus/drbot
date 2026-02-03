//! Copy trait extensions for drbot.
//!
//! This crate provides:
//! - Copy utilities
//! - Copy semantics helpers
//! - Byte copying

use thiserror::Error;

/// Copy extension error types.
#[derive(Error, Debug, Clone)]
pub enum CopyExtError {
    #[error("Copy failed: {0}")]
    Failed(String),

    #[error("Buffer too small")]
    BufferTooSmall,
}

/// Result type for copy operations.
pub type Result<T> = std::result::Result<T, CopyExtError>;

/// Copy extension trait.
pub trait CopyExt: Copy {
    /// Copy if predicate is true.
    fn copy_if(self, predicate: bool) -> Option<Self> {
        if predicate {
            Some(self)
        } else {
            None
        }
    }

    /// Copy n times.
    fn copy_n(self, n: usize) -> Vec<Self> {
        vec![self; n]
    }

    /// Copy into array.
    fn copy_array<const N: usize>(self) -> [Self; N] {
        [self; N]
    }
}

impl<T: Copy> CopyExt for T {}

/// Copy bytes from slice.
pub fn copy_bytes(src: &[u8], dst: &mut [u8]) -> Result<usize> {
    if dst.len() < src.len() {
        return Err(CopyExtError::BufferTooSmall);
    }
    dst[..src.len()].copy_from_slice(src);
    Ok(src.len())
}

/// Copy bytes with offset.
pub fn copy_bytes_at(src: &[u8], dst: &mut [u8], offset: usize) -> Result<usize> {
    if dst.len() < offset + src.len() {
        return Err(CopyExtError::BufferTooSmall);
    }
    dst[offset..offset + src.len()].copy_from_slice(src);
    Ok(src.len())
}

/// Zero-copy view.
#[derive(Debug, Clone, Copy)]
pub struct CopyView<'a, T> {
    data: &'a [T],
}

impl<'a, T> CopyView<'a, T> {
    /// Create new view.
    pub fn new(data: &'a [T]) -> Self {
        Self { data }
    }

    /// Get slice.
    pub fn as_slice(&self) -> &[T] {
        self.data
    }

    /// Get length.
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Get element.
    pub fn get(&self, index: usize) -> Option<&T> {
        self.data.get(index)
    }

    /// Subview.
    pub fn subview(&self, start: usize, end: usize) -> Option<Self> {
        if start <= end && end <= self.data.len() {
            Some(Self::new(&self.data[start..end]))
        } else {
            None
        }
    }
}

impl<'a, T: Copy> CopyView<'a, T> {
    /// Copy to vec.
    pub fn to_vec(&self) -> Vec<T> {
        self.data.to_vec()
    }

    /// Copy to slice.
    pub fn copy_to(&self, dst: &mut [T]) -> Result<usize> {
        if dst.len() < self.data.len() {
            return Err(CopyExtError::BufferTooSmall);
        }
        dst[..self.data.len()].copy_from_slice(self.data);
        Ok(self.data.len())
    }
}

/// Copyable wrapper.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Copyable<T: Copy>(pub T);

impl<T: Copy> Copyable<T> {
    /// Create new.
    pub fn new(value: T) -> Self {
        Self(value)
    }

    /// Get value.
    pub fn get(&self) -> T {
        self.0
    }

    /// Set value.
    pub fn set(&mut self, value: T) {
        self.0 = value;
    }

    /// Into inner.
    pub fn into_inner(self) -> T {
        self.0
    }

    /// Map value.
    pub fn map<U: Copy, F: FnOnce(T) -> U>(self, f: F) -> Copyable<U> {
        Copyable(f(self.0))
    }
}

impl<T: Copy + Default> Default for Copyable<T> {
    fn default() -> Self {
        Self(T::default())
    }
}

/// Memory copy utilities.
pub mod mem_copy {
    use super::*;

    /// Copy value to bytes.
    pub fn to_bytes<T: Copy>(value: &T) -> Vec<u8> {
        let ptr = value as *const T as *const u8;
        let len = std::mem::size_of::<T>();
        let slice = unsafe { std::slice::from_raw_parts(ptr, len) };
        slice.to_vec()
    }

    /// Copy bytes to value (unsafe).
    pub unsafe fn from_bytes<T: Copy>(bytes: &[u8]) -> Result<T> {
        if bytes.len() < std::mem::size_of::<T>() {
            return Err(CopyExtError::BufferTooSmall);
        }
        let ptr = bytes.as_ptr() as *const T;
        Ok(std::ptr::read_unaligned(ptr))
    }
}

/// Swap values.
pub fn swap<T>(a: &mut T, b: &mut T) {
    std::mem::swap(a, b);
}

/// Replace value.
pub fn replace<T>(dest: &mut T, src: T) -> T {
    std::mem::replace(dest, src)
}

/// Take value, leaving default.
pub fn take<T: Default>(dest: &mut T) -> T {
    std::mem::take(dest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_copy_ext() {
        let v = 42;
        assert_eq!(v.copy_if(true), Some(42));
        assert_eq!(v.copy_n(3), vec![42, 42, 42]);
        assert_eq!(v.copy_array::<3>(), [42, 42, 42]);
    }

    #[test]
    fn test_copy_bytes() {
        let src = [1u8, 2, 3];
        let mut dst = [0u8; 5];
        let n = copy_bytes(&src, &mut dst).unwrap();
        assert_eq!(n, 3);
        assert_eq!(&dst[..3], &src);
    }

    #[test]
    fn test_copy_view() {
        let data = [1, 2, 3, 4, 5];
        let view = CopyView::new(&data);
        assert_eq!(view.len(), 5);
        assert_eq!(view.get(2), Some(&3));

        let sub = view.subview(1, 4).unwrap();
        assert_eq!(sub.to_vec(), vec![2, 3, 4]);
    }

    #[test]
    fn test_copyable() {
        let c = Copyable::new(42);
        assert_eq!(c.get(), 42);
        let c2 = c.map(|x| x * 2);
        assert_eq!(c2.get(), 84);
    }
}
