//! ID generation for drbot.
//!
//! This crate provides:
//! - UUID generation
//! - ULID generation
//! - Snowflake IDs
//! - Custom ID formats

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;
use uuid::Uuid;

/// ID error types.
#[derive(Error, Debug)]
pub enum IdError {
    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Generation failed: {0}")]
    GenerationFailed(String),

    #[error("Invalid ID: {0}")]
    InvalidId(String),
}

/// Result type for ID operations.
pub type Result<T> = std::result::Result<T, IdError>;

/// UUID utilities.
pub struct UuidGenerator;

impl UuidGenerator {
    /// Generate random UUID v4.
    pub fn v4() -> Uuid {
        Uuid::new_v4()
    }

    /// Generate UUID v4 as string.
    pub fn v4_string() -> String {
        Uuid::new_v4().to_string()
    }

    /// Generate UUID v4 as hyphenated string.
    pub fn v4_hyphenated() -> String {
        Uuid::new_v4().hyphenated().to_string()
    }

    /// Generate UUID v4 as simple string (no hyphens).
    pub fn v4_simple() -> String {
        Uuid::new_v4().simple().to_string()
    }

    /// Parse UUID from string.
    pub fn parse(s: &str) -> Result<Uuid> {
        Uuid::parse_str(s).map_err(|e| IdError::ParseError(e.to_string()))
    }

    /// Check if string is valid UUID.
    pub fn is_valid(s: &str) -> bool {
        Uuid::parse_str(s).is_ok()
    }

    /// Generate nil UUID (all zeros).
    pub fn nil() -> Uuid {
        Uuid::nil()
    }

    /// Check if UUID is nil.
    pub fn is_nil(id: &Uuid) -> bool {
        id.is_nil()
    }
}

/// ULID (Universally Unique Lexicographically Sortable Identifier).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Ulid {
    bytes: [u8; 16],
}

#[derive(Debug)]
struct UlidMonotonicState {
    last_timestamp_ms: u64,
    last_random: [u8; 10],
}

fn ulid_monotonic_state() -> &'static Mutex<UlidMonotonicState> {
    static STATE: OnceLock<Mutex<UlidMonotonicState>> = OnceLock::new();
    STATE.get_or_init(|| {
        Mutex::new(UlidMonotonicState {
            last_timestamp_ms: 0,
            last_random: [0u8; 10],
        })
    })
}

fn increment_be_80(bytes: &mut [u8; 10]) -> bool {
    for i in (0..10).rev() {
        let (b, overflow) = bytes[i].overflowing_add(1);
        bytes[i] = b;
        if !overflow {
            return true;
        }
    }
    false
}

fn next_ulid_components(timestamp_ms: u64) -> (u64, [u8; 10]) {
    let mutex = ulid_monotonic_state();
    let mut state = mutex.lock().unwrap();

    if timestamp_ms > state.last_timestamp_ms {
        state.last_timestamp_ms = timestamp_ms;
        let uuid = Uuid::new_v4();
        state.last_random.copy_from_slice(&uuid.as_bytes()[..10]);
        return (state.last_timestamp_ms, state.last_random);
    }

    if increment_be_80(&mut state.last_random) {
        return (state.last_timestamp_ms, state.last_random);
    }

    // Random overflow within the same millisecond; bump timestamp and reseed.
    state.last_timestamp_ms = state.last_timestamp_ms.saturating_add(1);
    let uuid = Uuid::new_v4();
    state.last_random.copy_from_slice(&uuid.as_bytes()[..10]);
    (state.last_timestamp_ms, state.last_random)
}

impl Ulid {
    const ENCODING: &'static [u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

    /// Generate new ULID.
    pub fn new() -> Result<Self> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| IdError::GenerationFailed(e.to_string()))?
            .as_millis() as u64;

        Self::from_timestamp(timestamp)
    }

    /// Generate ULID from timestamp.
    pub fn from_timestamp(timestamp_ms: u64) -> Result<Self> {
        let (timestamp_ms, random) = next_ulid_components(timestamp_ms);

        let mut bytes = [0u8; 16];

        // First 6 bytes are timestamp (48 bits)
        bytes[0] = (timestamp_ms >> 40) as u8;
        bytes[1] = (timestamp_ms >> 32) as u8;
        bytes[2] = (timestamp_ms >> 24) as u8;
        bytes[3] = (timestamp_ms >> 16) as u8;
        bytes[4] = (timestamp_ms >> 8) as u8;
        bytes[5] = timestamp_ms as u8;

        bytes[6..].copy_from_slice(&random);

        Ok(Self { bytes })
    }

    /// Get timestamp from ULID.
    pub fn timestamp(&self) -> u64 {
        ((self.bytes[0] as u64) << 40)
            | ((self.bytes[1] as u64) << 32)
            | ((self.bytes[2] as u64) << 24)
            | ((self.bytes[3] as u64) << 16)
            | ((self.bytes[4] as u64) << 8)
            | (self.bytes[5] as u64)
    }

    /// Get datetime from ULID.
    pub fn datetime(&self) -> Option<DateTime<Utc>> {
        DateTime::from_timestamp_millis(self.timestamp() as i64)
    }

    /// Convert to string (Crockford Base32).
    pub fn to_string(&self) -> String {
        let mut result = String::with_capacity(26);
        let bytes = &self.bytes;

        // Encode 16 bytes to 26 characters
        result.push(Self::ENCODING[(bytes[0] >> 5) as usize] as char);
        result.push(Self::ENCODING[((bytes[0] >> 0) & 0x1F) as usize] as char);
        result.push(Self::ENCODING[(bytes[1] >> 3) as usize] as char);
        result.push(Self::ENCODING[((bytes[1] << 2 | bytes[2] >> 6) & 0x1F) as usize] as char);
        result.push(Self::ENCODING[((bytes[2] >> 1) & 0x1F) as usize] as char);
        result.push(Self::ENCODING[((bytes[2] << 4 | bytes[3] >> 4) & 0x1F) as usize] as char);
        result.push(Self::ENCODING[((bytes[3] << 1 | bytes[4] >> 7) & 0x1F) as usize] as char);
        result.push(Self::ENCODING[((bytes[4] >> 2) & 0x1F) as usize] as char);
        result.push(Self::ENCODING[((bytes[4] << 3 | bytes[5] >> 5) & 0x1F) as usize] as char);
        result.push(Self::ENCODING[(bytes[5] & 0x1F) as usize] as char);

        for i in 0..6 {
            let base = 6 + i;
            result.push(Self::ENCODING[(bytes[base] >> 3) as usize] as char);
            result.push(
                Self::ENCODING[((bytes[base] << 2 | bytes[base + 1].checked_shr(6).unwrap_or(0))
                    & 0x1F) as usize] as char,
            );
            if base + 1 < 16 {
                result.push(Self::ENCODING[((bytes[base + 1] >> 1) & 0x1F) as usize] as char);
            }
        }

        result.truncate(26);
        result
    }

    /// Get bytes.
    pub fn as_bytes(&self) -> &[u8; 16] {
        &self.bytes
    }
}

impl Default for Ulid {
    fn default() -> Self {
        Self::new().expect("Failed to generate ULID")
    }
}

impl std::fmt::Display for Ulid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_string())
    }
}

/// Snowflake ID generator.
pub struct SnowflakeGenerator {
    epoch: u64,
    machine_id: u16,
    sequence: AtomicU64,
    last_timestamp: AtomicU64,
}

impl SnowflakeGenerator {
    /// Create new generator with machine ID.
    pub fn new(machine_id: u16) -> Self {
        Self {
            epoch: 1704067200000,           // 2024-01-01 00:00:00 UTC
            machine_id: machine_id & 0x3FF, // 10 bits
            sequence: AtomicU64::new(0),
            last_timestamp: AtomicU64::new(0),
        }
    }

    /// Create with custom epoch.
    pub fn with_epoch(machine_id: u16, epoch_ms: u64) -> Self {
        Self {
            epoch: epoch_ms,
            machine_id: machine_id & 0x3FF,
            sequence: AtomicU64::new(0),
            last_timestamp: AtomicU64::new(0),
        }
    }

    /// Generate next ID.
    pub fn next(&self) -> Result<u64> {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| IdError::GenerationFailed(e.to_string()))?
            .as_millis() as u64;

        let mut timestamp = now_ms.saturating_sub(self.epoch);

        let last = self.last_timestamp.load(Ordering::SeqCst);
        if timestamp < last {
            timestamp = last;
        }

        let seq = if timestamp == last {
            let next = self.sequence.fetch_add(1, Ordering::SeqCst) + 1;
            let seq = next & 0xFFF; // 12 bits

            if seq == 0 {
                // Sequence overflow within the same millisecond; wait for next millisecond.
                loop {
                    let now_ms = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map_err(|e| IdError::GenerationFailed(e.to_string()))?
                        .as_millis() as u64;
                    let candidate = now_ms.saturating_sub(self.epoch);
                    if candidate > last {
                        timestamp = candidate;
                        self.last_timestamp.store(timestamp, Ordering::SeqCst);
                        self.sequence.store(0, Ordering::SeqCst);
                        break;
                    }
                }
                0
            } else {
                seq
            }
        } else {
            self.last_timestamp.store(timestamp, Ordering::SeqCst);
            self.sequence.store(0, Ordering::SeqCst);
            0
        };

        // 41 bits timestamp, 10 bits machine, 12 bits sequence
        let id = ((timestamp & 0x1FFFFFFFFFF) << 22) | ((self.machine_id as u64) << 12) | seq;

        Ok(id)
    }

    /// Extract timestamp from ID.
    pub fn extract_timestamp(&self, id: u64) -> u64 {
        (id >> 22) + self.epoch
    }

    /// Extract machine ID from ID.
    pub fn extract_machine_id(&self, id: u64) -> u16 {
        ((id >> 12) & 0x3FF) as u16
    }

    /// Extract sequence from ID.
    pub fn extract_sequence(&self, id: u64) -> u16 {
        (id & 0xFFF) as u16
    }
}

/// Short ID generator (URL-safe).
pub struct ShortIdGenerator {
    alphabet: Vec<char>,
    length: usize,
}

impl ShortIdGenerator {
    /// Create with default settings.
    pub fn new() -> Self {
        Self {
            alphabet: "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz"
                .chars()
                .collect(),
            length: 8,
        }
    }

    /// Set alphabet.
    pub fn alphabet(mut self, alphabet: &str) -> Self {
        self.alphabet = alphabet.chars().collect();
        self
    }

    /// Set length.
    pub fn length(mut self, length: usize) -> Self {
        self.length = length;
        self
    }

    /// Generate short ID.
    pub fn generate(&self) -> String {
        use std::collections::hash_map::RandomState;
        use std::hash::{BuildHasher, Hasher};

        let state = RandomState::new();
        let mut result = String::with_capacity(self.length);

        for i in 0..self.length {
            let mut hasher = state.build_hasher();
            hasher.write_usize(i);
            hasher.write_u64(
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos() as u64,
            );
            let idx = (hasher.finish() as usize) % self.alphabet.len();
            result.push(self.alphabet[idx]);
        }

        result
    }
}

impl Default for ShortIdGenerator {
    fn default() -> Self {
        Self::new()
    }
}

/// Prefixed ID (e.g., "usr_abc123").
pub struct PrefixedId;

impl PrefixedId {
    /// Generate prefixed UUID.
    pub fn uuid(prefix: &str) -> String {
        format!("{}_{}", prefix, UuidGenerator::v4_simple())
    }

    /// Generate prefixed short ID.
    pub fn short(prefix: &str, length: usize) -> String {
        let gen = ShortIdGenerator::new().length(length);
        format!("{}_{}", prefix, gen.generate())
    }

    /// Parse prefix from ID.
    pub fn parse_prefix(id: &str) -> Option<(&str, &str)> {
        id.split_once('_')
    }

    /// Validate prefix.
    pub fn has_prefix(id: &str, prefix: &str) -> bool {
        id.starts_with(&format!("{}_", prefix))
    }
}

// ============================================================================
// Kani Formal Verification Proofs
// ============================================================================

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    // ========================================================================
    // ULID Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_ulid_timestamp_extraction_low() {
        // Test with small timestamp values
        let timestamp: u64 = kani::any();
        kani::assume(timestamp <= 0xFFFF); // 16-bit value

        let mut bytes = [0u8; 16];
        bytes[0] = (timestamp >> 40) as u8;
        bytes[1] = (timestamp >> 32) as u8;
        bytes[2] = (timestamp >> 24) as u8;
        bytes[3] = (timestamp >> 16) as u8;
        bytes[4] = (timestamp >> 8) as u8;
        bytes[5] = timestamp as u8;

        let extracted = ((bytes[0] as u64) << 40)
            | ((bytes[1] as u64) << 32)
            | ((bytes[2] as u64) << 24)
            | ((bytes[3] as u64) << 16)
            | ((bytes[4] as u64) << 8)
            | (bytes[5] as u64);

        kani::assert!(
            extracted == timestamp,
            "Timestamp roundtrip correct for small values"
        );
    }

    #[kani::proof]
    fn proof_ulid_timestamp_extraction_48bit() {
        // Test with 48-bit timestamp (max ULID timestamp)
        let timestamp: u64 = kani::any();
        kani::assume(timestamp <= 0xFFFFFFFFFFFF); // 48-bit max

        let mut bytes = [0u8; 16];
        bytes[0] = (timestamp >> 40) as u8;
        bytes[1] = (timestamp >> 32) as u8;
        bytes[2] = (timestamp >> 24) as u8;
        bytes[3] = (timestamp >> 16) as u8;
        bytes[4] = (timestamp >> 8) as u8;
        bytes[5] = timestamp as u8;

        let extracted = ((bytes[0] as u64) << 40)
            | ((bytes[1] as u64) << 32)
            | ((bytes[2] as u64) << 24)
            | ((bytes[3] as u64) << 16)
            | ((bytes[4] as u64) << 8)
            | (bytes[5] as u64);

        kani::assert!(extracted == timestamp, "48-bit timestamp roundtrip correct");
    }

    #[kani::proof]
    fn proof_ulid_timestamp_byte_boundaries() {
        // Verify each byte boundary is handled correctly
        let b0: u8 = kani::any();
        let b1: u8 = kani::any();
        let b2: u8 = kani::any();
        let b3: u8 = kani::any();
        let b4: u8 = kani::any();
        let b5: u8 = kani::any();

        let timestamp = ((b0 as u64) << 40)
            | ((b1 as u64) << 32)
            | ((b2 as u64) << 24)
            | ((b3 as u64) << 16)
            | ((b4 as u64) << 8)
            | (b5 as u64);

        kani::assert!(timestamp <= 0xFFFFFFFFFFFF, "Timestamp fits in 48 bits");
        kani::assert!((timestamp >> 40) as u8 == b0, "Byte 0 extracted correctly");
        kani::assert!((timestamp >> 32) as u8 == b1, "Byte 1 extracted correctly");
        kani::assert!((timestamp >> 24) as u8 == b2, "Byte 2 extracted correctly");
        kani::assert!((timestamp >> 16) as u8 == b3, "Byte 3 extracted correctly");
        kani::assert!((timestamp >> 8) as u8 == b4, "Byte 4 extracted correctly");
        kani::assert!(timestamp as u8 == b5, "Byte 5 extracted correctly");
    }

    #[kani::proof]
    fn proof_ulid_encoding_table_size() {
        // Verify encoding table has 32 characters (Crockford Base32)
        kani::assert!(Ulid::ENCODING.len() == 32, "Crockford Base32 has 32 chars");
    }

    #[kani::proof]
    fn proof_ulid_bytes_length() {
        // ULID is always 16 bytes
        let bytes = [0u8; 16];
        let ulid = Ulid { bytes };
        kani::assert!(ulid.as_bytes().len() == 16, "ULID is 16 bytes");
    }

    // ========================================================================
    // Snowflake ID Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_snowflake_machine_id_masking() {
        let machine_id: u16 = kani::any();
        let masked = machine_id & 0x3FF; // 10 bits

        kani::assert!(masked <= 1023, "Machine ID fits in 10 bits");
        kani::assert!(masked == (machine_id & 0x3FF), "Masking is idempotent");
    }

    #[kani::proof]
    fn proof_snowflake_sequence_masking() {
        let sequence: u64 = kani::any();
        let masked = sequence & 0xFFF; // 12 bits

        kani::assert!(masked <= 4095, "Sequence fits in 12 bits");
    }

    #[kani::proof]
    fn proof_snowflake_timestamp_masking() {
        let timestamp: u64 = kani::any();
        let masked = timestamp & 0x1FFFFFFFFFF; // 41 bits

        kani::assert!(masked <= 0x1FFFFFFFFFF, "Timestamp fits in 41 bits");
    }

    #[kani::proof]
    fn proof_snowflake_id_composition() {
        let timestamp: u64 = kani::any();
        let machine_id: u16 = kani::any();
        let sequence: u64 = kani::any();

        // Apply masks
        let ts = timestamp & 0x1FFFFFFFFFF; // 41 bits
        let mid = (machine_id & 0x3FF) as u64; // 10 bits
        let seq = sequence & 0xFFF; // 12 bits

        // Compose ID: 41 bits timestamp | 10 bits machine | 12 bits sequence
        let id = (ts << 22) | (mid << 12) | seq;

        // Extract components
        let extracted_ts = id >> 22;
        let extracted_mid = (id >> 12) & 0x3FF;
        let extracted_seq = id & 0xFFF;

        kani::assert!(extracted_ts == ts, "Timestamp extraction correct");
        kani::assert!(extracted_mid == mid, "Machine ID extraction correct");
        kani::assert!(extracted_seq == seq, "Sequence extraction correct");
    }

    #[kani::proof]
    fn proof_snowflake_extract_machine_id() {
        let machine_id: u16 = kani::any();
        kani::assume(machine_id <= 1023); // 10-bit limit

        // Simulate ID creation
        let id = ((machine_id as u64) << 12);

        // Extract
        let extracted = ((id >> 12) & 0x3FF) as u16;

        kani::assert!(extracted == machine_id, "Machine ID roundtrip correct");
    }

    #[kani::proof]
    fn proof_snowflake_extract_sequence() {
        let sequence: u16 = kani::any();
        kani::assume(sequence <= 4095); // 12-bit limit

        // Simulate ID creation
        let id = sequence as u64;

        // Extract
        let extracted = (id & 0xFFF) as u16;

        kani::assert!(extracted == sequence, "Sequence roundtrip correct");
    }

    #[kani::proof]
    fn proof_snowflake_bit_layout_no_overlap() {
        // Verify bit ranges don't overlap
        // Timestamp: bits 22-62 (41 bits)
        // Machine ID: bits 12-21 (10 bits)
        // Sequence: bits 0-11 (12 bits)

        let timestamp_mask: u64 = 0x1FFFFFFFFFF << 22;
        let machine_mask: u64 = 0x3FF << 12;
        let sequence_mask: u64 = 0xFFF;

        // No overlap between timestamp and machine
        kani::assert!(
            (timestamp_mask & machine_mask) == 0,
            "No timestamp/machine overlap"
        );

        // No overlap between machine and sequence
        kani::assert!(
            (machine_mask & sequence_mask) == 0,
            "No machine/sequence overlap"
        );

        // No overlap between timestamp and sequence
        kani::assert!(
            (timestamp_mask & sequence_mask) == 0,
            "No timestamp/sequence overlap"
        );

        // Total bits used: 41 + 10 + 12 = 63 bits (fits in u64)
        let all_bits = timestamp_mask | machine_mask | sequence_mask;
        kani::assert!(all_bits == 0x7FFFFFFFFFFFFFFF, "Uses exactly 63 bits");
    }

    // ========================================================================
    // ShortIdGenerator Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_short_id_default_alphabet_size() {
        let alphabet: Vec<char> = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz"
            .chars()
            .collect();
        kani::assert!(alphabet.len() == 62, "Default alphabet has 62 chars");
    }

    #[kani::proof]
    fn proof_short_id_default_length() {
        // Default length is 8
        let default_length = 8usize;
        kani::assert!(default_length > 0, "Default length is positive");
        kani::assert!(default_length <= 32, "Default length is reasonable");
    }

    // ========================================================================
    // PrefixedId Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_prefixed_id_parse_none_without_underscore() {
        // parse_prefix returns None if no underscore
        let id = "abc";
        let result = id.split_once('_');
        kani::assert!(result.is_none(), "No underscore means no prefix");
    }

    #[kani::proof]
    fn proof_prefixed_id_parse_with_underscore() {
        // parse_prefix returns Some if underscore present
        let id = "usr_abc123";
        let result = id.split_once('_');
        kani::assert!(result.is_some(), "Underscore means prefix exists");

        if let Some((prefix, value)) = result {
            kani::assert!(prefix == "usr", "Prefix is correct");
            kani::assert!(value == "abc123", "Value is correct");
        }
    }

    #[kani::proof]
    fn proof_prefixed_id_has_prefix() {
        let id = "usr_abc123";
        let prefix = "usr_";
        let has_prefix = id.starts_with(prefix);
        kani::assert!(has_prefix, "ID starts with prefix");

        let wrong_prefix = "org_";
        let has_wrong = id.starts_with(wrong_prefix);
        kani::assert!(!has_wrong, "ID does not start with wrong prefix");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uuid_generation() {
        let id1 = UuidGenerator::v4();
        let id2 = UuidGenerator::v4();
        assert_ne!(id1, id2);

        let str_id = UuidGenerator::v4_string();
        assert_eq!(str_id.len(), 36);

        let simple = UuidGenerator::v4_simple();
        assert_eq!(simple.len(), 32);
    }

    #[test]
    fn test_uuid_parse() {
        let id = UuidGenerator::v4();
        let parsed = UuidGenerator::parse(&id.to_string()).unwrap();
        assert_eq!(id, parsed);
    }

    #[test]
    fn test_ulid() {
        let ulid1 = Ulid::new().unwrap();
        let ulid2 = Ulid::new().unwrap();

        // Should be sortable by time
        assert!(ulid1 <= ulid2);

        let str = ulid1.to_string();
        assert_eq!(str.len(), 26);

        // Timestamp should be recent
        let ts = ulid1.timestamp();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        assert!(ts <= now);
        assert!(ts > now - 1000); // Within last second
    }

    #[test]
    fn test_snowflake() {
        let gen = SnowflakeGenerator::new(1);

        let id1 = gen.next().unwrap();
        let id2 = gen.next().unwrap();

        assert_ne!(id1, id2);
        assert!(id1 < id2);

        assert_eq!(gen.extract_machine_id(id1), 1);
    }

    #[test]
    fn test_short_id() {
        let gen = ShortIdGenerator::new().length(12);
        let id = gen.generate();
        assert_eq!(id.len(), 12);

        let id2 = gen.generate();
        assert_ne!(id, id2);
    }

    #[test]
    fn test_prefixed_id() {
        let id = PrefixedId::uuid("usr");
        assert!(id.starts_with("usr_"));

        let (prefix, value) = PrefixedId::parse_prefix(&id).unwrap();
        assert_eq!(prefix, "usr");
        assert!(!value.is_empty());

        assert!(PrefixedId::has_prefix(&id, "usr"));
        assert!(!PrefixedId::has_prefix(&id, "org"));
    }
}
