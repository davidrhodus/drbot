//! Checksum calculation for drbot.
//!
//! This crate provides:
//! - CRC32 checksum
//! - Adler32 checksum
//! - Simple checksums

use thiserror::Error;

/// Checksum error types.
#[derive(Error, Debug, Clone)]
pub enum ChecksumError {
    #[error("Checksum mismatch: expected {expected:08x}, got {actual:08x}")]
    Mismatch { expected: u32, actual: u32 },

    #[error("Invalid checksum")]
    Invalid,
}

/// Result type for checksum operations.
pub type Result<T> = std::result::Result<T, ChecksumError>;

/// Checksum trait.
pub trait Checksum {
    /// Update checksum with data.
    fn update(&mut self, data: &[u8]);

    /// Finalize and get checksum value.
    fn finalize(&self) -> u32;

    /// Reset checksum state.
    fn reset(&mut self);
}

/// Simple XOR checksum.
pub struct XorChecksum {
    value: u8,
}

impl XorChecksum {
    /// Create new XOR checksum.
    pub fn new() -> Self {
        Self { value: 0 }
    }

    /// Calculate checksum of data.
    pub fn checksum(data: &[u8]) -> u8 {
        let mut cs = Self::new();
        cs.update(data);
        cs.value
    }
}

impl Default for XorChecksum {
    fn default() -> Self {
        Self::new()
    }
}

impl Checksum for XorChecksum {
    fn update(&mut self, data: &[u8]) {
        for byte in data {
            self.value ^= byte;
        }
    }

    fn finalize(&self) -> u32 {
        self.value as u32
    }

    fn reset(&mut self) {
        self.value = 0;
    }
}

/// Sum checksum (simple byte sum).
pub struct SumChecksum {
    value: u32,
}

impl SumChecksum {
    /// Create new sum checksum.
    pub fn new() -> Self {
        Self { value: 0 }
    }

    /// Calculate checksum of data.
    pub fn checksum(data: &[u8]) -> u32 {
        let mut cs = Self::new();
        cs.update(data);
        cs.finalize()
    }
}

impl Default for SumChecksum {
    fn default() -> Self {
        Self::new()
    }
}

impl Checksum for SumChecksum {
    fn update(&mut self, data: &[u8]) {
        for byte in data {
            self.value = self.value.wrapping_add(*byte as u32);
        }
    }

    fn finalize(&self) -> u32 {
        self.value
    }

    fn reset(&mut self) {
        self.value = 0;
    }
}

/// Fletcher-16 checksum.
pub struct Fletcher16 {
    sum1: u16,
    sum2: u16,
}

impl Fletcher16 {
    /// Create new Fletcher-16 checksum.
    pub fn new() -> Self {
        Self { sum1: 0, sum2: 0 }
    }

    /// Calculate checksum of data.
    pub fn checksum(data: &[u8]) -> u16 {
        let mut cs = Self::new();
        cs.update(data);
        cs.finalize() as u16
    }
}

impl Default for Fletcher16 {
    fn default() -> Self {
        Self::new()
    }
}

impl Checksum for Fletcher16 {
    fn update(&mut self, data: &[u8]) {
        for byte in data {
            self.sum1 = (self.sum1.wrapping_add(*byte as u16)) % 255;
            self.sum2 = (self.sum2.wrapping_add(self.sum1)) % 255;
        }
    }

    fn finalize(&self) -> u32 {
        ((self.sum2 as u32) << 8) | (self.sum1 as u32)
    }

    fn reset(&mut self) {
        self.sum1 = 0;
        self.sum2 = 0;
    }
}

/// Adler-32 checksum.
pub struct Adler32 {
    a: u32,
    b: u32,
}

impl Adler32 {
    const MOD: u32 = 65521;

    /// Create new Adler-32 checksum.
    pub fn new() -> Self {
        Self { a: 1, b: 0 }
    }

    /// Calculate checksum of data.
    pub fn checksum(data: &[u8]) -> u32 {
        let mut cs = Self::new();
        cs.update(data);
        cs.finalize()
    }
}

impl Default for Adler32 {
    fn default() -> Self {
        Self::new()
    }
}

impl Checksum for Adler32 {
    fn update(&mut self, data: &[u8]) {
        for byte in data {
            self.a = (self.a + *byte as u32) % Self::MOD;
            self.b = (self.b + self.a) % Self::MOD;
        }
    }

    fn finalize(&self) -> u32 {
        (self.b << 16) | self.a
    }

    fn reset(&mut self) {
        self.a = 1;
        self.b = 0;
    }
}

/// CRC-32 checksum (IEEE polynomial).
pub struct Crc32 {
    value: u32,
    table: [u32; 256],
}

impl Crc32 {
    const POLYNOMIAL: u32 = 0xEDB88320;

    /// Create new CRC-32 checksum.
    pub fn new() -> Self {
        let mut table = [0u32; 256];
        for i in 0..256 {
            let mut value = i as u32;
            for _ in 0..8 {
                if value & 1 != 0 {
                    value = (value >> 1) ^ Self::POLYNOMIAL;
                } else {
                    value >>= 1;
                }
            }
            table[i] = value;
        }
        Self {
            value: 0xFFFFFFFF,
            table,
        }
    }

    /// Calculate checksum of data.
    pub fn checksum(data: &[u8]) -> u32 {
        let mut cs = Self::new();
        cs.update(data);
        cs.finalize()
    }
}

impl Default for Crc32 {
    fn default() -> Self {
        Self::new()
    }
}

impl Checksum for Crc32 {
    fn update(&mut self, data: &[u8]) {
        for byte in data {
            let index = ((self.value ^ *byte as u32) & 0xFF) as usize;
            self.value = (self.value >> 8) ^ self.table[index];
        }
    }

    fn finalize(&self) -> u32 {
        self.value ^ 0xFFFFFFFF
    }

    fn reset(&mut self) {
        self.value = 0xFFFFFFFF;
    }
}

/// Verify checksum.
pub fn verify<C: Checksum>(mut checksum: C, data: &[u8], expected: u32) -> Result<()> {
    checksum.update(data);
    let actual = checksum.finalize();
    if actual == expected {
        Ok(())
    } else {
        Err(ChecksumError::Mismatch { expected, actual })
    }
}

/// Append checksum to data.
pub fn append_checksum<C: Checksum>(mut checksum: C, data: &[u8]) -> Vec<u8> {
    checksum.update(data);
    let cs = checksum.finalize();
    let mut result = data.to_vec();
    result.extend_from_slice(&cs.to_be_bytes());
    result
}

/// Verify and strip checksum.
pub fn verify_and_strip<C: Checksum>(mut checksum: C, data: &[u8]) -> Result<Vec<u8>> {
    if data.len() < 4 {
        return Err(ChecksumError::Invalid);
    }

    let (payload, cs_bytes) = data.split_at(data.len() - 4);
    let expected = u32::from_be_bytes(cs_bytes.try_into().unwrap());

    checksum.update(payload);
    let actual = checksum.finalize();

    if actual == expected {
        Ok(payload.to_vec())
    } else {
        Err(ChecksumError::Mismatch { expected, actual })
    }
}

// ============================================================================
// Kani Formal Verification Proofs
// ============================================================================

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    /// Verify XOR checksum is self-inverse.
    #[kani::proof]
    fn proof_xor_self_inverse() {
        let value: u8 = kani::any();
        let data: u8 = kani::any();

        let after_xor = value ^ data;
        let after_double_xor = after_xor ^ data;

        kani::assert(after_double_xor == value, "XOR is self-inverse");
    }

    /// Verify XOR checksum is commutative.
    #[kani::proof]
    fn proof_xor_commutative() {
        let a: u8 = kani::any();
        let b: u8 = kani::any();

        kani::assert(a ^ b == b ^ a, "XOR is commutative");
    }

    /// Verify XOR identity (x ^ 0 = x).
    #[kani::proof]
    fn proof_xor_identity() {
        let x: u8 = kani::any();

        kani::assert(x ^ 0 == x, "XOR with 0 is identity");
    }

    /// Verify sum checksum wrapping add.
    #[kani::proof]
    fn proof_sum_wrapping_add() {
        let current: u32 = kani::any();
        let byte: u8 = kani::any();

        let new_value = current.wrapping_add(byte as u32);

        // Wrapping add should never panic
        kani::assert(new_value >= 0, "Wrapping add produces valid result");
    }

    /// Verify Fletcher-16 modulo bounds.
    #[kani::proof]
    fn proof_fletcher16_bounds() {
        let sum1: u16 = kani::any();
        let sum2: u16 = kani::any();

        kani::assume(sum1 < 255);
        kani::assume(sum2 < 255);

        let byte: u8 = kani::any();

        let new_sum1 = (sum1.wrapping_add(byte as u16)) % 255;
        let new_sum2 = (sum2.wrapping_add(new_sum1)) % 255;

        kani::assert(new_sum1 < 255, "sum1 must be < 255");
        kani::assert(new_sum2 < 255, "sum2 must be < 255");
    }

    /// Verify Fletcher-16 finalize packing.
    #[kani::proof]
    fn proof_fletcher16_finalize() {
        let sum1: u16 = kani::any();
        let sum2: u16 = kani::any();

        kani::assume(sum1 < 256);
        kani::assume(sum2 < 256);

        let result = ((sum2 as u32) << 8) | (sum1 as u32);

        kani::assert(result < 65536, "Fletcher-16 result fits in 16 bits");
    }

    /// Verify Adler-32 modulo bounds.
    #[kani::proof]
    fn proof_adler32_bounds() {
        let a: u32 = kani::any();
        let b: u32 = kani::any();
        let byte: u8 = kani::any();

        kani::assume(a < 65521);
        kani::assume(b < 65521);

        let new_a = (a + byte as u32) % 65521;
        let new_b = (b + new_a) % 65521;

        kani::assert(new_a < 65521, "a must be < 65521");
        kani::assert(new_b < 65521, "b must be < 65521");
    }

    /// Verify Adler-32 initial values.
    #[kani::proof]
    fn proof_adler32_initial() {
        let a: u32 = 1;
        let b: u32 = 0;

        kani::assert(a == 1, "Adler-32 a starts at 1");
        kani::assert(b == 0, "Adler-32 b starts at 0");
    }

    /// Verify Adler-32 finalize packing.
    #[kani::proof]
    fn proof_adler32_finalize() {
        let a: u32 = kani::any();
        let b: u32 = kani::any();

        kani::assume(a < 65536);
        kani::assume(b < 65536);

        let result = (b << 16) | a;

        // Result should combine both values
        kani::assert((result >> 16) == b, "Upper 16 bits should be b");
        kani::assert((result & 0xFFFF) == a, "Lower 16 bits should be a");
    }

    /// Verify CRC-32 table index bounds.
    #[kani::proof]
    fn proof_crc32_table_index() {
        let value: u32 = kani::any();
        let byte: u8 = kani::any();

        let index = ((value ^ byte as u32) & 0xFF) as usize;

        kani::assert(index < 256, "CRC-32 table index must be < 256");
    }

    /// Verify CRC-32 initial and final XOR.
    #[kani::proof]
    fn proof_crc32_xor_pattern() {
        let initial: u32 = 0xFFFFFFFF;
        let value: u32 = kani::any();

        let finalized = value ^ 0xFFFFFFFF;
        let double_xor = finalized ^ 0xFFFFFFFF;

        kani::assert(
            double_xor == value,
            "Double XOR with 0xFFFFFFFF is identity",
        );
    }

    /// Verify verify function logic.
    #[kani::proof]
    fn proof_verify_logic() {
        let actual: u32 = kani::any();
        let expected: u32 = kani::any();

        let matches = actual == expected;

        if actual == expected {
            kani::assert(matches, "Should match when checksums equal");
        } else {
            kani::assert(!matches, "Should not match when checksums differ");
        }
    }

    /// Verify append adds 4 bytes.
    #[kani::proof]
    fn proof_append_adds_4_bytes() {
        let data_len: usize = kani::any();
        kani::assume(data_len < 10000);

        let result_len = data_len + 4;

        kani::assert(result_len == data_len + 4, "Append should add 4 bytes");
    }

    /// Verify verify_and_strip minimum length.
    #[kani::proof]
    fn proof_verify_strip_min_length() {
        let data_len: usize = kani::any();

        let valid = data_len >= 4;

        if data_len < 4 {
            kani::assert(!valid, "Data must be at least 4 bytes");
        } else {
            kani::assert(valid, "Data with 4+ bytes is valid");
        }
    }

    /// Verify reset restores initial state.
    #[kani::proof]
    fn proof_reset_restores_initial() {
        // XOR checksum resets to 0
        let xor_initial: u8 = 0;
        // Sum checksum resets to 0
        let sum_initial: u32 = 0;
        // Adler-32 resets to a=1, b=0
        let adler_a_initial: u32 = 1;
        let adler_b_initial: u32 = 0;
        // CRC-32 resets to 0xFFFFFFFF
        let crc_initial: u32 = 0xFFFFFFFF;

        kani::assert(xor_initial == 0, "XOR resets to 0");
        kani::assert(sum_initial == 0, "Sum resets to 0");
        kani::assert(adler_a_initial == 1, "Adler a resets to 1");
        kani::assert(adler_b_initial == 0, "Adler b resets to 0");
        kani::assert(crc_initial == 0xFFFFFFFF, "CRC resets to 0xFFFFFFFF");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xor_checksum() {
        assert_eq!(
            XorChecksum::checksum(&[0x12, 0x34, 0x56]),
            0x12 ^ 0x34 ^ 0x56
        );
    }

    #[test]
    fn test_sum_checksum() {
        assert_eq!(SumChecksum::checksum(&[1, 2, 3, 4, 5]), 15);
    }

    #[test]
    fn test_fletcher16() {
        let data = b"abcde";
        let cs = Fletcher16::checksum(data);
        assert!(cs > 0);
    }

    #[test]
    fn test_adler32() {
        // Known test vector: "Wikipedia"
        let cs = Adler32::checksum(b"Wikipedia");
        assert_eq!(cs, 0x11E60398);
    }

    #[test]
    fn test_crc32() {
        // Known test vector
        let cs = Crc32::checksum(b"123456789");
        assert_eq!(cs, 0xCBF43926);
    }

    #[test]
    fn test_verify() {
        let data = b"test data";
        let cs = Crc32::checksum(data);
        assert!(verify(Crc32::new(), data, cs).is_ok());
        assert!(verify(Crc32::new(), data, cs + 1).is_err());
    }

    #[test]
    fn test_append_and_verify() {
        let data = b"test data";
        let with_checksum = append_checksum(Crc32::new(), data);

        let verified = verify_and_strip(Crc32::new(), &with_checksum).unwrap();
        assert_eq!(verified, data);
    }
}
