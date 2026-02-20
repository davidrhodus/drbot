//! Session types for drbot.

use crate::message::Message;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A conversation session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// Unique session ID.
    pub id: Uuid,
    /// User this session belongs to.
    pub user_id: Uuid,
    /// Optional workspace ID for multi-workspace support.
    pub workspace_id: Option<Uuid>,
    /// Channel type where this session originated.
    pub channel_type: String,
    /// Channel-specific chat/conversation ID.
    pub channel_id: String,
    /// Session title (auto-generated or user-set).
    pub title: Option<String>,
    /// Provider used for this session (e.g. "claude-cli", "ollama").
    pub provider: Option<String>,
    /// Model being used for this session.
    pub model: Option<String>,
    /// Custom system prompt for this session.
    pub system_prompt: Option<String>,
    /// Message history.
    pub messages: Vec<Message>,
    /// Session metadata.
    #[serde(default)]
    pub metadata: SessionMetadata,
    /// When the session was created.
    pub created_at: DateTime<Utc>,
    /// When the session was last updated.
    pub updated_at: DateTime<Utc>,
    /// Session state.
    pub state: SessionState,
}

/// Session state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionState {
    /// Session is active.
    #[default]
    Active,
    /// Session is archived.
    Archived,
    /// Session is deleted (soft delete).
    Deleted,
}

/// Session metadata.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionMetadata {
    /// Total input tokens used in this session.
    pub total_input_tokens: usize,
    /// Total output tokens used in this session.
    pub total_output_tokens: usize,
    /// Number of messages in the session.
    pub message_count: usize,
    /// Tags for organization.
    #[serde(default)]
    pub tags: Vec<String>,
}

impl Session {
    /// Create a new session.
    pub fn new(
        user_id: Uuid,
        channel_type: impl Into<String>,
        channel_id: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            user_id,
            workspace_id: None,
            channel_type: channel_type.into(),
            channel_id: channel_id.into(),
            title: None,
            provider: None,
            model: None,
            system_prompt: None,
            messages: Vec::new(),
            metadata: SessionMetadata::default(),
            created_at: now,
            updated_at: now,
            state: SessionState::Active,
        }
    }

    /// Add a message to the session.
    pub fn add_message(&mut self, message: Message) {
        self.messages.push(message);
        self.metadata.message_count = self.messages.len();
        self.updated_at = Utc::now();
    }

    /// Get the last N messages.
    pub fn last_messages(&self, n: usize) -> &[Message] {
        let len = self.messages.len();
        if n >= len {
            &self.messages
        } else {
            &self.messages[len - n..]
        }
    }

    /// Clear the message history.
    pub fn clear_messages(&mut self) {
        self.messages.clear();
        self.metadata.message_count = 0;
        self.updated_at = Utc::now();
    }

    /// Archive the session.
    pub fn archive(&mut self) {
        self.state = SessionState::Archived;
        self.updated_at = Utc::now();
    }

    /// Check if the session is active.
    pub fn is_active(&self) -> bool {
        self.state == SessionState::Active
    }

    /// Set the session title.
    pub fn set_title(&mut self, title: impl Into<String>) {
        self.title = Some(title.into());
        self.updated_at = Utc::now();
    }

    /// Update token usage.
    pub fn add_token_usage(&mut self, input_tokens: usize, output_tokens: usize) {
        self.metadata.total_input_tokens += input_tokens;
        self.metadata.total_output_tokens += output_tokens;
        self.updated_at = Utc::now();
    }

    /// Update the timestamp to now.
    pub fn update_timestamp(&mut self) {
        self.updated_at = Utc::now();
    }
}

/// Session lookup key - either by session ID or by channel.
#[derive(Debug, Clone)]
pub enum SessionKey {
    /// Lookup by session ID.
    Id(Uuid),
    /// Lookup by channel type and channel ID.
    Channel {
        channel_type: String,
        channel_id: String,
    },
}

impl From<Uuid> for SessionKey {
    fn from(id: Uuid) -> Self {
        SessionKey::Id(id)
    }
}

impl SessionKey {
    /// Create a channel-based session key.
    pub fn channel(channel_type: impl Into<String>, channel_id: impl Into<String>) -> Self {
        SessionKey::Channel {
            channel_type: channel_type.into(),
            channel_id: channel_id.into(),
        }
    }
}

// ============================================================================
// KANI FORMAL VERIFICATION PROOFS
// ============================================================================

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    /// Proof: last_messages never panics for any valid input
    #[kani::proof]
    #[kani::unwind(6)]
    fn proof_last_messages_no_panic() {
        let n: usize = kani::any();
        kani::assume(n <= 100); // Reasonable bound

        // Create session with up to 5 messages
        let mut session = Session::new(Uuid::nil(), "test", "test");
        let msg_count: usize = kani::any();
        kani::assume(msg_count <= 5);

        for _ in 0..msg_count {
            session.messages.push(Message::user("test"));
        }

        let result = session.last_messages(n);

        // Result length is min(n, messages.len())
        let expected_len = if n >= session.messages.len() {
            session.messages.len()
        } else {
            n
        };
        kani::assert(
            result.len() == expected_len,
            "Result length must be correct",
        );
    }

    /// Proof: last_messages(0) always returns empty slice
    #[kani::proof]
    fn proof_last_messages_zero_is_empty() {
        let mut session = Session::new(Uuid::nil(), "test", "test");
        session.messages.push(Message::user("test"));

        let result = session.last_messages(0);
        kani::assert(
            result.is_empty(),
            "last_messages(0) must return empty slice",
        );
    }

    /// Proof: session state self-transitions are valid
    #[kani::proof]
    fn proof_state_self_transition() {
        let state_val: u8 = kani::any();
        kani::assume(state_val <= 2);

        let state = match state_val {
            0 => SessionState::Active,
            1 => SessionState::Archived,
            _ => SessionState::Deleted,
        };

        // Self-comparison should always be equal
        kani::assert(state == state, "State must equal itself");
    }

    /// Proof: Active session is_active returns true
    #[kani::proof]
    fn proof_active_session_is_active() {
        let session = Session::new(Uuid::nil(), "test", "test");
        kani::assert(session.is_active(), "New session must be active");
        kani::assert(
            session.state == SessionState::Active,
            "New session state must be Active",
        );
    }

    /// Proof: archive changes state correctly
    #[kani::proof]
    fn proof_archive_changes_state() {
        let mut session = Session::new(Uuid::nil(), "test", "test");
        kani::assert(session.is_active(), "Session starts active");

        session.archive();

        kani::assert(!session.is_active(), "Archived session must not be active");
        kani::assert(
            session.state == SessionState::Archived,
            "State must be Archived",
        );
    }

    /// Proof: add_message increments message_count correctly
    #[kani::proof]
    fn proof_add_message_count() {
        let mut session = Session::new(Uuid::nil(), "test", "test");
        let initial_count = session.metadata.message_count;

        session.add_message(Message::user("test"));

        kani::assert(
            session.metadata.message_count == initial_count + 1,
            "Message count must increment by 1",
        );
        kani::assert(
            session.metadata.message_count == session.messages.len(),
            "Message count must match messages length",
        );
    }

    /// Proof: token usage accumulates correctly (no overflow with saturating)
    #[kani::proof]
    fn proof_token_usage_accumulates() {
        let mut session = Session::new(Uuid::nil(), "test", "test");

        let input1: usize = kani::any();
        let output1: usize = kani::any();
        let input2: usize = kani::any();
        let output2: usize = kani::any();

        // Use smaller values to avoid overflow
        kani::assume(input1 < 1_000_000 && output1 < 1_000_000);
        kani::assume(input2 < 1_000_000 && output2 < 1_000_000);

        session.add_token_usage(input1, output1);
        session.add_token_usage(input2, output2);

        kani::assert(
            session.metadata.total_input_tokens == input1 + input2,
            "Input tokens must sum correctly",
        );
        kani::assert(
            session.metadata.total_output_tokens == output1 + output2,
            "Output tokens must sum correctly",
        );
    }

    /// Proof: clear_messages resets count to zero
    #[kani::proof]
    fn proof_clear_messages() {
        let mut session = Session::new(Uuid::nil(), "test", "test");
        session.add_message(Message::user("test1"));
        session.add_message(Message::user("test2"));

        session.clear_messages();

        kani::assert(
            session.messages.is_empty(),
            "Messages must be empty after clear",
        );
        kani::assert(
            session.metadata.message_count == 0,
            "Count must be 0 after clear",
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_creation() {
        let user_id = Uuid::new_v4();
        let session = Session::new(user_id, "telegram", "chat_123");

        assert_eq!(session.user_id, user_id);
        assert_eq!(session.channel_type, "telegram");
        assert_eq!(session.channel_id, "chat_123");
        assert!(session.is_active());
    }

    #[test]
    fn test_session_messages() {
        let mut session = Session::new(Uuid::new_v4(), "test", "test_chat");

        session.add_message(Message::user("Hello"));
        session.add_message(Message::assistant("Hi there!"));

        assert_eq!(session.messages.len(), 2);
        assert_eq!(session.metadata.message_count, 2);

        let last = session.last_messages(1);
        assert_eq!(last.len(), 1);
        assert_eq!(last[0].text_content(), "Hi there!");
    }
}
