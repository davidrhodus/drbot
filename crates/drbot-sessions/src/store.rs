//! SQLite session store implementation.

use crate::{ListOptions, SessionStore};
use async_trait::async_trait;
use drbot_core::message::Message;
use drbot_core::session::{Session, SessionMetadata, SessionState};
use drbot_core::{Error, Result};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use std::sync::Mutex;
use tracing::{debug, info};
use uuid::Uuid;

/// SQLite-based session store.
pub struct SqliteSessionStore {
    conn: Mutex<Connection>,
}

impl SqliteSessionStore {
    /// Create a new SQLite session store.
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();

        // Create parent directory if it doesn't exist
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| Error::Session(format!("Failed to create directory: {}", e)))?;
        }

        let conn = Connection::open(path)
            .map_err(|e| Error::Session(format!("Failed to open database: {}", e)))?;

        let store = Self {
            conn: Mutex::new(conn),
        };
        store.init_schema()?;

        info!(path = %path.display(), "Opened session database");
        Ok(store)
    }

    /// Create an in-memory session store (for testing).
    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()
            .map_err(|e| Error::Session(format!("Failed to open in-memory database: {}", e)))?;

        let store = Self {
            conn: Mutex::new(conn),
        };
        store.init_schema()?;

        Ok(store)
    }

    /// Initialize the database schema.
    fn init_schema(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY,
                user_id TEXT NOT NULL,
                workspace_id TEXT,
                channel_type TEXT NOT NULL,
                channel_id TEXT NOT NULL,
                title TEXT,
                model TEXT,
                system_prompt TEXT,
                state TEXT NOT NULL DEFAULT 'active',
                total_input_tokens INTEGER NOT NULL DEFAULT 0,
                total_output_tokens INTEGER NOT NULL DEFAULT 0,
                message_count INTEGER NOT NULL DEFAULT 0,
                tags TEXT NOT NULL DEFAULT '[]',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                UNIQUE(channel_type, channel_id)
            );

            CREATE TABLE IF NOT EXISTS messages (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                created_at TEXT NOT NULL,
                metadata TEXT NOT NULL DEFAULT '{}',
                FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS idx_sessions_channel ON sessions(channel_type, channel_id);
            CREATE INDEX IF NOT EXISTS idx_sessions_user ON sessions(user_id);
            CREATE INDEX IF NOT EXISTS idx_messages_session ON messages(session_id);
            "#,
        )
        .map_err(|e| Error::Session(format!("Failed to initialize schema: {}", e)))?;

        Ok(())
    }

    /// Serialize a session to database row.
    fn session_to_row(
        session: &Session,
    ) -> (
        String,
        String,
        Option<String>,
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        String,
        i64,
        i64,
        i64,
        String,
        String,
        String,
    ) {
        (
            session.id.to_string(),
            session.user_id.to_string(),
            session.workspace_id.map(|id| id.to_string()),
            session.channel_type.clone(),
            session.channel_id.clone(),
            session.title.clone(),
            session.model.clone(),
            session.system_prompt.clone(),
            match session.state {
                SessionState::Active => "active",
                SessionState::Archived => "archived",
                SessionState::Deleted => "deleted",
            }
            .to_string(),
            session.metadata.total_input_tokens as i64,
            session.metadata.total_output_tokens as i64,
            session.metadata.message_count as i64,
            serde_json::to_string(&session.metadata.tags).unwrap_or_else(|_| "[]".to_string()),
            session.created_at.to_rfc3339(),
            session.updated_at.to_rfc3339(),
        )
    }

    /// Deserialize a session from database row.
    fn row_to_session(row: &rusqlite::Row) -> rusqlite::Result<Session> {
        let id: String = row.get(0)?;
        let user_id: String = row.get(1)?;
        let workspace_id: Option<String> = row.get(2)?;
        let channel_type: String = row.get(3)?;
        let channel_id: String = row.get(4)?;
        let title: Option<String> = row.get(5)?;
        let model: Option<String> = row.get(6)?;
        let system_prompt: Option<String> = row.get(7)?;
        let state: String = row.get(8)?;
        let total_input_tokens: i64 = row.get(9)?;
        let total_output_tokens: i64 = row.get(10)?;
        let message_count: i64 = row.get(11)?;
        let tags: String = row.get(12)?;
        let created_at: String = row.get(13)?;
        let updated_at: String = row.get(14)?;

        Ok(Session {
            id: Uuid::parse_str(&id).unwrap_or_default(),
            user_id: Uuid::parse_str(&user_id).unwrap_or_default(),
            workspace_id: workspace_id.and_then(|s| Uuid::parse_str(&s).ok()),
            channel_type,
            channel_id,
            title,
            model,
            system_prompt,
            messages: Vec::new(), // Messages loaded separately
            metadata: SessionMetadata {
                total_input_tokens: total_input_tokens as usize,
                total_output_tokens: total_output_tokens as usize,
                message_count: message_count as usize,
                tags: serde_json::from_str(&tags).unwrap_or_default(),
            },
            created_at: chrono::DateTime::parse_from_rfc3339(&created_at)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now()),
            updated_at: chrono::DateTime::parse_from_rfc3339(&updated_at)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now()),
            state: match state.as_str() {
                "archived" => SessionState::Archived,
                "deleted" => SessionState::Deleted,
                _ => SessionState::Active,
            },
        })
    }

    /// Load messages for a session.
    fn load_messages(&self, session_id: Uuid) -> Result<Vec<Message>> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn
            .prepare(
                "SELECT id, role, content, created_at, metadata FROM messages
                 WHERE session_id = ? ORDER BY created_at ASC",
            )
            .map_err(|e| Error::Session(format!("Failed to prepare statement: {}", e)))?;

        let messages = stmt
            .query_map(params![session_id.to_string()], |row| {
                let id: String = row.get(0)?;
                let role: String = row.get(1)?;
                let content: String = row.get(2)?;
                let created_at: String = row.get(3)?;
                let metadata: String = row.get(4)?;

                Ok((id, role, content, created_at, metadata))
            })
            .map_err(|e| Error::Session(format!("Failed to query messages: {}", e)))?
            .filter_map(|r| r.ok())
            .filter_map(|(id, role, content, created_at, metadata)| {
                let role = match role.as_str() {
                    "system" => drbot_core::message::Role::System,
                    "user" => drbot_core::message::Role::User,
                    "assistant" => drbot_core::message::Role::Assistant,
                    _ => return None,
                };

                let content: Vec<drbot_core::message::Content> =
                    serde_json::from_str(&content).ok()?;
                let metadata: serde_json::Map<String, serde_json::Value> =
                    serde_json::from_str(&metadata).unwrap_or_default();

                Some(Message {
                    id: Uuid::parse_str(&id).unwrap_or_default(),
                    role,
                    content,
                    created_at: chrono::DateTime::parse_from_rfc3339(&created_at)
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                        .unwrap_or_else(|_| chrono::Utc::now()),
                    metadata,
                })
            })
            .collect();

        Ok(messages)
    }

    /// Save messages for a session.
    fn save_messages(&self, session_id: Uuid, messages: &[Message]) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        // Delete existing messages
        conn.execute(
            "DELETE FROM messages WHERE session_id = ?",
            params![session_id.to_string()],
        )
        .map_err(|e| Error::Session(format!("Failed to delete messages: {}", e)))?;

        // Insert new messages
        let mut stmt = conn
            .prepare(
                "INSERT INTO messages (id, session_id, role, content, created_at, metadata)
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .map_err(|e| Error::Session(format!("Failed to prepare statement: {}", e)))?;

        for msg in messages {
            let role = match msg.role {
                drbot_core::message::Role::System => "system",
                drbot_core::message::Role::User => "user",
                drbot_core::message::Role::Assistant => "assistant",
            };

            stmt.execute(params![
                msg.id.to_string(),
                session_id.to_string(),
                role,
                serde_json::to_string(&msg.content).unwrap_or_else(|_| "[]".to_string()),
                msg.created_at.to_rfc3339(),
                serde_json::to_string(&msg.metadata).unwrap_or_else(|_| "{}".to_string()),
            ])
            .map_err(|e| Error::Session(format!("Failed to insert message: {}", e)))?;
        }

        Ok(())
    }
}

#[async_trait]
impl SessionStore for SqliteSessionStore {
    async fn create(&self, session: &Session) -> Result<()> {
        let row = Self::session_to_row(session);

        {
            let conn = self.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO sessions (id, user_id, workspace_id, channel_type, channel_id,
                 title, model, system_prompt, state, total_input_tokens, total_output_tokens,
                 message_count, tags, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                params![
                    row.0, row.1, row.2, row.3, row.4, row.5, row.6, row.7, row.8, row.9, row.10,
                    row.11, row.12, row.13, row.14
                ],
            )
            .map_err(|e| Error::Session(format!("Failed to create session: {}", e)))?;
        }

        // Save messages
        if !session.messages.is_empty() {
            self.save_messages(session.id, &session.messages)?;
        }

        debug!(session_id = %session.id, "Created session");
        Ok(())
    }

    async fn get(&self, id: Uuid) -> Result<Option<Session>> {
        let session = {
            let conn = self.conn.lock().unwrap();
            conn.query_row(
                "SELECT id, user_id, workspace_id, channel_type, channel_id, title, model,
                 system_prompt, state, total_input_tokens, total_output_tokens, message_count,
                 tags, created_at, updated_at FROM sessions WHERE id = ?",
                params![id.to_string()],
                Self::row_to_session,
            )
            .optional()
            .map_err(|e| Error::Session(format!("Failed to get session: {}", e)))?
        };

        if let Some(mut session) = session {
            session.messages = self.load_messages(id)?;
            Ok(Some(session))
        } else {
            Ok(None)
        }
    }

    async fn get_by_channel(
        &self,
        channel_type: &str,
        channel_id: &str,
    ) -> Result<Option<Session>> {
        let session = {
            let conn = self.conn.lock().unwrap();
            conn.query_row(
                "SELECT id, user_id, workspace_id, channel_type, channel_id, title, model,
                 system_prompt, state, total_input_tokens, total_output_tokens, message_count,
                 tags, created_at, updated_at FROM sessions
                 WHERE channel_type = ? AND channel_id = ? AND state = 'active'",
                params![channel_type, channel_id],
                Self::row_to_session,
            )
            .optional()
            .map_err(|e| Error::Session(format!("Failed to get session: {}", e)))?
        };

        if let Some(mut session) = session {
            session.messages = self.load_messages(session.id)?;
            Ok(Some(session))
        } else {
            Ok(None)
        }
    }

    async fn update(&self, session: &Session) -> Result<()> {
        let row = Self::session_to_row(session);

        {
            let conn = self.conn.lock().unwrap();
            conn.execute(
                "UPDATE sessions SET user_id = ?, workspace_id = ?, channel_type = ?,
                 channel_id = ?, title = ?, model = ?, system_prompt = ?, state = ?,
                 total_input_tokens = ?, total_output_tokens = ?, message_count = ?,
                 tags = ?, updated_at = ? WHERE id = ?",
                params![
                    row.1, row.2, row.3, row.4, row.5, row.6, row.7, row.8, row.9, row.10, row.11,
                    row.12, row.14, row.0
                ],
            )
            .map_err(|e| Error::Session(format!("Failed to update session: {}", e)))?;
        }

        // Save messages
        self.save_messages(session.id, &session.messages)?;

        debug!(session_id = %session.id, "Updated session");
        Ok(())
    }

    async fn delete(&self, id: Uuid) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE sessions SET state = 'deleted', updated_at = ? WHERE id = ?",
            params![chrono::Utc::now().to_rfc3339(), id.to_string()],
        )
        .map_err(|e| Error::Session(format!("Failed to delete session: {}", e)))?;

        debug!(session_id = %id, "Deleted session");
        Ok(())
    }

    async fn list(&self, options: ListOptions) -> Result<Vec<Session>> {
        let conn = self.conn.lock().unwrap();

        let mut sql = String::from(
            "SELECT id, user_id, workspace_id, channel_type, channel_id, title, model,
             system_prompt, state, total_input_tokens, total_output_tokens, message_count,
             tags, created_at, updated_at FROM sessions WHERE 1=1",
        );

        if !options.include_archived {
            sql.push_str(" AND state = 'active'");
        } else {
            sql.push_str(" AND state != 'deleted'");
        }

        if options.user_id.is_some() {
            sql.push_str(" AND user_id = ?");
        }

        if options.channel_type.is_some() {
            sql.push_str(" AND channel_type = ?");
        }

        sql.push_str(" ORDER BY updated_at DESC");

        if let Some(limit) = options.limit {
            sql.push_str(&format!(" LIMIT {}", limit));
        }

        if let Some(offset) = options.offset {
            sql.push_str(&format!(" OFFSET {}", offset));
        }

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| Error::Session(format!("Failed to prepare statement: {}", e)))?;

        // Build params dynamically
        let mut param_values: Vec<String> = Vec::new();
        if let Some(user_id) = options.user_id {
            param_values.push(user_id.to_string());
        }
        if let Some(channel_type) = options.channel_type {
            param_values.push(channel_type);
        }

        let sessions: Vec<Session> = match param_values.len() {
            0 => stmt
                .query_map([], Self::row_to_session)
                .map_err(|e| Error::Session(format!("Failed to query sessions: {}", e)))?
                .filter_map(|r| r.ok())
                .collect(),
            1 => stmt
                .query_map(params![param_values[0]], Self::row_to_session)
                .map_err(|e| Error::Session(format!("Failed to query sessions: {}", e)))?
                .filter_map(|r| r.ok())
                .collect(),
            2 => stmt
                .query_map(
                    params![param_values[0], param_values[1]],
                    Self::row_to_session,
                )
                .map_err(|e| Error::Session(format!("Failed to query sessions: {}", e)))?
                .filter_map(|r| r.ok())
                .collect(),
            _ => Vec::new(),
        };

        Ok(sessions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_and_get_session() {
        let store = SqliteSessionStore::in_memory().unwrap();
        let user_id = Uuid::new_v4();
        let session = Session::new(user_id, "test", "channel_123");

        store.create(&session).await.unwrap();

        let retrieved = store.get(session.id).await.unwrap().unwrap();
        assert_eq!(retrieved.id, session.id);
        assert_eq!(retrieved.channel_type, "test");
        assert_eq!(retrieved.channel_id, "channel_123");
    }

    #[tokio::test]
    async fn test_get_by_channel() {
        let store = SqliteSessionStore::in_memory().unwrap();
        let user_id = Uuid::new_v4();
        let session = Session::new(user_id, "telegram", "chat_456");

        store.create(&session).await.unwrap();

        let retrieved = store
            .get_by_channel("telegram", "chat_456")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(retrieved.id, session.id);
    }

    #[tokio::test]
    async fn test_update_session_with_messages() {
        let store = SqliteSessionStore::in_memory().unwrap();
        let user_id = Uuid::new_v4();
        let mut session = Session::new(user_id, "test", "channel_789");

        store.create(&session).await.unwrap();

        session.add_message(Message::user("Hello"));
        session.add_message(Message::assistant("Hi there!"));
        store.update(&session).await.unwrap();

        let retrieved = store.get(session.id).await.unwrap().unwrap();
        assert_eq!(retrieved.messages.len(), 2);
        assert_eq!(retrieved.messages[0].text_content(), "Hello");
        assert_eq!(retrieved.messages[1].text_content(), "Hi there!");
    }

    #[tokio::test]
    async fn test_list_sessions() {
        let store = SqliteSessionStore::in_memory().unwrap();
        let user_id = Uuid::new_v4();

        for i in 0..5 {
            let session = Session::new(user_id, "test", &format!("channel_{}", i));
            store.create(&session).await.unwrap();
        }

        let sessions = store.list(ListOptions::default()).await.unwrap();
        assert_eq!(sessions.len(), 5);

        let sessions = store
            .list(ListOptions {
                limit: Some(2),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(sessions.len(), 2);
    }

    #[tokio::test]
    async fn test_get_or_create() {
        let store = SqliteSessionStore::in_memory().unwrap();
        let user_id = Uuid::new_v4();

        // First call creates
        let session1 = store
            .get_or_create(user_id, "test", "channel_abc")
            .await
            .unwrap();

        // Second call retrieves
        let session2 = store
            .get_or_create(user_id, "test", "channel_abc")
            .await
            .unwrap();

        assert_eq!(session1.id, session2.id);
    }
}
