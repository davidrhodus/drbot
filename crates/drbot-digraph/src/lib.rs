//! Directed graph for drbot.
//!
//! This crate provides:
//! - Directed graph with nodes and edges
//! - DFS, BFS traversal
//! - Topological sort
//! - Cycle detection

use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::Hash;
use thiserror::Error;

/// Digraph error types.
#[derive(Error, Debug)]
pub enum DiGraphError {
    #[error("Node not found")]
    NodeNotFound,

    #[error("Edge not found")]
    EdgeNotFound,

    #[error("Cycle detected")]
    CycleDetected,

    #[error("Node already exists")]
    NodeExists,
}

/// Result type for digraph operations.
pub type Result<T> = std::result::Result<T, DiGraphError>;

/// Directed graph.
#[derive(Debug, Clone)]
pub struct DiGraph<N, E = ()> {
    nodes: HashMap<N, ()>,
    edges: HashMap<N, HashMap<N, E>>,
}

impl<N: Hash + Eq + Clone, E> DiGraph<N, E> {
    /// Create empty graph.
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            edges: HashMap::new(),
        }
    }

    /// Add node.
    pub fn add_node(&mut self, node: N) -> bool {
        if self.nodes.contains_key(&node) {
            return false;
        }
        self.nodes.insert(node.clone(), ());
        self.edges.insert(node, HashMap::new());
        true
    }

    /// Add edge.
    pub fn add_edge(&mut self, from: N, to: N, weight: E) -> bool {
        // Ensure both nodes exist
        self.add_node(from.clone());
        self.add_node(to.clone());

        self.edges.entry(from).or_default().insert(to, weight);
        true
    }

    /// Remove node and all its edges.
    pub fn remove_node(&mut self, node: &N) -> bool {
        if self.nodes.remove(node).is_none() {
            return false;
        }

        self.edges.remove(node);

        // Remove edges pointing to this node
        for edges in self.edges.values_mut() {
            edges.remove(node);
        }

        true
    }

    /// Remove edge.
    pub fn remove_edge(&mut self, from: &N, to: &N) -> Option<E> {
        self.edges.get_mut(from)?.remove(to)
    }

    /// Check if node exists.
    pub fn has_node(&self, node: &N) -> bool {
        self.nodes.contains_key(node)
    }

    /// Check if edge exists.
    pub fn has_edge(&self, from: &N, to: &N) -> bool {
        self.edges
            .get(from)
            .map(|e| e.contains_key(to))
            .unwrap_or(false)
    }

    /// Get edge weight.
    pub fn edge_weight(&self, from: &N, to: &N) -> Option<&E> {
        self.edges.get(from)?.get(to)
    }

    /// Get successors of a node.
    pub fn successors(&self, node: &N) -> impl Iterator<Item = &N> {
        self.edges.get(node).into_iter().flat_map(|e| e.keys())
    }

    /// Get predecessors of a node.
    pub fn predecessors<'a>(&'a self, node: &'a N) -> impl Iterator<Item = &'a N> + 'a {
        self.edges
            .iter()
            .filter(move |(_, targets)| targets.contains_key(node))
            .map(|(source, _)| source)
    }

    /// Get out-degree.
    pub fn out_degree(&self, node: &N) -> usize {
        self.edges.get(node).map(|e| e.len()).unwrap_or(0)
    }

    /// Get in-degree.
    pub fn in_degree(&self, node: &N) -> usize {
        self.edges.values().filter(|e| e.contains_key(node)).count()
    }

    /// Get node count.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Get edge count.
    pub fn edge_count(&self) -> usize {
        self.edges.values().map(|e| e.len()).sum()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Get all nodes.
    pub fn nodes(&self) -> impl Iterator<Item = &N> {
        self.nodes.keys()
    }

    /// Get all edges.
    pub fn edges(&self) -> impl Iterator<Item = (&N, &N, &E)> {
        self.edges
            .iter()
            .flat_map(|(from, targets)| targets.iter().map(move |(to, weight)| (from, to, weight)))
    }

    /// DFS traversal from start node.
    pub fn dfs(&self, start: &N) -> Vec<N> {
        let mut visited = HashSet::new();
        let mut result = Vec::new();
        self.dfs_recursive(start, &mut visited, &mut result);
        result
    }

    fn dfs_recursive(&self, node: &N, visited: &mut HashSet<N>, result: &mut Vec<N>) {
        if visited.contains(node) {
            return;
        }

        visited.insert(node.clone());
        result.push(node.clone());

        if let Some(successors) = self.edges.get(node) {
            for successor in successors.keys() {
                self.dfs_recursive(successor, visited, result);
            }
        }
    }

    /// BFS traversal from start node.
    pub fn bfs(&self, start: &N) -> Vec<N> {
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        let mut result = Vec::new();

        visited.insert(start.clone());
        queue.push_back(start.clone());

        while let Some(node) = queue.pop_front() {
            result.push(node.clone());

            if let Some(successors) = self.edges.get(&node) {
                for successor in successors.keys() {
                    if !visited.contains(successor) {
                        visited.insert(successor.clone());
                        queue.push_back(successor.clone());
                    }
                }
            }
        }

        result
    }

    /// Check for cycles.
    pub fn has_cycle(&self) -> bool {
        let mut visited = HashSet::new();
        let mut rec_stack = HashSet::new();

        for node in self.nodes.keys() {
            if self.has_cycle_recursive(node, &mut visited, &mut rec_stack) {
                return true;
            }
        }

        false
    }

    fn has_cycle_recursive(
        &self,
        node: &N,
        visited: &mut HashSet<N>,
        rec_stack: &mut HashSet<N>,
    ) -> bool {
        if rec_stack.contains(node) {
            return true;
        }
        if visited.contains(node) {
            return false;
        }

        visited.insert(node.clone());
        rec_stack.insert(node.clone());

        if let Some(successors) = self.edges.get(node) {
            for successor in successors.keys() {
                if self.has_cycle_recursive(successor, visited, rec_stack) {
                    return true;
                }
            }
        }

        rec_stack.remove(node);
        false
    }

    /// Topological sort (only valid for DAGs).
    pub fn topological_sort(&self) -> Result<Vec<N>> {
        if self.has_cycle() {
            return Err(DiGraphError::CycleDetected);
        }

        let mut visited = HashSet::new();
        let mut result = Vec::new();

        for node in self.nodes.keys() {
            self.topo_recursive(node, &mut visited, &mut result);
        }

        result.reverse();
        Ok(result)
    }

    fn topo_recursive(&self, node: &N, visited: &mut HashSet<N>, result: &mut Vec<N>) {
        if visited.contains(node) {
            return;
        }

        visited.insert(node.clone());

        if let Some(successors) = self.edges.get(node) {
            for successor in successors.keys() {
                self.topo_recursive(successor, visited, result);
            }
        }

        result.push(node.clone());
    }

    /// Clear graph.
    pub fn clear(&mut self) {
        self.nodes.clear();
        self.edges.clear();
    }
}

impl<N: Hash + Eq + Clone, E> Default for DiGraph<N, E> {
    fn default() -> Self {
        Self::new()
    }
}

/// Unweighted digraph (edges have no weight).
pub type UnweightedDiGraph<N> = DiGraph<N, ()>;

impl<N: Hash + Eq + Clone> UnweightedDiGraph<N> {
    /// Add unweighted edge.
    pub fn add_unweighted_edge(&mut self, from: N, to: N) -> bool {
        self.add_edge(from, to, ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_operations() {
        let mut graph: UnweightedDiGraph<&str> = DiGraph::new();
        graph.add_node("a");
        graph.add_node("b");
        graph.add_unweighted_edge("a", "b");

        assert!(graph.has_node(&"a"));
        assert!(graph.has_edge(&"a", &"b"));
        assert!(!graph.has_edge(&"b", &"a"));
    }

    #[test]
    fn test_traversal() {
        let mut graph: UnweightedDiGraph<i32> = DiGraph::new();
        graph.add_unweighted_edge(1, 2);
        graph.add_unweighted_edge(1, 3);
        graph.add_unweighted_edge(2, 4);
        graph.add_unweighted_edge(3, 4);

        let dfs = graph.dfs(&1);
        assert!(dfs.contains(&1));
        assert!(dfs.contains(&4));

        let bfs = graph.bfs(&1);
        assert_eq!(bfs[0], 1);
    }

    #[test]
    fn test_cycle_detection() {
        let mut graph: UnweightedDiGraph<i32> = DiGraph::new();
        graph.add_unweighted_edge(1, 2);
        graph.add_unweighted_edge(2, 3);

        assert!(!graph.has_cycle());

        graph.add_unweighted_edge(3, 1);
        assert!(graph.has_cycle());
    }

    #[test]
    fn test_topological_sort() {
        let mut graph: UnweightedDiGraph<&str> = DiGraph::new();
        graph.add_unweighted_edge("a", "b");
        graph.add_unweighted_edge("a", "c");
        graph.add_unweighted_edge("b", "d");
        graph.add_unweighted_edge("c", "d");

        let sorted = graph.topological_sort().unwrap();
        let a_idx = sorted.iter().position(|n| *n == "a").unwrap();
        let d_idx = sorted.iter().position(|n| *n == "d").unwrap();
        assert!(a_idx < d_idx);
    }
}
