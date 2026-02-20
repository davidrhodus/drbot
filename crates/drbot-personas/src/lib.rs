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
        let mut persona = builder::presets::professional("default", "Assistant");
        persona.description = "A helpful AI assistant".to_string();
        persona
    }

    /// Creative writing persona.
    pub fn creative_writer() -> Persona {
        let mut persona = builder::presets::creative("creative", "Creative Writer");
        persona.description = "A creative and imaginative writing assistant".to_string();
        persona.system_prompt = Some("You are a creative writing assistant. Be imaginative, use vivid language, and help craft compelling narratives.".to_string());
        persona
    }

    /// Technical expert persona.
    pub fn technical_expert() -> Persona {
        let mut persona = builder::presets::technical("technical", "Technical Expert");
        persona.description = "A precise and detailed technical assistant".to_string();
        persona.system_prompt = Some("You are a technical expert. Provide detailed, accurate technical information. Use proper terminology and be thorough in explanations.".to_string());
        persona
    }

    /// Casual friend persona.
    pub fn casual_friend() -> Persona {
        let mut persona = builder::presets::casual("casual", "Casual Friend");
        persona.description = "A friendly, casual conversationalist".to_string();
        persona.system_prompt = Some("You are a friendly chat companion. Be casual, use conversational language, and feel free to use humor when appropriate.".to_string());
        persona
    }

    /// Educational tutor persona.
    pub fn teacher() -> Persona {
        let mut persona = builder::presets::teacher("teacher", "Teacher");
        persona.description = "An educational tutor".to_string();
        persona
    }

    /// Concise assistant persona.
    pub fn concise_assistant() -> Persona {
        let mut persona = builder::presets::concise("concise", "Concise Assistant");
        persona.description = "A brief and direct assistant".to_string();
        persona
    }

    /// Get all built-in personas.
    pub fn all() -> Vec<Persona> {
        vec![
            Self::default_assistant(),
            Self::creative_writer(),
            Self::technical_expert(),
            Self::casual_friend(),
            Self::teacher(),
            Self::concise_assistant(),
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
