//! Command definitions.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Command types.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommandType {
    /// System control commands.
    System(SystemCommand),
    /// Application commands.
    Application(AppCommand),
    /// File operations.
    File(FileCommand),
    /// Communication commands.
    Communication(CommCommand),
    /// Search commands.
    Search(SearchCommand),
    /// Settings commands.
    Settings(SettingsCommand),
    /// Custom/unknown command.
    Custom(String),
}

/// System-level commands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SystemCommand {
    Sleep,
    Shutdown,
    Restart,
    Lock,
    Logout,
    VolumeUp,
    VolumeDown,
    Mute,
    BrightnessUp,
    BrightnessDown,
    Screenshot,
    ScreenRecord,
}

/// Application commands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AppCommand {
    Open(String),
    Close(String),
    Focus(String),
    Minimize,
    Maximize,
    FullScreen,
    SwitchApp,
    ListApps,
}

/// File operation commands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileCommand {
    Open(String),
    Create(String),
    Delete(String),
    Move { from: String, to: String },
    Copy { from: String, to: String },
    Rename { from: String, to: String },
    Find(String),
    List(String),
}

/// Communication commands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommCommand {
    SendMessage {
        to: String,
        content: String,
    },
    Email {
        to: String,
        subject: String,
        body: String,
    },
    Call(String),
    Schedule {
        what: String,
        when: String,
    },
}

/// Search commands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SearchCommand {
    Web(String),
    Files(String),
    Apps(String),
    Contacts(String),
    Calendar(String),
}

/// Settings commands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SettingsCommand {
    WiFi(bool),
    Bluetooth(bool),
    DarkMode(bool),
    DoNotDisturb(bool),
    Set { key: String, value: String },
    Get(String),
}

/// Command parameter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Parameter {
    /// Parameter name.
    pub name: String,
    /// Parameter value.
    pub value: ParameterValue,
    /// Whether this parameter was inferred.
    pub inferred: bool,
}

/// Parameter value types.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ParameterValue {
    String(String),
    Number(f64),
    Boolean(bool),
    List(Vec<ParameterValue>),
    None,
}

impl ParameterValue {
    /// Get as string.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            ParameterValue::String(s) => Some(s),
            _ => None,
        }
    }

    /// Get as number.
    pub fn as_number(&self) -> Option<f64> {
        match self {
            ParameterValue::Number(n) => Some(*n),
            _ => None,
        }
    }

    /// Get as boolean.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            ParameterValue::Boolean(b) => Some(*b),
            _ => None,
        }
    }
}

/// A parsed command.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Command {
    /// Original input text.
    pub original: String,
    /// Command type.
    pub command_type: CommandType,
    /// Extracted parameters.
    pub parameters: HashMap<String, Parameter>,
    /// Confidence score (0.0 - 1.0).
    pub confidence: f32,
    /// Whether confirmation is needed.
    pub requires_confirmation: bool,
}

impl Command {
    /// Create a new command.
    pub fn new(original: impl Into<String>, command_type: CommandType) -> Self {
        Self {
            original: original.into(),
            command_type,
            parameters: HashMap::new(),
            confidence: 1.0,
            requires_confirmation: false,
        }
    }

    /// Add a parameter.
    pub fn with_parameter(mut self, name: impl Into<String>, value: ParameterValue) -> Self {
        let name_str = name.into();
        self.parameters.insert(
            name_str.clone(),
            Parameter {
                name: name_str,
                value,
                inferred: false,
            },
        );
        self
    }

    /// Set confidence.
    pub fn with_confidence(mut self, confidence: f32) -> Self {
        self.confidence = confidence.clamp(0.0, 1.0);
        self
    }

    /// Require confirmation.
    pub fn requires_confirmation(mut self) -> Self {
        self.requires_confirmation = true;
        self
    }

    /// Get a parameter value.
    pub fn get_param(&self, name: &str) -> Option<&ParameterValue> {
        self.parameters.get(name).map(|p| &p.value)
    }

    /// Get a string parameter.
    pub fn get_string_param(&self, name: &str) -> Option<&str> {
        self.get_param(name).and_then(|v| v.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_creation() {
        let cmd = Command::new(
            "open chrome",
            CommandType::Application(AppCommand::Open("chrome".to_string())),
        )
        .with_confidence(0.95);

        assert_eq!(cmd.confidence, 0.95);
    }

    #[test]
    fn test_command_parameters() {
        let cmd = Command::new("test", CommandType::Custom("test".to_string()))
            .with_parameter("name", ParameterValue::String("value".to_string()));

        assert_eq!(cmd.get_string_param("name"), Some("value"));
    }
}
