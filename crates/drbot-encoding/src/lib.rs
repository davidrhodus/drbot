//! Encoding/decoding utilities for drbot.
//!
//! This crate provides:
//! - Base64 encoding/decoding
//! - Hex encoding/decoding
//! - URL encoding/decoding
//! - Percent encoding

use thiserror::Error;

/// Encoding error types.
#[derive(Error, Debug)]
pub enum EncodingError {
    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Decode failed: {0}")]
    DecodeFailed(String),
}

/// Result type for encoding operations.
pub type Result<T> = std::result::Result<T, EncodingError>;

/// Base64 encoder/decoder.
pub struct Base64;

impl Base64 {
    const ALPHABET: &'static [u8] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    const URL_ALPHABET: &'static [u8] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

    /// Encode bytes to base64.
    pub fn encode(data: &[u8]) -> String {
        Self::encode_with_alphabet(data, Self::ALPHABET, true)
    }

    /// Encode bytes to base64url (no padding).
    pub fn encode_url(data: &[u8]) -> String {
        Self::encode_with_alphabet(data, Self::URL_ALPHABET, false)
    }

    fn encode_with_alphabet(data: &[u8], alphabet: &[u8], padding: bool) -> String {
        let mut result = Vec::with_capacity((data.len() + 2) / 3 * 4);

        for chunk in data.chunks(3) {
            let b0 = chunk[0] as usize;
            let b1 = chunk.get(1).copied().unwrap_or(0) as usize;
            let b2 = chunk.get(2).copied().unwrap_or(0) as usize;

            result.push(alphabet[b0 >> 2]);
            result.push(alphabet[((b0 & 0x03) << 4) | (b1 >> 4)]);

            if chunk.len() > 1 {
                result.push(alphabet[((b1 & 0x0f) << 2) | (b2 >> 6)]);
            } else if padding {
                result.push(b'=');
            }

            if chunk.len() > 2 {
                result.push(alphabet[b2 & 0x3f]);
            } else if padding {
                result.push(b'=');
            }
        }

        String::from_utf8(result).unwrap()
    }

    /// Decode base64 to bytes.
    pub fn decode(data: &str) -> Result<Vec<u8>> {
        Self::decode_with_alphabet(data, Self::ALPHABET)
    }

    /// Decode base64url to bytes.
    pub fn decode_url(data: &str) -> Result<Vec<u8>> {
        Self::decode_with_alphabet(data, Self::URL_ALPHABET)
    }

    fn decode_with_alphabet(data: &str, alphabet: &[u8]) -> Result<Vec<u8>> {
        let data = data.trim_end_matches('=');
        let mut result = Vec::with_capacity(data.len() * 3 / 4);

        let decode_char = |c: u8| -> Result<u8> {
            alphabet
                .iter()
                .position(|&x| x == c)
                .map(|p| p as u8)
                .ok_or_else(|| {
                    EncodingError::DecodeFailed(format!("Invalid character: {}", c as char))
                })
        };

        let bytes: Vec<u8> = data.bytes().collect();
        for chunk in bytes.chunks(4) {
            let b0 = decode_char(chunk[0])?;
            let b1 = decode_char(chunk[1])?;

            result.push((b0 << 2) | (b1 >> 4));

            if chunk.len() > 2 {
                let b2 = decode_char(chunk[2])?;
                result.push((b1 << 4) | (b2 >> 2));

                if chunk.len() > 3 {
                    let b3 = decode_char(chunk[3])?;
                    result.push((b2 << 6) | b3);
                }
            }
        }

        Ok(result)
    }
}

/// Hex encoder/decoder.
pub struct Hex;

impl Hex {
    const HEX_CHARS: &'static [u8] = b"0123456789abcdef";

    /// Encode bytes to hex string.
    pub fn encode(data: &[u8]) -> String {
        let mut result = String::with_capacity(data.len() * 2);
        for &byte in data {
            result.push(Self::HEX_CHARS[(byte >> 4) as usize] as char);
            result.push(Self::HEX_CHARS[(byte & 0x0f) as usize] as char);
        }
        result
    }

    /// Encode bytes to uppercase hex string.
    pub fn encode_upper(data: &[u8]) -> String {
        Self::encode(data).to_uppercase()
    }

    /// Decode hex string to bytes.
    pub fn decode(data: &str) -> Result<Vec<u8>> {
        if data.len() % 2 != 0 {
            return Err(EncodingError::InvalidInput(
                "Hex string must have even length".to_string(),
            ));
        }

        let mut result = Vec::with_capacity(data.len() / 2);
        let bytes: Vec<u8> = data.bytes().collect();

        for chunk in bytes.chunks(2) {
            let high = Self::decode_nibble(chunk[0])?;
            let low = Self::decode_nibble(chunk[1])?;
            result.push((high << 4) | low);
        }

        Ok(result)
    }

    fn decode_nibble(c: u8) -> Result<u8> {
        match c {
            b'0'..=b'9' => Ok(c - b'0'),
            b'a'..=b'f' => Ok(c - b'a' + 10),
            b'A'..=b'F' => Ok(c - b'A' + 10),
            _ => Err(EncodingError::DecodeFailed(format!(
                "Invalid hex character: {}",
                c as char
            ))),
        }
    }
}

/// URL encoder/decoder.
pub struct Url;

impl Url {
    /// Encode string for URL.
    pub fn encode(data: &str) -> String {
        let mut result = String::with_capacity(data.len() * 3);

        for byte in data.bytes() {
            if Self::is_unreserved(byte) {
                result.push(byte as char);
            } else {
                result.push('%');
                result.push_str(&format!("{:02X}", byte));
            }
        }

        result
    }

    /// Encode for path component.
    pub fn encode_path(data: &str) -> String {
        let mut result = String::with_capacity(data.len() * 3);

        for byte in data.bytes() {
            if Self::is_unreserved(byte) || byte == b'/' {
                result.push(byte as char);
            } else {
                result.push('%');
                result.push_str(&format!("{:02X}", byte));
            }
        }

        result
    }

    /// Decode URL-encoded string.
    pub fn decode(data: &str) -> Result<String> {
        let mut result = Vec::with_capacity(data.len());
        let bytes: Vec<u8> = data.bytes().collect();
        let mut i = 0;

        while i < bytes.len() {
            if bytes[i] == b'%' && i + 2 < bytes.len() {
                let high = Hex::decode(&String::from_utf8_lossy(&bytes[i + 1..i + 3]))?;
                if let Some(&byte) = high.first() {
                    result.push(byte);
                }
                i += 3;
            } else if bytes[i] == b'+' {
                result.push(b' ');
                i += 1;
            } else {
                result.push(bytes[i]);
                i += 1;
            }
        }

        String::from_utf8(result)
            .map_err(|e| EncodingError::DecodeFailed(format!("Invalid UTF-8: {}", e)))
    }

    fn is_unreserved(byte: u8) -> bool {
        matches!(byte, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~')
    }
}

/// Query string encoder/decoder.
pub struct QueryString;

impl QueryString {
    /// Encode key-value pairs to query string.
    pub fn encode<K, V>(params: &[(K, V)]) -> String
    where
        K: AsRef<str>,
        V: AsRef<str>,
    {
        params
            .iter()
            .map(|(k, v)| format!("{}={}", Url::encode(k.as_ref()), Url::encode(v.as_ref())))
            .collect::<Vec<_>>()
            .join("&")
    }

    /// Decode query string to key-value pairs.
    pub fn decode(query: &str) -> Result<Vec<(String, String)>> {
        let query = query.trim_start_matches('?');
        let mut result = Vec::new();

        for pair in query.split('&') {
            if pair.is_empty() {
                continue;
            }

            let parts: Vec<&str> = pair.splitn(2, '=').collect();
            let key = Url::decode(parts[0])?;
            let value = if parts.len() > 1 {
                Url::decode(parts[1])?
            } else {
                String::new()
            };

            result.push((key, value));
        }

        Ok(result)
    }
}

/// HTML entity encoder/decoder.
pub struct HtmlEntities;

impl HtmlEntities {
    /// Encode HTML entities.
    pub fn encode(data: &str) -> String {
        data.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&#39;")
    }

    /// Decode HTML entities.
    pub fn decode(data: &str) -> String {
        data.replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&#39;", "'")
            .replace("&amp;", "&")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base64_encode() {
        assert_eq!(Base64::encode(b"Hello, World!"), "SGVsbG8sIFdvcmxkIQ==");
        assert_eq!(Base64::encode(b""), "");
        assert_eq!(Base64::encode(b"a"), "YQ==");
    }

    #[test]
    fn test_base64_decode() {
        assert_eq!(
            Base64::decode("SGVsbG8sIFdvcmxkIQ==").unwrap(),
            b"Hello, World!"
        );
        assert_eq!(Base64::decode("").unwrap(), b"");
        assert_eq!(Base64::decode("YQ==").unwrap(), b"a");
    }

    #[test]
    fn test_base64_url() {
        let data = b"test+data/here";
        let encoded = Base64::encode_url(data);
        assert!(!encoded.contains('+'));
        assert!(!encoded.contains('/'));
        assert!(!encoded.contains('='));
        let decoded = Base64::decode_url(&encoded).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn test_hex_encode() {
        assert_eq!(Hex::encode(b"Hello"), "48656c6c6f");
        assert_eq!(Hex::encode_upper(b"Hello"), "48656C6C6F");
    }

    #[test]
    fn test_hex_decode() {
        assert_eq!(Hex::decode("48656c6c6f").unwrap(), b"Hello");
        assert_eq!(Hex::decode("48656C6C6F").unwrap(), b"Hello");
    }

    #[test]
    fn test_url_encode() {
        assert_eq!(Url::encode("hello world"), "hello%20world");
        assert_eq!(Url::encode("a=b&c=d"), "a%3Db%26c%3Dd");
    }

    #[test]
    fn test_url_decode() {
        assert_eq!(Url::decode("hello%20world").unwrap(), "hello world");
        assert_eq!(Url::decode("hello+world").unwrap(), "hello world");
    }

    #[test]
    fn test_query_string() {
        let params = [("name", "John Doe"), ("age", "30")];
        let encoded = QueryString::encode(&params);
        assert!(encoded.contains("name=John%20Doe"));
        assert!(encoded.contains("age=30"));

        let decoded = QueryString::decode(&encoded).unwrap();
        assert_eq!(decoded.len(), 2);
    }

    #[test]
    fn test_html_entities() {
        let input = "<script>alert('xss')</script>";
        let encoded = HtmlEntities::encode(input);
        assert!(!encoded.contains('<'));
        assert!(!encoded.contains('>'));

        let decoded = HtmlEntities::decode(&encoded);
        assert_eq!(decoded, input);
    }
}
