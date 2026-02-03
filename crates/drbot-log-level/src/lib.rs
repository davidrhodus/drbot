//! Log level utilities for drbot.
//!
//! This crate provides:
//! - Log level definitions
//! - Level parsing
//! - Level filtering

use std::str::FromStr;
use thiserror::Error;

/// Level error types.
#[derive(Error, Debug, Clone)]
pub enum LevelError {
    #[error("Invalid log level: {0}")]
    InvalidLevel(String),
}

/// Result type for level operations.
pub type Result<T> = std::result::Result<T, LevelError>;

/// Log level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Level {
    Trace = 0,
    Debug = 1,
    Info = 2,
    Warn = 3,
    Error = 4,
    Off = 5,
}

impl Level {
    /// Get all levels (excluding Off).
    pub fn all() -> &'static [Level] {
        &[
            Level::Trace,
            Level::Debug,
            Level::Info,
            Level::Warn,
            Level::Error,
        ]
    }

    /// Get level name.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Trace => "trace",
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
            Self::Off => "off",
        }
    }

    /// Get uppercase name.
    pub fn name_upper(&self) -> &'static str {
        match self {
            Self::Trace => "TRACE",
            Self::Debug => "DEBUG",
            Self::Info => "INFO",
            Self::Warn => "WARN",
            Self::Error => "ERROR",
            Self::Off => "OFF",
        }
    }

    /// Check if this level is enabled for the given filter level.
    pub fn is_enabled(&self, filter: Level) -> bool {
        *self >= filter
    }

    /// Get numeric value.
    pub fn as_u8(&self) -> u8 {
        *self as u8
    }

    /// Create from numeric value.
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Trace),
            1 => Some(Self::Debug),
            2 => Some(Self::Info),
            3 => Some(Self::Warn),
            4 => Some(Self::Error),
            5 => Some(Self::Off),
            _ => None,
        }
    }
}

impl FromStr for Level {
    type Err = LevelError;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "trace" | "trce" | "trc" => Ok(Self::Trace),
            "debug" | "dbug" | "dbg" => Ok(Self::Debug),
            "info" | "inf" => Ok(Self::Info),
            "warn" | "warning" | "wrn" => Ok(Self::Warn),
            "error" | "err" | "fatal" => Ok(Self::Error),
            "off" | "none" | "disabled" => Ok(Self::Off),
            _ => Err(LevelError::InvalidLevel(s.to_string())),
        }
    }
}

impl std::fmt::Display for Level {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

impl Default for Level {
    fn default() -> Self {
        Self::Info
    }
}

/// Level filter for configuring minimum log level.
#[derive(Debug, Clone)]
pub struct LevelFilter {
    level: Level,
}

impl LevelFilter {
    /// Create new filter.
    pub fn new(level: Level) -> Self {
        Self { level }
    }

    /// Create filter from string.
    pub fn from_str(s: &str) -> Result<Self> {
        Ok(Self { level: s.parse()? })
    }

    /// Create filter from env var.
    pub fn from_env(var: &str) -> Option<Self> {
        std::env::var(var)
            .ok()
            .and_then(|s| Self::from_str(&s).ok())
    }

    /// Check if level is enabled.
    pub fn is_enabled(&self, level: Level) -> bool {
        level.is_enabled(self.level)
    }

    /// Get filter level.
    pub fn level(&self) -> Level {
        self.level
    }

    /// Set filter level.
    pub fn set_level(&mut self, level: Level) {
        self.level = level;
    }
}

impl Default for LevelFilter {
    fn default() -> Self {
        Self { level: Level::Info }
    }
}

/// Parse log level directive string.
/// Format: "module=level,other_module=level"
pub fn parse_directives(s: &str) -> Vec<(Option<String>, Level)> {
    let mut directives = Vec::new();

    for part in s.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }

        if let Some(eq_pos) = part.find('=') {
            let target = &part[..eq_pos];
            let level_str = &part[eq_pos + 1..];

            if let Ok(level) = level_str.parse() {
                directives.push((Some(target.to_string()), level));
            }
        } else if let Ok(level) = part.parse() {
            directives.push((None, level));
        }
    }

    directives
}

/// Level guard that tracks if a level is enabled.
pub struct LevelGuard {
    enabled: bool,
}

impl LevelGuard {
    /// Create new guard.
    pub fn new(current: Level, filter: Level) -> Self {
        Self {
            enabled: current.is_enabled(filter),
        }
    }

    /// Check if enabled.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Run closure if enabled.
    pub fn then<F, R>(&self, f: F) -> Option<R>
    where
        F: FnOnce() -> R,
    {
        if self.enabled {
            Some(f())
        } else {
            None
        }
    }
}

/// Verbosity level (for -v flags).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Verbosity(u8);

impl Verbosity {
    /// Create new verbosity.
    pub fn new(level: u8) -> Self {
        Self(level.min(4))
    }

    /// Increment verbosity.
    pub fn increment(&mut self) {
        self.0 = (self.0 + 1).min(4);
    }

    /// Convert to log level.
    pub fn to_level(&self) -> Level {
        match self.0 {
            0 => Level::Warn,
            1 => Level::Info,
            2 => Level::Debug,
            _ => Level::Trace,
        }
    }

    /// Get raw value.
    pub fn value(&self) -> u8 {
        self.0
    }
}

impl Default for Verbosity {
    fn default() -> Self {
        Self(0)
    }
}

impl From<u8> for Verbosity {
    fn from(v: u8) -> Self {
        Self::new(v)
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
        assert_eq!("info".parse::<Level>().unwrap(), Level::Info);
        assert_eq!("ERROR".parse::<Level>().unwrap(), Level::Error);
        assert_eq!("debug".parse::<Level>().unwrap(), Level::Debug);
    }

    #[test]
    fn test_level_enabled() {
        assert!(Level::Error.is_enabled(Level::Info));
        assert!(Level::Info.is_enabled(Level::Info));
        assert!(!Level::Debug.is_enabled(Level::Info));
    }

    #[test]
    fn test_level_filter() {
        let filter = LevelFilter::new(Level::Info);

        assert!(filter.is_enabled(Level::Error));
        assert!(filter.is_enabled(Level::Info));
        assert!(!filter.is_enabled(Level::Debug));
    }

    #[test]
    fn test_parse_directives() {
        let directives = parse_directives("info,myapp=debug,other=trace");

        assert_eq!(directives.len(), 3);
        assert_eq!(directives[0], (None, Level::Info));
        assert_eq!(directives[1], (Some("myapp".into()), Level::Debug));
        assert_eq!(directives[2], (Some("other".into()), Level::Trace));
    }

    #[test]
    fn test_verbosity() {
        let mut v = Verbosity::default();
        assert_eq!(v.to_level(), Level::Warn);

        v.increment();
        assert_eq!(v.to_level(), Level::Info);

        v.increment();
        assert_eq!(v.to_level(), Level::Debug);

        v.increment();
        assert_eq!(v.to_level(), Level::Trace);
    }
}
