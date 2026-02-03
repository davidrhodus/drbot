//! Media processing types.

use serde::{Deserialize, Serialize};

/// Supported media types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MediaType {
    /// JPEG image.
    Jpeg,
    /// PNG image.
    Png,
    /// GIF image.
    Gif,
    /// WebP image.
    WebP,
    /// BMP image.
    Bmp,
    /// ICO image.
    Ico,
    /// TIFF image.
    Tiff,
    /// PDF document.
    Pdf,
    /// Plain text.
    Text,
    /// Unknown type.
    Unknown,
}

impl MediaType {
    /// Detect media type from file extension.
    pub fn from_extension(ext: &str) -> Self {
        match ext.to_lowercase().as_str() {
            "jpg" | "jpeg" => Self::Jpeg,
            "png" => Self::Png,
            "gif" => Self::Gif,
            "webp" => Self::WebP,
            "bmp" => Self::Bmp,
            "ico" => Self::Ico,
            "tiff" | "tif" => Self::Tiff,
            "pdf" => Self::Pdf,
            "txt" => Self::Text,
            _ => Self::Unknown,
        }
    }

    /// Detect media type from magic bytes.
    pub fn from_bytes(data: &[u8]) -> Self {
        if data.len() < 8 {
            return Self::Unknown;
        }

        // JPEG: FF D8 FF
        if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
            return Self::Jpeg;
        }

        // PNG: 89 50 4E 47 0D 0A 1A 0A
        if data.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) {
            return Self::Png;
        }

        // GIF: 47 49 46 38
        if data.starts_with(&[0x47, 0x49, 0x46, 0x38]) {
            return Self::Gif;
        }

        // WebP: RIFF....WEBP
        if data.len() >= 12 && data.starts_with(b"RIFF") && &data[8..12] == b"WEBP" {
            return Self::WebP;
        }

        // BMP: 42 4D
        if data.starts_with(&[0x42, 0x4D]) {
            return Self::Bmp;
        }

        // ICO: 00 00 01 00
        if data.starts_with(&[0x00, 0x00, 0x01, 0x00]) {
            return Self::Ico;
        }

        // TIFF: 49 49 2A 00 or 4D 4D 00 2A
        if data.starts_with(&[0x49, 0x49, 0x2A, 0x00])
            || data.starts_with(&[0x4D, 0x4D, 0x00, 0x2A])
        {
            return Self::Tiff;
        }

        // PDF: 25 50 44 46 (%PDF)
        if data.starts_with(b"%PDF") {
            return Self::Pdf;
        }

        Self::Unknown
    }

    /// Get MIME type string.
    pub fn mime_type(&self) -> &'static str {
        match self {
            Self::Jpeg => "image/jpeg",
            Self::Png => "image/png",
            Self::Gif => "image/gif",
            Self::WebP => "image/webp",
            Self::Bmp => "image/bmp",
            Self::Ico => "image/x-icon",
            Self::Tiff => "image/tiff",
            Self::Pdf => "application/pdf",
            Self::Text => "text/plain",
            Self::Unknown => "application/octet-stream",
        }
    }

    /// Get file extension.
    pub fn extension(&self) -> &'static str {
        match self {
            Self::Jpeg => "jpg",
            Self::Png => "png",
            Self::Gif => "gif",
            Self::WebP => "webp",
            Self::Bmp => "bmp",
            Self::Ico => "ico",
            Self::Tiff => "tiff",
            Self::Pdf => "pdf",
            Self::Text => "txt",
            Self::Unknown => "bin",
        }
    }

    /// Check if this is an image type.
    pub fn is_image(&self) -> bool {
        matches!(
            self,
            Self::Jpeg | Self::Png | Self::Gif | Self::WebP | Self::Bmp | Self::Ico | Self::Tiff
        )
    }
}

/// Image resize mode.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub enum ResizeMode {
    /// Resize to fit within bounds, preserving aspect ratio.
    #[default]
    Fit,
    /// Resize to fill bounds, cropping if necessary.
    Fill,
    /// Resize to exact dimensions, ignoring aspect ratio.
    Exact,
    /// Resize to exact width, calculating height to preserve aspect ratio.
    Width,
    /// Resize to exact height, calculating width to preserve aspect ratio.
    Height,
}

/// Image resize options.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResizeOptions {
    /// Target width.
    pub width: Option<u32>,
    /// Target height.
    pub height: Option<u32>,
    /// Resize mode.
    pub mode: ResizeMode,
    /// Output format (defaults to input format).
    pub format: Option<MediaType>,
    /// JPEG quality (1-100).
    pub quality: Option<u8>,
}

impl ResizeOptions {
    /// Create options for resizing to fit within bounds.
    pub fn fit(width: u32, height: u32) -> Self {
        Self {
            width: Some(width),
            height: Some(height),
            mode: ResizeMode::Fit,
            ..Default::default()
        }
    }

    /// Create options for resizing to fill bounds.
    pub fn fill(width: u32, height: u32) -> Self {
        Self {
            width: Some(width),
            height: Some(height),
            mode: ResizeMode::Fill,
            ..Default::default()
        }
    }

    /// Create options for exact resize.
    pub fn exact(width: u32, height: u32) -> Self {
        Self {
            width: Some(width),
            height: Some(height),
            mode: ResizeMode::Exact,
            ..Default::default()
        }
    }

    /// Create options for width-based resize.
    pub fn to_width(width: u32) -> Self {
        Self {
            width: Some(width),
            height: None,
            mode: ResizeMode::Width,
            ..Default::default()
        }
    }

    /// Create options for height-based resize.
    pub fn to_height(height: u32) -> Self {
        Self {
            width: None,
            height: Some(height),
            mode: ResizeMode::Height,
            ..Default::default()
        }
    }

    /// Set output format.
    pub fn with_format(mut self, format: MediaType) -> Self {
        self.format = Some(format);
        self
    }

    /// Set JPEG quality.
    pub fn with_quality(mut self, quality: u8) -> Self {
        self.quality = Some(quality.min(100));
        self
    }
}

/// Image metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageInfo {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Media type.
    pub media_type: MediaType,
    /// File size in bytes.
    pub size: usize,
}

/// Thumbnail options.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThumbnailOptions {
    /// Maximum width.
    pub max_width: u32,
    /// Maximum height.
    pub max_height: u32,
    /// Output format.
    pub format: MediaType,
    /// Quality (for JPEG).
    pub quality: u8,
}

impl Default for ThumbnailOptions {
    fn default() -> Self {
        Self {
            max_width: 256,
            max_height: 256,
            format: MediaType::Jpeg,
            quality: 80,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_media_type_from_extension() {
        assert_eq!(MediaType::from_extension("jpg"), MediaType::Jpeg);
        assert_eq!(MediaType::from_extension("JPEG"), MediaType::Jpeg);
        assert_eq!(MediaType::from_extension("png"), MediaType::Png);
        assert_eq!(MediaType::from_extension("pdf"), MediaType::Pdf);
        assert_eq!(MediaType::from_extension("xyz"), MediaType::Unknown);
    }

    #[test]
    fn test_media_type_from_bytes() {
        // JPEG
        assert_eq!(
            MediaType::from_bytes(&[0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x00, 0x00, 0x00]),
            MediaType::Jpeg
        );

        // PNG
        assert_eq!(
            MediaType::from_bytes(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]),
            MediaType::Png
        );

        // GIF
        assert_eq!(
            MediaType::from_bytes(&[0x47, 0x49, 0x46, 0x38, 0x39, 0x61, 0x00, 0x00]),
            MediaType::Gif
        );

        // PDF
        assert_eq!(MediaType::from_bytes(b"%PDF-1.4 test file"), MediaType::Pdf);
    }

    #[test]
    fn test_media_type_mime() {
        assert_eq!(MediaType::Jpeg.mime_type(), "image/jpeg");
        assert_eq!(MediaType::Png.mime_type(), "image/png");
        assert_eq!(MediaType::Pdf.mime_type(), "application/pdf");
    }

    #[test]
    fn test_media_type_is_image() {
        assert!(MediaType::Jpeg.is_image());
        assert!(MediaType::Png.is_image());
        assert!(!MediaType::Pdf.is_image());
        assert!(!MediaType::Text.is_image());
    }

    #[test]
    fn test_resize_options() {
        let opts = ResizeOptions::fit(800, 600).with_quality(90);
        assert_eq!(opts.width, Some(800));
        assert_eq!(opts.height, Some(600));
        assert_eq!(opts.quality, Some(90));
    }

    #[test]
    fn test_thumbnail_options_default() {
        let opts = ThumbnailOptions::default();
        assert_eq!(opts.max_width, 256);
        assert_eq!(opts.max_height, 256);
        assert_eq!(opts.quality, 80);
    }
}
