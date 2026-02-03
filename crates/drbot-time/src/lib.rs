//! Time utilities for drbot.
//!
//! This crate provides:
//! - Time manipulation
//! - Duration helpers
//! - Timezone handling
//! - Time parsing and formatting

use chrono::{
    DateTime, Datelike, Duration, Local, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Time error types.
#[derive(Error, Debug)]
pub enum TimeError {
    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Invalid time: {0}")]
    InvalidTime(String),

    #[error("Out of range: {0}")]
    OutOfRange(String),
}

/// Result type for time operations.
pub type Result<T> = std::result::Result<T, TimeError>;

/// Current time utilities.
pub struct Now;

impl Now {
    /// Current UTC time.
    pub fn utc() -> DateTime<Utc> {
        Utc::now()
    }

    /// Current local time.
    pub fn local() -> DateTime<Local> {
        Local::now()
    }

    /// Current Unix timestamp (seconds).
    pub fn timestamp() -> i64 {
        Utc::now().timestamp()
    }

    /// Current Unix timestamp (milliseconds).
    pub fn timestamp_millis() -> i64 {
        Utc::now().timestamp_millis()
    }

    /// Current Unix timestamp (microseconds).
    pub fn timestamp_micros() -> i64 {
        Utc::now().timestamp_micros()
    }

    /// Today's date (UTC).
    pub fn today_utc() -> NaiveDate {
        Utc::now().date_naive()
    }

    /// Today's date (local).
    pub fn today_local() -> NaiveDate {
        Local::now().date_naive()
    }
}

/// Time parsing utilities.
pub struct Parse;

impl Parse {
    /// Parse ISO 8601 datetime.
    pub fn iso8601(s: &str) -> Result<DateTime<Utc>> {
        DateTime::parse_from_rfc3339(s)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| TimeError::ParseError(e.to_string()))
    }

    /// Parse RFC 2822 datetime.
    pub fn rfc2822(s: &str) -> Result<DateTime<Utc>> {
        DateTime::parse_from_rfc2822(s)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| TimeError::ParseError(e.to_string()))
    }

    /// Parse with custom format.
    pub fn format(s: &str, fmt: &str) -> Result<NaiveDateTime> {
        NaiveDateTime::parse_from_str(s, fmt).map_err(|e| TimeError::ParseError(e.to_string()))
    }

    /// Parse date only.
    pub fn date(s: &str, fmt: &str) -> Result<NaiveDate> {
        NaiveDate::parse_from_str(s, fmt).map_err(|e| TimeError::ParseError(e.to_string()))
    }

    /// Parse time only.
    pub fn time(s: &str, fmt: &str) -> Result<NaiveTime> {
        NaiveTime::parse_from_str(s, fmt).map_err(|e| TimeError::ParseError(e.to_string()))
    }

    /// Parse Unix timestamp.
    pub fn timestamp(ts: i64) -> Result<DateTime<Utc>> {
        DateTime::from_timestamp(ts, 0)
            .ok_or_else(|| TimeError::OutOfRange("Invalid timestamp".to_string()))
    }

    /// Parse Unix timestamp millis.
    pub fn timestamp_millis(ts: i64) -> Result<DateTime<Utc>> {
        DateTime::from_timestamp_millis(ts)
            .ok_or_else(|| TimeError::OutOfRange("Invalid timestamp".to_string()))
    }
}

/// Time formatting utilities.
pub struct Format;

impl Format {
    /// Format as ISO 8601.
    pub fn iso8601<Tz: TimeZone>(dt: &DateTime<Tz>) -> String
    where
        Tz::Offset: std::fmt::Display,
    {
        dt.to_rfc3339()
    }

    /// Format as RFC 2822.
    pub fn rfc2822<Tz: TimeZone>(dt: &DateTime<Tz>) -> String
    where
        Tz::Offset: std::fmt::Display,
    {
        dt.to_rfc2822()
    }

    /// Format with custom format string.
    pub fn custom<Tz: TimeZone>(dt: &DateTime<Tz>, fmt: &str) -> String
    where
        Tz::Offset: std::fmt::Display,
    {
        dt.format(fmt).to_string()
    }

    /// Format as human readable date.
    pub fn date_human<Tz: TimeZone>(dt: &DateTime<Tz>) -> String
    where
        Tz::Offset: std::fmt::Display,
    {
        dt.format("%B %d, %Y").to_string()
    }

    /// Format as human readable time.
    pub fn time_human<Tz: TimeZone>(dt: &DateTime<Tz>) -> String
    where
        Tz::Offset: std::fmt::Display,
    {
        dt.format("%I:%M %p").to_string()
    }

    /// Format as human readable datetime.
    pub fn datetime_human<Tz: TimeZone>(dt: &DateTime<Tz>) -> String
    where
        Tz::Offset: std::fmt::Display,
    {
        dt.format("%B %d, %Y at %I:%M %p").to_string()
    }
}

/// Duration utilities.
pub struct Durations;

impl Durations {
    /// Create duration from seconds.
    pub fn seconds(s: i64) -> Duration {
        Duration::seconds(s)
    }

    /// Create duration from minutes.
    pub fn minutes(m: i64) -> Duration {
        Duration::minutes(m)
    }

    /// Create duration from hours.
    pub fn hours(h: i64) -> Duration {
        Duration::hours(h)
    }

    /// Create duration from days.
    pub fn days(d: i64) -> Duration {
        Duration::days(d)
    }

    /// Create duration from weeks.
    pub fn weeks(w: i64) -> Duration {
        Duration::weeks(w)
    }

    /// Parse duration from string (e.g., "1h30m", "2d", "30s").
    pub fn parse(s: &str) -> Result<Duration> {
        let s = s.trim().to_lowercase();
        let mut total = Duration::zero();
        let mut current_num = String::new();

        for c in s.chars() {
            if c.is_ascii_digit() {
                current_num.push(c);
            } else if !current_num.is_empty() {
                let num: i64 = current_num
                    .parse()
                    .map_err(|_| TimeError::ParseError("Invalid number".to_string()))?;
                current_num.clear();

                let duration = match c {
                    's' => Duration::seconds(num),
                    'm' => Duration::minutes(num),
                    'h' => Duration::hours(num),
                    'd' => Duration::days(num),
                    'w' => Duration::weeks(num),
                    _ => return Err(TimeError::ParseError(format!("Unknown unit: {}", c))),
                };
                total = total + duration;
            }
        }

        Ok(total)
    }

    /// Format duration as human readable string.
    pub fn format_human(d: Duration) -> String {
        let total_secs = d.num_seconds().abs();

        if total_secs == 0 {
            return "0 seconds".to_string();
        }

        let days = total_secs / 86400;
        let hours = (total_secs % 86400) / 3600;
        let mins = (total_secs % 3600) / 60;
        let secs = total_secs % 60;

        let mut parts = Vec::new();
        if days > 0 {
            parts.push(format!("{} day{}", days, if days == 1 { "" } else { "s" }));
        }
        if hours > 0 {
            parts.push(format!(
                "{} hour{}",
                hours,
                if hours == 1 { "" } else { "s" }
            ));
        }
        if mins > 0 {
            parts.push(format!(
                "{} minute{}",
                mins,
                if mins == 1 { "" } else { "s" }
            ));
        }
        if secs > 0 {
            parts.push(format!(
                "{} second{}",
                secs,
                if secs == 1 { "" } else { "s" }
            ));
        }

        parts.join(", ")
    }

    /// Format duration as short string (e.g., "1h30m").
    pub fn format_short(d: Duration) -> String {
        let total_secs = d.num_seconds().abs();

        if total_secs < 60 {
            format!("{}s", total_secs)
        } else if total_secs < 3600 {
            let mins = total_secs / 60;
            let secs = total_secs % 60;
            if secs > 0 {
                format!("{}m{}s", mins, secs)
            } else {
                format!("{}m", mins)
            }
        } else if total_secs < 86400 {
            let hours = total_secs / 3600;
            let mins = (total_secs % 3600) / 60;
            if mins > 0 {
                format!("{}h{}m", hours, mins)
            } else {
                format!("{}h", hours)
            }
        } else {
            let days = total_secs / 86400;
            let hours = (total_secs % 86400) / 3600;
            if hours > 0 {
                format!("{}d{}h", days, hours)
            } else {
                format!("{}d", days)
            }
        }
    }
}

/// Time range.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeRange {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

impl TimeRange {
    /// Create new time range.
    pub fn new(start: DateTime<Utc>, end: DateTime<Utc>) -> Self {
        Self { start, end }
    }

    /// Create range from now to duration in future.
    pub fn from_now(duration: Duration) -> Self {
        let start = Utc::now();
        let end = start + duration;
        Self { start, end }
    }

    /// Create range for today.
    pub fn today() -> Self {
        let today = Utc::now().date_naive();
        let start = today.and_hms_opt(0, 0, 0).unwrap();
        let end = today.and_hms_opt(23, 59, 59).unwrap();
        Self {
            start: Utc.from_utc_datetime(&start),
            end: Utc.from_utc_datetime(&end),
        }
    }

    /// Create range for this week.
    pub fn this_week() -> Self {
        let now = Utc::now();
        let weekday = now.weekday().num_days_from_monday();
        let start = now - Duration::days(weekday as i64);
        let end = start + Duration::days(7);
        Self {
            start: start
                .date_naive()
                .and_hms_opt(0, 0, 0)
                .map(|dt| Utc.from_utc_datetime(&dt))
                .unwrap(),
            end: end
                .date_naive()
                .and_hms_opt(23, 59, 59)
                .map(|dt| Utc.from_utc_datetime(&dt))
                .unwrap(),
        }
    }

    /// Duration of range.
    pub fn duration(&self) -> Duration {
        self.end - self.start
    }

    /// Check if time is in range.
    pub fn contains(&self, time: DateTime<Utc>) -> bool {
        time >= self.start && time <= self.end
    }

    /// Check if ranges overlap.
    pub fn overlaps(&self, other: &TimeRange) -> bool {
        self.start <= other.end && other.start <= self.end
    }

    /// Get intersection of ranges.
    pub fn intersection(&self, other: &TimeRange) -> Option<TimeRange> {
        if !self.overlaps(other) {
            return None;
        }

        let start = self.start.max(other.start);
        let end = self.end.min(other.end);
        Some(TimeRange::new(start, end))
    }
}

/// Relative time calculator.
pub struct RelativeTime;

impl RelativeTime {
    /// Time ago string (e.g., "5 minutes ago").
    pub fn ago(time: DateTime<Utc>) -> String {
        let now = Utc::now();
        let duration = now - time;

        if duration.num_seconds() < 0 {
            return Self::from_now_str(time);
        }

        Self::format_relative(duration, "ago")
    }

    /// Time from now string (e.g., "in 5 minutes").
    pub fn from_now_str(time: DateTime<Utc>) -> String {
        let now = Utc::now();
        let duration = time - now;

        if duration.num_seconds() < 0 {
            return Self::ago(time);
        }

        Self::format_relative(duration, "from now")
    }

    fn format_relative(duration: Duration, suffix: &str) -> String {
        let secs = duration.num_seconds().abs();

        if secs < 60 {
            if secs <= 1 {
                "just now".to_string()
            } else {
                format!("{} seconds {}", secs, suffix)
            }
        } else if secs < 3600 {
            let mins = secs / 60;
            format!(
                "{} minute{} {}",
                mins,
                if mins == 1 { "" } else { "s" },
                suffix
            )
        } else if secs < 86400 {
            let hours = secs / 3600;
            format!(
                "{} hour{} {}",
                hours,
                if hours == 1 { "" } else { "s" },
                suffix
            )
        } else if secs < 604800 {
            let days = secs / 86400;
            format!(
                "{} day{} {}",
                days,
                if days == 1 { "" } else { "s" },
                suffix
            )
        } else if secs < 2592000 {
            let weeks = secs / 604800;
            format!(
                "{} week{} {}",
                weeks,
                if weeks == 1 { "" } else { "s" },
                suffix
            )
        } else if secs < 31536000 {
            let months = secs / 2592000;
            format!(
                "{} month{} {}",
                months,
                if months == 1 { "" } else { "s" },
                suffix
            )
        } else {
            let years = secs / 31536000;
            format!(
                "{} year{} {}",
                years,
                if years == 1 { "" } else { "s" },
                suffix
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_now() {
        let utc = Now::utc();
        let local = Now::local();
        let ts = Now::timestamp();

        assert!(ts > 0);
        assert!(utc.timestamp() > 0);
        assert!(local.timestamp() > 0);
    }

    #[test]
    fn test_parse_iso8601() {
        let dt = Parse::iso8601("2024-01-15T12:30:00Z").unwrap();
        assert_eq!(dt.year(), 2024);
        assert_eq!(dt.month(), 1);
        assert_eq!(dt.day(), 15);
    }

    #[test]
    fn test_duration_parse() {
        let d = Durations::parse("1h30m").unwrap();
        assert_eq!(d.num_minutes(), 90);

        let d2 = Durations::parse("2d12h").unwrap();
        assert_eq!(d2.num_hours(), 60);
    }

    #[test]
    fn test_duration_format() {
        let d = Duration::hours(2) + Duration::minutes(30);
        assert_eq!(Durations::format_short(d), "2h30m");

        let d2 = Duration::days(1) + Duration::hours(6);
        assert_eq!(Durations::format_short(d2), "1d6h");
    }

    #[test]
    fn test_time_range() {
        let start = Utc::now();
        let end = start + Duration::hours(2);
        let range = TimeRange::new(start, end);

        assert!(range.contains(start + Duration::hours(1)));
        assert!(!range.contains(start + Duration::hours(3)));
    }

    #[test]
    fn test_time_range_overlap() {
        let r1 = TimeRange::new(
            Parse::iso8601("2024-01-01T00:00:00Z").unwrap(),
            Parse::iso8601("2024-01-03T00:00:00Z").unwrap(),
        );
        let r2 = TimeRange::new(
            Parse::iso8601("2024-01-02T00:00:00Z").unwrap(),
            Parse::iso8601("2024-01-04T00:00:00Z").unwrap(),
        );

        assert!(r1.overlaps(&r2));
        let intersection = r1.intersection(&r2).unwrap();
        assert_eq!(intersection.duration().num_days(), 1);
    }
}
