//! Intent classification for natural language commands.

use crate::command::{AppCommand, CommandType, FileCommand, SearchCommand, SystemCommand};
use serde::{Deserialize, Serialize};

/// Detected intent from natural language.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Intent {
    /// Intent category.
    pub category: IntentCategory,
    /// Action verb.
    pub action: String,
    /// Target of the action.
    pub target: Option<String>,
    /// Confidence score.
    pub confidence: f32,
}

/// Intent categories.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum IntentCategory {
    OpenApp,
    CloseApp,
    SystemControl,
    FileOperation,
    Search,
    Communication,
    Settings,
    Help,
    Unknown,
}

/// Intent classifier.
#[derive(Debug, Default)]
pub struct IntentClassifier {
    // In production, this would use an ML model
}

impl IntentClassifier {
    /// Create a new classifier.
    pub fn new() -> Self {
        Self::default()
    }

    /// Classify intent from text.
    pub fn classify(&self, text: &str) -> Intent {
        let lower = text.to_lowercase();
        let words: Vec<&str> = lower.split_whitespace().collect();

        // Simple rule-based classification
        let (category, action, target, confidence) = self.classify_rules(&lower, &words);

        Intent {
            category,
            action,
            target,
            confidence,
        }
    }

    fn classify_rules(
        &self,
        text: &str,
        words: &[&str],
    ) -> (IntentCategory, String, Option<String>, f32) {
        // Open/launch patterns
        let open_verbs = ["open", "launch", "start", "run", "execute"];
        for verb in &open_verbs {
            if words.first() == Some(verb) {
                let target = words.get(1..).map(|w| w.join(" "));
                return (IntentCategory::OpenApp, verb.to_string(), target, 0.9);
            }
        }

        // Close patterns
        let close_verbs = ["close", "quit", "exit", "kill", "terminate"];
        for verb in &close_verbs {
            if words.first() == Some(verb) {
                let target = words.get(1..).map(|w| w.join(" "));
                return (IntentCategory::CloseApp, verb.to_string(), target, 0.9);
            }
        }

        // System control
        if text.contains("volume up") || text.contains("louder") {
            return (
                IntentCategory::SystemControl,
                "volume_up".to_string(),
                None,
                0.9,
            );
        }
        if text.contains("volume down") || text.contains("quieter") {
            return (
                IntentCategory::SystemControl,
                "volume_down".to_string(),
                None,
                0.9,
            );
        }
        if text.contains("mute") {
            return (IntentCategory::SystemControl, "mute".to_string(), None, 0.9);
        }
        if text.contains("screenshot") || text.contains("screen shot") {
            return (
                IntentCategory::SystemControl,
                "screenshot".to_string(),
                None,
                0.9,
            );
        }
        if text.contains("sleep") || text.contains("lock") {
            return (
                IntentCategory::SystemControl,
                "lock".to_string(),
                None,
                0.85,
            );
        }

        // Search patterns
        let search_verbs = ["search", "find", "look for", "google", "lookup"];
        for verb in &search_verbs {
            if text.contains(verb) {
                let target = self.extract_after(text, verb);
                return (IntentCategory::Search, "search".to_string(), target, 0.85);
            }
        }

        // File operations
        let file_verbs = ["create", "delete", "move", "copy", "rename"];
        for verb in &file_verbs {
            if words.first() == Some(verb) {
                let target = words.get(1..).map(|w| w.join(" "));
                return (IntentCategory::FileOperation, verb.to_string(), target, 0.8);
            }
        }

        // Help
        if text.contains("help") || text.contains("what can you do") {
            return (IntentCategory::Help, "help".to_string(), None, 0.9);
        }

        // Unknown
        (
            IntentCategory::Unknown,
            String::new(),
            Some(text.to_string()),
            0.3,
        )
    }

    fn extract_after(&self, text: &str, keyword: &str) -> Option<String> {
        text.find(keyword)
            .map(|pos| {
                let after = &text[pos + keyword.len()..];
                after.trim().to_string()
            })
            .filter(|s| !s.is_empty())
    }

    /// Convert intent to command type.
    pub fn to_command_type(&self, intent: &Intent) -> CommandType {
        match intent.category {
            IntentCategory::OpenApp => CommandType::Application(AppCommand::Open(
                intent.target.clone().unwrap_or_default(),
            )),
            IntentCategory::CloseApp => CommandType::Application(AppCommand::Close(
                intent.target.clone().unwrap_or_default(),
            )),
            IntentCategory::SystemControl => match intent.action.as_str() {
                "volume_up" => CommandType::System(SystemCommand::VolumeUp),
                "volume_down" => CommandType::System(SystemCommand::VolumeDown),
                "mute" => CommandType::System(SystemCommand::Mute),
                "screenshot" => CommandType::System(SystemCommand::Screenshot),
                "lock" | "sleep" => CommandType::System(SystemCommand::Lock),
                _ => CommandType::Custom(intent.action.clone()),
            },
            IntentCategory::Search => CommandType::Search(SearchCommand::Web(
                intent.target.clone().unwrap_or_default(),
            )),
            IntentCategory::FileOperation => match intent.action.as_str() {
                "create" => CommandType::File(FileCommand::Create(
                    intent.target.clone().unwrap_or_default(),
                )),
                "delete" => CommandType::File(FileCommand::Delete(
                    intent.target.clone().unwrap_or_default(),
                )),
                "find" => {
                    CommandType::File(FileCommand::Find(intent.target.clone().unwrap_or_default()))
                }
                _ => CommandType::Custom(intent.action.clone()),
            },
            _ => CommandType::Custom(
                intent
                    .target
                    .clone()
                    .unwrap_or_else(|| intent.action.clone()),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_open() {
        let classifier = IntentClassifier::new();
        let intent = classifier.classify("open chrome");

        assert_eq!(intent.category, IntentCategory::OpenApp);
        assert_eq!(intent.target, Some("chrome".to_string()));
    }

    #[test]
    fn test_classify_search() {
        let classifier = IntentClassifier::new();
        let intent = classifier.classify("search for rust programming");

        assert_eq!(intent.category, IntentCategory::Search);
    }

    #[test]
    fn test_classify_system() {
        let classifier = IntentClassifier::new();

        let intent = classifier.classify("take a screenshot");
        assert_eq!(intent.category, IntentCategory::SystemControl);
        assert_eq!(intent.action, "screenshot");
    }
}
