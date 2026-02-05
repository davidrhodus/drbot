//! Core types, traits, and configuration for drbot.
//!
//! This crate provides the foundational types and abstractions used throughout drbot.

pub mod config;
pub mod error;
pub mod markdown;
pub mod message;
pub mod session;
pub mod user;

pub use config::Config;
pub use error::{Error, Result};
pub use message::{IncomingMessage, Message, OutgoingMessage};
pub use session::Session;
pub use user::User;
