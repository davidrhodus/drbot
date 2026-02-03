//! Semantic knowledge graph for drbot.
//!
//! Stores and queries knowledge in a graph structure with entities,
//! relationships, and semantic properties.
//!
//! # Features
//!
//! - Entity and relationship storage
//! - Semantic similarity search
//! - Path finding and traversal
//! - Temporal knowledge tracking
//! - Import/export capabilities
//!
//! # Example
//!
//! ```rust,no_run
//! use drbot_graph::{KnowledgeGraph, Entity, Relation, EntityType};
//!
//! async fn example() {
//!     let mut graph = KnowledgeGraph::new();
//!
//!     // Add entities
//!     let person = graph.add_entity(
//!         Entity::new("Alice", EntityType::Person)
//!             .with_property("age", 30)
//!     );
//!
//!     let company = graph.add_entity(
//!         Entity::new("Acme Corp", EntityType::Organization)
//!     );
//!
//!     // Add relationship
//!     graph.add_relation(person, company, Relation::WorksAt);
//! }
//! ```

mod entity;
mod graph;
mod query;
mod relation;
mod storage;

pub use entity::{Entity, EntityId, EntityType, Property};
pub use graph::{GraphConfig, GraphStats, KnowledgeGraph};
pub use query::{GraphQuery, PathQuery, QueryResult};
pub use relation::{Relation, RelationId, RelationType};
pub use storage::{GraphStorage, MemoryGraphStorage};

/// Result type for graph operations.
pub type Result<T> = std::result::Result<T, GraphError>;

/// Graph errors.
#[derive(Debug, thiserror::Error)]
pub enum GraphError {
    #[error("Entity not found: {0}")]
    EntityNotFound(String),
    #[error("Relation not found: {0}")]
    RelationNotFound(String),
    #[error("Invalid query: {0}")]
    InvalidQuery(String),
    #[error("Storage error: {0}")]
    StorageError(String),
    #[error("Cycle detected")]
    CycleDetected,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn test_basic_graph() {
        let graph = KnowledgeGraph::new();

        let e1 = graph
            .add_entity_async(Entity::new("Test", EntityType::Concept))
            .await
            .unwrap();
        let e2 = graph
            .add_entity_async(Entity::new("Other", EntityType::Concept))
            .await
            .unwrap();

        graph
            .add_relation_async(e1, e2, Relation::RelatedTo)
            .await
            .unwrap();

        assert_eq!(graph.entity_count(), 2);
        assert_eq!(graph.relation_count(), 1);
    }
}
