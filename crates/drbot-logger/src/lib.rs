//! Structured logging wrapper for drbot.
//!
//! This crate provides:
//! - Log levels and filtering
//! - Structured log output
//! - JSON logging support
//! - Log rotation helpers

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::sync::atomic::{AtomicUsize, Ordering};
use thiserror::Error;

/// Logger error types.
#[derive(Error, Debug)]
pub enum LoggerError {
    #[error("Invalid log level: {0}")]
    InvalidLevel(String),

    #[error("Logger not initialized")]
    NotInitialized,

    #[error("IO error: {0}")]
    Io(String),
}

/// Result type for logger operations.
pub type Result<T> = std::result::Result<T, LoggerError>;

/// Log level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Trace = 0,
    Debug = 1,
    Info = 2,
    Warn = 3,
    Error = 4,
}

impl Level {
    /// Parse level from string.
    pub fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "trace" => Ok(Level::Trace),
            "debug" => Ok(Level::Debug),
            "info" => Ok(Level::Info),
            "warn" | "warning" => Ok(Level::Warn),
            "error" => Ok(Level::Error),
            _ => Err(LoggerError::InvalidLevel(s.to_string())),
        }
    }

    /// Get level name.
    pub fn name(&self) -> &'static str {
        match self {
            Level::Trace => "trace",
            Level::Debug => "debug",
            Level::Info => "info",
            Level::Warn => "warn",
            Level::Error => "error",
        }
    }

    /// Get level as uppercase.
    pub fn upper_name(&self) -> &'static str {
        match self {
            Level::Trace => "TRACE",
            Level::Debug => "DEBUG",
            Level::Info => "INFO",
            Level::Warn => "WARN",
            Level::Error => "ERROR",
        }
    }
}

impl fmt::Display for Level {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.upper_name())
    }
}

/// Log record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogRecord {
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
    /// Log level.
    pub level: Level,
    /// Log message.
    pub message: String,
    /// Target module.
    pub target: Option<String>,
    /// File name.
    pub file: Option<String>,
    /// Line number.
    pub line: Option<u32>,
    /// Structured fields.
    pub fields: HashMap<String, serde_json::Value>,
}

impl LogRecord {
    /// Create new log record.
    pub fn new(level: Level, message: impl Into<String>) -> Self {
        Self {
            timestamp: Utc::now(),
            level,
            message: message.into(),
            target: None,
            file: None,
            line: None,
            fields: HashMap::new(),
        }
    }

    /// Set target.
    pub fn with_target(mut self, target: impl Into<String>) -> Self {
        self.target = Some(target.into());
        self
    }

    /// Set location.
    pub fn with_location(mut self, file: impl Into<String>, line: u32) -> Self {
        self.file = Some(file.into());
        self.line = Some(line);
        self
    }

    /// Add field.
    pub fn with_field(mut self, key: impl Into<String>, value: impl Serialize) -> Self {
        if let Ok(v) = serde_json::to_value(value) {
            self.fields.insert(key.into(), v);
        }
        self
    }

    /// Format as plain text.
    pub fn format_plain(&self) -> String {
        let ts = self.timestamp.format("%Y-%m-%d %H:%M:%S%.3f");
        let target = self.target.as_deref().unwrap_or("-");
        format!("{} {} [{}] {}", ts, self.level, target, self.message)
    }

    /// Format as JSON.
    pub fn format_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| self.format_plain())
    }
}

/// Log output format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Plain,
    Json,
    Pretty,
}

/// Logger configuration.
#[derive(Debug, Clone)]
pub struct LoggerConfig {
    /// Minimum log level.
    pub level: Level,
    /// Output format.
    pub format: Format,
    /// Include timestamps.
    pub timestamps: bool,
    /// Include target module.
    pub target: bool,
    /// Include file/line location.
    pub location: bool,
    /// Colorize output.
    pub colors: bool,
}

impl Default for LoggerConfig {
    fn default() -> Self {
        Self {
            level: Level::Info,
            format: Format::Plain,
            timestamps: true,
            target: true,
            location: false,
            colors: true,
        }
    }
}

/// Global log level (atomic for thread safety).
static LOG_LEVEL: AtomicUsize = AtomicUsize::new(Level::Info as usize);

/// Set global log level.
pub fn set_level(level: Level) {
    LOG_LEVEL.store(level as usize, Ordering::SeqCst);
}

/// Get global log level.
pub fn get_level() -> Level {
    match LOG_LEVEL.load(Ordering::SeqCst) {
        0 => Level::Trace,
        1 => Level::Debug,
        2 => Level::Info,
        3 => Level::Warn,
        _ => Level::Error,
    }
}

/// Check if level is enabled.
pub fn enabled(level: Level) -> bool {
    level >= get_level()
}

/// Log builder for structured logging.
#[derive(Debug)]
pub struct LogBuilder {
    record: LogRecord,
}

impl LogBuilder {
    /// Create new builder.
    pub fn new(level: Level, message: impl Into<String>) -> Self {
        Self {
            record: LogRecord::new(level, message),
        }
    }

    /// Set target.
    pub fn target(mut self, target: impl Into<String>) -> Self {
        self.record.target = Some(target.into());
        self
    }

    /// Set location.
    pub fn location(mut self, file: impl Into<String>, line: u32) -> Self {
        self.record.file = Some(file.into());
        self.record.line = Some(line);
        self
    }

    /// Add field.
    pub fn field(mut self, key: impl Into<String>, value: impl Serialize) -> Self {
        if let Ok(v) = serde_json::to_value(value) {
            self.record.fields.insert(key.into(), v);
        }
        self
    }

    /// Emit the log record.
    pub fn emit(self) {
        if enabled(self.record.level) {
            eprintln!("{}", self.record.format_plain());
        }
    }

    /// Emit as JSON.
    pub fn emit_json(self) {
        if enabled(self.record.level) {
            eprintln!("{}", self.record.format_json());
        }
    }

    /// Get the record.
    pub fn build(self) -> LogRecord {
        self.record
    }
}

/// Create trace log.
pub fn trace(message: impl Into<String>) -> LogBuilder {
    LogBuilder::new(Level::Trace, message)
}

/// Create debug log.
pub fn debug(message: impl Into<String>) -> LogBuilder {
    LogBuilder::new(Level::Debug, message)
}

/// Create info log.
pub fn info(message: impl Into<String>) -> LogBuilder {
    LogBuilder::new(Level::Info, message)
}

/// Create warn log.
pub fn warn(message: impl Into<String>) -> LogBuilder {
    LogBuilder::new(Level::Warn, message)
}

/// Create error log.
pub fn error(message: impl Into<String>) -> LogBuilder {
    LogBuilder::new(Level::Error, message)
}

/// Log filter by target.
#[derive(Debug, Clone)]
pub struct Filter {
    /// Default level.
    default_level: Level,
    /// Target-specific levels.
    targets: HashMap<String, Level>,
}

impl Filter {
    /// Create new filter.
    pub fn new(default_level: Level) -> Self {
        Self {
            default_level,
            targets: HashMap::new(),
        }
    }

    /// Add target filter.
    pub fn add_target(&mut self, target: impl Into<String>, level: Level) {
        self.targets.insert(target.into(), level);
    }

    /// Check if record should be logged.
    pub fn matches(&self, record: &LogRecord) -> bool {
        let min_level = record
            .target
            .as_ref()
            .and_then(|t| self.targets.get(t))
            .copied()
            .unwrap_or(self.default_level);

        record.level >= min_level
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_level_ordering() {
        assert!(Level::Error > Level::Warn);
        assert!(Level::Warn > Level::Info);
        assert!(Level::Info > Level::Debug);
        assert!(Level::Debug > Level::Trace);
    }

    #[test]
    fn test_level_parse() {
        assert_eq!(Level::from_str("info").unwrap(), Level::Info);
        assert_eq!(Level::from_str("DEBUG").unwrap(), Level::Debug);
        assert_eq!(Level::from_str("warning").unwrap(), Level::Warn);
    }

    #[test]
    fn test_log_record() {
        let record = LogRecord::new(Level::Info, "test message")
            .with_target("test_module")
            .with_field("key", "value");

        assert_eq!(record.level, Level::Info);
        assert_eq!(record.message, "test message");
        assert_eq!(record.target.as_deref(), Some("test_module"));
        assert!(record.fields.contains_key("key"));
    }

    #[test]
    fn test_filter() {
        let mut filter = Filter::new(Level::Info);
        filter.add_target("verbose_module", Level::Debug);

        let info_record = LogRecord::new(Level::Info, "info");
        let debug_record = LogRecord::new(Level::Debug, "debug");
        let verbose_debug = LogRecord::new(Level::Debug, "debug").with_target("verbose_module");

        assert!(filter.matches(&info_record));
        assert!(!filter.matches(&debug_record));
        assert!(filter.matches(&verbose_debug));
    }

    #[test]
    fn test_log_builder() {
        let record = info("test").target("module").field("count", 42).build();

        assert_eq!(record.level, Level::Info);
        assert_eq!(record.target.as_deref(), Some("module"));
    }
}
