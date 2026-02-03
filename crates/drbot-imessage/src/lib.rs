//! iMessage channel for drbot (macOS only).
//!
//! This crate provides iMessage integration using AppleScript for sending
//! and the Messages database for receiving.
//!
//! # Requirements
//!
//! - macOS with Messages.app
//! - Full Disk Access permission for database reading
//! - Automation permission for AppleScript
//!
//! # Example
//!
//! ```rust,no_run
//! use drbot_imessage::IMessageChannel;
//! use drbot_channels::Channel;
//!
//! async fn example() -> drbot_core::Result<()> {
//!     let mut channel = IMessageChannel::new()?;
//!     channel.connect().await?;
//!
//!     // Subscribe to incoming messages
//!     let mut rx = channel.subscribe();
//!
//!     while let Ok(msg) = rx.recv().await {
//!         println!("Received: {:?}", msg);
//!     }
//!
//!     Ok(())
//! }
//! ```

mod applescript;
mod database;

pub use applescript::{get_chats, is_messages_running, send_message, send_to_chat, ChatInfo};
pub use database::{DbChat, DbMessage, MessageDatabase};

use async_trait::async_trait;
use drbot_channels::Channel;
use drbot_core::message::{Content, IncomingMessage, MessageSender, OutgoingMessage};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{debug, info, warn};
use uuid::Uuid;

/// iMessage channel implementation.
pub struct IMessageChannel {
    /// Message database reader.
    database: MessageDatabase,
    /// Last processed message row ID.
    last_rowid: Arc<AtomicI64>,
    /// Message sender.
    tx: broadcast::Sender<IncomingMessage>,
    /// Whether the channel is running.
    running: Arc<AtomicBool>,
    /// Poll interval in milliseconds.
    poll_interval_ms: u64,
}

impl IMessageChannel {
    /// Create a new iMessage channel.
    pub fn new() -> drbot_core::Result<Self> {
        #[cfg(not(target_os = "macos"))]
        {
            return Err(drbot_core::Error::Internal(
                "iMessage channel is only available on macOS".to_string(),
            ));
        }

        #[cfg(target_os = "macos")]
        {
            let database = MessageDatabase::new()?;
            let (tx, _) = broadcast::channel(256);

            Ok(Self {
                database,
                last_rowid: Arc::new(AtomicI64::new(0)),
                tx,
                running: Arc::new(AtomicBool::new(false)),
                poll_interval_ms: 1000,
            })
        }
    }

    /// Set the poll interval.
    pub fn with_poll_interval(mut self, ms: u64) -> Self {
        self.poll_interval_ms = ms;
        self
    }

    /// Initialize the last row ID to the current latest.
    fn init_last_rowid(&self) -> drbot_core::Result<()> {
        let latest = self.database.get_latest_rowid()?;
        self.last_rowid.store(latest, Ordering::SeqCst);
        debug!("Initialized last_rowid to {}", latest);
        Ok(())
    }

    /// Check if channel is connected.
    pub fn is_connected(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Get the database path.
    pub fn db_path(&self) -> &std::path::Path {
        &self.database.db_path
    }
}

#[async_trait]
impl Channel for IMessageChannel {
    fn channel_type(&self) -> &str {
        "imessage"
    }

    async fn connect(&mut self) -> drbot_core::Result<()> {
        info!("Connecting iMessage channel");

        // Check if on macOS
        #[cfg(not(target_os = "macos"))]
        {
            return Err(drbot_core::Error::Internal(
                "iMessage channel is only available on macOS".to_string(),
            ));
        }

        // Initialize last rowid to current
        self.init_last_rowid()?;
        self.running.store(true, Ordering::SeqCst);

        // Start polling task
        let running = self.running.clone();
        let last_rowid = self.last_rowid.clone();
        let tx = self.tx.clone();
        let poll_interval = self.poll_interval_ms;
        let db_path = self.database.db_path.clone();

        tokio::spawn(async move {
            let database = MessageDatabase::with_path(db_path);

            while running.load(Ordering::SeqCst) {
                let last = last_rowid.load(Ordering::SeqCst);

                match database.get_messages_after(last) {
                    Ok(messages) => {
                        for msg in messages {
                            if msg.rowid > last {
                                last_rowid.store(msg.rowid, Ordering::SeqCst);
                            }

                            if msg.is_from_me {
                                continue;
                            }

                            let Some(text) = msg.text.as_ref() else {
                                continue;
                            };

                            let channel_id = msg
                                .chat_id
                                .and_then(|id| database.get_chat_identifier(id).ok().flatten())
                                .unwrap_or_else(|| msg.sender.clone().unwrap_or_default());

                            let incoming = IncomingMessage {
                                id: Uuid::new_v4(),
                                channel_type: "imessage".to_string(),
                                channel_id,
                                sender: MessageSender {
                                    id: msg.sender.clone().unwrap_or_default(),
                                    name: None,
                                    username: None,
                                },
                                content: vec![Content::Text { text: text.clone() }],
                                received_at: msg.datetime(),
                                raw: Some(serde_json::json!({
                                    "rowid": msg.rowid,
                                    "guid": msg.guid,
                                })),
                                reply_to: None,
                            };

                            if tx.send(incoming).is_err() {
                                // No receivers
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Failed to poll messages: {}", e);
                    }
                }

                tokio::time::sleep(std::time::Duration::from_millis(poll_interval)).await;
            }

            info!("iMessage polling stopped");
        });

        info!("iMessage channel connected");
        Ok(())
    }

    async fn disconnect(&mut self) -> drbot_core::Result<()> {
        info!("Disconnecting iMessage channel");
        self.running.store(false, Ordering::SeqCst);
        Ok(())
    }

    async fn send(&self, to: &str, message: OutgoingMessage) -> drbot_core::Result<()> {
        debug!("Sending message to {}", to);

        // Extract text content from the message
        let text = message
            .content
            .iter()
            .filter_map(|c| {
                if let Content::Text { text } = c {
                    Some(text.as_str())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n");

        if text.is_empty() {
            return Err(drbot_core::Error::InvalidInput(
                "Message has no text content".to_string(),
            ));
        }

        // Use AppleScript to send
        send_message(to, &text)?;

        Ok(())
    }

    fn subscribe(&self) -> broadcast::Receiver<IncomingMessage> {
        self.tx.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_channel_type() {
        #[cfg(target_os = "macos")]
        {
            if let Ok(channel) = IMessageChannel::new() {
                assert_eq!(channel.channel_type(), "imessage");
            }
        }
    }

    #[test]
    fn test_poll_interval() {
        #[cfg(target_os = "macos")]
        {
            if let Ok(channel) = IMessageChannel::new() {
                let channel = channel.with_poll_interval(500);
                assert_eq!(channel.poll_interval_ms, 500);
            }
        }
    }

    #[test]
    #[cfg(not(target_os = "macos"))]
    fn test_non_macos_error() {
        let result = IMessageChannel::new();
        assert!(result.is_err());
    }
}
