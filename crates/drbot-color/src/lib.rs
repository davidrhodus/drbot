//! Color utilities and terminal colors for drbot.
//!
//! This crate provides:
//! - RGB/HSL color types
//! - ANSI terminal colors
//! - Color conversion
//! - Styled text output

use std::fmt;
use thiserror::Error;

/// Color error types.
#[derive(Error, Debug)]
pub enum ColorError {
    #[error("Invalid hex color: {0}")]
    InvalidHex(String),

    #[error("Invalid RGB value")]
    InvalidRgb,

    #[error("Invalid HSL value")]
    InvalidHsl,
}

/// Result type for color operations.
pub type Result<T> = std::result::Result<T, ColorError>;

/// RGB color.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    /// Create RGB color.
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// Create from hex string (e.g., "#ff0000" or "ff0000").
    pub fn from_hex(hex: &str) -> Result<Self> {
        let hex = hex.trim_start_matches('#');
        if hex.len() != 6 {
            return Err(ColorError::InvalidHex(hex.to_string()));
        }

        let r = u8::from_str_radix(&hex[0..2], 16)
            .map_err(|_| ColorError::InvalidHex(hex.to_string()))?;
        let g = u8::from_str_radix(&hex[2..4], 16)
            .map_err(|_| ColorError::InvalidHex(hex.to_string()))?;
        let b = u8::from_str_radix(&hex[4..6], 16)
            .map_err(|_| ColorError::InvalidHex(hex.to_string()))?;

        Ok(Self { r, g, b })
    }

    /// Convert to hex string.
    pub fn to_hex(&self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }

    /// Convert to HSL.
    pub fn to_hsl(&self) -> Hsl {
        let r = self.r as f64 / 255.0;
        let g = self.g as f64 / 255.0;
        let b = self.b as f64 / 255.0;

        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let l = (max + min) / 2.0;

        if (max - min).abs() < f64::EPSILON {
            return Hsl { h: 0.0, s: 0.0, l };
        }

        let d = max - min;
        let s = if l > 0.5 {
            d / (2.0 - max - min)
        } else {
            d / (max + min)
        };

        let h = if (max - r).abs() < f64::EPSILON {
            ((g - b) / d + if g < b { 6.0 } else { 0.0 }) / 6.0
        } else if (max - g).abs() < f64::EPSILON {
            ((b - r) / d + 2.0) / 6.0
        } else {
            ((r - g) / d + 4.0) / 6.0
        };

        Hsl { h, s, l }
    }

    /// Lighten color.
    pub fn lighten(&self, amount: f64) -> Self {
        let mut hsl = self.to_hsl();
        hsl.l = (hsl.l + amount).clamp(0.0, 1.0);
        hsl.to_rgb()
    }

    /// Darken color.
    pub fn darken(&self, amount: f64) -> Self {
        let mut hsl = self.to_hsl();
        hsl.l = (hsl.l - amount).clamp(0.0, 1.0);
        hsl.to_rgb()
    }

    /// Mix with another color.
    pub fn mix(&self, other: &Rgb, weight: f64) -> Self {
        let w = weight.clamp(0.0, 1.0);
        Self {
            r: ((self.r as f64 * (1.0 - w) + other.r as f64 * w).round() as u8),
            g: ((self.g as f64 * (1.0 - w) + other.g as f64 * w).round() as u8),
            b: ((self.b as f64 * (1.0 - w) + other.b as f64 * w).round() as u8),
        }
    }

    /// Calculate luminance.
    pub fn luminance(&self) -> f64 {
        let r = self.r as f64 / 255.0;
        let g = self.g as f64 / 255.0;
        let b = self.b as f64 / 255.0;
        0.2126 * r + 0.7152 * g + 0.0722 * b
    }

    /// Calculate contrast ratio with another color.
    pub fn contrast(&self, other: &Rgb) -> f64 {
        let l1 = self.luminance() + 0.05;
        let l2 = other.luminance() + 0.05;
        if l1 > l2 {
            l1 / l2
        } else {
            l2 / l1
        }
    }

    // Predefined colors
    pub const BLACK: Self = Self::new(0, 0, 0);
    pub const WHITE: Self = Self::new(255, 255, 255);
    pub const RED: Self = Self::new(255, 0, 0);
    pub const GREEN: Self = Self::new(0, 255, 0);
    pub const BLUE: Self = Self::new(0, 0, 255);
    pub const YELLOW: Self = Self::new(255, 255, 0);
    pub const CYAN: Self = Self::new(0, 255, 255);
    pub const MAGENTA: Self = Self::new(255, 0, 255);
}

impl fmt::Display for Rgb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "rgb({}, {}, {})", self.r, self.g, self.b)
    }
}

/// HSL color.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hsl {
    /// Hue (0-1).
    pub h: f64,
    /// Saturation (0-1).
    pub s: f64,
    /// Lightness (0-1).
    pub l: f64,
}

impl Hsl {
    /// Create HSL color.
    pub fn new(h: f64, s: f64, l: f64) -> Self {
        Self {
            h: h.clamp(0.0, 1.0),
            s: s.clamp(0.0, 1.0),
            l: l.clamp(0.0, 1.0),
        }
    }

    /// Convert to RGB.
    pub fn to_rgb(&self) -> Rgb {
        if self.s.abs() < f64::EPSILON {
            let v = (self.l * 255.0).round() as u8;
            return Rgb::new(v, v, v);
        }

        let q = if self.l < 0.5 {
            self.l * (1.0 + self.s)
        } else {
            self.l + self.s - self.l * self.s
        };
        let p = 2.0 * self.l - q;

        let r = Self::hue_to_rgb(p, q, self.h + 1.0 / 3.0);
        let g = Self::hue_to_rgb(p, q, self.h);
        let b = Self::hue_to_rgb(p, q, self.h - 1.0 / 3.0);

        Rgb::new(
            (r * 255.0).round() as u8,
            (g * 255.0).round() as u8,
            (b * 255.0).round() as u8,
        )
    }

    fn hue_to_rgb(p: f64, q: f64, mut t: f64) -> f64 {
        if t < 0.0 {
            t += 1.0;
        }
        if t > 1.0 {
            t -= 1.0;
        }

        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 1.0 / 2.0 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    }
}

/// ANSI color codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnsiColor {
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    BrightBlack,
    BrightRed,
    BrightGreen,
    BrightYellow,
    BrightBlue,
    BrightMagenta,
    BrightCyan,
    BrightWhite,
    Rgb(u8, u8, u8),
    Indexed(u8),
}

impl AnsiColor {
    /// Get ANSI foreground code.
    pub fn fg_code(&self) -> String {
        match self {
            AnsiColor::Black => "30".to_string(),
            AnsiColor::Red => "31".to_string(),
            AnsiColor::Green => "32".to_string(),
            AnsiColor::Yellow => "33".to_string(),
            AnsiColor::Blue => "34".to_string(),
            AnsiColor::Magenta => "35".to_string(),
            AnsiColor::Cyan => "36".to_string(),
            AnsiColor::White => "37".to_string(),
            AnsiColor::BrightBlack => "90".to_string(),
            AnsiColor::BrightRed => "91".to_string(),
            AnsiColor::BrightGreen => "92".to_string(),
            AnsiColor::BrightYellow => "93".to_string(),
            AnsiColor::BrightBlue => "94".to_string(),
            AnsiColor::BrightMagenta => "95".to_string(),
            AnsiColor::BrightCyan => "96".to_string(),
            AnsiColor::BrightWhite => "97".to_string(),
            AnsiColor::Rgb(r, g, b) => format!("38;2;{};{};{}", r, g, b),
            AnsiColor::Indexed(i) => format!("38;5;{}", i),
        }
    }

    /// Get ANSI background code.
    pub fn bg_code(&self) -> String {
        match self {
            AnsiColor::Black => "40".to_string(),
            AnsiColor::Red => "41".to_string(),
            AnsiColor::Green => "42".to_string(),
            AnsiColor::Yellow => "43".to_string(),
            AnsiColor::Blue => "44".to_string(),
            AnsiColor::Magenta => "45".to_string(),
            AnsiColor::Cyan => "46".to_string(),
            AnsiColor::White => "47".to_string(),
            AnsiColor::BrightBlack => "100".to_string(),
            AnsiColor::BrightRed => "101".to_string(),
            AnsiColor::BrightGreen => "102".to_string(),
            AnsiColor::BrightYellow => "103".to_string(),
            AnsiColor::BrightBlue => "104".to_string(),
            AnsiColor::BrightMagenta => "105".to_string(),
            AnsiColor::BrightCyan => "106".to_string(),
            AnsiColor::BrightWhite => "107".to_string(),
            AnsiColor::Rgb(r, g, b) => format!("48;2;{};{};{}", r, g, b),
            AnsiColor::Indexed(i) => format!("48;5;{}", i),
        }
    }

    /// Create from RGB.
    pub fn from_rgb(rgb: Rgb) -> Self {
        AnsiColor::Rgb(rgb.r, rgb.g, rgb.b)
    }
}

/// Text style.
#[derive(Debug, Clone, Default)]
pub struct Style {
    /// Foreground color.
    pub fg: Option<AnsiColor>,
    /// Background color.
    pub bg: Option<AnsiColor>,
    /// Bold.
    pub bold: bool,
    /// Italic.
    pub italic: bool,
    /// Underline.
    pub underline: bool,
    /// Strikethrough.
    pub strikethrough: bool,
    /// Dim.
    pub dim: bool,
}

impl Style {
    /// Create new style.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set foreground color.
    pub fn fg(mut self, color: AnsiColor) -> Self {
        self.fg = Some(color);
        self
    }

    /// Set background color.
    pub fn bg(mut self, color: AnsiColor) -> Self {
        self.bg = Some(color);
        self
    }

    /// Set bold.
    pub fn bold(mut self) -> Self {
        self.bold = true;
        self
    }

    /// Set italic.
    pub fn italic(mut self) -> Self {
        self.italic = true;
        self
    }

    /// Set underline.
    pub fn underline(mut self) -> Self {
        self.underline = true;
        self
    }

    /// Set strikethrough.
    pub fn strikethrough(mut self) -> Self {
        self.strikethrough = true;
        self
    }

    /// Set dim.
    pub fn dim(mut self) -> Self {
        self.dim = true;
        self
    }

    /// Generate ANSI escape sequence.
    pub fn to_ansi(&self) -> String {
        let mut codes = Vec::new();

        if self.bold {
            codes.push("1".to_string());
        }
        if self.dim {
            codes.push("2".to_string());
        }
        if self.italic {
            codes.push("3".to_string());
        }
        if self.underline {
            codes.push("4".to_string());
        }
        if self.strikethrough {
            codes.push("9".to_string());
        }

        if let Some(ref fg) = self.fg {
            codes.push(fg.fg_code());
        }
        if let Some(ref bg) = self.bg {
            codes.push(bg.bg_code());
        }

        if codes.is_empty() {
            String::new()
        } else {
            format!("\x1b[{}m", codes.join(";"))
        }
    }

    /// Apply style to text.
    pub fn apply(&self, text: &str) -> String {
        let start = self.to_ansi();
        if start.is_empty() {
            text.to_string()
        } else {
            format!("{}{}\x1b[0m", start, text)
        }
    }
}

/// Styled text.
pub struct StyledText {
    text: String,
    style: Style,
}

impl StyledText {
    /// Create styled text.
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            style: Style::new(),
        }
    }

    /// Set foreground color.
    pub fn fg(mut self, color: AnsiColor) -> Self {
        self.style = self.style.fg(color);
        self
    }

    /// Set background color.
    pub fn bg(mut self, color: AnsiColor) -> Self {
        self.style = self.style.bg(color);
        self
    }

    /// Set bold.
    pub fn bold(mut self) -> Self {
        self.style = self.style.bold();
        self
    }

    /// Set italic.
    pub fn italic(mut self) -> Self {
        self.style = self.style.italic();
        self
    }

    /// Set underline.
    pub fn underline(mut self) -> Self {
        self.style = self.style.underline();
        self
    }
}

impl fmt::Display for StyledText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.style.apply(&self.text))
    }
}

/// Helper function to style text.
pub fn styled(text: impl Into<String>) -> StyledText {
    StyledText::new(text)
}

/// Color shortcuts.
pub fn red(text: &str) -> String {
    Style::new().fg(AnsiColor::Red).apply(text)
}

pub fn green(text: &str) -> String {
    Style::new().fg(AnsiColor::Green).apply(text)
}

pub fn yellow(text: &str) -> String {
    Style::new().fg(AnsiColor::Yellow).apply(text)
}

pub fn blue(text: &str) -> String {
    Style::new().fg(AnsiColor::Blue).apply(text)
}

pub fn bold(text: &str) -> String {
    Style::new().bold().apply(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rgb_from_hex() {
        let rgb = Rgb::from_hex("#ff0000").unwrap();
        assert_eq!(rgb.r, 255);
        assert_eq!(rgb.g, 0);
        assert_eq!(rgb.b, 0);
    }

    #[test]
    fn test_rgb_to_hex() {
        let rgb = Rgb::new(255, 128, 64);
        assert_eq!(rgb.to_hex(), "#ff8040");
    }

    #[test]
    fn test_rgb_hsl_roundtrip() {
        let rgb = Rgb::new(128, 64, 192);
        let hsl = rgb.to_hsl();
        let back = hsl.to_rgb();

        assert!((rgb.r as i16 - back.r as i16).abs() <= 1);
        assert!((rgb.g as i16 - back.g as i16).abs() <= 1);
        assert!((rgb.b as i16 - back.b as i16).abs() <= 1);
    }

    #[test]
    fn test_luminance() {
        assert!(Rgb::WHITE.luminance() > 0.9);
        assert!(Rgb::BLACK.luminance() < 0.1);
    }

    #[test]
    fn test_contrast() {
        let contrast = Rgb::WHITE.contrast(&Rgb::BLACK);
        assert!(contrast > 20.0);
    }

    #[test]
    fn test_style() {
        let style = Style::new().fg(AnsiColor::Red).bold();
        let text = style.apply("hello");
        assert!(text.contains("\x1b["));
        assert!(text.contains("1;")); // bold
        assert!(text.contains("31")); // red
    }

    #[test]
    fn test_styled_text() {
        let text = styled("hello").fg(AnsiColor::Green).bold().to_string();
        assert!(text.contains("\x1b["));
    }
}
