//! Storage backends for the knowledge graph.

use crate::entity::{Entity, EntityId};
use crate::relation::{Relation, RelationType};
use crate::{GraphError, Result};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Trait for graph storage backends.
#[async_trait]
pub trait GraphStorage: Send + Sync {
    /// Store an entity.
    async fn store_entity(&self, entity: Entity) -> Result<EntityId>;

    /// Get an entity by ID.
    async fn get_entity(&self, id: EntityId) -> Result<Option<Entity>>;

    /// Update an entity.
    async fn update_entity(&self, entity: Entity) -> Result<()>;

    /// Delete an entity.
    async fn delete_entity(&self, id: EntityId) -> Result<()>;

    /// Store a relation.
    async fn store_relation(&self, relation: RelationType) -> Result<()>;

    /// Get relations for an entity.
    async fn get_relations(&self, entity_id: EntityId) -> Result<Vec<RelationType>>;

    /// Delete a relation.
    async fn delete_relation(
        &self,
        source: EntityId,
        target: EntityId,
        relation: &Relation,
    ) -> Result<()>;

    /// Get all entities.
    async fn all_entities(&self) -> Result<Vec<Entity>>;

    /// Get all relations.
    async fn all_relations(&self) -> Result<Vec<RelationType>>;

    /// Count entities.
    async fn entity_count(&self) -> Result<usize>;

    /// Count relations.
    async fn relation_count(&self) -> Result<usize>;

    /// Clear all data.
    async fn clear(&self) -> Result<()>;
}

/// In-memory graph storage.
#[derive(Debug, Default)]
pub struct MemoryGraphStorage {
    entities: Arc<RwLock<HashMap<EntityId, Entity>>>,
    relations: Arc<RwLock<Vec<RelationType>>>,
}

impl MemoryGraphStorage {
    /// Create a new memory storage.
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl GraphStorage for MemoryGraphStorage {
    async fn store_entity(&self, entity: Entity) -> Result<EntityId> {
        let id = entity.id;
        let mut entities = self.entities.write().await;
        entities.insert(id, entity);
        Ok(id)
    }

    async fn get_entity(&self, id: EntityId) -> Result<Option<Entity>> {
        let entities = self.entities.read().await;
        Ok(entities.get(&id).cloned())
    }

    async fn update_entity(&self, entity: Entity) -> Result<()> {
        let mut entities = self.entities.write().await;
        if entities.contains_key(&entity.id) {
            entities.insert(entity.id, entity);
            Ok(())
        } else {
            Err(GraphError::EntityNotFound(entity.id.to_string()))
        }
    }

    async fn delete_entity(&self, id: EntityId) -> Result<()> {
        let mut entities = self.entities.write().await;
        entities.remove(&id);

        // Also remove relations involving this entity
        let mut relations = self.relations.write().await;
        relations.retain(|r| r.source != id && r.target != id);

        Ok(())
    }

    async fn store_relation(&self, relation: RelationType) -> Result<()> {
        let mut relations = self.relations.write().await;
        relations.push(relation);
        Ok(())
    }

    async fn get_relations(&self, entity_id: EntityId) -> Result<Vec<RelationType>> {
        let relations = self.relations.read().await;
        Ok(relations
            .iter()
            .filter(|r| r.source == entity_id || r.target == entity_id)
            .cloned()
            .collect())
    }

    async fn delete_relation(
        &self,
        source: EntityId,
        target: EntityId,
        relation: &Relation,
    ) -> Result<()> {
        let mut relations = self.relations.write().await;
        relations
            .retain(|r| !(r.source == source && r.target == target && &r.relation == relation));
        Ok(())
    }

    async fn all_entities(&self) -> Result<Vec<Entity>> {
        let entities = self.entities.read().await;
        Ok(entities.values().cloned().collect())
    }

    async fn all_relations(&self) -> Result<Vec<RelationType>> {
        let relations = self.relations.read().await;
        Ok(relations.clone())
    }

    async fn entity_count(&self) -> Result<usize> {
        let entities = self.entities.read().await;
        Ok(entities.len())
    }

    async fn relation_count(&self) -> Result<usize> {
        let relations = self.relations.read().await;
        Ok(relations.len())
    }

    async fn clear(&self) -> Result<()> {
        let mut entities = self.entities.write().await;
        let mut relations = self.relations.write().await;
        entities.clear();
        relations.clear();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::EntityType;

    #[tokio::test]
    async fn test_memory_storage_entity() {
        let storage = MemoryGraphStorage::new();

        let entity = Entity::new("Test", EntityType::Concept);
        let id = storage.store_entity(entity.clone()).await.unwrap();

        let retrieved = storage.get_entity(id).await.unwrap().unwrap();
        assert_eq!(retrieved.name, "Test");

        assert_eq!(storage.entity_count().await.unwrap(), 1);

        storage.delete_entity(id).await.unwrap();
        assert_eq!(storage.entity_count().await.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_memory_storage_relation() {
        let storage = MemoryGraphStorage::new();

        let e1 = Entity::new("A", EntityType::Concept);
        let e2 = Entity::new("B", EntityType::Concept);

        let id1 = storage.store_entity(e1).await.unwrap();
        let id2 = storage.store_entity(e2).await.unwrap();

        let relation = RelationType::new(id1, id2, Relation::RelatedTo);
        storage.store_relation(relation).await.unwrap();

        let relations = storage.get_relations(id1).await.unwrap();
        assert_eq!(relations.len(), 1);

        storage
            .delete_relation(id1, id2, &Relation::RelatedTo)
            .await
            .unwrap();
        let relations = storage.get_relations(id1).await.unwrap();
        assert_eq!(relations.len(), 0);
    }
}
