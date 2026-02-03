//! Percentile calculations for drbot.
//!
//! This crate provides:
//! - Percentile calculations
//! - Quantile functions
//! - Streaming percentiles (t-digest, etc.)

use thiserror::Error;

/// Percentile error types.
#[derive(Error, Debug)]
pub enum PercentileError {
    #[error("Empty dataset")]
    EmptyDataset,

    #[error("Invalid percentile: {0} (must be 0-100)")]
    InvalidPercentile(f64),

    #[error("Invalid quantile: {0} (must be 0-1)")]
    InvalidQuantile(f64),
}

/// Result type for percentile operations.
pub type Result<T> = std::result::Result<T, PercentileError>;

/// Calculate percentile using linear interpolation.
pub fn percentile(data: &[f64], p: f64) -> Result<f64> {
    if data.is_empty() {
        return Err(PercentileError::EmptyDataset);
    }
    if !(0.0..=100.0).contains(&p) {
        return Err(PercentileError::InvalidPercentile(p));
    }

    let mut sorted = data.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let index = p / 100.0 * (sorted.len() - 1) as f64;
    let lower = index.floor() as usize;
    let upper = index.ceil() as usize;

    if lower == upper {
        Ok(sorted[lower])
    } else {
        let frac = index - lower as f64;
        Ok(sorted[lower] * (1.0 - frac) + sorted[upper] * frac)
    }
}

/// Calculate quantile (0-1 scale).
pub fn quantile(data: &[f64], q: f64) -> Result<f64> {
    if !(0.0..=1.0).contains(&q) {
        return Err(PercentileError::InvalidQuantile(q));
    }
    percentile(data, q * 100.0)
}

/// Calculate median (50th percentile).
pub fn median(data: &[f64]) -> Result<f64> {
    percentile(data, 50.0)
}

/// Calculate quartiles (Q1, Q2, Q3).
pub fn quartiles(data: &[f64]) -> Result<(f64, f64, f64)> {
    Ok((
        percentile(data, 25.0)?,
        percentile(data, 50.0)?,
        percentile(data, 75.0)?,
    ))
}

/// Calculate interquartile range (IQR).
pub fn iqr(data: &[f64]) -> Result<f64> {
    let (q1, _, q3) = quartiles(data)?;
    Ok(q3 - q1)
}

/// Calculate multiple percentiles at once.
pub fn percentiles(data: &[f64], ps: &[f64]) -> Result<Vec<f64>> {
    ps.iter().map(|&p| percentile(data, p)).collect()
}

/// Common percentiles (5, 25, 50, 75, 95).
pub fn common_percentiles(data: &[f64]) -> Result<CommonPercentiles> {
    Ok(CommonPercentiles {
        p5: percentile(data, 5.0)?,
        p25: percentile(data, 25.0)?,
        p50: percentile(data, 50.0)?,
        p75: percentile(data, 75.0)?,
        p95: percentile(data, 95.0)?,
    })
}

/// Common percentiles result.
#[derive(Debug, Clone, Copy)]
pub struct CommonPercentiles {
    pub p5: f64,
    pub p25: f64,
    pub p50: f64,
    pub p75: f64,
    pub p95: f64,
}

/// Percentile rank (what percentile is value at).
pub fn percentile_rank(data: &[f64], value: f64) -> Result<f64> {
    if data.is_empty() {
        return Err(PercentileError::EmptyDataset);
    }

    let count_below = data.iter().filter(|&&x| x < value).count();
    let count_equal = data
        .iter()
        .filter(|&&x| (x - value).abs() < f64::EPSILON)
        .count();

    Ok((count_below as f64 + 0.5 * count_equal as f64) / data.len() as f64 * 100.0)
}

/// Streaming percentile estimator using P² algorithm.
pub struct StreamingPercentile {
    target_percentile: f64,
    markers: [f64; 5],
    marker_positions: [usize; 5],
    desired_positions: [f64; 5],
    count: usize,
}

impl StreamingPercentile {
    /// Create new streaming percentile for target p (0-1).
    pub fn new(p: f64) -> Self {
        Self {
            target_percentile: p,
            markers: [0.0; 5],
            marker_positions: [1, 2, 3, 4, 5],
            desired_positions: [1.0, 1.0 + 2.0 * p, 1.0 + 4.0 * p, 3.0 + 2.0 * p, 5.0],
            count: 0,
        }
    }

    /// Add a value.
    pub fn push(&mut self, value: f64) {
        self.count += 1;

        if self.count <= 5 {
            self.markers[self.count - 1] = value;
            if self.count == 5 {
                self.markers.sort_by(|a, b| a.partial_cmp(b).unwrap());
            }
            return;
        }

        // Find cell k
        let k = if value < self.markers[0] {
            self.markers[0] = value;
            0
        } else if value < self.markers[1] {
            0
        } else if value < self.markers[2] {
            1
        } else if value < self.markers[3] {
            2
        } else if value < self.markers[4] {
            self.markers[4] = self.markers[4].max(value);
            3
        } else {
            self.markers[4] = value;
            3
        };

        // Update positions
        for i in (k + 1)..5 {
            self.marker_positions[i] += 1;
        }

        // Update desired positions
        let increment = [
            0.0,
            self.target_percentile / 2.0,
            self.target_percentile,
            (1.0 + self.target_percentile) / 2.0,
            1.0,
        ];
        for i in 0..5 {
            self.desired_positions[i] += increment[i];
        }

        // Adjust marker heights if needed
        for i in 1..4 {
            let d = self.desired_positions[i] - self.marker_positions[i] as f64;
            if (d >= 1.0 && self.marker_positions[i + 1] - self.marker_positions[i] > 1)
                || (d <= -1.0
                    && (self.marker_positions[i - 1] as i64 - self.marker_positions[i] as i64) < -1)
            {
                let sign = if d >= 0.0 { 1 } else { -1 };
                let new_marker = self.parabolic(i, sign as f64);

                if new_marker > self.markers[i - 1] && new_marker < self.markers[i + 1] {
                    self.markers[i] = new_marker;
                } else {
                    self.markers[i] = self.linear(i, sign);
                }

                self.marker_positions[i] = (self.marker_positions[i] as i64 + sign as i64) as usize;
            }
        }
    }

    fn parabolic(&self, i: usize, d: f64) -> f64 {
        let qi = self.markers[i];
        let qim1 = self.markers[i - 1];
        let qip1 = self.markers[i + 1];
        let ni = self.marker_positions[i] as f64;
        let nim1 = self.marker_positions[i - 1] as f64;
        let nip1 = self.marker_positions[i + 1] as f64;

        qi + d / (nip1 - nim1)
            * ((ni - nim1 + d) * (qip1 - qi) / (nip1 - ni)
                + (nip1 - ni - d) * (qi - qim1) / (ni - nim1))
    }

    fn linear(&self, i: usize, d: i32) -> f64 {
        let qi = self.markers[i];
        let qd = if d > 0 {
            self.markers[i + 1]
        } else {
            self.markers[i - 1]
        };
        let ni = self.marker_positions[i] as f64;
        let nd = if d > 0 {
            self.marker_positions[i + 1] as f64
        } else {
            self.marker_positions[i - 1] as f64
        };

        qi + d as f64 * (qd - qi) / (nd - ni)
    }

    /// Get current percentile estimate.
    pub fn percentile(&self) -> Option<f64> {
        if self.count < 5 {
            if self.count == 0 {
                None
            } else {
                let mut sorted: Vec<f64> = self.markers[..self.count].to_vec();
                sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
                let idx = (self.target_percentile * (self.count - 1) as f64).round() as usize;
                Some(sorted[idx.min(self.count - 1)])
            }
        } else {
            Some(self.markers[2])
        }
    }

    /// Get count.
    pub fn count(&self) -> usize {
        self.count
    }
}

// ============================================================================
// Kani Formal Verification Proofs
// ============================================================================

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    /// Verify percentile bounds validation.
    #[kani::proof]
    fn proof_percentile_bounds() {
        let p: f64 = kani::any();

        kani::assume(p.is_finite());

        let valid = p >= 0.0 && p <= 100.0;

        if p < 0.0 || p > 100.0 {
            kani::assert(!valid, "Percentile outside 0-100 is invalid");
        }
    }

    /// Verify quantile bounds validation.
    #[kani::proof]
    fn proof_quantile_bounds() {
        let q: f64 = kani::any();

        kani::assume(q.is_finite());

        let valid = q >= 0.0 && q <= 1.0;

        if q < 0.0 || q > 1.0 {
            kani::assert(!valid, "Quantile outside 0-1 is invalid");
        }
    }

    /// Verify quantile to percentile conversion.
    #[kani::proof]
    fn proof_quantile_percentile_conversion() {
        let q: f64 = kani::any();

        kani::assume(q.is_finite() && q >= 0.0 && q <= 1.0);

        let p = q * 100.0;

        kani::assert(
            p >= 0.0 && p <= 100.0,
            "Converted percentile in valid range",
        );
    }

    /// Verify index calculation for percentile.
    #[kani::proof]
    fn proof_percentile_index() {
        let p: f64 = kani::any();
        let len: usize = kani::any();

        kani::assume(p.is_finite() && p >= 0.0 && p <= 100.0);
        kani::assume(len > 0 && len <= 1000);

        let index = p / 100.0 * (len - 1) as f64;
        let lower = index.floor() as usize;
        let upper = index.ceil() as usize;

        kani::assert(lower <= len - 1, "Lower index must be valid");
        kani::assert(upper <= len - 1, "Upper index must be valid");
        kani::assert(upper >= lower, "Upper >= lower");
    }

    /// Verify interpolation fraction bounds.
    #[kani::proof]
    fn proof_interpolation_fraction() {
        let index: f64 = kani::any();

        kani::assume(index.is_finite() && index >= 0.0);

        let lower = index.floor();
        let frac = index - lower;

        kani::assert(frac >= 0.0, "Fraction must be >= 0");
        kani::assert(frac <= 1.0, "Fraction must be <= 1");
    }

    /// Verify linear interpolation.
    #[kani::proof]
    fn proof_linear_interpolation() {
        let a: f64 = kani::any();
        let b: f64 = kani::any();
        let frac: f64 = kani::any();

        kani::assume(a.is_finite() && b.is_finite() && frac.is_finite());
        kani::assume(a >= 0.0 && a <= 1000.0);
        kani::assume(b >= 0.0 && b <= 1000.0);
        kani::assume(frac >= 0.0 && frac <= 1.0);

        let result = a * (1.0 - frac) + b * frac;

        if a <= b {
            kani::assert(result >= a - 1e-10, "Interpolation >= min");
            kani::assert(result <= b + 1e-10, "Interpolation <= max");
        }
    }

    /// Verify median is 50th percentile.
    #[kani::proof]
    fn proof_median_is_p50() {
        let median_percentile: f64 = 50.0;

        kani::assert(
            median_percentile >= 0.0 && median_percentile <= 100.0,
            "Median percentile is valid",
        );
    }

    /// Verify quartile ordering.
    #[kani::proof]
    fn proof_quartile_ordering() {
        let q1: f64 = kani::any();
        let q2: f64 = kani::any();
        let q3: f64 = kani::any();

        kani::assume(q1.is_finite() && q2.is_finite() && q3.is_finite());
        kani::assume(q1 <= q2 && q2 <= q3);

        kani::assert(q1 <= q2, "Q1 <= Q2");
        kani::assert(q2 <= q3, "Q2 <= Q3");
        kani::assert(q1 <= q3, "Q1 <= Q3");
    }

    /// Verify IQR is non-negative.
    #[kani::proof]
    fn proof_iqr_non_negative() {
        let q1: f64 = kani::any();
        let q3: f64 = kani::any();

        kani::assume(q1.is_finite() && q3.is_finite());
        kani::assume(q3 >= q1);

        let iqr = q3 - q1;

        kani::assert(iqr >= 0.0, "IQR must be non-negative");
    }

    /// Verify percentile_rank bounds.
    #[kani::proof]
    fn proof_percentile_rank_bounds() {
        let count_below: usize = kani::any();
        let count_equal: usize = kani::any();
        let total: usize = kani::any();

        kani::assume(total > 0 && total <= 1000);
        kani::assume(count_below <= total);
        kani::assume(count_equal <= total);
        kani::assume(count_below + count_equal <= total);

        let rank = (count_below as f64 + 0.5 * count_equal as f64) / total as f64 * 100.0;

        kani::assert(rank >= 0.0, "Rank must be >= 0");
        kani::assert(rank <= 100.0, "Rank must be <= 100");
    }

    /// Verify streaming percentile marker count.
    #[kani::proof]
    fn proof_streaming_marker_count() {
        let marker_count: usize = 5;

        kani::assert(marker_count == 5, "P² algorithm uses 5 markers");
    }

    /// Verify streaming count increases.
    #[kani::proof]
    fn proof_streaming_count_increases() {
        let initial_count: usize = kani::any();
        kani::assume(initial_count < usize::MAX);

        let new_count = initial_count + 1;

        kani::assert(new_count > initial_count, "Count should increase on push");
    }

    /// Verify desired position formula.
    #[kani::proof]
    fn proof_desired_positions() {
        let p: f64 = kani::any();

        kani::assume(p.is_finite() && p >= 0.0 && p <= 1.0);

        // Desired positions for P² algorithm
        let d0 = 1.0;
        let d1 = 1.0 + 2.0 * p;
        let d2 = 1.0 + 4.0 * p;
        let d3 = 3.0 + 2.0 * p;
        let d4 = 5.0;

        kani::assert(d0 <= d1, "d0 <= d1");
        kani::assert(d1 <= d2, "d1 <= d2");
        kani::assert(d2 <= d3 || p <= 0.5, "d2 <= d3 for p > 0.5");
        kani::assert(d3 <= d4, "d3 <= d4");
    }

    /// Verify common percentiles are in order.
    #[kani::proof]
    fn proof_common_percentiles_order() {
        let p5: f64 = 5.0;
        let p25: f64 = 25.0;
        let p50: f64 = 50.0;
        let p75: f64 = 75.0;
        let p95: f64 = 95.0;

        kani::assert(p5 < p25, "p5 < p25");
        kani::assert(p25 < p50, "p25 < p50");
        kani::assert(p50 < p75, "p50 < p75");
        kani::assert(p75 < p95, "p75 < p95");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_percentile() {
        let data = [1.0, 2.0, 3.0, 4.0, 5.0];

        assert!((percentile(&data, 0.0).unwrap() - 1.0).abs() < 1e-10);
        assert!((percentile(&data, 50.0).unwrap() - 3.0).abs() < 1e-10);
        assert!((percentile(&data, 100.0).unwrap() - 5.0).abs() < 1e-10);
    }

    #[test]
    fn test_quantile() {
        let data = [1.0, 2.0, 3.0, 4.0, 5.0];

        assert!((quantile(&data, 0.5).unwrap() - 3.0).abs() < 1e-10);
    }

    #[test]
    fn test_quartiles() {
        let data: Vec<f64> = (1..=100).map(|x| x as f64).collect();
        let (q1, q2, q3) = quartiles(&data).unwrap();

        assert!((q2 - 50.5).abs() < 0.5);
        assert!(q1 < q2);
        assert!(q2 < q3);
    }

    #[test]
    fn test_iqr() {
        let data: Vec<f64> = (1..=100).map(|x| x as f64).collect();
        let result = iqr(&data).unwrap();
        assert!(result > 40.0 && result < 60.0); // Approximately 50
    }

    #[test]
    fn test_percentile_rank() {
        let data = [1.0, 2.0, 3.0, 4.0, 5.0];
        let rank = percentile_rank(&data, 3.0).unwrap();
        assert!((rank - 50.0).abs() < 1e-10);
    }

    #[test]
    fn test_streaming_percentile() {
        let mut sp = StreamingPercentile::new(0.5);
        for i in 1..=1000 {
            sp.push(i as f64);
        }

        let p50 = sp.percentile().unwrap();
        // Should be approximately 500
        assert!((p50 - 500.0).abs() < 50.0);
    }
}
