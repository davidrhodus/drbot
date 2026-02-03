//! Screenshot capture functionality.

use crate::{Result, ScreenError};
use serde::{Deserialize, Serialize};

/// Screenshot data.
#[derive(Debug, Clone)]
pub struct Screenshot {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// PNG-encoded image data.
    pub data: Vec<u8>,
    /// Screen scale factor.
    pub scale: f32,
}

impl Screenshot {
    /// Create a new screenshot from raw data.
    pub fn new(width: u32, height: u32, data: Vec<u8>) -> Self {
        Self {
            width,
            height,
            data,
            scale: 1.0,
        }
    }

    /// Set the scale factor.
    pub fn with_scale(mut self, scale: f32) -> Self {
        self.scale = scale;
        self
    }

    /// Get the size in bytes.
    pub fn size_bytes(&self) -> usize {
        self.data.len()
    }

    /// Save to a file.
    pub fn save(&self, path: &str) -> Result<()> {
        std::fs::write(path, &self.data).map_err(|e| ScreenError::ScreenshotFailed(e.to_string()))
    }

    /// Convert to base64.
    pub fn to_base64(&self) -> String {
        use std::io::Write;
        let mut encoder = base64_encoder();
        encoder.write_all(&self.data).unwrap();
        String::from_utf8(encoder.into_inner()).unwrap()
    }
}

fn base64_encoder() -> std::io::Cursor<Vec<u8>> {
    // Simple base64 encoding would go here
    // For now, return a cursor for the data
    std::io::Cursor::new(Vec::new())
}

/// Screenshot capture options.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureOptions {
    /// Capture only the active display.
    pub active_display_only: bool,
    /// Include the cursor in the capture.
    pub include_cursor: bool,
    /// Capture region (x, y, width, height).
    pub region: Option<CaptureRegion>,
    /// Scale factor (1.0 = native, 0.5 = half size).
    pub scale: f32,
    /// Output format.
    pub format: CaptureFormat,
}

impl Default for CaptureOptions {
    fn default() -> Self {
        Self {
            active_display_only: true,
            include_cursor: false,
            region: None,
            scale: 1.0,
            format: CaptureFormat::Png,
        }
    }
}

/// Capture region.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CaptureRegion {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// Capture format.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum CaptureFormat {
    Png,
    Jpeg,
}

/// Capture the screen.
pub async fn capture_screen(options: CaptureOptions) -> Result<Screenshot> {
    #[cfg(target_os = "macos")]
    {
        capture_screen_macos(options).await
    }

    #[cfg(not(target_os = "macos"))]
    {
        Err(ScreenError::PlatformNotSupported)
    }
}

/// Capture a specific window.
pub async fn capture_window(window_id: u32) -> Result<Screenshot> {
    #[cfg(target_os = "macos")]
    {
        capture_window_macos(window_id).await
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = window_id;
        Err(ScreenError::PlatformNotSupported)
    }
}

#[cfg(target_os = "macos")]
async fn capture_screen_macos(options: CaptureOptions) -> Result<Screenshot> {
    use std::process::Command;

    // Use screencapture command for simplicity
    // In production, would use Core Graphics directly
    let temp_path = format!("/tmp/drbot_screenshot_{}.png", std::process::id());

    let mut cmd = Command::new("screencapture");
    cmd.arg("-x"); // No sound
    cmd.arg("-t").arg("png");

    if options.active_display_only {
        cmd.arg("-m"); // Main display only
    }

    if let Some(region) = options.region {
        cmd.arg("-R").arg(format!(
            "{},{},{},{}",
            region.x, region.y, region.width, region.height
        ));
    }

    cmd.arg(&temp_path);

    let status = cmd
        .status()
        .map_err(|e| ScreenError::ScreenshotFailed(e.to_string()))?;

    if !status.success() {
        return Err(ScreenError::ScreenshotFailed(
            "screencapture failed".to_string(),
        ));
    }

    // Read the file
    let data =
        std::fs::read(&temp_path).map_err(|e| ScreenError::ScreenshotFailed(e.to_string()))?;

    // Clean up
    let _ = std::fs::remove_file(&temp_path);

    // Get dimensions from PNG header (simplified)
    let (width, height) = get_png_dimensions(&data).unwrap_or((0, 0));

    Ok(Screenshot::new(width, height, data).with_scale(options.scale))
}

#[cfg(target_os = "macos")]
async fn capture_window_macos(window_id: u32) -> Result<Screenshot> {
    use std::process::Command;

    let temp_path = format!("/tmp/drbot_window_{}.png", std::process::id());

    let status = Command::new("screencapture")
        .arg("-x")
        .arg("-t")
        .arg("png")
        .arg("-l")
        .arg(window_id.to_string())
        .arg(&temp_path)
        .status()
        .map_err(|e| ScreenError::ScreenshotFailed(e.to_string()))?;

    if !status.success() {
        return Err(ScreenError::ScreenshotFailed(
            "screencapture failed".to_string(),
        ));
    }

    let data =
        std::fs::read(&temp_path).map_err(|e| ScreenError::ScreenshotFailed(e.to_string()))?;

    let _ = std::fs::remove_file(&temp_path);

    let (width, height) = get_png_dimensions(&data).unwrap_or((0, 0));

    Ok(Screenshot::new(width, height, data))
}

/// Extract dimensions from PNG header.
fn get_png_dimensions(data: &[u8]) -> Option<(u32, u32)> {
    // PNG signature + IHDR chunk
    if data.len() < 24 {
        return None;
    }

    // Check PNG signature
    if &data[0..8] != b"\x89PNG\r\n\x1a\n" {
        return None;
    }

    // IHDR chunk starts at offset 8
    // Length (4) + Type (4) + Width (4) + Height (4)
    let width = u32::from_be_bytes([data[16], data[17], data[18], data[19]]);
    let height = u32::from_be_bytes([data[20], data[21], data[22], data[23]]);

    Some((width, height))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_options() {
        let opts = CaptureOptions::default();
        assert!(opts.active_display_only);
        assert!(!opts.include_cursor);
    }

    #[test]
    fn test_png_dimensions() {
        // Valid PNG header with 100x100 dimensions
        let mut data = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        // IHDR chunk length
        data.extend(&[0x00, 0x00, 0x00, 0x0D]);
        // IHDR type
        data.extend(b"IHDR");
        // Width: 100 (big-endian)
        data.extend(&[0x00, 0x00, 0x00, 0x64]);
        // Height: 100 (big-endian)
        data.extend(&[0x00, 0x00, 0x00, 0x64]);

        let (w, h) = get_png_dimensions(&data).unwrap();
        assert_eq!(w, 100);
        assert_eq!(h, 100);
    }
}
