//! Synchronization for real-time collaboration.

use crate::{CollabError, CollabMessage, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

/// Sync event types.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SyncEvent {
    /// New message added.
    MessageAdded { message: CollabMessage },
    /// Message updated.
    MessageUpdated { message_id: Uuid, content: String },
    /// Message deleted.
    MessageDeleted { message_id: Uuid },
    /// Participant joined.
    ParticipantJoined {
        user_id: String,
        display_name: String,
    },
    /// Participant left.
    ParticipantLeft { user_id: String },
    /// Participant typing.
    ParticipantTyping { user_id: String, typing: bool },
    /// Session state changed.
    SessionStateChanged { state: String },
    /// Bot response streaming.
    BotResponseChunk {
        message_id: Uuid,
        chunk: String,
        done: bool,
    },
    /// Full sync request.
    SyncRequest,
    /// Full sync response.
    SyncResponse {
        messages: Vec<CollabMessage>,
        participants: Vec<String>,
    },
}

/// Sync manager for handling real-time updates.
pub struct SyncManager {
    /// Session ID.
    session_id: Uuid,
    /// Event broadcaster.
    broadcaster: broadcast::Sender<SyncEvent>,
    /// Pending events buffer.
    pending: Arc<RwLock<Vec<SyncEvent>>>,
    /// Last sync timestamps per user.
    last_sync: Arc<RwLock<HashMap<String, DateTime<Utc>>>>,
}

impl SyncManager {
    /// Create a new sync manager.
    pub fn new(session_id: Uuid) -> Self {
        let (broadcaster, _) = broadcast::channel(1000);

        Self {
            session_id,
            broadcaster,
            pending: Arc::new(RwLock::new(Vec::new())),
            last_sync: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Subscribe to events.
    pub fn subscribe(&self) -> broadcast::Receiver<SyncEvent> {
        self.broadcaster.subscribe()
    }

    /// Broadcast an event.
    pub async fn broadcast(&self, event: SyncEvent) -> Result<()> {
        // Store in pending buffer
        {
            let mut pending = self.pending.write().await;
            pending.push(event.clone());

            // Keep buffer size reasonable
            if pending.len() > 1000 {
                pending.drain(0..500);
            }
        }

        // Broadcast to subscribers
        let _ = self.broadcaster.send(event);

        Ok(())
    }

    /// Broadcast a message.
    pub async fn broadcast_message(&self, message: CollabMessage) -> Result<()> {
        self.broadcast(SyncEvent::MessageAdded { message }).await
    }

    /// Broadcast typing indicator.
    pub async fn broadcast_typing(&self, user_id: &str, typing: bool) -> Result<()> {
        self.broadcast(SyncEvent::ParticipantTyping {
            user_id: user_id.to_string(),
            typing,
        })
        .await
    }

    /// Broadcast participant join.
    pub async fn broadcast_join(&self, user_id: &str, display_name: &str) -> Result<()> {
        self.broadcast(SyncEvent::ParticipantJoined {
            user_id: user_id.to_string(),
            display_name: display_name.to_string(),
        })
        .await
    }

    /// Broadcast participant leave.
    pub async fn broadcast_leave(&self, user_id: &str) -> Result<()> {
        self.broadcast(SyncEvent::ParticipantLeft {
            user_id: user_id.to_string(),
        })
        .await
    }

    /// Broadcast bot response chunk.
    pub async fn broadcast_bot_chunk(
        &self,
        message_id: Uuid,
        chunk: &str,
        done: bool,
    ) -> Result<()> {
        self.broadcast(SyncEvent::BotResponseChunk {
            message_id,
            chunk: chunk.to_string(),
            done,
        })
        .await
    }

    /// Get events since last sync.
    pub async fn get_events_since(&self, user_id: &str, since: DateTime<Utc>) -> Vec<SyncEvent> {
        let pending = self.pending.read().await;

        // For simplicity, return recent events
        // In production, would filter by timestamp
        pending.iter().cloned().collect()
    }

    /// Record sync for user.
    pub async fn record_sync(&self, user_id: &str) {
        let mut last_sync = self.last_sync.write().await;
        last_sync.insert(user_id.to_string(), Utc::now());
    }

    /// Get subscriber count.
    pub fn subscriber_count(&self) -> usize {
        self.broadcaster.receiver_count()
    }
}

/// Conflict resolution for concurrent edits.
pub struct ConflictResolver;

impl ConflictResolver {
    /// Resolve message ordering conflict.
    pub fn resolve_order(messages: &mut [CollabMessage]) {
        // Sort by timestamp, then by ID for deterministic ordering
        messages.sort_by(|a, b| match a.timestamp.cmp(&b.timestamp) {
            std::cmp::Ordering::Equal => a.id.cmp(&b.id),
            other => other,
        });
    }

    /// Check if messages conflict.
    pub fn conflicts(a: &CollabMessage, b: &CollabMessage) -> bool {
        // Messages conflict if they're from the same user within a short window
        // and have different IDs
        if a.sender_id != b.sender_id {
            return false;
        }

        let time_diff = (a.timestamp - b.timestamp).num_milliseconds().abs();
        time_diff < 1000 && a.id != b.id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_sync_manager() {
        let manager = SyncManager::new(Uuid::new_v4());

        let mut receiver = manager.subscribe();

        manager.broadcast_typing("user1", true).await.unwrap();

        let event = receiver.recv().await.unwrap();
        match event {
            SyncEvent::ParticipantTyping { user_id, typing } => {
                assert_eq!(user_id, "user1");
                assert!(typing);
            }
            _ => panic!("Unexpected event"),
        }
    }

    #[test]
    fn test_conflict_resolver() {
        let msg1 = CollabMessage::new(Uuid::new_v4(), "user1", "Hello");
        let msg2 = CollabMessage::new(Uuid::new_v4(), "user2", "World");

        assert!(!ConflictResolver::conflicts(&msg1, &msg2));
    }
}
