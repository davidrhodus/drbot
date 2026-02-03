//! AI-powered calendar intelligence for drbot.
//!
//! Smart scheduling and time management.
//!
//! # Features
//!
//! - Intelligent scheduling
//! - Meeting prep briefs
//! - Conflict resolution
//! - Time blocking suggestions
//! - Availability optimization

use async_trait::async_trait;
use chrono::{DateTime, Duration, NaiveTime, Utc, Weekday};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Schedule AI result type.
pub type Result<T> = std::result::Result<T, ScheduleError>;

/// Schedule errors.
#[derive(Debug, thiserror::Error)]
pub enum ScheduleError {
    #[error("Event not found: {0}")]
    EventNotFound(String),
    #[error("Scheduling conflict: {0}")]
    Conflict(String),
    #[error("No available slots")]
    NoAvailableSlots,
    #[error("Invalid time range")]
    InvalidTimeRange,
}

/// Calendar event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalendarEvent {
    /// Event ID.
    pub id: Uuid,
    /// Title.
    pub title: String,
    /// Description.
    pub description: Option<String>,
    /// Start time.
    pub start: DateTime<Utc>,
    /// End time.
    pub end: DateTime<Utc>,
    /// Location.
    pub location: Option<String>,
    /// Attendees.
    pub attendees: Vec<Attendee>,
    /// Event type.
    pub event_type: EventType,
    /// Is all day.
    pub all_day: bool,
    /// Recurrence.
    pub recurrence: Option<Recurrence>,
    /// Reminder minutes before.
    pub reminder_minutes: Option<i32>,
    /// Created at.
    pub created_at: DateTime<Utc>,
}

impl CalendarEvent {
    /// Create a new event.
    pub fn new(title: &str, start: DateTime<Utc>, end: DateTime<Utc>) -> Self {
        Self {
            id: Uuid::new_v4(),
            title: title.to_string(),
            description: None,
            start,
            end,
            location: None,
            attendees: Vec::new(),
            event_type: EventType::Meeting,
            all_day: false,
            recurrence: None,
            reminder_minutes: Some(15),
            created_at: Utc::now(),
        }
    }

    /// Duration in minutes.
    pub fn duration_minutes(&self) -> i64 {
        (self.end - self.start).num_minutes()
    }

    /// Check if overlaps with another event.
    pub fn overlaps(&self, other: &CalendarEvent) -> bool {
        self.start < other.end && self.end > other.start
    }
}

/// Event attendee.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attendee {
    /// Email.
    pub email: String,
    /// Name.
    pub name: Option<String>,
    /// Response status.
    pub status: AttendeeStatus,
    /// Is organizer.
    pub is_organizer: bool,
}

/// Attendee response status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttendeeStatus {
    Pending,
    Accepted,
    Declined,
    Tentative,
}

/// Event type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    Meeting,
    FocusTime,
    Travel,
    Break,
    Personal,
    Deadline,
    Reminder,
}

/// Recurrence pattern.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recurrence {
    /// Frequency.
    pub frequency: RecurrenceFrequency,
    /// Interval.
    pub interval: u32,
    /// End date.
    pub until: Option<DateTime<Utc>>,
    /// Count.
    pub count: Option<u32>,
    /// Days of week (for weekly).
    pub days: Vec<Weekday>,
}

/// Recurrence frequency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecurrenceFrequency {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

/// Available time slot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeSlot {
    /// Start time.
    pub start: DateTime<Utc>,
    /// End time.
    pub end: DateTime<Utc>,
    /// Quality score (0-1).
    pub quality: f32,
    /// Reasons for quality score.
    pub reasons: Vec<String>,
}

impl TimeSlot {
    /// Duration in minutes.
    pub fn duration_minutes(&self) -> i64 {
        (self.end - self.start).num_minutes()
    }
}

/// Meeting brief.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingBrief {
    /// Event ID.
    pub event_id: Uuid,
    /// Summary.
    pub summary: String,
    /// Attendee info.
    pub attendee_info: Vec<AttendeeInfo>,
    /// Related documents.
    pub related_docs: Vec<String>,
    /// Suggested agenda.
    pub suggested_agenda: Vec<String>,
    /// Previous meeting notes.
    pub previous_notes: Option<String>,
    /// Generated at.
    pub generated_at: DateTime<Utc>,
}

/// Attendee information for brief.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttendeeInfo {
    /// Name.
    pub name: String,
    /// Role/title.
    pub role: Option<String>,
    /// Recent interactions.
    pub recent_interactions: Vec<String>,
    /// Notes.
    pub notes: Option<String>,
}

/// Scheduling suggestion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulingSuggestion {
    /// Suggested slot.
    pub slot: TimeSlot,
    /// Confidence.
    pub confidence: f32,
    /// Reasoning.
    pub reasoning: String,
    /// Alternatives.
    pub alternatives: Vec<TimeSlot>,
}

/// Time block suggestion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeBlockSuggestion {
    /// Block type.
    pub block_type: EventType,
    /// Suggested times.
    pub slots: Vec<TimeSlot>,
    /// Total hours suggested.
    pub total_hours: f32,
    /// Reasoning.
    pub reasoning: String,
}

/// Working hours.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkingHours {
    /// Start time.
    pub start: NaiveTime,
    /// End time.
    pub end: NaiveTime,
    /// Working days.
    pub days: Vec<Weekday>,
}

impl Default for WorkingHours {
    fn default() -> Self {
        Self {
            start: NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
            end: NaiveTime::from_hms_opt(17, 0, 0).unwrap(),
            days: vec![
                Weekday::Mon,
                Weekday::Tue,
                Weekday::Wed,
                Weekday::Thu,
                Weekday::Fri,
            ],
        }
    }
}

/// Schedule AI configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleConfig {
    /// Working hours.
    pub working_hours: WorkingHours,
    /// Buffer between meetings (minutes).
    pub meeting_buffer: i32,
    /// Preferred meeting duration (minutes).
    pub preferred_duration: i32,
    /// Focus time goal (hours per day).
    pub focus_time_goal: f32,
    /// Enable smart scheduling.
    pub smart_scheduling: bool,
}

impl Default for ScheduleConfig {
    fn default() -> Self {
        Self {
            working_hours: WorkingHours::default(),
            meeting_buffer: 15,
            preferred_duration: 30,
            focus_time_goal: 4.0,
            smart_scheduling: true,
        }
    }
}

/// Trait for schedule optimizers.
#[async_trait]
pub trait ScheduleOptimizer: Send + Sync {
    /// Find best time slots for a meeting.
    async fn find_slots(
        &self,
        duration_minutes: i32,
        attendees: &[String],
        constraints: &SlotConstraints,
    ) -> Result<Vec<TimeSlot>>;

    /// Suggest time blocks.
    async fn suggest_blocks(
        &self,
        events: &[CalendarEvent],
        config: &ScheduleConfig,
    ) -> Vec<TimeBlockSuggestion>;
}

/// Slot search constraints.
#[derive(Debug, Clone, Default)]
pub struct SlotConstraints {
    /// Earliest start.
    pub earliest: Option<DateTime<Utc>>,
    /// Latest end.
    pub latest: Option<DateTime<Utc>>,
    /// Preferred times of day.
    pub preferred_times: Vec<(NaiveTime, NaiveTime)>,
    /// Avoid days.
    pub avoid_days: Vec<Weekday>,
}

/// Trait for brief generators.
#[async_trait]
pub trait BriefGenerator: Send + Sync {
    /// Generate meeting brief.
    async fn generate_brief(&self, event: &CalendarEvent) -> Result<MeetingBrief>;
}

/// Schedule AI engine.
pub struct ScheduleAIEngine<O: ScheduleOptimizer, B: BriefGenerator> {
    config: ScheduleConfig,
    optimizer: O,
    brief_gen: B,
    events: Arc<RwLock<HashMap<Uuid, CalendarEvent>>>,
    briefs: Arc<RwLock<HashMap<Uuid, MeetingBrief>>>,
}

impl<O: ScheduleOptimizer, B: BriefGenerator> ScheduleAIEngine<O, B> {
    /// Create a new schedule AI engine.
    pub fn new(config: ScheduleConfig, optimizer: O, brief_gen: B) -> Self {
        Self {
            config,
            optimizer,
            brief_gen,
            events: Arc::new(RwLock::new(HashMap::new())),
            briefs: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Add an event.
    pub async fn add_event(&self, event: CalendarEvent) -> Result<()> {
        // Check for conflicts
        let events = self.events.read().await;
        for existing in events.values() {
            if event.overlaps(existing) {
                return Err(ScheduleError::Conflict(format!(
                    "Conflicts with: {}",
                    existing.title
                )));
            }
        }
        drop(events);

        self.events.write().await.insert(event.id, event);
        Ok(())
    }

    /// Get event.
    pub async fn get_event(&self, id: Uuid) -> Option<CalendarEvent> {
        self.events.read().await.get(&id).cloned()
    }

    /// Find available slots.
    pub async fn find_available_slots(
        &self,
        duration_minutes: i32,
        attendees: &[String],
    ) -> Result<Vec<TimeSlot>> {
        let constraints = SlotConstraints {
            earliest: Some(Utc::now()),
            latest: Some(Utc::now() + Duration::days(14)),
            ..Default::default()
        };

        self.optimizer
            .find_slots(duration_minutes, attendees, &constraints)
            .await
    }

    /// Schedule a meeting.
    pub async fn schedule_meeting(
        &self,
        title: &str,
        duration_minutes: i32,
        attendees: Vec<String>,
    ) -> Result<SchedulingSuggestion> {
        let slots = self
            .find_available_slots(duration_minutes, &attendees)
            .await?;

        if slots.is_empty() {
            return Err(ScheduleError::NoAvailableSlots);
        }

        let best = slots[0].clone();
        let alternatives = slots.into_iter().skip(1).take(3).collect();

        Ok(SchedulingSuggestion {
            slot: best.clone(),
            confidence: best.quality,
            reasoning: format!("Best available slot with quality score {:.2}", best.quality),
            alternatives,
        })
    }

    /// Get meeting brief.
    pub async fn get_brief(&self, event_id: Uuid) -> Result<MeetingBrief> {
        // Check cache
        if let Some(brief) = self.briefs.read().await.get(&event_id) {
            return Ok(brief.clone());
        }

        // Generate new brief
        let event = self
            .events
            .read()
            .await
            .get(&event_id)
            .cloned()
            .ok_or(ScheduleError::EventNotFound(event_id.to_string()))?;

        let brief = self.brief_gen.generate_brief(&event).await?;
        self.briefs.write().await.insert(event_id, brief.clone());

        Ok(brief)
    }

    /// Get time block suggestions.
    pub async fn suggest_time_blocks(&self) -> Vec<TimeBlockSuggestion> {
        let events: Vec<_> = self.events.read().await.values().cloned().collect();
        self.optimizer.suggest_blocks(&events, &self.config).await
    }

    /// Get upcoming events.
    pub async fn upcoming(&self, hours: i64) -> Vec<CalendarEvent> {
        let now = Utc::now();
        let until = now + Duration::hours(hours);

        let mut events: Vec<_> = self
            .events
            .read()
            .await
            .values()
            .filter(|e| e.start >= now && e.start <= until)
            .cloned()
            .collect();

        events.sort_by_key(|e| e.start);
        events
    }

    /// Get daily summary.
    pub async fn daily_summary(&self, date: DateTime<Utc>) -> DailySummary {
        let start_of_day = date.date_naive().and_hms_opt(0, 0, 0).unwrap().and_utc();
        let end_of_day = start_of_day + Duration::days(1);

        let events: Vec<_> = self
            .events
            .read()
            .await
            .values()
            .filter(|e| e.start >= start_of_day && e.start < end_of_day)
            .cloned()
            .collect();

        let meeting_hours: f32 = events
            .iter()
            .filter(|e| e.event_type == EventType::Meeting)
            .map(|e| e.duration_minutes() as f32 / 60.0)
            .sum();

        let focus_hours: f32 = events
            .iter()
            .filter(|e| e.event_type == EventType::FocusTime)
            .map(|e| e.duration_minutes() as f32 / 60.0)
            .sum();

        DailySummary {
            date: start_of_day,
            event_count: events.len(),
            meeting_hours,
            focus_hours,
            first_event: events.iter().min_by_key(|e| e.start).cloned(),
            busiest_period: None,
        }
    }
}

/// Daily summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailySummary {
    pub date: DateTime<Utc>,
    pub event_count: usize,
    pub meeting_hours: f32,
    pub focus_hours: f32,
    pub first_event: Option<CalendarEvent>,
    pub busiest_period: Option<(DateTime<Utc>, DateTime<Utc>)>,
}

/// Simple schedule optimizer for testing.
pub struct SimpleOptimizer;

#[async_trait]
impl ScheduleOptimizer for SimpleOptimizer {
    async fn find_slots(
        &self,
        duration_minutes: i32,
        _attendees: &[String],
        constraints: &SlotConstraints,
    ) -> Result<Vec<TimeSlot>> {
        let start = constraints.earliest.unwrap_or_else(Utc::now);
        let duration = Duration::minutes(duration_minutes as i64);

        // Generate some sample slots
        let mut slots = Vec::new();
        let mut current = start;

        for i in 0..5 {
            let slot_start = current + Duration::hours(i * 2);
            slots.push(TimeSlot {
                start: slot_start,
                end: slot_start + duration,
                quality: 0.9 - (i as f32 * 0.1),
                reasons: vec!["Available slot".to_string()],
            });
        }

        Ok(slots)
    }

    async fn suggest_blocks(
        &self,
        events: &[CalendarEvent],
        config: &ScheduleConfig,
    ) -> Vec<TimeBlockSuggestion> {
        let meeting_hours: f32 = events
            .iter()
            .filter(|e| e.event_type == EventType::Meeting)
            .map(|e| e.duration_minutes() as f32 / 60.0)
            .sum();

        let focus_needed = config.focus_time_goal - meeting_hours.min(config.focus_time_goal);

        if focus_needed > 0.0 {
            vec![TimeBlockSuggestion {
                block_type: EventType::FocusTime,
                slots: vec![TimeSlot {
                    start: Utc::now() + Duration::hours(1),
                    end: Utc::now() + Duration::hours(1 + focus_needed as i64),
                    quality: 0.8,
                    reasons: vec!["Focus time recommended".to_string()],
                }],
                total_hours: focus_needed,
                reasoning: format!("You need {:.1} hours of focus time today", focus_needed),
            }]
        } else {
            Vec::new()
        }
    }
}

/// Simple brief generator for testing.
pub struct SimpleBriefGenerator;

#[async_trait]
impl BriefGenerator for SimpleBriefGenerator {
    async fn generate_brief(&self, event: &CalendarEvent) -> Result<MeetingBrief> {
        Ok(MeetingBrief {
            event_id: event.id,
            summary: format!(
                "Meeting: {} ({} minutes)",
                event.title,
                event.duration_minutes()
            ),
            attendee_info: event
                .attendees
                .iter()
                .map(|a| AttendeeInfo {
                    name: a.name.clone().unwrap_or_else(|| a.email.clone()),
                    role: None,
                    recent_interactions: Vec::new(),
                    notes: None,
                })
                .collect(),
            related_docs: Vec::new(),
            suggested_agenda: vec![
                "Opening and introductions".to_string(),
                "Main discussion points".to_string(),
                "Action items and next steps".to_string(),
            ],
            previous_notes: None,
            generated_at: Utc::now(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_add_event() {
        let engine = ScheduleAIEngine::new(
            ScheduleConfig::default(),
            SimpleOptimizer,
            SimpleBriefGenerator,
        );

        let event = CalendarEvent::new(
            "Team Meeting",
            Utc::now() + Duration::hours(1),
            Utc::now() + Duration::hours(2),
        );

        engine.add_event(event).await.unwrap();
        assert_eq!(engine.events.read().await.len(), 1);
    }

    #[tokio::test]
    async fn test_find_slots() {
        let engine = ScheduleAIEngine::new(
            ScheduleConfig::default(),
            SimpleOptimizer,
            SimpleBriefGenerator,
        );

        let slots = engine
            .find_available_slots(30, &["alice@example.com".to_string()])
            .await
            .unwrap();
        assert!(!slots.is_empty());
    }

    #[tokio::test]
    async fn test_meeting_brief() {
        let engine = ScheduleAIEngine::new(
            ScheduleConfig::default(),
            SimpleOptimizer,
            SimpleBriefGenerator,
        );

        let mut event = CalendarEvent::new(
            "Project Review",
            Utc::now() + Duration::hours(1),
            Utc::now() + Duration::hours(2),
        );
        event.attendees.push(Attendee {
            email: "alice@example.com".to_string(),
            name: Some("Alice".to_string()),
            status: AttendeeStatus::Accepted,
            is_organizer: false,
        });

        let event_id = event.id;
        engine.add_event(event).await.unwrap();

        let brief = engine.get_brief(event_id).await.unwrap();
        assert!(!brief.suggested_agenda.is_empty());
    }

    #[tokio::test]
    async fn test_conflict_detection() {
        let engine = ScheduleAIEngine::new(
            ScheduleConfig::default(),
            SimpleOptimizer,
            SimpleBriefGenerator,
        );

        let start = Utc::now() + Duration::hours(1);
        let event1 = CalendarEvent::new("Meeting 1", start, start + Duration::hours(1));
        let event2 = CalendarEvent::new("Meeting 2", start, start + Duration::hours(1));

        engine.add_event(event1).await.unwrap();
        let result = engine.add_event(event2).await;
        assert!(result.is_err());
    }
}
