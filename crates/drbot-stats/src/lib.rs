//! Statistics utilities for drbot.
//!
//! This crate provides:
//! - Descriptive statistics (mean, median, mode, variance, std dev)
//! - Running statistics (online algorithms)
//! - Correlation and covariance
//! - Statistical tests

use thiserror::Error;

/// Statistics error types.
#[derive(Error, Debug)]
pub enum StatsError {
    #[error("Empty dataset")]
    EmptyDataset,

    #[error("Insufficient data: need at least {0} values")]
    InsufficientData(usize),

    #[error("Invalid parameter: {0}")]
    InvalidParameter(String),
}

/// Result type for stats operations.
pub type Result<T> = std::result::Result<T, StatsError>;

/// Calculate mean of a slice.
pub fn mean(data: &[f64]) -> Result<f64> {
    if data.is_empty() {
        return Err(StatsError::EmptyDataset);
    }
    Ok(data.iter().sum::<f64>() / data.len() as f64)
}

/// Calculate median of a slice.
pub fn median(data: &[f64]) -> Result<f64> {
    if data.is_empty() {
        return Err(StatsError::EmptyDataset);
    }

    let mut sorted = data.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let mid = sorted.len() / 2;
    if sorted.len() % 2 == 0 {
        Ok((sorted[mid - 1] + sorted[mid]) / 2.0)
    } else {
        Ok(sorted[mid])
    }
}

/// Calculate mode of a slice (most frequent value).
pub fn mode(data: &[f64]) -> Result<f64> {
    if data.is_empty() {
        return Err(StatsError::EmptyDataset);
    }

    use std::collections::HashMap;
    let mut counts: HashMap<u64, usize> = HashMap::new();

    for &x in data {
        let key = x.to_bits();
        *counts.entry(key).or_insert(0) += 1;
    }

    let (key, _) = counts.into_iter().max_by_key(|(_, count)| *count).unwrap();

    Ok(f64::from_bits(key))
}

/// Calculate variance (population).
pub fn variance(data: &[f64]) -> Result<f64> {
    if data.is_empty() {
        return Err(StatsError::EmptyDataset);
    }

    let m = mean(data)?;
    let sum_sq: f64 = data.iter().map(|x| (x - m).powi(2)).sum();
    Ok(sum_sq / data.len() as f64)
}

/// Calculate sample variance.
pub fn sample_variance(data: &[f64]) -> Result<f64> {
    if data.len() < 2 {
        return Err(StatsError::InsufficientData(2));
    }

    let m = mean(data)?;
    let sum_sq: f64 = data.iter().map(|x| (x - m).powi(2)).sum();
    Ok(sum_sq / (data.len() - 1) as f64)
}

/// Calculate standard deviation (population).
pub fn std_dev(data: &[f64]) -> Result<f64> {
    Ok(variance(data)?.sqrt())
}

/// Calculate sample standard deviation.
pub fn sample_std_dev(data: &[f64]) -> Result<f64> {
    Ok(sample_variance(data)?.sqrt())
}

/// Calculate min value.
pub fn min(data: &[f64]) -> Result<f64> {
    data.iter()
        .copied()
        .min_by(|a, b| a.partial_cmp(b).unwrap())
        .ok_or(StatsError::EmptyDataset)
}

/// Calculate max value.
pub fn max(data: &[f64]) -> Result<f64> {
    data.iter()
        .copied()
        .max_by(|a, b| a.partial_cmp(b).unwrap())
        .ok_or(StatsError::EmptyDataset)
}

/// Calculate range (max - min).
pub fn range(data: &[f64]) -> Result<f64> {
    Ok(max(data)? - min(data)?)
}

/// Calculate sum.
pub fn sum(data: &[f64]) -> f64 {
    data.iter().sum()
}

/// Calculate covariance between two datasets.
pub fn covariance(x: &[f64], y: &[f64]) -> Result<f64> {
    if x.len() != y.len() {
        return Err(StatsError::InvalidParameter(
            "datasets must have same length".into(),
        ));
    }
    if x.is_empty() {
        return Err(StatsError::EmptyDataset);
    }

    let mean_x = mean(x)?;
    let mean_y = mean(y)?;

    let cov: f64 = x
        .iter()
        .zip(y.iter())
        .map(|(xi, yi)| (xi - mean_x) * (yi - mean_y))
        .sum();

    Ok(cov / x.len() as f64)
}

/// Calculate Pearson correlation coefficient.
pub fn correlation(x: &[f64], y: &[f64]) -> Result<f64> {
    let cov = covariance(x, y)?;
    let std_x = std_dev(x)?;
    let std_y = std_dev(y)?;

    if std_x == 0.0 || std_y == 0.0 {
        return Err(StatsError::InvalidParameter(
            "zero standard deviation".into(),
        ));
    }

    Ok(cov / (std_x * std_y))
}

/// Running statistics (Welford's online algorithm).
#[derive(Debug, Clone, Default)]
pub struct RunningStats {
    count: usize,
    mean: f64,
    m2: f64,
    min: f64,
    max: f64,
}

impl RunningStats {
    /// Create new running stats.
    pub fn new() -> Self {
        Self {
            count: 0,
            mean: 0.0,
            m2: 0.0,
            min: f64::INFINITY,
            max: f64::NEG_INFINITY,
        }
    }

    /// Add a value.
    pub fn push(&mut self, value: f64) {
        self.count += 1;
        let delta = value - self.mean;
        self.mean += delta / self.count as f64;
        let delta2 = value - self.mean;
        self.m2 += delta * delta2;

        if value < self.min {
            self.min = value;
        }
        if value > self.max {
            self.max = value;
        }
    }

    /// Get count.
    pub fn count(&self) -> usize {
        self.count
    }

    /// Get mean.
    pub fn mean(&self) -> Option<f64> {
        if self.count > 0 {
            Some(self.mean)
        } else {
            None
        }
    }

    /// Get variance.
    pub fn variance(&self) -> Option<f64> {
        if self.count > 0 {
            Some(self.m2 / self.count as f64)
        } else {
            None
        }
    }

    /// Get sample variance.
    pub fn sample_variance(&self) -> Option<f64> {
        if self.count > 1 {
            Some(self.m2 / (self.count - 1) as f64)
        } else {
            None
        }
    }

    /// Get standard deviation.
    pub fn std_dev(&self) -> Option<f64> {
        self.variance().map(|v| v.sqrt())
    }

    /// Get min.
    pub fn min(&self) -> Option<f64> {
        if self.count > 0 {
            Some(self.min)
        } else {
            None
        }
    }

    /// Get max.
    pub fn max(&self) -> Option<f64> {
        if self.count > 0 {
            Some(self.max)
        } else {
            None
        }
    }

    /// Merge with another RunningStats.
    pub fn merge(&mut self, other: &RunningStats) {
        if other.count == 0 {
            return;
        }
        if self.count == 0 {
            *self = other.clone();
            return;
        }

        let combined_count = self.count + other.count;
        let delta = other.mean - self.mean;

        self.mean = (self.count as f64 * self.mean + other.count as f64 * other.mean)
            / combined_count as f64;
        self.m2 = self.m2
            + other.m2
            + delta * delta * (self.count * other.count) as f64 / combined_count as f64;
        self.count = combined_count;
        self.min = self.min.min(other.min);
        self.max = self.max.max(other.max);
    }
}

/// Summary statistics.
#[derive(Debug, Clone)]
pub struct Summary {
    pub count: usize,
    pub sum: f64,
    pub mean: f64,
    pub median: f64,
    pub variance: f64,
    pub std_dev: f64,
    pub min: f64,
    pub max: f64,
    pub range: f64,
}

impl Summary {
    /// Calculate summary statistics.
    pub fn from_data(data: &[f64]) -> Result<Self> {
        if data.is_empty() {
            return Err(StatsError::EmptyDataset);
        }

        let count = data.len();
        let sum_val = sum(data);
        let mean_val = mean(data)?;
        let median_val = median(data)?;
        let var = variance(data)?;
        let std = std_dev(data)?;
        let min_val = min(data)?;
        let max_val = max(data)?;

        Ok(Self {
            count,
            sum: sum_val,
            mean: mean_val,
            median: median_val,
            variance: var,
            std_dev: std,
            min: min_val,
            max: max_val,
            range: max_val - min_val,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mean() {
        assert!((mean(&[1.0, 2.0, 3.0, 4.0, 5.0]).unwrap() - 3.0).abs() < 1e-10);
    }

    #[test]
    fn test_median_odd() {
        assert!((median(&[1.0, 2.0, 3.0, 4.0, 5.0]).unwrap() - 3.0).abs() < 1e-10);
    }

    #[test]
    fn test_median_even() {
        assert!((median(&[1.0, 2.0, 3.0, 4.0]).unwrap() - 2.5).abs() < 1e-10);
    }

    #[test]
    fn test_variance() {
        let data = [2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0];
        assert!((variance(&data).unwrap() - 4.0).abs() < 1e-10);
    }

    #[test]
    fn test_std_dev() {
        let data = [2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0];
        assert!((std_dev(&data).unwrap() - 2.0).abs() < 1e-10);
    }

    #[test]
    fn test_running_stats() {
        let mut rs = RunningStats::new();
        for x in &[1.0, 2.0, 3.0, 4.0, 5.0] {
            rs.push(*x);
        }

        assert_eq!(rs.count(), 5);
        assert!((rs.mean().unwrap() - 3.0).abs() < 1e-10);
        assert!((rs.min().unwrap() - 1.0).abs() < 1e-10);
        assert!((rs.max().unwrap() - 5.0).abs() < 1e-10);
    }

    #[test]
    fn test_correlation() {
        let x = [1.0, 2.0, 3.0, 4.0, 5.0];
        let y = [2.0, 4.0, 6.0, 8.0, 10.0];

        let r = correlation(&x, &y).unwrap();
        assert!((r - 1.0).abs() < 1e-10); // Perfect correlation
    }

    #[test]
    fn test_summary() {
        let data = [1.0, 2.0, 3.0, 4.0, 5.0];
        let summary = Summary::from_data(&data).unwrap();

        assert_eq!(summary.count, 5);
        assert!((summary.mean - 3.0).abs() < 1e-10);
        assert!((summary.min - 1.0).abs() < 1e-10);
        assert!((summary.max - 5.0).abs() < 1e-10);
    }
}
