//! Conversation graphs for drbot.
//!
//! Visualize conversation flow and relationships.
//!
//! # Features
//!
//! - Graph-based conversation modeling
//! - Topic tracking
//! - Relationship visualization
//! - Path analysis

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Conversation graph result type.
pub type Result<T> = std::result::Result<T, GraphError>;

/// Graph errors.
#[derive(Debug, thiserror::Error)]
pub enum GraphError {
    #[error("Node not found: {0}")]
    NodeNotFound(Uuid),
    #[error("Edge not found")]
    EdgeNotFound,
    #[error("Graph is empty")]
    EmptyGraph,
    #[error("Cycle detected")]
    CycleDetected,
}

/// A conversation node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationNode {
    /// Node ID.
    pub id: Uuid,
    /// Node type.
    pub node_type: NodeType,
    /// Content.
    pub content: String,
    /// Topic.
    pub topic: Option<String>,
    /// Sentiment (-1 to 1).
    pub sentiment: f32,
    /// Importance (0-1).
    pub importance: f32,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
    /// Metadata.
    pub metadata: HashMap<String, serde_json::Value>,
}

impl ConversationNode {
    /// Create a new node.
    pub fn new(node_type: NodeType, content: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            node_type,
            content: content.to_string(),
            topic: None,
            sentiment: 0.0,
            importance: 0.5,
            timestamp: Utc::now(),
            metadata: HashMap::new(),
        }
    }

    /// Set topic.
    pub fn with_topic(mut self, topic: &str) -> Self {
        self.topic = Some(topic.to_string());
        self
    }

    /// Set importance.
    pub fn with_importance(mut self, importance: f32) -> Self {
        self.importance = importance.clamp(0.0, 1.0);
        self
    }
}

/// Node types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeType {
    /// User message.
    UserMessage,
    /// Assistant response.
    AssistantResponse,
    /// System message.
    SystemMessage,
    /// Topic marker.
    Topic,
    /// Action taken.
    Action,
    /// Decision point.
    Decision,
    /// Summary.
    Summary,
}

/// An edge between nodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationEdge {
    /// Edge ID.
    pub id: Uuid,
    /// Source node.
    pub from: Uuid,
    /// Target node.
    pub to: Uuid,
    /// Edge type.
    pub edge_type: EdgeType,
    /// Weight (0-1).
    pub weight: f32,
    /// Label.
    pub label: Option<String>,
}

impl ConversationEdge {
    /// Create a new edge.
    pub fn new(from: Uuid, to: Uuid, edge_type: EdgeType) -> Self {
        Self {
            id: Uuid::new_v4(),
            from,
            to,
            edge_type,
            weight: 1.0,
            label: None,
        }
    }

    /// Set weight.
    pub fn with_weight(mut self, weight: f32) -> Self {
        self.weight = weight.clamp(0.0, 1.0);
        self
    }

    /// Set label.
    pub fn with_label(mut self, label: &str) -> Self {
        self.label = Some(label.to_string());
        self
    }
}

/// Edge types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeType {
    /// Sequential flow.
    Sequence,
    /// Topic relationship.
    TopicRelation,
    /// Reference to earlier.
    Reference,
    /// Caused by.
    CausedBy,
    /// Follow-up.
    FollowUp,
    /// Contradiction.
    Contradiction,
    /// Clarification.
    Clarification,
}

/// A path through the graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationPath {
    /// Path nodes.
    pub nodes: Vec<Uuid>,
    /// Total weight.
    pub total_weight: f32,
    /// Topics covered.
    pub topics: Vec<String>,
}

/// Graph statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphStats {
    /// Total nodes.
    pub total_nodes: usize,
    /// Total edges.
    pub total_edges: usize,
    /// Nodes by type.
    pub nodes_by_type: HashMap<NodeType, usize>,
    /// Topics count.
    pub topics_count: usize,
    /// Average node importance.
    pub avg_importance: f32,
    /// Graph depth.
    pub depth: usize,
}

/// Graph configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphConfig {
    /// Auto-detect topics.
    pub auto_topics: bool,
    /// Auto-calculate importance.
    pub auto_importance: bool,
    /// Track sentiment.
    pub track_sentiment: bool,
    /// Maximum nodes to keep.
    pub max_nodes: usize,
}

impl Default for GraphConfig {
    fn default() -> Self {
        Self {
            auto_topics: true,
            auto_importance: true,
            track_sentiment: true,
            max_nodes: 1000,
        }
    }
}

/// Conversation graph.
pub struct ConversationGraph {
    config: GraphConfig,
    nodes: Arc<RwLock<HashMap<Uuid, ConversationNode>>>,
    edges: Arc<RwLock<Vec<ConversationEdge>>>,
    adjacency: Arc<RwLock<HashMap<Uuid, Vec<Uuid>>>>,
    topics: Arc<RwLock<HashMap<String, Vec<Uuid>>>>,
}

impl ConversationGraph {
    /// Create a new conversation graph.
    pub fn new(config: GraphConfig) -> Self {
        Self {
            config,
            nodes: Arc::new(RwLock::new(HashMap::new())),
            edges: Arc::new(RwLock::new(Vec::new())),
            adjacency: Arc::new(RwLock::new(HashMap::new())),
            topics: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Add a node to the graph.
    pub async fn add_node(&self, node: ConversationNode) -> Uuid {
        let id = node.id;

        // Update topic index
        if let Some(topic) = &node.topic {
            self.topics
                .write()
                .await
                .entry(topic.clone())
                .or_default()
                .push(id);
        }

        // Enforce max nodes
        let mut nodes = self.nodes.write().await;
        if nodes.len() >= self.config.max_nodes {
            // Remove oldest node
            if let Some(oldest_id) = nodes.values().min_by_key(|n| n.timestamp).map(|n| n.id) {
                nodes.remove(&oldest_id);
                self.adjacency.write().await.remove(&oldest_id);
            }
        }

        nodes.insert(id, node);
        self.adjacency.write().await.insert(id, Vec::new());

        id
    }

    /// Add an edge between nodes.
    pub async fn add_edge(&self, edge: ConversationEdge) -> Result<()> {
        let nodes = self.nodes.read().await;
        if !nodes.contains_key(&edge.from) {
            return Err(GraphError::NodeNotFound(edge.from));
        }
        if !nodes.contains_key(&edge.to) {
            return Err(GraphError::NodeNotFound(edge.to));
        }
        drop(nodes);

        self.adjacency
            .write()
            .await
            .entry(edge.from)
            .or_default()
            .push(edge.to);
        self.edges.write().await.push(edge);

        Ok(())
    }

    /// Connect nodes sequentially.
    pub async fn connect_sequence(&self, from: Uuid, to: Uuid) -> Result<()> {
        self.add_edge(ConversationEdge::new(from, to, EdgeType::Sequence))
            .await
    }

    /// Get a node by ID.
    pub async fn get_node(&self, id: Uuid) -> Option<ConversationNode> {
        self.nodes.read().await.get(&id).cloned()
    }

    /// Get all nodes.
    pub async fn list_nodes(&self) -> Vec<ConversationNode> {
        self.nodes.read().await.values().cloned().collect()
    }

    /// Get edges from a node.
    pub async fn edges_from(&self, node_id: Uuid) -> Vec<ConversationEdge> {
        self.edges
            .read()
            .await
            .iter()
            .filter(|e| e.from == node_id)
            .cloned()
            .collect()
    }

    /// Get edges to a node.
    pub async fn edges_to(&self, node_id: Uuid) -> Vec<ConversationEdge> {
        self.edges
            .read()
            .await
            .iter()
            .filter(|e| e.to == node_id)
            .cloned()
            .collect()
    }

    /// Find path between nodes.
    pub async fn find_path(&self, from: Uuid, to: Uuid) -> Option<ConversationPath> {
        let adjacency = self.adjacency.read().await;
        let nodes = self.nodes.read().await;

        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        let mut parent: HashMap<Uuid, Uuid> = HashMap::new();

        queue.push_back(from);
        visited.insert(from);

        while let Some(current) = queue.pop_front() {
            if current == to {
                // Reconstruct path
                let mut path = vec![to];
                let mut node = to;
                while let Some(&p) = parent.get(&node) {
                    path.push(p);
                    node = p;
                }
                path.reverse();

                let topics: Vec<_> = path
                    .iter()
                    .filter_map(|id| nodes.get(id))
                    .filter_map(|n| n.topic.clone())
                    .collect();

                return Some(ConversationPath {
                    nodes: path,
                    total_weight: 1.0,
                    topics,
                });
            }

            if let Some(neighbors) = adjacency.get(&current) {
                for &neighbor in neighbors {
                    if !visited.contains(&neighbor) {
                        visited.insert(neighbor);
                        parent.insert(neighbor, current);
                        queue.push_back(neighbor);
                    }
                }
            }
        }

        None
    }

    /// Get nodes by topic.
    pub async fn nodes_by_topic(&self, topic: &str) -> Vec<ConversationNode> {
        let topics = self.topics.read().await;
        let nodes = self.nodes.read().await;

        topics
            .get(topic)
            .map(|ids| ids.iter().filter_map(|id| nodes.get(id).cloned()).collect())
            .unwrap_or_default()
    }

    /// Get all topics.
    pub async fn list_topics(&self) -> Vec<String> {
        self.topics.read().await.keys().cloned().collect()
    }

    /// Get recent nodes.
    pub async fn recent_nodes(&self, limit: usize) -> Vec<ConversationNode> {
        let mut nodes: Vec<_> = self.nodes.read().await.values().cloned().collect();
        nodes.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        nodes.truncate(limit);
        nodes
    }

    /// Get important nodes.
    pub async fn important_nodes(&self, threshold: f32) -> Vec<ConversationNode> {
        self.nodes
            .read()
            .await
            .values()
            .filter(|n| n.importance >= threshold)
            .cloned()
            .collect()
    }

    /// Calculate graph depth.
    pub async fn depth(&self) -> usize {
        let adjacency = self.adjacency.read().await;

        // Find root nodes (no incoming edges)
        let all_targets: HashSet<_> = adjacency.values().flatten().copied().collect();
        let roots: Vec<_> = adjacency
            .keys()
            .filter(|k| !all_targets.contains(*k))
            .copied()
            .collect();

        let mut max_depth = 0;
        for root in roots {
            let depth = self.node_depth(root, &adjacency, &mut HashSet::new());
            max_depth = max_depth.max(depth);
        }

        max_depth
    }

    fn node_depth(
        &self,
        node: Uuid,
        adjacency: &HashMap<Uuid, Vec<Uuid>>,
        visited: &mut HashSet<Uuid>,
    ) -> usize {
        if visited.contains(&node) {
            return 0;
        }
        visited.insert(node);

        let mut max_child_depth = 0;
        if let Some(children) = adjacency.get(&node) {
            for &child in children {
                let depth = self.node_depth(child, adjacency, visited);
                max_child_depth = max_child_depth.max(depth);
            }
        }

        1 + max_child_depth
    }

    /// Get graph statistics.
    pub async fn stats(&self) -> GraphStats {
        let nodes = self.nodes.read().await;
        let edges = self.edges.read().await;
        let topics = self.topics.read().await;

        let mut nodes_by_type: HashMap<NodeType, usize> = HashMap::new();
        let mut total_importance = 0.0;

        for node in nodes.values() {
            *nodes_by_type.entry(node.node_type).or_insert(0) += 1;
            total_importance += node.importance;
        }

        GraphStats {
            total_nodes: nodes.len(),
            total_edges: edges.len(),
            nodes_by_type,
            topics_count: topics.len(),
            avg_importance: if !nodes.is_empty() {
                total_importance / nodes.len() as f32
            } else {
                0.0
            },
            depth: 0, // Calculated separately
        }
    }

    /// Render graph to DOT format.
    pub async fn to_dot(&self) -> String {
        let nodes = self.nodes.read().await;
        let edges = self.edges.read().await;

        let mut dot = String::from("digraph conversation {\n");
        dot.push_str("  rankdir=TB;\n");
        dot.push_str("  node [shape=box];\n\n");

        for node in nodes.values() {
            let color = match node.node_type {
                NodeType::UserMessage => "lightblue",
                NodeType::AssistantResponse => "lightgreen",
                NodeType::Topic => "yellow",
                NodeType::Action => "orange",
                _ => "white",
            };

            let label = node.content.chars().take(30).collect::<String>();
            dot.push_str(&format!(
                "  \"{}\" [label=\"{}\", fillcolor=\"{}\", style=filled];\n",
                node.id,
                label.replace('"', "'"),
                color
            ));
        }

        dot.push('\n');

        for edge in edges.iter() {
            let style = match edge.edge_type {
                EdgeType::Sequence => "",
                EdgeType::TopicRelation => "[style=dashed]",
                EdgeType::Reference => "[style=dotted]",
                _ => "",
            };

            dot.push_str(&format!(
                "  \"{}\" -> \"{}\" {};\n",
                edge.from, edge.to, style
            ));
        }

        dot.push_str("}\n");
        dot
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_add_nodes_and_edges() {
        let graph = ConversationGraph::new(GraphConfig::default());

        let node1 = ConversationNode::new(NodeType::UserMessage, "Hello");
        let node2 = ConversationNode::new(NodeType::AssistantResponse, "Hi there!");

        let id1 = graph.add_node(node1).await;
        let id2 = graph.add_node(node2).await;

        graph.connect_sequence(id1, id2).await.unwrap();

        let stats = graph.stats().await;
        assert_eq!(stats.total_nodes, 2);
        assert_eq!(stats.total_edges, 1);
    }

    #[tokio::test]
    async fn test_find_path() {
        let graph = ConversationGraph::new(GraphConfig::default());

        let id1 = graph
            .add_node(ConversationNode::new(NodeType::UserMessage, "A"))
            .await;
        let id2 = graph
            .add_node(ConversationNode::new(NodeType::AssistantResponse, "B"))
            .await;
        let id3 = graph
            .add_node(ConversationNode::new(NodeType::UserMessage, "C"))
            .await;

        graph.connect_sequence(id1, id2).await.unwrap();
        graph.connect_sequence(id2, id3).await.unwrap();

        let path = graph.find_path(id1, id3).await;
        assert!(path.is_some());
        assert_eq!(path.unwrap().nodes.len(), 3);
    }

    #[tokio::test]
    async fn test_topics() {
        let graph = ConversationGraph::new(GraphConfig::default());

        graph
            .add_node(
                ConversationNode::new(NodeType::UserMessage, "About AI")
                    .with_topic("artificial_intelligence"),
            )
            .await;

        graph
            .add_node(
                ConversationNode::new(NodeType::UserMessage, "More about AI")
                    .with_topic("artificial_intelligence"),
            )
            .await;

        let ai_nodes = graph.nodes_by_topic("artificial_intelligence").await;
        assert_eq!(ai_nodes.len(), 2);

        let topics = graph.list_topics().await;
        assert!(topics.contains(&"artificial_intelligence".to_string()));
    }

    #[tokio::test]
    async fn test_to_dot() {
        let graph = ConversationGraph::new(GraphConfig::default());

        let id1 = graph
            .add_node(ConversationNode::new(NodeType::UserMessage, "Hello"))
            .await;
        let id2 = graph
            .add_node(ConversationNode::new(NodeType::AssistantResponse, "Hi"))
            .await;

        graph.connect_sequence(id1, id2).await.unwrap();

        let dot = graph.to_dot().await;
        assert!(dot.contains("digraph conversation"));
        assert!(dot.contains("->"));
    }
}
