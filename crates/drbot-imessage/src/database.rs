//! iMessage database access.
//!
//! Reads messages from the macOS Messages SQLite database.

use chrono::{DateTime, TimeZone, Utc};
use rusqlite::{Connection, OpenFlags};
use std::path::PathBuf;
use tracing::{debug, warn};

/// A message from the database.
#[derive(Debug, Clone)]
pub struct DbMessage {
    /// Row ID (unique identifier).
    pub rowid: i64,
    /// Message GUID.
    pub guid: String,
    /// Message text.
    pub text: Option<String>,
    /// Sender handle ID.
    pub handle_id: i64,
    /// Whether the message is from me.
    pub is_from_me: bool,
    /// Date sent (Apple timestamp).
    pub date: i64,
    /// Chat ID.
    pub chat_id: Option<i64>,
    /// Sender identifier (phone/email).
    pub sender: Option<String>,
}

impl DbMessage {
    /// Convert Apple timestamp to DateTime.
    pub fn datetime(&self) -> DateTime<Utc> {
        // Apple uses nanoseconds since 2001-01-01
        // Convert to Unix timestamp
        let apple_epoch = 978307200i64; // Unix timestamp of 2001-01-01
        let unix_nanos = self.date + (apple_epoch * 1_000_000_000);
        let unix_secs = unix_nanos / 1_000_000_000;
        match Utc.timestamp_opt(unix_secs, 0) {
            chrono::LocalResult::Single(dt) => dt,
            _ => Utc::now(),
        }
    }
}

/// A chat from the database.
#[derive(Debug, Clone)]
pub struct DbChat {
    /// Row ID.
    pub rowid: i64,
    /// Chat GUID.
    pub guid: String,
    /// Chat identifier.
    pub chat_identifier: String,
    /// Display name.
    pub display_name: Option<String>,
}

/// Database reader for iMessage.
pub struct MessageDatabase {
    /// Path to the chat.db file.
    pub db_path: PathBuf,
}

impl MessageDatabase {
    /// Create a new database reader with the default path.
    pub fn new() -> drbot_core::Result<Self> {
        let db_path = Self::default_path()?;
        Ok(Self { db_path })
    }

    /// Create with a custom path.
    pub fn with_path(path: PathBuf) -> Self {
        Self { db_path: path }
    }

    /// Get the default database path.
    pub fn default_path() -> drbot_core::Result<PathBuf> {
        let home = dirs::home_dir()
            .ok_or_else(|| drbot_core::Error::NotFound("Home directory not found".to_string()))?;

        let path = home.join("Library/Messages/chat.db");

        if !path.exists() {
            return Err(drbot_core::Error::NotFound(format!(
                "Messages database not found at {}",
                path.display()
            )));
        }

        Ok(path)
    }

    /// Open the database in read-only mode.
    fn open(&self) -> drbot_core::Result<Connection> {
        Connection::open_with_flags(&self.db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|e| drbot_core::Error::Internal(format!("Failed to open database: {}", e)))
    }

    /// Get messages after a certain row ID.
    pub fn get_messages_after(&self, after_rowid: i64) -> drbot_core::Result<Vec<DbMessage>> {
        let conn = self.open()?;

        let mut stmt = conn
            .prepare(
                r#"
                SELECT
                    m.ROWID,
                    m.guid,
                    m.text,
                    m.handle_id,
                    m.is_from_me,
                    m.date,
                    cmj.chat_id,
                    h.id as sender
                FROM message m
                LEFT JOIN chat_message_join cmj ON m.ROWID = cmj.message_id
                LEFT JOIN handle h ON m.handle_id = h.ROWID
                WHERE m.ROWID > ?
                ORDER BY m.ROWID ASC
                LIMIT 100
                "#,
            )
            .map_err(|e| drbot_core::Error::Internal(format!("SQL prepare failed: {}", e)))?;

        let rows = stmt
            .query_map([after_rowid], |row| {
                Ok(DbMessage {
                    rowid: row.get(0)?,
                    guid: row.get(1)?,
                    text: row.get(2)?,
                    handle_id: row.get(3)?,
                    is_from_me: row.get::<_, i32>(4)? != 0,
                    date: row.get(5)?,
                    chat_id: row.get(6)?,
                    sender: row.get(7)?,
                })
            })
            .map_err(|e| drbot_core::Error::Internal(format!("Query failed: {}", e)))?;

        let mut messages = Vec::new();
        for row in rows {
            match row {
                Ok(msg) => messages.push(msg),
                Err(e) => warn!("Failed to read message row: {}", e),
            }
        }

        debug!(
            "Retrieved {} messages after rowid {}",
            messages.len(),
            after_rowid
        );
        Ok(messages)
    }

    /// Get the latest message row ID.
    pub fn get_latest_rowid(&self) -> drbot_core::Result<i64> {
        let conn = self.open()?;

        let rowid: i64 = conn
            .query_row("SELECT MAX(ROWID) FROM message", [], |row| row.get(0))
            .map_err(|e| drbot_core::Error::Internal(format!("Query failed: {}", e)))?;

        Ok(rowid)
    }

    /// Get all chats.
    pub fn get_chats(&self) -> drbot_core::Result<Vec<DbChat>> {
        let conn = self.open()?;

        let mut stmt = conn
            .prepare(
                r#"
                SELECT ROWID, guid, chat_identifier, display_name
                FROM chat
                ORDER BY ROWID DESC
                "#,
            )
            .map_err(|e| drbot_core::Error::Internal(format!("SQL prepare failed: {}", e)))?;

        let rows = stmt
            .query_map([], |row| {
                Ok(DbChat {
                    rowid: row.get(0)?,
                    guid: row.get(1)?,
                    chat_identifier: row.get(2)?,
                    display_name: row.get(3)?,
                })
            })
            .map_err(|e| drbot_core::Error::Internal(format!("Query failed: {}", e)))?;

        let mut chats = Vec::new();
        for row in rows {
            match row {
                Ok(chat) => chats.push(chat),
                Err(e) => warn!("Failed to read chat row: {}", e),
            }
        }

        Ok(chats)
    }

    /// Get chat identifier by chat ID.
    pub fn get_chat_identifier(&self, chat_id: i64) -> drbot_core::Result<Option<String>> {
        let conn = self.open()?;

        let result: Result<String, _> = conn.query_row(
            "SELECT chat_identifier FROM chat WHERE ROWID = ?",
            [chat_id],
            |row| row.get(0),
        );

        match result {
            Ok(id) => Ok(Some(id)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(drbot_core::Error::Internal(format!("Query failed: {}", e))),
        }
    }
}

impl Default for MessageDatabase {
    fn default() -> Self {
        Self::new().unwrap_or_else(|_| Self {
            db_path: PathBuf::from("/nonexistent"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Datelike;

    #[test]
    fn test_apple_timestamp() {
        let msg = DbMessage {
            rowid: 1,
            guid: "test".to_string(),
            text: Some("hello".to_string()),
            handle_id: 1,
            is_from_me: false,
            date: 700000000000000000, // Some timestamp
            chat_id: Some(1),
            sender: Some("+1234567890".to_string()),
        };

        let dt = msg.datetime();
        assert!(dt.year() > 2020);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_default_path() {
        // This may fail if running in CI without home directory
        let result = MessageDatabase::default_path();
        // Just check it returns something or a proper error
        match result {
            Ok(path) => assert!(path.to_string_lossy().contains("chat.db")),
            Err(_) => {} // OK if database not found
        }
    }
}
