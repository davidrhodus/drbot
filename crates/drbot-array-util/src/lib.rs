//! Fixed-size array utilities for drbot.
//!
//! This crate provides:
//! - Array operations
//! - Array conversions
//! - Array utilities

use thiserror::Error;

/// Array error types.
#[derive(Error, Debug, Clone)]
pub enum ArrayError {
    #[error("Size mismatch: expected {expected}, got {actual}")]
    SizeMismatch { expected: usize, actual: usize },

    #[error("Index out of bounds: {0}")]
    IndexOutOfBounds(usize),
}

/// Result type for array operations.
pub type Result<T> = std::result::Result<T, ArrayError>;

/// Create array from function.
pub fn from_fn<T, const N: usize, F: FnMut(usize) -> T>(f: F) -> [T; N] {
    std::array::from_fn(f)
}

/// Create array filled with value.
pub fn filled<T: Clone, const N: usize>(value: T) -> [T; N] {
    std::array::from_fn(|_| value.clone())
}

/// Create array filled with default.
pub fn default_array<T: Default, const N: usize>() -> [T; N] {
    std::array::from_fn(|_| T::default())
}

/// Try convert slice to array.
pub fn try_from_slice<T: Clone, const N: usize>(slice: &[T]) -> Result<[T; N]> {
    if slice.len() != N {
        return Err(ArrayError::SizeMismatch {
            expected: N,
            actual: slice.len(),
        });
    }
    Ok(std::array::from_fn(|i| slice[i].clone()))
}

/// Map array.
pub fn map<T, U, const N: usize, F: FnMut(T) -> U>(arr: [T; N], f: F) -> [U; N] {
    arr.map(f)
}

/// Zip two arrays.
pub fn zip<T, U, const N: usize>(a: [T; N], b: [U; N]) -> [(T, U); N] {
    let mut a_iter = a.into_iter();
    let mut b_iter = b.into_iter();
    std::array::from_fn(|_| (a_iter.next().unwrap(), b_iter.next().unwrap()))
}

/// Unzip array of tuples.
pub fn unzip<T, U, const N: usize>(arr: [(T, U); N]) -> ([T; N], [U; N]) {
    let mut ts: [Option<T>; N] = std::array::from_fn(|_| None);
    let mut us: [Option<U>; N] = std::array::from_fn(|_| None);

    for (i, (t, u)) in arr.into_iter().enumerate() {
        ts[i] = Some(t);
        us[i] = Some(u);
    }

    (ts.map(|x| x.unwrap()), us.map(|x| x.unwrap()))
}

/// Reverse array.
pub fn reverse<T, const N: usize>(mut arr: [T; N]) -> [T; N] {
    arr.reverse();
    arr
}

/// Rotate array left.
pub fn rotate_left<T, const N: usize>(mut arr: [T; N], n: usize) -> [T; N] {
    if N > 0 {
        arr.rotate_left(n % N);
    }
    arr
}

/// Rotate array right.
pub fn rotate_right<T, const N: usize>(mut arr: [T; N], n: usize) -> [T; N] {
    if N > 0 {
        arr.rotate_right(n % N);
    }
    arr
}

/// Concatenate two arrays.
pub fn concat<T, const A: usize, const B: usize, const C: usize>(a: [T; A], b: [T; B]) -> [T; C] {
    assert_eq!(A + B, C, "Array sizes must match");
    let mut a_iter = a.into_iter();
    let mut b_iter = b.into_iter();
    std::array::from_fn(|i| {
        if i < A {
            a_iter.next().unwrap()
        } else {
            b_iter.next().unwrap()
        }
    })
}

/// Split array at index.
pub fn split_at<T: Clone, const N: usize, const A: usize, const B: usize>(
    arr: &[T; N],
) -> ([T; A], [T; B]) {
    assert_eq!(A + B, N, "Split sizes must match array size");
    let first = std::array::from_fn(|i| arr[i].clone());
    let second = std::array::from_fn(|i| arr[A + i].clone());
    (first, second)
}

/// Get array length.
pub const fn len<T, const N: usize>(_arr: &[T; N]) -> usize {
    N
}

/// Check if array is empty.
pub const fn is_empty<T, const N: usize>(_arr: &[T; N]) -> bool {
    N == 0
}

/// Array extension trait.
pub trait ArrayExt<T, const N: usize> {
    /// Map with index.
    fn map_indexed<U, F: FnMut(usize, T) -> U>(self, f: F) -> [U; N];

    /// Fold array.
    fn fold<B, F: FnMut(B, T) -> B>(self, init: B, f: F) -> B;

    /// All elements satisfy predicate.
    fn all<F: FnMut(&T) -> bool>(&self, f: F) -> bool;

    /// Any element satisfies predicate.
    fn any<F: FnMut(&T) -> bool>(&self, f: F) -> bool;
}

impl<T, const N: usize> ArrayExt<T, N> for [T; N] {
    fn map_indexed<U, F: FnMut(usize, T) -> U>(self, mut f: F) -> [U; N] {
        let mut idx = 0;
        self.map(|x| {
            let result = f(idx, x);
            idx += 1;
            result
        })
    }

    fn fold<B, F: FnMut(B, T) -> B>(self, init: B, f: F) -> B {
        self.into_iter().fold(init, f)
    }

    fn all<F: FnMut(&T) -> bool>(&self, mut f: F) -> bool {
        self.iter().all(|x| f(x))
    }

    fn any<F: FnMut(&T) -> bool>(&self, mut f: F) -> bool {
        self.iter().any(|x| f(x))
    }
}

/// 2D array operations.
pub mod array2d {
    /// Transpose 2D array.
    pub fn transpose<T: Copy, const R: usize, const C: usize>(arr: [[T; C]; R]) -> [[T; R]; C] {
        std::array::from_fn(|c| std::array::from_fn(|r| arr[r][c]))
    }

    /// Map 2D array.
    pub fn map<T, U, const R: usize, const C: usize, F: FnMut(T) -> U>(
        arr: [[T; C]; R],
        mut f: F,
    ) -> [[U; C]; R] {
        arr.map(|row| row.map(&mut f))
    }

    /// Flatten 2D to 1D.
    pub fn flatten<T: Copy, const R: usize, const C: usize, const N: usize>(
        arr: [[T; C]; R],
    ) -> [T; N] {
        assert_eq!(R * C, N, "Flattened size must match");
        std::array::from_fn(|i| arr[i / C][i % C])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_fn() {
        let arr: [i32; 5] = from_fn(|i| i as i32 * 2);
        assert_eq!(arr, [0, 2, 4, 6, 8]);
    }

    #[test]
    fn test_filled() {
        let arr: [i32; 3] = filled(42);
        assert_eq!(arr, [42, 42, 42]);
    }

    #[test]
    fn test_map() {
        let arr = [1, 2, 3];
        let mapped = map(arr, |x| x * 2);
        assert_eq!(mapped, [2, 4, 6]);
    }

    #[test]
    fn test_zip() {
        let a = [1, 2, 3];
        let b = ['a', 'b', 'c'];
        let zipped = zip(a, b);
        assert_eq!(zipped, [(1, 'a'), (2, 'b'), (3, 'c')]);
    }

    #[test]
    fn test_reverse() {
        let arr = [1, 2, 3];
        assert_eq!(reverse(arr), [3, 2, 1]);
    }

    #[test]
    fn test_map_indexed() {
        let arr = [10, 20, 30];
        let result = arr.map_indexed(|i, x| x + i as i32);
        assert_eq!(result, [10, 21, 32]);
    }

    #[test]
    fn test_transpose() {
        let arr = [[1, 2], [3, 4], [5, 6]];
        let transposed = array2d::transpose(arr);
        assert_eq!(transposed, [[1, 3, 5], [2, 4, 6]]);
    }
}

// ============================================================================
// Kani Formal Verification Proofs
// ============================================================================

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    // ------------------------------------------------------------------------
    // Basic Array Operations Proofs
    // ------------------------------------------------------------------------

    #[kani::proof]
    fn proof_from_fn_length() {
        let arr: [u8; 4] = from_fn(|i| i as u8);
        kani::assert(arr.len() == 4, "from_fn creates correct length");
    }

    #[kani::proof]
    fn proof_from_fn_values() {
        let arr: [u8; 3] = from_fn(|i| i as u8 * 2);
        kani::assert(arr[0] == 0, "from_fn index 0 correct");
        kani::assert(arr[1] == 2, "from_fn index 1 correct");
        kani::assert(arr[2] == 4, "from_fn index 2 correct");
    }

    #[kani::proof]
    fn proof_filled_all_same() {
        let value: u8 = kani::any();
        let arr: [u8; 3] = filled(value);

        kani::assert(arr[0] == value, "filled index 0 has value");
        kani::assert(arr[1] == value, "filled index 1 has value");
        kani::assert(arr[2] == value, "filled index 2 has value");
    }

    #[kani::proof]
    fn proof_default_array() {
        let arr: [u8; 3] = default_array();

        kani::assert(arr[0] == 0, "default_array has defaults");
        kani::assert(arr[1] == 0, "default_array has defaults");
        kani::assert(arr[2] == 0, "default_array has defaults");
    }

    #[kani::proof]
    fn proof_try_from_slice_valid() {
        let slice = [1u8, 2, 3];
        let result: Result<[u8; 3]> = try_from_slice(&slice);

        kani::assert(result.is_ok(), "Same size succeeds");
        let arr = result.unwrap();
        kani::assert(
            arr[0] == 1 && arr[1] == 2 && arr[2] == 3,
            "Values preserved",
        );
    }

    #[kani::proof]
    fn proof_try_from_slice_wrong_size() {
        let slice = [1u8, 2];
        let result: Result<[u8; 3]> = try_from_slice(&slice);

        kani::assert(result.is_err(), "Wrong size fails");
    }

    #[kani::proof]
    fn proof_map_preserves_length() {
        let arr = [1u8, 2, 3];
        let mapped = map(arr, |x| x.wrapping_add(1));

        kani::assert(mapped.len() == 3, "map preserves length");
    }

    #[kani::proof]
    fn proof_map_applies_function() {
        let v1: u8 = kani::any();
        let v2: u8 = kani::any();
        let arr = [v1, v2];
        let mapped = map(arr, |x| x.wrapping_add(1));

        kani::assert(mapped[0] == v1.wrapping_add(1), "map applies to first");
        kani::assert(mapped[1] == v2.wrapping_add(1), "map applies to second");
    }

    #[kani::proof]
    fn proof_zip_pairs_correctly() {
        let a = [1u8, 2];
        let b = [3u8, 4];
        let zipped = zip(a, b);

        kani::assert(zipped[0] == (1, 3), "First pair correct");
        kani::assert(zipped[1] == (2, 4), "Second pair correct");
    }

    #[kani::proof]
    fn proof_unzip_reverses_zip() {
        let v1: u8 = kani::any();
        let v2: u8 = kani::any();
        let v3: u8 = kani::any();
        let v4: u8 = kani::any();

        let pairs = [(v1, v2), (v3, v4)];
        let (a, b) = unzip(pairs);

        kani::assert(a[0] == v1 && a[1] == v3, "First array correct");
        kani::assert(b[0] == v2 && b[1] == v4, "Second array correct");
    }

    #[kani::proof]
    fn proof_reverse_reverses() {
        let arr = [1u8, 2, 3];
        let rev = reverse(arr);

        kani::assert(rev[0] == 3, "Reverse index 0");
        kani::assert(rev[1] == 2, "Reverse index 1");
        kani::assert(rev[2] == 1, "Reverse index 2");
    }

    #[kani::proof]
    fn proof_reverse_double_is_identity() {
        let v1: u8 = kani::any();
        let v2: u8 = kani::any();
        let arr = [v1, v2];
        let double_rev = reverse(reverse(arr));

        kani::assert(double_rev[0] == v1, "Double reverse is identity");
        kani::assert(double_rev[1] == v2, "Double reverse is identity");
    }

    #[kani::proof]
    fn proof_rotate_left_zero() {
        let arr = [1u8, 2, 3];
        let rotated = rotate_left(arr, 0);

        kani::assert(rotated == [1, 2, 3], "Rotate 0 is identity");
    }

    #[kani::proof]
    fn proof_rotate_left_one() {
        let arr = [1u8, 2, 3];
        let rotated = rotate_left(arr, 1);

        kani::assert(rotated == [2, 3, 1], "Rotate left 1");
    }

    #[kani::proof]
    fn proof_rotate_right_one() {
        let arr = [1u8, 2, 3];
        let rotated = rotate_right(arr, 1);

        kani::assert(rotated == [3, 1, 2], "Rotate right 1");
    }

    #[kani::proof]
    fn proof_rotate_full_cycle() {
        let arr = [1u8, 2, 3];
        let rotated = rotate_left(arr, 3);

        kani::assert(rotated == [1, 2, 3], "Full cycle is identity");
    }

    #[kani::proof]
    fn proof_len_correct() {
        let arr = [1u8, 2, 3, 4, 5];
        kani::assert(len(&arr) == 5, "len returns correct value");
    }

    #[kani::proof]
    fn proof_is_empty_nonempty() {
        let arr = [1u8];
        kani::assert(!is_empty(&arr), "Non-empty array is not empty");
    }

    #[kani::proof]
    fn proof_is_empty_empty() {
        let arr: [u8; 0] = [];
        kani::assert(is_empty(&arr), "Empty array is empty");
    }

    // ------------------------------------------------------------------------
    // ArrayExt Trait Proofs
    // ------------------------------------------------------------------------

    #[kani::proof]
    fn proof_fold_sum() {
        let arr = [1u8, 2, 3];
        let sum = arr.fold(0u8, |acc, x| acc.wrapping_add(x));

        kani::assert(sum == 6, "fold sum correct");
    }

    #[kani::proof]
    fn proof_all_true() {
        let arr = [2u8, 4, 6];
        let all_even = arr.all(|x| x % 2 == 0);

        kani::assert(all_even, "all returns true when all match");
    }

    #[kani::proof]
    fn proof_all_false() {
        let arr = [2u8, 3, 4];
        let all_even = arr.all(|x| x % 2 == 0);

        kani::assert(!all_even, "all returns false when one doesn't match");
    }

    #[kani::proof]
    fn proof_any_true() {
        let arr = [1u8, 2, 3];
        let any_even = arr.any(|x| x % 2 == 0);

        kani::assert(any_even, "any returns true when one matches");
    }

    #[kani::proof]
    fn proof_any_false() {
        let arr = [1u8, 3, 5];
        let any_even = arr.any(|x| x % 2 == 0);

        kani::assert(!any_even, "any returns false when none match");
    }

    // ------------------------------------------------------------------------
    // 2D Array Proofs
    // ------------------------------------------------------------------------

    #[kani::proof]
    fn proof_transpose_dimensions() {
        let arr: [[u8; 2]; 3] = [[1, 2], [3, 4], [5, 6]];
        let transposed: [[u8; 3]; 2] = array2d::transpose(arr);

        // Check dimensions are swapped
        kani::assert(transposed.len() == 2, "Transposed has correct outer dim");
        kani::assert(transposed[0].len() == 3, "Transposed has correct inner dim");
    }

    #[kani::proof]
    fn proof_transpose_values() {
        let arr: [[u8; 2]; 2] = [[1, 2], [3, 4]];
        let transposed = array2d::transpose(arr);

        kani::assert(transposed[0][0] == 1, "Transpose [0][0]");
        kani::assert(transposed[0][1] == 3, "Transpose [0][1]");
        kani::assert(transposed[1][0] == 2, "Transpose [1][0]");
        kani::assert(transposed[1][1] == 4, "Transpose [1][1]");
    }

    #[kani::proof]
    fn proof_transpose_double_is_identity() {
        let arr: [[u8; 2]; 2] = [[1, 2], [3, 4]];
        let double = array2d::transpose(array2d::transpose(arr));

        kani::assert(double == arr, "Double transpose is identity");
    }

    #[kani::proof]
    fn proof_map_2d() {
        let arr: [[u8; 2]; 2] = [[1, 2], [3, 4]];
        let mapped = array2d::map(arr, |x| x.wrapping_add(1));

        kani::assert(mapped[0][0] == 2, "map 2d applies to all");
        kani::assert(mapped[0][1] == 3, "map 2d applies to all");
        kani::assert(mapped[1][0] == 4, "map 2d applies to all");
        kani::assert(mapped[1][1] == 5, "map 2d applies to all");
    }
}
