//! Hex encoding/decoding for drbot.
//!
//! This crate provides:
//! - Hex encoding
//! - Hex decoding
//! - Case options

use thiserror::Error;

/// Hex error types.
#[derive(Error, Debug, Clone)]
pub enum HexError {
    #[error("Invalid hex character: {0}")]
    InvalidCharacter(char),

    #[error("Invalid length: odd number of characters")]
    InvalidLength,
}

/// Result type for hex operations.
pub type Result<T> = std::result::Result<T, HexError>;

const HEX_LOWER: &[u8; 16] = b"0123456789abcdef";
const HEX_UPPER: &[u8; 16] = b"0123456789ABCDEF";

/// Hex case option.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HexCase {
    Lower,
    Upper,
}

impl Default for HexCase {
    fn default() -> Self {
        Self::Lower
    }
}

/// Encode bytes to hex string (lowercase).
pub fn encode(data: &[u8]) -> String {
    encode_with_case(data, HexCase::Lower)
}

/// Encode bytes to hex string (uppercase).
pub fn encode_upper(data: &[u8]) -> String {
    encode_with_case(data, HexCase::Upper)
}

/// Encode with case option.
pub fn encode_with_case(data: &[u8], case: HexCase) -> String {
    let alphabet = match case {
        HexCase::Lower => HEX_LOWER,
        HexCase::Upper => HEX_UPPER,
    };

    let mut result = String::with_capacity(data.len() * 2);
    for byte in data {
        result.push(alphabet[(byte >> 4) as usize] as char);
        result.push(alphabet[(byte & 0x0F) as usize] as char);
    }
    result
}

/// Decode hex string to bytes.
pub fn decode(hex: &str) -> Result<Vec<u8>> {
    if hex.len() % 2 != 0 {
        return Err(HexError::InvalidLength);
    }

    let mut result = Vec::with_capacity(hex.len() / 2);
    let mut chars = hex.chars();

    while let (Some(c1), Some(c2)) = (chars.next(), chars.next()) {
        let high = hex_digit(c1)?;
        let low = hex_digit(c2)?;
        result.push((high << 4) | low);
    }

    Ok(result)
}

fn hex_digit(c: char) -> Result<u8> {
    match c {
        '0'..='9' => Ok(c as u8 - b'0'),
        'a'..='f' => Ok(c as u8 - b'a' + 10),
        'A'..='F' => Ok(c as u8 - b'A' + 10),
        _ => Err(HexError::InvalidCharacter(c)),
    }
}

/// Encode single byte to hex.
pub fn encode_byte(byte: u8) -> [char; 2] {
    [
        HEX_LOWER[(byte >> 4) as usize] as char,
        HEX_LOWER[(byte & 0x0F) as usize] as char,
    ]
}

/// Decode hex pair to byte.
pub fn decode_byte(high: char, low: char) -> Result<u8> {
    let h = hex_digit(high)?;
    let l = hex_digit(low)?;
    Ok((h << 4) | l)
}

/// Hex encoder/decoder struct.
pub struct Hex {
    case: HexCase,
}

impl Hex {
    /// Create with case option.
    pub fn new(case: HexCase) -> Self {
        Self { case }
    }

    /// Lowercase hex.
    pub fn lower() -> Self {
        Self::new(HexCase::Lower)
    }

    /// Uppercase hex.
    pub fn upper() -> Self {
        Self::new(HexCase::Upper)
    }

    /// Encode bytes.
    pub fn encode(&self, data: &[u8]) -> String {
        encode_with_case(data, self.case)
    }

    /// Decode string.
    pub fn decode(&self, hex: &str) -> Result<Vec<u8>> {
        decode(hex)
    }
}

impl Default for Hex {
    fn default() -> Self {
        Self::lower()
    }
}

/// Format bytes as hex dump.
pub fn hex_dump(data: &[u8], bytes_per_line: usize) -> String {
    let mut result = String::new();
    let bytes_per_line = bytes_per_line.max(1);

    for (offset, chunk) in data.chunks(bytes_per_line).enumerate() {
        // Offset
        result.push_str(&format!("{:08x}  ", offset * bytes_per_line));

        // Hex bytes
        for byte in chunk {
            result.push_str(&format!("{:02x} ", byte));
        }

        // Padding
        for _ in chunk.len()..bytes_per_line {
            result.push_str("   ");
        }

        result.push(' ');

        // ASCII
        for byte in chunk {
            let c = if byte.is_ascii_graphic() || *byte == b' ' {
                *byte as char
            } else {
                '.'
            };
            result.push(c);
        }

        result.push('\n');
    }

    result
}

/// Check if string is valid hex.
pub fn is_valid_hex(s: &str) -> bool {
    s.len() % 2 == 0 && s.chars().all(|c| c.is_ascii_hexdigit())
}

/// Format number as hex with prefix.
pub fn to_hex_string<T: std::fmt::LowerHex>(value: T) -> String {
    format!("0x{:x}", value)
}

/// Parse hex string with optional 0x prefix.
pub fn parse_hex_u64(s: &str) -> Result<u64> {
    let s = s.trim_start_matches("0x").trim_start_matches("0X");
    u64::from_str_radix(s, 16).map_err(|_| HexError::InvalidCharacter('?'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_decode() {
        let original = b"Hello, World!";
        let encoded = encode(original);
        let decoded = decode(&encoded).unwrap();
        assert_eq!(original.to_vec(), decoded);
    }

    #[test]
    fn test_encode_lower() {
        assert_eq!(encode(&[0xDE, 0xAD, 0xBE, 0xEF]), "deadbeef");
    }

    #[test]
    fn test_encode_upper() {
        assert_eq!(encode_upper(&[0xDE, 0xAD, 0xBE, 0xEF]), "DEADBEEF");
    }

    #[test]
    fn test_decode_case_insensitive() {
        assert_eq!(decode("DeAdBeEf").unwrap(), vec![0xDE, 0xAD, 0xBE, 0xEF]);
    }

    #[test]
    fn test_invalid_length() {
        assert!(matches!(decode("abc"), Err(HexError::InvalidLength)));
    }

    #[test]
    fn test_invalid_character() {
        assert!(matches!(decode("ghij"), Err(HexError::InvalidCharacter(_))));
    }

    #[test]
    fn test_is_valid_hex() {
        assert!(is_valid_hex("deadbeef"));
        assert!(is_valid_hex("DEADBEEF"));
        assert!(is_valid_hex("1234567890abcdef"));
        assert!(!is_valid_hex("ghij"));
        assert!(!is_valid_hex("abc")); // Odd length
    }

    #[test]
    fn test_parse_hex_u64() {
        assert_eq!(parse_hex_u64("ff").unwrap(), 255);
        assert_eq!(parse_hex_u64("0xff").unwrap(), 255);
        assert_eq!(parse_hex_u64("0xFF").unwrap(), 255);
    }

    #[test]
    fn test_hex_dump() {
        let data = b"Hello, World!";
        let dump = hex_dump(data, 16);
        assert!(dump.contains("48 65 6c 6c 6f")); // "Hello"
    }
}
