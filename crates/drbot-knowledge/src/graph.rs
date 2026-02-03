//! Knowledge graph for relationship tracking.

use crate::store::Document;
use crate::{KnowledgeEntry, KnowledgeError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::RwLock;
use uuid::Uuid;

/// A node in the knowledge graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    /// Node ID.
    pub id: Uuid,
    /// Node type (document, concept, entity, etc.).
    pub node_type: NodeType,
    /// Node label/name.
    pub label: String,
    /// Node properties.
    pub properties: HashMap<String, serde_json::Value>,
}

/// Node types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeType {
    Document,
    Chunk,
    Concept,
    Entity,
    Topic,
}

/// An edge in the knowledge graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    /// Edge ID.
    pub id: Uuid,
    /// Source node ID.
    pub source: Uuid,
    /// Target node ID.
    pub target: Uuid,
    /// Relationship type.
    pub relation: Relation,
    /// Edge weight/strength.
    pub weight: f32,
    /// Edge properties.
    pub properties: HashMap<String, serde_json::Value>,
}

/// Relationship types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Relation {
    /// Document contains chunk.
    Contains,
    /// Entity mentioned in document.
    MentionedIn,
    /// Topic discussed in document.
    DiscussesIn,
    /// Related to another concept.
    RelatedTo,
    /// Is a type of.
    IsA,
    /// Part of.
    PartOf,
    /// Causes or leads to.
    CausesTo,
    /// Similar to.
    SimilarTo,
    /// References.
    References,
    /// Custom relation.
    Custom,
}

/// In-memory knowledge graph.
pub struct KnowledgeGraph {
    nodes: RwLock<HashMap<Uuid, Node>>,
    edges: RwLock<Vec<Edge>>,
    /// Index from label to node IDs.
    label_index: RwLock<HashMap<String, Vec<Uuid>>>,
}

impl KnowledgeGraph {
    /// Create a new knowledge graph.
    pub fn new() -> Self {
        Self {
            nodes: RwLock::new(HashMap::new()),
            edges: RwLock::new(Vec::new()),
            label_index: RwLock::new(HashMap::new()),
        }
    }

    /// Add a node to the graph.
    pub fn add_node(&self, node: Node) -> Result<()> {
        let mut nodes = self.nodes.write().unwrap();
        let mut index = self.label_index.write().unwrap();

        // Update label index
        index
            .entry(node.label.to_lowercase())
            .or_default()
            .push(node.id);

        nodes.insert(node.id, node);
        Ok(())
    }

    /// Add an edge to the graph.
    pub fn add_edge(&self, edge: Edge) -> Result<()> {
        let mut edges = self.edges.write().unwrap();
        edges.push(edge);
        Ok(())
    }

    /// Get a node by ID.
    pub fn get_node(&self, id: Uuid) -> Option<Node> {
        let nodes = self.nodes.read().unwrap();
        nodes.get(&id).cloned()
    }

    /// Find nodes by label.
    pub fn find_by_label(&self, label: &str) -> Vec<Node> {
        let index = self.label_index.read().unwrap();
        let nodes = self.nodes.read().unwrap();

        index
            .get(&label.to_lowercase())
            .map(|ids| ids.iter().filter_map(|id| nodes.get(id).cloned()).collect())
            .unwrap_or_default()
    }

    /// Get edges from a node.
    pub fn get_outgoing_edges(&self, node_id: Uuid) -> Vec<Edge> {
        let edges = self.edges.read().unwrap();
        edges
            .iter()
            .filter(|e| e.source == node_id)
            .cloned()
            .collect()
    }

    /// Get edges to a node.
    pub fn get_incoming_edges(&self, node_id: Uuid) -> Vec<Edge> {
        let edges = self.edges.read().unwrap();
        edges
            .iter()
            .filter(|e| e.target == node_id)
            .cloned()
            .collect()
    }

    /// Get related nodes.
    pub fn get_related(&self, node_id: Uuid, relation: Option<Relation>) -> Vec<Node> {
        let edges = self.edges.read().unwrap();
        let nodes = self.nodes.read().unwrap();

        edges
            .iter()
            .filter(|e| e.source == node_id && relation.map(|r| e.relation == r).unwrap_or(true))
            .filter_map(|e| nodes.get(&e.target).cloned())
            .collect()
    }

    /// Add document and its entries to the graph.
    pub async fn add_document(
        &self,
        document: &Document,
        entries: &[KnowledgeEntry],
    ) -> Result<()> {
        // Create document node
        let doc_node = Node {
            id: document.id,
            node_type: NodeType::Document,
            label: document.title.clone(),
            properties: HashMap::new(),
        };
        self.add_node(doc_node)?;

        // Create chunk nodes and edges
        for entry in entries {
            let chunk_node = Node {
                id: entry.id,
                node_type: NodeType::Chunk,
                label: entry.content.chars().take(50).collect::<String>() + "...",
                properties: HashMap::new(),
            };
            self.add_node(chunk_node)?;

            let edge = Edge {
                id: Uuid::new_v4(),
                source: document.id,
                target: entry.id,
                relation: Relation::Contains,
                weight: 1.0,
                properties: HashMap::new(),
            };
            self.add_edge(edge)?;
        }

        // Extract and add entities (simple for now)
        // In production, use NER
        self.extract_entities(document)?;

        Ok(())
    }

    /// Simple entity extraction.
    fn extract_entities(&self, document: &Document) -> Result<()> {
        // Very simple: look for capitalized words
        let words: Vec<&str> = document.content.split_whitespace().collect();

        for window in words.windows(2) {
            if window.len() == 2
                && window[0]
                    .chars()
                    .next()
                    .map(|c| c.is_uppercase())
                    .unwrap_or(false)
                && window[1]
                    .chars()
                    .next()
                    .map(|c| c.is_uppercase())
                    .unwrap_or(false)
            {
                let entity_name = format!("{} {}", window[0], window[1]);

                // Check if entity already exists
                let existing = self.find_by_label(&entity_name);
                let entity_id = if existing.is_empty() {
                    let node = Node {
                        id: Uuid::new_v4(),
                        node_type: NodeType::Entity,
                        label: entity_name,
                        properties: HashMap::new(),
                    };
                    let id = node.id;
                    self.add_node(node)?;
                    id
                } else {
                    existing[0].id
                };

                // Link entity to document
                let edge = Edge {
                    id: Uuid::new_v4(),
                    source: entity_id,
                    target: document.id,
                    relation: Relation::MentionedIn,
                    weight: 1.0,
                    properties: HashMap::new(),
                };
                self.add_edge(edge)?;
            }
        }

        Ok(())
    }

    /// Get graph statistics.
    pub fn stats(&self) -> GraphStats {
        let nodes = self.nodes.read().unwrap();
        let edges = self.edges.read().unwrap();

        GraphStats {
            node_count: nodes.len(),
            edge_count: edges.len(),
            document_count: nodes
                .values()
                .filter(|n| n.node_type == NodeType::Document)
                .count(),
            entity_count: nodes
                .values()
                .filter(|n| n.node_type == NodeType::Entity)
                .count(),
        }
    }
}

impl Default for KnowledgeGraph {
    fn default() -> Self {
        Self::new()
    }
}

/// Graph statistics.
#[derive(Debug, Clone)]
pub struct GraphStats {
    pub node_count: usize,
    pub edge_count: usize,
    pub document_count: usize,
    pub entity_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_graph_operations() {
        let graph = KnowledgeGraph::new();

        let node1 = Node {
            id: Uuid::new_v4(),
            node_type: NodeType::Concept,
            label: "Rust".to_string(),
            properties: HashMap::new(),
        };

        let node2 = Node {
            id: Uuid::new_v4(),
            node_type: NodeType::Concept,
            label: "Programming".to_string(),
            properties: HashMap::new(),
        };

        graph.add_node(node1.clone()).unwrap();
        graph.add_node(node2.clone()).unwrap();

        let edge = Edge {
            id: Uuid::new_v4(),
            source: node1.id,
            target: node2.id,
            relation: Relation::IsA,
            weight: 1.0,
            properties: HashMap::new(),
        };
        graph.add_edge(edge).unwrap();

        let related = graph.get_related(node1.id, Some(Relation::IsA));
        assert_eq!(related.len(), 1);
        assert_eq!(related[0].label, "Programming");
    }

    #[test]
    fn test_find_by_label() {
        let graph = KnowledgeGraph::new();

        let node = Node {
            id: Uuid::new_v4(),
            node_type: NodeType::Entity,
            label: "Test Entity".to_string(),
            properties: HashMap::new(),
        };
        graph.add_node(node).unwrap();

        let found = graph.find_by_label("test entity");
        assert_eq!(found.len(), 1);
    }
}
