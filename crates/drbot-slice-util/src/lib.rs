//! Slice utilities for drbot.
//!
//! This crate provides:
//! - Slice operations
//! - Slice splitting
//! - Slice searching

use thiserror::Error;

/// Slice error types.
#[derive(Error, Debug, Clone)]
pub enum SliceError {
    #[error("Index out of bounds: {0}")]
    IndexOutOfBounds(usize),

    #[error("Range out of bounds")]
    RangeOutOfBounds,

    #[error("Empty slice")]
    Empty,
}

/// Result type for slice operations.
pub type Result<T> = std::result::Result<T, SliceError>;

/// Slice extension trait.
pub trait SliceExt<T> {
    /// Get first element.
    fn first_or_err(&self) -> Result<&T>;

    /// Get last element.
    fn last_or_err(&self) -> Result<&T>;

    /// Get element at index.
    fn get_or_err(&self, index: usize) -> Result<&T>;

    /// Split at index.
    fn split_at_checked(&self, mid: usize) -> Result<(&[T], &[T])>;
}

impl<T> SliceExt<T> for [T] {
    fn first_or_err(&self) -> Result<&T> {
        self.first().ok_or(SliceError::Empty)
    }

    fn last_or_err(&self) -> Result<&T> {
        self.last().ok_or(SliceError::Empty)
    }

    fn get_or_err(&self, index: usize) -> Result<&T> {
        self.get(index).ok_or(SliceError::IndexOutOfBounds(index))
    }

    fn split_at_checked(&self, mid: usize) -> Result<(&[T], &[T])> {
        if mid > self.len() {
            return Err(SliceError::RangeOutOfBounds);
        }
        Ok(self.split_at(mid))
    }
}

/// Mutable slice extension trait.
pub trait SliceMutExt<T> {
    /// Get first element mutable.
    fn first_mut_or_err(&mut self) -> Result<&mut T>;

    /// Get last element mutable.
    fn last_mut_or_err(&mut self) -> Result<&mut T>;

    /// Get element at index mutable.
    fn get_mut_or_err(&mut self, index: usize) -> Result<&mut T>;

    /// Split at index mutable.
    fn split_at_mut_checked(&mut self, mid: usize) -> Result<(&mut [T], &mut [T])>;

    /// Rotate left.
    fn rotate_left_n(&mut self, n: usize);

    /// Rotate right.
    fn rotate_right_n(&mut self, n: usize);
}

impl<T> SliceMutExt<T> for [T] {
    fn first_mut_or_err(&mut self) -> Result<&mut T> {
        self.first_mut().ok_or(SliceError::Empty)
    }

    fn last_mut_or_err(&mut self) -> Result<&mut T> {
        self.last_mut().ok_or(SliceError::Empty)
    }

    fn get_mut_or_err(&mut self, index: usize) -> Result<&mut T> {
        self.get_mut(index)
            .ok_or(SliceError::IndexOutOfBounds(index))
    }

    fn split_at_mut_checked(&mut self, mid: usize) -> Result<(&mut [T], &mut [T])> {
        if mid > self.len() {
            return Err(SliceError::RangeOutOfBounds);
        }
        Ok(self.split_at_mut(mid))
    }

    fn rotate_left_n(&mut self, n: usize) {
        if !self.is_empty() && n > 0 {
            self.rotate_left(n % self.len());
        }
    }

    fn rotate_right_n(&mut self, n: usize) {
        if !self.is_empty() && n > 0 {
            self.rotate_right(n % self.len());
        }
    }
}

/// Find index of element.
pub fn find_index<T: PartialEq>(slice: &[T], value: &T) -> Option<usize> {
    slice.iter().position(|x| x == value)
}

/// Find all indices of element.
pub fn find_all_indices<T: PartialEq>(slice: &[T], value: &T) -> Vec<usize> {
    slice
        .iter()
        .enumerate()
        .filter_map(|(i, x)| if x == value { Some(i) } else { None })
        .collect()
}

/// Count occurrences.
pub fn count<T: PartialEq>(slice: &[T], value: &T) -> usize {
    slice.iter().filter(|&x| x == value).count()
}

/// Check if slice starts with.
pub fn starts_with<T: PartialEq>(slice: &[T], prefix: &[T]) -> bool {
    slice.starts_with(prefix)
}

/// Check if slice ends with.
pub fn ends_with<T: PartialEq>(slice: &[T], suffix: &[T]) -> bool {
    slice.ends_with(suffix)
}

/// Check if slice contains.
pub fn contains<T: PartialEq>(slice: &[T], value: &T) -> bool {
    slice.contains(value)
}

/// Get subslice.
pub fn subslice<T>(slice: &[T], start: usize, end: usize) -> Result<&[T]> {
    if start > end || end > slice.len() {
        return Err(SliceError::RangeOutOfBounds);
    }
    Ok(&slice[start..end])
}

/// Split into n equal parts.
pub fn split_n<T>(slice: &[T], n: usize) -> Vec<&[T]> {
    if n == 0 || slice.is_empty() {
        return vec![];
    }
    let chunk_size = (slice.len() + n - 1) / n;
    slice.chunks(chunk_size).collect()
}

/// Interleave two slices.
pub fn interleave<'a, T>(a: &'a [T], b: &'a [T]) -> Vec<&'a T> {
    let mut result = Vec::with_capacity(a.len() + b.len());
    let mut ai = a.iter();
    let mut bi = b.iter();
    loop {
        match (ai.next(), bi.next()) {
            (Some(x), Some(y)) => {
                result.push(x);
                result.push(y);
            }
            (Some(x), None) => result.push(x),
            (None, Some(y)) => result.push(y),
            (None, None) => break,
        }
    }
    result
}

/// Get every nth element.
pub fn every_nth<T>(slice: &[T], n: usize) -> Vec<&T> {
    if n == 0 {
        return vec![];
    }
    slice.iter().step_by(n).collect()
}

/// Reverse slice into new vec.
pub fn reversed<T: Clone>(slice: &[T]) -> Vec<T> {
    slice.iter().rev().cloned().collect()
}

/// Take first n elements.
pub fn take<T>(slice: &[T], n: usize) -> &[T] {
    let n = n.min(slice.len());
    &slice[..n]
}

/// Skip first n elements.
pub fn skip<T>(slice: &[T], n: usize) -> &[T] {
    let n = n.min(slice.len());
    &slice[n..]
}

/// Take last n elements.
pub fn take_last<T>(slice: &[T], n: usize) -> &[T] {
    let n = n.min(slice.len());
    &slice[slice.len() - n..]
}

/// Skip last n elements.
pub fn skip_last<T>(slice: &[T], n: usize) -> &[T] {
    let n = n.min(slice.len());
    &slice[..slice.len() - n]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slice_ext() {
        let arr = [1, 2, 3, 4, 5];
        assert_eq!(arr.first_or_err().unwrap(), &1);
        assert_eq!(arr.last_or_err().unwrap(), &5);
        assert_eq!(arr.get_or_err(2).unwrap(), &3);
    }

    #[test]
    fn test_find_index() {
        let arr = [1, 2, 3, 2, 1];
        assert_eq!(find_index(&arr, &2), Some(1));
        assert_eq!(find_index(&arr, &5), None);
    }

    #[test]
    fn test_find_all_indices() {
        let arr = [1, 2, 3, 2, 1];
        assert_eq!(find_all_indices(&arr, &2), vec![1, 3]);
    }

    #[test]
    fn test_split_n() {
        let arr = [1, 2, 3, 4, 5];
        let parts = split_n(&arr, 2);
        assert_eq!(parts.len(), 2);
    }

    #[test]
    fn test_take_skip() {
        let arr = [1, 2, 3, 4, 5];
        assert_eq!(take(&arr, 3), &[1, 2, 3]);
        assert_eq!(skip(&arr, 2), &[3, 4, 5]);
        assert_eq!(take_last(&arr, 2), &[4, 5]);
        assert_eq!(skip_last(&arr, 2), &[1, 2, 3]);
    }
}
