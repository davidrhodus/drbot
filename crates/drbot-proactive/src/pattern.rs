//! Pattern detection for proactive messaging.

use chrono::{DateTime, Datelike, Timelike, Utc, Weekday};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A detected usage pattern.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pattern {
    /// Pattern ID.
    pub id: String,
    /// Pattern type.
    pub pattern_type: PatternType,
    /// Confidence score (0-1).
    pub confidence: f32,
    /// Pattern data.
    pub data: PatternData,
    /// When pattern was detected.
    pub detected_at: DateTime<Utc>,
}

/// Pattern types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PatternType {
    /// User tends to be active at certain times.
    ActivityTime,
    /// User asks about certain topics frequently.
    FrequentTopic,
    /// User has regular check-in pattern.
    RegularCheckIn,
    /// User shows declining engagement.
    DecliningEngagement,
    /// User shows interest in specific area.
    InterestArea,
}

/// Pattern-specific data.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PatternData {
    /// Activity time pattern.
    ActivityTime {
        peak_hours: Vec<u8>,
        peak_days: Vec<Weekday>,
    },
    /// Frequent topic pattern.
    FrequentTopic { topics: Vec<String>, frequency: u32 },
    /// Regular check-in pattern.
    RegularCheckIn {
        interval_hours: u32,
        last_checkin: DateTime<Utc>,
    },
    /// Declining engagement pattern.
    DecliningEngagement {
        trend: f32, // Negative = declining
        days_tracked: u32,
    },
    /// Interest area pattern.
    InterestArea { area: String, mentions: u32 },
}

/// Pattern matcher for detecting user patterns.
pub struct PatternMatcher {
    /// Minimum data points for pattern detection.
    min_data_points: usize,
    /// Confidence threshold.
    confidence_threshold: f32,
}

impl PatternMatcher {
    /// Create a new pattern matcher.
    pub fn new() -> Self {
        Self {
            min_data_points: 5,
            confidence_threshold: 0.7,
        }
    }

    /// Set minimum data points.
    pub fn with_min_data_points(mut self, min: usize) -> Self {
        self.min_data_points = min;
        self
    }

    /// Set confidence threshold.
    pub fn with_confidence_threshold(mut self, threshold: f32) -> Self {
        self.confidence_threshold = threshold;
        self
    }

    /// Analyze activity times to find patterns.
    pub fn analyze_activity_times(&self, timestamps: &[DateTime<Utc>]) -> Option<Pattern> {
        if timestamps.len() < self.min_data_points {
            return None;
        }

        // Count activity by hour and day
        let mut hour_counts: HashMap<u8, u32> = HashMap::new();
        let mut day_counts: HashMap<Weekday, u32> = HashMap::new();

        for ts in timestamps {
            *hour_counts.entry(ts.hour() as u8).or_insert(0) += 1;
            *day_counts.entry(ts.weekday()).or_insert(0) += 1;
        }

        // Find peak hours (top 3)
        let mut hours: Vec<_> = hour_counts.into_iter().collect();
        hours.sort_by(|a, b| b.1.cmp(&a.1));
        let peak_hours: Vec<u8> = hours.into_iter().take(3).map(|(h, _)| h).collect();

        // Find peak days (above average)
        let avg_day_count = timestamps.len() as f32 / 7.0;
        let peak_days: Vec<Weekday> = day_counts
            .into_iter()
            .filter(|(_, count)| *count as f32 > avg_day_count)
            .map(|(day, _)| day)
            .collect();

        // Calculate confidence based on consistency
        let confidence = if peak_hours.len() <= 3 && !peak_days.is_empty() {
            0.8
        } else {
            0.5
        };

        if confidence < self.confidence_threshold {
            return None;
        }

        Some(Pattern {
            id: uuid::Uuid::new_v4().to_string(),
            pattern_type: PatternType::ActivityTime,
            confidence,
            data: PatternData::ActivityTime {
                peak_hours,
                peak_days,
            },
            detected_at: Utc::now(),
        })
    }

    /// Analyze topics to find frequent ones.
    pub fn analyze_topics(&self, topics: &[String]) -> Option<Pattern> {
        if topics.len() < self.min_data_points {
            return None;
        }

        // Count topic occurrences
        let mut topic_counts: HashMap<&String, u32> = HashMap::new();
        for topic in topics {
            *topic_counts.entry(topic).or_insert(0) += 1;
        }

        // Find frequent topics (appearing more than once)
        let frequent: Vec<(String, u32)> = topic_counts
            .into_iter()
            .filter(|(_, count)| *count > 1)
            .map(|(topic, count)| (topic.clone(), count))
            .collect();

        if frequent.is_empty() {
            return None;
        }

        let max_freq = frequent.iter().map(|(_, c)| *c).max().unwrap_or(1);
        let confidence = (max_freq as f32 / topics.len() as f32).min(1.0);

        if confidence < self.confidence_threshold {
            return None;
        }

        let top_topics: Vec<String> = frequent.into_iter().take(5).map(|(t, _)| t).collect();

        Some(Pattern {
            id: uuid::Uuid::new_v4().to_string(),
            pattern_type: PatternType::FrequentTopic,
            confidence,
            data: PatternData::FrequentTopic {
                topics: top_topics,
                frequency: max_freq,
            },
            detected_at: Utc::now(),
        })
    }

    /// Detect declining engagement.
    pub fn analyze_engagement(&self, daily_message_counts: &[u32]) -> Option<Pattern> {
        if daily_message_counts.len() < self.min_data_points {
            return None;
        }

        // Calculate trend using simple linear regression
        let n = daily_message_counts.len() as f32;
        let sum_x: f32 = (0..daily_message_counts.len()).map(|i| i as f32).sum();
        let sum_y: f32 = daily_message_counts.iter().map(|&c| c as f32).sum();
        let sum_xy: f32 = daily_message_counts
            .iter()
            .enumerate()
            .map(|(i, &c)| i as f32 * c as f32)
            .sum();
        let sum_x2: f32 = (0..daily_message_counts.len())
            .map(|i| (i * i) as f32)
            .sum();

        let slope = (n * sum_xy - sum_x * sum_y) / (n * sum_x2 - sum_x * sum_x);

        // Only report if there's a clear declining trend
        if slope >= -0.5 {
            return None;
        }

        let confidence = (-slope / 2.0).min(1.0);

        Some(Pattern {
            id: uuid::Uuid::new_v4().to_string(),
            pattern_type: PatternType::DecliningEngagement,
            confidence,
            data: PatternData::DecliningEngagement {
                trend: slope,
                days_tracked: daily_message_counts.len() as u32,
            },
            detected_at: Utc::now(),
        })
    }
}

impl Default for PatternMatcher {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn test_activity_time_pattern() {
        let matcher = PatternMatcher::new().with_min_data_points(3);

        // Create timestamps at similar times
        let timestamps: Vec<DateTime<Utc>> = (0..10)
            .map(|i| Utc.with_ymd_and_hms(2024, 1, i + 1, 9, 0, 0).unwrap())
            .collect();

        let pattern = matcher.analyze_activity_times(&timestamps);
        assert!(pattern.is_some());

        if let Some(p) = pattern {
            assert_eq!(p.pattern_type, PatternType::ActivityTime);
            if let PatternData::ActivityTime { peak_hours, .. } = p.data {
                assert!(peak_hours.contains(&9));
            }
        }
    }

    #[test]
    fn test_topic_pattern() {
        let matcher = PatternMatcher::new()
            .with_min_data_points(3)
            .with_confidence_threshold(0.3);

        let topics = vec![
            "rust".to_string(),
            "rust".to_string(),
            "rust".to_string(),
            "python".to_string(),
            "java".to_string(),
        ];

        let pattern = matcher.analyze_topics(&topics);
        assert!(pattern.is_some());
    }
}
