//! Query capabilities for the knowledge graph.

use crate::entity::{Entity, EntityId, EntityType};
use crate::relation::{Relation, RelationType};
use serde::{Deserialize, Serialize};

/// Query result.
#[derive(Debug, Clone)]
pub struct QueryResult {
    /// Matching entities.
    pub entities: Vec<Entity>,
    /// Matching relations.
    pub relations: Vec<RelationType>,
    /// Paths found (for path queries).
    pub paths: Vec<GraphPath>,
}

impl QueryResult {
    /// Create an empty result.
    pub fn empty() -> Self {
        Self {
            entities: Vec::new(),
            relations: Vec::new(),
            paths: Vec::new(),
        }
    }

    /// Check if result is empty.
    pub fn is_empty(&self) -> bool {
        self.entities.is_empty() && self.relations.is_empty() && self.paths.is_empty()
    }
}

/// A path in the graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphPath {
    /// Entities in the path.
    pub entities: Vec<EntityId>,
    /// Relations connecting the entities.
    pub relations: Vec<Relation>,
    /// Total path length.
    pub length: usize,
}

impl GraphPath {
    /// Create a new path starting from an entity.
    pub fn new(start: EntityId) -> Self {
        Self {
            entities: vec![start],
            relations: Vec::new(),
            length: 0,
        }
    }

    /// Extend the path.
    pub fn extend(&mut self, relation: Relation, entity: EntityId) {
        self.relations.push(relation);
        self.entities.push(entity);
        self.length += 1;
    }

    /// Get the start entity.
    pub fn start(&self) -> Option<EntityId> {
        self.entities.first().copied()
    }

    /// Get the end entity.
    pub fn end(&self) -> Option<EntityId> {
        self.entities.last().copied()
    }
}

/// Graph query builder.
#[derive(Debug, Clone, Default)]
pub struct GraphQuery {
    /// Entity type filter.
    pub entity_type: Option<EntityType>,
    /// Name contains filter.
    pub name_contains: Option<String>,
    /// Property filters.
    pub property_filters: Vec<PropertyFilter>,
    /// Relation filters.
    pub relation_filters: Vec<RelationFilter>,
    /// Maximum results.
    pub limit: Option<usize>,
    /// Offset for pagination.
    pub offset: usize,
}

/// Property filter.
#[derive(Debug, Clone)]
pub struct PropertyFilter {
    /// Property key.
    pub key: String,
    /// Filter operation.
    pub op: FilterOp,
    /// Value to compare.
    pub value: serde_json::Value,
}

/// Filter operation.
#[derive(Debug, Clone, Copy)]
pub enum FilterOp {
    Equals,
    NotEquals,
    Contains,
    GreaterThan,
    LessThan,
    GreaterThanOrEqual,
    LessThanOrEqual,
    Exists,
    NotExists,
}

/// Relation filter.
#[derive(Debug, Clone)]
pub struct RelationFilter {
    /// Relation type.
    pub relation: Option<Relation>,
    /// Direction (outgoing, incoming, any).
    pub direction: RelationDirection,
    /// Target entity type.
    pub target_type: Option<EntityType>,
}

/// Relation direction.
#[derive(Debug, Clone, Copy, Default)]
pub enum RelationDirection {
    Outgoing,
    Incoming,
    #[default]
    Any,
}

impl GraphQuery {
    /// Create a new query.
    pub fn new() -> Self {
        Self::default()
    }

    /// Filter by entity type.
    pub fn of_type(mut self, entity_type: EntityType) -> Self {
        self.entity_type = Some(entity_type);
        self
    }

    /// Filter by name containing.
    pub fn name_contains(mut self, text: impl Into<String>) -> Self {
        self.name_contains = Some(text.into());
        self
    }

    /// Add property filter.
    pub fn where_property(
        mut self,
        key: impl Into<String>,
        op: FilterOp,
        value: serde_json::Value,
    ) -> Self {
        self.property_filters.push(PropertyFilter {
            key: key.into(),
            op,
            value,
        });
        self
    }

    /// Filter by having a relation.
    pub fn has_relation(mut self, relation: Relation, direction: RelationDirection) -> Self {
        self.relation_filters.push(RelationFilter {
            relation: Some(relation),
            direction,
            target_type: None,
        });
        self
    }

    /// Limit results.
    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Set offset.
    pub fn offset(mut self, offset: usize) -> Self {
        self.offset = offset;
        self
    }
}

/// Path query for finding connections.
#[derive(Debug, Clone)]
pub struct PathQuery {
    /// Starting entity.
    pub from: EntityId,
    /// Target entity.
    pub to: EntityId,
    /// Maximum path length.
    pub max_length: usize,
    /// Allowed relation types (empty = all).
    pub allowed_relations: Vec<Relation>,
    /// Find all paths or just shortest.
    pub find_all: bool,
}

impl PathQuery {
    /// Create a new path query.
    pub fn new(from: EntityId, to: EntityId) -> Self {
        Self {
            from,
            to,
            max_length: 5,
            allowed_relations: Vec::new(),
            find_all: false,
        }
    }

    /// Set maximum path length.
    pub fn max_length(mut self, length: usize) -> Self {
        self.max_length = length;
        self
    }

    /// Restrict to certain relation types.
    pub fn via_relations(mut self, relations: Vec<Relation>) -> Self {
        self.allowed_relations = relations;
        self
    }

    /// Find all paths instead of just shortest.
    pub fn find_all(mut self) -> Self {
        self.find_all = true;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_builder() {
        let query = GraphQuery::new()
            .of_type(EntityType::Person)
            .name_contains("John")
            .limit(10);

        assert_eq!(query.entity_type, Some(EntityType::Person));
        assert_eq!(query.name_contains, Some("John".to_string()));
        assert_eq!(query.limit, Some(10));
    }

    #[test]
    fn test_path_creation() {
        let start = EntityId::new();
        let end = EntityId::new();

        let mut path = GraphPath::new(start);
        path.extend(Relation::Knows, end);

        assert_eq!(path.start(), Some(start));
        assert_eq!(path.end(), Some(end));
        assert_eq!(path.length, 1);
    }

    #[test]
    fn test_path_query() {
        let from = EntityId::new();
        let to = EntityId::new();

        let query = PathQuery::new(from, to).max_length(3).find_all();

        assert_eq!(query.max_length, 3);
        assert!(query.find_all);
    }
}
