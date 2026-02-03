//! Main knowledge graph implementation.

use crate::entity::{Entity, EntityId, EntityType};
use crate::query::{GraphPath, GraphQuery, PathQuery, QueryResult};
use crate::relation::{Relation, RelationType};
use crate::storage::{GraphStorage, MemoryGraphStorage};
use crate::{GraphError, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use tracing::{debug, info};

/// Knowledge graph configuration.
#[derive(Debug, Clone)]
pub struct GraphConfig {
    /// Maximum number of entities.
    pub max_entities: usize,
    /// Maximum path length for queries.
    pub max_path_length: usize,
    /// Enable semantic similarity.
    pub enable_semantic: bool,
}

impl Default for GraphConfig {
    fn default() -> Self {
        Self {
            max_entities: 100000,
            max_path_length: 10,
            enable_semantic: true,
        }
    }
}

/// Graph statistics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GraphStats {
    /// Total number of entities.
    pub entity_count: usize,
    /// Total number of relations.
    pub relation_count: usize,
    /// Entities by type.
    pub entities_by_type: HashMap<String, usize>,
    /// Relations by type.
    pub relations_by_type: HashMap<String, usize>,
}

/// A semantic knowledge graph.
pub struct KnowledgeGraph {
    config: GraphConfig,
    storage: Arc<dyn GraphStorage>,
}

impl KnowledgeGraph {
    /// Create a new knowledge graph with default storage.
    pub fn new() -> Self {
        Self {
            config: GraphConfig::default(),
            storage: Arc::new(MemoryGraphStorage::new()),
        }
    }

    /// Create with custom config and storage.
    pub fn with_storage(config: GraphConfig, storage: Arc<dyn GraphStorage>) -> Self {
        Self { config, storage }
    }

    /// Add an entity to the graph.
    pub fn add_entity(&self, entity: Entity) -> EntityId {
        let id = entity.id;
        let storage = self.storage.clone();
        tokio::spawn(async move {
            let _ = storage.store_entity(entity).await;
        });
        id
    }

    /// Add an entity and wait for completion.
    pub async fn add_entity_async(&self, entity: Entity) -> Result<EntityId> {
        self.storage.store_entity(entity).await
    }

    /// Get an entity by ID.
    pub async fn get_entity(&self, id: EntityId) -> Result<Option<Entity>> {
        self.storage.get_entity(id).await
    }

    /// Update an entity.
    pub async fn update_entity(&self, entity: Entity) -> Result<()> {
        self.storage.update_entity(entity).await
    }

    /// Delete an entity.
    pub async fn delete_entity(&self, id: EntityId) -> Result<()> {
        self.storage.delete_entity(id).await
    }

    /// Add a relation between entities.
    pub fn add_relation(&self, source: EntityId, target: EntityId, relation: Relation) {
        let rel = RelationType::new(source, target, relation);
        let storage = self.storage.clone();
        tokio::spawn(async move {
            let _ = storage.store_relation(rel).await;
        });
    }

    /// Add a relation and wait for completion.
    pub async fn add_relation_async(
        &self,
        source: EntityId,
        target: EntityId,
        relation: Relation,
    ) -> Result<()> {
        let rel = RelationType::new(source, target, relation);
        self.storage.store_relation(rel).await
    }

    /// Add a bidirectional relation.
    pub async fn add_bidirectional_relation(
        &self,
        entity1: EntityId,
        entity2: EntityId,
        relation: Relation,
    ) -> Result<()> {
        self.add_relation_async(entity1, entity2, relation.clone())
            .await?;
        self.add_relation_async(entity2, entity1, relation).await
    }

    /// Get relations for an entity.
    pub async fn get_relations(&self, entity_id: EntityId) -> Result<Vec<RelationType>> {
        self.storage.get_relations(entity_id).await
    }

    /// Delete a relation.
    pub async fn delete_relation(
        &self,
        source: EntityId,
        target: EntityId,
        relation: &Relation,
    ) -> Result<()> {
        self.storage.delete_relation(source, target, relation).await
    }

    /// Execute a query.
    pub async fn query(&self, query: GraphQuery) -> Result<QueryResult> {
        let entities = self.storage.all_entities().await?;
        let mut results: Vec<Entity> = Vec::new();

        for entity in entities {
            let mut matches = true;

            // Check entity type
            if let Some(ref entity_type) = query.entity_type {
                if &entity.entity_type != entity_type {
                    matches = false;
                }
            }

            // Check name contains
            if matches {
                if let Some(ref name_contains) = query.name_contains {
                    if !entity.matches_name(name_contains) {
                        matches = false;
                    }
                }
            }

            if matches {
                results.push(entity);
            }
        }

        // Apply offset and limit
        let total = results.len();
        let results: Vec<Entity> = results
            .into_iter()
            .skip(query.offset)
            .take(query.limit.unwrap_or(total))
            .collect();

        Ok(QueryResult {
            entities: results,
            relations: Vec::new(),
            paths: Vec::new(),
        })
    }

    /// Find paths between entities.
    pub async fn find_paths(&self, query: PathQuery) -> Result<Vec<GraphPath>> {
        let all_relations = self.storage.all_relations().await?;

        // Build adjacency list
        let mut adjacency: HashMap<EntityId, Vec<(EntityId, Relation)>> = HashMap::new();
        for rel in &all_relations {
            // Check if relation is allowed
            if !query.allowed_relations.is_empty()
                && !query.allowed_relations.contains(&rel.relation)
            {
                continue;
            }

            adjacency
                .entry(rel.source)
                .or_default()
                .push((rel.target, rel.relation.clone()));

            // Add reverse for bidirectional relations
            if !rel.relation.is_directional() {
                adjacency
                    .entry(rel.target)
                    .or_default()
                    .push((rel.source, rel.relation.clone()));
            }
        }

        // BFS to find paths
        let mut paths: Vec<GraphPath> = Vec::new();
        let mut queue: VecDeque<GraphPath> = VecDeque::new();
        let mut visited: HashSet<EntityId> = HashSet::new();

        queue.push_back(GraphPath::new(query.from));

        while let Some(path) = queue.pop_front() {
            let current = path.end().unwrap();

            if current == query.to {
                paths.push(path);
                if !query.find_all {
                    break;
                }
                continue;
            }

            if path.length >= query.max_length {
                continue;
            }

            if !query.find_all && visited.contains(&current) {
                continue;
            }
            visited.insert(current);

            if let Some(neighbors) = adjacency.get(&current) {
                for (neighbor, relation) in neighbors {
                    if !path.entities.contains(neighbor) {
                        let mut new_path = path.clone();
                        new_path.extend(relation.clone(), *neighbor);
                        queue.push_back(new_path);
                    }
                }
            }
        }

        debug!(
            "Found {} paths from {} to {}",
            paths.len(),
            query.from,
            query.to
        );
        Ok(paths)
    }

    /// Find entities related to a given entity.
    pub async fn find_related(&self, entity_id: EntityId, max_depth: usize) -> Result<Vec<Entity>> {
        let mut related: HashSet<EntityId> = HashSet::new();
        let mut queue: VecDeque<(EntityId, usize)> = VecDeque::new();

        queue.push_back((entity_id, 0));

        while let Some((current, depth)) = queue.pop_front() {
            if depth > max_depth {
                continue;
            }

            if related.contains(&current) {
                continue;
            }
            related.insert(current);

            let relations = self.storage.get_relations(current).await?;
            for rel in relations {
                let other = if rel.source == current {
                    rel.target
                } else {
                    rel.source
                };
                if !related.contains(&other) {
                    queue.push_back((other, depth + 1));
                }
            }
        }

        // Remove the starting entity
        related.remove(&entity_id);

        // Fetch full entities
        let mut entities = Vec::new();
        for id in related {
            if let Ok(Some(entity)) = self.storage.get_entity(id).await {
                entities.push(entity);
            }
        }

        Ok(entities)
    }

    /// Get entity count.
    pub fn entity_count(&self) -> usize {
        // Synchronous helper - use blocking
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(async { self.storage.entity_count().await.unwrap_or(0) })
        })
    }

    /// Get relation count.
    pub fn relation_count(&self) -> usize {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(async { self.storage.relation_count().await.unwrap_or(0) })
        })
    }

    /// Get graph statistics.
    pub async fn stats(&self) -> Result<GraphStats> {
        let entities = self.storage.all_entities().await?;
        let relations = self.storage.all_relations().await?;

        let mut entities_by_type: HashMap<String, usize> = HashMap::new();
        for entity in &entities {
            *entities_by_type
                .entry(entity.entity_type.to_string())
                .or_default() += 1;
        }

        let mut relations_by_type: HashMap<String, usize> = HashMap::new();
        for rel in &relations {
            *relations_by_type
                .entry(rel.relation.to_string())
                .or_default() += 1;
        }

        Ok(GraphStats {
            entity_count: entities.len(),
            relation_count: relations.len(),
            entities_by_type,
            relations_by_type,
        })
    }

    /// Clear the graph.
    pub async fn clear(&self) -> Result<()> {
        self.storage.clear().await
    }

    /// Get the config.
    pub fn config(&self) -> &GraphConfig {
        &self.config
    }
}

impl Default for KnowledgeGraph {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_graph_operations() {
        let graph = KnowledgeGraph::new();

        let alice = graph
            .add_entity_async(Entity::new("Alice", EntityType::Person))
            .await
            .unwrap();
        let bob = graph
            .add_entity_async(Entity::new("Bob", EntityType::Person))
            .await
            .unwrap();
        let acme = graph
            .add_entity_async(Entity::new("Acme", EntityType::Organization))
            .await
            .unwrap();

        graph
            .add_relation_async(alice, bob, Relation::Knows)
            .await
            .unwrap();
        graph
            .add_relation_async(alice, acme, Relation::WorksAt)
            .await
            .unwrap();
        graph
            .add_relation_async(bob, acme, Relation::WorksAt)
            .await
            .unwrap();

        let stats = graph.stats().await.unwrap();
        assert_eq!(stats.entity_count, 3);
        assert_eq!(stats.relation_count, 3);
    }

    #[tokio::test]
    async fn test_find_paths() {
        let graph = KnowledgeGraph::new();

        let a = graph
            .add_entity_async(Entity::new("A", EntityType::Concept))
            .await
            .unwrap();
        let b = graph
            .add_entity_async(Entity::new("B", EntityType::Concept))
            .await
            .unwrap();
        let c = graph
            .add_entity_async(Entity::new("C", EntityType::Concept))
            .await
            .unwrap();

        graph
            .add_relation_async(a, b, Relation::RelatedTo)
            .await
            .unwrap();
        graph
            .add_relation_async(b, c, Relation::RelatedTo)
            .await
            .unwrap();

        let paths = graph
            .find_paths(PathQuery::new(a, c).max_length(3))
            .await
            .unwrap();
        assert!(!paths.is_empty());
        assert_eq!(paths[0].length, 2);
    }

    #[tokio::test]
    async fn test_query() {
        let graph = KnowledgeGraph::new();

        graph
            .add_entity_async(Entity::new("Alice", EntityType::Person))
            .await
            .unwrap();
        graph
            .add_entity_async(Entity::new("Bob", EntityType::Person))
            .await
            .unwrap();
        graph
            .add_entity_async(Entity::new("Acme", EntityType::Organization))
            .await
            .unwrap();

        let result = graph
            .query(GraphQuery::new().of_type(EntityType::Person))
            .await
            .unwrap();

        assert_eq!(result.entities.len(), 2);
    }
}
