//! Deadline handling for drbot.
//!
//! This crate provides:
//! - Deadline tracking
//! - Timeout enforcement
//! - Deadline propagation

use std::time::{Duration, Instant};
use thiserror::Error;

/// Deadline error types.
#[derive(Error, Debug, Clone)]
pub enum DeadlineError {
    #[error("Deadline exceeded")]
    Exceeded,

    #[error("Deadline exceeded by {0:?}")]
    ExceededBy(Duration),

    #[error("No deadline set")]
    NotSet,
}

/// Result type for deadline operations.
pub type Result<T> = std::result::Result<T, DeadlineError>;

/// Deadline tracker.
#[derive(Debug, Clone, Copy)]
pub struct Deadline {
    instant: Instant,
}

impl Deadline {
    /// Create deadline from duration.
    pub fn after(duration: Duration) -> Self {
        Self {
            instant: Instant::now() + duration,
        }
    }

    /// Create deadline from instant.
    pub fn at(instant: Instant) -> Self {
        Self { instant }
    }

    /// Create deadline from timestamp (seconds from now).
    pub fn in_secs(secs: u64) -> Self {
        Self::after(Duration::from_secs(secs))
    }

    /// Create deadline from milliseconds.
    pub fn in_millis(millis: u64) -> Self {
        Self::after(Duration::from_millis(millis))
    }

    /// Check if deadline has passed.
    pub fn is_exceeded(&self) -> bool {
        Instant::now() >= self.instant
    }

    /// Get remaining time.
    pub fn remaining(&self) -> Duration {
        self.instant.saturating_duration_since(Instant::now())
    }

    /// Get overage if exceeded.
    pub fn overage(&self) -> Option<Duration> {
        if self.is_exceeded() {
            Some(Instant::now() - self.instant)
        } else {
            None
        }
    }

    /// Check deadline, returning error if exceeded.
    pub fn check(&self) -> Result<()> {
        if self.is_exceeded() {
            Err(DeadlineError::Exceeded)
        } else {
            Ok(())
        }
    }

    /// Check with detailed error.
    pub fn check_detailed(&self) -> Result<()> {
        if let Some(overage) = self.overage() {
            Err(DeadlineError::ExceededBy(overage))
        } else {
            Ok(())
        }
    }

    /// Extend deadline by duration.
    pub fn extend(&mut self, duration: Duration) {
        self.instant += duration;
    }

    /// Get the deadline instant.
    pub fn instant(&self) -> Instant {
        self.instant
    }
}

/// Optional deadline.
#[derive(Debug, Clone, Copy, Default)]
pub struct OptionalDeadline {
    deadline: Option<Deadline>,
}

impl OptionalDeadline {
    /// Create with no deadline.
    pub fn none() -> Self {
        Self { deadline: None }
    }

    /// Create with deadline.
    pub fn some(deadline: Deadline) -> Self {
        Self {
            deadline: Some(deadline),
        }
    }

    /// Create from duration.
    pub fn after(duration: Duration) -> Self {
        Self::some(Deadline::after(duration))
    }

    /// Check if has deadline.
    pub fn has_deadline(&self) -> bool {
        self.deadline.is_some()
    }

    /// Check if exceeded (false if no deadline).
    pub fn is_exceeded(&self) -> bool {
        self.deadline.map(|d| d.is_exceeded()).unwrap_or(false)
    }

    /// Get remaining time (None if no deadline).
    pub fn remaining(&self) -> Option<Duration> {
        self.deadline.map(|d| d.remaining())
    }

    /// Check deadline.
    pub fn check(&self) -> Result<()> {
        if let Some(deadline) = self.deadline {
            deadline.check()
        } else {
            Ok(())
        }
    }
}

impl From<Deadline> for OptionalDeadline {
    fn from(deadline: Deadline) -> Self {
        Self::some(deadline)
    }
}

impl From<Option<Deadline>> for OptionalDeadline {
    fn from(deadline: Option<Deadline>) -> Self {
        Self { deadline }
    }
}

/// Deadline context for propagating deadlines.
pub struct DeadlineContext {
    deadline: OptionalDeadline,
}

impl DeadlineContext {
    /// Create new context with no deadline.
    pub fn new() -> Self {
        Self {
            deadline: OptionalDeadline::none(),
        }
    }

    /// Create context with deadline.
    pub fn with_deadline(deadline: Deadline) -> Self {
        Self {
            deadline: OptionalDeadline::some(deadline),
        }
    }

    /// Create context with timeout.
    pub fn with_timeout(timeout: Duration) -> Self {
        Self::with_deadline(Deadline::after(timeout))
    }

    /// Get deadline.
    pub fn deadline(&self) -> &OptionalDeadline {
        &self.deadline
    }

    /// Check deadline.
    pub fn check(&self) -> Result<()> {
        self.deadline.check()
    }

    /// Get remaining time.
    pub fn remaining(&self) -> Option<Duration> {
        self.deadline.remaining()
    }

    /// Create child context with tighter deadline.
    pub fn with_tighter_deadline(&self, deadline: Deadline) -> Self {
        let new_deadline = match self.deadline.deadline {
            Some(existing) if existing.instant < deadline.instant => existing,
            _ => deadline,
        };
        Self::with_deadline(new_deadline)
    }
}

impl Default for DeadlineContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Execute with deadline.
pub fn with_deadline<T, F>(deadline: Deadline, f: F) -> Result<T>
where
    F: FnOnce() -> T,
{
    deadline.check()?;
    let result = f();
    deadline.check()?;
    Ok(result)
}

/// Execute with timeout.
pub fn with_timeout<T, F>(timeout: Duration, f: F) -> Result<T>
where
    F: FnOnce() -> T,
{
    with_deadline(Deadline::after(timeout), f)
}

/// Deadline watcher for periodic checks.
pub struct DeadlineWatcher {
    deadline: Deadline,
    check_interval: usize,
    counter: usize,
}

impl DeadlineWatcher {
    /// Create new watcher.
    pub fn new(deadline: Deadline, check_interval: usize) -> Self {
        Self {
            deadline,
            check_interval: check_interval.max(1),
            counter: 0,
        }
    }

    /// Check deadline (only actually checks every N calls).
    pub fn check(&mut self) -> Result<()> {
        self.counter += 1;
        if self.counter >= self.check_interval {
            self.counter = 0;
            self.deadline.check()
        } else {
            Ok(())
        }
    }

    /// Force check.
    pub fn force_check(&self) -> Result<()> {
        self.deadline.check()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deadline_not_exceeded() {
        let deadline = Deadline::after(Duration::from_secs(60));
        assert!(!deadline.is_exceeded());
        assert!(deadline.check().is_ok());
    }

    #[test]
    fn test_deadline_exceeded() {
        let deadline = Deadline::after(Duration::from_nanos(1));
        std::thread::sleep(Duration::from_millis(1));
        assert!(deadline.is_exceeded());
        assert!(deadline.check().is_err());
    }

    #[test]
    fn test_remaining() {
        let deadline = Deadline::after(Duration::from_secs(10));
        let remaining = deadline.remaining();
        assert!(remaining.as_secs() <= 10);
        assert!(remaining.as_secs() >= 9);
    }

    #[test]
    fn test_optional_deadline() {
        let no_deadline = OptionalDeadline::none();
        assert!(!no_deadline.is_exceeded());
        assert!(no_deadline.check().is_ok());

        let with_deadline = OptionalDeadline::after(Duration::from_secs(60));
        assert!(!with_deadline.is_exceeded());
    }

    #[test]
    fn test_with_timeout() {
        let result = with_timeout(Duration::from_secs(10), || 42);
        assert_eq!(result.unwrap(), 42);
    }
}
