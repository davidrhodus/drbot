//! Size utilities for drbot.
//!
//! This crate provides:
//! - Size computations
//! - Size formatting
//! - Size limits

use thiserror::Error;

/// Size error types.
#[derive(Error, Debug, Clone)]
pub enum SizeError {
    #[error("Size overflow")]
    Overflow,

    #[error("Size underflow")]
    Underflow,

    #[error("Invalid size")]
    Invalid,
}

/// Result type for size operations.
pub type Result<T> = std::result::Result<T, SizeError>;

/// Size of type.
pub const fn size_of<T>() -> usize {
    std::mem::size_of::<T>()
}

/// Size of value.
pub fn size_of_val<T: ?Sized>(val: &T) -> usize {
    std::mem::size_of_val(val)
}

/// Size units.
pub mod units {
    /// Byte.
    pub const BYTE: usize = 1;
    /// Kilobyte (1024).
    pub const KB: usize = 1024;
    /// Megabyte.
    pub const MB: usize = 1024 * KB;
    /// Gigabyte.
    pub const GB: usize = 1024 * MB;
    /// Terabyte.
    pub const TB: usize = 1024 * GB;

    /// Kilobyte (1000).
    pub const KB_DECIMAL: usize = 1000;
    /// Megabyte (decimal).
    pub const MB_DECIMAL: usize = 1000 * KB_DECIMAL;
    /// Gigabyte (decimal).
    pub const GB_DECIMAL: usize = 1000 * MB_DECIMAL;
    /// Terabyte (decimal).
    pub const TB_DECIMAL: usize = 1000 * GB_DECIMAL;
}

/// Size wrapper with formatting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Size(usize);

impl Size {
    /// Create from bytes.
    pub const fn new(bytes: usize) -> Self {
        Self(bytes)
    }

    /// Create from kilobytes.
    pub const fn from_kb(kb: usize) -> Self {
        Self(kb * units::KB)
    }

    /// Create from megabytes.
    pub const fn from_mb(mb: usize) -> Self {
        Self(mb * units::MB)
    }

    /// Create from gigabytes.
    pub const fn from_gb(gb: usize) -> Self {
        Self(gb * units::GB)
    }

    /// Get bytes.
    pub const fn bytes(&self) -> usize {
        self.0
    }

    /// Get kilobytes.
    pub const fn kb(&self) -> usize {
        self.0 / units::KB
    }

    /// Get megabytes.
    pub const fn mb(&self) -> usize {
        self.0 / units::MB
    }

    /// Get gigabytes.
    pub const fn gb(&self) -> usize {
        self.0 / units::GB
    }

    /// Is zero.
    pub const fn is_zero(&self) -> bool {
        self.0 == 0
    }

    /// Add sizes.
    pub fn add(&self, other: Size) -> Result<Size> {
        self.0
            .checked_add(other.0)
            .map(Size)
            .ok_or(SizeError::Overflow)
    }

    /// Subtract sizes.
    pub fn sub(&self, other: Size) -> Result<Size> {
        self.0
            .checked_sub(other.0)
            .map(Size)
            .ok_or(SizeError::Underflow)
    }

    /// Multiply size.
    pub fn mul(&self, n: usize) -> Result<Size> {
        self.0.checked_mul(n).map(Size).ok_or(SizeError::Overflow)
    }

    /// Divide size.
    pub fn div(&self, n: usize) -> Result<Size> {
        if n == 0 {
            return Err(SizeError::Invalid);
        }
        Ok(Size(self.0 / n))
    }

    /// Format as human-readable string.
    pub fn format(&self) -> String {
        let bytes = self.0;
        if bytes >= units::TB {
            format!("{:.2} TB", bytes as f64 / units::TB as f64)
        } else if bytes >= units::GB {
            format!("{:.2} GB", bytes as f64 / units::GB as f64)
        } else if bytes >= units::MB {
            format!("{:.2} MB", bytes as f64 / units::MB as f64)
        } else if bytes >= units::KB {
            format!("{:.2} KB", bytes as f64 / units::KB as f64)
        } else {
            format!("{} B", bytes)
        }
    }

    /// Format as compact string.
    pub fn format_compact(&self) -> String {
        let bytes = self.0;
        if bytes >= units::TB {
            format!("{}T", bytes / units::TB)
        } else if bytes >= units::GB {
            format!("{}G", bytes / units::GB)
        } else if bytes >= units::MB {
            format!("{}M", bytes / units::MB)
        } else if bytes >= units::KB {
            format!("{}K", bytes / units::KB)
        } else {
            format!("{}B", bytes)
        }
    }
}

impl std::fmt::Display for Size {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.format())
    }
}

impl From<usize> for Size {
    fn from(bytes: usize) -> Self {
        Self(bytes)
    }
}

impl From<Size> for usize {
    fn from(size: Size) -> Self {
        size.0
    }
}

/// Checked size operations.
pub fn checked_add(a: usize, b: usize) -> Result<usize> {
    a.checked_add(b).ok_or(SizeError::Overflow)
}

/// Checked subtraction.
pub fn checked_sub(a: usize, b: usize) -> Result<usize> {
    a.checked_sub(b).ok_or(SizeError::Underflow)
}

/// Checked multiplication.
pub fn checked_mul(a: usize, b: usize) -> Result<usize> {
    a.checked_mul(b).ok_or(SizeError::Overflow)
}

/// Compute array size.
pub fn array_size<T>(count: usize) -> Result<usize> {
    size_of::<T>().checked_mul(count).ok_or(SizeError::Overflow)
}

/// Compute total size with padding.
pub fn total_size_with_align(size: usize, align: usize, count: usize) -> Result<usize> {
    let padded_size = size.checked_add(align - 1).ok_or(SizeError::Overflow)? & !(align - 1);
    padded_size.checked_mul(count).ok_or(SizeError::Overflow)
}

/// Size limits.
#[derive(Debug, Clone, Copy)]
pub struct SizeLimits {
    /// Minimum size.
    pub min: usize,
    /// Maximum size.
    pub max: usize,
}

impl SizeLimits {
    /// Create new limits.
    pub const fn new(min: usize, max: usize) -> Self {
        Self { min, max }
    }

    /// Check if size is within limits.
    pub fn check(&self, size: usize) -> bool {
        size >= self.min && size <= self.max
    }

    /// Clamp size to limits.
    pub fn clamp(&self, size: usize) -> usize {
        size.max(self.min).min(self.max)
    }

    /// Range.
    pub fn range(&self) -> usize {
        self.max.saturating_sub(self.min)
    }
}

impl Default for SizeLimits {
    fn default() -> Self {
        Self {
            min: 0,
            max: usize::MAX,
        }
    }
}

/// Parse size from string (e.g., "10KB", "1MB").
pub fn parse_size(s: &str) -> Result<Size> {
    let s = s.trim().to_uppercase();

    if s.ends_with("TB") || s.ends_with("T") {
        let num = s.trim_end_matches(|c| c == 'T' || c == 'B');
        let n: usize = num.trim().parse().map_err(|_| SizeError::Invalid)?;
        Ok(Size(n * units::TB))
    } else if s.ends_with("GB") || s.ends_with("G") {
        let num = s.trim_end_matches(|c| c == 'G' || c == 'B');
        let n: usize = num.trim().parse().map_err(|_| SizeError::Invalid)?;
        Ok(Size(n * units::GB))
    } else if s.ends_with("MB") || s.ends_with("M") {
        let num = s.trim_end_matches(|c| c == 'M' || c == 'B');
        let n: usize = num.trim().parse().map_err(|_| SizeError::Invalid)?;
        Ok(Size(n * units::MB))
    } else if s.ends_with("KB") || s.ends_with("K") {
        let num = s.trim_end_matches(|c| c == 'K' || c == 'B');
        let n: usize = num.trim().parse().map_err(|_| SizeError::Invalid)?;
        Ok(Size(n * units::KB))
    } else if s.ends_with('B') {
        let num = s.trim_end_matches('B');
        let n: usize = num.trim().parse().map_err(|_| SizeError::Invalid)?;
        Ok(Size(n))
    } else {
        let n: usize = s.parse().map_err(|_| SizeError::Invalid)?;
        Ok(Size(n))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_size() {
        let size = Size::from_mb(1);
        assert_eq!(size.bytes(), 1024 * 1024);
        assert_eq!(size.kb(), 1024);
        assert_eq!(size.mb(), 1);
    }

    #[test]
    fn test_format() {
        assert_eq!(Size::new(500).format(), "500 B");
        assert_eq!(Size::from_kb(1).format(), "1.00 KB");
        assert_eq!(Size::from_mb(1).format(), "1.00 MB");
        assert_eq!(Size::from_gb(1).format(), "1.00 GB");
    }

    #[test]
    fn test_parse() {
        assert_eq!(parse_size("1KB").unwrap().bytes(), 1024);
        assert_eq!(parse_size("1MB").unwrap().bytes(), 1024 * 1024);
        assert_eq!(parse_size("100").unwrap().bytes(), 100);
    }

    #[test]
    fn test_operations() {
        let a = Size::from_kb(1);
        let b = Size::from_kb(2);
        assert_eq!(a.add(b).unwrap().kb(), 3);
        assert_eq!(b.sub(a).unwrap().kb(), 1);
    }

    #[test]
    fn test_limits() {
        let limits = SizeLimits::new(10, 100);
        assert!(limits.check(50));
        assert!(!limits.check(5));
        assert_eq!(limits.clamp(5), 10);
        assert_eq!(limits.clamp(150), 100);
    }
}
