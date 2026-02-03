//! Log filtering utilities for drbot.
//!
//! This crate provides:
//! - Target-based filtering
//! - Pattern matching filters
//! - Composite filters

use thiserror::Error;

/// Filter error types.
#[derive(Error, Debug, Clone)]
pub enum FilterError {
    #[error("Invalid filter: {0}")]
    InvalidFilter(String),

    #[error("Parse error: {0}")]
    ParseError(String),
}

/// Result type for filter operations.
pub type Result<T> = std::result::Result<T, FilterError>;

/// Log level for filtering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

/// Log filter trait.
pub trait LogFilter {
    /// Check if a log entry should be included.
    fn filter(&self, level: Level, target: &str, message: &str) -> bool;
}

/// Allow all filter.
pub struct AllowAll;

impl LogFilter for AllowAll {
    fn filter(&self, _level: Level, _target: &str, _message: &str) -> bool {
        true
    }
}

/// Deny all filter.
pub struct DenyAll;

impl LogFilter for DenyAll {
    fn filter(&self, _level: Level, _target: &str, _message: &str) -> bool {
        false
    }
}

/// Level filter.
pub struct LevelFilter {
    min_level: Level,
}

impl LevelFilter {
    /// Create new level filter.
    pub fn new(min_level: Level) -> Self {
        Self { min_level }
    }
}

impl LogFilter for LevelFilter {
    fn filter(&self, level: Level, _target: &str, _message: &str) -> bool {
        level >= self.min_level
    }
}

/// Target filter.
pub struct TargetFilter {
    patterns: Vec<String>,
    include: bool,
}

impl TargetFilter {
    /// Create include filter.
    pub fn include(patterns: Vec<String>) -> Self {
        Self {
            patterns,
            include: true,
        }
    }

    /// Create exclude filter.
    pub fn exclude(patterns: Vec<String>) -> Self {
        Self {
            patterns,
            include: false,
        }
    }

    fn matches(&self, target: &str) -> bool {
        self.patterns.iter().any(|p| {
            if p.ends_with('*') {
                target.starts_with(&p[..p.len() - 1])
            } else if p.starts_with('*') {
                target.ends_with(&p[1..])
            } else {
                target == p
            }
        })
    }
}

impl LogFilter for TargetFilter {
    fn filter(&self, _level: Level, target: &str, _message: &str) -> bool {
        let matches = self.matches(target);
        if self.include {
            matches
        } else {
            !matches
        }
    }
}

/// Message filter.
pub struct MessageFilter {
    patterns: Vec<String>,
    include: bool,
}

impl MessageFilter {
    /// Create include filter.
    pub fn include(patterns: Vec<String>) -> Self {
        Self {
            patterns,
            include: true,
        }
    }

    /// Create exclude filter.
    pub fn exclude(patterns: Vec<String>) -> Self {
        Self {
            patterns,
            include: false,
        }
    }

    fn matches(&self, message: &str) -> bool {
        self.patterns.iter().any(|p| message.contains(p))
    }
}

impl LogFilter for MessageFilter {
    fn filter(&self, _level: Level, _target: &str, message: &str) -> bool {
        let matches = self.matches(message);
        if self.include {
            matches
        } else {
            !matches
        }
    }
}

/// Combined AND filter.
pub struct AndFilter<A, B> {
    a: A,
    b: B,
}

impl<A: LogFilter, B: LogFilter> AndFilter<A, B> {
    /// Create new AND filter.
    pub fn new(a: A, b: B) -> Self {
        Self { a, b }
    }
}

impl<A: LogFilter, B: LogFilter> LogFilter for AndFilter<A, B> {
    fn filter(&self, level: Level, target: &str, message: &str) -> bool {
        self.a.filter(level, target, message) && self.b.filter(level, target, message)
    }
}

/// Combined OR filter.
pub struct OrFilter<A, B> {
    a: A,
    b: B,
}

impl<A: LogFilter, B: LogFilter> OrFilter<A, B> {
    /// Create new OR filter.
    pub fn new(a: A, b: B) -> Self {
        Self { a, b }
    }
}

impl<A: LogFilter, B: LogFilter> LogFilter for OrFilter<A, B> {
    fn filter(&self, level: Level, target: &str, message: &str) -> bool {
        self.a.filter(level, target, message) || self.b.filter(level, target, message)
    }
}

/// NOT filter.
pub struct NotFilter<F> {
    inner: F,
}

impl<F: LogFilter> NotFilter<F> {
    /// Create new NOT filter.
    pub fn new(inner: F) -> Self {
        Self { inner }
    }
}

impl<F: LogFilter> LogFilter for NotFilter<F> {
    fn filter(&self, level: Level, target: &str, message: &str) -> bool {
        !self.inner.filter(level, target, message)
    }
}

/// Dynamic filter chain.
pub struct FilterChain {
    filters: Vec<Box<dyn LogFilter + Send + Sync>>,
    mode: ChainMode,
}

/// Chain mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChainMode {
    All,
    Any,
}

impl FilterChain {
    /// Create new filter chain (all must match).
    pub fn all() -> Self {
        Self {
            filters: Vec::new(),
            mode: ChainMode::All,
        }
    }

    /// Create new filter chain (any must match).
    pub fn any() -> Self {
        Self {
            filters: Vec::new(),
            mode: ChainMode::Any,
        }
    }

    /// Add filter to chain.
    pub fn add<F: LogFilter + Send + Sync + 'static>(mut self, filter: F) -> Self {
        self.filters.push(Box::new(filter));
        self
    }
}

impl LogFilter for FilterChain {
    fn filter(&self, level: Level, target: &str, message: &str) -> bool {
        if self.filters.is_empty() {
            return true;
        }

        match self.mode {
            ChainMode::All => self
                .filters
                .iter()
                .all(|f| f.filter(level, target, message)),
            ChainMode::Any => self
                .filters
                .iter()
                .any(|f| f.filter(level, target, message)),
        }
    }
}

/// Rate limiting filter.
pub struct RateFilter {
    max_per_second: usize,
    count: std::sync::atomic::AtomicUsize,
    last_reset: std::sync::atomic::AtomicU64,
}

impl RateFilter {
    /// Create new rate filter.
    pub fn new(max_per_second: usize) -> Self {
        Self {
            max_per_second,
            count: std::sync::atomic::AtomicUsize::new(0),
            last_reset: std::sync::atomic::AtomicU64::new(0),
        }
    }
}

impl LogFilter for RateFilter {
    fn filter(&self, _level: Level, _target: &str, _message: &str) -> bool {
        use std::sync::atomic::Ordering;
        use std::time::{SystemTime, UNIX_EPOCH};

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let last = self.last_reset.load(Ordering::Relaxed);
        if now > last {
            self.last_reset.store(now, Ordering::Relaxed);
            self.count.store(1, Ordering::Relaxed);
            return true;
        }

        let count = self.count.fetch_add(1, Ordering::Relaxed);
        count < self.max_per_second
    }
}

/// Sampling filter.
pub struct SampleFilter {
    rate: f64,
}

impl SampleFilter {
    /// Create new sample filter.
    pub fn new(rate: f64) -> Self {
        Self {
            rate: rate.clamp(0.0, 1.0),
        }
    }

    /// Create filter that samples 1 in N.
    pub fn one_in(n: usize) -> Self {
        Self::new(1.0 / n as f64)
    }
}

impl LogFilter for SampleFilter {
    fn filter(&self, _level: Level, _target: &str, _message: &str) -> bool {
        use std::time::{SystemTime, UNIX_EPOCH};

        // Simple pseudo-random using time
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos();

        (seed as f64 / u32::MAX as f64) < self.rate
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allow_deny_all() {
        assert!(AllowAll.filter(Level::Debug, "", ""));
        assert!(!DenyAll.filter(Level::Error, "", ""));
    }

    #[test]
    fn test_level_filter() {
        let filter = LevelFilter::new(Level::Info);

        assert!(filter.filter(Level::Error, "", ""));
        assert!(filter.filter(Level::Info, "", ""));
        assert!(!filter.filter(Level::Debug, "", ""));
    }

    #[test]
    fn test_target_filter() {
        let filter = TargetFilter::include(vec!["myapp*".into()]);

        assert!(filter.filter(Level::Info, "myapp::module", ""));
        assert!(!filter.filter(Level::Info, "other::module", ""));
    }

    #[test]
    fn test_message_filter() {
        let filter = MessageFilter::exclude(vec!["password".into(), "secret".into()]);

        assert!(filter.filter(Level::Info, "", "Hello world"));
        assert!(!filter.filter(Level::Info, "", "password=123"));
    }

    #[test]
    fn test_and_filter() {
        let filter = AndFilter::new(
            LevelFilter::new(Level::Info),
            TargetFilter::include(vec!["myapp*".into()]),
        );

        assert!(filter.filter(Level::Info, "myapp::mod", ""));
        assert!(!filter.filter(Level::Debug, "myapp::mod", ""));
        assert!(!filter.filter(Level::Info, "other", ""));
    }

    #[test]
    fn test_filter_chain() {
        let chain = FilterChain::all()
            .add(LevelFilter::new(Level::Info))
            .add(TargetFilter::include(vec!["myapp*".into()]));

        assert!(chain.filter(Level::Error, "myapp::test", ""));
        assert!(!chain.filter(Level::Debug, "myapp::test", ""));
    }
}
