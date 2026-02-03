//! Persona builder for fluent API.

use crate::persona::{Persona, PersonaExample, PersonaStyle, PersonaTrait};

/// Builder for creating personas.
pub struct PersonaBuilder {
    persona: Persona,
}

impl PersonaBuilder {
    /// Create a new builder.
    pub fn new(id: &str) -> Self {
        Self {
            persona: Persona::new(id, id),
        }
    }

    /// Set the name.
    pub fn name(mut self, name: &str) -> Self {
        self.persona.name = name.to_string();
        self
    }

    /// Set the description.
    pub fn description(mut self, description: &str) -> Self {
        self.persona.description = description.to_string();
        self
    }

    /// Set the communication style.
    pub fn style(mut self, style: PersonaStyle) -> Self {
        self.persona.style = style;
        self
    }

    /// Add a trait.
    pub fn trait_(mut self, trait_: PersonaTrait) -> Self {
        self.persona.add_trait(trait_);
        self
    }

    /// Set traits (replaces existing).
    pub fn traits(mut self, traits: Vec<PersonaTrait>) -> Self {
        self.persona.traits = traits;
        self
    }

    /// Set the system prompt.
    pub fn system_prompt(mut self, prompt: &str) -> Self {
        self.persona.system_prompt = Some(prompt.to_string());
        self
    }

    /// Add an example interaction.
    pub fn example(mut self, user_input: &str, response: &str) -> Self {
        self.persona
            .examples
            .push(PersonaExample::new(user_input, response));
        self
    }

    /// Add multiple examples.
    pub fn examples(mut self, examples: Vec<(&str, &str)>) -> Self {
        for (input, response) in examples {
            self.persona
                .examples
                .push(PersonaExample::new(input, response));
        }
        self
    }

    /// Set a parameter.
    pub fn param(mut self, key: &str, value: serde_json::Value) -> Self {
        self.persona.set_param(key, value);
        self
    }

    /// Set temperature.
    pub fn temperature(mut self, temp: f32) -> Self {
        self.persona.temperature = Some(temp);
        self
    }

    /// Set enabled state.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.persona.enabled = enabled;
        self
    }

    /// Build the persona.
    pub fn build(self) -> Persona {
        self.persona
    }
}

/// Quick helper functions for creating common persona types.
pub mod presets {
    use super::*;

    /// Create a professional assistant persona.
    pub fn professional(id: &str, name: &str) -> Persona {
        PersonaBuilder::new(id)
            .name(name)
            .description("a professional AI assistant")
            .style(PersonaStyle::Professional)
            .trait_(PersonaTrait::Helpful)
            .trait_(PersonaTrait::Accurate)
            .trait_(PersonaTrait::Thorough)
            .build()
    }

    /// Create a casual friend persona.
    pub fn casual(id: &str, name: &str) -> Persona {
        PersonaBuilder::new(id)
            .name(name)
            .description("a friendly chat companion")
            .style(PersonaStyle::Casual)
            .trait_(PersonaTrait::Friendly)
            .trait_(PersonaTrait::Humorous)
            .trait_(PersonaTrait::Enthusiastic)
            .temperature(0.9)
            .build()
    }

    /// Create a technical expert persona.
    pub fn technical(id: &str, name: &str) -> Persona {
        PersonaBuilder::new(id)
            .name(name)
            .description("a technical expert")
            .style(PersonaStyle::Technical)
            .trait_(PersonaTrait::Precise)
            .trait_(PersonaTrait::Accurate)
            .trait_(PersonaTrait::Thorough)
            .temperature(0.3)
            .build()
    }

    /// Create a creative writer persona.
    pub fn creative(id: &str, name: &str) -> Persona {
        PersonaBuilder::new(id)
            .name(name)
            .description("a creative writing assistant")
            .style(PersonaStyle::Creative)
            .trait_(PersonaTrait::Creative)
            .trait_(PersonaTrait::Expressive)
            .trait_(PersonaTrait::Enthusiastic)
            .temperature(1.0)
            .build()
    }

    /// Create a teacher persona.
    pub fn teacher(id: &str, name: &str) -> Persona {
        PersonaBuilder::new(id)
            .name(name)
            .description("an educational tutor")
            .style(PersonaStyle::Educational)
            .trait_(PersonaTrait::Patient)
            .trait_(PersonaTrait::Helpful)
            .trait_(PersonaTrait::Thorough)
            .temperature(0.5)
            .build()
    }

    /// Create a concise assistant persona.
    pub fn concise(id: &str, name: &str) -> Persona {
        PersonaBuilder::new(id)
            .name(name)
            .description("a brief and direct assistant")
            .style(PersonaStyle::Concise)
            .trait_(PersonaTrait::Accurate)
            .trait_(PersonaTrait::Precise)
            .temperature(0.3)
            .build()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder() {
        let persona = PersonaBuilder::new("test")
            .name("Test Bot")
            .description("A test persona")
            .style(PersonaStyle::Casual)
            .trait_(PersonaTrait::Friendly)
            .trait_(PersonaTrait::Humorous)
            .example("Hello", "Hey there! How's it going?")
            .temperature(0.8)
            .build();

        assert_eq!(persona.id, "test");
        assert_eq!(persona.name, "Test Bot");
        assert_eq!(persona.style, PersonaStyle::Casual);
        assert!(persona.has_trait(PersonaTrait::Friendly));
        assert_eq!(persona.examples.len(), 1);
        assert_eq!(persona.temperature, Some(0.8));
    }

    #[test]
    fn test_presets() {
        let professional = presets::professional("pro", "Pro Bot");
        assert_eq!(professional.style, PersonaStyle::Professional);

        let casual = presets::casual("fun", "Fun Bot");
        assert_eq!(casual.style, PersonaStyle::Casual);

        let technical = presets::technical("tech", "Tech Bot");
        assert!(technical.has_trait(PersonaTrait::Precise));
    }
}
