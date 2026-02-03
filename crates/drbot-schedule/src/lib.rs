//! Scheduling utilities for drbot.
//!
//! This crate provides:
//! - Schedule expressions
//! - Recurrence patterns
//! - Next occurrence calculation
//! - Schedule validation

use chrono::{DateTime, Datelike, Duration, NaiveTime, Timelike, Utc, Weekday};
use std::collections::HashSet;
use thiserror::Error;

/// Schedule error types.
#[derive(Error, Debug)]
pub enum ScheduleError {
    #[error("Invalid schedule expression: {0}")]
    InvalidExpression(String),

    #[error("Invalid time")]
    InvalidTime,

    #[error("No next occurrence")]
    NoNextOccurrence,
}

/// Result type for schedule operations.
pub type Result<T> = std::result::Result<T, ScheduleError>;

/// Day of week set.
#[derive(Debug, Clone, Default)]
pub struct DaySet {
    days: HashSet<Weekday>,
}

impl DaySet {
    /// Create empty set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create with all weekdays.
    pub fn weekdays() -> Self {
        Self {
            days: [
                Weekday::Mon,
                Weekday::Tue,
                Weekday::Wed,
                Weekday::Thu,
                Weekday::Fri,
            ]
            .into_iter()
            .collect(),
        }
    }

    /// Create with weekend.
    pub fn weekend() -> Self {
        Self {
            days: [Weekday::Sat, Weekday::Sun].into_iter().collect(),
        }
    }

    /// Create with all days.
    pub fn all() -> Self {
        Self {
            days: [
                Weekday::Mon,
                Weekday::Tue,
                Weekday::Wed,
                Weekday::Thu,
                Weekday::Fri,
                Weekday::Sat,
                Weekday::Sun,
            ]
            .into_iter()
            .collect(),
        }
    }

    /// Add day.
    pub fn add(&mut self, day: Weekday) {
        self.days.insert(day);
    }

    /// Remove day.
    pub fn remove(&mut self, day: Weekday) {
        self.days.remove(&day);
    }

    /// Check if contains day.
    pub fn contains(&self, day: Weekday) -> bool {
        self.days.contains(&day)
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.days.is_empty()
    }
}

/// Schedule pattern.
#[derive(Debug, Clone)]
pub enum Pattern {
    /// Run once at specific time.
    Once(DateTime<Utc>),
    /// Run every N seconds.
    EverySeconds(u32),
    /// Run every N minutes.
    EveryMinutes(u32),
    /// Run every N hours.
    EveryHours(u32),
    /// Run daily at specific time.
    Daily { time: NaiveTime },
    /// Run on specific days at specific time.
    Weekly { days: DaySet, time: NaiveTime },
    /// Run on specific day of month.
    Monthly { day: u32, time: NaiveTime },
    /// Custom interval.
    Interval(Duration),
}

impl Pattern {
    /// Create daily pattern.
    pub fn daily_at(hour: u32, minute: u32) -> Result<Self> {
        let time = NaiveTime::from_hms_opt(hour, minute, 0).ok_or(ScheduleError::InvalidTime)?;
        Ok(Pattern::Daily { time })
    }

    /// Create hourly pattern.
    pub fn hourly() -> Self {
        Pattern::EveryHours(1)
    }

    /// Create weekly pattern.
    pub fn weekly_on(days: DaySet, hour: u32, minute: u32) -> Result<Self> {
        let time = NaiveTime::from_hms_opt(hour, minute, 0).ok_or(ScheduleError::InvalidTime)?;
        Ok(Pattern::Weekly { days, time })
    }
}

/// Schedule definition.
#[derive(Debug, Clone)]
pub struct Schedule {
    pattern: Pattern,
    timezone: String,
    enabled: bool,
}

impl Schedule {
    /// Create new schedule.
    pub fn new(pattern: Pattern) -> Self {
        Self {
            pattern,
            timezone: "UTC".to_string(),
            enabled: true,
        }
    }

    /// Set timezone.
    pub fn timezone(mut self, tz: impl Into<String>) -> Self {
        self.timezone = tz.into();
        self
    }

    /// Enable/disable.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Get next occurrence.
    pub fn next_occurrence(&self, after: DateTime<Utc>) -> Option<DateTime<Utc>> {
        if !self.enabled {
            return None;
        }

        match &self.pattern {
            Pattern::Once(time) => {
                if *time > after {
                    Some(*time)
                } else {
                    None
                }
            }
            Pattern::EverySeconds(n) => {
                let next = after + Duration::seconds(*n as i64);
                Some(next)
            }
            Pattern::EveryMinutes(n) => {
                let next = after + Duration::minutes(*n as i64);
                Some(next)
            }
            Pattern::EveryHours(n) => {
                let next = after + Duration::hours(*n as i64);
                Some(next)
            }
            Pattern::Daily { time } => {
                let today = after.date_naive().and_time(*time).and_utc();
                if today > after {
                    Some(today)
                } else {
                    Some(
                        (after.date_naive() + chrono::Duration::days(1))
                            .and_time(*time)
                            .and_utc(),
                    )
                }
            }
            Pattern::Weekly { days, time } => {
                if days.is_empty() {
                    return None;
                }

                let mut current = after.date_naive();
                for _ in 0..8 {
                    if days.contains(current.weekday()) {
                        let candidate = current.and_time(*time).and_utc();
                        if candidate > after {
                            return Some(candidate);
                        }
                    }
                    current = current + chrono::Duration::days(1);
                }
                None
            }
            Pattern::Monthly { day, time } => {
                let current_day = after.day();
                let target_day = *day;

                let date = if current_day < target_day {
                    after
                        .date_naive()
                        .with_day(target_day)
                        .map(|d| d.and_time(*time).and_utc())
                        .filter(|t| t > &after)
                } else {
                    None
                };

                date.or_else(|| {
                    // Next month
                    let next_month = if after.month() == 12 {
                        after.with_year(after.year() + 1)?.with_month(1)?
                    } else {
                        after.with_month(after.month() + 1)?
                    };
                    next_month
                        .date_naive()
                        .with_day(target_day)
                        .map(|d| d.and_time(*time).and_utc())
                })
            }
            Pattern::Interval(duration) => Some(after + *duration),
        }
    }

    /// Check if due.
    pub fn is_due(&self, at: DateTime<Utc>, last_run: Option<DateTime<Utc>>) -> bool {
        if !self.enabled {
            return false;
        }

        match last_run {
            Some(last) => {
                if let Some(next) = self.next_occurrence(last) {
                    next <= at
                } else {
                    false
                }
            }
            None => true,
        }
    }

    /// Get pattern.
    pub fn pattern(&self) -> &Pattern {
        &self.pattern
    }

    /// Check if enabled.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
}

/// Recurrence builder.
pub struct RecurrenceBuilder {
    interval: Option<Duration>,
    count: Option<u32>,
    until: Option<DateTime<Utc>>,
}

impl RecurrenceBuilder {
    /// Create new builder.
    pub fn new() -> Self {
        Self {
            interval: None,
            count: None,
            until: None,
        }
    }

    /// Set interval.
    pub fn every(mut self, duration: Duration) -> Self {
        self.interval = Some(duration);
        self
    }

    /// Set count limit.
    pub fn times(mut self, count: u32) -> Self {
        self.count = Some(count);
        self
    }

    /// Set end date.
    pub fn until(mut self, time: DateTime<Utc>) -> Self {
        self.until = Some(time);
        self
    }

    /// Build recurrence.
    pub fn build(self) -> Recurrence {
        Recurrence {
            interval: self.interval.unwrap_or_else(|| Duration::days(1)),
            count: self.count,
            until: self.until,
            current_count: 0,
        }
    }
}

impl Default for RecurrenceBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Recurrence iterator.
pub struct Recurrence {
    interval: Duration,
    count: Option<u32>,
    until: Option<DateTime<Utc>>,
    current_count: u32,
}

impl Recurrence {
    /// Get occurrences starting from a time.
    pub fn occurrences(&self, start: DateTime<Utc>) -> impl Iterator<Item = DateTime<Utc>> + '_ {
        let mut current = start;
        let mut count = 0u32;

        std::iter::from_fn(move || {
            if let Some(max) = self.count {
                if count >= max {
                    return None;
                }
            }
            if let Some(end) = self.until {
                if current > end {
                    return None;
                }
            }

            let result = current;
            current = current + self.interval;
            count += 1;
            Some(result)
        })
    }
}

/// Time window.
#[derive(Debug, Clone)]
pub struct TimeWindow {
    start: NaiveTime,
    end: NaiveTime,
}

impl TimeWindow {
    /// Create new time window.
    pub fn new(start: NaiveTime, end: NaiveTime) -> Self {
        Self { start, end }
    }

    /// Create from hour/minute.
    pub fn from_hours(start_hour: u32, end_hour: u32) -> Result<Self> {
        let start = NaiveTime::from_hms_opt(start_hour, 0, 0).ok_or(ScheduleError::InvalidTime)?;
        let end = NaiveTime::from_hms_opt(end_hour, 0, 0).ok_or(ScheduleError::InvalidTime)?;
        Ok(Self { start, end })
    }

    /// Check if time is in window.
    pub fn contains(&self, time: NaiveTime) -> bool {
        if self.start <= self.end {
            time >= self.start && time <= self.end
        } else {
            // Window spans midnight
            time >= self.start || time <= self.end
        }
    }

    /// Check if datetime is in window.
    pub fn contains_datetime(&self, dt: DateTime<Utc>) -> bool {
        self.contains(dt.time())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_daily_schedule() {
        let schedule = Schedule::new(Pattern::daily_at(10, 30).unwrap());
        let now = Utc::now();
        let next = schedule.next_occurrence(now);

        assert!(next.is_some());
        let next_time = next.unwrap();
        assert!(next_time > now);
    }

    #[test]
    fn test_interval_schedule() {
        let schedule = Schedule::new(Pattern::EveryMinutes(5));
        let now = Utc::now();
        let next = schedule.next_occurrence(now);

        assert!(next.is_some());
        let diff = next.unwrap() - now;
        assert_eq!(diff.num_minutes(), 5);
    }

    #[test]
    fn test_day_set() {
        let weekdays = DaySet::weekdays();
        assert!(weekdays.contains(Weekday::Mon));
        assert!(!weekdays.contains(Weekday::Sat));

        let weekend = DaySet::weekend();
        assert!(!weekend.contains(Weekday::Mon));
        assert!(weekend.contains(Weekday::Sat));
    }

    #[test]
    fn test_recurrence() {
        let recurrence = RecurrenceBuilder::new()
            .every(Duration::hours(1))
            .times(5)
            .build();

        let start = Utc::now();
        let occurrences: Vec<_> = recurrence.occurrences(start).collect();

        assert_eq!(occurrences.len(), 5);
    }

    #[test]
    fn test_time_window() {
        let window = TimeWindow::from_hours(9, 17).unwrap();
        let inside = NaiveTime::from_hms_opt(12, 0, 0).unwrap();
        let outside = NaiveTime::from_hms_opt(20, 0, 0).unwrap();

        assert!(window.contains(inside));
        assert!(!window.contains(outside));
    }
}
