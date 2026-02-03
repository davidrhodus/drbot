//! Session management for drbot.
//!
//! This crate provides session storage and retrieval using SQLite.

mod store;

pub use store::SqliteSessionStore;

use async_trait::async_trait;
use drbot_core::session::Session;
use drbot_core::Result;
use uuid::Uuid;

/// Options for listing sessions.
#[derive(Debug, Clone, Default)]
pub struct ListOptions {
    /// Maximum number of sessions to return.
    pub limit: Option<usize>,
    /// Offset for pagination.
    pub offset: Option<usize>,
    /// Filter by user ID.
    pub user_id: Option<Uuid>,
    /// Filter by channel type.
    pub channel_type: Option<String>,
    /// Include archived sessions.
    pub include_archived: bool,
}

/// Trait for session storage backends.
#[async_trait]
pub trait SessionStore: Send + Sync {
    /// Create a new session.
    async fn create(&self, session: &Session) -> Result<()>;

    /// Get a session by ID.
    async fn get(&self, id: Uuid) -> Result<Option<Session>>;

    /// Get a session by channel type and channel ID.
    async fn get_by_channel(&self, channel_type: &str, channel_id: &str)
        -> Result<Option<Session>>;

    /// Update a session.
    async fn update(&self, session: &Session) -> Result<()>;

    /// Delete a session.
    async fn delete(&self, id: Uuid) -> Result<()>;

    /// List sessions with options.
    async fn list(&self, options: ListOptions) -> Result<Vec<Session>>;

    /// Get or create a session for a channel.
    async fn get_or_create(
        &self,
        user_id: Uuid,
        channel_type: &str,
        channel_id: &str,
    ) -> Result<Session> {
        if let Some(session) = self.get_by_channel(channel_type, channel_id).await? {
            Ok(session)
        } else {
            let session = Session::new(user_id, channel_type, channel_id);
            self.create(&session).await?;
            Ok(session)
        }
    }
}
