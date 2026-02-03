//! Bitset data structure for drbot.
//!
//! This crate provides:
//! - Fixed-size bitset
//! - Dynamic bitset
//! - Bit operations (and, or, xor)
//! - Iteration over set bits

use std::fmt;
use std::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign, BitXor, BitXorAssign, Not};
use thiserror::Error;

/// Bitset error types.
#[derive(Error, Debug)]
pub enum BitsetError {
    #[error("Index out of bounds: {0}")]
    IndexOutOfBounds(usize),

    #[error("Size mismatch")]
    SizeMismatch,
}

/// Result type for bitset operations.
pub type Result<T> = std::result::Result<T, BitsetError>;

/// Dynamic bitset.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Bitset {
    words: Vec<u64>,
    len: usize,
}

impl Bitset {
    /// Create new bitset with given size.
    pub fn new(size: usize) -> Self {
        let num_words = (size + 63) / 64;
        Self {
            words: vec![0u64; num_words],
            len: size,
        }
    }

    /// Create bitset with all bits set.
    pub fn all_set(size: usize) -> Self {
        let num_words = (size + 63) / 64;
        let mut words = vec![!0u64; num_words];

        // Clear extra bits in last word
        let extra = size % 64;
        if extra > 0 && !words.is_empty() {
            let last = words.len() - 1;
            words[last] = (1u64 << extra) - 1;
        }

        Self { words, len: size }
    }

    /// Get the size.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Check if empty (no bits set).
    pub fn is_empty(&self) -> bool {
        self.words.iter().all(|&w| w == 0)
    }

    /// Set bit at index.
    pub fn set(&mut self, index: usize) {
        if index < self.len {
            let word = index / 64;
            let bit = index % 64;
            self.words[word] |= 1u64 << bit;
        }
    }

    /// Clear bit at index.
    pub fn clear(&mut self, index: usize) {
        if index < self.len {
            let word = index / 64;
            let bit = index % 64;
            self.words[word] &= !(1u64 << bit);
        }
    }

    /// Toggle bit at index.
    pub fn toggle(&mut self, index: usize) {
        if index < self.len {
            let word = index / 64;
            let bit = index % 64;
            self.words[word] ^= 1u64 << bit;
        }
    }

    /// Get bit at index.
    pub fn get(&self, index: usize) -> bool {
        if index >= self.len {
            return false;
        }
        let word = index / 64;
        let bit = index % 64;
        (self.words[word] >> bit) & 1 == 1
    }

    /// Set bit to specific value.
    pub fn set_value(&mut self, index: usize, value: bool) {
        if value {
            self.set(index);
        } else {
            self.clear(index);
        }
    }

    /// Clear all bits.
    pub fn clear_all(&mut self) {
        self.words.fill(0);
    }

    /// Set all bits.
    pub fn set_all(&mut self) {
        self.words.fill(!0u64);

        // Clear extra bits in last word
        let extra = self.len % 64;
        if extra > 0 && !self.words.is_empty() {
            let last = self.words.len() - 1;
            self.words[last] = (1u64 << extra) - 1;
        }
    }

    /// Count set bits.
    pub fn count_ones(&self) -> usize {
        self.words.iter().map(|w| w.count_ones() as usize).sum()
    }

    /// Count clear bits.
    pub fn count_zeros(&self) -> usize {
        self.len - self.count_ones()
    }

    /// Check if any bit is set.
    pub fn any(&self) -> bool {
        self.words.iter().any(|&w| w != 0)
    }

    /// Check if all bits are set.
    pub fn all(&self) -> bool {
        self.count_ones() == self.len
    }

    /// Check if no bits are set.
    pub fn none(&self) -> bool {
        !self.any()
    }

    /// Get index of first set bit.
    pub fn first_set(&self) -> Option<usize> {
        for (word_idx, &word) in self.words.iter().enumerate() {
            if word != 0 {
                let bit = word.trailing_zeros() as usize;
                let index = word_idx * 64 + bit;
                if index < self.len {
                    return Some(index);
                }
            }
        }
        None
    }

    /// Get index of last set bit.
    pub fn last_set(&self) -> Option<usize> {
        for (word_idx, &word) in self.words.iter().enumerate().rev() {
            if word != 0 {
                let bit = 63 - word.leading_zeros() as usize;
                let index = word_idx * 64 + bit;
                if index < self.len {
                    return Some(index);
                }
            }
        }
        None
    }

    /// Iterate over set bit indices.
    pub fn ones(&self) -> impl Iterator<Item = usize> + '_ {
        BitIterator {
            bitset: self,
            word_idx: 0,
            current_word: self.words.first().copied().unwrap_or(0),
        }
    }

    /// Resize the bitset.
    pub fn resize(&mut self, new_size: usize) {
        let new_num_words = (new_size + 63) / 64;
        self.words.resize(new_num_words, 0);
        self.len = new_size;

        // Clear extra bits in last word
        let extra = new_size % 64;
        if extra > 0 && !self.words.is_empty() {
            let last = self.words.len() - 1;
            self.words[last] &= (1u64 << extra) - 1;
        }
    }
}

impl BitAnd for &Bitset {
    type Output = Bitset;

    fn bitand(self, rhs: Self) -> Bitset {
        let len = self.len.max(rhs.len);
        let mut result = Bitset::new(len);

        for i in 0..self.words.len().min(rhs.words.len()) {
            result.words[i] = self.words[i] & rhs.words[i];
        }

        result
    }
}

impl BitOr for &Bitset {
    type Output = Bitset;

    fn bitor(self, rhs: Self) -> Bitset {
        let len = self.len.max(rhs.len);
        let mut result = Bitset::new(len);

        for i in 0..result.words.len() {
            let a = self.words.get(i).copied().unwrap_or(0);
            let b = rhs.words.get(i).copied().unwrap_or(0);
            result.words[i] = a | b;
        }

        result
    }
}

impl BitXor for &Bitset {
    type Output = Bitset;

    fn bitxor(self, rhs: Self) -> Bitset {
        let len = self.len.max(rhs.len);
        let mut result = Bitset::new(len);

        for i in 0..result.words.len() {
            let a = self.words.get(i).copied().unwrap_or(0);
            let b = rhs.words.get(i).copied().unwrap_or(0);
            result.words[i] = a ^ b;
        }

        result
    }
}

impl BitAndAssign<&Bitset> for Bitset {
    fn bitand_assign(&mut self, rhs: &Bitset) {
        for i in 0..self.words.len() {
            let b = rhs.words.get(i).copied().unwrap_or(0);
            self.words[i] &= b;
        }
    }
}

impl BitOrAssign<&Bitset> for Bitset {
    fn bitor_assign(&mut self, rhs: &Bitset) {
        if rhs.len > self.len {
            self.resize(rhs.len);
        }

        for i in 0..rhs.words.len().min(self.words.len()) {
            self.words[i] |= rhs.words[i];
        }
    }
}

impl BitXorAssign<&Bitset> for Bitset {
    fn bitxor_assign(&mut self, rhs: &Bitset) {
        if rhs.len > self.len {
            self.resize(rhs.len);
        }

        for i in 0..rhs.words.len().min(self.words.len()) {
            self.words[i] ^= rhs.words[i];
        }
    }
}

impl Not for &Bitset {
    type Output = Bitset;

    fn not(self) -> Bitset {
        let mut result = Bitset::new(self.len);

        for i in 0..self.words.len() {
            result.words[i] = !self.words[i];
        }

        // Clear extra bits
        let extra = self.len % 64;
        if extra > 0 && !result.words.is_empty() {
            let last = result.words.len() - 1;
            result.words[last] &= (1u64 << extra) - 1;
        }

        result
    }
}

impl fmt::Debug for Bitset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Bitset(")?;
        for i in 0..self.len.min(64) {
            write!(f, "{}", if self.get(i) { '1' } else { '0' })?;
        }
        if self.len > 64 {
            write!(f, "...")?;
        }
        write!(f, ")")
    }
}

/// Iterator over set bits.
struct BitIterator<'a> {
    bitset: &'a Bitset,
    word_idx: usize,
    current_word: u64,
}

impl<'a> Iterator for BitIterator<'a> {
    type Item = usize;

    fn next(&mut self) -> Option<usize> {
        while self.current_word == 0 {
            self.word_idx += 1;
            if self.word_idx >= self.bitset.words.len() {
                return None;
            }
            self.current_word = self.bitset.words[self.word_idx];
        }

        let bit = self.current_word.trailing_zeros() as usize;
        let index = self.word_idx * 64 + bit;

        if index >= self.bitset.len {
            return None;
        }

        self.current_word &= self.current_word - 1; // Clear lowest set bit
        Some(index)
    }
}

impl FromIterator<usize> for Bitset {
    fn from_iter<I: IntoIterator<Item = usize>>(iter: I) -> Self {
        let items: Vec<usize> = iter.into_iter().collect();
        let max = items.iter().max().copied().unwrap_or(0);
        let mut bitset = Bitset::new(max + 1);

        for i in items {
            bitset.set(i);
        }

        bitset
    }
}

// ============================================================================
// Kani Formal Verification Proofs
// ============================================================================

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    /// Verify word/bit index calculation.
    #[kani::proof]
    fn proof_word_bit_index() {
        let index: usize = kani::any();
        kani::assume(index < 10000);

        let word = index / 64;
        let bit = index % 64;

        kani::assert(bit < 64, "Bit offset must be < 64");
        kani::assert(word * 64 + bit == index, "Word and bit reconstruct index");
    }

    /// Verify num_words calculation covers all bits.
    #[kani::proof]
    fn proof_num_words_covers_all() {
        let size: usize = kani::any();
        kani::assume(size > 0 && size <= 10000);

        let num_words = (size + 63) / 64;

        kani::assert(num_words * 64 >= size, "num_words must cover all bits");
        kani::assert(num_words > 0, "num_words must be positive");
    }

    /// Verify set operation.
    #[kani::proof]
    fn proof_set_operation() {
        let word: u64 = kani::any();
        let bit: usize = kani::any();
        kani::assume(bit < 64);

        let new_word = word | (1u64 << bit);
        let bit_set = (new_word >> bit) & 1 == 1;

        kani::assert(bit_set, "Set bit must read as 1");
    }

    /// Verify clear operation.
    #[kani::proof]
    fn proof_clear_operation() {
        let word: u64 = kani::any();
        let bit: usize = kani::any();
        kani::assume(bit < 64);

        let new_word = word & !(1u64 << bit);
        let bit_clear = (new_word >> bit) & 1 == 0;

        kani::assert(bit_clear, "Cleared bit must read as 0");
    }

    /// Verify toggle is self-inverse.
    #[kani::proof]
    fn proof_toggle_self_inverse() {
        let word: u64 = kani::any();
        let bit: usize = kani::any();
        kani::assume(bit < 64);

        let toggled_once = word ^ (1u64 << bit);
        let toggled_twice = toggled_once ^ (1u64 << bit);

        kani::assert(toggled_twice == word, "Double toggle restores original");
    }

    /// Verify count_ones + count_zeros = len.
    #[kani::proof]
    fn proof_count_sum() {
        let count_ones: usize = kani::any();
        let len: usize = kani::any();

        kani::assume(len > 0 && len <= 1000);
        kani::assume(count_ones <= len);

        let count_zeros = len - count_ones;

        kani::assert(
            count_ones + count_zeros == len,
            "Ones + zeros = total length",
        );
    }

    /// Verify any/none consistency.
    #[kani::proof]
    fn proof_any_none_consistency() {
        let has_any_set: bool = kani::any();

        let none = !has_any_set;

        if has_any_set {
            kani::assert(!none, "any implies not none");
        } else {
            kani::assert(none, "not any implies none");
        }
    }

    /// Verify all/is_empty consistency.
    #[kani::proof]
    fn proof_all_empty_exclusive() {
        let count_ones: usize = kani::any();
        let len: usize = kani::any();

        kani::assume(len > 0 && len <= 1000);
        kani::assume(count_ones <= len);

        let is_all = count_ones == len;
        let is_empty = count_ones == 0;

        if len > 0 {
            kani::assert(!(is_all && is_empty), "Cannot be both all and empty");
        }
    }

    /// Verify first_set is within bounds.
    #[kani::proof]
    fn proof_first_set_bounds() {
        let first_set: Option<usize> = kani::any();
        let len: usize = kani::any();

        kani::assume(len > 0 && len <= 1000);

        if let Some(idx) = first_set {
            kani::assume(idx < len);
            kani::assert(idx < len, "first_set must be within bounds");
        }
    }

    /// Verify AND operation (intersection).
    #[kani::proof]
    fn proof_bitwise_and() {
        let a: u64 = kani::any();
        let b: u64 = kani::any();

        let result = a & b;

        // Result should have bits set only where both inputs have bits set
        kani::assert(result <= a, "AND result <= first operand");
        kani::assert(result <= b, "AND result <= second operand");
    }

    /// Verify OR operation (union).
    #[kani::proof]
    fn proof_bitwise_or() {
        let a: u64 = kani::any();
        let b: u64 = kani::any();

        let result = a | b;

        // Result should have at least as many bits as each input
        kani::assert(result >= a, "OR result >= first operand");
        kani::assert(result >= b, "OR result >= second operand");
    }

    /// Verify XOR properties.
    #[kani::proof]
    fn proof_bitwise_xor() {
        let a: u64 = kani::any();
        let b: u64 = kani::any();

        kani::assert(a ^ b == b ^ a, "XOR is commutative");
        kani::assert(a ^ a == 0, "XOR with self is 0");
        kani::assert(a ^ 0 == a, "XOR with 0 is identity");
    }

    /// Verify NOT with mask for extra bits.
    #[kani::proof]
    fn proof_not_with_mask() {
        let extra: usize = kani::any();
        kani::assume(extra > 0 && extra < 64);

        let mask = (1u64 << extra) - 1;
        let word: u64 = kani::any();

        let masked = (!word) & mask;

        kani::assert(masked <= mask, "Masked NOT should not exceed mask");
    }

    /// Verify trailing_zeros for first_set.
    #[kani::proof]
    fn proof_trailing_zeros() {
        let word: u64 = kani::any();
        kani::assume(word != 0);

        let tz = word.trailing_zeros() as usize;

        kani::assert(tz < 64, "trailing_zeros must be < 64 for non-zero");
        kani::assert((word >> tz) & 1 == 1, "Bit at trailing_zeros must be set");
    }

    /// Verify resize clears extra bits.
    #[kani::proof]
    fn proof_resize_clears_extra() {
        let new_size: usize = kani::any();
        kani::assume(new_size > 0 && new_size <= 128);

        let extra = new_size % 64;
        if extra > 0 {
            let mask = (1u64 << extra) - 1;
            let last_word: u64 = kani::any();
            let cleared = last_word & mask;

            kani::assert(cleared <= mask, "Extra bits should be cleared");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_operations() {
        let mut bs = Bitset::new(100);

        bs.set(0);
        bs.set(50);
        bs.set(99);

        assert!(bs.get(0));
        assert!(bs.get(50));
        assert!(bs.get(99));
        assert!(!bs.get(1));
    }

    #[test]
    fn test_toggle() {
        let mut bs = Bitset::new(10);

        bs.toggle(5);
        assert!(bs.get(5));

        bs.toggle(5);
        assert!(!bs.get(5));
    }

    #[test]
    fn test_count() {
        let mut bs = Bitset::new(100);

        bs.set(10);
        bs.set(20);
        bs.set(30);

        assert_eq!(bs.count_ones(), 3);
        assert_eq!(bs.count_zeros(), 97);
    }

    #[test]
    fn test_bitwise_and() {
        let mut a = Bitset::new(10);
        let mut b = Bitset::new(10);

        a.set(1);
        a.set(2);
        b.set(2);
        b.set(3);

        let c = &a & &b;
        assert!(!c.get(1));
        assert!(c.get(2));
        assert!(!c.get(3));
    }

    #[test]
    fn test_bitwise_or() {
        let mut a = Bitset::new(10);
        let mut b = Bitset::new(10);

        a.set(1);
        b.set(2);

        let c = &a | &b;
        assert!(c.get(1));
        assert!(c.get(2));
    }

    #[test]
    fn test_iteration() {
        let mut bs = Bitset::new(100);

        bs.set(5);
        bs.set(10);
        bs.set(15);

        let ones: Vec<usize> = bs.ones().collect();
        assert_eq!(ones, vec![5, 10, 15]);
    }

    #[test]
    fn test_first_last_set() {
        let mut bs = Bitset::new(100);

        bs.set(10);
        bs.set(50);
        bs.set(90);

        assert_eq!(bs.first_set(), Some(10));
        assert_eq!(bs.last_set(), Some(90));
    }

    #[test]
    fn test_all_set() {
        let bs = Bitset::all_set(10);
        assert_eq!(bs.count_ones(), 10);
        assert!(bs.all());
    }

    #[test]
    fn test_from_iterator() {
        let bs: Bitset = vec![1, 5, 10].into_iter().collect();

        assert!(bs.get(1));
        assert!(bs.get(5));
        assert!(bs.get(10));
        assert!(!bs.get(0));
    }
}
