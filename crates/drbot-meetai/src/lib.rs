//! AI meeting assistant for drbot.
//!
//! Real-time meeting intelligence.
//!
//! # Features
//!
//! - Live transcription
//! - Action item extraction
//! - Meeting summaries
//! - Speaker identification
//! - Decision tracking

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

/// Meeting AI result type.
pub type Result<T> = std::result::Result<T, MeetingError>;

/// Meeting AI errors.
#[derive(Debug, thiserror::Error)]
pub enum MeetingError {
    #[error("Meeting not found: {0}")]
    NotFound(String),
    #[error("Transcription failed: {0}")]
    TranscriptionFailed(String),
    #[error("Analysis failed: {0}")]
    AnalysisFailed(String),
}

/// Meeting.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Meeting {
    /// Meeting ID.
    pub id: Uuid,
    /// Title.
    pub title: String,
    /// Participants.
    pub participants: Vec<Participant>,
    /// Start time.
    pub started_at: DateTime<Utc>,
    /// End time.
    pub ended_at: Option<DateTime<Utc>>,
    /// Status.
    pub status: MeetingStatus,
    /// Transcript.
    pub transcript: Vec<TranscriptEntry>,
    /// Metadata.
    pub metadata: HashMap<String, String>,
}

impl Meeting {
    /// Create a new meeting.
    pub fn new(title: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            title: title.to_string(),
            participants: Vec::new(),
            started_at: Utc::now(),
            ended_at: None,
            status: MeetingStatus::InProgress,
            transcript: Vec::new(),
            metadata: HashMap::new(),
        }
    }

    /// Duration in minutes.
    pub fn duration_minutes(&self) -> i64 {
        let end = self.ended_at.unwrap_or_else(Utc::now);
        (end - self.started_at).num_minutes()
    }

    /// Add participant.
    pub fn add_participant(&mut self, participant: Participant) {
        self.participants.push(participant);
    }
}

/// Meeting status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MeetingStatus {
    Scheduled,
    InProgress,
    Ended,
    Cancelled,
}

/// Meeting participant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Participant {
    /// Participant ID.
    pub id: Uuid,
    /// Name.
    pub name: String,
    /// Email.
    pub email: Option<String>,
    /// Role.
    pub role: ParticipantRole,
    /// Speaking time (seconds).
    pub speaking_time: u64,
    /// Joined at.
    pub joined_at: DateTime<Utc>,
    /// Left at.
    pub left_at: Option<DateTime<Utc>>,
}

impl Participant {
    /// Create a new participant.
    pub fn new(name: &str, role: ParticipantRole) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.to_string(),
            email: None,
            role,
            speaking_time: 0,
            joined_at: Utc::now(),
            left_at: None,
        }
    }
}

/// Participant role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParticipantRole {
    Host,
    Presenter,
    Attendee,
    Guest,
}

/// Transcript entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptEntry {
    /// Entry ID.
    pub id: Uuid,
    /// Speaker ID.
    pub speaker_id: Option<Uuid>,
    /// Speaker name.
    pub speaker: String,
    /// Text.
    pub text: String,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
    /// Confidence.
    pub confidence: f32,
    /// Is final.
    pub is_final: bool,
}

/// Action item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionItem {
    /// Item ID.
    pub id: Uuid,
    /// Description.
    pub description: String,
    /// Assignee.
    pub assignee: Option<String>,
    /// Due date.
    pub due_date: Option<DateTime<Utc>>,
    /// Priority.
    pub priority: Priority,
    /// Source text.
    pub source_text: String,
    /// Created at.
    pub created_at: DateTime<Utc>,
    /// Completed.
    pub completed: bool,
}

/// Priority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    High,
    Medium,
    Low,
}

/// Decision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decision {
    /// Decision ID.
    pub id: Uuid,
    /// Description.
    pub description: String,
    /// Context.
    pub context: String,
    /// Made by.
    pub made_by: Vec<String>,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
}

/// Meeting summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingSummary {
    /// Meeting ID.
    pub meeting_id: Uuid,
    /// Brief summary.
    pub brief: String,
    /// Key points.
    pub key_points: Vec<String>,
    /// Decisions made.
    pub decisions: Vec<Decision>,
    /// Action items.
    pub action_items: Vec<ActionItem>,
    /// Topics discussed.
    pub topics: Vec<String>,
    /// Participant stats.
    pub participant_stats: Vec<ParticipantStats>,
    /// Generated at.
    pub generated_at: DateTime<Utc>,
}

/// Participant statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParticipantStats {
    /// Participant name.
    pub name: String,
    /// Speaking time (seconds).
    pub speaking_time: u64,
    /// Speaking percentage.
    pub speaking_percent: f32,
    /// Word count.
    pub word_count: usize,
}

/// Meeting event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MeetingEvent {
    /// Meeting started.
    Started(Uuid),
    /// Meeting ended.
    Ended(Uuid),
    /// Participant joined.
    ParticipantJoined {
        meeting_id: Uuid,
        participant: Participant,
    },
    /// Participant left.
    ParticipantLeft {
        meeting_id: Uuid,
        participant_id: Uuid,
    },
    /// New transcript entry.
    Transcript {
        meeting_id: Uuid,
        entry: TranscriptEntry,
    },
    /// Action item detected.
    ActionDetected {
        meeting_id: Uuid,
        action: ActionItem,
    },
    /// Decision made.
    DecisionMade {
        meeting_id: Uuid,
        decision: Decision,
    },
}

/// Meeting AI configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingConfig {
    /// Enable live transcription.
    pub live_transcription: bool,
    /// Enable action detection.
    pub detect_actions: bool,
    /// Enable decision tracking.
    pub track_decisions: bool,
    /// Auto-generate summary.
    pub auto_summary: bool,
    /// Language.
    pub language: String,
}

impl Default for MeetingConfig {
    fn default() -> Self {
        Self {
            live_transcription: true,
            detect_actions: true,
            track_decisions: true,
            auto_summary: true,
            language: "en".to_string(),
        }
    }
}

/// Trait for transcribers.
#[async_trait]
pub trait Transcriber: Send + Sync {
    /// Transcribe audio chunk.
    async fn transcribe(&self, audio: &[u8], speaker_hint: Option<&str>)
        -> Result<TranscriptEntry>;
}

/// Trait for meeting analyzers.
#[async_trait]
pub trait MeetingAnalyzer: Send + Sync {
    /// Generate summary.
    async fn summarize(&self, meeting: &Meeting) -> Result<MeetingSummary>;
    /// Extract action items.
    fn extract_actions(&self, transcript: &[TranscriptEntry]) -> Vec<ActionItem>;
    /// Extract decisions.
    fn extract_decisions(&self, transcript: &[TranscriptEntry]) -> Vec<Decision>;
}

/// Meeting AI engine.
pub struct MeetingAIEngine<T: Transcriber, A: MeetingAnalyzer> {
    config: MeetingConfig,
    transcriber: T,
    analyzer: A,
    meetings: Arc<RwLock<HashMap<Uuid, Meeting>>>,
    summaries: Arc<RwLock<HashMap<Uuid, MeetingSummary>>>,
    event_tx: broadcast::Sender<MeetingEvent>,
}

impl<T: Transcriber, A: MeetingAnalyzer> MeetingAIEngine<T, A> {
    /// Create a new meeting AI engine.
    pub fn new(config: MeetingConfig, transcriber: T, analyzer: A) -> Self {
        let (event_tx, _) = broadcast::channel(100);

        Self {
            config,
            transcriber,
            analyzer,
            meetings: Arc::new(RwLock::new(HashMap::new())),
            summaries: Arc::new(RwLock::new(HashMap::new())),
            event_tx,
        }
    }

    /// Start a meeting.
    pub async fn start_meeting(&self, title: &str) -> Meeting {
        let meeting = Meeting::new(title);
        let id = meeting.id;

        self.meetings.write().await.insert(id, meeting.clone());
        let _ = self.event_tx.send(MeetingEvent::Started(id));

        meeting
    }

    /// End a meeting.
    pub async fn end_meeting(&self, meeting_id: Uuid) -> Result<MeetingSummary> {
        {
            let mut meetings = self.meetings.write().await;
            if let Some(meeting) = meetings.get_mut(&meeting_id) {
                meeting.status = MeetingStatus::Ended;
                meeting.ended_at = Some(Utc::now());
            }
        }

        let _ = self.event_tx.send(MeetingEvent::Ended(meeting_id));

        // Generate summary
        if self.config.auto_summary {
            self.generate_summary(meeting_id).await
        } else {
            Err(MeetingError::NotFound(meeting_id.to_string()))
        }
    }

    /// Add participant.
    pub async fn add_participant(&self, meeting_id: Uuid, participant: Participant) -> Result<()> {
        let mut meetings = self.meetings.write().await;
        let meeting = meetings
            .get_mut(&meeting_id)
            .ok_or(MeetingError::NotFound(meeting_id.to_string()))?;

        meeting.add_participant(participant.clone());

        let _ = self.event_tx.send(MeetingEvent::ParticipantJoined {
            meeting_id,
            participant,
        });

        Ok(())
    }

    /// Process audio.
    pub async fn process_audio(
        &self,
        meeting_id: Uuid,
        audio: &[u8],
        speaker_hint: Option<&str>,
    ) -> Result<TranscriptEntry> {
        if !self.config.live_transcription {
            return Err(MeetingError::TranscriptionFailed(
                "Transcription disabled".to_string(),
            ));
        }

        let entry = self.transcriber.transcribe(audio, speaker_hint).await?;

        {
            let mut meetings = self.meetings.write().await;
            if let Some(meeting) = meetings.get_mut(&meeting_id) {
                meeting.transcript.push(entry.clone());
            }
        }

        // Check for action items
        if self.config.detect_actions {
            let actions = self.analyzer.extract_actions(&[entry.clone()]);
            for action in actions {
                let _ = self
                    .event_tx
                    .send(MeetingEvent::ActionDetected { meeting_id, action });
            }
        }

        let _ = self.event_tx.send(MeetingEvent::Transcript {
            meeting_id,
            entry: entry.clone(),
        });

        Ok(entry)
    }

    /// Add transcript entry manually.
    pub async fn add_transcript(
        &self,
        meeting_id: Uuid,
        speaker: &str,
        text: &str,
    ) -> Result<TranscriptEntry> {
        let entry = TranscriptEntry {
            id: Uuid::new_v4(),
            speaker_id: None,
            speaker: speaker.to_string(),
            text: text.to_string(),
            timestamp: Utc::now(),
            confidence: 1.0,
            is_final: true,
        };

        {
            let mut meetings = self.meetings.write().await;
            if let Some(meeting) = meetings.get_mut(&meeting_id) {
                meeting.transcript.push(entry.clone());
            }
        }

        let _ = self.event_tx.send(MeetingEvent::Transcript {
            meeting_id,
            entry: entry.clone(),
        });

        Ok(entry)
    }

    /// Generate summary.
    pub async fn generate_summary(&self, meeting_id: Uuid) -> Result<MeetingSummary> {
        let meeting = self
            .meetings
            .read()
            .await
            .get(&meeting_id)
            .cloned()
            .ok_or(MeetingError::NotFound(meeting_id.to_string()))?;

        let summary = self.analyzer.summarize(&meeting).await?;
        self.summaries
            .write()
            .await
            .insert(meeting_id, summary.clone());

        Ok(summary)
    }

    /// Get meeting.
    pub async fn get_meeting(&self, id: Uuid) -> Option<Meeting> {
        self.meetings.read().await.get(&id).cloned()
    }

    /// Get summary.
    pub async fn get_summary(&self, meeting_id: Uuid) -> Option<MeetingSummary> {
        self.summaries.read().await.get(&meeting_id).cloned()
    }

    /// List meetings.
    pub async fn list_meetings(&self) -> Vec<Meeting> {
        self.meetings.read().await.values().cloned().collect()
    }

    /// Subscribe to events.
    pub fn subscribe(&self) -> broadcast::Receiver<MeetingEvent> {
        self.event_tx.subscribe()
    }

    /// Get statistics.
    pub async fn stats(&self) -> MeetingStats {
        let meetings = self.meetings.read().await;

        let total_duration: i64 = meetings.values().map(|m| m.duration_minutes()).sum();
        let active = meetings
            .values()
            .filter(|m| m.status == MeetingStatus::InProgress)
            .count();

        MeetingStats {
            total_meetings: meetings.len(),
            active_meetings: active,
            total_duration_minutes: total_duration,
        }
    }
}

/// Meeting statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingStats {
    pub total_meetings: usize,
    pub active_meetings: usize,
    pub total_duration_minutes: i64,
}

/// Simple transcriber for testing.
pub struct SimpleTranscriber;

#[async_trait]
impl Transcriber for SimpleTranscriber {
    async fn transcribe(
        &self,
        _audio: &[u8],
        speaker_hint: Option<&str>,
    ) -> Result<TranscriptEntry> {
        Ok(TranscriptEntry {
            id: Uuid::new_v4(),
            speaker_id: None,
            speaker: speaker_hint.unwrap_or("Unknown").to_string(),
            text: "[Transcribed audio]".to_string(),
            timestamp: Utc::now(),
            confidence: 0.9,
            is_final: true,
        })
    }
}

/// Simple meeting analyzer for testing.
pub struct SimpleAnalyzer;

#[async_trait]
impl MeetingAnalyzer for SimpleAnalyzer {
    async fn summarize(&self, meeting: &Meeting) -> Result<MeetingSummary> {
        let word_count: usize = meeting
            .transcript
            .iter()
            .map(|e| e.text.split_whitespace().count())
            .sum();

        let mut participant_stats: Vec<ParticipantStats> = meeting
            .participants
            .iter()
            .map(|p| {
                let words = meeting
                    .transcript
                    .iter()
                    .filter(|e| e.speaker == p.name)
                    .map(|e| e.text.split_whitespace().count())
                    .sum();
                ParticipantStats {
                    name: p.name.clone(),
                    speaking_time: p.speaking_time,
                    speaking_percent: if word_count > 0 {
                        words as f32 / word_count as f32 * 100.0
                    } else {
                        0.0
                    },
                    word_count: words,
                }
            })
            .collect();
        participant_stats.sort_by(|a, b| b.word_count.cmp(&a.word_count));

        let actions = self.extract_actions(&meeting.transcript);
        let decisions = self.extract_decisions(&meeting.transcript);

        Ok(MeetingSummary {
            meeting_id: meeting.id,
            brief: format!(
                "Meeting: {} ({} minutes)",
                meeting.title,
                meeting.duration_minutes()
            ),
            key_points: vec!["Meeting conducted".to_string()],
            decisions,
            action_items: actions,
            topics: Vec::new(),
            participant_stats,
            generated_at: Utc::now(),
        })
    }

    fn extract_actions(&self, transcript: &[TranscriptEntry]) -> Vec<ActionItem> {
        transcript
            .iter()
            .filter(|e| {
                let lower = e.text.to_lowercase();
                lower.contains("action") || lower.contains("todo") || lower.contains("will do")
            })
            .map(|e| ActionItem {
                id: Uuid::new_v4(),
                description: e.text.clone(),
                assignee: Some(e.speaker.clone()),
                due_date: None,
                priority: Priority::Medium,
                source_text: e.text.clone(),
                created_at: Utc::now(),
                completed: false,
            })
            .collect()
    }

    fn extract_decisions(&self, transcript: &[TranscriptEntry]) -> Vec<Decision> {
        transcript
            .iter()
            .filter(|e| {
                let lower = e.text.to_lowercase();
                lower.contains("decided")
                    || lower.contains("agreed")
                    || lower.contains("will go with")
            })
            .map(|e| Decision {
                id: Uuid::new_v4(),
                description: e.text.clone(),
                context: "From transcript".to_string(),
                made_by: vec![e.speaker.clone()],
                timestamp: e.timestamp,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_start_meeting() {
        let engine =
            MeetingAIEngine::new(MeetingConfig::default(), SimpleTranscriber, SimpleAnalyzer);

        let meeting = engine.start_meeting("Team Sync").await;
        assert_eq!(meeting.title, "Team Sync");
        assert_eq!(meeting.status, MeetingStatus::InProgress);
    }

    #[tokio::test]
    async fn test_add_transcript() {
        let engine =
            MeetingAIEngine::new(MeetingConfig::default(), SimpleTranscriber, SimpleAnalyzer);

        let meeting = engine.start_meeting("Test").await;
        engine
            .add_transcript(meeting.id, "Alice", "Hello everyone")
            .await
            .unwrap();
        engine
            .add_transcript(meeting.id, "Bob", "Hi Alice")
            .await
            .unwrap();

        let m = engine.get_meeting(meeting.id).await.unwrap();
        assert_eq!(m.transcript.len(), 2);
    }

    #[tokio::test]
    async fn test_generate_summary() {
        let engine =
            MeetingAIEngine::new(MeetingConfig::default(), SimpleTranscriber, SimpleAnalyzer);

        let meeting = engine.start_meeting("Planning").await;
        engine
            .add_participant(meeting.id, Participant::new("Alice", ParticipantRole::Host))
            .await
            .unwrap();
        engine
            .add_transcript(
                meeting.id,
                "Alice",
                "We decided to use Rust for this project",
            )
            .await
            .unwrap();

        let summary = engine.generate_summary(meeting.id).await.unwrap();
        assert!(!summary.decisions.is_empty());
    }

    #[tokio::test]
    async fn test_action_extraction() {
        let analyzer = SimpleAnalyzer;
        let transcript = vec![TranscriptEntry {
            id: Uuid::new_v4(),
            speaker_id: None,
            speaker: "Bob".to_string(),
            text: "Action: Review the proposal by Friday".to_string(),
            timestamp: Utc::now(),
            confidence: 1.0,
            is_final: true,
        }];

        let actions = analyzer.extract_actions(&transcript);
        assert!(!actions.is_empty());
    }
}
