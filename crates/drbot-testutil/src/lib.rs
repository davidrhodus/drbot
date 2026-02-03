//! Test utility helpers for drbot.
//!
//! This crate provides:
//! - Assertion helpers
//! - Test logging
//! - Test capture
//! - Timeout helpers

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use thiserror::Error;

/// Test error types.
#[derive(Error, Debug)]
pub enum TestError {
    #[error("Test failed: {0}")]
    Failed(String),

    #[error("Timeout after {0:?}")]
    Timeout(Duration),

    #[error("Panicked: {0}")]
    Panicked(String),

    #[error("Assertion failed: {0}")]
    AssertionFailed(String),
}

/// Result type for test operations.
pub type Result<T> = std::result::Result<T, TestError>;

/// Captured output.
#[derive(Debug, Clone, Default)]
pub struct CapturedOutput {
    /// Captured lines.
    pub lines: Vec<String>,
}

impl CapturedOutput {
    /// Create new capture.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add line.
    pub fn push(&mut self, line: impl Into<String>) {
        self.lines.push(line.into());
    }

    /// Get all output as string.
    pub fn as_string(&self) -> String {
        self.lines.join("\n")
    }

    /// Check if contains.
    pub fn contains(&self, needle: &str) -> bool {
        self.lines.iter().any(|l| l.contains(needle))
    }

    /// Get line count.
    pub fn len(&self) -> usize {
        self.lines.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
}

/// Output capturer.
pub struct OutputCapture {
    output: Arc<Mutex<CapturedOutput>>,
}

impl OutputCapture {
    /// Create new capturer.
    pub fn new() -> Self {
        Self {
            output: Arc::new(Mutex::new(CapturedOutput::new())),
        }
    }

    /// Get writer.
    pub fn writer(&self) -> CaptureWriter {
        CaptureWriter {
            output: self.output.clone(),
        }
    }

    /// Get captured output.
    pub fn captured(&self) -> CapturedOutput {
        self.output.lock().unwrap().clone()
    }

    /// Clear captured output.
    pub fn clear(&self) {
        self.output.lock().unwrap().lines.clear();
    }
}

impl Default for OutputCapture {
    fn default() -> Self {
        Self::new()
    }
}

/// Writer for output capture.
pub struct CaptureWriter {
    output: Arc<Mutex<CapturedOutput>>,
}

impl CaptureWriter {
    /// Write line.
    pub fn writeln(&self, line: impl Into<String>) {
        self.output.lock().unwrap().push(line);
    }
}

/// Test logger.
pub struct TestLogger {
    messages: Arc<Mutex<Vec<LogMessage>>>,
    enabled: AtomicBool,
}

/// Log message.
#[derive(Debug, Clone)]
pub struct LogMessage {
    /// Log level.
    pub level: LogLevel,
    /// Message.
    pub message: String,
    /// Timestamp.
    pub timestamp: std::time::Instant,
}

/// Log level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    /// Debug.
    Debug,
    /// Info.
    Info,
    /// Warning.
    Warn,
    /// Error.
    Error,
}

impl TestLogger {
    /// Create new logger.
    pub fn new() -> Self {
        Self {
            messages: Arc::new(Mutex::new(Vec::new())),
            enabled: AtomicBool::new(true),
        }
    }

    /// Log message.
    pub fn log(&self, level: LogLevel, message: impl Into<String>) {
        if self.enabled.load(Ordering::SeqCst) {
            self.messages.lock().unwrap().push(LogMessage {
                level,
                message: message.into(),
                timestamp: std::time::Instant::now(),
            });
        }
    }

    /// Log debug.
    pub fn debug(&self, message: impl Into<String>) {
        self.log(LogLevel::Debug, message);
    }

    /// Log info.
    pub fn info(&self, message: impl Into<String>) {
        self.log(LogLevel::Info, message);
    }

    /// Log warning.
    pub fn warn(&self, message: impl Into<String>) {
        self.log(LogLevel::Warn, message);
    }

    /// Log error.
    pub fn error(&self, message: impl Into<String>) {
        self.log(LogLevel::Error, message);
    }

    /// Get messages.
    pub fn messages(&self) -> Vec<LogMessage> {
        self.messages.lock().unwrap().clone()
    }

    /// Get messages at level.
    pub fn messages_at_level(&self, level: LogLevel) -> Vec<LogMessage> {
        self.messages
            .lock()
            .unwrap()
            .iter()
            .filter(|m| m.level == level)
            .cloned()
            .collect()
    }

    /// Clear messages.
    pub fn clear(&self) {
        self.messages.lock().unwrap().clear();
    }

    /// Enable/disable.
    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::SeqCst);
    }
}

impl Default for TestLogger {
    fn default() -> Self {
        Self::new()
    }
}

/// Test counter.
pub struct TestCounter {
    value: AtomicUsize,
}

impl TestCounter {
    /// Create new counter.
    pub fn new() -> Self {
        Self {
            value: AtomicUsize::new(0),
        }
    }

    /// Increment.
    pub fn inc(&self) -> usize {
        self.value.fetch_add(1, Ordering::SeqCst)
    }

    /// Decrement.
    pub fn dec(&self) -> usize {
        self.value.fetch_sub(1, Ordering::SeqCst)
    }

    /// Get value.
    pub fn get(&self) -> usize {
        self.value.load(Ordering::SeqCst)
    }

    /// Set value.
    pub fn set(&self, value: usize) {
        self.value.store(value, Ordering::SeqCst);
    }

    /// Reset to zero.
    pub fn reset(&self) {
        self.set(0);
    }
}

impl Default for TestCounter {
    fn default() -> Self {
        Self::new()
    }
}

/// Test flag.
pub struct TestFlag {
    value: AtomicBool,
}

impl TestFlag {
    /// Create new flag.
    pub fn new() -> Self {
        Self {
            value: AtomicBool::new(false),
        }
    }

    /// Create set flag.
    pub fn set_true() -> Self {
        Self {
            value: AtomicBool::new(true),
        }
    }

    /// Set flag.
    pub fn set(&self) {
        self.value.store(true, Ordering::SeqCst);
    }

    /// Clear flag.
    pub fn clear(&self) {
        self.value.store(false, Ordering::SeqCst);
    }

    /// Check if set.
    pub fn is_set(&self) -> bool {
        self.value.load(Ordering::SeqCst)
    }

    /// Toggle flag.
    pub fn toggle(&self) -> bool {
        !self.value.fetch_xor(true, Ordering::SeqCst)
    }
}

impl Default for TestFlag {
    fn default() -> Self {
        Self::new()
    }
}

/// Run test catching panics.
pub fn catch_panic<F, R>(f: F) -> Result<R>
where
    F: FnOnce() -> R,
{
    catch_unwind(AssertUnwindSafe(f)).map_err(|e| {
        let msg = if let Some(s) = e.downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = e.downcast_ref::<String>() {
            s.clone()
        } else {
            "Unknown panic".to_string()
        };
        TestError::Panicked(msg)
    })
}

/// Assert condition with message.
pub fn assert_with_msg(condition: bool, msg: impl Into<String>) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(TestError::AssertionFailed(msg.into()))
    }
}

/// Assert equality with message.
pub fn assert_eq_with_msg<T: PartialEq + std::fmt::Debug>(
    left: T,
    right: T,
    msg: impl Into<String>,
) -> Result<()> {
    if left == right {
        Ok(())
    } else {
        Err(TestError::AssertionFailed(format!(
            "{}: {:?} != {:?}",
            msg.into(),
            left,
            right
        )))
    }
}

/// Retry helper.
pub struct Retry {
    max_attempts: usize,
    delay: Duration,
}

impl Retry {
    /// Create new retry.
    pub fn new(max_attempts: usize) -> Self {
        Self {
            max_attempts,
            delay: Duration::from_millis(100),
        }
    }

    /// Set delay between attempts.
    pub fn delay(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }

    /// Run with retries.
    pub fn run<F, T, E>(self, mut f: F) -> std::result::Result<T, E>
    where
        F: FnMut() -> std::result::Result<T, E>,
    {
        let mut last_error = None;

        for _ in 0..self.max_attempts {
            match f() {
                Ok(v) => return Ok(v),
                Err(e) => {
                    last_error = Some(e);
                    std::thread::sleep(self.delay);
                }
            }
        }

        Err(last_error.unwrap())
    }
}

/// Wait for condition.
pub fn wait_for<F>(mut condition: F, timeout: Duration, poll_interval: Duration) -> bool
where
    F: FnMut() -> bool,
{
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        if condition() {
            return true;
        }
        std::thread::sleep(poll_interval);
    }
    false
}

/// Wait for value.
pub fn wait_for_value<T: PartialEq + Clone, F>(
    mut getter: F,
    expected: T,
    timeout: Duration,
) -> bool
where
    F: FnMut() -> T,
{
    wait_for(|| getter() == expected, timeout, Duration::from_millis(10))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output_capture() {
        let capture = OutputCapture::new();
        let writer = capture.writer();

        writer.writeln("hello");
        writer.writeln("world");

        let output = capture.captured();
        assert_eq!(output.len(), 2);
        assert!(output.contains("hello"));
    }

    #[test]
    fn test_logger() {
        let logger = TestLogger::new();
        logger.info("test message");
        logger.error("error message");

        let messages = logger.messages();
        assert_eq!(messages.len(), 2);

        let errors = logger.messages_at_level(LogLevel::Error);
        assert_eq!(errors.len(), 1);
    }

    #[test]
    fn test_counter() {
        let counter = TestCounter::new();
        assert_eq!(counter.get(), 0);

        counter.inc();
        counter.inc();
        assert_eq!(counter.get(), 2);

        counter.dec();
        assert_eq!(counter.get(), 1);
    }

    #[test]
    fn test_flag() {
        let flag = TestFlag::new();
        assert!(!flag.is_set());

        flag.set();
        assert!(flag.is_set());

        flag.clear();
        assert!(!flag.is_set());
    }

    #[test]
    fn test_catch_panic() {
        let result = catch_panic(|| 42);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);

        let result: Result<i32> = catch_panic(|| panic!("test panic"));
        assert!(result.is_err());
    }

    #[test]
    fn test_wait_for() {
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();

        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            counter_clone.store(5, Ordering::SeqCst);
        });

        let result = wait_for(
            || counter.load(Ordering::SeqCst) == 5,
            Duration::from_millis(200),
            Duration::from_millis(10),
        );

        assert!(result);
    }
}
