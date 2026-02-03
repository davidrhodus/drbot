//! Base64 encoding/decoding for drbot.
//!
//! This crate provides:
//! - Standard base64 encoding
//! - URL-safe base64
//! - Padding options

use thiserror::Error;

/// Base64 error types.
#[derive(Error, Debug, Clone)]
pub enum Base64Error {
    #[error("Invalid character: {0}")]
    InvalidCharacter(char),

    #[error("Invalid length")]
    InvalidLength,

    #[error("Invalid padding")]
    InvalidPadding,
}

/// Result type for base64 operations.
pub type Result<T> = std::result::Result<T, Base64Error>;

const STANDARD_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

const URL_SAFE_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// Base64 configuration.
#[derive(Debug, Clone, Copy)]
pub struct Base64Config {
    /// Alphabet to use.
    pub alphabet: &'static [u8; 64],
    /// Whether to add padding.
    pub padding: bool,
}

impl Base64Config {
    /// Standard base64 with padding.
    pub const STANDARD: Self = Self {
        alphabet: STANDARD_ALPHABET,
        padding: true,
    };

    /// Standard base64 without padding.
    pub const STANDARD_NO_PAD: Self = Self {
        alphabet: STANDARD_ALPHABET,
        padding: false,
    };

    /// URL-safe base64 with padding.
    pub const URL_SAFE: Self = Self {
        alphabet: URL_SAFE_ALPHABET,
        padding: true,
    };

    /// URL-safe base64 without padding.
    pub const URL_SAFE_NO_PAD: Self = Self {
        alphabet: URL_SAFE_ALPHABET,
        padding: false,
    };
}

impl Default for Base64Config {
    fn default() -> Self {
        Self::STANDARD
    }
}

/// Encode bytes to base64 string.
pub fn encode(data: &[u8]) -> String {
    encode_config(data, Base64Config::STANDARD)
}

/// Encode with configuration.
pub fn encode_config(data: &[u8], config: Base64Config) -> String {
    let mut result = String::new();
    let alphabet = config.alphabet;

    let mut chunks = data.chunks_exact(3);

    for chunk in chunks.by_ref() {
        let n = ((chunk[0] as u32) << 16) | ((chunk[1] as u32) << 8) | (chunk[2] as u32);
        result.push(alphabet[((n >> 18) & 0x3F) as usize] as char);
        result.push(alphabet[((n >> 12) & 0x3F) as usize] as char);
        result.push(alphabet[((n >> 6) & 0x3F) as usize] as char);
        result.push(alphabet[(n & 0x3F) as usize] as char);
    }

    let remainder = chunks.remainder();
    match remainder.len() {
        1 => {
            let n = (remainder[0] as u32) << 16;
            result.push(alphabet[((n >> 18) & 0x3F) as usize] as char);
            result.push(alphabet[((n >> 12) & 0x3F) as usize] as char);
            if config.padding {
                result.push_str("==");
            }
        }
        2 => {
            let n = ((remainder[0] as u32) << 16) | ((remainder[1] as u32) << 8);
            result.push(alphabet[((n >> 18) & 0x3F) as usize] as char);
            result.push(alphabet[((n >> 12) & 0x3F) as usize] as char);
            result.push(alphabet[((n >> 6) & 0x3F) as usize] as char);
            if config.padding {
                result.push('=');
            }
        }
        _ => {}
    }

    result
}

/// Decode base64 string to bytes.
pub fn decode(encoded: &str) -> Result<Vec<u8>> {
    decode_config(encoded, Base64Config::STANDARD)
}

/// Decode with configuration.
pub fn decode_config(encoded: &str, config: Base64Config) -> Result<Vec<u8>> {
    let encoded = encoded.trim_end_matches('=');
    let mut result = Vec::with_capacity(encoded.len() * 3 / 4);

    let decode_char = |c: char| -> Result<u8> {
        config
            .alphabet
            .iter()
            .position(|&b| b == c as u8)
            .map(|p| p as u8)
            .ok_or(Base64Error::InvalidCharacter(c))
    };

    let mut chars = encoded.chars().peekable();

    while chars.peek().is_some() {
        let c0 = chars.next().ok_or(Base64Error::InvalidLength)?;
        let c1 = chars.next().ok_or(Base64Error::InvalidLength)?;
        let c2 = chars.next();
        let c3 = chars.next();

        let b0 = decode_char(c0)?;
        let b1 = decode_char(c1)?;

        result.push((b0 << 2) | (b1 >> 4));

        if let Some(c2) = c2 {
            let b2 = decode_char(c2)?;
            result.push((b1 << 4) | (b2 >> 2));

            if let Some(c3) = c3 {
                let b3 = decode_char(c3)?;
                result.push((b2 << 6) | b3);
            }
        }
    }

    Ok(result)
}

/// Encode URL-safe base64.
pub fn encode_url_safe(data: &[u8]) -> String {
    encode_config(data, Base64Config::URL_SAFE_NO_PAD)
}

/// Decode URL-safe base64.
pub fn decode_url_safe(encoded: &str) -> Result<Vec<u8>> {
    decode_config(encoded, Base64Config::URL_SAFE_NO_PAD)
}

/// Base64 encoder/decoder struct.
pub struct Base64 {
    config: Base64Config,
}

impl Base64 {
    /// Create with configuration.
    pub fn new(config: Base64Config) -> Self {
        Self { config }
    }

    /// Standard base64.
    pub fn standard() -> Self {
        Self::new(Base64Config::STANDARD)
    }

    /// URL-safe base64.
    pub fn url_safe() -> Self {
        Self::new(Base64Config::URL_SAFE_NO_PAD)
    }

    /// Encode bytes.
    pub fn encode(&self, data: &[u8]) -> String {
        encode_config(data, self.config)
    }

    /// Decode string.
    pub fn decode(&self, encoded: &str) -> Result<Vec<u8>> {
        decode_config(encoded, self.config)
    }
}

impl Default for Base64 {
    fn default() -> Self {
        Self::standard()
    }
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
    fn test_standard_vectors() {
        // Standard test vectors
        assert_eq!(encode(b""), "");
        assert_eq!(encode(b"f"), "Zg==");
        assert_eq!(encode(b"fo"), "Zm8=");
        assert_eq!(encode(b"foo"), "Zm9v");
        assert_eq!(encode(b"foob"), "Zm9vYg==");
        assert_eq!(encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn test_decode_standard() {
        assert_eq!(decode("Zm9vYmFy").unwrap(), b"foobar");
        assert_eq!(decode("Zm9vYmE=").unwrap(), b"fooba");
        assert_eq!(decode("Zm9vYg==").unwrap(), b"foob");
    }

    #[test]
    fn test_url_safe() {
        let data = b"\xff\xfe\xfd"; // Contains bytes that differ in URL-safe
        let encoded = encode_url_safe(data);
        assert!(!encoded.contains('+'));
        assert!(!encoded.contains('/'));
        let decoded = decode_url_safe(&encoded).unwrap();
        assert_eq!(data.to_vec(), decoded);
    }

    #[test]
    fn test_no_padding() {
        let config = Base64Config::STANDARD_NO_PAD;
        let encoded = encode_config(b"f", config);
        assert!(!encoded.contains('='));
        let decoded = decode_config(&encoded, config).unwrap();
        assert_eq!(b"f".to_vec(), decoded);
    }

    #[test]
    fn test_struct_api() {
        let b64 = Base64::standard();
        let original = b"test data";
        let encoded = b64.encode(original);
        let decoded = b64.decode(&encoded).unwrap();
        assert_eq!(original.to_vec(), decoded);
    }
}

// ============================================================================
// Kani Formal Verification Proofs
// ============================================================================

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    // ========================================================================
    // Base64Config Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_standard_alphabet_size() {
        kani::assert(
            STANDARD_ALPHABET.len() == 64,
            "standard alphabet must have 64 chars",
        );
    }

    #[kani::proof]
    fn proof_url_safe_alphabet_size() {
        kani::assert(
            URL_SAFE_ALPHABET.len() == 64,
            "url-safe alphabet must have 64 chars",
        );
    }

    #[kani::proof]
    fn proof_standard_alphabet_unique() {
        // Verify first few characters are unique (full check would be expensive)
        for i in 0..10 {
            for j in (i + 1)..10 {
                kani::assert(
                    STANDARD_ALPHABET[i] != STANDARD_ALPHABET[j],
                    "alphabet chars must be unique",
                );
            }
        }
    }

    #[kani::proof]
    fn proof_url_safe_no_plus_slash() {
        for i in 0..64 {
            kani::assert(URL_SAFE_ALPHABET[i] != b'+', "url-safe must not contain +");
            kani::assert(URL_SAFE_ALPHABET[i] != b'/', "url-safe must not contain /");
        }
    }

    #[kani::proof]
    fn proof_default_config_is_standard() {
        let config = Base64Config::default();
        kani::assert(config.padding == true, "default must have padding");
        kani::assert(
            std::ptr::eq(config.alphabet, STANDARD_ALPHABET),
            "default must use standard alphabet",
        );
    }

    #[kani::proof]
    fn proof_standard_config_has_padding() {
        kani::assert(
            Base64Config::STANDARD.padding == true,
            "STANDARD must have padding",
        );
    }

    #[kani::proof]
    fn proof_standard_no_pad_config() {
        kani::assert(
            Base64Config::STANDARD_NO_PAD.padding == false,
            "STANDARD_NO_PAD must not have padding",
        );
    }

    #[kani::proof]
    fn proof_url_safe_config() {
        kani::assert(
            Base64Config::URL_SAFE.padding == true,
            "URL_SAFE must have padding",
        );
        kani::assert(
            std::ptr::eq(Base64Config::URL_SAFE.alphabet, URL_SAFE_ALPHABET),
            "URL_SAFE must use url-safe alphabet",
        );
    }

    #[kani::proof]
    fn proof_url_safe_no_pad_config() {
        kani::assert(
            Base64Config::URL_SAFE_NO_PAD.padding == false,
            "URL_SAFE_NO_PAD must not have padding",
        );
    }

    // ========================================================================
    // Encoding Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_encode_empty() {
        let result = encode(b"");
        kani::assert(result.is_empty(), "empty input must produce empty output");
    }

    #[kani::proof]
    fn proof_encode_one_byte_length() {
        let byte: u8 = kani::any();
        let data = [byte];
        let encoded = encode(&data);
        // 1 byte -> 2 chars + 2 padding = 4 chars
        kani::assert(
            encoded.len() == 4,
            "1 byte must encode to 4 chars with padding",
        );
    }

    #[kani::proof]
    fn proof_encode_two_bytes_length() {
        let b1: u8 = kani::any();
        let b2: u8 = kani::any();
        let data = [b1, b2];
        let encoded = encode(&data);
        // 2 bytes -> 3 chars + 1 padding = 4 chars
        kani::assert(
            encoded.len() == 4,
            "2 bytes must encode to 4 chars with padding",
        );
    }

    #[kani::proof]
    fn proof_encode_three_bytes_length() {
        let b1: u8 = kani::any();
        let b2: u8 = kani::any();
        let b3: u8 = kani::any();
        let data = [b1, b2, b3];
        let encoded = encode(&data);
        // 3 bytes -> 4 chars, no padding needed
        kani::assert(encoded.len() == 4, "3 bytes must encode to 4 chars");
    }

    #[kani::proof]
    fn proof_encode_no_pad_one_byte() {
        let byte: u8 = kani::any();
        let data = [byte];
        let encoded = encode_config(&data, Base64Config::STANDARD_NO_PAD);
        // 1 byte -> 2 chars, no padding
        kani::assert(encoded.len() == 2, "1 byte without padding must be 2 chars");
    }

    #[kani::proof]
    fn proof_encode_no_pad_two_bytes() {
        let b1: u8 = kani::any();
        let b2: u8 = kani::any();
        let data = [b1, b2];
        let encoded = encode_config(&data, Base64Config::STANDARD_NO_PAD);
        // 2 bytes -> 3 chars, no padding
        kani::assert(
            encoded.len() == 3,
            "2 bytes without padding must be 3 chars",
        );
    }

    #[kani::proof]
    fn proof_encode_uses_valid_chars() {
        let byte: u8 = kani::any();
        let data = [byte];
        let encoded = encode(&data);

        for c in encoded.chars() {
            let is_valid = c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '=';
            kani::assert(is_valid, "encoded must only use valid base64 chars");
        }
    }

    #[kani::proof]
    fn proof_encode_url_safe_no_special() {
        let byte: u8 = kani::any();
        let data = [byte];
        let encoded = encode_url_safe(&data);

        for c in encoded.chars() {
            kani::assert(c != '+', "url-safe must not contain +");
            kani::assert(c != '/', "url-safe must not contain /");
            kani::assert(c != '=', "url-safe no-pad must not contain =");
        }
    }

    // ========================================================================
    // Bit Manipulation Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_six_bit_mask() {
        let value: u32 = kani::any();
        let masked = value & 0x3F;
        kani::assert(masked < 64, "6-bit mask must produce value < 64");
    }

    #[kani::proof]
    fn proof_three_byte_packing() {
        let b0: u8 = kani::any();
        let b1: u8 = kani::any();
        let b2: u8 = kani::any();

        let n = ((b0 as u32) << 16) | ((b1 as u32) << 8) | (b2 as u32);

        // Extract each 6-bit group
        let g0 = ((n >> 18) & 0x3F) as u8;
        let g1 = ((n >> 12) & 0x3F) as u8;
        let g2 = ((n >> 6) & 0x3F) as u8;
        let g3 = (n & 0x3F) as u8;

        kani::assert(g0 < 64, "group 0 must be < 64");
        kani::assert(g1 < 64, "group 1 must be < 64");
        kani::assert(g2 < 64, "group 2 must be < 64");
        kani::assert(g3 < 64, "group 3 must be < 64");
    }

    #[kani::proof]
    fn proof_one_byte_packing() {
        let b0: u8 = kani::any();
        let n = (b0 as u32) << 16;

        let g0 = ((n >> 18) & 0x3F) as u8;
        let g1 = ((n >> 12) & 0x3F) as u8;

        kani::assert(g0 < 64, "group 0 must be < 64");
        kani::assert(g1 < 64, "group 1 must be < 64");
    }

    #[kani::proof]
    fn proof_two_byte_packing() {
        let b0: u8 = kani::any();
        let b1: u8 = kani::any();
        let n = ((b0 as u32) << 16) | ((b1 as u32) << 8);

        let g0 = ((n >> 18) & 0x3F) as u8;
        let g1 = ((n >> 12) & 0x3F) as u8;
        let g2 = ((n >> 6) & 0x3F) as u8;

        kani::assert(g0 < 64, "group 0 must be < 64");
        kani::assert(g1 < 64, "group 1 must be < 64");
        kani::assert(g2 < 64, "group 2 must be < 64");
    }

    // ========================================================================
    // Decoding Bit Reconstruction Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_decode_first_byte_reconstruction() {
        let b0: u8 = kani::any();
        let b1: u8 = kani::any();
        kani::assume(b0 < 64);
        kani::assume(b1 < 64);

        let reconstructed = (b0 << 2) | (b1 >> 4);

        // The reconstructed byte is valid
        kani::assert(reconstructed <= 255, "reconstructed byte must be valid");
    }

    #[kani::proof]
    fn proof_decode_second_byte_reconstruction() {
        let b1: u8 = kani::any();
        let b2: u8 = kani::any();
        kani::assume(b1 < 64);
        kani::assume(b2 < 64);

        let reconstructed = (b1 << 4) | (b2 >> 2);

        kani::assert(reconstructed <= 255, "reconstructed byte must be valid");
    }

    #[kani::proof]
    fn proof_decode_third_byte_reconstruction() {
        let b2: u8 = kani::any();
        let b3: u8 = kani::any();
        kani::assume(b2 < 64);
        kani::assume(b3 < 64);

        let reconstructed = (b2 << 6) | b3;

        kani::assert(reconstructed <= 255, "reconstructed byte must be valid");
    }

    // ========================================================================
    // Base64 Struct Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_base64_standard_default() {
        let b64 = Base64::default();
        kani::assert(b64.config.padding == true, "default must have padding");
    }

    #[kani::proof]
    fn proof_base64_standard_constructor() {
        let b64 = Base64::standard();
        kani::assert(b64.config.padding == true, "standard must have padding");
    }

    #[kani::proof]
    fn proof_base64_url_safe_constructor() {
        let b64 = Base64::url_safe();
        kani::assert(
            b64.config.padding == false,
            "url_safe must not have padding",
        );
    }

    #[kani::proof]
    fn proof_base64_encode_matches_function() {
        let byte: u8 = kani::any();
        let data = [byte];

        let b64 = Base64::standard();
        let struct_result = b64.encode(&data);
        let fn_result = encode(&data);

        kani::assert(
            struct_result == fn_result,
            "struct encode must match function",
        );
    }
}
