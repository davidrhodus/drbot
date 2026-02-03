//! Vec extensions for drbot.
//!
//! This crate provides:
//! - Vec utilities
//! - Vec operations
//! - Vec builders

use thiserror::Error;

/// Vec error types.
#[derive(Error, Debug, Clone)]
pub enum VecError {
    #[error("Index out of bounds: {0}")]
    IndexOutOfBounds(usize),

    #[error("Empty vec")]
    Empty,

    #[error("Capacity exceeded")]
    CapacityExceeded,
}

/// Result type for vec operations.
pub type Result<T> = std::result::Result<T, VecError>;

/// Vec extension trait.
pub trait VecExt<T> {
    /// Get or error.
    fn get_or_err(&self, index: usize) -> Result<&T>;

    /// Get mut or error.
    fn get_mut_or_err(&mut self, index: usize) -> Result<&mut T>;

    /// Pop or error.
    fn pop_or_err(&mut self) -> Result<T>;

    /// First or error.
    fn first_or_err(&self) -> Result<&T>;

    /// Last or error.
    fn last_or_err(&self) -> Result<&T>;

    /// Remove and return.
    fn remove_at(&mut self, index: usize) -> Result<T>;

    /// Insert at index.
    fn insert_at(&mut self, index: usize, value: T) -> Result<()>;

    /// Swap remove.
    fn swap_remove_at(&mut self, index: usize) -> Result<T>;
}

impl<T> VecExt<T> for Vec<T> {
    fn get_or_err(&self, index: usize) -> Result<&T> {
        self.get(index).ok_or(VecError::IndexOutOfBounds(index))
    }

    fn get_mut_or_err(&mut self, index: usize) -> Result<&mut T> {
        self.get_mut(index).ok_or(VecError::IndexOutOfBounds(index))
    }

    fn pop_or_err(&mut self) -> Result<T> {
        self.pop().ok_or(VecError::Empty)
    }

    fn first_or_err(&self) -> Result<&T> {
        self.first().ok_or(VecError::Empty)
    }

    fn last_or_err(&self) -> Result<&T> {
        self.last().ok_or(VecError::Empty)
    }

    fn remove_at(&mut self, index: usize) -> Result<T> {
        if index >= self.len() {
            return Err(VecError::IndexOutOfBounds(index));
        }
        Ok(self.remove(index))
    }

    fn insert_at(&mut self, index: usize, value: T) -> Result<()> {
        if index > self.len() {
            return Err(VecError::IndexOutOfBounds(index));
        }
        self.insert(index, value);
        Ok(())
    }

    fn swap_remove_at(&mut self, index: usize) -> Result<T> {
        if index >= self.len() {
            return Err(VecError::IndexOutOfBounds(index));
        }
        Ok(self.swap_remove(index))
    }
}

/// Push multiple items.
pub fn push_all<T>(vec: &mut Vec<T>, items: impl IntoIterator<Item = T>) {
    vec.extend(items);
}

/// Create vec with capacity.
pub fn with_capacity<T>(capacity: usize) -> Vec<T> {
    Vec::with_capacity(capacity)
}

/// Create vec from single element.
pub fn singleton<T>(value: T) -> Vec<T> {
    vec![value]
}

/// Create vec with repeated element.
pub fn repeat<T: Clone>(value: T, count: usize) -> Vec<T> {
    vec![value; count]
}

/// Deduplicate vec.
pub fn dedup<T: PartialEq>(vec: &mut Vec<T>) {
    vec.dedup();
}

/// Deduplicate by key.
pub fn dedup_by_key<T, K: PartialEq, F: FnMut(&mut T) -> K>(vec: &mut Vec<T>, key: F) {
    vec.dedup_by_key(key);
}

/// Retain elements matching predicate.
pub fn retain<T, F: FnMut(&T) -> bool>(vec: &mut Vec<T>, predicate: F) {
    vec.retain(predicate);
}

/// Drain and collect.
pub fn drain<T>(vec: &mut Vec<T>) -> Vec<T> {
    vec.drain(..).collect()
}

/// Drain range.
pub fn drain_range<T>(vec: &mut Vec<T>, start: usize, end: usize) -> Vec<T> {
    vec.drain(start..end).collect()
}

/// Split off at index.
pub fn split_off<T>(vec: &mut Vec<T>, at: usize) -> Vec<T> {
    vec.split_off(at)
}

/// Truncate to length.
pub fn truncate<T>(vec: &mut Vec<T>, len: usize) {
    vec.truncate(len);
}

/// Resize with value.
pub fn resize<T: Clone>(vec: &mut Vec<T>, new_len: usize, value: T) {
    vec.resize(new_len, value);
}

/// Flatten nested vecs.
pub fn flatten<T>(nested: Vec<Vec<T>>) -> Vec<T> {
    nested.into_iter().flatten().collect()
}

/// Partition by predicate.
pub fn partition<T, F: FnMut(&T) -> bool>(vec: Vec<T>, predicate: F) -> (Vec<T>, Vec<T>) {
    vec.into_iter().partition(predicate)
}

/// Interleave two vecs.
pub fn interleave<T>(a: Vec<T>, b: Vec<T>) -> Vec<T> {
    let mut result = Vec::with_capacity(a.len() + b.len());
    let mut a_iter = a.into_iter();
    let mut b_iter = b.into_iter();
    loop {
        match (a_iter.next(), b_iter.next()) {
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

/// Zip two vecs.
pub fn zip<T, U>(a: Vec<T>, b: Vec<U>) -> Vec<(T, U)> {
    a.into_iter().zip(b).collect()
}

/// Unzip vec of tuples.
pub fn unzip<T, U>(vec: Vec<(T, U)>) -> (Vec<T>, Vec<U>) {
    vec.into_iter().unzip()
}

/// Vec builder.
pub struct VecBuilder<T> {
    inner: Vec<T>,
}

impl<T> VecBuilder<T> {
    /// Create new.
    pub fn new() -> Self {
        Self { inner: Vec::new() }
    }

    /// With capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            inner: Vec::with_capacity(capacity),
        }
    }

    /// Push value.
    pub fn push(mut self, value: T) -> Self {
        self.inner.push(value);
        self
    }

    /// Push multiple values.
    pub fn push_all(mut self, values: impl IntoIterator<Item = T>) -> Self {
        self.inner.extend(values);
        self
    }

    /// Build.
    pub fn build(self) -> Vec<T> {
        self.inner
    }
}

impl<T> Default for VecBuilder<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vec_ext() {
        let mut v = vec![1, 2, 3];
        assert_eq!(v.get_or_err(1).unwrap(), &2);
        assert_eq!(v.pop_or_err().unwrap(), 3);
        assert_eq!(v.first_or_err().unwrap(), &1);
    }

    #[test]
    fn test_push_all() {
        let mut v = vec![1, 2];
        push_all(&mut v, [3, 4, 5]);
        assert_eq!(v, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_flatten() {
        let nested = vec![vec![1, 2], vec![3, 4], vec![5]];
        assert_eq!(flatten(nested), vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_partition() {
        let v = vec![1, 2, 3, 4, 5];
        let (even, odd): (Vec<_>, Vec<_>) = partition(v, |x| x % 2 == 0);
        assert_eq!(even, vec![2, 4]);
        assert_eq!(odd, vec![1, 3, 5]);
    }

    #[test]
    fn test_vec_builder() {
        let v = VecBuilder::new()
            .push(1)
            .push(2)
            .push_all([3, 4, 5])
            .build();
        assert_eq!(v, vec![1, 2, 3, 4, 5]);
    }
}

// ============================================================================
// Kani Formal Verification Proofs
// ============================================================================

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    // ------------------------------------------------------------------------
    // VecExt Trait Proofs
    // ------------------------------------------------------------------------

    #[kani::proof]
    fn proof_vec_ext_get_or_err_valid() {
        let v = vec![1u8, 2, 3];
        let idx: usize = kani::any();
        kani::assume(idx < v.len());

        let result = v.get_or_err(idx);
        kani::assert(result.is_ok(), "Valid index should succeed");
        kani::assert(*result.unwrap() == v[idx], "Returns correct element");
    }

    #[kani::proof]
    fn proof_vec_ext_get_or_err_invalid() {
        let v = vec![1u8, 2, 3];
        let idx: usize = kani::any();
        kani::assume(idx >= v.len());

        let result = v.get_or_err(idx);
        kani::assert(result.is_err(), "Invalid index should fail");
    }

    #[kani::proof]
    fn proof_vec_ext_pop_or_err_nonempty() {
        let mut v = vec![1u8, 2, 3];

        let result = v.pop_or_err();
        kani::assert(result.is_ok(), "Pop from non-empty should succeed");
        kani::assert(result.unwrap() == 3, "Pop returns last element");
        kani::assert(v.len() == 2, "Length decreases after pop");
    }

    #[kani::proof]
    fn proof_vec_ext_pop_or_err_empty() {
        let mut v: Vec<u8> = Vec::new();

        let result = v.pop_or_err();
        kani::assert(result.is_err(), "Pop from empty should fail");
    }

    #[kani::proof]
    fn proof_vec_ext_first_or_err_nonempty() {
        let v = vec![1u8, 2, 3];

        let result = v.first_or_err();
        kani::assert(result.is_ok(), "First from non-empty should succeed");
        kani::assert(*result.unwrap() == 1, "First returns first element");
    }

    #[kani::proof]
    fn proof_vec_ext_first_or_err_empty() {
        let v: Vec<u8> = Vec::new();

        let result = v.first_or_err();
        kani::assert(result.is_err(), "First from empty should fail");
    }

    #[kani::proof]
    fn proof_vec_ext_last_or_err_nonempty() {
        let v = vec![1u8, 2, 3];

        let result = v.last_or_err();
        kani::assert(result.is_ok(), "Last from non-empty should succeed");
        kani::assert(*result.unwrap() == 3, "Last returns last element");
    }

    #[kani::proof]
    fn proof_vec_ext_last_or_err_empty() {
        let v: Vec<u8> = Vec::new();

        let result = v.last_or_err();
        kani::assert(result.is_err(), "Last from empty should fail");
    }

    #[kani::proof]
    fn proof_vec_ext_remove_at_valid() {
        let mut v = vec![1u8, 2, 3];

        let result = v.remove_at(1);
        kani::assert(result.is_ok(), "Valid remove should succeed");
        kani::assert(result.unwrap() == 2, "Removes correct element");
        kani::assert(v.len() == 2, "Length decreases after remove");
    }

    #[kani::proof]
    fn proof_vec_ext_remove_at_invalid() {
        let mut v = vec![1u8, 2, 3];
        let idx: usize = kani::any();
        kani::assume(idx >= v.len());

        let result = v.remove_at(idx);
        kani::assert(result.is_err(), "Invalid remove should fail");
    }

    #[kani::proof]
    fn proof_vec_ext_insert_at_valid() {
        let mut v = vec![1u8, 3];

        let result = v.insert_at(1, 2);
        kani::assert(result.is_ok(), "Valid insert should succeed");
        kani::assert(v.len() == 3, "Length increases after insert");
        kani::assert(v[1] == 2, "Element inserted at correct position");
    }

    #[kani::proof]
    fn proof_vec_ext_insert_at_end() {
        let mut v = vec![1u8, 2];

        let result = v.insert_at(2, 3);
        kani::assert(result.is_ok(), "Insert at end should succeed");
        kani::assert(v.len() == 3, "Length increases");
        kani::assert(v[2] == 3, "Element at end");
    }

    #[kani::proof]
    fn proof_vec_ext_insert_at_invalid() {
        let mut v = vec![1u8, 2];
        let idx: usize = kani::any();
        kani::assume(idx > v.len()); // Note: > not >= for insert

        let result = v.insert_at(idx, 3);
        kani::assert(result.is_err(), "Invalid insert should fail");
    }

    #[kani::proof]
    fn proof_vec_ext_swap_remove_at_valid() {
        let mut v = vec![1u8, 2, 3];

        let result = v.swap_remove_at(0);
        kani::assert(result.is_ok(), "Valid swap_remove should succeed");
        kani::assert(result.unwrap() == 1, "Returns removed element");
        kani::assert(v.len() == 2, "Length decreases");
    }

    #[kani::proof]
    fn proof_vec_ext_swap_remove_at_invalid() {
        let mut v = vec![1u8, 2, 3];
        let idx: usize = kani::any();
        kani::assume(idx >= v.len());

        let result = v.swap_remove_at(idx);
        kani::assert(result.is_err(), "Invalid swap_remove should fail");
    }

    // ------------------------------------------------------------------------
    // Utility Function Proofs
    // ------------------------------------------------------------------------

    #[kani::proof]
    fn proof_singleton() {
        let value: u8 = kani::any();
        let v = singleton(value);

        kani::assert(v.len() == 1, "Singleton has length 1");
        kani::assert(v[0] == value, "Singleton contains the value");
    }

    #[kani::proof]
    fn proof_repeat_length() {
        let value: u8 = kani::any();
        let count: usize = kani::any();
        kani::assume(count <= 5); // Bound for Kani

        let v = repeat(value, count);
        kani::assert(v.len() == count, "Repeat produces correct length");
    }

    #[kani::proof]
    fn proof_repeat_values() {
        let value: u8 = kani::any();
        let v = repeat(value, 3);

        kani::assert(v[0] == value, "All elements equal value");
        kani::assert(v[1] == value, "All elements equal value");
        kani::assert(v[2] == value, "All elements equal value");
    }

    #[kani::proof]
    fn proof_truncate() {
        let mut v = vec![1u8, 2, 3, 4, 5];
        let len: usize = kani::any();
        kani::assume(len <= 5);

        truncate(&mut v, len);
        kani::assert(v.len() <= len, "Truncate reduces length");
    }

    #[kani::proof]
    fn proof_zip_length() {
        let a = vec![1u8, 2, 3];
        let b = vec![4u8, 5];

        let zipped = zip(a, b);
        kani::assert(zipped.len() == 2, "Zip length is min of inputs");
    }

    #[kani::proof]
    fn proof_interleave_length() {
        let a = vec![1u8, 2];
        let b = vec![3u8, 4];

        let result = interleave(a, b);
        kani::assert(result.len() == 4, "Interleave length is sum of inputs");
    }

    #[kani::proof]
    fn proof_interleave_order() {
        let a = vec![1u8, 3];
        let b = vec![2u8, 4];

        let result = interleave(a, b);
        kani::assert(result[0] == 1, "First from a");
        kani::assert(result[1] == 2, "Second from b");
        kani::assert(result[2] == 3, "Third from a");
        kani::assert(result[3] == 4, "Fourth from b");
    }

    // ------------------------------------------------------------------------
    // VecBuilder Proofs
    // ------------------------------------------------------------------------

    #[kani::proof]
    fn proof_vec_builder_new_empty() {
        let builder: VecBuilder<u8> = VecBuilder::new();
        let v = builder.build();

        kani::assert(v.is_empty(), "New builder produces empty vec");
    }

    #[kani::proof]
    fn proof_vec_builder_push() {
        let v1: u8 = kani::any();
        let v2: u8 = kani::any();

        let v = VecBuilder::new().push(v1).push(v2).build();

        kani::assert(v.len() == 2, "Builder push increases length");
        kani::assert(v[0] == v1, "First element correct");
        kani::assert(v[1] == v2, "Second element correct");
    }

    #[kani::proof]
    fn proof_vec_builder_push_all() {
        let v = VecBuilder::new().push(1u8).push_all([2u8, 3, 4]).build();

        kani::assert(v.len() == 4, "push_all adds all elements");
    }
}
