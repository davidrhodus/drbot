//! Custom AI personalities for drbot.
//!
//! Allows defining and switching between different AI personas.

mod builder;
mod persona;
mod registry;

pub use builder::PersonaBuilder;
pub use persona::{Persona, PersonaStyle, PersonaTrait};
pub use registry::{PersonaId, PersonaRegistry};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Persona result.
pub type Result<T> = std::result::Result<T, PersonaError>;

/// Persona errors.
#[derive(Debug, thiserror::Error)]
pub enum PersonaError {
    #[error("Persona not found: {0}")]
    NotFound(String),
    #[error("Invalid persona definition: {0}")]
    InvalidDefinition(String),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Persona configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonaConfig {
    /// Default persona ID.
    pub default_persona: Option<String>,
    /// Path to custom personas directory.
    pub personas_dir: Option<String>,
    /// Allow per-channel personas.
    pub per_channel_personas: bool,
}

impl Default for PersonaConfig {
    fn default() -> Self {
        Self {
            default_persona: None,
            personas_dir: None,
            per_channel_personas: true,
        }
    }
}

/// Built-in personas.
pub struct BuiltinPersonas;

impl BuiltinPersonas {
    /// Default assistant persona.
    pub fn default_assistant() -> Persona {
        PersonaBuilder::new("default")
            .name("Assistant")
            .description("A helpful AI assistant")
            .style(PersonaStyle::Professional)
            .trait_(PersonaTrait::Helpful)
            .trait_(PersonaTrait::Accurate)
            .build()
    }

    /// Creative writing persona.
    pub fn creative_writer() -> Persona {
        PersonaBuilder::new("creative")
            .name("Creative Writer")
            .description("A creative and imaginative writing assistant")
            .style(PersonaStyle::Creative)
            .trait_(PersonaTrait::Creative)
            .trait_(PersonaTrait::Expressive)
            .system_prompt("You are a creative writing assistant. Be imaginative, use vivid language, and help craft compelling narratives.")
            .build()
    }

    /// Technical expert persona.
    pub fn technical_expert() -> Persona {
        PersonaBuilder::new("technical")
            .name("Technical Expert")
            .description("A precise and detailed technical assistant")
            .style(PersonaStyle::Technical)
            .trait_(PersonaTrait::Precise)
            .trait_(PersonaTrait::Thorough)
            .system_prompt("You are a technical expert. Provide detailed, accurate technical information. Use proper terminology and be thorough in explanations.")
            .build()
    }

    /// Casual friend persona.
    pub fn casual_friend() -> Persona {
        PersonaBuilder::new("casual")
            .name("Casual Friend")
            .description("A friendly, casual conversationalist")
            .style(PersonaStyle::Casual)
            .trait_(PersonaTrait::Friendly)
            .trait_(PersonaTrait::Humorous)
            .system_prompt("You are a friendly chat companion. Be casual, use conversational language, and feel free to use humor when appropriate.")
            .build()
    }

    /// Get all built-in personas.
    pub fn all() -> Vec<Persona> {
        vec![
            Self::default_assistant(),
            Self::creative_writer(),
            Self::technical_expert(),
            Self::casual_friend(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_personas() {
        let personas = BuiltinPersonas::all();
        assert!(!personas.is_empty());
        assert!(personas.iter().any(|p| p.id == "default"));
    }
}
