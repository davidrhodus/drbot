//! Persistence layer for pairing data.

use crate::{
    Allowlist, AllowlistEntry, ApprovalCode, PairedSender, PairingError, PendingApproval, Result,
};
use async_trait::async_trait;
use rusqlite::{params, Connection};
use std::path::Path;
use std::sync::Mutex;
use uuid::Uuid;

/// Trait for pairing data persistence.
#[async_trait]
pub trait PairingStore: Send + Sync {
    /// Save a pending approval.
    async fn save_pending(&self, pending: &PendingApproval) -> Result<()>;

    /// Get a pending approval by sender and channel.
    async fn get_pending(&self, sender_id: &str, channel: &str) -> Result<Option<PendingApproval>>;

    /// Delete a pending approval.
    async fn delete_pending(&self, id: Uuid) -> Result<()>;

    /// Delete expired pending approvals.
    async fn delete_expired_pending(&self) -> Result<usize>;

    /// Save a paired sender.
    async fn save_paired(&self, sender: &PairedSender) -> Result<()>;

    /// Get a paired sender.
    async fn get_paired(&self, sender_id: &str, channel: &str) -> Result<Option<PairedSender>>;

    /// List all paired senders for a channel.
    async fn list_paired(&self, channel: Option<&str>) -> Result<Vec<PairedSender>>;

    /// Delete a paired sender.
    async fn delete_paired(&self, sender_id: &str, channel: &str) -> Result<bool>;

    /// Load the allowlist.
    async fn load_allowlist(&self) -> Result<Allowlist>;

    /// Save the allowlist.
    async fn save_allowlist(&self, allowlist: &Allowlist) -> Result<()>;

    /// Add an allowlist entry.
    async fn add_allowlist_entry(&self, entry: &AllowlistEntry) -> Result<()>;

    /// Remove an allowlist entry.
    async fn remove_allowlist_entry(&self, sender_id: &str, channel: Option<&str>) -> Result<bool>;
}

/// SQLite-based pairing store.
pub struct SqlitePairingStore {
    conn: Mutex<Connection>,
}

impl SqlitePairingStore {
    /// Create a new SQLite pairing store.
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let conn =
            Connection::open(path).map_err(|e| PairingError::DatabaseError(e.to_string()))?;

        let store = Self {
            conn: Mutex::new(conn),
        };
        store.init_schema()?;
        Ok(store)
    }

    /// Create an in-memory store (useful for testing).
    pub fn in_memory() -> Result<Self> {
        let conn =
            Connection::open_in_memory().map_err(|e| PairingError::DatabaseError(e.to_string()))?;

        let store = Self {
            conn: Mutex::new(conn),
        };
        store.init_schema()?;
        Ok(store)
    }

    fn init_schema(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS pending_approvals (
                id TEXT PRIMARY KEY,
                sender_id TEXT NOT NULL,
                channel TEXT NOT NULL,
                code TEXT NOT NULL,
                created_at TEXT NOT NULL,
                expires_at TEXT NOT NULL,
                attempts INTEGER NOT NULL DEFAULT 0,
                max_attempts INTEGER NOT NULL DEFAULT 3,
                consumed INTEGER NOT NULL DEFAULT 0,
                UNIQUE(sender_id, channel)
            );

            CREATE TABLE IF NOT EXISTS paired_senders (
                sender_id TEXT NOT NULL,
                channel TEXT NOT NULL,
                paired_at TEXT NOT NULL,
                approved_by TEXT,
                metadata TEXT,
                PRIMARY KEY(sender_id, channel)
            );

            CREATE TABLE IF NOT EXISTS allowlist (
                sender_id TEXT NOT NULL,
                channel TEXT,
                display_name TEXT,
                added_at TEXT NOT NULL,
                added_by TEXT,
                expires_at TEXT,
                active INTEGER NOT NULL DEFAULT 1,
                notes TEXT,
                UNIQUE(sender_id, channel)
            );

            CREATE INDEX IF NOT EXISTS idx_pending_expires ON pending_approvals(expires_at);
            CREATE INDEX IF NOT EXISTS idx_allowlist_sender ON allowlist(sender_id);
            "#,
        )
        .map_err(|e| PairingError::DatabaseError(e.to_string()))?;

        Ok(())
    }
}

#[async_trait]
impl PairingStore for SqlitePairingStore {
    async fn save_pending(&self, pending: &PendingApproval) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        conn.execute(
            r#"
            INSERT OR REPLACE INTO pending_approvals
                (id, sender_id, channel, code, created_at, expires_at, attempts, max_attempts, consumed)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            "#,
            params![
                pending.id.to_string(),
                pending.sender_id,
                pending.channel,
                pending.code.code,
                pending.code.created_at.to_rfc3339(),
                pending.code.expires_at.to_rfc3339(),
                pending.attempts,
                pending.max_attempts,
                pending.consumed as i32,
            ],
        )
        .map_err(|e| PairingError::DatabaseError(e.to_string()))?;

        Ok(())
    }

    async fn get_pending(&self, sender_id: &str, channel: &str) -> Result<Option<PendingApproval>> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn
            .prepare(
                r#"
                SELECT id, sender_id, channel, code, created_at, expires_at, attempts, max_attempts, consumed
                FROM pending_approvals
                WHERE sender_id = ?1 AND channel = ?2
                "#,
            )
            .map_err(|e| PairingError::DatabaseError(e.to_string()))?;

        let result = stmt.query_row(params![sender_id, channel], |row| {
            let id: String = row.get(0)?;
            let sender_id: String = row.get(1)?;
            let channel: String = row.get(2)?;
            let code_str: String = row.get(3)?;
            let created_at: String = row.get(4)?;
            let expires_at: String = row.get(5)?;
            let attempts: u32 = row.get(6)?;
            let max_attempts: u32 = row.get(7)?;
            let consumed: i32 = row.get(8)?;

            Ok((
                id,
                sender_id,
                channel,
                code_str,
                created_at,
                expires_at,
                attempts,
                max_attempts,
                consumed,
            ))
        });

        match result {
            Ok((
                id,
                sender_id,
                channel,
                code_str,
                created_at,
                expires_at,
                attempts,
                max_attempts,
                consumed,
            )) => {
                let code = ApprovalCode {
                    code: code_str,
                    created_at: chrono::DateTime::parse_from_rfc3339(&created_at)
                        .map_err(|e| PairingError::DatabaseError(e.to_string()))?
                        .with_timezone(&chrono::Utc),
                    expires_at: chrono::DateTime::parse_from_rfc3339(&expires_at)
                        .map_err(|e| PairingError::DatabaseError(e.to_string()))?
                        .with_timezone(&chrono::Utc),
                };

                Ok(Some(PendingApproval {
                    id: Uuid::parse_str(&id)
                        .map_err(|e| PairingError::DatabaseError(e.to_string()))?,
                    sender_id,
                    channel,
                    code,
                    attempts,
                    max_attempts,
                    consumed: consumed != 0,
                }))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(PairingError::DatabaseError(e.to_string())),
        }
    }

    async fn delete_pending(&self, id: Uuid) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        conn.execute(
            "DELETE FROM pending_approvals WHERE id = ?1",
            params![id.to_string()],
        )
        .map_err(|e| PairingError::DatabaseError(e.to_string()))?;

        Ok(())
    }

    async fn delete_expired_pending(&self) -> Result<usize> {
        let conn = self.conn.lock().unwrap();

        let now = chrono::Utc::now().to_rfc3339();
        let deleted = conn
            .execute(
                "DELETE FROM pending_approvals WHERE expires_at < ?1",
                params![now],
            )
            .map_err(|e| PairingError::DatabaseError(e.to_string()))?;

        Ok(deleted)
    }

    async fn save_paired(&self, sender: &PairedSender) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        let metadata = sender
            .metadata
            .as_ref()
            .map(|m| serde_json::to_string(m).unwrap_or_default());

        conn.execute(
            r#"
            INSERT OR REPLACE INTO paired_senders
                (sender_id, channel, paired_at, approved_by, metadata)
            VALUES (?1, ?2, ?3, ?4, ?5)
            "#,
            params![
                sender.sender_id,
                sender.channel,
                sender.paired_at.to_rfc3339(),
                sender.approved_by,
                metadata,
            ],
        )
        .map_err(|e| PairingError::DatabaseError(e.to_string()))?;

        Ok(())
    }

    async fn get_paired(&self, sender_id: &str, channel: &str) -> Result<Option<PairedSender>> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn
            .prepare(
                r#"
                SELECT sender_id, channel, paired_at, approved_by, metadata
                FROM paired_senders
                WHERE sender_id = ?1 AND channel = ?2
                "#,
            )
            .map_err(|e| PairingError::DatabaseError(e.to_string()))?;

        let result = stmt.query_row(params![sender_id, channel], |row| {
            let sender_id: String = row.get(0)?;
            let channel: String = row.get(1)?;
            let paired_at: String = row.get(2)?;
            let approved_by: Option<String> = row.get(3)?;
            let metadata: Option<String> = row.get(4)?;
            Ok((sender_id, channel, paired_at, approved_by, metadata))
        });

        match result {
            Ok((sender_id, channel, paired_at, approved_by, metadata)) => Ok(Some(PairedSender {
                sender_id,
                channel,
                paired_at: chrono::DateTime::parse_from_rfc3339(&paired_at)
                    .map_err(|e| PairingError::DatabaseError(e.to_string()))?
                    .with_timezone(&chrono::Utc),
                approved_by,
                metadata: metadata.and_then(|m| serde_json::from_str(&m).ok()),
            })),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(PairingError::DatabaseError(e.to_string())),
        }
    }

    async fn list_paired(&self, channel: Option<&str>) -> Result<Vec<PairedSender>> {
        let conn = self.conn.lock().unwrap();

        let mut senders = Vec::new();

        let (sql, channel_param): (&str, Option<&str>) = match channel {
            Some(ch) => (
                "SELECT sender_id, channel, paired_at, approved_by, metadata FROM paired_senders WHERE channel = ?1",
                Some(ch),
            ),
            None => (
                "SELECT sender_id, channel, paired_at, approved_by, metadata FROM paired_senders",
                None,
            ),
        };

        let mut stmt = conn
            .prepare(sql)
            .map_err(|e| PairingError::DatabaseError(e.to_string()))?;

        let rows = if let Some(ch) = channel_param {
            stmt.query(params![ch])
        } else {
            stmt.query([])
        }
        .map_err(|e| PairingError::DatabaseError(e.to_string()))?;

        let mut rows = rows;
        while let Some(row) = rows
            .next()
            .map_err(|e| PairingError::DatabaseError(e.to_string()))?
        {
            let sender_id: String = row
                .get(0)
                .map_err(|e| PairingError::DatabaseError(e.to_string()))?;
            let channel: String = row
                .get(1)
                .map_err(|e| PairingError::DatabaseError(e.to_string()))?;
            let paired_at: String = row
                .get(2)
                .map_err(|e| PairingError::DatabaseError(e.to_string()))?;
            let approved_by: Option<String> = row
                .get(3)
                .map_err(|e| PairingError::DatabaseError(e.to_string()))?;
            let metadata: Option<String> = row
                .get(4)
                .map_err(|e| PairingError::DatabaseError(e.to_string()))?;

            senders.push(PairedSender {
                sender_id,
                channel,
                paired_at: chrono::DateTime::parse_from_rfc3339(&paired_at)
                    .map_err(|e| PairingError::DatabaseError(e.to_string()))?
                    .with_timezone(&chrono::Utc),
                approved_by,
                metadata: metadata.and_then(|m| serde_json::from_str(&m).ok()),
            });
        }

        Ok(senders)
    }

    async fn delete_paired(&self, sender_id: &str, channel: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();

        let deleted = conn
            .execute(
                "DELETE FROM paired_senders WHERE sender_id = ?1 AND channel = ?2",
                params![sender_id, channel],
            )
            .map_err(|e| PairingError::DatabaseError(e.to_string()))?;

        Ok(deleted > 0)
    }

    async fn load_allowlist(&self) -> Result<Allowlist> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn
            .prepare(
                r#"
                SELECT sender_id, channel, display_name, added_at, added_by, expires_at, active, notes
                FROM allowlist
                "#,
            )
            .map_err(|e| PairingError::DatabaseError(e.to_string()))?;

        let mut entries = Vec::new();
        let mut rows = stmt
            .query([])
            .map_err(|e| PairingError::DatabaseError(e.to_string()))?;

        while let Some(row) = rows
            .next()
            .map_err(|e| PairingError::DatabaseError(e.to_string()))?
        {
            let sender_id: String = row
                .get(0)
                .map_err(|e| PairingError::DatabaseError(e.to_string()))?;
            let channel: Option<String> = row
                .get(1)
                .map_err(|e| PairingError::DatabaseError(e.to_string()))?;
            let display_name: Option<String> = row
                .get(2)
                .map_err(|e| PairingError::DatabaseError(e.to_string()))?;
            let added_at: String = row
                .get(3)
                .map_err(|e| PairingError::DatabaseError(e.to_string()))?;
            let added_by: Option<String> = row
                .get(4)
                .map_err(|e| PairingError::DatabaseError(e.to_string()))?;
            let expires_at: Option<String> = row
                .get(5)
                .map_err(|e| PairingError::DatabaseError(e.to_string()))?;
            let active: i32 = row
                .get(6)
                .map_err(|e| PairingError::DatabaseError(e.to_string()))?;
            let notes: Option<String> = row
                .get(7)
                .map_err(|e| PairingError::DatabaseError(e.to_string()))?;

            entries.push(AllowlistEntry {
                sender_id,
                channel,
                display_name,
                added_at: chrono::DateTime::parse_from_rfc3339(&added_at)
                    .map_err(|e| PairingError::DatabaseError(e.to_string()))?
                    .with_timezone(&chrono::Utc),
                added_by,
                expires_at: expires_at.and_then(|e| {
                    chrono::DateTime::parse_from_rfc3339(&e)
                        .ok()
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                }),
                active: active != 0,
                notes,
            });
        }

        Ok(Allowlist { entries })
    }

    async fn save_allowlist(&self, allowlist: &Allowlist) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        conn.execute("DELETE FROM allowlist", [])
            .map_err(|e| PairingError::DatabaseError(e.to_string()))?;

        for entry in &allowlist.entries {
            conn.execute(
                r#"
                INSERT INTO allowlist
                    (sender_id, channel, display_name, added_at, added_by, expires_at, active, notes)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                "#,
                params![
                    entry.sender_id,
                    entry.channel,
                    entry.display_name,
                    entry.added_at.to_rfc3339(),
                    entry.added_by,
                    entry.expires_at.map(|e| e.to_rfc3339()),
                    entry.active as i32,
                    entry.notes,
                ],
            )
            .map_err(|e| PairingError::DatabaseError(e.to_string()))?;
        }

        Ok(())
    }

    async fn add_allowlist_entry(&self, entry: &AllowlistEntry) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        conn.execute(
            r#"
            INSERT OR REPLACE INTO allowlist
                (sender_id, channel, display_name, added_at, added_by, expires_at, active, notes)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            "#,
            params![
                entry.sender_id,
                entry.channel,
                entry.display_name,
                entry.added_at.to_rfc3339(),
                entry.added_by,
                entry.expires_at.map(|e| e.to_rfc3339()),
                entry.active as i32,
                entry.notes,
            ],
        )
        .map_err(|e| PairingError::DatabaseError(e.to_string()))?;

        Ok(())
    }

    async fn remove_allowlist_entry(&self, sender_id: &str, channel: Option<&str>) -> Result<bool> {
        let conn = self.conn.lock().unwrap();

        let deleted = match channel {
            Some(ch) => conn.execute(
                "DELETE FROM allowlist WHERE sender_id = ?1 AND channel = ?2",
                params![sender_id, ch],
            ),
            None => conn.execute(
                "DELETE FROM allowlist WHERE sender_id = ?1",
                params![sender_id],
            ),
        }
        .map_err(|e| PairingError::DatabaseError(e.to_string()))?;

        Ok(deleted > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_sqlite_pairing_store() {
        let store = SqlitePairingStore::in_memory().unwrap();

        // Test pending approvals
        let code = ApprovalCode::new("123456".to_string(), 300);
        let pending = PendingApproval::new("user1", "telegram", code);

        store.save_pending(&pending).await.unwrap();

        let loaded = store.get_pending("user1", "telegram").await.unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().sender_id, "user1");

        store.delete_pending(pending.id).await.unwrap();
        let deleted = store.get_pending("user1", "telegram").await.unwrap();
        assert!(deleted.is_none());
    }

    #[tokio::test]
    async fn test_paired_senders() {
        let store = SqlitePairingStore::in_memory().unwrap();

        let sender = PairedSender {
            sender_id: "user1".to_string(),
            channel: "telegram".to_string(),
            paired_at: chrono::Utc::now(),
            approved_by: Some("admin".to_string()),
            metadata: None,
        };

        store.save_paired(&sender).await.unwrap();

        let loaded = store.get_paired("user1", "telegram").await.unwrap();
        assert!(loaded.is_some());

        let list = store.list_paired(Some("telegram")).await.unwrap();
        assert_eq!(list.len(), 1);

        store.delete_paired("user1", "telegram").await.unwrap();
        let deleted = store.get_paired("user1", "telegram").await.unwrap();
        assert!(deleted.is_none());
    }

    #[tokio::test]
    async fn test_allowlist_persistence() {
        let store = SqlitePairingStore::in_memory().unwrap();

        let entry = AllowlistEntry::new("user1").with_channel("telegram");
        store.add_allowlist_entry(&entry).await.unwrap();

        let allowlist = store.load_allowlist().await.unwrap();
        assert_eq!(allowlist.entries.len(), 1);
        assert!(allowlist.is_allowed("user1", Some("telegram")));

        store
            .remove_allowlist_entry("user1", Some("telegram"))
            .await
            .unwrap();
        let allowlist = store.load_allowlist().await.unwrap();
        assert!(allowlist.entries.is_empty());
    }
}
