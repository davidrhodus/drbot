//! Knowledge graph for drbot.
//!
//! Structured knowledge representation.
//!
//! # Features
//!
//! - Entity management
//! - Relationship modeling
//! - Graph queries
//! - Inference engine
//! - Knowledge extraction

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Knowledge graph result type.
pub type Result<T> = std::result::Result<T, KGraphError>;

/// Knowledge graph errors.
#[derive(Debug, thiserror::Error)]
pub enum KGraphError {
    #[error("Entity not found: {0}")]
    EntityNotFound(String),
    #[error("Relation not found: {0}")]
    RelationNotFound(String),
    #[error("Invalid query: {0}")]
    InvalidQuery(String),
    #[error("Cycle detected")]
    CycleDetected,
}

/// Entity type.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityType {
    Person,
    Organization,
    Place,
    Event,
    Concept,
    Document,
    Product,
    Date,
    Custom(String),
}

/// Entity in the knowledge graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    /// Entity ID.
    pub id: Uuid,
    /// Name.
    pub name: String,
    /// Type.
    pub entity_type: EntityType,
    /// Description.
    pub description: Option<String>,
    /// Properties.
    pub properties: HashMap<String, PropertyValue>,
    /// Aliases.
    pub aliases: Vec<String>,
    /// Source.
    pub source: Option<String>,
    /// Confidence score.
    pub confidence: f64,
    /// Created at.
    pub created_at: DateTime<Utc>,
    /// Updated at.
    pub updated_at: DateTime<Utc>,
}

impl Entity {
    /// Create a new entity.
    pub fn new(name: &str, entity_type: EntityType) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.to_string(),
            entity_type,
            description: None,
            properties: HashMap::new(),
            aliases: Vec::new(),
            source: None,
            confidence: 1.0,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    /// Add property.
    pub fn with_property(mut self, key: &str, value: PropertyValue) -> Self {
        self.properties.insert(key.to_string(), value);
        self
    }

    /// Add alias.
    pub fn with_alias(mut self, alias: &str) -> Self {
        self.aliases.push(alias.to_string());
        self
    }
}

/// Property value types.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PropertyValue {
    String(String),
    Integer(i64),
    Float(f64),
    Boolean(bool),
    List(Vec<PropertyValue>),
}

/// Relation type.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationType {
    IsA,
    PartOf,
    HasPart,
    RelatedTo,
    LocatedIn,
    WorksFor,
    Knows,
    CreatedBy,
    OccurredAt,
    Before,
    After,
    Causes,
    Custom(String),
}

/// Relation between entities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relation {
    /// Relation ID.
    pub id: Uuid,
    /// Source entity ID.
    pub source: Uuid,
    /// Target entity ID.
    pub target: Uuid,
    /// Relation type.
    pub relation_type: RelationType,
    /// Weight/strength.
    pub weight: f64,
    /// Properties.
    pub properties: HashMap<String, PropertyValue>,
    /// Confidence.
    pub confidence: f64,
    /// Source document.
    pub source_doc: Option<String>,
    /// Created at.
    pub created_at: DateTime<Utc>,
}

impl Relation {
    /// Create a new relation.
    pub fn new(source: Uuid, target: Uuid, relation_type: RelationType) -> Self {
        Self {
            id: Uuid::new_v4(),
            source,
            target,
            relation_type,
            weight: 1.0,
            properties: HashMap::new(),
            confidence: 1.0,
            source_doc: None,
            created_at: Utc::now(),
        }
    }
}

/// Triple (subject, predicate, object).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Triple {
    /// Subject.
    pub subject: String,
    /// Predicate.
    pub predicate: String,
    /// Object.
    pub object: String,
    /// Confidence.
    pub confidence: f64,
}

/// Graph query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphQuery {
    /// Query type.
    pub query_type: QueryType,
    /// Start entity (optional).
    pub start: Option<Uuid>,
    /// Entity type filter.
    pub entity_type: Option<EntityType>,
    /// Relation type filter.
    pub relation_type: Option<RelationType>,
    /// Max depth.
    pub max_depth: usize,
    /// Limit.
    pub limit: usize,
}

/// Query types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryType {
    /// Get neighbors.
    Neighbors,
    /// Find path between entities.
    Path,
    /// Find all connected.
    Connected,
    /// Search by name.
    Search,
    /// Get subgraph.
    Subgraph,
}

/// Query result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResult {
    /// Entities.
    pub entities: Vec<Entity>,
    /// Relations.
    pub relations: Vec<Relation>,
    /// Paths (for path queries).
    pub paths: Vec<Vec<Uuid>>,
}

/// Inference rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceRule {
    /// Rule ID.
    pub id: Uuid,
    /// Name.
    pub name: String,
    /// Premise relations.
    pub premises: Vec<RelationType>,
    /// Conclusion relation.
    pub conclusion: RelationType,
    /// Confidence modifier.
    pub confidence_modifier: f64,
}

impl InferenceRule {
    /// Transitive rule (A->B, B->C => A->C).
    pub fn transitive(relation: RelationType) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: format!("Transitive {:?}", relation),
            premises: vec![relation.clone(), relation.clone()],
            conclusion: relation,
            confidence_modifier: 0.9,
        }
    }
}

/// Knowledge graph configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KGraphConfig {
    /// Enable inference.
    pub enable_inference: bool,
    /// Max inference depth.
    pub max_inference_depth: usize,
    /// Minimum confidence threshold.
    pub min_confidence: f64,
}

impl Default for KGraphConfig {
    fn default() -> Self {
        Self {
            enable_inference: true,
            max_inference_depth: 3,
            min_confidence: 0.5,
        }
    }
}

/// Trait for knowledge extractors.
#[async_trait]
pub trait KnowledgeExtractor: Send + Sync {
    /// Extract entities from text.
    async fn extract_entities(&self, text: &str) -> Vec<Entity>;
    /// Extract relations from text.
    async fn extract_relations(&self, text: &str, entities: &[Entity]) -> Vec<Triple>;
}

/// Knowledge graph engine.
pub struct KnowledgeGraph<E: KnowledgeExtractor> {
    config: KGraphConfig,
    extractor: E,
    entities: Arc<RwLock<HashMap<Uuid, Entity>>>,
    relations: Arc<RwLock<HashMap<Uuid, Relation>>>,
    inference_rules: Arc<RwLock<Vec<InferenceRule>>>,
    /// Index: entity name -> entity IDs.
    name_index: Arc<RwLock<HashMap<String, HashSet<Uuid>>>>,
    /// Index: entity type -> entity IDs.
    type_index: Arc<RwLock<HashMap<EntityType, HashSet<Uuid>>>>,
}

impl<E: KnowledgeExtractor> KnowledgeGraph<E> {
    /// Create a new knowledge graph.
    pub fn new(config: KGraphConfig, extractor: E) -> Self {
        Self {
            config,
            extractor,
            entities: Arc::new(RwLock::new(HashMap::new())),
            relations: Arc::new(RwLock::new(HashMap::new())),
            inference_rules: Arc::new(RwLock::new(Vec::new())),
            name_index: Arc::new(RwLock::new(HashMap::new())),
            type_index: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Add entity.
    pub async fn add_entity(&self, entity: Entity) -> Uuid {
        let id = entity.id;

        // Update indices
        {
            let mut name_idx = self.name_index.write().await;
            name_idx
                .entry(entity.name.to_lowercase())
                .or_default()
                .insert(id);
            for alias in &entity.aliases {
                name_idx.entry(alias.to_lowercase()).or_default().insert(id);
            }
        }
        {
            let mut type_idx = self.type_index.write().await;
            type_idx
                .entry(entity.entity_type.clone())
                .or_default()
                .insert(id);
        }

        self.entities.write().await.insert(id, entity);
        id
    }

    /// Get entity.
    pub async fn get_entity(&self, id: Uuid) -> Option<Entity> {
        self.entities.read().await.get(&id).cloned()
    }

    /// Find entities by name.
    pub async fn find_by_name(&self, name: &str) -> Vec<Entity> {
        let name_lower = name.to_lowercase();
        let name_idx = self.name_index.read().await;

        if let Some(ids) = name_idx.get(&name_lower) {
            let entities = self.entities.read().await;
            ids.iter()
                .filter_map(|id| entities.get(id).cloned())
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Find entities by type.
    pub async fn find_by_type(&self, entity_type: &EntityType) -> Vec<Entity> {
        let type_idx = self.type_index.read().await;

        if let Some(ids) = type_idx.get(entity_type) {
            let entities = self.entities.read().await;
            ids.iter()
                .filter_map(|id| entities.get(id).cloned())
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Add relation.
    pub async fn add_relation(&self, relation: Relation) -> Result<Uuid> {
        // Verify entities exist
        let entities = self.entities.read().await;
        if !entities.contains_key(&relation.source) {
            return Err(KGraphError::EntityNotFound(relation.source.to_string()));
        }
        if !entities.contains_key(&relation.target) {
            return Err(KGraphError::EntityNotFound(relation.target.to_string()));
        }
        drop(entities);

        let id = relation.id;
        self.relations.write().await.insert(id, relation);

        // Run inference if enabled
        if self.config.enable_inference {
            self.run_inference().await;
        }

        Ok(id)
    }

    /// Get relations for entity.
    pub async fn get_relations(&self, entity_id: Uuid) -> Vec<Relation> {
        self.relations
            .read()
            .await
            .values()
            .filter(|r| r.source == entity_id || r.target == entity_id)
            .cloned()
            .collect()
    }

    /// Get outgoing relations.
    pub async fn get_outgoing(&self, entity_id: Uuid) -> Vec<Relation> {
        self.relations
            .read()
            .await
            .values()
            .filter(|r| r.source == entity_id)
            .cloned()
            .collect()
    }

    /// Get incoming relations.
    pub async fn get_incoming(&self, entity_id: Uuid) -> Vec<Relation> {
        self.relations
            .read()
            .await
            .values()
            .filter(|r| r.target == entity_id)
            .cloned()
            .collect()
    }

    /// Add inference rule.
    pub async fn add_rule(&self, rule: InferenceRule) {
        self.inference_rules.write().await.push(rule);
    }

    /// Run inference.
    async fn run_inference(&self) {
        let rules = self.inference_rules.read().await.clone();
        let relations = self.relations.read().await.clone();

        let mut new_relations = Vec::new();

        for rule in &rules {
            // Simple transitive inference
            if rule.premises.len() == 2 && rule.premises[0] == rule.premises[1] {
                for r1 in relations.values() {
                    if r1.relation_type == rule.premises[0] {
                        for r2 in relations.values() {
                            if r2.relation_type == rule.premises[1] && r1.target == r2.source {
                                // Check if relation already exists
                                let exists = relations.values().any(|r| {
                                    r.source == r1.source
                                        && r.target == r2.target
                                        && r.relation_type == rule.conclusion
                                });

                                if !exists {
                                    let mut new_rel = Relation::new(
                                        r1.source,
                                        r2.target,
                                        rule.conclusion.clone(),
                                    );
                                    new_rel.confidence =
                                        r1.confidence * r2.confidence * rule.confidence_modifier;

                                    if new_rel.confidence >= self.config.min_confidence {
                                        new_relations.push(new_rel);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Add inferred relations
        let mut rels = self.relations.write().await;
        for rel in new_relations {
            rels.insert(rel.id, rel);
        }
    }

    /// Find path between entities.
    pub async fn find_path(&self, start: Uuid, end: Uuid, max_depth: usize) -> Option<Vec<Uuid>> {
        let relations = self.relations.read().await;

        // BFS
        let mut queue = VecDeque::new();
        let mut visited = HashSet::new();
        let mut parent: HashMap<Uuid, Uuid> = HashMap::new();

        queue.push_back((start, 0));
        visited.insert(start);

        while let Some((current, depth)) = queue.pop_front() {
            if current == end {
                // Reconstruct path
                let mut path = vec![end];
                let mut node = end;
                while let Some(&p) = parent.get(&node) {
                    path.push(p);
                    node = p;
                }
                path.reverse();
                return Some(path);
            }

            if depth >= max_depth {
                continue;
            }

            // Get neighbors
            for rel in relations.values() {
                let neighbor = if rel.source == current {
                    Some(rel.target)
                } else if rel.target == current {
                    Some(rel.source)
                } else {
                    None
                };

                if let Some(n) = neighbor {
                    if !visited.contains(&n) {
                        visited.insert(n);
                        parent.insert(n, current);
                        queue.push_back((n, depth + 1));
                    }
                }
            }
        }

        None
    }

    /// Execute query.
    pub async fn query(&self, query: GraphQuery) -> Result<QueryResult> {
        match query.query_type {
            QueryType::Neighbors => self.query_neighbors(query).await,
            QueryType::Path => self.query_path(query).await,
            QueryType::Connected => self.query_connected(query).await,
            QueryType::Search => self.query_search(query).await,
            QueryType::Subgraph => self.query_subgraph(query).await,
        }
    }

    async fn query_neighbors(&self, query: GraphQuery) -> Result<QueryResult> {
        let start = query.start.ok_or(KGraphError::InvalidQuery(
            "Start entity required".to_string(),
        ))?;

        let relations: Vec<_> = self
            .get_relations(start)
            .await
            .into_iter()
            .filter(|r| {
                query
                    .relation_type
                    .as_ref()
                    .map(|t| &r.relation_type == t)
                    .unwrap_or(true)
            })
            .take(query.limit)
            .collect();

        let entity_ids: HashSet<_> = relations
            .iter()
            .flat_map(|r| vec![r.source, r.target])
            .collect();

        let entities_map = self.entities.read().await;
        let entities: Vec<_> = entity_ids
            .iter()
            .filter_map(|id| entities_map.get(id).cloned())
            .filter(|e| {
                query
                    .entity_type
                    .as_ref()
                    .map(|t| &e.entity_type == t)
                    .unwrap_or(true)
            })
            .collect();

        Ok(QueryResult {
            entities,
            relations,
            paths: Vec::new(),
        })
    }

    async fn query_path(&self, query: GraphQuery) -> Result<QueryResult> {
        // For simplicity, we expect start in query.start and implement a simple version
        let start = query.start.ok_or(KGraphError::InvalidQuery(
            "Start entity required".to_string(),
        ))?;

        // This would need a target, but simplified for now
        Ok(QueryResult {
            entities: Vec::new(),
            relations: Vec::new(),
            paths: Vec::new(),
        })
    }

    async fn query_connected(&self, query: GraphQuery) -> Result<QueryResult> {
        let start = query.start.ok_or(KGraphError::InvalidQuery(
            "Start entity required".to_string(),
        ))?;

        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back((start, 0));
        visited.insert(start);

        let relations = self.relations.read().await;

        while let Some((current, depth)) = queue.pop_front() {
            if depth >= query.max_depth {
                continue;
            }

            for rel in relations.values() {
                let neighbor = if rel.source == current {
                    Some(rel.target)
                } else if rel.target == current {
                    Some(rel.source)
                } else {
                    None
                };

                if let Some(n) = neighbor {
                    if !visited.contains(&n) && visited.len() < query.limit {
                        visited.insert(n);
                        queue.push_back((n, depth + 1));
                    }
                }
            }
        }

        let entities_map = self.entities.read().await;
        let entities: Vec<_> = visited
            .iter()
            .filter_map(|id| entities_map.get(id).cloned())
            .collect();

        Ok(QueryResult {
            entities,
            relations: Vec::new(),
            paths: Vec::new(),
        })
    }

    async fn query_search(&self, _query: GraphQuery) -> Result<QueryResult> {
        // Would search by text
        Ok(QueryResult {
            entities: Vec::new(),
            relations: Vec::new(),
            paths: Vec::new(),
        })
    }

    async fn query_subgraph(&self, query: GraphQuery) -> Result<QueryResult> {
        // Get connected entities and their relations
        let connected = self.query_connected(query.clone()).await?;
        let entity_ids: HashSet<_> = connected.entities.iter().map(|e| e.id).collect();

        let all_relations = self.relations.read().await;
        let relations: Vec<_> = all_relations
            .values()
            .filter(|r| entity_ids.contains(&r.source) && entity_ids.contains(&r.target))
            .cloned()
            .collect();

        Ok(QueryResult {
            entities: connected.entities,
            relations,
            paths: Vec::new(),
        })
    }

    /// Extract and add knowledge from text.
    pub async fn learn_from_text(&self, text: &str) -> (Vec<Uuid>, Vec<Uuid>) {
        let entities = self.extractor.extract_entities(text).await;
        let triples = self.extractor.extract_relations(text, &entities).await;

        let mut entity_ids = Vec::new();
        let mut relation_ids = Vec::new();

        // Add entities
        for entity in entities {
            let id = self.add_entity(entity).await;
            entity_ids.push(id);
        }

        // Add relations
        for triple in triples {
            // Find source and target entities
            let sources = self.find_by_name(&triple.subject).await;
            let targets = self.find_by_name(&triple.object).await;

            if let (Some(source), Some(target)) = (sources.first(), targets.first()) {
                let rel_type = RelationType::Custom(triple.predicate);
                let mut relation = Relation::new(source.id, target.id, rel_type);
                relation.confidence = triple.confidence;

                if let Ok(id) = self.add_relation(relation).await {
                    relation_ids.push(id);
                }
            }
        }

        (entity_ids, relation_ids)
    }

    /// Get statistics.
    pub async fn stats(&self) -> KGraphStats {
        let entities = self.entities.read().await;
        let relations = self.relations.read().await;

        let mut by_type: HashMap<EntityType, usize> = HashMap::new();
        for entity in entities.values() {
            *by_type.entry(entity.entity_type.clone()).or_insert(0) += 1;
        }

        let mut by_relation: HashMap<RelationType, usize> = HashMap::new();
        for relation in relations.values() {
            *by_relation
                .entry(relation.relation_type.clone())
                .or_insert(0) += 1;
        }

        KGraphStats {
            total_entities: entities.len(),
            total_relations: relations.len(),
            by_entity_type: by_type,
            by_relation_type: by_relation,
        }
    }
}

/// Knowledge graph statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KGraphStats {
    pub total_entities: usize,
    pub total_relations: usize,
    pub by_entity_type: HashMap<EntityType, usize>,
    pub by_relation_type: HashMap<RelationType, usize>,
}

/// Simple extractor for testing.
pub struct SimpleExtractor;

#[async_trait]
impl KnowledgeExtractor for SimpleExtractor {
    async fn extract_entities(&self, text: &str) -> Vec<Entity> {
        // Very simple: capitalize words might be entities
        text.split_whitespace()
            .filter(|w| w.chars().next().map(|c| c.is_uppercase()).unwrap_or(false))
            .map(|w| Entity::new(w, EntityType::Concept))
            .collect()
    }

    async fn extract_relations(&self, _text: &str, _entities: &[Entity]) -> Vec<Triple> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_add_entity() {
        let kg = KnowledgeGraph::new(KGraphConfig::default(), SimpleExtractor);

        let entity = Entity::new("Alice", EntityType::Person);
        let id = kg.add_entity(entity.clone()).await;

        let retrieved = kg.get_entity(id).await.unwrap();
        assert_eq!(retrieved.name, "Alice");
    }

    #[tokio::test]
    async fn test_find_by_name() {
        let kg = KnowledgeGraph::new(KGraphConfig::default(), SimpleExtractor);

        kg.add_entity(Entity::new("Alice", EntityType::Person))
            .await;
        kg.add_entity(Entity::new("Bob", EntityType::Person)).await;

        let results = kg.find_by_name("alice").await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Alice");
    }

    #[tokio::test]
    async fn test_find_by_type() {
        let kg = KnowledgeGraph::new(KGraphConfig::default(), SimpleExtractor);

        kg.add_entity(Entity::new("Alice", EntityType::Person))
            .await;
        kg.add_entity(Entity::new("Bob", EntityType::Person)).await;
        kg.add_entity(Entity::new("NYC", EntityType::Place)).await;

        let people = kg.find_by_type(&EntityType::Person).await;
        assert_eq!(people.len(), 2);

        let places = kg.find_by_type(&EntityType::Place).await;
        assert_eq!(places.len(), 1);
    }

    #[tokio::test]
    async fn test_add_relation() {
        let kg = KnowledgeGraph::new(KGraphConfig::default(), SimpleExtractor);

        let alice = kg
            .add_entity(Entity::new("Alice", EntityType::Person))
            .await;
        let bob = kg.add_entity(Entity::new("Bob", EntityType::Person)).await;

        let relation = Relation::new(alice, bob, RelationType::Knows);
        kg.add_relation(relation).await.unwrap();

        let relations = kg.get_relations(alice).await;
        assert_eq!(relations.len(), 1);
    }

    #[tokio::test]
    async fn test_find_path() {
        let kg = KnowledgeGraph::new(KGraphConfig::default(), SimpleExtractor);

        let a = kg.add_entity(Entity::new("A", EntityType::Concept)).await;
        let b = kg.add_entity(Entity::new("B", EntityType::Concept)).await;
        let c = kg.add_entity(Entity::new("C", EntityType::Concept)).await;

        kg.add_relation(Relation::new(a, b, RelationType::RelatedTo))
            .await
            .unwrap();
        kg.add_relation(Relation::new(b, c, RelationType::RelatedTo))
            .await
            .unwrap();

        let path = kg.find_path(a, c, 5).await;
        assert!(path.is_some());
        assert_eq!(path.unwrap().len(), 3);
    }

    #[tokio::test]
    async fn test_query_neighbors() {
        let kg = KnowledgeGraph::new(KGraphConfig::default(), SimpleExtractor);

        let center = kg
            .add_entity(Entity::new("Center", EntityType::Concept))
            .await;
        let n1 = kg.add_entity(Entity::new("N1", EntityType::Concept)).await;
        let n2 = kg.add_entity(Entity::new("N2", EntityType::Concept)).await;

        kg.add_relation(Relation::new(center, n1, RelationType::RelatedTo))
            .await
            .unwrap();
        kg.add_relation(Relation::new(center, n2, RelationType::RelatedTo))
            .await
            .unwrap();

        let query = GraphQuery {
            query_type: QueryType::Neighbors,
            start: Some(center),
            entity_type: None,
            relation_type: None,
            max_depth: 1,
            limit: 10,
        };

        let result = kg.query(query).await.unwrap();
        assert_eq!(result.entities.len(), 3);
        assert_eq!(result.relations.len(), 2);
    }

    #[tokio::test]
    async fn test_inference() {
        let mut config = KGraphConfig::default();
        config.enable_inference = true;

        let kg = KnowledgeGraph::new(config, SimpleExtractor);
        kg.add_rule(InferenceRule::transitive(RelationType::PartOf))
            .await;

        let a = kg.add_entity(Entity::new("A", EntityType::Concept)).await;
        let b = kg.add_entity(Entity::new("B", EntityType::Concept)).await;
        let c = kg.add_entity(Entity::new("C", EntityType::Concept)).await;

        kg.add_relation(Relation::new(a, b, RelationType::PartOf))
            .await
            .unwrap();
        kg.add_relation(Relation::new(b, c, RelationType::PartOf))
            .await
            .unwrap();

        // Should infer A -> C
        let relations = kg.get_outgoing(a).await;
        assert_eq!(relations.len(), 2);
    }

    #[tokio::test]
    async fn test_stats() {
        let kg = KnowledgeGraph::new(KGraphConfig::default(), SimpleExtractor);

        kg.add_entity(Entity::new("Alice", EntityType::Person))
            .await;
        kg.add_entity(Entity::new("Bob", EntityType::Person)).await;
        kg.add_entity(Entity::new("NYC", EntityType::Place)).await;

        let stats = kg.stats().await;
        assert_eq!(stats.total_entities, 3);
        assert_eq!(*stats.by_entity_type.get(&EntityType::Person).unwrap(), 2);
    }
}
