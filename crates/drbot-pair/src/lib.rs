//! Real-time pair programming and collaborative working.
//!
//! This crate provides pair programming capabilities:
//! - Real-time collaboration on code
//! - Shared editing sessions
//! - Live suggestions and feedback
//! - Synchronized cursors and selections

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

/// Pair programming errors.
#[derive(Debug, Error)]
pub enum PairError {
    #[error("Session not found: {0}")]
    SessionNotFound(String),

    #[error("Operation failed: {0}")]
    OperationFailed(String),

    #[error("Sync error: {0}")]
    SyncError(String),

    #[error("Provider error: {0}")]
    ProviderError(String),
}

/// Result type for pair operations.
pub type Result<T> = std::result::Result<T, PairError>;

/// A collaborative editing session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// Session identifier.
    pub id: String,
    /// Session name.
    pub name: String,
    /// Participants.
    pub participants: Vec<Participant>,
    /// Active files.
    pub files: Vec<SharedFile>,
    /// Session state.
    pub state: SessionState,
    /// Created timestamp.
    pub created_at: DateTime<Utc>,
    /// Configuration.
    pub config: SessionConfig,
}

/// A session participant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Participant {
    /// Participant ID.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Role.
    pub role: ParticipantRole,
    /// Current cursor position.
    pub cursor: Option<CursorPosition>,
    /// Current selection.
    pub selection: Option<Selection>,
    /// Status.
    pub status: ParticipantStatus,
    /// Joined timestamp.
    pub joined_at: DateTime<Utc>,
}

/// Participant roles.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ParticipantRole {
    /// Human driver (primary editor).
    Driver,
    /// AI navigator (provides suggestions).
    Navigator,
    /// Observer (read-only).
    Observer,
}

/// Participant status.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ParticipantStatus {
    Active,
    Idle,
    Away,
    Disconnected,
}

/// Cursor position in a file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CursorPosition {
    /// File ID.
    pub file_id: String,
    /// Line number (0-indexed).
    pub line: u32,
    /// Column number (0-indexed).
    pub column: u32,
}

/// A text selection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Selection {
    /// File ID.
    pub file_id: String,
    /// Start position.
    pub start: Position,
    /// End position.
    pub end: Position,
}

/// A position in text.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    /// Line number.
    pub line: u32,
    /// Column number.
    pub column: u32,
}

/// A file being edited in the session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedFile {
    /// File identifier.
    pub id: String,
    /// File path.
    pub path: String,
    /// Current content.
    pub content: String,
    /// Language/type.
    pub language: String,
    /// Version (for conflict resolution).
    pub version: u64,
    /// Last modified.
    pub modified_at: DateTime<Utc>,
}

/// Session state.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SessionState {
    Active,
    Paused,
    Ended,
}

/// Session configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    /// Enable AI suggestions.
    pub ai_suggestions: bool,
    /// Suggestion delay in ms.
    pub suggestion_delay_ms: u32,
    /// Enable auto-complete.
    pub auto_complete: bool,
    /// Show participant cursors.
    pub show_cursors: bool,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            ai_suggestions: true,
            suggestion_delay_ms: 500,
            auto_complete: true,
            show_cursors: true,
        }
    }
}

/// An edit operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditOperation {
    /// Operation ID.
    pub id: String,
    /// Participant who made the edit.
    pub participant_id: String,
    /// File ID.
    pub file_id: String,
    /// Operation type.
    pub operation: OperationType,
    /// Base version.
    pub base_version: u64,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
}

/// Types of edit operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OperationType {
    /// Insert text at position.
    Insert { position: Position, text: String },
    /// Delete text range.
    Delete { start: Position, end: Position },
    /// Replace text range.
    Replace {
        start: Position,
        end: Position,
        text: String,
    },
}

/// A suggestion from the AI navigator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Suggestion {
    /// Suggestion ID.
    pub id: String,
    /// File ID.
    pub file_id: String,
    /// Suggestion type.
    pub suggestion_type: SuggestionType,
    /// Start position.
    pub position: Position,
    /// Suggested text/change.
    pub content: String,
    /// Explanation.
    pub explanation: String,
    /// Confidence (0.0-1.0).
    pub confidence: f64,
    /// Timestamp.
    pub created_at: DateTime<Utc>,
}

/// Types of suggestions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SuggestionType {
    /// Code completion.
    Completion,
    /// Bug fix.
    BugFix,
    /// Refactoring.
    Refactor,
    /// Documentation.
    Documentation,
    /// Performance improvement.
    Performance,
    /// Security fix.
    Security,
    /// Best practice.
    BestPractice,
}

/// Event in a pairing session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SessionEvent {
    ParticipantJoined(Participant),
    ParticipantLeft(String),
    CursorMoved {
        participant_id: String,
        position: CursorPosition,
    },
    SelectionChanged {
        participant_id: String,
        selection: Option<Selection>,
    },
    EditMade(EditOperation),
    SuggestionOffered(Suggestion),
    SuggestionAccepted {
        suggestion_id: String,
    },
    SuggestionRejected {
        suggestion_id: String,
    },
    FileAdded(SharedFile),
    FileClosed(String),
}

/// Provider for pair programming intelligence.
#[async_trait]
pub trait PairProvider: Send + Sync {
    /// Generate suggestions for current context.
    async fn suggest(&self, file: &SharedFile, cursor: &CursorPosition) -> Result<Vec<Suggestion>>;

    /// Review code and provide feedback.
    async fn review(&self, file: &SharedFile) -> Result<Vec<Suggestion>>;

    /// Explain code at cursor.
    async fn explain(&self, file: &SharedFile, selection: &Selection) -> Result<String>;
}

/// The pair programming coordinator.
pub struct PairCoordinator {
    /// Provider for AI assistance.
    provider: Arc<dyn PairProvider>,
    /// Active sessions.
    sessions: Arc<RwLock<HashMap<String, Session>>>,
    /// Event broadcaster.
    event_tx: broadcast::Sender<(String, SessionEvent)>,
}

impl PairCoordinator {
    /// Create a new pair coordinator.
    pub fn new(provider: Arc<dyn PairProvider>) -> Self {
        let (event_tx, _) = broadcast::channel(1000);
        Self {
            provider,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            event_tx,
        }
    }

    /// Create a new session.
    pub async fn create_session(&self, name: &str, config: SessionConfig) -> Result<Session> {
        let session = Session {
            id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            participants: Vec::new(),
            files: Vec::new(),
            state: SessionState::Active,
            created_at: Utc::now(),
            config,
        };

        let mut sessions = self.sessions.write().await;
        sessions.insert(session.id.clone(), session.clone());

        Ok(session)
    }

    /// Join a session.
    pub async fn join_session(
        &self,
        session_id: &str,
        name: &str,
        role: ParticipantRole,
    ) -> Result<Participant> {
        let mut sessions = self.sessions.write().await;
        let session = sessions
            .get_mut(session_id)
            .ok_or_else(|| PairError::SessionNotFound(session_id.to_string()))?;

        let participant = Participant {
            id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            role,
            cursor: None,
            selection: None,
            status: ParticipantStatus::Active,
            joined_at: Utc::now(),
        };

        session.participants.push(participant.clone());

        let _ = self.event_tx.send((
            session_id.to_string(),
            SessionEvent::ParticipantJoined(participant.clone()),
        ));

        Ok(participant)
    }

    /// Leave a session.
    pub async fn leave_session(&self, session_id: &str, participant_id: &str) -> Result<()> {
        let mut sessions = self.sessions.write().await;
        let session = sessions
            .get_mut(session_id)
            .ok_or_else(|| PairError::SessionNotFound(session_id.to_string()))?;

        session.participants.retain(|p| p.id != participant_id);

        let _ = self.event_tx.send((
            session_id.to_string(),
            SessionEvent::ParticipantLeft(participant_id.to_string()),
        ));

        Ok(())
    }

    /// Add a file to the session.
    pub async fn add_file(
        &self,
        session_id: &str,
        path: &str,
        content: &str,
        language: &str,
    ) -> Result<SharedFile> {
        let mut sessions = self.sessions.write().await;
        let session = sessions
            .get_mut(session_id)
            .ok_or_else(|| PairError::SessionNotFound(session_id.to_string()))?;

        let file = SharedFile {
            id: Uuid::new_v4().to_string(),
            path: path.to_string(),
            content: content.to_string(),
            language: language.to_string(),
            version: 1,
            modified_at: Utc::now(),
        };

        session.files.push(file.clone());

        let _ = self.event_tx.send((
            session_id.to_string(),
            SessionEvent::FileAdded(file.clone()),
        ));

        Ok(file)
    }

    /// Apply an edit operation.
    pub async fn apply_edit(&self, session_id: &str, operation: EditOperation) -> Result<()> {
        let mut sessions = self.sessions.write().await;
        let session = sessions
            .get_mut(session_id)
            .ok_or_else(|| PairError::SessionNotFound(session_id.to_string()))?;

        let file = session
            .files
            .iter_mut()
            .find(|f| f.id == operation.file_id)
            .ok_or_else(|| PairError::OperationFailed("File not found".to_string()))?;

        // Apply operation to content
        match &operation.operation {
            OperationType::Insert { position, text } => {
                let lines: Vec<&str> = file.content.lines().collect();
                let mut new_content = String::new();

                for (i, line) in lines.iter().enumerate() {
                    if i == position.line as usize {
                        let (before, after) =
                            line.split_at(position.column.min(line.len() as u32) as usize);
                        new_content.push_str(before);
                        new_content.push_str(text);
                        new_content.push_str(after);
                    } else {
                        new_content.push_str(line);
                    }
                    new_content.push('\n');
                }

                file.content = new_content.trim_end().to_string();
            }
            OperationType::Delete { start, end } => {
                let lines: Vec<&str> = file.content.lines().collect();
                let mut new_lines = Vec::new();

                for (i, line) in lines.iter().enumerate() {
                    if i < start.line as usize || i > end.line as usize {
                        new_lines.push(line.to_string());
                    } else if i == start.line as usize && i == end.line as usize {
                        let before = &line[..start.column.min(line.len() as u32) as usize];
                        let after = &line[end.column.min(line.len() as u32) as usize..];
                        new_lines.push(format!("{}{}", before, after));
                    }
                }

                file.content = new_lines.join("\n");
            }
            OperationType::Replace { start, end, text } => {
                // Simplified: delete then insert
                let lines: Vec<&str> = file.content.lines().collect();
                let mut new_lines = Vec::new();

                for (i, line) in lines.iter().enumerate() {
                    if i < start.line as usize || i > end.line as usize {
                        new_lines.push(line.to_string());
                    } else if i == start.line as usize {
                        let before = &line[..start.column.min(line.len() as u32) as usize];
                        new_lines.push(format!("{}{}", before, text));
                    }
                }

                file.content = new_lines.join("\n");
            }
        }

        file.version += 1;
        file.modified_at = Utc::now();

        let _ = self
            .event_tx
            .send((session_id.to_string(), SessionEvent::EditMade(operation)));

        Ok(())
    }

    /// Get suggestions from AI navigator.
    pub async fn get_suggestions(
        &self,
        session_id: &str,
        file_id: &str,
        cursor: &CursorPosition,
    ) -> Result<Vec<Suggestion>> {
        let sessions = self.sessions.read().await;
        let session = sessions
            .get(session_id)
            .ok_or_else(|| PairError::SessionNotFound(session_id.to_string()))?;

        let file = session
            .files
            .iter()
            .find(|f| f.id == file_id)
            .ok_or_else(|| PairError::OperationFailed("File not found".to_string()))?;

        self.provider.suggest(file, cursor).await
    }

    /// Subscribe to session events.
    pub fn subscribe(&self) -> broadcast::Receiver<(String, SessionEvent)> {
        self.event_tx.subscribe()
    }

    /// Get a session.
    pub async fn get_session(&self, session_id: &str) -> Option<Session> {
        let sessions = self.sessions.read().await;
        sessions.get(session_id).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockProvider;

    #[async_trait]
    impl PairProvider for MockProvider {
        async fn suggest(
            &self,
            file: &SharedFile,
            _cursor: &CursorPosition,
        ) -> Result<Vec<Suggestion>> {
            Ok(vec![Suggestion {
                id: Uuid::new_v4().to_string(),
                file_id: file.id.clone(),
                suggestion_type: SuggestionType::Completion,
                position: Position { line: 0, column: 0 },
                content: "suggested_code()".to_string(),
                explanation: "Auto-complete suggestion".to_string(),
                confidence: 0.85,
                created_at: Utc::now(),
            }])
        }

        async fn review(&self, _file: &SharedFile) -> Result<Vec<Suggestion>> {
            Ok(vec![])
        }

        async fn explain(&self, _file: &SharedFile, _selection: &Selection) -> Result<String> {
            Ok("This code does something".to_string())
        }
    }

    #[tokio::test]
    async fn test_create_session() {
        let provider = Arc::new(MockProvider);
        let coordinator = PairCoordinator::new(provider);

        let session = coordinator
            .create_session("Test Session", SessionConfig::default())
            .await
            .unwrap();
        assert_eq!(session.name, "Test Session");
        assert_eq!(session.state, SessionState::Active);
    }

    #[tokio::test]
    async fn test_join_session() {
        let provider = Arc::new(MockProvider);
        let coordinator = PairCoordinator::new(provider);

        let session = coordinator
            .create_session("Test", SessionConfig::default())
            .await
            .unwrap();
        let participant = coordinator
            .join_session(&session.id, "Alice", ParticipantRole::Driver)
            .await
            .unwrap();

        assert_eq!(participant.name, "Alice");
        assert_eq!(participant.role, ParticipantRole::Driver);
    }

    #[tokio::test]
    async fn test_add_file() {
        let provider = Arc::new(MockProvider);
        let coordinator = PairCoordinator::new(provider);

        let session = coordinator
            .create_session("Test", SessionConfig::default())
            .await
            .unwrap();
        let file = coordinator
            .add_file(&session.id, "test.rs", "fn main() {}", "rust")
            .await
            .unwrap();

        assert_eq!(file.path, "test.rs");
        assert_eq!(file.language, "rust");
    }

    #[tokio::test]
    async fn test_get_suggestions() {
        let provider = Arc::new(MockProvider);
        let coordinator = PairCoordinator::new(provider);

        let session = coordinator
            .create_session("Test", SessionConfig::default())
            .await
            .unwrap();
        let file = coordinator
            .add_file(&session.id, "test.rs", "fn main() {}", "rust")
            .await
            .unwrap();

        let cursor = CursorPosition {
            file_id: file.id.clone(),
            line: 0,
            column: 0,
        };
        let suggestions = coordinator
            .get_suggestions(&session.id, &file.id, &cursor)
            .await
            .unwrap();

        assert!(!suggestions.is_empty());
    }

    #[test]
    fn test_suggestion_types() {
        let completion = SuggestionType::Completion;
        let security = SuggestionType::Security;

        let _ = serde_json::to_string(&completion).unwrap();
        let _ = serde_json::to_string(&security).unwrap();
    }
}
