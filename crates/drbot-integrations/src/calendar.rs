//! Calendar integration.

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{IntegrationError, IntegrationProvider, Result};

/// Calendar event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalendarEvent {
    /// Event ID.
    pub id: String,
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
    /// Is all day.
    pub all_day: bool,
    /// Attendees.
    pub attendees: Vec<Attendee>,
    /// Meeting link.
    pub meeting_link: Option<String>,
    /// Status.
    pub status: EventStatus,
    /// Calendar ID.
    pub calendar_id: String,
}

/// Attendee.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attendee {
    /// Email.
    pub email: String,
    /// Display name.
    pub name: Option<String>,
    /// Response status.
    pub status: AttendeeStatus,
}

/// Attendee status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttendeeStatus {
    Pending,
    Accepted,
    Declined,
    Tentative,
}

/// Event status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventStatus {
    Confirmed,
    Tentative,
    Cancelled,
}

/// Calendar configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalendarConfig {
    /// Provider (google, outlook).
    pub provider: String,
    /// Primary calendar ID.
    pub primary_calendar: Option<String>,
    /// Sync calendars.
    pub sync_calendars: Vec<String>,
    /// Default reminder minutes.
    pub default_reminder_mins: u32,
}

impl Default for CalendarConfig {
    fn default() -> Self {
        Self {
            provider: "google".to_string(),
            primary_calendar: None,
            sync_calendars: Vec::new(),
            default_reminder_mins: 15,
        }
    }
}

/// Calendar provider trait.
#[async_trait]
pub trait CalendarProvider: IntegrationProvider {
    /// List calendars.
    async fn list_calendars(&self) -> Result<Vec<Calendar>>;

    /// Get events.
    async fn get_events(
        &self,
        calendar_id: &str,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<CalendarEvent>>;

    /// Create event.
    async fn create_event(&self, calendar_id: &str, event: CreateEvent) -> Result<CalendarEvent>;

    /// Update event.
    async fn update_event(&self, event: &CalendarEvent) -> Result<CalendarEvent>;

    /// Delete event.
    async fn delete_event(&self, calendar_id: &str, event_id: &str) -> Result<()>;

    /// Get upcoming events.
    async fn get_upcoming(&self, hours: u32) -> Result<Vec<CalendarEvent>> {
        let now = Utc::now();
        let end = now + Duration::hours(hours as i64);

        let mut events = Vec::new();
        for cal in self.list_calendars().await? {
            let cal_events = self.get_events(&cal.id, now, end).await?;
            events.extend(cal_events);
        }

        events.sort_by_key(|e| e.start);
        Ok(events)
    }
}

/// Calendar info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Calendar {
    /// Calendar ID.
    pub id: String,
    /// Calendar name.
    pub name: String,
    /// Is primary.
    pub is_primary: bool,
    /// Can edit.
    pub can_edit: bool,
    /// Color.
    pub color: Option<String>,
}

/// Event creation request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateEvent {
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
    pub attendees: Vec<String>,
    /// Create meeting link.
    pub create_meeting: bool,
}

impl CreateEvent {
    /// Create a simple event.
    pub fn new(title: &str, start: DateTime<Utc>, end: DateTime<Utc>) -> Self {
        Self {
            title: title.to_string(),
            description: None,
            start,
            end,
            location: None,
            attendees: Vec::new(),
            create_meeting: false,
        }
    }
}

/// Mock calendar provider for testing.
pub struct MockCalendarProvider {
    calendars: Vec<Calendar>,
    events: Vec<CalendarEvent>,
    connected: bool,
}

impl MockCalendarProvider {
    /// Create a new mock provider.
    pub fn new() -> Self {
        Self {
            calendars: vec![Calendar {
                id: "primary".to_string(),
                name: "My Calendar".to_string(),
                is_primary: true,
                can_edit: true,
                color: Some("#4285f4".to_string()),
            }],
            events: Vec::new(),
            connected: false,
        }
    }
}

impl Default for MockCalendarProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl IntegrationProvider for MockCalendarProvider {
    fn name(&self) -> &str {
        "mock-calendar"
    }

    async fn is_connected(&self) -> bool {
        self.connected
    }

    async fn connect(&mut self) -> Result<()> {
        self.connected = true;
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        self.connected = false;
        Ok(())
    }

    async fn refresh(&mut self) -> Result<()> {
        Ok(())
    }
}

#[async_trait]
impl CalendarProvider for MockCalendarProvider {
    async fn list_calendars(&self) -> Result<Vec<Calendar>> {
        Ok(self.calendars.clone())
    }

    async fn get_events(
        &self,
        calendar_id: &str,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<CalendarEvent>> {
        Ok(self
            .events
            .iter()
            .filter(|e| e.calendar_id == calendar_id && e.start >= start && e.start <= end)
            .cloned()
            .collect())
    }

    async fn create_event(&self, _calendar_id: &str, event: CreateEvent) -> Result<CalendarEvent> {
        Ok(CalendarEvent {
            id: Uuid::new_v4().to_string(),
            title: event.title,
            description: event.description,
            start: event.start,
            end: event.end,
            location: event.location,
            all_day: false,
            attendees: event
                .attendees
                .into_iter()
                .map(|e| Attendee {
                    email: e,
                    name: None,
                    status: AttendeeStatus::Pending,
                })
                .collect(),
            meeting_link: None,
            status: EventStatus::Confirmed,
            calendar_id: "primary".to_string(),
        })
    }

    async fn update_event(&self, event: &CalendarEvent) -> Result<CalendarEvent> {
        Ok(event.clone())
    }

    async fn delete_event(&self, _calendar_id: &str, _event_id: &str) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_calendar() {
        let mut provider = MockCalendarProvider::new();
        provider.connect().await.unwrap();

        let calendars = provider.list_calendars().await.unwrap();
        assert_eq!(calendars.len(), 1);

        let event = CreateEvent::new("Test Event", Utc::now(), Utc::now() + Duration::hours(1));

        let created = provider.create_event("primary", event).await.unwrap();
        assert_eq!(created.title, "Test Event");
    }
}
