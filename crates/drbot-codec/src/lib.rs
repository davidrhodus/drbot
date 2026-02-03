//! Generic codec trait for drbot.
//!
//! This crate provides:
//! - Encoder/Decoder traits
//! - Codec composition
//! - Common codec implementations

use thiserror::Error;

/// Codec error types.
#[derive(Error, Debug, Clone)]
pub enum CodecError {
    #[error("Encoding failed: {0}")]
    EncodeFailed(String),

    #[error("Decoding failed: {0}")]
    DecodeFailed(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Buffer too small")]
    BufferTooSmall,
}

/// Result type for codec operations.
pub type Result<T> = std::result::Result<T, CodecError>;

/// Encoder trait.
pub trait Encoder<T> {
    /// Encode value to bytes.
    fn encode(&self, value: &T) -> Result<Vec<u8>>;

    /// Encode to existing buffer.
    fn encode_to(&self, value: &T, buf: &mut Vec<u8>) -> Result<()> {
        let encoded = self.encode(value)?;
        buf.extend_from_slice(&encoded);
        Ok(())
    }
}

/// Decoder trait.
pub trait Decoder<T> {
    /// Decode bytes to value.
    fn decode(&self, bytes: &[u8]) -> Result<T>;
}

/// Combined codec trait.
pub trait Codec<T>: Encoder<T> + Decoder<T> {}

impl<T, C: Encoder<T> + Decoder<T>> Codec<T> for C {}

/// Identity codec (no transformation).
pub struct Identity;

impl Encoder<Vec<u8>> for Identity {
    fn encode(&self, value: &Vec<u8>) -> Result<Vec<u8>> {
        Ok(value.clone())
    }
}

impl Decoder<Vec<u8>> for Identity {
    fn decode(&self, bytes: &[u8]) -> Result<Vec<u8>> {
        Ok(bytes.to_vec())
    }
}

/// String codec (UTF-8).
pub struct Utf8Codec;

impl Encoder<String> for Utf8Codec {
    fn encode(&self, value: &String) -> Result<Vec<u8>> {
        Ok(value.as_bytes().to_vec())
    }
}

impl Decoder<String> for Utf8Codec {
    fn decode(&self, bytes: &[u8]) -> Result<String> {
        String::from_utf8(bytes.to_vec()).map_err(|e| CodecError::DecodeFailed(e.to_string()))
    }
}

/// Integer codec (big-endian).
pub struct BigEndianCodec;

impl Encoder<u32> for BigEndianCodec {
    fn encode(&self, value: &u32) -> Result<Vec<u8>> {
        Ok(value.to_be_bytes().to_vec())
    }
}

impl Decoder<u32> for BigEndianCodec {
    fn decode(&self, bytes: &[u8]) -> Result<u32> {
        if bytes.len() < 4 {
            return Err(CodecError::BufferTooSmall);
        }
        let arr: [u8; 4] = bytes[..4]
            .try_into()
            .map_err(|_| CodecError::DecodeFailed("Invalid length".to_string()))?;
        Ok(u32::from_be_bytes(arr))
    }
}

impl Encoder<u64> for BigEndianCodec {
    fn encode(&self, value: &u64) -> Result<Vec<u8>> {
        Ok(value.to_be_bytes().to_vec())
    }
}

impl Decoder<u64> for BigEndianCodec {
    fn decode(&self, bytes: &[u8]) -> Result<u64> {
        if bytes.len() < 8 {
            return Err(CodecError::BufferTooSmall);
        }
        let arr: [u8; 8] = bytes[..8]
            .try_into()
            .map_err(|_| CodecError::DecodeFailed("Invalid length".to_string()))?;
        Ok(u64::from_be_bytes(arr))
    }
}

/// Little-endian codec.
pub struct LittleEndianCodec;

impl Encoder<u32> for LittleEndianCodec {
    fn encode(&self, value: &u32) -> Result<Vec<u8>> {
        Ok(value.to_le_bytes().to_vec())
    }
}

impl Decoder<u32> for LittleEndianCodec {
    fn decode(&self, bytes: &[u8]) -> Result<u32> {
        if bytes.len() < 4 {
            return Err(CodecError::BufferTooSmall);
        }
        let arr: [u8; 4] = bytes[..4]
            .try_into()
            .map_err(|_| CodecError::DecodeFailed("Invalid length".to_string()))?;
        Ok(u32::from_le_bytes(arr))
    }
}

/// Composed codec (encode with A, then B).
pub struct ComposedCodec<A, B> {
    first: A,
    second: B,
}

impl<A, B> ComposedCodec<A, B> {
    /// Create composed codec.
    pub fn new(first: A, second: B) -> Self {
        Self { first, second }
    }
}

impl<T, A, B> Encoder<T> for ComposedCodec<A, B>
where
    A: Encoder<T>,
    B: Encoder<Vec<u8>>,
{
    fn encode(&self, value: &T) -> Result<Vec<u8>> {
        let intermediate = self.first.encode(value)?;
        self.second.encode(&intermediate)
    }
}

impl<T, A, B> Decoder<T> for ComposedCodec<A, B>
where
    A: Decoder<T>,
    B: Decoder<Vec<u8>>,
{
    fn decode(&self, bytes: &[u8]) -> Result<T> {
        let intermediate = self.second.decode(bytes)?;
        self.first.decode(&intermediate)
    }
}

/// Length-prefixed codec.
pub struct LengthPrefixedCodec<C> {
    inner: C,
}

impl<C> LengthPrefixedCodec<C> {
    /// Create length-prefixed codec.
    pub fn new(inner: C) -> Self {
        Self { inner }
    }
}

impl<T, C: Encoder<T>> Encoder<T> for LengthPrefixedCodec<C> {
    fn encode(&self, value: &T) -> Result<Vec<u8>> {
        let encoded = self.inner.encode(value)?;
        let len = encoded.len() as u32;
        let mut result = len.to_be_bytes().to_vec();
        result.extend_from_slice(&encoded);
        Ok(result)
    }
}

impl<T, C: Decoder<T>> Decoder<T> for LengthPrefixedCodec<C> {
    fn decode(&self, bytes: &[u8]) -> Result<T> {
        if bytes.len() < 4 {
            return Err(CodecError::BufferTooSmall);
        }
        let len = u32::from_be_bytes(bytes[..4].try_into().unwrap()) as usize;
        if bytes.len() < 4 + len {
            return Err(CodecError::BufferTooSmall);
        }
        self.inner.decode(&bytes[4..4 + len])
    }
}

/// Codec builder.
pub struct CodecBuilder<C> {
    codec: C,
}

impl CodecBuilder<Identity> {
    /// Start building codec.
    pub fn new() -> Self {
        Self { codec: Identity }
    }
}

impl Default for CodecBuilder<Identity> {
    fn default() -> Self {
        Self::new()
    }
}

impl<C> CodecBuilder<C> {
    /// Add length prefix.
    pub fn with_length_prefix(self) -> CodecBuilder<LengthPrefixedCodec<C>> {
        CodecBuilder {
            codec: LengthPrefixedCodec::new(self.codec),
        }
    }

    /// Compose with another codec.
    pub fn then<D>(self, other: D) -> CodecBuilder<ComposedCodec<C, D>> {
        CodecBuilder {
            codec: ComposedCodec::new(self.codec, other),
        }
    }

    /// Build codec.
    pub fn build(self) -> C {
        self.codec
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_utf8_codec() {
        let codec = Utf8Codec;
        let original = "Hello, World!".to_string();

        let encoded = codec.encode(&original).unwrap();
        let decoded = codec.decode(&encoded).unwrap();

        assert_eq!(original, decoded);
    }

    #[test]
    fn test_big_endian() {
        let codec = BigEndianCodec;
        let original: u32 = 0x12345678;

        let encoded = codec.encode(&original).unwrap();
        assert_eq!(encoded, vec![0x12, 0x34, 0x56, 0x78]);

        let decoded: u32 = codec.decode(&encoded).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn test_little_endian() {
        let codec = LittleEndianCodec;
        let original: u32 = 0x12345678;

        let encoded = codec.encode(&original).unwrap();
        assert_eq!(encoded, vec![0x78, 0x56, 0x34, 0x12]);

        let decoded: u32 = codec.decode(&encoded).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn test_length_prefixed() {
        let codec = LengthPrefixedCodec::new(Utf8Codec);
        let original = "Hello".to_string();

        let encoded = codec.encode(&original).unwrap();
        assert_eq!(encoded[..4], [0, 0, 0, 5]); // Length prefix

        let decoded = codec.decode(&encoded).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn test_identity() {
        let codec = Identity;
        let original = vec![1, 2, 3, 4, 5];

        let encoded = codec.encode(&original).unwrap();
        let decoded: Vec<u8> = codec.decode(&encoded).unwrap();

        assert_eq!(original, decoded);
    }
}
