//! Image processing operations.

use crate::types::{ImageInfo, MediaType, ResizeMode, ResizeOptions, ThumbnailOptions};
use image::{DynamicImage, GenericImageView, ImageFormat};
use std::io::Cursor;
use tracing::debug;

/// Load an image from bytes.
pub fn load_image(data: &[u8]) -> drbot_core::Result<DynamicImage> {
    image::load_from_memory(data)
        .map_err(|e| drbot_core::Error::InvalidInput(format!("Failed to load image: {}", e)))
}

/// Get image information.
pub fn get_info(data: &[u8]) -> drbot_core::Result<ImageInfo> {
    let img = load_image(data)?;
    let (width, height) = img.dimensions();
    let media_type = MediaType::from_bytes(data);

    Ok(ImageInfo {
        width,
        height,
        media_type,
        size: data.len(),
    })
}

/// Resize an image.
pub fn resize(data: &[u8], options: &ResizeOptions) -> drbot_core::Result<Vec<u8>> {
    let img = load_image(data)?;
    let (orig_width, orig_height) = img.dimensions();

    let (new_width, new_height) = calculate_dimensions(
        orig_width,
        orig_height,
        options.width,
        options.height,
        options.mode,
    );

    debug!(
        orig_width = orig_width,
        orig_height = orig_height,
        new_width = new_width,
        new_height = new_height,
        "Resizing image"
    );

    let resized = match options.mode {
        ResizeMode::Fill => {
            // For fill mode, we need to crop after resize
            let scale_w = new_width as f64 / orig_width as f64;
            let scale_h = new_height as f64 / orig_height as f64;
            let scale = scale_w.max(scale_h);

            let scaled_w = (orig_width as f64 * scale) as u32;
            let scaled_h = (orig_height as f64 * scale) as u32;

            let scaled =
                img.resize_exact(scaled_w, scaled_h, image::imageops::FilterType::Lanczos3);

            // Crop to target size
            let x = (scaled_w - new_width) / 2;
            let y = (scaled_h - new_height) / 2;

            scaled.crop_imm(x, y, new_width, new_height)
        }
        ResizeMode::Exact => {
            // Exact mode: resize to exact dimensions
            img.resize_exact(new_width, new_height, image::imageops::FilterType::Lanczos3)
        }
        _ => img.resize(new_width, new_height, image::imageops::FilterType::Lanczos3),
    };

    encode_image(&resized, options.format, options.quality, data)
}

/// Create a thumbnail.
pub fn thumbnail(data: &[u8], options: &ThumbnailOptions) -> drbot_core::Result<Vec<u8>> {
    let img = load_image(data)?;

    let thumb = img.thumbnail(options.max_width, options.max_height);

    encode_image(&thumb, Some(options.format), Some(options.quality), data)
}

/// Convert image to a different format.
pub fn convert(data: &[u8], target_format: MediaType) -> drbot_core::Result<Vec<u8>> {
    let img = load_image(data)?;
    encode_image(&img, Some(target_format), None, data)
}

/// Compress a JPEG image.
pub fn compress_jpeg(data: &[u8], quality: u8) -> drbot_core::Result<Vec<u8>> {
    let img = load_image(data)?;

    let mut buffer = Cursor::new(Vec::new());
    let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buffer, quality.min(100));

    img.write_with_encoder(encoder)
        .map_err(|e| drbot_core::Error::Internal(format!("Failed to encode JPEG: {}", e)))?;

    Ok(buffer.into_inner())
}

/// Calculate new dimensions based on resize mode.
fn calculate_dimensions(
    orig_width: u32,
    orig_height: u32,
    target_width: Option<u32>,
    target_height: Option<u32>,
    mode: ResizeMode,
) -> (u32, u32) {
    let aspect = orig_width as f64 / orig_height as f64;

    match mode {
        ResizeMode::Width => {
            let w = target_width.unwrap_or(orig_width);
            let h = (w as f64 / aspect) as u32;
            (w, h.max(1))
        }
        ResizeMode::Height => {
            let h = target_height.unwrap_or(orig_height);
            let w = (h as f64 * aspect) as u32;
            (w.max(1), h)
        }
        ResizeMode::Exact => (
            target_width.unwrap_or(orig_width),
            target_height.unwrap_or(orig_height),
        ),
        ResizeMode::Fill => (
            target_width.unwrap_or(orig_width),
            target_height.unwrap_or(orig_height),
        ),
        ResizeMode::Fit => {
            let target_w = target_width.unwrap_or(orig_width);
            let target_h = target_height.unwrap_or(orig_height);

            let scale_w = target_w as f64 / orig_width as f64;
            let scale_h = target_h as f64 / orig_height as f64;
            let scale = scale_w.min(scale_h);

            let new_w = (orig_width as f64 * scale) as u32;
            let new_h = (orig_height as f64 * scale) as u32;

            (new_w.max(1), new_h.max(1))
        }
    }
}

/// Encode an image to bytes.
fn encode_image(
    img: &DynamicImage,
    format: Option<MediaType>,
    quality: Option<u8>,
    original_data: &[u8],
) -> drbot_core::Result<Vec<u8>> {
    let format = format.unwrap_or_else(|| MediaType::from_bytes(original_data));

    let image_format = match format {
        MediaType::Jpeg => ImageFormat::Jpeg,
        MediaType::Png => ImageFormat::Png,
        MediaType::Gif => ImageFormat::Gif,
        MediaType::WebP => ImageFormat::WebP,
        MediaType::Bmp => ImageFormat::Bmp,
        MediaType::Ico => ImageFormat::Ico,
        MediaType::Tiff => ImageFormat::Tiff,
        _ => ImageFormat::Png, // Default to PNG
    };

    let mut buffer = Cursor::new(Vec::new());

    if image_format == ImageFormat::Jpeg {
        let q = quality.unwrap_or(85);
        let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buffer, q);
        img.write_with_encoder(encoder)
            .map_err(|e| drbot_core::Error::Internal(format!("Failed to encode: {}", e)))?;
    } else {
        img.write_to(&mut buffer, image_format)
            .map_err(|e| drbot_core::Error::Internal(format!("Failed to encode: {}", e)))?;
    }

    Ok(buffer.into_inner())
}

/// Create a solid color image.
pub fn create_solid(width: u32, height: u32, r: u8, g: u8, b: u8) -> Vec<u8> {
    let img = DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
        width,
        height,
        image::Rgb([r, g, b]),
    ));

    let mut buffer = Cursor::new(Vec::new());
    img.write_to(&mut buffer, ImageFormat::Png).unwrap();
    buffer.into_inner()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_image() -> Vec<u8> {
        create_solid(100, 100, 255, 0, 0)
    }

    #[test]
    fn test_load_image() {
        let data = create_test_image();
        let img = load_image(&data).unwrap();
        assert_eq!(img.dimensions(), (100, 100));
    }

    #[test]
    fn test_get_info() {
        let data = create_test_image();
        let info = get_info(&data).unwrap();
        assert_eq!(info.width, 100);
        assert_eq!(info.height, 100);
        assert_eq!(info.media_type, MediaType::Png);
    }

    #[test]
    fn test_resize_fit() {
        let data = create_test_image();
        let options = ResizeOptions::fit(50, 50);
        let resized = resize(&data, &options).unwrap();

        let info = get_info(&resized).unwrap();
        assert!(info.width <= 50);
        assert!(info.height <= 50);
    }

    #[test]
    fn test_resize_exact() {
        let data = create_test_image();
        let options = ResizeOptions::exact(80, 60);
        let resized = resize(&data, &options).unwrap();

        let info = get_info(&resized).unwrap();
        assert_eq!(info.width, 80);
        assert_eq!(info.height, 60);
    }

    #[test]
    fn test_thumbnail() {
        let data = create_test_image();
        let options = ThumbnailOptions::default();
        let thumb = thumbnail(&data, &options).unwrap();

        let info = get_info(&thumb).unwrap();
        assert!(info.width <= 256);
        assert!(info.height <= 256);
    }

    #[test]
    fn test_convert() {
        let data = create_test_image();
        let converted = convert(&data, MediaType::Jpeg).unwrap();

        let media_type = MediaType::from_bytes(&converted);
        assert_eq!(media_type, MediaType::Jpeg);
    }

    #[test]
    fn test_calculate_dimensions() {
        // Fit mode
        let (w, h) = calculate_dimensions(200, 100, Some(100), Some(100), ResizeMode::Fit);
        assert_eq!((w, h), (100, 50));

        // Width mode
        let (w, h) = calculate_dimensions(200, 100, Some(100), None, ResizeMode::Width);
        assert_eq!((w, h), (100, 50));

        // Height mode
        let (w, h) = calculate_dimensions(200, 100, None, Some(50), ResizeMode::Height);
        assert_eq!((w, h), (100, 50));
    }
}
