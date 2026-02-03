//! Persona definition.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Persona communication style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PersonaStyle {
    /// Professional and formal.
    Professional,
    /// Casual and friendly.
    Casual,
    /// Technical and precise.
    Technical,
    /// Creative and expressive.
    Creative,
    /// Concise and direct.
    Concise,
    /// Educational and explanatory.
    Educational,
}

impl PersonaStyle {
    /// Get style description for prompt.
    pub fn description(&self) -> &str {
        match self {
            PersonaStyle::Professional => {
                "Use professional, formal language. Be polite and businesslike."
            }
            PersonaStyle::Casual => {
                "Use casual, conversational language. Be friendly and approachable."
            }
            PersonaStyle::Technical => "Use precise technical language. Be accurate and detailed.",
            PersonaStyle::Creative => {
                "Use creative, expressive language. Be imaginative and engaging."
            }
            PersonaStyle::Concise => "Be brief and to the point. Avoid unnecessary elaboration.",
            PersonaStyle::Educational => "Explain concepts clearly. Use examples and analogies.",
        }
    }
}

/// Persona personality trait.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PersonaTrait {
    /// Helpful and supportive.
    Helpful,
    /// Accurate and fact-focused.
    Accurate,
    /// Creative and imaginative.
    Creative,
    /// Friendly and warm.
    Friendly,
    /// Humorous and witty.
    Humorous,
    /// Precise and detail-oriented.
    Precise,
    /// Thorough and comprehensive.
    Thorough,
    /// Expressive and emotive.
    Expressive,
    /// Patient and understanding.
    Patient,
    /// Enthusiastic and energetic.
    Enthusiastic,
}

impl PersonaTrait {
    /// Get trait description for prompt.
    pub fn description(&self) -> &str {
        match self {
            PersonaTrait::Helpful => "Always try to be helpful and provide useful assistance.",
            PersonaTrait::Accurate => "Focus on accuracy and factual correctness.",
            PersonaTrait::Creative => "Think creatively and offer unique perspectives.",
            PersonaTrait::Friendly => "Be warm and friendly in interactions.",
            PersonaTrait::Humorous => "Use appropriate humor when suitable.",
            PersonaTrait::Precise => "Be precise and pay attention to details.",
            PersonaTrait::Thorough => "Provide thorough, comprehensive responses.",
            PersonaTrait::Expressive => "Express ideas with emotion and enthusiasm.",
            PersonaTrait::Patient => "Be patient and understanding with questions.",
            PersonaTrait::Enthusiastic => "Show enthusiasm and positive energy.",
        }
    }
}

/// A persona definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Persona {
    /// Unique persona ID.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Communication style.
    pub style: PersonaStyle,
    /// Personality traits.
    pub traits: Vec<PersonaTrait>,
    /// System prompt.
    pub system_prompt: Option<String>,
    /// Example responses (for few-shot learning).
    pub examples: Vec<PersonaExample>,
    /// Custom parameters.
    pub parameters: HashMap<String, serde_json::Value>,
    /// Temperature override (if any).
    pub temperature: Option<f32>,
    /// Whether persona is enabled.
    pub enabled: bool,
}

impl Persona {
    /// Create a new persona.
    pub fn new(id: &str, name: &str) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            description: String::new(),
            style: PersonaStyle::Professional,
            traits: vec![PersonaTrait::Helpful],
            system_prompt: None,
            examples: Vec::new(),
            parameters: HashMap::new(),
            temperature: None,
            enabled: true,
        }
    }

    /// Build system prompt for this persona.
    pub fn build_system_prompt(&self) -> String {
        let mut parts = Vec::new();

        // Add custom system prompt if provided
        if let Some(prompt) = &self.system_prompt {
            parts.push(prompt.clone());
        } else {
            parts.push(format!("You are {}, {}.", self.name, self.description));
        }

        // Add style instructions
        parts.push(format!(
            "\nCommunication style: {}",
            self.style.description()
        ));

        // Add trait instructions
        if !self.traits.is_empty() {
            let trait_desc: Vec<&str> = self.traits.iter().map(|t| t.description()).collect();
            parts.push(format!("\nPersonality: {}", trait_desc.join(" ")));
        }

        // Add examples
        if !self.examples.is_empty() {
            parts.push("\nExample interactions:".to_string());
            for example in &self.examples {
                parts.push(format!(
                    "\nUser: {}\n{}: {}",
                    example.user_input, self.name, example.response
                ));
            }
        }

        parts.join("\n")
    }

    /// Check if persona has a specific trait.
    pub fn has_trait(&self, trait_: PersonaTrait) -> bool {
        self.traits.contains(&trait_)
    }

    /// Add a trait.
    pub fn add_trait(&mut self, trait_: PersonaTrait) {
        if !self.traits.contains(&trait_) {
            self.traits.push(trait_);
        }
    }

    /// Set a parameter.
    pub fn set_param(&mut self, key: &str, value: serde_json::Value) {
        self.parameters.insert(key.to_string(), value);
    }

    /// Get a parameter.
    pub fn get_param(&self, key: &str) -> Option<&serde_json::Value> {
        self.parameters.get(key)
    }

    // Builder methods

    /// Set the description (builder pattern).
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    /// Set the style (builder pattern).
    pub fn with_style(mut self, style: PersonaStyle) -> Self {
        self.style = style;
        self
    }

    /// Add a trait (builder pattern).
    pub fn with_trait(mut self, trait_: PersonaTrait) -> Self {
        self.add_trait(trait_);
        self
    }

    /// Set the system prompt (builder pattern).
    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    /// Set the temperature (builder pattern).
    pub fn with_temperature(mut self, temp: f32) -> Self {
        self.temperature = Some(temp);
        self
    }

    /// Add an example (builder pattern).
    pub fn with_example(mut self, user_input: &str, response: &str) -> Self {
        self.examples
            .push(PersonaExample::new(user_input, response));
        self
    }
}

/// An example interaction for a persona.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonaExample {
    /// User input.
    pub user_input: String,
    /// Expected response.
    pub response: String,
}

impl PersonaExample {
    /// Create a new example.
    pub fn new(user_input: &str, response: &str) -> Self {
        Self {
            user_input: user_input.to_string(),
            response: response.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_persona_creation() {
        let persona = Persona::new("test", "Test Bot");
        assert_eq!(persona.id, "test");
        assert!(persona.enabled);
    }

    #[test]
    fn test_persona_system_prompt() {
        let mut persona = Persona::new("helper", "Helper Bot");
        persona.description = "a helpful assistant".to_string();
        persona.style = PersonaStyle::Casual;
        persona.traits = vec![PersonaTrait::Helpful, PersonaTrait::Patient];

        let prompt = persona.build_system_prompt();
        assert!(prompt.contains("Helper Bot"));
        assert!(prompt.contains("helpful"));
    }

    #[test]
    fn test_persona_traits() {
        let mut persona = Persona::new("test", "Test");
        persona.add_trait(PersonaTrait::Creative);
        persona.add_trait(PersonaTrait::Creative); // Duplicate

        assert!(persona.has_trait(PersonaTrait::Creative));
        assert_eq!(
            persona
                .traits
                .iter()
                .filter(|&t| *t == PersonaTrait::Creative)
                .count(),
            1
        );
    }
}
