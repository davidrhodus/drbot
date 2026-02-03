//! Command processor for natural language input.

use crate::command::{Command, CommandType};
use crate::executor::{CommandExecutor, ExecutionResult};
use crate::intent::IntentClassifier;
use tracing::{debug, info};

/// Command processor configuration.
#[derive(Debug, Clone)]
pub struct ProcessorConfig {
    /// Minimum confidence threshold.
    pub min_confidence: f32,
    /// Whether to require confirmation for dangerous commands.
    pub confirm_dangerous: bool,
}

impl Default for ProcessorConfig {
    fn default() -> Self {
        Self {
            min_confidence: 0.5,
            confirm_dangerous: true,
        }
    }
}

/// Natural language command processor.
pub struct CommandProcessor {
    config: ProcessorConfig,
    classifier: IntentClassifier,
    executor: CommandExecutor,
}

impl CommandProcessor {
    /// Create a new command processor.
    pub fn new() -> Self {
        Self::with_config(ProcessorConfig::default())
    }

    /// Create with custom config.
    pub fn with_config(config: ProcessorConfig) -> Self {
        Self {
            config,
            classifier: IntentClassifier::new(),
            executor: CommandExecutor::new(),
        }
    }

    /// Parse natural language into a command.
    pub async fn parse(&self, input: &str) -> Option<Command> {
        let input = input.trim();
        if input.is_empty() {
            return None;
        }

        debug!("Parsing command: {}", input);

        // Classify intent
        let intent = self.classifier.classify(input);

        // Check confidence threshold
        if intent.confidence < self.config.min_confidence {
            debug!(
                "Low confidence ({}) for input: {}",
                intent.confidence, input
            );
            return None;
        }

        // Convert to command type
        let command_type = self.classifier.to_command_type(&intent);

        // Build command
        let mut command = Command::new(input, command_type).with_confidence(intent.confidence);

        // Check if dangerous
        if self.config.confirm_dangerous && is_dangerous_command(&command) {
            command = command.requires_confirmation();
        }

        info!("Parsed command with confidence {}", intent.confidence);
        Some(command)
    }

    /// Execute a command.
    pub async fn execute(&self, command: Command) -> ExecutionResult {
        self.executor.execute(command).await
    }

    /// Parse and execute in one step.
    pub async fn process(&self, input: &str) -> Option<ExecutionResult> {
        let command = self.parse(input).await?;
        Some(self.execute(command).await)
    }

    /// Get the config.
    pub fn config(&self) -> &ProcessorConfig {
        &self.config
    }
}

impl Default for CommandProcessor {
    fn default() -> Self {
        Self::new()
    }
}

/// Check if a command is potentially dangerous.
fn is_dangerous_command(command: &Command) -> bool {
    use crate::command::{FileCommand, SystemCommand};

    match &command.command_type {
        CommandType::System(sys) => matches!(
            sys,
            SystemCommand::Shutdown | SystemCommand::Restart | SystemCommand::Logout
        ),
        CommandType::File(file) => matches!(file, FileCommand::Delete(_)),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_parse_command() {
        let processor = CommandProcessor::new();

        let cmd = processor.parse("open safari").await;
        assert!(cmd.is_some());

        let cmd = cmd.unwrap();
        assert!(cmd.confidence > 0.5);
    }

    #[tokio::test]
    async fn test_empty_input() {
        let processor = CommandProcessor::new();

        let cmd = processor.parse("").await;
        assert!(cmd.is_none());

        let cmd = processor.parse("   ").await;
        assert!(cmd.is_none());
    }

    #[test]
    fn test_dangerous_detection() {
        use crate::command::SystemCommand;

        let safe_cmd = Command::new(
            "open chrome",
            CommandType::Application(crate::command::AppCommand::Open("chrome".to_string())),
        );
        assert!(!is_dangerous_command(&safe_cmd));

        let dangerous_cmd = Command::new("shutdown", CommandType::System(SystemCommand::Shutdown));
        assert!(is_dangerous_command(&dangerous_cmd));
    }
}
