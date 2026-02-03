//! Entity definitions for the knowledge graph.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Entity identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EntityId(pub Uuid);

impl EntityId {
    /// Create a new random entity ID.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Create from a UUID.
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

impl Default for EntityId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for EntityId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Entity types.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EntityType {
    /// A person.
    Person,
    /// An organization.
    Organization,
    /// A location.
    Location,
    /// An event.
    Event,
    /// A concept or idea.
    Concept,
    /// A document.
    Document,
    /// A task or action.
    Task,
    /// A time period.
    TimePeriod,
    /// A custom type.
    Custom(String),
}

impl std::fmt::Display for EntityType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EntityType::Person => write!(f, "Person"),
            EntityType::Organization => write!(f, "Organization"),
            EntityType::Location => write!(f, "Location"),
            EntityType::Event => write!(f, "Event"),
            EntityType::Concept => write!(f, "Concept"),
            EntityType::Document => write!(f, "Document"),
            EntityType::Task => write!(f, "Task"),
            EntityType::TimePeriod => write!(f, "TimePeriod"),
            EntityType::Custom(s) => write!(f, "{}", s),
        }
    }
}

/// Property value.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Property {
    String(String),
    Integer(i64),
    Float(f64),
    Boolean(bool),
    DateTime(chrono::DateTime<chrono::Utc>),
    List(Vec<Property>),
    Map(HashMap<String, Property>),
    Null,
}

impl Property {
    /// Get as string.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Property::String(s) => Some(s),
            _ => None,
        }
    }

    /// Get as integer.
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Property::Integer(i) => Some(*i),
            _ => None,
        }
    }

    /// Get as float.
    pub fn as_float(&self) -> Option<f64> {
        match self {
            Property::Float(f) => Some(*f),
            Property::Integer(i) => Some(*i as f64),
            _ => None,
        }
    }

    /// Get as boolean.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Property::Boolean(b) => Some(*b),
            _ => None,
        }
    }
}

impl From<String> for Property {
    fn from(s: String) -> Self {
        Property::String(s)
    }
}

impl From<&str> for Property {
    fn from(s: &str) -> Self {
        Property::String(s.to_string())
    }
}

impl From<i64> for Property {
    fn from(i: i64) -> Self {
        Property::Integer(i)
    }
}

impl From<i32> for Property {
    fn from(i: i32) -> Self {
        Property::Integer(i as i64)
    }
}

impl From<f64> for Property {
    fn from(f: f64) -> Self {
        Property::Float(f)
    }
}

impl From<bool> for Property {
    fn from(b: bool) -> Self {
        Property::Boolean(b)
    }
}

/// An entity in the knowledge graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    /// Unique identifier.
    pub id: EntityId,
    /// Entity name/label.
    pub name: String,
    /// Entity type.
    pub entity_type: EntityType,
    /// Properties/attributes.
    pub properties: HashMap<String, Property>,
    /// When this entity was created.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// When this entity was last updated.
    pub updated_at: chrono::DateTime<chrono::Utc>,
    /// Optional embedding for semantic search.
    pub embedding: Option<Vec<f32>>,
    /// Aliases for this entity.
    pub aliases: Vec<String>,
}

impl Entity {
    /// Create a new entity.
    pub fn new(name: impl Into<String>, entity_type: EntityType) -> Self {
        let now = chrono::Utc::now();
        Self {
            id: EntityId::new(),
            name: name.into(),
            entity_type,
            properties: HashMap::new(),
            created_at: now,
            updated_at: now,
            embedding: None,
            aliases: Vec::new(),
        }
    }

    /// Add a property.
    pub fn with_property(mut self, key: impl Into<String>, value: impl Into<Property>) -> Self {
        self.properties.insert(key.into(), value.into());
        self
    }

    /// Add an alias.
    pub fn with_alias(mut self, alias: impl Into<String>) -> Self {
        self.aliases.push(alias.into());
        self
    }

    /// Set embedding.
    pub fn with_embedding(mut self, embedding: Vec<f32>) -> Self {
        self.embedding = Some(embedding);
        self
    }

    /// Get a property.
    pub fn get_property(&self, key: &str) -> Option<&Property> {
        self.properties.get(key)
    }

    /// Set a property.
    pub fn set_property(&mut self, key: impl Into<String>, value: impl Into<Property>) {
        self.properties.insert(key.into(), value.into());
        self.updated_at = chrono::Utc::now();
    }

    /// Check if entity matches a name or alias.
    pub fn matches_name(&self, query: &str) -> bool {
        let query_lower = query.to_lowercase();
        self.name.to_lowercase().contains(&query_lower)
            || self
                .aliases
                .iter()
                .any(|a| a.to_lowercase().contains(&query_lower))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entity_creation() {
        let entity = Entity::new("Alice", EntityType::Person)
            .with_property("age", 30)
            .with_property("email", "alice@example.com")
            .with_alias("Ali");

        assert_eq!(entity.name, "Alice");
        assert_eq!(entity.entity_type, EntityType::Person);
        assert_eq!(
            entity.get_property("age").and_then(|p| p.as_int()),
            Some(30)
        );
        assert!(entity.matches_name("ali"));
    }

    #[test]
    fn test_property_conversions() {
        let _s: Property = "hello".into();
        let _i: Property = 42i32.into();
        let _f: Property = 3.14f64.into();
        let _b: Property = true.into();
    }

    #[test]
    fn test_entity_id() {
        let id1 = EntityId::new();
        let id2 = EntityId::new();
        assert_ne!(id1, id2);
    }
}
