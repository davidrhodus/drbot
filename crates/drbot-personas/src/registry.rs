//! Persona registry for managing personas.

use crate::{Persona, PersonaError, Result};
use std::collections::HashMap;
use std::path::Path;
use std::sync::RwLock;

/// Persona identifier.
pub type PersonaId = String;

/// Registry for managing personas.
pub struct PersonaRegistry {
    personas: RwLock<HashMap<PersonaId, Persona>>,
    default_persona: RwLock<Option<PersonaId>>,
    channel_personas: RwLock<HashMap<String, PersonaId>>,
}

impl PersonaRegistry {
    /// Create a new registry.
    pub fn new() -> Self {
        Self {
            personas: RwLock::new(HashMap::new()),
            default_persona: RwLock::new(None),
            channel_personas: RwLock::new(HashMap::new()),
        }
    }

    /// Register a persona.
    pub fn register(&self, persona: Persona) -> Result<()> {
        let mut personas = self.personas.write().unwrap();
        personas.insert(persona.id.clone(), persona);
        Ok(())
    }

    /// Get a persona by ID.
    pub fn get(&self, id: &str) -> Option<Persona> {
        let personas = self.personas.read().unwrap();
        personas.get(id).cloned()
    }

    /// Remove a persona.
    pub fn remove(&self, id: &str) -> Option<Persona> {
        let mut personas = self.personas.write().unwrap();
        personas.remove(id)
    }

    /// List all personas.
    pub fn list(&self) -> Vec<Persona> {
        let personas = self.personas.read().unwrap();
        personas.values().cloned().collect()
    }

    /// List enabled personas.
    pub fn list_enabled(&self) -> Vec<Persona> {
        let personas = self.personas.read().unwrap();
        personas.values().filter(|p| p.enabled).cloned().collect()
    }

    /// Set default persona.
    pub fn set_default(&self, id: &str) -> Result<()> {
        let personas = self.personas.read().unwrap();
        if !personas.contains_key(id) {
            return Err(PersonaError::NotFound(id.to_string()));
        }

        let mut default = self.default_persona.write().unwrap();
        *default = Some(id.to_string());
        Ok(())
    }

    /// Get default persona.
    pub fn get_default(&self) -> Option<Persona> {
        let default = self.default_persona.read().unwrap();
        if let Some(id) = default.as_ref() {
            self.get(id)
        } else {
            None
        }
    }

    /// Set persona for a channel.
    pub fn set_channel_persona(&self, channel_id: &str, persona_id: &str) -> Result<()> {
        let personas = self.personas.read().unwrap();
        if !personas.contains_key(persona_id) {
            return Err(PersonaError::NotFound(persona_id.to_string()));
        }

        let mut channel_personas = self.channel_personas.write().unwrap();
        channel_personas.insert(channel_id.to_string(), persona_id.to_string());
        Ok(())
    }

    /// Get persona for a channel.
    pub fn get_channel_persona(&self, channel_id: &str) -> Option<Persona> {
        let channel_personas = self.channel_personas.read().unwrap();
        if let Some(persona_id) = channel_personas.get(channel_id) {
            self.get(persona_id)
        } else {
            self.get_default()
        }
    }

    /// Clear channel persona.
    pub fn clear_channel_persona(&self, channel_id: &str) {
        let mut channel_personas = self.channel_personas.write().unwrap();
        channel_personas.remove(channel_id);
    }

    /// Load personas from a directory.
    pub fn load_from_dir(&self, dir: &Path) -> Result<usize> {
        let mut count = 0;

        if !dir.exists() {
            return Ok(0);
        }

        for entry in std::fs::read_dir(dir).map_err(|e| PersonaError::IoError(e))? {
            let entry = entry.map_err(|e| PersonaError::IoError(e))?;
            let path = entry.path();

            if path.extension().map(|e| e == "json").unwrap_or(false) {
                if let Ok(persona) = self.load_persona_file(&path) {
                    self.register(persona)?;
                    count += 1;
                }
            }
        }

        Ok(count)
    }

    /// Load a persona from a file.
    fn load_persona_file(&self, path: &Path) -> Result<Persona> {
        let content = std::fs::read_to_string(path).map_err(|e| PersonaError::IoError(e))?;

        serde_json::from_str(&content).map_err(|e| PersonaError::InvalidDefinition(e.to_string()))
    }

    /// Save a persona to a file.
    pub fn save_persona(&self, persona: &Persona, dir: &Path) -> Result<()> {
        std::fs::create_dir_all(dir).map_err(|e| PersonaError::IoError(e))?;

        let path = dir.join(format!("{}.json", persona.id));
        let content = serde_json::to_string_pretty(persona)
            .map_err(|e| PersonaError::InvalidDefinition(e.to_string()))?;

        std::fs::write(path, content).map_err(|e| PersonaError::IoError(e))
    }

    /// Get number of registered personas.
    pub fn count(&self) -> usize {
        let personas = self.personas.read().unwrap();
        personas.len()
    }
}

impl Default for PersonaRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PersonaBuilder;

    #[test]
    fn test_registry_basic() {
        let registry = PersonaRegistry::new();

        let persona = PersonaBuilder::new("test").name("Test Bot").build();

        registry.register(persona).unwrap();
        assert!(registry.get("test").is_some());
        assert_eq!(registry.count(), 1);
    }

    #[test]
    fn test_registry_default() {
        let registry = PersonaRegistry::new();

        let persona = PersonaBuilder::new("default").name("Default Bot").build();

        registry.register(persona).unwrap();
        registry.set_default("default").unwrap();

        let default = registry.get_default();
        assert!(default.is_some());
        assert_eq!(default.unwrap().id, "default");
    }

    #[test]
    fn test_channel_persona() {
        let registry = PersonaRegistry::new();

        let persona1 = PersonaBuilder::new("formal").name("Formal Bot").build();
        let persona2 = PersonaBuilder::new("casual").name("Casual Bot").build();

        registry.register(persona1).unwrap();
        registry.register(persona2).unwrap();
        registry.set_default("formal").unwrap();
        registry
            .set_channel_persona("fun-channel", "casual")
            .unwrap();

        let channel_persona = registry.get_channel_persona("fun-channel").unwrap();
        assert_eq!(channel_persona.id, "casual");

        let other_persona = registry.get_channel_persona("work-channel").unwrap();
        assert_eq!(other_persona.id, "formal"); // Falls back to default
    }
}
