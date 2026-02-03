//! Relation definitions for the knowledge graph.

use crate::entity::EntityId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Relation identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RelationId(pub Uuid);

impl RelationId {
    /// Create a new random relation ID.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for RelationId {
    fn default() -> Self {
        Self::new()
    }
}

/// Relation type enum.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Relation {
    // Person relations
    /// A person knows another person.
    Knows,
    /// A person works at an organization.
    WorksAt,
    /// A person lives in a location.
    LivesIn,
    /// A person is related to another person.
    RelatedTo,
    /// A person manages another person.
    Manages,
    /// A person reports to another person.
    ReportsTo,

    // Organization relations
    /// An organization is located in a location.
    LocatedIn,
    /// An organization owns something.
    Owns,
    /// An organization is part of another organization.
    PartOf,
    /// An organization collaborates with another.
    CollaboratesWith,

    // Concept relations
    /// Something is a type of another.
    IsA,
    /// Something has a part.
    HasPart,
    /// Something is similar to another.
    SimilarTo,
    /// Something is opposite of another.
    OppositeOf,
    /// Something causes another.
    Causes,
    /// Something results in another.
    ResultsIn,

    // Task relations
    /// A task depends on another.
    DependsOn,
    /// A task blocks another.
    Blocks,
    /// A task is assigned to someone.
    AssignedTo,

    // Document relations
    /// Something references another.
    References,
    /// Something mentions another.
    Mentions,
    /// Something is about a topic.
    About,

    // Time relations
    /// Something happened before another.
    Before,
    /// Something happened after another.
    After,
    /// Something happened during another.
    During,

    // Generic
    /// A custom relation type.
    Custom(String),
}

impl Relation {
    /// Check if this relation is directional.
    pub fn is_directional(&self) -> bool {
        !matches!(
            self,
            Relation::Knows
                | Relation::RelatedTo
                | Relation::SimilarTo
                | Relation::CollaboratesWith
        )
    }

    /// Get the inverse relation (if applicable).
    pub fn inverse(&self) -> Option<Relation> {
        match self {
            Relation::Manages => Some(Relation::ReportsTo),
            Relation::ReportsTo => Some(Relation::Manages),
            Relation::PartOf => Some(Relation::HasPart),
            Relation::HasPart => Some(Relation::PartOf),
            Relation::Causes => Some(Relation::ResultsIn),
            Relation::ResultsIn => Some(Relation::Causes),
            Relation::DependsOn => Some(Relation::Blocks),
            Relation::Blocks => Some(Relation::DependsOn),
            Relation::Before => Some(Relation::After),
            Relation::After => Some(Relation::Before),
            _ => None,
        }
    }
}

impl std::fmt::Display for Relation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Relation::Knows => write!(f, "knows"),
            Relation::WorksAt => write!(f, "works_at"),
            Relation::LivesIn => write!(f, "lives_in"),
            Relation::RelatedTo => write!(f, "related_to"),
            Relation::Manages => write!(f, "manages"),
            Relation::ReportsTo => write!(f, "reports_to"),
            Relation::LocatedIn => write!(f, "located_in"),
            Relation::Owns => write!(f, "owns"),
            Relation::PartOf => write!(f, "part_of"),
            Relation::CollaboratesWith => write!(f, "collaborates_with"),
            Relation::IsA => write!(f, "is_a"),
            Relation::HasPart => write!(f, "has_part"),
            Relation::SimilarTo => write!(f, "similar_to"),
            Relation::OppositeOf => write!(f, "opposite_of"),
            Relation::Causes => write!(f, "causes"),
            Relation::ResultsIn => write!(f, "results_in"),
            Relation::DependsOn => write!(f, "depends_on"),
            Relation::Blocks => write!(f, "blocks"),
            Relation::AssignedTo => write!(f, "assigned_to"),
            Relation::References => write!(f, "references"),
            Relation::Mentions => write!(f, "mentions"),
            Relation::About => write!(f, "about"),
            Relation::Before => write!(f, "before"),
            Relation::After => write!(f, "after"),
            Relation::During => write!(f, "during"),
            Relation::Custom(s) => write!(f, "{}", s),
        }
    }
}

/// A typed relation in the graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationType {
    /// Unique identifier.
    pub id: RelationId,
    /// Source entity.
    pub source: EntityId,
    /// Target entity.
    pub target: EntityId,
    /// Relation type.
    pub relation: Relation,
    /// Relation properties.
    pub properties: HashMap<String, serde_json::Value>,
    /// Confidence score (0.0 - 1.0).
    pub confidence: f32,
    /// When this relation was created.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// When this relation was last verified.
    pub verified_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl RelationType {
    /// Create a new relation.
    pub fn new(source: EntityId, target: EntityId, relation: Relation) -> Self {
        Self {
            id: RelationId::new(),
            source,
            target,
            relation,
            properties: HashMap::new(),
            confidence: 1.0,
            created_at: chrono::Utc::now(),
            verified_at: None,
        }
    }

    /// Set confidence.
    pub fn with_confidence(mut self, confidence: f32) -> Self {
        self.confidence = confidence.clamp(0.0, 1.0);
        self
    }

    /// Add a property.
    pub fn with_property(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.properties.insert(key.into(), value);
        self
    }

    /// Mark as verified.
    pub fn verify(&mut self) {
        self.verified_at = Some(chrono::Utc::now());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_relation_directional() {
        assert!(Relation::WorksAt.is_directional());
        assert!(!Relation::Knows.is_directional());
    }

    #[test]
    fn test_relation_inverse() {
        assert_eq!(Relation::Manages.inverse(), Some(Relation::ReportsTo));
        assert_eq!(Relation::Knows.inverse(), None);
    }

    #[test]
    fn test_relation_type_creation() {
        let source = EntityId::new();
        let target = EntityId::new();
        let rel = RelationType::new(source, target, Relation::WorksAt).with_confidence(0.9);

        assert_eq!(rel.source, source);
        assert_eq!(rel.target, target);
        assert_eq!(rel.confidence, 0.9);
    }
}
