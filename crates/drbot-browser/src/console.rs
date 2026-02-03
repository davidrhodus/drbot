//! Console message monitoring via Chrome DevTools Protocol.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};

/// Console message log level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    /// Verbose/debug messages.
    Verbose,
    /// Informational messages.
    Info,
    /// Warnings.
    Warning,
    /// Errors.
    Error,
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Verbose => write!(f, "verbose"),
            Self::Info => write!(f, "info"),
            Self::Warning => write!(f, "warning"),
            Self::Error => write!(f, "error"),
        }
    }
}

/// A console message from the browser.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsoleMessage {
    /// Message log level.
    pub level: LogLevel,
    /// Message text.
    pub text: String,
    /// Source URL.
    pub url: Option<String>,
    /// Line number.
    pub line: Option<u32>,
    /// Column number.
    pub column: Option<u32>,
    /// Stack trace (if available).
    pub stack_trace: Option<String>,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
    /// Execution context ID.
    pub context_id: Option<i64>,
}

impl ConsoleMessage {
    /// Create a new console message.
    pub fn new(level: LogLevel, text: &str) -> Self {
        Self {
            level,
            text: text.to_string(),
            url: None,
            line: None,
            column: None,
            stack_trace: None,
            timestamp: Utc::now(),
            context_id: None,
        }
    }

    /// Set source location.
    pub fn with_location(mut self, url: &str, line: u32, column: u32) -> Self {
        self.url = Some(url.to_string());
        self.line = Some(line);
        self.column = Some(column);
        self
    }

    /// Set stack trace.
    pub fn with_stack_trace(mut self, trace: &str) -> Self {
        self.stack_trace = Some(trace.to_string());
        self
    }

    /// Check if this is an error message.
    pub fn is_error(&self) -> bool {
        self.level == LogLevel::Error
    }

    /// Check if this is a warning.
    pub fn is_warning(&self) -> bool {
        self.level == LogLevel::Warning
    }

    /// Format as a log line.
    pub fn format(&self) -> String {
        let location = match (&self.url, self.line) {
            (Some(url), Some(line)) => format!(" ({}:{})", url, line),
            _ => String::new(),
        };
        format!("[{}]{} {}", self.level, location, self.text)
    }
}

/// Console monitor configuration.
#[derive(Debug, Clone)]
pub struct ConsoleConfig {
    /// Capture verbose messages.
    pub capture_verbose: bool,
    /// Capture info messages.
    pub capture_info: bool,
    /// Capture warnings.
    pub capture_warnings: bool,
    /// Capture errors.
    pub capture_errors: bool,
    /// Maximum messages to buffer.
    pub max_buffer_size: usize,
    /// Include stack traces.
    pub include_stack_traces: bool,
}

impl Default for ConsoleConfig {
    fn default() -> Self {
        Self {
            capture_verbose: false,
            capture_info: true,
            capture_warnings: true,
            capture_errors: true,
            max_buffer_size: 1000,
            include_stack_traces: true,
        }
    }
}

/// Console message monitor.
pub struct ConsoleMonitor {
    /// Configuration.
    config: ConsoleConfig,
    /// Buffered messages.
    messages: Arc<RwLock<Vec<ConsoleMessage>>>,
    /// Message broadcast channel.
    message_tx: broadcast::Sender<ConsoleMessage>,
    /// Whether monitoring is active.
    active: Arc<RwLock<bool>>,
}

impl ConsoleMonitor {
    /// Create a new console monitor.
    pub fn new(config: ConsoleConfig) -> Self {
        let (message_tx, _) = broadcast::channel(256);
        Self {
            config,
            messages: Arc::new(RwLock::new(Vec::new())),
            message_tx,
            active: Arc::new(RwLock::new(false)),
        }
    }

    /// Start monitoring.
    pub async fn start(&self) {
        let mut active = self.active.write().await;
        *active = true;
    }

    /// Stop monitoring.
    pub async fn stop(&self) {
        let mut active = self.active.write().await;
        *active = false;
    }

    /// Check if monitoring is active.
    pub async fn is_active(&self) -> bool {
        *self.active.read().await
    }

    /// Add a message (called by CDP event handler).
    pub async fn add_message(&self, message: ConsoleMessage) {
        if !*self.active.read().await {
            return;
        }

        // Check if we should capture this level
        let should_capture = match message.level {
            LogLevel::Verbose => self.config.capture_verbose,
            LogLevel::Info => self.config.capture_info,
            LogLevel::Warning => self.config.capture_warnings,
            LogLevel::Error => self.config.capture_errors,
        };

        if !should_capture {
            return;
        }

        // Add to buffer
        let mut messages = self.messages.write().await;
        messages.push(message.clone());

        // Trim buffer if needed
        if messages.len() > self.config.max_buffer_size {
            let to_remove = messages.len() - self.config.max_buffer_size;
            messages.drain(0..to_remove);
        }

        // Broadcast
        let _ = self.message_tx.send(message);
    }

    /// Get all buffered messages.
    pub async fn get_messages(&self) -> Vec<ConsoleMessage> {
        let messages = self.messages.read().await;
        messages.clone()
    }

    /// Get messages filtered by level.
    pub async fn get_messages_by_level(&self, level: LogLevel) -> Vec<ConsoleMessage> {
        let messages = self.messages.read().await;
        messages
            .iter()
            .filter(|m| m.level == level)
            .cloned()
            .collect()
    }

    /// Get error messages only.
    pub async fn get_errors(&self) -> Vec<ConsoleMessage> {
        self.get_messages_by_level(LogLevel::Error).await
    }

    /// Get warning messages only.
    pub async fn get_warnings(&self) -> Vec<ConsoleMessage> {
        self.get_messages_by_level(LogLevel::Warning).await
    }

    /// Clear buffered messages.
    pub async fn clear(&self) {
        let mut messages = self.messages.write().await;
        messages.clear();
    }

    /// Subscribe to new messages.
    pub fn subscribe(&self) -> broadcast::Receiver<ConsoleMessage> {
        self.message_tx.subscribe()
    }

    /// Get message count.
    pub async fn count(&self) -> usize {
        let messages = self.messages.read().await;
        messages.len()
    }

    /// Get error count.
    pub async fn error_count(&self) -> usize {
        let messages = self.messages.read().await;
        messages.iter().filter(|m| m.is_error()).count()
    }

    /// Get warning count.
    pub async fn warning_count(&self) -> usize {
        let messages = self.messages.read().await;
        messages.iter().filter(|m| m.is_warning()).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_console_message() {
        let msg = ConsoleMessage::new(LogLevel::Error, "Something went wrong")
            .with_location("app.js", 42, 10);

        assert!(msg.is_error());
        assert!(!msg.is_warning());
        assert!(msg.format().contains("error"));
        assert!(msg.format().contains("app.js:42"));
    }

    #[tokio::test]
    async fn test_console_monitor() {
        let monitor = ConsoleMonitor::new(ConsoleConfig::default());
        monitor.start().await;

        monitor
            .add_message(ConsoleMessage::new(LogLevel::Info, "Hello"))
            .await;
        monitor
            .add_message(ConsoleMessage::new(LogLevel::Error, "Oops"))
            .await;

        assert_eq!(monitor.count().await, 2);
        assert_eq!(monitor.error_count().await, 1);

        let errors = monitor.get_errors().await;
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].text, "Oops");

        monitor.clear().await;
        assert_eq!(monitor.count().await, 0);
    }
}
