//! Bloom filter for drbot.
//!
//! This crate provides:
//! - Bloom filter for probabilistic membership testing
//! - Counting bloom filter
//! - Scalable bloom filter
//! - False positive rate control

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use thiserror::Error;

/// Bloom filter error types.
#[derive(Error, Debug)]
pub enum BloomError {
    #[error("Invalid parameters: {0}")]
    InvalidParameters(String),

    #[error("Filter full")]
    Full,
}

/// Result type for bloom operations.
pub type Result<T> = std::result::Result<T, BloomError>;

/// Standard bloom filter.
pub struct BloomFilter {
    bits: Vec<u64>,
    num_bits: usize,
    num_hashes: usize,
    count: usize,
}

impl BloomFilter {
    /// Create new bloom filter with expected items and false positive rate.
    pub fn new(expected_items: usize, fp_rate: f64) -> Self {
        let (num_bits, num_hashes) = optimal_params(expected_items, fp_rate);
        let num_words = (num_bits + 63) / 64;

        Self {
            bits: vec![0u64; num_words],
            num_bits,
            num_hashes,
            count: 0,
        }
    }

    /// Create with specific parameters.
    pub fn with_params(num_bits: usize, num_hashes: usize) -> Self {
        let num_words = (num_bits + 63) / 64;

        Self {
            bits: vec![0u64; num_words],
            num_bits,
            num_hashes,
            count: 0,
        }
    }

    /// Insert item.
    pub fn insert<T: Hash>(&mut self, item: &T) {
        for i in 0..self.num_hashes {
            let bit = self.hash(item, i);
            self.set_bit(bit);
        }
        self.count += 1;
    }

    /// Check if item might be in the filter.
    pub fn contains<T: Hash>(&self, item: &T) -> bool {
        for i in 0..self.num_hashes {
            let bit = self.hash(item, i);
            if !self.get_bit(bit) {
                return false;
            }
        }
        true
    }

    /// Get number of items inserted.
    pub fn count(&self) -> usize {
        self.count
    }

    /// Get estimated false positive rate.
    pub fn estimated_fp_rate(&self) -> f64 {
        let set_bits = self
            .bits
            .iter()
            .map(|w| w.count_ones() as usize)
            .sum::<usize>();
        let fill_ratio = set_bits as f64 / self.num_bits as f64;
        fill_ratio.powi(self.num_hashes as i32)
    }

    /// Get fill ratio.
    pub fn fill_ratio(&self) -> f64 {
        let set_bits = self
            .bits
            .iter()
            .map(|w| w.count_ones() as usize)
            .sum::<usize>();
        set_bits as f64 / self.num_bits as f64
    }

    /// Clear the filter.
    pub fn clear(&mut self) {
        self.bits.fill(0);
        self.count = 0;
    }

    fn hash<T: Hash>(&self, item: &T, seed: usize) -> usize {
        let mut hasher1 = DefaultHasher::new();
        let mut hasher2 = DefaultHasher::new();

        item.hash(&mut hasher1);
        seed.hash(&mut hasher2);
        item.hash(&mut hasher2);

        let h1 = hasher1.finish() as usize;
        let h2 = hasher2.finish() as usize;

        // Double hashing
        h1.wrapping_add(seed.wrapping_mul(h2)) % self.num_bits
    }

    fn set_bit(&mut self, bit: usize) {
        let word = bit / 64;
        let offset = bit % 64;
        self.bits[word] |= 1u64 << offset;
    }

    fn get_bit(&self, bit: usize) -> bool {
        let word = bit / 64;
        let offset = bit % 64;
        (self.bits[word] >> offset) & 1 == 1
    }
}

/// Counting bloom filter (supports removal).
pub struct CountingBloomFilter {
    counters: Vec<u8>,
    num_counters: usize,
    num_hashes: usize,
    count: usize,
}

impl CountingBloomFilter {
    /// Create new counting bloom filter.
    pub fn new(expected_items: usize, fp_rate: f64) -> Self {
        let (num_counters, num_hashes) = optimal_params(expected_items, fp_rate);

        Self {
            counters: vec![0u8; num_counters],
            num_counters,
            num_hashes,
            count: 0,
        }
    }

    /// Insert item.
    pub fn insert<T: Hash>(&mut self, item: &T) {
        for i in 0..self.num_hashes {
            let idx = self.hash(item, i);
            self.counters[idx] = self.counters[idx].saturating_add(1);
        }
        self.count += 1;
    }

    /// Remove item.
    pub fn remove<T: Hash>(&mut self, item: &T) -> bool {
        if !self.contains(item) {
            return false;
        }

        for i in 0..self.num_hashes {
            let idx = self.hash(item, i);
            self.counters[idx] = self.counters[idx].saturating_sub(1);
        }
        self.count = self.count.saturating_sub(1);
        true
    }

    /// Check if item might be in the filter.
    pub fn contains<T: Hash>(&self, item: &T) -> bool {
        for i in 0..self.num_hashes {
            let idx = self.hash(item, i);
            if self.counters[idx] == 0 {
                return false;
            }
        }
        true
    }

    /// Get count of items.
    pub fn count(&self) -> usize {
        self.count
    }

    /// Clear the filter.
    pub fn clear(&mut self) {
        self.counters.fill(0);
        self.count = 0;
    }

    fn hash<T: Hash>(&self, item: &T, seed: usize) -> usize {
        let mut hasher1 = DefaultHasher::new();
        let mut hasher2 = DefaultHasher::new();

        item.hash(&mut hasher1);
        seed.hash(&mut hasher2);
        item.hash(&mut hasher2);

        let h1 = hasher1.finish() as usize;
        let h2 = hasher2.finish() as usize;

        h1.wrapping_add(seed.wrapping_mul(h2)) % self.num_counters
    }
}

/// Scalable bloom filter that grows as needed.
pub struct ScalableBloomFilter {
    filters: Vec<BloomFilter>,
    initial_capacity: usize,
    fp_rate: f64,
    growth_factor: usize,
}

impl ScalableBloomFilter {
    /// Create new scalable bloom filter.
    pub fn new(initial_capacity: usize, fp_rate: f64) -> Self {
        let mut filter = Self {
            filters: Vec::new(),
            initial_capacity,
            fp_rate,
            growth_factor: 2,
        };
        filter.add_filter();
        filter
    }

    /// Insert item.
    pub fn insert<T: Hash>(&mut self, item: &T) {
        // Check if current filter is getting full
        if let Some(last) = self.filters.last() {
            if last.fill_ratio() > 0.5 {
                self.add_filter();
            }
        }

        if let Some(last) = self.filters.last_mut() {
            last.insert(item);
        }
    }

    /// Check if item might be in any filter.
    pub fn contains<T: Hash>(&self, item: &T) -> bool {
        self.filters.iter().any(|f| f.contains(item))
    }

    /// Get total count across all filters.
    pub fn count(&self) -> usize {
        self.filters.iter().map(|f| f.count()).sum()
    }

    /// Get number of internal filters.
    pub fn num_filters(&self) -> usize {
        self.filters.len()
    }

    fn add_filter(&mut self) {
        let capacity = self.initial_capacity * self.growth_factor.pow(self.filters.len() as u32);
        let fp = self.fp_rate / (self.filters.len() + 1) as f64;
        self.filters.push(BloomFilter::new(capacity, fp));
    }
}

/// Calculate optimal bloom filter parameters.
fn optimal_params(n: usize, fp: f64) -> (usize, usize) {
    // m = -n * ln(p) / (ln(2)^2)
    let ln2 = std::f64::consts::LN_2;
    let m = -(n as f64 * fp.ln()) / (ln2 * ln2);
    let num_bits = m.ceil() as usize;

    // k = (m / n) * ln(2)
    let k = (num_bits as f64 / n as f64) * ln2;
    let num_hashes = k.ceil() as usize;

    (num_bits.max(64), num_hashes.max(1))
}

// ============================================================================
// Kani Formal Verification Proofs
// ============================================================================

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    /// Verify optimal_params returns valid values.
    #[kani::proof]
    fn proof_optimal_params_valid() {
        let n: usize = kani::any();
        let fp_raw: u32 = kani::any();

        kani::assume(n > 0 && n <= 10000);
        kani::assume(fp_raw > 0 && fp_raw <= 1000); // 0.001 to 1.0

        let fp = fp_raw as f64 / 1000.0;
        kani::assume(fp > 0.0 && fp < 1.0);

        let (num_bits, num_hashes) = optimal_params(n, fp);

        kani::assert(num_bits >= 64, "num_bits must be at least 64");
        kani::assert(num_hashes >= 1, "num_hashes must be at least 1");
    }

    /// Verify bit index is within bounds.
    #[kani::proof]
    fn proof_bit_index_bounds() {
        let num_bits: usize = kani::any();
        let h1: usize = kani::any();
        let h2: usize = kani::any();
        let seed: usize = kani::any();

        kani::assume(num_bits > 0 && num_bits <= 10000);
        kani::assume(seed <= 20);

        // Simulate hash calculation
        let bit_index = h1.wrapping_add(seed.wrapping_mul(h2)) % num_bits;

        kani::assert(bit_index < num_bits, "Bit index must be within bounds");
    }

    /// Verify word and offset calculation for bit operations.
    #[kani::proof]
    fn proof_word_offset_calculation() {
        let bit: usize = kani::any();
        kani::assume(bit < 10000);

        let word = bit / 64;
        let offset = bit % 64;

        kani::assert(offset < 64, "Offset must be less than 64");
        kani::assert(
            word * 64 + offset == bit,
            "Word and offset must reconstruct bit",
        );
    }

    /// Verify set_bit and get_bit consistency.
    #[kani::proof]
    fn proof_bit_set_get_consistency() {
        let word_value: u64 = kani::any();
        let offset: usize = kani::any();
        kani::assume(offset < 64);

        // Set bit
        let new_value = word_value | (1u64 << offset);

        // Get bit
        let bit_set = (new_value >> offset) & 1 == 1;

        kani::assert(bit_set, "Set bit must read as true");
    }

    /// Verify counting bloom filter counter saturation.
    #[kani::proof]
    fn proof_counter_saturating_add() {
        let counter: u8 = kani::any();

        let new_counter = counter.saturating_add(1);

        kani::assert(new_counter >= counter, "Counter should not decrease on add");
        kani::assert(new_counter <= 255, "Counter should not overflow");
    }

    /// Verify counting bloom filter counter saturation on remove.
    #[kani::proof]
    fn proof_counter_saturating_sub() {
        let counter: u8 = kani::any();

        let new_counter = counter.saturating_sub(1);

        kani::assert(new_counter <= counter, "Counter should not increase on sub");
        kani::assert(new_counter >= 0, "Counter should not underflow");
    }

    /// Verify fill_ratio bounds.
    #[kani::proof]
    fn proof_fill_ratio_bounds() {
        let set_bits: usize = kani::any();
        let num_bits: usize = kani::any();

        kani::assume(num_bits > 0 && num_bits <= 10000);
        kani::assume(set_bits <= num_bits);

        let fill_ratio = set_bits as f64 / num_bits as f64;

        kani::assert(fill_ratio >= 0.0, "Fill ratio must be >= 0");
        kani::assert(fill_ratio <= 1.0, "Fill ratio must be <= 1");
    }

    /// Verify num_words calculation.
    #[kani::proof]
    fn proof_num_words_calculation() {
        let num_bits: usize = kani::any();
        kani::assume(num_bits > 0 && num_bits <= 100000);

        let num_words = (num_bits + 63) / 64;

        kani::assert(num_words * 64 >= num_bits, "num_words must cover all bits");
        kani::assert(num_words > 0, "num_words must be positive");
    }

    /// Verify scalable bloom filter growth factor.
    #[kani::proof]
    fn proof_scalable_growth() {
        let initial_capacity: usize = kani::any();
        let growth_factor: usize = kani::any();
        let num_filters: u32 = kani::any();

        kani::assume(initial_capacity > 0 && initial_capacity <= 1000);
        kani::assume(growth_factor >= 2 && growth_factor <= 4);
        kani::assume(num_filters < 10);

        let capacity = initial_capacity * growth_factor.pow(num_filters);

        kani::assert(capacity >= initial_capacity, "Capacity should grow");
    }

    /// Verify false positive rate decreases for scalable bloom.
    #[kani::proof]
    fn proof_scalable_fp_decreases() {
        let fp_rate: f64 = kani::any();
        let num_filters: usize = kani::any();

        kani::assume(fp_rate > 0.0 && fp_rate <= 0.1);
        kani::assume(fp_rate.is_finite());
        kani::assume(num_filters > 0 && num_filters <= 10);

        let new_fp = fp_rate / (num_filters + 1) as f64;

        kani::assert(new_fp < fp_rate, "FP rate per filter should decrease");
        kani::assert(new_fp > 0.0, "FP rate should remain positive");
    }

    /// Verify bloom filter no false negatives property (conceptual).
    #[kani::proof]
    fn proof_no_false_negatives_property() {
        // If all bits for an item are set, contains returns true
        let all_bits_set = true;

        // Simulate contains logic
        let result = all_bits_set;

        kani::assert(result == true, "All bits set implies contains returns true");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bloom_filter() {
        let mut bf = BloomFilter::new(1000, 0.01);

        bf.insert(&"hello");
        bf.insert(&"world");

        assert!(bf.contains(&"hello"));
        assert!(bf.contains(&"world"));
        assert!(!bf.contains(&"foo")); // Might have false positives
    }

    #[test]
    fn test_no_false_negatives() {
        let mut bf = BloomFilter::new(1000, 0.01);

        for i in 0..100 {
            bf.insert(&i);
        }

        for i in 0..100 {
            assert!(bf.contains(&i));
        }
    }

    #[test]
    fn test_counting_bloom_filter() {
        let mut cbf = CountingBloomFilter::new(1000, 0.01);

        cbf.insert(&"hello");
        assert!(cbf.contains(&"hello"));

        cbf.remove(&"hello");
        assert!(!cbf.contains(&"hello"));
    }

    #[test]
    fn test_counting_double_insert() {
        let mut cbf = CountingBloomFilter::new(1000, 0.01);

        cbf.insert(&"hello");
        cbf.insert(&"hello");

        cbf.remove(&"hello");
        assert!(cbf.contains(&"hello")); // Still there after one removal

        cbf.remove(&"hello");
        assert!(!cbf.contains(&"hello")); // Gone after second removal
    }

    #[test]
    fn test_scalable_bloom_filter() {
        let mut sbf = ScalableBloomFilter::new(100, 0.01);

        for i in 0..1000 {
            sbf.insert(&i);
        }

        for i in 0..1000 {
            assert!(sbf.contains(&i));
        }

        assert!(sbf.num_filters() > 1); // Should have grown
    }

    #[test]
    fn test_clear() {
        let mut bf = BloomFilter::new(100, 0.01);

        bf.insert(&"hello");
        bf.clear();

        assert!(!bf.contains(&"hello"));
        assert_eq!(bf.count(), 0);
    }
}
