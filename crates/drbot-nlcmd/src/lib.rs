//! Natural language command interface for drbot.
//!
//! Enables controlling drbot and system functions through natural language.
//!
//! # Example
//!
//! ```rust,no_run
//! use drbot_nlcmd::{CommandProcessor, Command};
//!
//! async fn example() {
//!     let processor = CommandProcessor::new();
//!
//!     // Parse natural language command
//!     if let Some(command) = processor.parse("open my browser and go to github").await {
//!         // Execute the command
//!         processor.execute(command).await;
//!     }
//! }
//! ```

mod command;
mod executor;
mod intent;
mod processor;

pub use command::{Command, CommandType, Parameter};
pub use executor::{CommandExecutor, ExecutionResult};
pub use intent::{Intent, IntentClassifier};
pub use processor::CommandProcessor;

/// Result type.
pub type Result<T> = std::result::Result<T, CommandError>;

/// Command errors.
#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error("Failed to parse command: {0}")]
    ParseError(String),
    #[error("Unknown command: {0}")]
    UnknownCommand(String),
    #[error("Execution failed: {0}")]
    ExecutionFailed(String),
    #[error("Permission denied: {0}")]
    PermissionDenied(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_command_processor() {
        let processor = CommandProcessor::new();

        // Test with a valid command
        let result = processor.parse("open chrome").await;
        assert!(result.is_some());

        // Test with empty input
        let result = processor.parse("").await;
        assert!(result.is_none());
    }
}
