//! WebChat interface for drbot.
//!
//! This crate provides a web-based chat interface with embedded UI
//! that can be used for testing and as a standalone chat channel.

mod server;

pub use drbot_core::config::WebChatConfig;
pub use server::WebChatChannel;

/// The embedded HTML/JS chat interface.
pub const CHAT_HTML: &str = include_str!("chat.html");
