//! Collaborative session management.

use crate::participant::{Participant, ParticipantRole};
use crate::permissions::Permissions;
use crate::{CollabConfig, CollabError, CollabMessage, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Session state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    /// Session is active.
    Active,
    /// Session is paused.
    Paused,
    /// Session has ended.
    Ended,
}

/// A collaborative session.
#[derive(Debug)]
pub struct CollabSession {
    /// Session ID.
    pub id: Uuid,
    /// Session name.
    pub name: String,
    /// Description.
    pub description: Option<String>,
    /// Current state.
    state: Arc<RwLock<SessionState>>,
    /// Participants.
    participants: Arc<RwLock<HashMap<String, Participant>>>,
    /// Messages.
    messages: Arc<RwLock<Vec<CollabMessage>>>,
    /// Configuration.
    config: CollabConfig,
    /// Created timestamp.
    pub created_at: DateTime<Utc>,
    /// Last activity timestamp.
    last_activity: Arc<RwLock<DateTime<Utc>>>,
    /// Host user ID.
    pub host_id: String,
}

impl CollabSession {
    /// Create a new session.
    pub fn new(name: &str, host_id: &str, config: CollabConfig) -> Self {
        let now = Utc::now();

        let mut session = Self {
            id: Uuid::new_v4(),
            name: name.to_string(),
            description: None,
            state: Arc::new(RwLock::new(SessionState::Active)),
            participants: Arc::new(RwLock::new(HashMap::new())),
            messages: Arc::new(RwLock::new(Vec::new())),
            config,
            created_at: now,
            last_activity: Arc::new(RwLock::new(now)),
            host_id: host_id.to_string(),
        };

        // Add host as first participant
        let host = Participant::new(host_id, host_id).with_role(ParticipantRole::Host);

        // We can't use async in new(), so we'll skip adding for now
        // The host should be added after creation

        session
    }

    /// Get session state.
    pub async fn state(&self) -> SessionState {
        *self.state.read().await
    }

    /// Set session state.
    pub async fn set_state(&self, state: SessionState) {
        *self.state.write().await = state;
        *self.last_activity.write().await = Utc::now();
    }

    /// Add a participant.
    pub async fn add_participant(&self, participant: Participant) -> Result<()> {
        let mut participants = self.participants.write().await;

        if participants.len() >= self.config.max_participants {
            return Err(CollabError::PermissionDenied("Session is full".to_string()));
        }

        participants.insert(participant.id.clone(), participant);
        *self.last_activity.write().await = Utc::now();

        Ok(())
    }

    /// Remove a participant.
    pub async fn remove_participant(&self, user_id: &str) -> Result<Participant> {
        let mut participants = self.participants.write().await;

        participants
            .remove(user_id)
            .ok_or_else(|| CollabError::ParticipantNotFound(user_id.to_string()))
    }

    /// Get a participant.
    pub async fn get_participant(&self, user_id: &str) -> Option<Participant> {
        let participants = self.participants.read().await;
        participants.get(user_id).cloned()
    }

    /// List participants.
    pub async fn list_participants(&self) -> Vec<Participant> {
        let participants = self.participants.read().await;
        participants.values().cloned().collect()
    }

    /// Get participant count.
    pub async fn participant_count(&self) -> usize {
        self.participants.read().await.len()
    }

    /// Add a message.
    pub async fn add_message(&self, message: CollabMessage) -> Result<()> {
        // Check if sender is a participant
        let participants = self.participants.read().await;
        if !participants.contains_key(&message.sender_id) {
            return Err(CollabError::ParticipantNotFound(message.sender_id.clone()));
        }
        drop(participants);

        let mut messages = self.messages.write().await;
        messages.push(message);
        *self.last_activity.write().await = Utc::now();

        Ok(())
    }

    /// Get messages.
    pub async fn get_messages(&self, limit: Option<usize>) -> Vec<CollabMessage> {
        let messages = self.messages.read().await;

        if let Some(limit) = limit {
            messages
                .iter()
                .rev()
                .take(limit)
                .cloned()
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect()
        } else {
            messages.clone()
        }
    }

    /// Get message count.
    pub async fn message_count(&self) -> usize {
        self.messages.read().await.len()
    }

    /// Check if session has timed out.
    pub async fn is_timed_out(&self) -> bool {
        let last_activity = *self.last_activity.read().await;
        let elapsed = Utc::now().signed_duration_since(last_activity);
        elapsed.num_seconds() as u64 > self.config.session_timeout_secs
    }

    /// Update last activity.
    pub async fn touch(&self) {
        *self.last_activity.write().await = Utc::now();
    }

    /// Get last activity time.
    pub async fn last_activity(&self) -> DateTime<Utc> {
        *self.last_activity.read().await
    }

    /// End the session.
    pub async fn end(&self) {
        *self.state.write().await = SessionState::Ended;
    }

    /// Pause the session.
    pub async fn pause(&self) {
        *self.state.write().await = SessionState::Paused;
    }

    /// Resume the session.
    pub async fn resume(&self) {
        let mut state = self.state.write().await;
        if *state == SessionState::Paused {
            *state = SessionState::Active;
        }
    }

    /// Check if a user can perform an action.
    pub async fn can_perform(&self, user_id: &str, action: &str) -> bool {
        let participants = self.participants.read().await;

        if let Some(participant) = participants.get(user_id) {
            participant.permissions.has_permission(action)
        } else {
            false
        }
    }

    /// Get session info.
    pub async fn info(&self) -> SessionInfo {
        SessionInfo {
            id: self.id,
            name: self.name.clone(),
            description: self.description.clone(),
            state: *self.state.read().await,
            participant_count: self.participants.read().await.len(),
            message_count: self.messages.read().await.len(),
            created_at: self.created_at,
            last_activity: *self.last_activity.read().await,
            host_id: self.host_id.clone(),
        }
    }
}

/// Session info (serializable summary).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    /// Session ID.
    pub id: Uuid,
    /// Session name.
    pub name: String,
    /// Description.
    pub description: Option<String>,
    /// Current state.
    pub state: SessionState,
    /// Number of participants.
    pub participant_count: usize,
    /// Number of messages.
    pub message_count: usize,
    /// Created timestamp.
    pub created_at: DateTime<Utc>,
    /// Last activity timestamp.
    pub last_activity: DateTime<Utc>,
    /// Host user ID.
    pub host_id: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_session_creation() {
        let config = CollabConfig::default();
        let session = CollabSession::new("Test Session", "host1", config);

        assert_eq!(session.name, "Test Session");
        assert_eq!(session.host_id, "host1");
        assert_eq!(session.state().await, SessionState::Active);
    }

    #[tokio::test]
    async fn test_session_participants() {
        let config = CollabConfig::default();
        let session = CollabSession::new("Test", "host1", config);

        let participant = Participant::new("user1", "User One");
        session.add_participant(participant).await.unwrap();

        assert_eq!(session.participant_count().await, 1);
        assert!(session.get_participant("user1").await.is_some());
    }

    #[tokio::test]
    async fn test_session_messages() {
        let config = CollabConfig::default();
        let session = CollabSession::new("Test", "host1", config);

        let participant = Participant::new("user1", "User");
        session.add_participant(participant).await.unwrap();

        let message = CollabMessage::new(session.id, "user1", "Hello!");
        session.add_message(message).await.unwrap();

        assert_eq!(session.message_count().await, 1);
    }
}
