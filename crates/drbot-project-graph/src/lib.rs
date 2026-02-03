//! Project dependency graph analysis.
//!
//! This crate provides:
//! - Dependency graph construction
//! - Circular dependency detection
//! - Impact analysis
//! - Project structure understanding

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;

/// Graph errors.
#[derive(Debug, Error)]
pub enum GraphError {
    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Node not found: {0}")]
    NodeNotFound(String),

    #[error("Circular dependency detected: {0}")]
    CircularDependency(String),

    #[error("Analysis failed: {0}")]
    AnalysisFailed(String),
}

/// Result type for graph operations.
pub type Result<T> = std::result::Result<T, GraphError>;

/// A node in the dependency graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    /// Node identifier.
    pub id: String,
    /// Node name (e.g., package name).
    pub name: String,
    /// Node type.
    pub node_type: NodeType,
    /// File path.
    pub path: Option<PathBuf>,
    /// Version.
    pub version: Option<String>,
    /// Metadata.
    pub metadata: HashMap<String, String>,
    /// Created at.
    pub created_at: DateTime<Utc>,
}

/// Node types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeType {
    /// Package/crate.
    Package,
    /// Module.
    Module,
    /// File.
    File,
    /// Function.
    Function,
    /// Class/struct.
    Type,
    /// External dependency.
    External,
}

/// An edge in the dependency graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    /// Source node ID.
    pub from: String,
    /// Target node ID.
    pub to: String,
    /// Edge type.
    pub edge_type: EdgeType,
    /// Weight/strength.
    pub weight: f64,
    /// Metadata.
    pub metadata: HashMap<String, String>,
}

/// Edge types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EdgeType {
    /// Direct dependency.
    Depends,
    /// Dev dependency.
    DevDepends,
    /// Optional dependency.
    OptionalDepends,
    /// Imports.
    Imports,
    /// Extends/inherits.
    Extends,
    /// Calls.
    Calls,
    /// References.
    References,
}

/// The dependency graph.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DependencyGraph {
    /// Nodes in the graph.
    pub nodes: HashMap<String, GraphNode>,
    /// Edges (from -> list of edges).
    pub edges: HashMap<String, Vec<GraphEdge>>,
    /// Reverse edges (to -> list of edges).
    pub reverse_edges: HashMap<String, Vec<GraphEdge>>,
    /// Graph metadata.
    pub metadata: HashMap<String, String>,
    /// Last updated.
    pub updated_at: DateTime<Utc>,
}

impl DependencyGraph {
    /// Create a new empty graph.
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            edges: HashMap::new(),
            reverse_edges: HashMap::new(),
            metadata: HashMap::new(),
            updated_at: Utc::now(),
        }
    }

    /// Add a node to the graph.
    pub fn add_node(&mut self, node: GraphNode) {
        self.nodes.insert(node.id.clone(), node);
        self.updated_at = Utc::now();
    }

    /// Add an edge to the graph.
    pub fn add_edge(&mut self, edge: GraphEdge) {
        self.edges
            .entry(edge.from.clone())
            .or_default()
            .push(edge.clone());
        self.reverse_edges
            .entry(edge.to.clone())
            .or_default()
            .push(edge);
        self.updated_at = Utc::now();
    }

    /// Get a node by ID.
    pub fn get_node(&self, id: &str) -> Option<&GraphNode> {
        self.nodes.get(id)
    }

    /// Get outgoing edges from a node.
    pub fn get_edges(&self, from: &str) -> Vec<&GraphEdge> {
        self.edges
            .get(from)
            .map(|e| e.iter().collect())
            .unwrap_or_default()
    }

    /// Get incoming edges to a node.
    pub fn get_reverse_edges(&self, to: &str) -> Vec<&GraphEdge> {
        self.reverse_edges
            .get(to)
            .map(|e| e.iter().collect())
            .unwrap_or_default()
    }

    /// Get direct dependencies of a node.
    pub fn get_dependencies(&self, node_id: &str) -> Vec<&GraphNode> {
        self.get_edges(node_id)
            .iter()
            .filter_map(|e| self.nodes.get(&e.to))
            .collect()
    }

    /// Get direct dependents of a node.
    pub fn get_dependents(&self, node_id: &str) -> Vec<&GraphNode> {
        self.get_reverse_edges(node_id)
            .iter()
            .filter_map(|e| self.nodes.get(&e.from))
            .collect()
    }

    /// Get all transitive dependencies.
    pub fn get_all_dependencies(&self, node_id: &str) -> Vec<&GraphNode> {
        let mut visited = HashSet::new();
        let mut result = Vec::new();
        let mut queue = VecDeque::new();

        // Mark starting node as visited
        visited.insert(node_id.to_string());
        queue.push_back(node_id.to_string());

        while let Some(current) = queue.pop_front() {
            for edge in self.get_edges(&current) {
                if !visited.contains(&edge.to) {
                    visited.insert(edge.to.clone());
                    if let Some(node) = self.nodes.get(&edge.to) {
                        result.push(node);
                        queue.push_back(edge.to.clone());
                    }
                }
            }
        }

        result
    }

    /// Get all transitive dependents (impact analysis).
    pub fn get_all_dependents(&self, node_id: &str) -> Vec<&GraphNode> {
        let mut visited = HashSet::new();
        let mut result = Vec::new();
        let mut queue = VecDeque::new();

        // Mark starting node as visited
        visited.insert(node_id.to_string());
        queue.push_back(node_id.to_string());

        while let Some(current) = queue.pop_front() {
            for edge in self.get_reverse_edges(&current) {
                if !visited.contains(&edge.from) {
                    visited.insert(edge.from.clone());
                    if let Some(node) = self.nodes.get(&edge.from) {
                        result.push(node);
                        queue.push_back(edge.from.clone());
                    }
                }
            }
        }

        result
    }

    /// Detect circular dependencies.
    pub fn detect_cycles(&self) -> Vec<Vec<String>> {
        let mut cycles = Vec::new();
        let mut visited = HashSet::new();
        let mut rec_stack = HashSet::new();
        let mut path = Vec::new();

        for node_id in self.nodes.keys() {
            if !visited.contains(node_id) {
                self.dfs_cycle(
                    node_id,
                    &mut visited,
                    &mut rec_stack,
                    &mut path,
                    &mut cycles,
                );
            }
        }

        cycles
    }

    fn dfs_cycle(
        &self,
        node_id: &str,
        visited: &mut HashSet<String>,
        rec_stack: &mut HashSet<String>,
        path: &mut Vec<String>,
        cycles: &mut Vec<Vec<String>>,
    ) {
        visited.insert(node_id.to_string());
        rec_stack.insert(node_id.to_string());
        path.push(node_id.to_string());

        for edge in self.get_edges(node_id) {
            if !visited.contains(&edge.to) {
                self.dfs_cycle(&edge.to, visited, rec_stack, path, cycles);
            } else if rec_stack.contains(&edge.to) {
                // Found a cycle - extract it from path
                if let Some(start_idx) = path.iter().position(|x| x == &edge.to) {
                    let cycle: Vec<String> = path[start_idx..].to_vec();
                    cycles.push(cycle);
                }
            }
        }

        path.pop();
        rec_stack.remove(node_id);
    }

    /// Topological sort of the graph.
    /// Returns nodes in dependency order (dependencies first).
    pub fn topological_sort(&self) -> Result<Vec<String>> {
        let mut out_degree: HashMap<String, usize> = HashMap::new();

        // Initialize out-degrees
        for node_id in self.nodes.keys() {
            out_degree.insert(node_id.clone(), 0);
        }

        // Calculate out-degrees (number of dependencies)
        // An edge A->B means A depends on B
        for edges in self.edges.values() {
            for edge in edges {
                *out_degree.entry(edge.from.clone()).or_insert(0) += 1;
            }
        }

        // Queue of nodes with no dependencies (leaf nodes)
        let mut queue: VecDeque<String> = out_degree
            .iter()
            .filter(|(_, &degree)| degree == 0)
            .map(|(id, _)| id.clone())
            .collect();

        let mut result = Vec::new();

        while let Some(node_id) = queue.pop_front() {
            result.push(node_id.clone());

            // For each node that depends on this one, reduce their out_degree
            for edge in self.get_reverse_edges(&node_id) {
                if let Some(degree) = out_degree.get_mut(&edge.from) {
                    *degree -= 1;
                    if *degree == 0 {
                        queue.push_back(edge.from.clone());
                    }
                }
            }
        }

        if result.len() != self.nodes.len() {
            return Err(GraphError::CircularDependency(
                "Graph contains cycles, cannot topologically sort".to_string(),
            ));
        }

        Ok(result)
    }

    /// Get graph statistics.
    pub fn stats(&self) -> GraphStats {
        let edge_count: usize = self.edges.values().map(|e| e.len()).sum();

        GraphStats {
            node_count: self.nodes.len(),
            edge_count,
            max_depth: self.calculate_max_depth(),
            cycles: self.detect_cycles().len(),
        }
    }

    fn calculate_max_depth(&self) -> usize {
        let mut max_depth = 0;

        // Find root nodes (no incoming edges)
        let roots: Vec<&String> = self
            .nodes
            .keys()
            .filter(|id| self.get_reverse_edges(id).is_empty())
            .collect();

        for root in roots {
            let depth = self.calculate_depth_from(root, &mut HashSet::new());
            max_depth = max_depth.max(depth);
        }

        max_depth
    }

    fn calculate_depth_from(&self, node_id: &str, visited: &mut HashSet<String>) -> usize {
        if visited.contains(node_id) {
            return 0;
        }
        visited.insert(node_id.to_string());

        let children: Vec<_> = self.get_edges(node_id).iter().map(|e| &e.to).collect();

        if children.is_empty() {
            return 1;
        }

        let max_child_depth = children
            .iter()
            .map(|child| self.calculate_depth_from(child, visited))
            .max()
            .unwrap_or(0);

        1 + max_child_depth
    }
}

/// Graph statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphStats {
    /// Number of nodes.
    pub node_count: usize,
    /// Number of edges.
    pub edge_count: usize,
    /// Maximum depth.
    pub max_depth: usize,
    /// Number of cycles.
    pub cycles: usize,
}

/// Impact analysis result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpactAnalysis {
    /// Changed node.
    pub changed_node: String,
    /// Directly affected nodes.
    pub direct_impact: Vec<String>,
    /// All affected nodes (transitive).
    pub total_impact: Vec<String>,
    /// Impact score (0-1).
    pub impact_score: f64,
    /// Risk level.
    pub risk_level: RiskLevel,
}

/// Risk levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

/// Graph analyzer trait.
#[async_trait]
pub trait GraphAnalyzer: Send + Sync {
    /// Parse project and build dependency graph.
    async fn analyze_project(&self, path: &std::path::Path) -> Result<DependencyGraph>;

    /// Supported project types.
    fn supported_types(&self) -> Vec<ProjectType>;
}

/// Project types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProjectType {
    Rust,
    Node,
    Python,
    Go,
    Java,
    CSharp,
    Generic,
}

/// The project graph engine.
pub struct ProjectGraph {
    /// Analyzers for different project types.
    analyzers: Vec<Arc<dyn GraphAnalyzer>>,
    /// Cached graphs.
    graphs: Arc<RwLock<HashMap<PathBuf, DependencyGraph>>>,
}

impl ProjectGraph {
    /// Create a new project graph engine.
    pub fn new() -> Self {
        Self {
            analyzers: Vec::new(),
            graphs: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register an analyzer.
    pub fn register_analyzer(&mut self, analyzer: Arc<dyn GraphAnalyzer>) {
        self.analyzers.push(analyzer);
    }

    /// Analyze a project.
    pub async fn analyze(&self, path: &std::path::Path) -> Result<DependencyGraph> {
        // Try each analyzer
        for analyzer in &self.analyzers {
            match analyzer.analyze_project(path).await {
                Ok(graph) => {
                    // Cache the result
                    let mut graphs = self.graphs.write().await;
                    graphs.insert(path.to_path_buf(), graph.clone());
                    return Ok(graph);
                }
                Err(_) => continue,
            }
        }

        Err(GraphError::AnalysisFailed(
            "No suitable analyzer found for project".to_string(),
        ))
    }

    /// Get cached graph.
    pub async fn get_cached(&self, path: &std::path::Path) -> Option<DependencyGraph> {
        let graphs = self.graphs.read().await;
        graphs.get(path).cloned()
    }

    /// Analyze impact of changing a node.
    pub async fn analyze_impact(
        &self,
        path: &std::path::Path,
        node_id: &str,
    ) -> Result<ImpactAnalysis> {
        let graph = self
            .get_cached(path)
            .await
            .ok_or_else(|| GraphError::AnalysisFailed("Project not analyzed".to_string()))?;

        if !graph.nodes.contains_key(node_id) {
            return Err(GraphError::NodeNotFound(node_id.to_string()));
        }

        let direct_dependents: Vec<String> = graph
            .get_dependents(node_id)
            .iter()
            .map(|n| n.id.clone())
            .collect();

        let all_dependents: Vec<String> = graph
            .get_all_dependents(node_id)
            .iter()
            .map(|n| n.id.clone())
            .collect();

        let total_nodes = graph.nodes.len();
        let impact_ratio = if total_nodes > 0 {
            all_dependents.len() as f64 / total_nodes as f64
        } else {
            0.0
        };

        let risk_level = if impact_ratio > 0.5 {
            RiskLevel::Critical
        } else if impact_ratio > 0.25 {
            RiskLevel::High
        } else if impact_ratio > 0.1 {
            RiskLevel::Medium
        } else {
            RiskLevel::Low
        };

        Ok(ImpactAnalysis {
            changed_node: node_id.to_string(),
            direct_impact: direct_dependents,
            total_impact: all_dependents,
            impact_score: impact_ratio,
            risk_level,
        })
    }

    /// Find build order (topological sort).
    pub async fn build_order(&self, path: &std::path::Path) -> Result<Vec<String>> {
        let graph = self
            .get_cached(path)
            .await
            .ok_or_else(|| GraphError::AnalysisFailed("Project not analyzed".to_string()))?;

        graph.topological_sort()
    }

    /// Check for circular dependencies.
    pub async fn check_cycles(&self, path: &std::path::Path) -> Result<Vec<Vec<String>>> {
        let graph = self
            .get_cached(path)
            .await
            .ok_or_else(|| GraphError::AnalysisFailed("Project not analyzed".to_string()))?;

        Ok(graph.detect_cycles())
    }
}

impl Default for ProjectGraph {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_graph() -> DependencyGraph {
        let mut graph = DependencyGraph::new();

        // Add nodes
        graph.add_node(GraphNode {
            id: "A".to_string(),
            name: "Package A".to_string(),
            node_type: NodeType::Package,
            path: None,
            version: Some("1.0.0".to_string()),
            metadata: HashMap::new(),
            created_at: Utc::now(),
        });

        graph.add_node(GraphNode {
            id: "B".to_string(),
            name: "Package B".to_string(),
            node_type: NodeType::Package,
            path: None,
            version: Some("1.0.0".to_string()),
            metadata: HashMap::new(),
            created_at: Utc::now(),
        });

        graph.add_node(GraphNode {
            id: "C".to_string(),
            name: "Package C".to_string(),
            node_type: NodeType::Package,
            path: None,
            version: Some("1.0.0".to_string()),
            metadata: HashMap::new(),
            created_at: Utc::now(),
        });

        // A depends on B and C
        graph.add_edge(GraphEdge {
            from: "A".to_string(),
            to: "B".to_string(),
            edge_type: EdgeType::Depends,
            weight: 1.0,
            metadata: HashMap::new(),
        });

        graph.add_edge(GraphEdge {
            from: "A".to_string(),
            to: "C".to_string(),
            edge_type: EdgeType::Depends,
            weight: 1.0,
            metadata: HashMap::new(),
        });

        // B depends on C
        graph.add_edge(GraphEdge {
            from: "B".to_string(),
            to: "C".to_string(),
            edge_type: EdgeType::Depends,
            weight: 1.0,
            metadata: HashMap::new(),
        });

        graph
    }

    #[test]
    fn test_get_dependencies() {
        let graph = create_test_graph();

        let deps = graph.get_dependencies("A");
        assert_eq!(deps.len(), 2);
    }

    #[test]
    fn test_get_dependents() {
        let graph = create_test_graph();

        let dependents = graph.get_dependents("C");
        assert_eq!(dependents.len(), 2); // A and B depend on C
    }

    #[test]
    fn test_all_dependencies() {
        let graph = create_test_graph();

        let all_deps = graph.get_all_dependencies("A");
        assert_eq!(all_deps.len(), 2); // B and C
    }

    #[test]
    fn test_topological_sort() {
        let graph = create_test_graph();

        let order = graph.topological_sort().unwrap();

        // C should come before B, and B should come before A
        let c_pos = order.iter().position(|x| x == "C").unwrap();
        let b_pos = order.iter().position(|x| x == "B").unwrap();
        let a_pos = order.iter().position(|x| x == "A").unwrap();

        assert!(c_pos < b_pos);
        assert!(b_pos < a_pos);
    }

    #[test]
    fn test_no_cycles() {
        let graph = create_test_graph();

        let cycles = graph.detect_cycles();
        assert!(cycles.is_empty());
    }

    #[test]
    fn test_detect_cycle() {
        let mut graph = DependencyGraph::new();

        graph.add_node(GraphNode {
            id: "A".to_string(),
            name: "A".to_string(),
            node_type: NodeType::Package,
            path: None,
            version: None,
            metadata: HashMap::new(),
            created_at: Utc::now(),
        });

        graph.add_node(GraphNode {
            id: "B".to_string(),
            name: "B".to_string(),
            node_type: NodeType::Package,
            path: None,
            version: None,
            metadata: HashMap::new(),
            created_at: Utc::now(),
        });

        // Create cycle: A -> B -> A
        graph.add_edge(GraphEdge {
            from: "A".to_string(),
            to: "B".to_string(),
            edge_type: EdgeType::Depends,
            weight: 1.0,
            metadata: HashMap::new(),
        });

        graph.add_edge(GraphEdge {
            from: "B".to_string(),
            to: "A".to_string(),
            edge_type: EdgeType::Depends,
            weight: 1.0,
            metadata: HashMap::new(),
        });

        let cycles = graph.detect_cycles();
        assert!(!cycles.is_empty());
    }

    #[test]
    fn test_graph_stats() {
        let graph = create_test_graph();

        let stats = graph.stats();
        assert_eq!(stats.node_count, 3);
        assert_eq!(stats.edge_count, 3);
        assert_eq!(stats.cycles, 0);
    }

    #[tokio::test]
    async fn test_project_graph_engine() {
        let engine = ProjectGraph::new();

        // Without analyzers, should fail
        let result = engine.analyze(std::path::Path::new("/test")).await;
        assert!(result.is_err());
    }
}
