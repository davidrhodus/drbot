//! Briefing system for proactive notifications.
//!
//! Generates morning briefings, end-of-day summaries, and context-aware notifications.

use chrono::{DateTime, Datelike, Timelike, Utc, Weekday};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Briefing types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BriefingType {
    /// Morning briefing with daily overview.
    Morning,
    /// End of day summary.
    EndOfDay,
    /// Weekly review.
    WeeklyReview,
    /// Context-triggered briefing.
    ContextTriggered,
    /// Custom scheduled briefing.
    Custom,
}

/// A briefing to be generated and delivered.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Briefing {
    /// Unique briefing ID.
    pub id: Uuid,
    /// Briefing type.
    pub briefing_type: BriefingType,
    /// Target user ID.
    pub user_id: String,
    /// Target channel.
    pub channel_id: String,
    /// Scheduled time.
    pub scheduled_for: DateTime<Utc>,
    /// Sections to include.
    pub sections: Vec<BriefingSection>,
    /// Whether the briefing has been delivered.
    pub delivered: bool,
    /// Creation time.
    pub created_at: DateTime<Utc>,
}

impl Briefing {
    /// Create a new briefing.
    pub fn new(briefing_type: BriefingType, user_id: &str, channel_id: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            briefing_type,
            user_id: user_id.to_string(),
            channel_id: channel_id.to_string(),
            scheduled_for: Utc::now(),
            sections: Vec::new(),
            delivered: false,
            created_at: Utc::now(),
        }
    }

    /// Schedule for a specific time.
    pub fn at(mut self, time: DateTime<Utc>) -> Self {
        self.scheduled_for = time;
        self
    }

    /// Add a section.
    pub fn with_section(mut self, section: BriefingSection) -> Self {
        self.sections.push(section);
        self
    }
}

/// A section within a briefing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BriefingSection {
    /// Section type.
    pub section_type: SectionType,
    /// Section title.
    pub title: String,
    /// Section content.
    pub content: String,
    /// Priority (1-10).
    pub priority: u8,
    /// Items in this section.
    pub items: Vec<BriefingItem>,
}

impl BriefingSection {
    /// Create a new section.
    pub fn new(section_type: SectionType, title: &str) -> Self {
        Self {
            section_type,
            title: title.to_string(),
            content: String::new(),
            priority: 5,
            items: Vec::new(),
        }
    }

    /// Set content.
    pub fn with_content(mut self, content: &str) -> Self {
        self.content = content.to_string();
        self
    }

    /// Add an item.
    pub fn with_item(mut self, item: BriefingItem) -> Self {
        self.items.push(item);
        self
    }

    /// Set priority.
    pub fn with_priority(mut self, priority: u8) -> Self {
        self.priority = priority.min(10);
        self
    }
}

/// Section types for briefings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SectionType {
    /// Calendar events.
    Calendar,
    /// Tasks and todos.
    Tasks,
    /// Unread messages summary.
    Messages,
    /// Weather forecast.
    Weather,
    /// News or updates.
    News,
    /// Reminders.
    Reminders,
    /// Follow-ups from previous conversations.
    FollowUps,
    /// Insights and patterns.
    Insights,
    /// Custom section.
    Custom,
}

/// An item within a briefing section.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BriefingItem {
    /// Item title.
    pub title: String,
    /// Item description.
    pub description: Option<String>,
    /// Time if applicable.
    pub time: Option<DateTime<Utc>>,
    /// Source of the item.
    pub source: Option<String>,
    /// Link or action.
    pub action: Option<String>,
    /// Metadata.
    pub metadata: HashMap<String, String>,
}

impl BriefingItem {
    /// Create a new item.
    pub fn new(title: &str) -> Self {
        Self {
            title: title.to_string(),
            description: None,
            time: None,
            source: None,
            action: None,
            metadata: HashMap::new(),
        }
    }

    /// Add description.
    pub fn with_description(mut self, desc: &str) -> Self {
        self.description = Some(desc.to_string());
        self
    }

    /// Add time.
    pub fn with_time(mut self, time: DateTime<Utc>) -> Self {
        self.time = Some(time);
        self
    }

    /// Add source.
    pub fn with_source(mut self, source: &str) -> Self {
        self.source = Some(source.to_string());
        self
    }
}

/// Briefing generator configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BriefingConfig {
    /// Enable morning briefings.
    pub morning_enabled: bool,
    /// Morning briefing hour (24h format).
    pub morning_hour: u8,
    /// Enable end of day briefings.
    pub eod_enabled: bool,
    /// End of day hour.
    pub eod_hour: u8,
    /// Enable weekly reviews.
    pub weekly_enabled: bool,
    /// Weekly review day.
    pub weekly_day: Weekday,
    /// Sections to include in morning briefing.
    pub morning_sections: Vec<SectionType>,
    /// Sections to include in EOD briefing.
    pub eod_sections: Vec<SectionType>,
    /// Time zone offset in hours.
    pub timezone_offset: i32,
}

impl Default for BriefingConfig {
    fn default() -> Self {
        Self {
            morning_enabled: true,
            morning_hour: 8,
            eod_enabled: true,
            eod_hour: 18,
            weekly_enabled: true,
            weekly_day: Weekday::Mon,
            morning_sections: vec![
                SectionType::Calendar,
                SectionType::Tasks,
                SectionType::Messages,
                SectionType::Weather,
            ],
            eod_sections: vec![
                SectionType::Tasks,
                SectionType::FollowUps,
                SectionType::Insights,
            ],
            timezone_offset: 0,
        }
    }
}

/// Briefing generator.
pub struct BriefingGenerator {
    config: BriefingConfig,
    data_sources: Vec<Box<dyn BriefingDataSource>>,
}

impl BriefingGenerator {
    /// Create a new generator.
    pub fn new(config: BriefingConfig) -> Self {
        Self {
            config,
            data_sources: Vec::new(),
        }
    }

    /// Register a data source.
    pub fn register_source(&mut self, source: Box<dyn BriefingDataSource>) {
        self.data_sources.push(source);
    }

    /// Check if a briefing should be generated now.
    pub fn should_generate(&self, briefing_type: BriefingType, now: DateTime<Utc>) -> bool {
        let hour = now.hour() as u8;
        let adjusted_hour = ((hour as i32 + self.config.timezone_offset) % 24) as u8;

        match briefing_type {
            BriefingType::Morning => {
                self.config.morning_enabled && adjusted_hour == self.config.morning_hour
            }
            BriefingType::EndOfDay => {
                self.config.eod_enabled && adjusted_hour == self.config.eod_hour
            }
            BriefingType::WeeklyReview => {
                self.config.weekly_enabled
                    && now.weekday() == self.config.weekly_day
                    && adjusted_hour == self.config.morning_hour
            }
            _ => false,
        }
    }

    /// Generate a briefing.
    pub async fn generate(
        &self,
        briefing_type: BriefingType,
        user_id: &str,
        channel_id: &str,
    ) -> Briefing {
        let mut briefing = Briefing::new(briefing_type, user_id, channel_id);

        let sections = match briefing_type {
            BriefingType::Morning => &self.config.morning_sections,
            BriefingType::EndOfDay => &self.config.eod_sections,
            _ => &self.config.morning_sections,
        };

        for section_type in sections {
            if let Some(section) = self.generate_section(*section_type, user_id).await {
                briefing.sections.push(section);
            }
        }

        // Sort sections by priority
        briefing
            .sections
            .sort_by(|a, b| b.priority.cmp(&a.priority));

        briefing
    }

    async fn generate_section(
        &self,
        section_type: SectionType,
        user_id: &str,
    ) -> Option<BriefingSection> {
        for source in &self.data_sources {
            if source.supports_section(section_type) {
                return source.get_section(section_type, user_id).await;
            }
        }
        None
    }

    /// Format a briefing as text.
    pub fn format_briefing(&self, briefing: &Briefing) -> String {
        let mut output = String::new();

        let greeting = match briefing.briefing_type {
            BriefingType::Morning => "Good morning! Here's your daily briefing:",
            BriefingType::EndOfDay => "Here's your end-of-day summary:",
            BriefingType::WeeklyReview => "Here's your weekly review:",
            _ => "Here's your briefing:",
        };

        output.push_str(greeting);
        output.push_str("\n\n");

        for section in &briefing.sections {
            output.push_str(&format!("**{}**\n", section.title));

            if !section.content.is_empty() {
                output.push_str(&section.content);
                output.push('\n');
            }

            for item in &section.items {
                output.push_str(&format!("• {}", item.title));
                if let Some(desc) = &item.description {
                    output.push_str(&format!(" - {}", desc));
                }
                output.push('\n');
            }

            output.push('\n');
        }

        output
    }
}

/// Data source for briefing content.
#[async_trait::async_trait]
pub trait BriefingDataSource: Send + Sync {
    /// Check if this source supports the given section type.
    fn supports_section(&self, section_type: SectionType) -> bool;

    /// Get section data.
    async fn get_section(
        &self,
        section_type: SectionType,
        user_id: &str,
    ) -> Option<BriefingSection>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_briefing_creation() {
        let briefing = Briefing::new(BriefingType::Morning, "user1", "channel1").with_section(
            BriefingSection::new(SectionType::Calendar, "Today's Events")
                .with_item(BriefingItem::new("Team standup").with_description("10:00 AM")),
        );

        assert_eq!(briefing.user_id, "user1");
        assert_eq!(briefing.sections.len(), 1);
    }

    #[test]
    fn test_briefing_config_default() {
        let config = BriefingConfig::default();
        assert!(config.morning_enabled);
        assert_eq!(config.morning_hour, 8);
    }

    #[test]
    fn test_should_generate() {
        let config = BriefingConfig::default();
        let generator = BriefingGenerator::new(config);

        // Would need to construct a DateTime at the right hour to test
    }
}
