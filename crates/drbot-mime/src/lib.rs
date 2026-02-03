//! MIME type handling for drbot.
//!
//! This crate provides:
//! - MIME type detection
//! - Extension mapping
//! - Content type parsing

use std::collections::HashMap;
use std::path::Path;
use std::str::FromStr;
use thiserror::Error;

/// MIME error types.
#[derive(Error, Debug)]
pub enum MimeError {
    #[error("Invalid MIME type: {0}")]
    Invalid(String),

    #[error("Unknown extension: {0}")]
    UnknownExtension(String),
}

/// Result type for MIME operations.
pub type Result<T> = std::result::Result<T, MimeError>;

/// MIME type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MimeType {
    /// Top-level type (e.g., "text", "application").
    pub type_: String,
    /// Subtype (e.g., "plain", "json").
    pub subtype: String,
    /// Suffix (e.g., "xml" in "application/atom+xml").
    pub suffix: Option<String>,
    /// Parameters (e.g., charset=utf-8).
    pub params: HashMap<String, String>,
}

impl MimeType {
    /// Create new MIME type.
    pub fn new(type_: &str, subtype: &str) -> Self {
        Self {
            type_: type_.to_lowercase(),
            subtype: subtype.to_lowercase(),
            suffix: None,
            params: HashMap::new(),
        }
    }

    /// Parse MIME type from string.
    pub fn parse(s: &str) -> Result<Self> {
        let s = s.trim();

        // Split off parameters
        let (main, params_str) = if let Some(pos) = s.find(';') {
            (&s[..pos], Some(&s[pos + 1..]))
        } else {
            (s, None)
        };

        // Parse type/subtype
        let parts: Vec<&str> = main.trim().splitn(2, '/').collect();
        if parts.len() != 2 {
            return Err(MimeError::Invalid(s.to_string()));
        }

        let type_ = parts[0].trim().to_lowercase();
        let subtype_full = parts[1].trim().to_lowercase();

        // Check for suffix (e.g., xml in application/atom+xml)
        let (subtype, suffix) = if let Some(pos) = subtype_full.rfind('+') {
            (
                subtype_full[..pos].to_string(),
                Some(subtype_full[pos + 1..].to_string()),
            )
        } else {
            (subtype_full, None)
        };

        // Parse parameters
        let mut params = HashMap::new();
        if let Some(ps) = params_str {
            for param in ps.split(';') {
                let param = param.trim();
                if let Some(pos) = param.find('=') {
                    let key = param[..pos].trim().to_lowercase();
                    let value = param[pos + 1..].trim().trim_matches('"').to_string();
                    params.insert(key, value);
                }
            }
        }

        Ok(Self {
            type_,
            subtype,
            suffix,
            params,
        })
    }

    /// Get the essence (type/subtype without parameters).
    pub fn essence(&self) -> String {
        if let Some(ref suffix) = self.suffix {
            format!("{}/{}+{}", self.type_, self.subtype, suffix)
        } else {
            format!("{}/{}", self.type_, self.subtype)
        }
    }

    /// Get charset parameter.
    pub fn charset(&self) -> Option<&str> {
        self.params.get("charset").map(|s| s.as_str())
    }

    /// Set charset parameter.
    pub fn with_charset(mut self, charset: &str) -> Self {
        self.params
            .insert("charset".to_string(), charset.to_string());
        self
    }

    /// Check if this is a text type.
    pub fn is_text(&self) -> bool {
        self.type_ == "text"
    }

    /// Check if this is an image type.
    pub fn is_image(&self) -> bool {
        self.type_ == "image"
    }

    /// Check if this is an audio type.
    pub fn is_audio(&self) -> bool {
        self.type_ == "audio"
    }

    /// Check if this is a video type.
    pub fn is_video(&self) -> bool {
        self.type_ == "video"
    }

    /// Check if this is an application type.
    pub fn is_application(&self) -> bool {
        self.type_ == "application"
    }

    /// Check if this is a JSON type.
    pub fn is_json(&self) -> bool {
        self.subtype == "json" || self.suffix.as_deref() == Some("json")
    }

    /// Check if this is an XML type.
    pub fn is_xml(&self) -> bool {
        self.subtype == "xml" || self.suffix.as_deref() == Some("xml")
    }

    /// Convert to string with parameters.
    pub fn to_string(&self) -> String {
        let mut s = self.essence();
        for (key, value) in &self.params {
            s.push_str(&format!("; {}={}", key, value));
        }
        s
    }
}

impl FromStr for MimeType {
    type Err = MimeError;

    fn from_str(s: &str) -> Result<Self> {
        Self::parse(s)
    }
}

impl std::fmt::Display for MimeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_string())
    }
}

/// Well-known MIME types.
pub struct Mime;

impl Mime {
    // Text types
    pub const TEXT_PLAIN: &'static str = "text/plain";
    pub const TEXT_HTML: &'static str = "text/html";
    pub const TEXT_CSS: &'static str = "text/css";
    pub const TEXT_JAVASCRIPT: &'static str = "text/javascript";
    pub const TEXT_CSV: &'static str = "text/csv";
    pub const TEXT_XML: &'static str = "text/xml";
    pub const TEXT_MARKDOWN: &'static str = "text/markdown";

    // Application types
    pub const APPLICATION_JSON: &'static str = "application/json";
    pub const APPLICATION_XML: &'static str = "application/xml";
    pub const APPLICATION_PDF: &'static str = "application/pdf";
    pub const APPLICATION_ZIP: &'static str = "application/zip";
    pub const APPLICATION_GZIP: &'static str = "application/gzip";
    pub const APPLICATION_OCTET_STREAM: &'static str = "application/octet-stream";
    pub const APPLICATION_FORM_URLENCODED: &'static str = "application/x-www-form-urlencoded";
    pub const APPLICATION_JAVASCRIPT: &'static str = "application/javascript";

    // Multipart types
    pub const MULTIPART_FORM_DATA: &'static str = "multipart/form-data";

    // Image types
    pub const IMAGE_PNG: &'static str = "image/png";
    pub const IMAGE_JPEG: &'static str = "image/jpeg";
    pub const IMAGE_GIF: &'static str = "image/gif";
    pub const IMAGE_WEBP: &'static str = "image/webp";
    pub const IMAGE_SVG: &'static str = "image/svg+xml";

    // Audio types
    pub const AUDIO_MP3: &'static str = "audio/mpeg";
    pub const AUDIO_WAV: &'static str = "audio/wav";
    pub const AUDIO_OGG: &'static str = "audio/ogg";

    // Video types
    pub const VIDEO_MP4: &'static str = "video/mp4";
    pub const VIDEO_WEBM: &'static str = "video/webm";
    pub const VIDEO_OGG: &'static str = "video/ogg";

    /// Get MIME type from extension.
    pub fn from_extension(ext: &str) -> Option<&'static str> {
        let ext = ext.trim_start_matches('.').to_lowercase();
        match ext.as_str() {
            // Text
            "txt" => Some(Self::TEXT_PLAIN),
            "html" | "htm" => Some(Self::TEXT_HTML),
            "css" => Some(Self::TEXT_CSS),
            "js" | "mjs" => Some(Self::TEXT_JAVASCRIPT),
            "csv" => Some(Self::TEXT_CSV),
            "xml" => Some(Self::TEXT_XML),
            "md" | "markdown" => Some(Self::TEXT_MARKDOWN),

            // Application
            "json" => Some(Self::APPLICATION_JSON),
            "pdf" => Some(Self::APPLICATION_PDF),
            "zip" => Some(Self::APPLICATION_ZIP),
            "gz" | "gzip" => Some(Self::APPLICATION_GZIP),
            "tar" => Some("application/x-tar"),
            "wasm" => Some("application/wasm"),

            // Image
            "png" => Some(Self::IMAGE_PNG),
            "jpg" | "jpeg" => Some(Self::IMAGE_JPEG),
            "gif" => Some(Self::IMAGE_GIF),
            "webp" => Some(Self::IMAGE_WEBP),
            "svg" => Some(Self::IMAGE_SVG),
            "ico" => Some("image/x-icon"),
            "bmp" => Some("image/bmp"),

            // Audio
            "mp3" => Some(Self::AUDIO_MP3),
            "wav" => Some(Self::AUDIO_WAV),
            "ogg" => Some(Self::AUDIO_OGG),
            "flac" => Some("audio/flac"),
            "aac" => Some("audio/aac"),

            // Video
            "mp4" | "m4v" => Some(Self::VIDEO_MP4),
            "webm" => Some(Self::VIDEO_WEBM),
            "ogv" => Some(Self::VIDEO_OGG),
            "avi" => Some("video/x-msvideo"),
            "mov" => Some("video/quicktime"),

            // Fonts
            "woff" => Some("font/woff"),
            "woff2" => Some("font/woff2"),
            "ttf" => Some("font/ttf"),
            "otf" => Some("font/otf"),
            "eot" => Some("application/vnd.ms-fontobject"),

            // Other
            "yaml" | "yml" => Some("application/x-yaml"),
            "toml" => Some("application/toml"),
            "rs" => Some("text/x-rust"),
            "py" => Some("text/x-python"),
            "rb" => Some("text/x-ruby"),
            "java" => Some("text/x-java"),
            "c" => Some("text/x-c"),
            "cpp" | "cc" | "cxx" => Some("text/x-c++"),
            "h" => Some("text/x-c-header"),
            "hpp" => Some("text/x-c++-header"),
            "go" => Some("text/x-go"),
            "sh" => Some("application/x-sh"),
            "sql" => Some("application/sql"),

            _ => None,
        }
    }

    /// Get extension from MIME type.
    pub fn to_extension(mime: &str) -> Option<&'static str> {
        match mime {
            Self::TEXT_PLAIN => Some("txt"),
            Self::TEXT_HTML => Some("html"),
            Self::TEXT_CSS => Some("css"),
            Self::TEXT_JAVASCRIPT | Self::APPLICATION_JAVASCRIPT => Some("js"),
            Self::TEXT_CSV => Some("csv"),
            Self::TEXT_XML | Self::APPLICATION_XML => Some("xml"),
            Self::TEXT_MARKDOWN => Some("md"),
            Self::APPLICATION_JSON => Some("json"),
            Self::APPLICATION_PDF => Some("pdf"),
            Self::APPLICATION_ZIP => Some("zip"),
            Self::APPLICATION_GZIP => Some("gz"),
            Self::IMAGE_PNG => Some("png"),
            Self::IMAGE_JPEG => Some("jpg"),
            Self::IMAGE_GIF => Some("gif"),
            Self::IMAGE_WEBP => Some("webp"),
            Self::IMAGE_SVG => Some("svg"),
            Self::AUDIO_MP3 => Some("mp3"),
            Self::AUDIO_WAV => Some("wav"),
            Self::AUDIO_OGG => Some("ogg"),
            Self::VIDEO_MP4 => Some("mp4"),
            Self::VIDEO_WEBM => Some("webm"),
            _ => None,
        }
    }

    /// Get MIME type from path.
    pub fn from_path(path: &Path) -> Option<&'static str> {
        path.extension()
            .and_then(|ext| ext.to_str())
            .and_then(Self::from_extension)
    }

    /// Get MIME type from filename.
    pub fn from_filename(filename: &str) -> Option<&'static str> {
        Self::from_path(Path::new(filename))
    }

    /// Guess MIME type from content (magic bytes).
    pub fn from_bytes(bytes: &[u8]) -> Option<&'static str> {
        if bytes.len() < 4 {
            return None;
        }

        // PNG
        if bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
            return Some(Self::IMAGE_PNG);
        }

        // JPEG
        if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
            return Some(Self::IMAGE_JPEG);
        }

        // GIF
        if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
            return Some(Self::IMAGE_GIF);
        }

        // WebP
        if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
            return Some(Self::IMAGE_WEBP);
        }

        // PDF
        if bytes.starts_with(b"%PDF") {
            return Some(Self::APPLICATION_PDF);
        }

        // ZIP
        if bytes.starts_with(&[0x50, 0x4B, 0x03, 0x04]) {
            return Some(Self::APPLICATION_ZIP);
        }

        // GZIP
        if bytes.starts_with(&[0x1F, 0x8B]) {
            return Some(Self::APPLICATION_GZIP);
        }

        // MP3
        if bytes.starts_with(&[0xFF, 0xFB])
            || bytes.starts_with(&[0xFF, 0xFA])
            || bytes.starts_with(b"ID3")
        {
            return Some(Self::AUDIO_MP3);
        }

        // WAV
        if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WAVE" {
            return Some(Self::AUDIO_WAV);
        }

        // MP4
        if bytes.len() >= 8
            && (&bytes[4..8] == b"ftyp" || &bytes[4..8] == b"mdat" || &bytes[4..8] == b"moov")
        {
            return Some(Self::VIDEO_MP4);
        }

        // WebM (starts with EBML)
        if bytes.starts_with(&[0x1A, 0x45, 0xDF, 0xA3]) {
            return Some(Self::VIDEO_WEBM);
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple() {
        let mime = MimeType::parse("text/plain").unwrap();
        assert_eq!(mime.type_, "text");
        assert_eq!(mime.subtype, "plain");
        assert!(mime.params.is_empty());
    }

    #[test]
    fn test_parse_with_params() {
        let mime = MimeType::parse("text/html; charset=utf-8").unwrap();
        assert_eq!(mime.type_, "text");
        assert_eq!(mime.subtype, "html");
        assert_eq!(mime.charset(), Some("utf-8"));
    }

    #[test]
    fn test_parse_with_suffix() {
        let mime = MimeType::parse("application/atom+xml").unwrap();
        assert_eq!(mime.type_, "application");
        assert_eq!(mime.subtype, "atom");
        assert_eq!(mime.suffix, Some("xml".to_string()));
        assert!(mime.is_xml());
    }

    #[test]
    fn test_from_extension() {
        assert_eq!(Mime::from_extension("html"), Some(Mime::TEXT_HTML));
        assert_eq!(Mime::from_extension(".json"), Some(Mime::APPLICATION_JSON));
        assert_eq!(Mime::from_extension("PNG"), Some(Mime::IMAGE_PNG));
    }

    #[test]
    fn test_from_path() {
        assert_eq!(
            Mime::from_path(Path::new("file.html")),
            Some(Mime::TEXT_HTML)
        );
        assert_eq!(
            Mime::from_path(Path::new("/path/to/data.json")),
            Some(Mime::APPLICATION_JSON)
        );
    }

    #[test]
    fn test_from_bytes() {
        // PNG magic bytes
        assert_eq!(
            Mime::from_bytes(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]),
            Some(Mime::IMAGE_PNG)
        );

        // JPEG magic bytes
        assert_eq!(
            Mime::from_bytes(&[0xFF, 0xD8, 0xFF, 0xE0]),
            Some(Mime::IMAGE_JPEG)
        );

        // PDF magic bytes
        assert_eq!(Mime::from_bytes(b"%PDF-1.4"), Some(Mime::APPLICATION_PDF));
    }

    #[test]
    fn test_is_methods() {
        let mime = MimeType::parse("text/plain").unwrap();
        assert!(mime.is_text());
        assert!(!mime.is_image());

        let mime = MimeType::parse("image/png").unwrap();
        assert!(mime.is_image());
        assert!(!mime.is_text());

        let mime = MimeType::parse("application/json").unwrap();
        assert!(mime.is_json());
    }

    #[test]
    fn test_to_string() {
        let mime = MimeType::new("text", "html").with_charset("utf-8");
        assert!(mime.to_string().contains("text/html"));
        assert!(mime.to_string().contains("charset=utf-8"));
    }
}
