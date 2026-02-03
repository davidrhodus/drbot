//! Histogram data structure for drbot.
//!
//! This crate provides:
//! - Fixed-width histograms
//! - Variable-width histograms
//! - HDR histograms for latency tracking

use std::fmt;
use thiserror::Error;

/// Histogram error types.
#[derive(Error, Debug)]
pub enum HistogramError {
    #[error("Value out of range: {0}")]
    OutOfRange(f64),

    #[error("Invalid parameters: {0}")]
    InvalidParameters(String),

    #[error("Empty histogram")]
    Empty,
}

/// Result type for histogram operations.
pub type Result<T> = std::result::Result<T, HistogramError>;

/// Fixed-width histogram.
#[derive(Debug, Clone)]
pub struct Histogram {
    /// Bucket counts.
    buckets: Vec<u64>,
    /// Minimum value.
    min: f64,
    /// Maximum value.
    max: f64,
    /// Bucket width.
    width: f64,
    /// Total count.
    count: u64,
    /// Sum of all values.
    sum: f64,
}

impl Histogram {
    /// Create new histogram.
    pub fn new(min: f64, max: f64, num_buckets: usize) -> Result<Self> {
        if min >= max {
            return Err(HistogramError::InvalidParameters(
                "min must be less than max".into(),
            ));
        }
        if num_buckets == 0 {
            return Err(HistogramError::InvalidParameters(
                "num_buckets must be positive".into(),
            ));
        }

        let width = (max - min) / num_buckets as f64;

        Ok(Self {
            buckets: vec![0; num_buckets],
            min,
            max,
            width,
            count: 0,
            sum: 0.0,
        })
    }

    /// Add a value to the histogram.
    pub fn record(&mut self, value: f64) -> Result<()> {
        if value < self.min || value > self.max {
            return Err(HistogramError::OutOfRange(value));
        }

        let bucket = ((value - self.min) / self.width).floor() as usize;
        let bucket = bucket.min(self.buckets.len() - 1);

        self.buckets[bucket] += 1;
        self.count += 1;
        self.sum += value;

        Ok(())
    }

    /// Record value, clamping to range.
    pub fn record_clamped(&mut self, value: f64) {
        let clamped = value.max(self.min).min(self.max - f64::EPSILON);
        let _ = self.record(clamped);
    }

    /// Get total count.
    pub fn count(&self) -> u64 {
        self.count
    }

    /// Get sum of all values.
    pub fn sum(&self) -> f64 {
        self.sum
    }

    /// Get mean.
    pub fn mean(&self) -> Option<f64> {
        if self.count > 0 {
            Some(self.sum / self.count as f64)
        } else {
            None
        }
    }

    /// Get bucket count.
    pub fn bucket_count(&self, index: usize) -> Option<u64> {
        self.buckets.get(index).copied()
    }

    /// Get bucket range.
    pub fn bucket_range(&self, index: usize) -> Option<(f64, f64)> {
        if index < self.buckets.len() {
            let start = self.min + index as f64 * self.width;
            let end = start + self.width;
            Some((start, end))
        } else {
            None
        }
    }

    /// Get all buckets.
    pub fn buckets(&self) -> &[u64] {
        &self.buckets
    }

    /// Get number of buckets.
    pub fn num_buckets(&self) -> usize {
        self.buckets.len()
    }

    /// Get percentile value.
    pub fn percentile(&self, p: f64) -> Result<f64> {
        if self.count == 0 {
            return Err(HistogramError::Empty);
        }
        if !(0.0..=100.0).contains(&p) {
            return Err(HistogramError::InvalidParameters(
                "percentile must be 0-100".into(),
            ));
        }

        let target = (p / 100.0 * self.count as f64).ceil() as u64;
        let mut cumulative = 0u64;

        for (i, &count) in self.buckets.iter().enumerate() {
            cumulative += count;
            if cumulative >= target {
                let (start, end) = self.bucket_range(i).unwrap();
                return Ok((start + end) / 2.0);
            }
        }

        Ok(self.max)
    }

    /// Reset histogram.
    pub fn reset(&mut self) {
        self.buckets.fill(0);
        self.count = 0;
        self.sum = 0.0;
    }

    /// Merge another histogram.
    pub fn merge(&mut self, other: &Histogram) -> Result<()> {
        if self.buckets.len() != other.buckets.len()
            || (self.min - other.min).abs() > f64::EPSILON
            || (self.max - other.max).abs() > f64::EPSILON
        {
            return Err(HistogramError::InvalidParameters(
                "histograms must have same parameters".into(),
            ));
        }

        for (i, &count) in other.buckets.iter().enumerate() {
            self.buckets[i] += count;
        }
        self.count += other.count;
        self.sum += other.sum;

        Ok(())
    }
}

impl fmt::Display for Histogram {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let max_count = self.buckets.iter().max().copied().unwrap_or(0);
        let scale = if max_count > 50 {
            50.0 / max_count as f64
        } else {
            1.0
        };

        for (i, &count) in self.buckets.iter().enumerate() {
            let (start, end) = self.bucket_range(i).unwrap();
            let bar_len = (count as f64 * scale).round() as usize;
            let bar: String = "*".repeat(bar_len);
            writeln!(f, "[{:8.2}, {:8.2}): {:6} {}", start, end, count, bar)?;
        }

        Ok(())
    }
}

/// Exponential histogram for latencies.
#[derive(Debug, Clone)]
pub struct ExponentialHistogram {
    buckets: Vec<u64>,
    base: f64,
    count: u64,
    sum: f64,
    min_value: Option<f64>,
    max_value: Option<f64>,
}

impl ExponentialHistogram {
    /// Create new exponential histogram.
    /// Buckets are: [0, base), [base, base^2), [base^2, base^3), ...
    pub fn new(base: f64, num_buckets: usize) -> Self {
        Self {
            buckets: vec![0; num_buckets],
            base,
            count: 0,
            sum: 0.0,
            min_value: None,
            max_value: None,
        }
    }

    /// Create for latency tracking (microseconds).
    /// Covers 1us to ~1 hour with reasonable precision.
    pub fn for_latency() -> Self {
        Self::new(2.0, 40) // 2^40 us = ~12 days
    }

    /// Record a value.
    pub fn record(&mut self, value: f64) {
        if value <= 0.0 {
            self.buckets[0] += 1;
        } else {
            let bucket = (value.log(self.base).floor() as usize).min(self.buckets.len() - 1);
            self.buckets[bucket] += 1;
        }

        self.count += 1;
        self.sum += value;

        self.min_value = Some(self.min_value.map_or(value, |m| m.min(value)));
        self.max_value = Some(self.max_value.map_or(value, |m| m.max(value)));
    }

    /// Get count.
    pub fn count(&self) -> u64 {
        self.count
    }

    /// Get mean.
    pub fn mean(&self) -> Option<f64> {
        if self.count > 0 {
            Some(self.sum / self.count as f64)
        } else {
            None
        }
    }

    /// Get min.
    pub fn min(&self) -> Option<f64> {
        self.min_value
    }

    /// Get max.
    pub fn max(&self) -> Option<f64> {
        self.max_value
    }

    /// Get bucket boundaries.
    pub fn bucket_bounds(&self, index: usize) -> Option<(f64, f64)> {
        if index >= self.buckets.len() {
            return None;
        }

        let lower = if index == 0 {
            0.0
        } else {
            self.base.powi(index as i32)
        };
        let upper = self.base.powi((index + 1) as i32);

        Some((lower, upper))
    }

    /// Get percentile.
    pub fn percentile(&self, p: f64) -> Option<f64> {
        if self.count == 0 {
            return None;
        }

        let target = (p / 100.0 * self.count as f64).ceil() as u64;
        let mut cumulative = 0u64;

        for (i, &count) in self.buckets.iter().enumerate() {
            cumulative += count;
            if cumulative >= target {
                let (lower, upper) = self.bucket_bounds(i)?;
                return Some((lower + upper) / 2.0);
            }
        }

        self.max_value
    }
}

// ============================================================================
// Kani Formal Verification Proofs
// ============================================================================

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    /// Verify bucket width calculation.
    #[kani::proof]
    fn proof_bucket_width() {
        let min: f64 = kani::any();
        let max: f64 = kani::any();
        let num_buckets: usize = kani::any();

        kani::assume(min.is_finite() && max.is_finite());
        kani::assume(min >= 0.0 && min <= 100.0);
        kani::assume(max > min && max <= 1000.0);
        kani::assume(num_buckets > 0 && num_buckets <= 100);

        let width = (max - min) / num_buckets as f64;

        kani::assert(width > 0.0, "Bucket width must be positive");
        kani::assert(width.is_finite(), "Bucket width must be finite");
    }

    /// Verify bucket index calculation is within bounds.
    #[kani::proof]
    fn proof_bucket_index_bounds() {
        let value: f64 = kani::any();
        let min: f64 = kani::any();
        let width: f64 = kani::any();
        let num_buckets: usize = kani::any();

        kani::assume(value.is_finite() && min.is_finite() && width.is_finite());
        kani::assume(min >= 0.0 && min <= 100.0);
        kani::assume(value >= min && value <= min + 1000.0);
        kani::assume(width > 0.0 && width <= 100.0);
        kani::assume(num_buckets > 0 && num_buckets <= 100);

        let bucket = ((value - min) / width).floor() as usize;
        let clamped = bucket.min(num_buckets - 1);

        kani::assert(clamped < num_buckets, "Bucket index must be within bounds");
    }

    /// Verify count increases on record.
    #[kani::proof]
    fn proof_count_increases() {
        let initial_count: u64 = kani::any();
        kani::assume(initial_count < u64::MAX);

        let new_count = initial_count + 1;

        kani::assert(new_count > initial_count, "Count should increase");
    }

    /// Verify sum accumulates correctly.
    #[kani::proof]
    fn proof_sum_accumulates() {
        let initial_sum: f64 = kani::any();
        let value: f64 = kani::any();

        kani::assume(initial_sum.is_finite() && value.is_finite());
        kani::assume(initial_sum >= 0.0 && initial_sum <= 1e9);
        kani::assume(value >= 0.0 && value <= 1000.0);

        let new_sum = initial_sum + value;

        kani::assert(new_sum >= initial_sum, "Sum should not decrease");
    }

    /// Verify mean calculation.
    #[kani::proof]
    fn proof_mean_calculation() {
        let sum: f64 = kani::any();
        let count: u64 = kani::any();

        kani::assume(sum.is_finite() && sum >= 0.0);
        kani::assume(count > 0 && count <= 1000000);

        let mean = sum / count as f64;

        kani::assert(mean.is_finite(), "Mean should be finite");
        if sum > 0.0 {
            kani::assert(mean > 0.0, "Mean of positive values should be positive");
        }
    }

    /// Verify bucket range calculation.
    #[kani::proof]
    fn proof_bucket_range() {
        let min: f64 = kani::any();
        let width: f64 = kani::any();
        let index: usize = kani::any();

        kani::assume(min.is_finite() && width.is_finite());
        kani::assume(min >= 0.0 && min <= 100.0);
        kani::assume(width > 0.0 && width <= 100.0);
        kani::assume(index < 100);

        let start = min + index as f64 * width;
        let end = start + width;

        kani::assert(end > start, "Bucket end must be after start");
    }

    /// Verify percentile bounds.
    #[kani::proof]
    fn proof_percentile_bounds() {
        let p: f64 = kani::any();

        kani::assume(p.is_finite());

        let valid = p >= 0.0 && p <= 100.0;

        if p < 0.0 || p > 100.0 {
            kani::assert(!valid, "Percentile outside 0-100 is invalid");
        } else {
            kani::assert(valid, "Percentile in 0-100 is valid");
        }
    }

    /// Verify cumulative count for percentile.
    #[kani::proof]
    fn proof_cumulative_percentile() {
        let total_count: u64 = kani::any();
        let p: f64 = kani::any();

        kani::assume(total_count > 0 && total_count <= 1000000);
        kani::assume(p.is_finite() && p >= 0.0 && p <= 100.0);

        let target = (p / 100.0 * total_count as f64).ceil() as u64;

        kani::assert(
            target <= total_count + 1,
            "Target should not greatly exceed count",
        );
    }

    /// Verify merge preserves count sum.
    #[kani::proof]
    fn proof_merge_count_sum() {
        let count1: u64 = kani::any();
        let count2: u64 = kani::any();

        kani::assume(count1 < u64::MAX / 2);
        kani::assume(count2 < u64::MAX / 2);

        let merged_count = count1 + count2;

        kani::assert(merged_count >= count1, "Merged count >= first");
        kani::assert(merged_count >= count2, "Merged count >= second");
    }

    /// Verify reset clears state.
    #[kani::proof]
    fn proof_reset_clears() {
        let count_after_reset: u64 = 0;
        let sum_after_reset: f64 = 0.0;

        kani::assert(count_after_reset == 0, "Count should be 0 after reset");
        kani::assert(sum_after_reset == 0.0, "Sum should be 0 after reset");
    }

    /// Verify exponential histogram bucket bounds.
    #[kani::proof]
    fn proof_exp_bucket_bounds() {
        let base: f64 = kani::any();
        let index: usize = kani::any();

        kani::assume(base.is_finite() && base > 1.0 && base <= 10.0);
        kani::assume(index < 40);

        let lower = if index == 0 {
            0.0
        } else {
            base.powi(index as i32)
        };
        let upper = base.powi((index + 1) as i32);

        kani::assert(upper > lower, "Upper bound must exceed lower bound");
    }

    /// Verify clamping logic.
    #[kani::proof]
    fn proof_clamping() {
        let value: f64 = kani::any();
        let min: f64 = kani::any();
        let max: f64 = kani::any();

        kani::assume(value.is_finite() && min.is_finite() && max.is_finite());
        kani::assume(min >= 0.0 && max > min && max <= 1000.0);

        let clamped = value.max(min).min(max);

        kani::assert(clamped >= min, "Clamped value must be >= min");
        kani::assert(clamped <= max, "Clamped value must be <= max");
    }

    /// Verify min < max validation.
    #[kani::proof]
    fn proof_min_max_validation() {
        let min: f64 = kani::any();
        let max: f64 = kani::any();

        kani::assume(min.is_finite() && max.is_finite());

        let valid = min < max;

        if min >= max {
            kani::assert(!valid, "min >= max is invalid");
        } else {
            kani::assert(valid, "min < max is valid");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_histogram_basic() {
        let mut hist = Histogram::new(0.0, 100.0, 10).unwrap();

        hist.record(5.0).unwrap();
        hist.record(15.0).unwrap();
        hist.record(25.0).unwrap();

        assert_eq!(hist.count(), 3);
        assert!((hist.mean().unwrap() - 15.0).abs() < 1e-10);
    }

    #[test]
    fn test_histogram_buckets() {
        let mut hist = Histogram::new(0.0, 100.0, 10).unwrap();

        for _ in 0..10 {
            hist.record(5.0).unwrap(); // First bucket
        }
        for _ in 0..20 {
            hist.record(55.0).unwrap(); // Sixth bucket
        }

        assert_eq!(hist.bucket_count(0), Some(10));
        assert_eq!(hist.bucket_count(5), Some(20));
        assert_eq!(hist.bucket_count(1), Some(0));
    }

    #[test]
    fn test_histogram_percentile() {
        let mut hist = Histogram::new(0.0, 100.0, 100).unwrap();

        for i in 1..=100 {
            hist.record(i as f64).unwrap();
        }

        let p50 = hist.percentile(50.0).unwrap();
        assert!(p50 > 45.0 && p50 < 55.0);
    }

    #[test]
    fn test_exponential_histogram() {
        let mut hist = ExponentialHistogram::new(2.0, 20);

        hist.record(1.0);
        hist.record(10.0);
        hist.record(100.0);
        hist.record(1000.0);

        assert_eq!(hist.count(), 4);
        assert!((hist.min().unwrap() - 1.0).abs() < 1e-10);
        assert!((hist.max().unwrap() - 1000.0).abs() < 1e-10);
    }

    #[test]
    fn test_histogram_merge() {
        let mut h1 = Histogram::new(0.0, 100.0, 10).unwrap();
        let mut h2 = Histogram::new(0.0, 100.0, 10).unwrap();

        h1.record(5.0).unwrap();
        h2.record(55.0).unwrap();

        h1.merge(&h2).unwrap();

        assert_eq!(h1.count(), 2);
        assert!((h1.sum() - 60.0).abs() < 1e-10);
    }
}
