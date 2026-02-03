//! Log formatting utilities for drbot.
//!
//! This crate provides:
//! - Log line formatters
//! - Timestamp formatting
//! - Color support

use std::fmt::Write;
use thiserror::Error;

/// Format error types.
#[derive(Error, Debug, Clone)]
pub enum FormatError {
    #[error("Format error: {0}")]
    Error(String),

    #[error("Write error")]
    WriteError,
}

/// Result type for format operations.
pub type Result<T> = std::result::Result<T, FormatError>;

/// Log level for formatting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl Level {
    /// Get level name.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Trace => "TRACE",
            Self::Debug => "DEBUG",
            Self::Info => "INFO",
            Self::Warn => "WARN",
            Self::Error => "ERROR",
        }
    }

    /// Get short name.
    pub fn short_name(&self) -> &'static str {
        match self {
            Self::Trace => "TRC",
            Self::Debug => "DBG",
            Self::Info => "INF",
            Self::Warn => "WRN",
            Self::Error => "ERR",
        }
    }

    /// Get ANSI color code.
    pub fn color_code(&self) -> &'static str {
        match self {
            Self::Trace => "\x1b[90m", // Gray
            Self::Debug => "\x1b[36m", // Cyan
            Self::Info => "\x1b[32m",  // Green
            Self::Warn => "\x1b[33m",  // Yellow
            Self::Error => "\x1b[31m", // Red
        }
    }
}

/// Log record for formatting.
#[derive(Debug, Clone)]
pub struct LogRecord {
    pub timestamp: String,
    pub level: Level,
    pub target: String,
    pub message: String,
    pub fields: Vec<(String, String)>,
}

impl LogRecord {
    /// Create new log record.
    pub fn new(level: Level, message: impl Into<String>) -> Self {
        Self {
            timestamp: current_timestamp(),
            level,
            target: String::new(),
            message: message.into(),
            fields: Vec::new(),
        }
    }

    /// Set target.
    pub fn target(mut self, target: impl Into<String>) -> Self {
        self.target = target.into();
        self
    }

    /// Add field.
    pub fn field(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.fields.push((key.into(), value.into()));
        self
    }
}

/// Log formatter trait.
pub trait LogFormatter {
    /// Format a log record.
    fn format(&self, record: &LogRecord) -> Result<String>;
}

/// Simple text formatter.
pub struct TextFormatter {
    pub include_timestamp: bool,
    pub include_level: bool,
    pub include_target: bool,
    pub use_colors: bool,
}

impl Default for TextFormatter {
    fn default() -> Self {
        Self {
            include_timestamp: true,
            include_level: true,
            include_target: true,
            use_colors: false,
        }
    }
}

impl TextFormatter {
    /// Create new text formatter.
    pub fn new() -> Self {
        Self::default()
    }

    /// Enable colors.
    pub fn with_colors(mut self) -> Self {
        self.use_colors = true;
        self
    }

    /// Disable timestamp.
    pub fn without_timestamp(mut self) -> Self {
        self.include_timestamp = false;
        self
    }
}

impl LogFormatter for TextFormatter {
    fn format(&self, record: &LogRecord) -> Result<String> {
        let mut output = String::new();

        if self.include_timestamp {
            write!(output, "{} ", record.timestamp).map_err(|_| FormatError::WriteError)?;
        }

        if self.include_level {
            if self.use_colors {
                write!(
                    output,
                    "{}[{}]\x1b[0m ",
                    record.level.color_code(),
                    record.level.name()
                )
                .map_err(|_| FormatError::WriteError)?;
            } else {
                write!(output, "[{}] ", record.level.name())
                    .map_err(|_| FormatError::WriteError)?;
            }
        }

        if self.include_target && !record.target.is_empty() {
            write!(output, "{}: ", record.target).map_err(|_| FormatError::WriteError)?;
        }

        write!(output, "{}", record.message).map_err(|_| FormatError::WriteError)?;

        for (key, value) in &record.fields {
            write!(output, " {}={}", key, value).map_err(|_| FormatError::WriteError)?;
        }

        Ok(output)
    }
}

/// JSON formatter.
pub struct JsonFormatter {
    pub pretty: bool,
}

impl Default for JsonFormatter {
    fn default() -> Self {
        Self { pretty: false }
    }
}

impl JsonFormatter {
    /// Create new JSON formatter.
    pub fn new() -> Self {
        Self::default()
    }

    /// Enable pretty printing.
    pub fn pretty(mut self) -> Self {
        self.pretty = true;
        self
    }
}

impl LogFormatter for JsonFormatter {
    fn format(&self, record: &LogRecord) -> Result<String> {
        let mut output = String::from("{");

        output.push_str(&format!(
            "\"timestamp\":\"{}\"",
            escape_json(&record.timestamp)
        ));
        output.push_str(&format!(",\"level\":\"{}\"", record.level.name()));

        if !record.target.is_empty() {
            output.push_str(&format!(",\"target\":\"{}\"", escape_json(&record.target)));
        }

        output.push_str(&format!(
            ",\"message\":\"{}\"",
            escape_json(&record.message)
        ));

        if !record.fields.is_empty() {
            output.push_str(",\"fields\":{");
            let fields: Vec<String> = record
                .fields
                .iter()
                .map(|(k, v)| format!("\"{}\":\"{}\"", escape_json(k), escape_json(v)))
                .collect();
            output.push_str(&fields.join(","));
            output.push('}');
        }

        output.push('}');
        Ok(output)
    }
}

/// Compact formatter.
pub struct CompactFormatter;

impl LogFormatter for CompactFormatter {
    fn format(&self, record: &LogRecord) -> Result<String> {
        Ok(format!(
            "{} {} {}",
            record.level.short_name(),
            record.target,
            record.message
        ))
    }
}

/// Escape string for JSON.
fn escape_json(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            c if c.is_control() => result.push_str(&format!("\\u{:04x}", c as u32)),
            c => result.push(c),
        }
    }
    result
}

/// Get current timestamp.
fn current_timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();

    let secs = now.as_secs();
    let millis = now.subsec_millis();

    // Simple ISO-ish format without chrono
    format!("{}.{:03}", secs, millis)
}

/// Format timestamp from seconds.
pub fn format_timestamp(secs: u64) -> String {
    // Simple format - in production use chrono
    format!("{}", secs)
}

/// Format duration.
pub fn format_duration_ms(millis: u64) -> String {
    if millis < 1000 {
        format!("{}ms", millis)
    } else if millis < 60_000 {
        format!("{:.2}s", millis as f64 / 1000.0)
    } else if millis < 3_600_000 {
        format!("{:.2}m", millis as f64 / 60_000.0)
    } else {
        format!("{:.2}h", millis as f64 / 3_600_000.0)
    }
}

/// Format bytes size.
pub fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes < KB {
        format!("{}B", bytes)
    } else if bytes < MB {
        format!("{:.2}KB", bytes as f64 / KB as f64)
    } else if bytes < GB {
        format!("{:.2}MB", bytes as f64 / MB as f64)
    } else {
        format!("{:.2}GB", bytes as f64 / GB as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_level() {
        assert_eq!(Level::Info.name(), "INFO");
        assert_eq!(Level::Error.short_name(), "ERR");
        assert!(Level::Error > Level::Warn);
    }

    #[test]
    fn test_text_formatter() {
        let formatter = TextFormatter::new();
        let record = LogRecord::new(Level::Info, "Hello world")
            .target("test")
            .field("key", "value");

        let output = formatter.format(&record).unwrap();
        assert!(output.contains("[INFO]"));
        assert!(output.contains("Hello world"));
        assert!(output.contains("key=value"));
    }

    #[test]
    fn test_json_formatter() {
        let formatter = JsonFormatter::new();
        let record = LogRecord::new(Level::Error, "Something failed").target("app");

        let output = formatter.format(&record).unwrap();
        assert!(output.contains("\"level\":\"ERROR\""));
        assert!(output.contains("\"message\":\"Something failed\""));
    }

    #[test]
    fn test_escape_json() {
        assert_eq!(escape_json("hello"), "hello");
        assert_eq!(escape_json("hello\nworld"), "hello\\nworld");
        assert_eq!(escape_json("say \"hi\""), "say \\\"hi\\\"");
    }

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration_ms(500), "500ms");
        assert_eq!(format_duration_ms(1500), "1.50s");
        assert_eq!(format_duration_ms(90000), "1.50m");
    }

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(500), "500B");
        assert_eq!(format_bytes(2048), "2.00KB");
        assert_eq!(format_bytes(1048576), "1.00MB");
    }
}
