//! Countdown and deadline utilities for drbot.
//!
//! This crate provides:
//! - Countdown timers
//! - Deadline tracking
//! - Expiration checking
//! - Duration formatting

use chrono::{DateTime, Duration, Utc};
use std::sync::atomic::{AtomicBool, Ordering};
use thiserror::Error;

/// Countdown error types.
#[derive(Error, Debug)]
pub enum CountdownError {
    #[error("Already expired")]
    AlreadyExpired,

    #[error("Invalid duration")]
    InvalidDuration,

    #[error("Already started")]
    AlreadyStarted,
}

/// Result type for countdown operations.
pub type Result<T> = std::result::Result<T, CountdownError>;

/// Countdown timer.
#[derive(Debug, Clone)]
pub struct Countdown {
    deadline: DateTime<Utc>,
    started_at: DateTime<Utc>,
    total_duration: Duration,
}

impl Countdown {
    /// Create countdown with duration.
    pub fn new(duration: Duration) -> Result<Self> {
        if duration <= Duration::zero() {
            return Err(CountdownError::InvalidDuration);
        }

        let now = Utc::now();
        Ok(Self {
            deadline: now + duration,
            started_at: now,
            total_duration: duration,
        })
    }

    /// Create countdown to deadline.
    pub fn until(deadline: DateTime<Utc>) -> Result<Self> {
        let now = Utc::now();
        if deadline <= now {
            return Err(CountdownError::AlreadyExpired);
        }

        let duration = deadline - now;
        Ok(Self {
            deadline,
            started_at: now,
            total_duration: duration,
        })
    }

    /// Get remaining time.
    pub fn remaining(&self) -> Duration {
        let now = Utc::now();
        if now >= self.deadline {
            Duration::zero()
        } else {
            self.deadline - now
        }
    }

    /// Get remaining seconds.
    pub fn remaining_secs(&self) -> i64 {
        self.remaining().num_seconds()
    }

    /// Get elapsed time.
    pub fn elapsed(&self) -> Duration {
        Utc::now() - self.started_at
    }

    /// Get progress (0.0 to 1.0).
    pub fn progress(&self) -> f64 {
        if self.total_duration.num_milliseconds() == 0 {
            return 1.0;
        }
        let elapsed = self.elapsed().num_milliseconds() as f64;
        let total = self.total_duration.num_milliseconds() as f64;
        (elapsed / total).min(1.0)
    }

    /// Check if expired.
    pub fn is_expired(&self) -> bool {
        Utc::now() >= self.deadline
    }

    /// Get deadline.
    pub fn deadline(&self) -> DateTime<Utc> {
        self.deadline
    }

    /// Format remaining time.
    pub fn format_remaining(&self) -> String {
        format_duration(self.remaining())
    }
}

/// Deadline tracker.
#[derive(Debug, Clone)]
pub struct Deadline {
    time: DateTime<Utc>,
    name: Option<String>,
}

impl Deadline {
    /// Create deadline.
    pub fn new(time: DateTime<Utc>) -> Self {
        Self { time, name: None }
    }

    /// Create deadline from duration.
    pub fn after(duration: Duration) -> Self {
        Self::new(Utc::now() + duration)
    }

    /// Set name.
    pub fn named(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Check if passed.
    pub fn is_passed(&self) -> bool {
        Utc::now() >= self.time
    }

    /// Get time until deadline.
    pub fn time_until(&self) -> Duration {
        let now = Utc::now();
        if now >= self.time {
            Duration::zero()
        } else {
            self.time - now
        }
    }

    /// Get deadline time.
    pub fn time(&self) -> DateTime<Utc> {
        self.time
    }

    /// Get name.
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// Extend deadline.
    pub fn extend(&mut self, duration: Duration) {
        self.time = self.time + duration;
    }
}

/// Expiration tracker.
pub struct Expiration {
    created: DateTime<Utc>,
    ttl: Duration,
    refreshed: std::sync::Mutex<DateTime<Utc>>,
}

impl Expiration {
    /// Create new expiration.
    pub fn new(ttl: Duration) -> Self {
        let now = Utc::now();
        Self {
            created: now,
            ttl,
            refreshed: std::sync::Mutex::new(now),
        }
    }

    /// Check if expired.
    pub fn is_expired(&self) -> bool {
        let refreshed = *self.refreshed.lock().unwrap();
        Utc::now() - refreshed > self.ttl
    }

    /// Refresh expiration.
    pub fn refresh(&self) {
        *self.refreshed.lock().unwrap() = Utc::now();
    }

    /// Get remaining TTL.
    pub fn remaining_ttl(&self) -> Duration {
        let refreshed = *self.refreshed.lock().unwrap();
        let elapsed = Utc::now() - refreshed;
        if elapsed >= self.ttl {
            Duration::zero()
        } else {
            self.ttl - elapsed
        }
    }

    /// Get age since creation.
    pub fn age(&self) -> Duration {
        Utc::now() - self.created
    }

    /// Get TTL.
    pub fn ttl(&self) -> Duration {
        self.ttl
    }
}

/// Stopwatch.
pub struct Stopwatch {
    start: DateTime<Utc>,
    paused_at: Option<DateTime<Utc>>,
    accumulated: Duration,
    running: AtomicBool,
}

impl Stopwatch {
    /// Create new stopwatch.
    pub fn new() -> Self {
        Self {
            start: Utc::now(),
            paused_at: None,
            accumulated: Duration::zero(),
            running: AtomicBool::new(false),
        }
    }

    /// Start stopwatch.
    pub fn start(&mut self) {
        if !self.running.load(Ordering::SeqCst) {
            self.start = Utc::now();
            self.running.store(true, Ordering::SeqCst);
        }
    }

    /// Start and return self.
    pub fn started(mut self) -> Self {
        self.start();
        self
    }

    /// Stop stopwatch.
    pub fn stop(&mut self) -> Duration {
        if self.running.load(Ordering::SeqCst) {
            self.accumulated = self.accumulated + (Utc::now() - self.start);
            self.running.store(false, Ordering::SeqCst);
        }
        self.accumulated
    }

    /// Pause stopwatch.
    pub fn pause(&mut self) {
        if self.running.load(Ordering::SeqCst) {
            self.paused_at = Some(Utc::now());
            self.running.store(false, Ordering::SeqCst);
        }
    }

    /// Resume stopwatch.
    pub fn resume(&mut self) {
        if let Some(paused) = self.paused_at.take() {
            self.accumulated = self.accumulated + (paused - self.start);
            self.start = Utc::now();
            self.running.store(true, Ordering::SeqCst);
        }
    }

    /// Get elapsed time.
    pub fn elapsed(&self) -> Duration {
        if self.running.load(Ordering::SeqCst) {
            self.accumulated + (Utc::now() - self.start)
        } else {
            self.accumulated
        }
    }

    /// Reset stopwatch.
    pub fn reset(&mut self) {
        self.start = Utc::now();
        self.paused_at = None;
        self.accumulated = Duration::zero();
        self.running.store(false, Ordering::SeqCst);
    }

    /// Check if running.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}

impl Default for Stopwatch {
    fn default() -> Self {
        Self::new()
    }
}

/// Format duration as human readable string.
pub fn format_duration(duration: Duration) -> String {
    let total_secs = duration.num_seconds();
    if total_secs < 0 {
        return "0s".to_string();
    }

    let days = total_secs / 86400;
    let hours = (total_secs % 86400) / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;

    if days > 0 {
        format!("{}d {}h {}m", days, hours, minutes)
    } else if hours > 0 {
        format!("{}h {}m {}s", hours, minutes, seconds)
    } else if minutes > 0 {
        format!("{}m {}s", minutes, seconds)
    } else {
        format!("{}s", seconds)
    }
}

/// Parse duration from string.
pub fn parse_duration(s: &str) -> Result<Duration> {
    let s = s.trim().to_lowercase();

    if let Some(n) = s.strip_suffix('s') {
        let secs: i64 = n
            .trim()
            .parse()
            .map_err(|_| CountdownError::InvalidDuration)?;
        return Ok(Duration::seconds(secs));
    }

    if let Some(n) = s.strip_suffix('m') {
        let mins: i64 = n
            .trim()
            .parse()
            .map_err(|_| CountdownError::InvalidDuration)?;
        return Ok(Duration::minutes(mins));
    }

    if let Some(n) = s.strip_suffix('h') {
        let hours: i64 = n
            .trim()
            .parse()
            .map_err(|_| CountdownError::InvalidDuration)?;
        return Ok(Duration::hours(hours));
    }

    if let Some(n) = s.strip_suffix('d') {
        let days: i64 = n
            .trim()
            .parse()
            .map_err(|_| CountdownError::InvalidDuration)?;
        return Ok(Duration::days(days));
    }

    // Try as seconds
    let secs: i64 = s.parse().map_err(|_| CountdownError::InvalidDuration)?;
    Ok(Duration::seconds(secs))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_countdown() {
        let countdown = Countdown::new(Duration::seconds(10)).unwrap();
        assert!(!countdown.is_expired());
        assert!(countdown.remaining().num_seconds() > 0);
        assert!(countdown.progress() < 1.0);
    }

    #[test]
    fn test_deadline() {
        let deadline = Deadline::after(Duration::seconds(10));
        assert!(!deadline.is_passed());
        assert!(deadline.time_until().num_seconds() > 0);
    }

    #[test]
    fn test_expiration() {
        let exp = Expiration::new(Duration::milliseconds(100));
        assert!(!exp.is_expired());

        std::thread::sleep(std::time::Duration::from_millis(150));
        assert!(exp.is_expired());

        exp.refresh();
        assert!(!exp.is_expired());
    }

    #[test]
    fn test_stopwatch() {
        let mut sw = Stopwatch::new().started();
        std::thread::sleep(std::time::Duration::from_millis(50));
        let elapsed = sw.stop();
        assert!(elapsed.num_milliseconds() >= 40);
    }

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(Duration::seconds(30)), "30s");
        assert_eq!(format_duration(Duration::seconds(90)), "1m 30s");
        assert_eq!(format_duration(Duration::seconds(3661)), "1h 1m 1s");
    }

    #[test]
    fn test_parse_duration() {
        assert_eq!(parse_duration("30s").unwrap(), Duration::seconds(30));
        assert_eq!(parse_duration("5m").unwrap(), Duration::minutes(5));
        assert_eq!(parse_duration("2h").unwrap(), Duration::hours(2));
        assert_eq!(parse_duration("1d").unwrap(), Duration::days(1));
    }
}
