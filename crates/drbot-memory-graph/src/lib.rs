//! Semantic knowledge graph with temporal reasoning.
//!
//! This crate provides a graph-based memory system that:
//! - Stores knowledge as interconnected nodes and relationships
//! - Supports temporal reasoning (facts that change over time)
//! - Enables semantic traversal and inference
//! - Provides relevance-based retrieval

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Memory graph errors.
#[derive(Debug, Error)]
pub enum MemoryGraphError {
    #[error("Node not found: {0}")]
    NodeNotFound(String),

    #[error("Edge not found: {0}")]
    EdgeNotFound(String),

    #[error("Invalid relationship: {0}")]
    InvalidRelationship(String),

    #[error("Cycle detected in graph")]
    CycleDetected,

    #[error("Query failed: {0}")]
    QueryFailed(String),

    #[error("Provider error: {0}")]
    ProviderError(String),
}

/// Result type for memory graph operations.
pub type Result<T> = std::result::Result<T, MemoryGraphError>;

/// A node in the knowledge graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    /// Unique node identifier.
    pub id: String,
    /// Node type/category.
    pub node_type: NodeType,
    /// Node label (human readable).
    pub label: String,
    /// Node properties.
    pub properties: HashMap<String, serde_json::Value>,
    /// Embedding vector for semantic search.
    pub embedding: Option<Vec<f32>>,
    /// Temporal validity.
    pub temporal: TemporalValidity,
    /// Node importance/weight.
    pub importance: f64,
    /// Access count for relevance.
    pub access_count: u32,
    /// Last accessed timestamp.
    pub last_accessed: DateTime<Utc>,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
}

/// Types of nodes in the graph.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum NodeType {
    /// A concept or abstract idea.
    Concept,
    /// A specific entity (person, place, thing).
    Entity,
    /// An event that occurred.
    Event,
    /// A fact or piece of knowledge.
    Fact,
    /// A user preference.
    Preference,
    /// A conversation/interaction.
    Conversation,
    /// A task or action.
    Task,
    /// A document or content source.
    Document,
    /// Custom node type.
    Custom(String),
}

/// Temporal validity of a node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalValidity {
    /// When this fact became valid.
    pub valid_from: Option<DateTime<Utc>>,
    /// When this fact stopped being valid.
    pub valid_until: Option<DateTime<Utc>>,
    /// Whether this is currently valid.
    pub is_current: bool,
    /// Confidence that this is still valid.
    pub freshness: f64,
}

impl Default for TemporalValidity {
    fn default() -> Self {
        Self {
            valid_from: Some(Utc::now()),
            valid_until: None,
            is_current: true,
            freshness: 1.0,
        }
    }
}

/// An edge (relationship) in the graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    /// Unique edge identifier.
    pub id: String,
    /// Source node ID.
    pub from: String,
    /// Target node ID.
    pub to: String,
    /// Relationship type.
    pub relationship: RelationshipType,
    /// Edge weight/strength.
    pub weight: f64,
    /// Edge properties.
    pub properties: HashMap<String, serde_json::Value>,
    /// Temporal validity.
    pub temporal: TemporalValidity,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
}

/// Types of relationships between nodes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum RelationshipType {
    /// Is a type/instance of.
    IsA,
    /// Part of a whole.
    PartOf,
    /// Related to.
    RelatedTo,
    /// Caused by.
    CausedBy,
    /// Causes (inverse of CausedBy).
    Causes,
    /// Preceded by.
    PrecededBy,
    /// Follows (inverse of PrecededBy).
    Follows,
    /// Created by.
    CreatedBy,
    /// Mentioned in.
    MentionedIn,
    /// Contradicts.
    Contradicts,
    /// Supports.
    Supports,
    /// Same as (identity).
    SameAs,
    /// Custom relationship.
    Custom(String),
}

/// Query for traversing the graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphQuery {
    /// Starting nodes (by ID or semantic search).
    pub start: QueryStart,
    /// Relationship filters.
    pub relationships: Vec<RelationshipType>,
    /// Maximum traversal depth.
    pub max_depth: u32,
    /// Node type filter.
    pub node_types: Option<Vec<NodeType>>,
    /// Minimum importance threshold.
    pub min_importance: Option<f64>,
    /// Only include temporally valid nodes.
    pub temporal_filter: bool,
    /// Maximum results.
    pub limit: u32,
}

/// How to start a graph traversal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum QueryStart {
    /// Start from specific node IDs.
    NodeIds(Vec<String>),
    /// Start from semantic search.
    Semantic { query: String, top_k: u32 },
    /// Start from node type.
    ByType(NodeType),
    /// Start from all nodes.
    All,
}

/// Result of a graph query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResult {
    /// Matched nodes.
    pub nodes: Vec<Node>,
    /// Edges connecting the nodes.
    pub edges: Vec<Edge>,
    /// Paths found (sequences of node IDs).
    pub paths: Vec<Vec<String>>,
    /// Query execution time in ms.
    pub execution_time_ms: u64,
}

/// A temporal query for time-based retrieval.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalQuery {
    /// Point in time to query.
    pub at_time: DateTime<Utc>,
    /// Node to query about.
    pub node_id: Option<String>,
    /// Semantic query.
    pub semantic: Option<String>,
    /// Include historical values.
    pub include_history: bool,
}

/// Result of a temporal query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalResult {
    /// Current value at the queried time.
    pub current: Option<Node>,
    /// Historical values.
    pub history: Vec<TemporalSnapshot>,
    /// Changes over time.
    pub changes: Vec<TemporalChange>,
}

/// A snapshot of a node at a point in time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalSnapshot {
    /// The node state.
    pub node: Node,
    /// Timestamp of this snapshot.
    pub timestamp: DateTime<Utc>,
}

/// A change in node value over time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalChange {
    /// Property that changed.
    pub property: String,
    /// Old value.
    pub old_value: serde_json::Value,
    /// New value.
    pub new_value: serde_json::Value,
    /// When the change occurred.
    pub changed_at: DateTime<Utc>,
    /// Reason for the change.
    pub reason: Option<String>,
}

/// Provider for embedding and semantic operations.
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// Generate embedding for text.
    async fn embed(&self, text: &str) -> Result<Vec<f32>>;

    /// Calculate similarity between embeddings.
    fn similarity(&self, a: &[f32], b: &[f32]) -> f64;
}

/// The memory graph storage and query engine.
pub struct MemoryGraph {
    /// All nodes.
    nodes: Arc<RwLock<HashMap<String, Node>>>,
    /// All edges.
    edges: Arc<RwLock<HashMap<String, Edge>>>,
    /// Index: node ID -> outgoing edge IDs.
    outgoing: Arc<RwLock<HashMap<String, Vec<String>>>>,
    /// Index: node ID -> incoming edge IDs.
    incoming: Arc<RwLock<HashMap<String, Vec<String>>>>,
    /// Temporal snapshots.
    snapshots: Arc<RwLock<HashMap<String, Vec<TemporalSnapshot>>>>,
    /// Embedding provider.
    embedding_provider: Option<Arc<dyn EmbeddingProvider>>,
}

impl MemoryGraph {
    /// Create a new memory graph.
    pub fn new() -> Self {
        Self {
            nodes: Arc::new(RwLock::new(HashMap::new())),
            edges: Arc::new(RwLock::new(HashMap::new())),
            outgoing: Arc::new(RwLock::new(HashMap::new())),
            incoming: Arc::new(RwLock::new(HashMap::new())),
            snapshots: Arc::new(RwLock::new(HashMap::new())),
            embedding_provider: None,
        }
    }

    /// Create with an embedding provider.
    pub fn with_embeddings(provider: Arc<dyn EmbeddingProvider>) -> Self {
        Self {
            embedding_provider: Some(provider),
            ..Self::new()
        }
    }

    /// Add a node to the graph.
    pub async fn add_node(&self, mut node: Node) -> Result<String> {
        // Generate embedding if provider available
        if let Some(provider) = &self.embedding_provider {
            if node.embedding.is_none() {
                let text = format!(
                    "{} {}",
                    node.label,
                    node.properties
                        .get("description")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                );
                node.embedding = Some(provider.embed(&text).await?);
            }
        }

        let id = node.id.clone();

        let mut nodes = self.nodes.write().await;
        nodes.insert(id.clone(), node.clone());

        // Initialize edge indexes
        let mut outgoing = self.outgoing.write().await;
        outgoing.entry(id.clone()).or_insert_with(Vec::new);

        let mut incoming = self.incoming.write().await;
        incoming.entry(id.clone()).or_insert_with(Vec::new);

        // Create initial snapshot
        let mut snapshots = self.snapshots.write().await;
        snapshots
            .entry(id.clone())
            .or_insert_with(Vec::new)
            .push(TemporalSnapshot {
                node,
                timestamp: Utc::now(),
            });

        Ok(id)
    }

    /// Add an edge to the graph.
    pub async fn add_edge(&self, edge: Edge) -> Result<String> {
        // Verify both nodes exist
        let nodes = self.nodes.read().await;
        if !nodes.contains_key(&edge.from) {
            return Err(MemoryGraphError::NodeNotFound(edge.from.clone()));
        }
        if !nodes.contains_key(&edge.to) {
            return Err(MemoryGraphError::NodeNotFound(edge.to.clone()));
        }
        drop(nodes);

        let id = edge.id.clone();

        // Add to edge storage
        let mut edges = self.edges.write().await;
        edges.insert(id.clone(), edge.clone());

        // Update indexes
        let mut outgoing = self.outgoing.write().await;
        outgoing
            .entry(edge.from.clone())
            .or_insert_with(Vec::new)
            .push(id.clone());

        let mut incoming = self.incoming.write().await;
        incoming
            .entry(edge.to.clone())
            .or_insert_with(Vec::new)
            .push(id.clone());

        Ok(id)
    }

    /// Get a node by ID.
    pub async fn get_node(&self, id: &str) -> Option<Node> {
        let mut nodes = self.nodes.write().await;
        if let Some(node) = nodes.get_mut(id) {
            node.access_count += 1;
            node.last_accessed = Utc::now();
            Some(node.clone())
        } else {
            None
        }
    }

    /// Query the graph.
    pub async fn query(&self, query: &GraphQuery) -> Result<QueryResult> {
        let start_time = std::time::Instant::now();

        // Get starting nodes
        let start_nodes = self.resolve_query_start(&query.start).await?;

        // Traverse graph
        let (nodes, edges, paths) = self
            .traverse(
                &start_nodes,
                &query.relationships,
                query.max_depth,
                &query.node_types,
                query.min_importance,
                query.temporal_filter,
                query.limit,
            )
            .await?;

        Ok(QueryResult {
            nodes,
            edges,
            paths,
            execution_time_ms: start_time.elapsed().as_millis() as u64,
        })
    }

    /// Resolve query start to node IDs.
    async fn resolve_query_start(&self, start: &QueryStart) -> Result<Vec<String>> {
        match start {
            QueryStart::NodeIds(ids) => Ok(ids.clone()),
            QueryStart::Semantic { query, top_k } => self.semantic_search(query, *top_k).await,
            QueryStart::ByType(node_type) => {
                let nodes = self.nodes.read().await;
                Ok(nodes
                    .values()
                    .filter(|n| &n.node_type == node_type)
                    .map(|n| n.id.clone())
                    .collect())
            }
            QueryStart::All => {
                let nodes = self.nodes.read().await;
                Ok(nodes.keys().cloned().collect())
            }
        }
    }

    /// Perform semantic search.
    async fn semantic_search(&self, query: &str, top_k: u32) -> Result<Vec<String>> {
        let provider = self
            .embedding_provider
            .as_ref()
            .ok_or_else(|| MemoryGraphError::QueryFailed("No embedding provider".to_string()))?;

        let query_embedding = provider.embed(query).await?;
        let nodes = self.nodes.read().await;

        let mut scored: Vec<_> = nodes
            .values()
            .filter_map(|n| {
                n.embedding.as_ref().map(|e| {
                    let score = provider.similarity(&query_embedding, e);
                    (n.id.clone(), score)
                })
            })
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(top_k as usize);

        Ok(scored.into_iter().map(|(id, _)| id).collect())
    }

    /// Traverse the graph from starting nodes.
    async fn traverse(
        &self,
        start_ids: &[String],
        relationships: &[RelationshipType],
        max_depth: u32,
        node_types: &Option<Vec<NodeType>>,
        min_importance: Option<f64>,
        temporal_filter: bool,
        limit: u32,
    ) -> Result<(Vec<Node>, Vec<Edge>, Vec<Vec<String>>)> {
        let nodes = self.nodes.read().await;
        let edges = self.edges.read().await;
        let outgoing = self.outgoing.read().await;

        let mut visited: HashSet<String> = HashSet::new();
        let mut result_nodes: Vec<Node> = Vec::new();
        let mut result_edges: Vec<Edge> = Vec::new();
        let mut paths: Vec<Vec<String>> = Vec::new();

        let mut queue: VecDeque<(String, Vec<String>, u32)> = VecDeque::new();

        for id in start_ids {
            queue.push_back((id.clone(), vec![id.clone()], 0));
        }

        while let Some((node_id, path, depth)) = queue.pop_front() {
            if visited.contains(&node_id) || depth > max_depth {
                continue;
            }
            if result_nodes.len() >= limit as usize {
                break;
            }

            visited.insert(node_id.clone());

            if let Some(node) = nodes.get(&node_id) {
                // Apply filters
                if let Some(types) = node_types {
                    if !types.contains(&node.node_type) {
                        continue;
                    }
                }
                if let Some(min) = min_importance {
                    if node.importance < min {
                        continue;
                    }
                }
                if temporal_filter && !node.temporal.is_current {
                    continue;
                }

                result_nodes.push(node.clone());
                paths.push(path.clone());

                // Traverse outgoing edges
                if let Some(edge_ids) = outgoing.get(&node_id) {
                    for edge_id in edge_ids {
                        if let Some(edge) = edges.get(edge_id) {
                            if relationships.is_empty()
                                || relationships.contains(&edge.relationship)
                            {
                                result_edges.push(edge.clone());

                                let mut new_path = path.clone();
                                new_path.push(edge.to.clone());
                                queue.push_back((edge.to.clone(), new_path, depth + 1));
                            }
                        }
                    }
                }
            }
        }

        Ok((result_nodes, result_edges, paths))
    }

    /// Query at a specific point in time.
    pub async fn temporal_query(&self, query: &TemporalQuery) -> Result<TemporalResult> {
        let snapshots = self.snapshots.read().await;

        if let Some(node_id) = &query.node_id {
            let history = snapshots.get(node_id).cloned().unwrap_or_default();

            // Find the snapshot valid at the queried time
            let current = history
                .iter()
                .filter(|s| s.timestamp <= query.at_time)
                .max_by_key(|s| s.timestamp)
                .map(|s| s.node.clone());

            // Calculate changes
            let changes = self.calculate_changes(&history);

            Ok(TemporalResult {
                current,
                history: if query.include_history {
                    history
                } else {
                    Vec::new()
                },
                changes,
            })
        } else {
            Ok(TemporalResult {
                current: None,
                history: Vec::new(),
                changes: Vec::new(),
            })
        }
    }

    /// Calculate changes between snapshots.
    fn calculate_changes(&self, history: &[TemporalSnapshot]) -> Vec<TemporalChange> {
        let mut changes = Vec::new();

        for window in history.windows(2) {
            let old = &window[0];
            let new = &window[1];

            for (key, new_val) in &new.node.properties {
                if let Some(old_val) = old.node.properties.get(key) {
                    if old_val != new_val {
                        changes.push(TemporalChange {
                            property: key.clone(),
                            old_value: old_val.clone(),
                            new_value: new_val.clone(),
                            changed_at: new.timestamp,
                            reason: None,
                        });
                    }
                }
            }
        }

        changes
    }

    /// Update a node and create a new snapshot.
    pub async fn update_node(
        &self,
        id: &str,
        properties: HashMap<String, serde_json::Value>,
    ) -> Result<()> {
        let mut nodes = self.nodes.write().await;

        let node = nodes
            .get_mut(id)
            .ok_or_else(|| MemoryGraphError::NodeNotFound(id.to_string()))?;

        for (key, value) in properties {
            node.properties.insert(key, value);
        }

        let updated_node = node.clone();
        drop(nodes);

        // Create new snapshot
        let mut snapshots = self.snapshots.write().await;
        snapshots
            .entry(id.to_string())
            .or_insert_with(Vec::new)
            .push(TemporalSnapshot {
                node: updated_node,
                timestamp: Utc::now(),
            });

        Ok(())
    }

    /// Decay importance of unused nodes over time.
    pub async fn decay_importance(&self, decay_factor: f64) {
        let mut nodes = self.nodes.write().await;
        let now = Utc::now();

        for node in nodes.values_mut() {
            let age_hours = (now - node.last_accessed).num_hours() as f64;
            // Apply decay based on age, with minimum decay of decay_factor
            let decay = (-decay_factor * (age_hours + 1.0)).exp();
            node.importance *= decay;
            node.temporal.freshness *= decay;
        }
    }

    /// Get statistics about the graph.
    pub async fn stats(&self) -> GraphStats {
        let nodes = self.nodes.read().await;
        let edges = self.edges.read().await;

        let node_type_counts: HashMap<String, usize> =
            nodes.values().fold(HashMap::new(), |mut acc, n| {
                *acc.entry(format!("{:?}", n.node_type)).or_insert(0) += 1;
                acc
            });

        GraphStats {
            node_count: nodes.len(),
            edge_count: edges.len(),
            node_type_counts,
            avg_importance: nodes.values().map(|n| n.importance).sum::<f64>()
                / nodes.len().max(1) as f64,
        }
    }
}

impl Default for MemoryGraph {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics about the graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphStats {
    /// Total number of nodes.
    pub node_count: usize,
    /// Total number of edges.
    pub edge_count: usize,
    /// Counts by node type.
    pub node_type_counts: HashMap<String, usize>,
    /// Average importance.
    pub avg_importance: f64,
}

/// Builder for creating nodes easily.
pub struct NodeBuilder {
    node: Node,
}

impl NodeBuilder {
    /// Create a new node builder.
    pub fn new(label: &str, node_type: NodeType) -> Self {
        Self {
            node: Node {
                id: Uuid::new_v4().to_string(),
                node_type,
                label: label.to_string(),
                properties: HashMap::new(),
                embedding: None,
                temporal: TemporalValidity::default(),
                importance: 0.5,
                access_count: 0,
                last_accessed: Utc::now(),
                created_at: Utc::now(),
            },
        }
    }

    /// Set a property.
    pub fn property(mut self, key: &str, value: serde_json::Value) -> Self {
        self.node.properties.insert(key.to_string(), value);
        self
    }

    /// Set importance.
    pub fn importance(mut self, importance: f64) -> Self {
        self.node.importance = importance;
        self
    }

    /// Build the node.
    pub fn build(self) -> Node {
        self.node
    }
}

/// Builder for creating edges easily.
pub struct EdgeBuilder {
    edge: Edge,
}

impl EdgeBuilder {
    /// Create a new edge builder.
    pub fn new(from: &str, to: &str, relationship: RelationshipType) -> Self {
        Self {
            edge: Edge {
                id: Uuid::new_v4().to_string(),
                from: from.to_string(),
                to: to.to_string(),
                relationship,
                weight: 1.0,
                properties: HashMap::new(),
                temporal: TemporalValidity::default(),
                created_at: Utc::now(),
            },
        }
    }

    /// Set weight.
    pub fn weight(mut self, weight: f64) -> Self {
        self.edge.weight = weight;
        self
    }

    /// Build the edge.
    pub fn build(self) -> Edge {
        self.edge
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_add_node() {
        let graph = MemoryGraph::new();

        let node = NodeBuilder::new("Rust", NodeType::Concept)
            .property(
                "description",
                serde_json::json!("A systems programming language"),
            )
            .importance(0.8)
            .build();

        let id = graph.add_node(node).await.unwrap();
        let retrieved = graph.get_node(&id).await.unwrap();

        assert_eq!(retrieved.label, "Rust");
        assert_eq!(retrieved.importance, 0.8);
    }

    #[tokio::test]
    async fn test_add_edge() {
        let graph = MemoryGraph::new();

        let node1 = NodeBuilder::new("Rust", NodeType::Concept).build();
        let node2 = NodeBuilder::new("Memory Safety", NodeType::Concept).build();

        let id1 = graph.add_node(node1).await.unwrap();
        let id2 = graph.add_node(node2).await.unwrap();

        let edge = EdgeBuilder::new(&id1, &id2, RelationshipType::RelatedTo)
            .weight(0.9)
            .build();

        graph.add_edge(edge).await.unwrap();

        let query = GraphQuery {
            start: QueryStart::NodeIds(vec![id1.clone()]),
            relationships: vec![RelationshipType::RelatedTo],
            max_depth: 1,
            node_types: None,
            min_importance: None,
            temporal_filter: false,
            limit: 10,
        };

        let result = graph.query(&query).await.unwrap();
        assert_eq!(result.nodes.len(), 2);
        assert_eq!(result.edges.len(), 1);
    }

    #[tokio::test]
    async fn test_temporal_query() {
        let graph = MemoryGraph::new();

        let node = NodeBuilder::new("Version", NodeType::Fact)
            .property("value", serde_json::json!("1.0"))
            .build();

        let id = graph.add_node(node).await.unwrap();

        // Update the node
        let mut props = HashMap::new();
        props.insert("value".to_string(), serde_json::json!("2.0"));
        graph.update_node(&id, props).await.unwrap();

        let query = TemporalQuery {
            at_time: Utc::now(),
            node_id: Some(id),
            semantic: None,
            include_history: true,
        };

        let result = graph.temporal_query(&query).await.unwrap();
        assert!(result.current.is_some());
        assert_eq!(result.history.len(), 2);
    }

    #[tokio::test]
    async fn test_query_by_type() {
        let graph = MemoryGraph::new();

        graph
            .add_node(NodeBuilder::new("Event1", NodeType::Event).build())
            .await
            .unwrap();
        graph
            .add_node(NodeBuilder::new("Event2", NodeType::Event).build())
            .await
            .unwrap();
        graph
            .add_node(NodeBuilder::new("Fact1", NodeType::Fact).build())
            .await
            .unwrap();

        let query = GraphQuery {
            start: QueryStart::ByType(NodeType::Event),
            relationships: vec![],
            max_depth: 0,
            node_types: None,
            min_importance: None,
            temporal_filter: false,
            limit: 10,
        };

        let result = graph.query(&query).await.unwrap();
        assert_eq!(result.nodes.len(), 2);
    }

    #[tokio::test]
    async fn test_importance_decay() {
        let graph = MemoryGraph::new();

        let node = NodeBuilder::new("Test", NodeType::Concept)
            .importance(1.0)
            .build();

        let id = graph.add_node(node).await.unwrap();

        graph.decay_importance(0.01).await;

        let decayed = graph.get_node(&id).await.unwrap();
        assert!(decayed.importance < 1.0);
    }

    #[tokio::test]
    async fn test_graph_stats() {
        let graph = MemoryGraph::new();

        graph
            .add_node(NodeBuilder::new("N1", NodeType::Concept).build())
            .await
            .unwrap();
        graph
            .add_node(NodeBuilder::new("N2", NodeType::Entity).build())
            .await
            .unwrap();

        let stats = graph.stats().await;
        assert_eq!(stats.node_count, 2);
        assert_eq!(stats.edge_count, 0);
    }

    #[test]
    fn test_node_builder() {
        let node = NodeBuilder::new("Test", NodeType::Fact)
            .property("key", serde_json::json!("value"))
            .importance(0.7)
            .build();

        assert_eq!(node.label, "Test");
        assert_eq!(node.importance, 0.7);
        assert!(node.properties.contains_key("key"));
    }

    #[test]
    fn test_edge_builder() {
        let edge = EdgeBuilder::new("a", "b", RelationshipType::Causes)
            .weight(0.5)
            .build();

        assert_eq!(edge.from, "a");
        assert_eq!(edge.to, "b");
        assert_eq!(edge.weight, 0.5);
    }
}
