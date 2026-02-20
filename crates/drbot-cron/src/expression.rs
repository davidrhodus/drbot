//! Cron expression parsing.
//!
//! Supports standard cron format: minute hour day-of-month month day-of-week
//! Also supports special strings like @hourly, @daily, @weekly, @monthly

use chrono::{DateTime, Datelike, Timelike, Utc};
use std::collections::HashSet;

/// A parsed cron expression.
#[derive(Debug, Clone)]
pub struct CronExpression {
    /// Minutes (0-59).
    pub minutes: HashSet<u32>,
    /// Hours (0-23).
    pub hours: HashSet<u32>,
    /// Days of month (1-31).
    pub days_of_month: HashSet<u32>,
    /// Months (1-12).
    pub months: HashSet<u32>,
    /// Days of week (0-6, Sunday = 0).
    pub days_of_week: HashSet<u32>,
}

impl CronExpression {
    /// Parse a cron expression string.
    pub fn parse(expr: &str) -> Result<Self, String> {
        let expr = expr.trim();

        // Handle special strings
        match expr {
            "@yearly" | "@annually" => {
                return Ok(Self::specific(0, 0, 1, 1, None));
            }
            "@monthly" => {
                return Ok(Self::specific(0, 0, 1, None, None));
            }
            "@weekly" => {
                return Ok(Self::specific_dow(0, 0, 0));
            }
            "@daily" | "@midnight" => {
                return Ok(Self::specific(0, 0, None, None, None));
            }
            "@hourly" => {
                return Ok(Self::specific(0, None, None, None, None));
            }
            _ => {}
        }

        let parts: Vec<&str> = expr.split_whitespace().collect();
        if parts.len() != 5 {
            return Err(format!(
                "Invalid cron expression: expected 5 fields, got {}",
                parts.len()
            ));
        }

        let minutes = Self::parse_field(parts[0], 0, 59)?;
        let hours = Self::parse_field(parts[1], 0, 23)?;
        let days_of_month = Self::parse_field(parts[2], 1, 31)?;
        let months = Self::parse_field(parts[3], 1, 12)?;
        let days_of_week = Self::parse_field(parts[4], 0, 6)?;

        Ok(Self {
            minutes,
            hours,
            days_of_month,
            months,
            days_of_week,
        })
    }

    /// Create expression for specific values.
    fn specific(
        minute: u32,
        hour: impl Into<Option<u32>>,
        day: impl Into<Option<u32>>,
        month: impl Into<Option<u32>>,
        dow: impl Into<Option<u32>>,
    ) -> Self {
        Self {
            minutes: [minute].into_iter().collect(),
            hours: hour
                .into()
                .map(|h| [h].into_iter().collect())
                .unwrap_or_else(|| (0..24).collect()),
            days_of_month: day
                .into()
                .map(|d| [d].into_iter().collect())
                .unwrap_or_else(|| (1..32).collect()),
            months: month
                .into()
                .map(|m| [m].into_iter().collect())
                .unwrap_or_else(|| (1..13).collect()),
            days_of_week: dow
                .into()
                .map(|d| [d].into_iter().collect())
                .unwrap_or_else(|| (0..7).collect()),
        }
    }

    /// Create expression for specific day of week.
    fn specific_dow(minute: u32, hour: u32, dow: u32) -> Self {
        Self {
            minutes: [minute].into_iter().collect(),
            hours: [hour].into_iter().collect(),
            days_of_month: (1..32).collect(),
            months: (1..13).collect(),
            days_of_week: [dow].into_iter().collect(),
        }
    }

    /// Parse a single field.
    fn parse_field(field: &str, min: u32, max: u32) -> Result<HashSet<u32>, String> {
        let mut values = HashSet::new();

        for part in field.split(',') {
            let part = part.trim();

            // Handle * (all values)
            if part == "*" {
                values.extend(min..=max);
                continue;
            }

            // Handle */n (step)
            if let Some(step_str) = part.strip_prefix("*/") {
                let step: u32 = step_str
                    .parse()
                    .map_err(|_| format!("Invalid step value: {}", step_str))?;
                if step == 0 {
                    return Err("Step value cannot be 0".to_string());
                }
                for v in (min..=max).step_by(step as usize) {
                    values.insert(v);
                }
                continue;
            }

            // Handle ranges (n-m)
            if part.contains('-') {
                let range_parts: Vec<&str> = part.split('-').collect();
                if range_parts.len() != 2 {
                    return Err(format!("Invalid range: {}", part));
                }
                let start: u32 = range_parts[0]
                    .parse()
                    .map_err(|_| format!("Invalid range start: {}", range_parts[0]))?;
                let end: u32 = range_parts[1]
                    .parse()
                    .map_err(|_| format!("Invalid range end: {}", range_parts[1]))?;

                if start < min || end > max || start > end {
                    return Err(format!("Invalid range: {}-{}", start, end));
                }
                values.extend(start..=end);
                continue;
            }

            // Handle single value
            let value: u32 = part
                .parse()
                .map_err(|_| format!("Invalid value: {}", part))?;
            if value < min || value > max {
                return Err(format!("Value {} out of range {}-{}", value, min, max));
            }
            values.insert(value);
        }

        Ok(values)
    }

    /// Check if a datetime matches this expression.
    pub fn matches(&self, dt: &DateTime<Utc>) -> bool {
        let minute = dt.minute();
        let hour = dt.hour();
        let day = dt.day();
        let month = dt.month();
        let dow = dt.weekday().num_days_from_sunday();

        self.minutes.contains(&minute)
            && self.hours.contains(&hour)
            && self.days_of_month.contains(&day)
            && self.months.contains(&month)
            && self.days_of_week.contains(&dow)
    }

    /// Get the next datetime that matches this expression.
    pub fn next(&self, after: &DateTime<Utc>) -> Option<DateTime<Utc>> {
        let mut current = *after + chrono::Duration::minutes(1);
        // Zero out seconds
        current = current.with_second(0).unwrap_or(current);

        // Search for up to 4 years
        let max_iterations = 365 * 4 * 24 * 60;

        for _ in 0..max_iterations {
            if self.matches(&current) {
                return Some(current);
            }
            current = current + chrono::Duration::minutes(1);
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_all_stars() {
        let expr = CronExpression::parse("* * * * *").unwrap();
        assert_eq!(expr.minutes.len(), 60);
        assert_eq!(expr.hours.len(), 24);
        assert_eq!(expr.days_of_month.len(), 31);
        assert_eq!(expr.months.len(), 12);
        assert_eq!(expr.days_of_week.len(), 7);
    }

    #[test]
    fn test_parse_specific_values() {
        let expr = CronExpression::parse("30 9 * * *").unwrap();
        assert_eq!(expr.minutes, [30].into_iter().collect());
        assert_eq!(expr.hours, [9].into_iter().collect());
    }

    #[test]
    fn test_parse_ranges() {
        let expr = CronExpression::parse("0-5 9-17 * * *").unwrap();
        assert_eq!(expr.minutes.len(), 6);
        assert_eq!(expr.hours.len(), 9);
    }

    #[test]
    fn test_parse_steps() {
        let expr = CronExpression::parse("*/15 */2 * * *").unwrap();
        assert_eq!(expr.minutes, [0, 15, 30, 45].into_iter().collect());
        assert_eq!(
            expr.hours,
            [0, 2, 4, 6, 8, 10, 12, 14, 16, 18, 20, 22]
                .into_iter()
                .collect()
        );
    }

    #[test]
    fn test_parse_special_hourly() {
        let expr = CronExpression::parse("@hourly").unwrap();
        assert_eq!(expr.minutes, [0].into_iter().collect());
        assert_eq!(expr.hours.len(), 24);
    }

    #[test]
    fn test_parse_special_daily() {
        let expr = CronExpression::parse("@daily").unwrap();
        assert_eq!(expr.minutes, [0].into_iter().collect());
        assert_eq!(expr.hours, [0].into_iter().collect());
    }

    #[test]
    fn test_matches() {
        let expr = CronExpression::parse("30 9 * * *").unwrap();
        let dt = Utc::now().with_hour(9).unwrap().with_minute(30).unwrap();
        assert!(expr.matches(&dt));

        let dt2 = dt.with_minute(31).unwrap();
        assert!(!expr.matches(&dt2));
    }

    #[test]
    fn test_next() {
        let expr = CronExpression::parse("0 * * * *").unwrap(); // Every hour
        let now = Utc::now();
        let next = expr.next(&now).unwrap();
        assert!(next > now);
        assert_eq!(next.minute(), 0);
    }
}
