//! Random number generation for drbot.
//!
//! This crate provides:
//! - Cryptographically secure random generation
//! - Random value utilities
//! - Shuffling and sampling

use ring::rand::{SecureRandom, SystemRandom};
use thiserror::Error;

/// Random error types.
#[derive(Error, Debug)]
pub enum RandomError {
    #[error("Random generation failed")]
    GenerationFailed,

    #[error("Invalid range: {0}")]
    InvalidRange(String),
}

/// Result type for random operations.
pub type Result<T> = std::result::Result<T, RandomError>;

/// Cryptographically secure random generator.
pub struct SecureRng {
    rng: SystemRandom,
}

impl SecureRng {
    /// Create new secure RNG.
    pub fn new() -> Self {
        Self {
            rng: SystemRandom::new(),
        }
    }

    /// Generate random bytes.
    pub fn bytes(&self, len: usize) -> Result<Vec<u8>> {
        let mut buf = vec![0u8; len];
        self.rng
            .fill(&mut buf)
            .map_err(|_| RandomError::GenerationFailed)?;
        Ok(buf)
    }

    /// Generate random byte array.
    pub fn byte_array<const N: usize>(&self) -> Result<[u8; N]> {
        let mut buf = [0u8; N];
        self.rng
            .fill(&mut buf)
            .map_err(|_| RandomError::GenerationFailed)?;
        Ok(buf)
    }

    /// Generate random u8.
    pub fn u8(&self) -> Result<u8> {
        let bytes = self.byte_array::<1>()?;
        Ok(bytes[0])
    }

    /// Generate random u16.
    pub fn u16(&self) -> Result<u16> {
        let bytes = self.byte_array::<2>()?;
        Ok(u16::from_le_bytes(bytes))
    }

    /// Generate random u32.
    pub fn u32(&self) -> Result<u32> {
        let bytes = self.byte_array::<4>()?;
        Ok(u32::from_le_bytes(bytes))
    }

    /// Generate random u64.
    pub fn u64(&self) -> Result<u64> {
        let bytes = self.byte_array::<8>()?;
        Ok(u64::from_le_bytes(bytes))
    }

    /// Generate random i32.
    pub fn i32(&self) -> Result<i32> {
        let bytes = self.byte_array::<4>()?;
        Ok(i32::from_le_bytes(bytes))
    }

    /// Generate random i64.
    pub fn i64(&self) -> Result<i64> {
        let bytes = self.byte_array::<8>()?;
        Ok(i64::from_le_bytes(bytes))
    }

    /// Generate random f64 in [0, 1).
    pub fn f64(&self) -> Result<f64> {
        let u = self.u64()?;
        // Use only 53 bits for mantissa precision
        Ok((u >> 11) as f64 / (1u64 << 53) as f64)
    }

    /// Generate random f64 in [min, max).
    pub fn f64_range(&self, min: f64, max: f64) -> Result<f64> {
        if min >= max {
            return Err(RandomError::InvalidRange(format!("{} >= {}", min, max)));
        }
        let f = self.f64()?;
        Ok(min + f * (max - min))
    }

    /// Generate random u32 in [0, max).
    pub fn u32_max(&self, max: u32) -> Result<u32> {
        if max == 0 {
            return Err(RandomError::InvalidRange("max cannot be 0".to_string()));
        }
        // Rejection sampling to avoid modulo bias
        let threshold = u32::MAX - (u32::MAX % max);
        loop {
            let val = self.u32()?;
            if val < threshold {
                return Ok(val % max);
            }
        }
    }

    /// Generate random u64 in [0, max).
    pub fn u64_max(&self, max: u64) -> Result<u64> {
        if max == 0 {
            return Err(RandomError::InvalidRange("max cannot be 0".to_string()));
        }
        let threshold = u64::MAX - (u64::MAX % max);
        loop {
            let val = self.u64()?;
            if val < threshold {
                return Ok(val % max);
            }
        }
    }

    /// Generate random u32 in [min, max).
    pub fn u32_range(&self, min: u32, max: u32) -> Result<u32> {
        if min >= max {
            return Err(RandomError::InvalidRange(format!("{} >= {}", min, max)));
        }
        let range = max - min;
        Ok(min + self.u32_max(range)?)
    }

    /// Generate random i32 in [min, max).
    pub fn i32_range(&self, min: i32, max: i32) -> Result<i32> {
        if min >= max {
            return Err(RandomError::InvalidRange(format!("{} >= {}", min, max)));
        }
        let range = (max - min) as u32;
        Ok(min + self.u32_max(range)? as i32)
    }

    /// Generate random bool.
    pub fn bool(&self) -> Result<bool> {
        Ok(self.u8()? & 1 == 1)
    }

    /// Generate random bool with probability.
    pub fn bool_with_probability(&self, p: f64) -> Result<bool> {
        if !(0.0..=1.0).contains(&p) {
            return Err(RandomError::InvalidRange(
                "Probability must be in [0, 1]".to_string(),
            ));
        }
        Ok(self.f64()? < p)
    }
}

impl Default for SecureRng {
    fn default() -> Self {
        Self::new()
    }
}

/// Random string generator.
pub struct RandomString {
    rng: SecureRng,
}

impl RandomString {
    /// Create new random string generator.
    pub fn new() -> Self {
        Self {
            rng: SecureRng::new(),
        }
    }

    /// Generate alphanumeric string.
    pub fn alphanumeric(&self, len: usize) -> Result<String> {
        const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
        self.from_charset(len, CHARSET)
    }

    /// Generate lowercase alphanumeric string.
    pub fn alphanumeric_lower(&self, len: usize) -> Result<String> {
        const CHARSET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
        self.from_charset(len, CHARSET)
    }

    /// Generate hex string.
    pub fn hex(&self, len: usize) -> Result<String> {
        let bytes = self.rng.bytes((len + 1) / 2)?;
        let hex: String = bytes.iter().map(|b| format!("{:02x}", b)).collect();
        Ok(hex[..len].to_string())
    }

    /// Generate numeric string.
    pub fn numeric(&self, len: usize) -> Result<String> {
        const CHARSET: &[u8] = b"0123456789";
        self.from_charset(len, CHARSET)
    }

    /// Generate alphabetic string.
    pub fn alphabetic(&self, len: usize) -> Result<String> {
        const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
        self.from_charset(len, CHARSET)
    }

    /// Generate URL-safe string.
    pub fn url_safe(&self, len: usize) -> Result<String> {
        const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
        self.from_charset(len, CHARSET)
    }

    /// Generate from custom charset.
    pub fn from_charset(&self, len: usize, charset: &[u8]) -> Result<String> {
        if charset.is_empty() {
            return Err(RandomError::InvalidRange(
                "Charset cannot be empty".to_string(),
            ));
        }

        let mut result = String::with_capacity(len);
        for _ in 0..len {
            let idx = self.rng.u32_max(charset.len() as u32)? as usize;
            result.push(charset[idx] as char);
        }
        Ok(result)
    }

    /// Generate password.
    pub fn password(&self, len: usize) -> Result<String> {
        const CHARSET: &[u8] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789!@#$%^&*";
        self.from_charset(len, CHARSET)
    }
}

impl Default for RandomString {
    fn default() -> Self {
        Self::new()
    }
}

/// Random collection utilities.
pub struct RandomCollection {
    rng: SecureRng,
}

impl RandomCollection {
    /// Create new random collection utility.
    pub fn new() -> Self {
        Self {
            rng: SecureRng::new(),
        }
    }

    /// Choose random element from slice.
    pub fn choose<'a, T>(&self, items: &'a [T]) -> Result<Option<&'a T>> {
        if items.is_empty() {
            return Ok(None);
        }
        let idx = self.rng.u32_max(items.len() as u32)? as usize;
        Ok(Some(&items[idx]))
    }

    /// Choose random element, panicking if empty.
    pub fn choose_unwrap<'a, T>(&self, items: &'a [T]) -> Result<&'a T> {
        self.choose(items)?
            .ok_or_else(|| RandomError::InvalidRange("Slice is empty".to_string()))
    }

    /// Choose n random elements (with replacement).
    pub fn choose_multiple<'a, T>(&self, items: &'a [T], n: usize) -> Result<Vec<&'a T>> {
        let mut result = Vec::with_capacity(n);
        for _ in 0..n {
            if let Some(item) = self.choose(items)? {
                result.push(item);
            }
        }
        Ok(result)
    }

    /// Sample n unique elements (without replacement).
    pub fn sample<T: Clone>(&self, items: &[T], n: usize) -> Result<Vec<T>> {
        if n > items.len() {
            return Err(RandomError::InvalidRange(
                "Sample size larger than collection".to_string(),
            ));
        }

        let mut pool: Vec<_> = items.to_vec();
        let mut result = Vec::with_capacity(n);

        for _ in 0..n {
            let idx = self.rng.u32_max(pool.len() as u32)? as usize;
            result.push(pool.swap_remove(idx));
        }

        Ok(result)
    }

    /// Shuffle slice in place.
    pub fn shuffle<T>(&self, items: &mut [T]) -> Result<()> {
        let len = items.len();
        for i in (1..len).rev() {
            let j = self.rng.u32_max((i + 1) as u32)? as usize;
            items.swap(i, j);
        }
        Ok(())
    }

    /// Return shuffled copy.
    pub fn shuffled<T: Clone>(&self, items: &[T]) -> Result<Vec<T>> {
        let mut result = items.to_vec();
        self.shuffle(&mut result)?;
        Ok(result)
    }

    /// Weighted random choice.
    pub fn weighted_choice<'a, T>(&self, items: &'a [(T, f64)]) -> Result<Option<&'a T>> {
        if items.is_empty() {
            return Ok(None);
        }

        let total_weight: f64 = items.iter().map(|(_, w)| w).sum();
        if total_weight <= 0.0 {
            return Err(RandomError::InvalidRange(
                "Total weight must be positive".to_string(),
            ));
        }

        let mut target = self.rng.f64()? * total_weight;
        for (item, weight) in items {
            target -= weight;
            if target <= 0.0 {
                return Ok(Some(item));
            }
        }

        Ok(Some(&items.last().unwrap().0))
    }
}

impl Default for RandomCollection {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_secure_rng_bytes() {
        let rng = SecureRng::new();
        let bytes = rng.bytes(32).unwrap();
        assert_eq!(bytes.len(), 32);

        let bytes2 = rng.bytes(32).unwrap();
        assert_ne!(bytes, bytes2); // Should be different
    }

    #[test]
    fn test_secure_rng_range() {
        let rng = SecureRng::new();

        for _ in 0..100 {
            let val = rng.u32_range(10, 20).unwrap();
            assert!(val >= 10 && val < 20);
        }
    }

    #[test]
    fn test_secure_rng_f64() {
        let rng = SecureRng::new();

        for _ in 0..100 {
            let val = rng.f64().unwrap();
            assert!((0.0..1.0).contains(&val));
        }
    }

    #[test]
    fn test_random_string() {
        let gen = RandomString::new();

        let s = gen.alphanumeric(16).unwrap();
        assert_eq!(s.len(), 16);
        assert!(s.chars().all(|c| c.is_alphanumeric()));

        let hex = gen.hex(8).unwrap();
        assert_eq!(hex.len(), 8);
        assert!(hex.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_random_collection() {
        let coll = RandomCollection::new();
        let items = vec![1, 2, 3, 4, 5];

        let choice = coll.choose(&items).unwrap().unwrap();
        assert!(items.contains(choice));

        let sample = coll.sample(&items, 3).unwrap();
        assert_eq!(sample.len(), 3);
        // All unique
        let mut unique: Vec<_> = sample.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), 3);
    }

    #[test]
    fn test_shuffle() {
        let coll = RandomCollection::new();
        let items = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];

        let shuffled = coll.shuffled(&items).unwrap();
        assert_eq!(shuffled.len(), items.len());

        // Contains same elements
        let mut sorted = shuffled.clone();
        sorted.sort();
        assert_eq!(sorted, items);
    }

    #[test]
    fn test_weighted_choice() {
        let coll = RandomCollection::new();
        let items = vec![("a", 1.0), ("b", 2.0), ("c", 3.0)];

        // Just verify it works
        let choice = coll.weighted_choice(&items).unwrap().unwrap();
        assert!(["a", "b", "c"].contains(choice));
    }
}
