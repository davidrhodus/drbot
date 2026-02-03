//! Error types for drbot.

use thiserror::Error;

/// The main error type for drbot operations.
#[derive(Error, Debug)]
pub enum Error {
    /// Configuration error.
    #[error("configuration error: {0}")]
    Config(String),

    /// IO error.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// Serialization/deserialization error.
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// WebSocket error.
    #[error("websocket error: {0}")]
    WebSocket(String),

    /// HTTP error.
    #[error("http error: {0}")]
    Http(String),

    /// Authentication error.
    #[error("authentication error: {0}")]
    Auth(String),

    /// Provider error.
    #[error("provider error: {0}")]
    Provider(String),

    /// Channel error.
    #[error("channel error: {0}")]
    Channel(String),

    /// Session error.
    #[error("session error: {0}")]
    Session(String),

    /// Not found error.
    #[error("not found: {0}")]
    NotFound(String),

    /// Invalid input error.
    #[error("invalid input: {0}")]
    InvalidInput(String),

    /// Rate limit error.
    #[error("rate limited: {0}")]
    RateLimit(String),

    /// Timeout error.
    #[error("timeout: {0}")]
    Timeout(String),

    /// Internal error.
    #[error("internal error: {0}")]
    Internal(String),
}

/// Result type alias using drbot's Error type.
pub type Result<T> = std::result::Result<T, Error>;
