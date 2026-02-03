//! Channel abstraction layer for drbot.
//!
//! This crate defines the `Channel` trait that all messaging platforms must implement.

use async_trait::async_trait;
use drbot_core::message::{IncomingMessage, OutgoingMessage};
use drbot_core::Result;
use tokio::sync::broadcast;

/// Trait for messaging channels.
#[async_trait]
pub trait Channel: Send + Sync {
    /// Connect to the messaging platform.
    async fn connect(&mut self) -> Result<()>;

    /// Send a message to a recipient.
    async fn send(&self, to: &str, message: OutgoingMessage) -> Result<()>;

    /// Subscribe to incoming messages.
    fn subscribe(&self) -> broadcast::Receiver<IncomingMessage>;

    /// Disconnect from the messaging platform.
    async fn disconnect(&mut self) -> Result<()>;

    /// Get the channel type identifier.
    fn channel_type(&self) -> &str;
}
