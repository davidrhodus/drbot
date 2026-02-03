//! Media processing for drbot.
//!
//! This crate provides media processing capabilities including image
//! manipulation and format detection.
//!
//! # Features
//!
//! - Image resize and thumbnail generation
//! - Format conversion
//! - Media type detection (magic bytes)
//! - JPEG compression
//!
//! # Example
//!
//! ```rust,no_run
//! use drbot_media::{image, types::{ResizeOptions, MediaType}};
//!
//! fn example(image_data: &[u8]) -> drbot_core::Result<Vec<u8>> {
//!     // Get image info
//!     let info = image::get_info(image_data)?;
//!     println!("Image: {}x{}", info.width, info.height);
//!
//!     // Resize to fit within 800x600
//!     let resized = image::resize(image_data, &ResizeOptions::fit(800, 600))?;
//!
//!     // Convert to JPEG
//!     let jpeg = image::convert(&resized, MediaType::Jpeg)?;
//!
//!     Ok(jpeg)
//! }
//! ```

pub mod image;
pub mod types;

use crate::types::{ImageInfo, MediaType, ResizeOptions, ThumbnailOptions};
use base64::Engine;

/// High-level media processor.
pub struct MediaProcessor {
    /// Default JPEG quality.
    pub jpeg_quality: u8,
    /// Default thumbnail options.
    pub thumbnail_options: ThumbnailOptions,
}

impl MediaProcessor {
    /// Create a new media processor with default settings.
    pub fn new() -> Self {
        Self {
            jpeg_quality: 85,
            thumbnail_options: ThumbnailOptions::default(),
        }
    }

    /// Set default JPEG quality.
    pub fn with_jpeg_quality(mut self, quality: u8) -> Self {
        self.jpeg_quality = quality.min(100);
        self
    }

    /// Set default thumbnail options.
    pub fn with_thumbnail_options(mut self, options: ThumbnailOptions) -> Self {
        self.thumbnail_options = options;
        self
    }

    /// Detect media type from bytes.
    pub fn detect_type(&self, data: &[u8]) -> MediaType {
        MediaType::from_bytes(data)
    }

    /// Get image information.
    pub fn get_image_info(&self, data: &[u8]) -> drbot_core::Result<ImageInfo> {
        image::get_info(data)
    }

    /// Resize an image.
    pub fn resize(&self, data: &[u8], options: &ResizeOptions) -> drbot_core::Result<Vec<u8>> {
        image::resize(data, options)
    }

    /// Create a thumbnail.
    pub fn thumbnail(&self, data: &[u8]) -> drbot_core::Result<Vec<u8>> {
        image::thumbnail(data, &self.thumbnail_options)
    }

    /// Create a thumbnail with custom options.
    pub fn thumbnail_with_options(
        &self,
        data: &[u8],
        options: &ThumbnailOptions,
    ) -> drbot_core::Result<Vec<u8>> {
        image::thumbnail(data, options)
    }

    /// Convert image format.
    pub fn convert(&self, data: &[u8], format: MediaType) -> drbot_core::Result<Vec<u8>> {
        image::convert(data, format)
    }

    /// Compress a JPEG image.
    pub fn compress_jpeg(&self, data: &[u8], quality: Option<u8>) -> drbot_core::Result<Vec<u8>> {
        image::compress_jpeg(data, quality.unwrap_or(self.jpeg_quality))
    }

    /// Encode image data as base64.
    pub fn to_base64(&self, data: &[u8]) -> String {
        base64::engine::general_purpose::STANDARD.encode(data)
    }

    /// Decode base64 to image data.
    pub fn from_base64(&self, encoded: &str) -> drbot_core::Result<Vec<u8>> {
        base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|e| drbot_core::Error::InvalidInput(format!("Invalid base64: {}", e)))
    }

    /// Create a data URL for an image.
    pub fn to_data_url(&self, data: &[u8]) -> String {
        let media_type = self.detect_type(data);
        let base64 = self.to_base64(data);
        format!("data:{};base64,{}", media_type.mime_type(), base64)
    }
}

impl Default for MediaProcessor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_image() -> Vec<u8> {
        image::create_solid(100, 100, 0, 128, 255)
    }

    #[test]
    fn test_media_processor_new() {
        let processor = MediaProcessor::new();
        assert_eq!(processor.jpeg_quality, 85);
    }

    #[test]
    fn test_detect_type() {
        let processor = MediaProcessor::new();
        let data = create_test_image();
        assert_eq!(processor.detect_type(&data), MediaType::Png);
    }

    #[test]
    fn test_get_image_info() {
        let processor = MediaProcessor::new();
        let data = create_test_image();
        let info = processor.get_image_info(&data).unwrap();
        assert_eq!(info.width, 100);
        assert_eq!(info.height, 100);
    }

    #[test]
    fn test_resize() {
        let processor = MediaProcessor::new();
        let data = create_test_image();
        let resized = processor
            .resize(&data, &ResizeOptions::fit(50, 50))
            .unwrap();

        let info = processor.get_image_info(&resized).unwrap();
        assert!(info.width <= 50);
        assert!(info.height <= 50);
    }

    #[test]
    fn test_thumbnail() {
        let processor = MediaProcessor::new();
        let data = create_test_image();
        let thumb = processor.thumbnail(&data).unwrap();

        let info = processor.get_image_info(&thumb).unwrap();
        assert!(info.width <= 256);
    }

    #[test]
    fn test_convert() {
        let processor = MediaProcessor::new();
        let data = create_test_image();
        let jpeg = processor.convert(&data, MediaType::Jpeg).unwrap();

        assert_eq!(processor.detect_type(&jpeg), MediaType::Jpeg);
    }

    #[test]
    fn test_base64_roundtrip() {
        let processor = MediaProcessor::new();
        let data = create_test_image();

        let encoded = processor.to_base64(&data);
        let decoded = processor.from_base64(&encoded).unwrap();

        assert_eq!(data, decoded);
    }

    #[test]
    fn test_data_url() {
        let processor = MediaProcessor::new();
        let data = create_test_image();
        let url = processor.to_data_url(&data);

        assert!(url.starts_with("data:image/png;base64,"));
    }
}
