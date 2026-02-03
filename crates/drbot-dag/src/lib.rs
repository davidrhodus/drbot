//! Directed acyclic graph for drbot.
//!
//! This crate provides:
//! - DAG with automatic cycle prevention
//! - Dependency tracking
//! - Execution ordering

use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::Hash;
use thiserror::Error;

/// DAG error types.
#[derive(Error, Debug)]
pub enum DagError {
    #[error("Node not found")]
    NodeNotFound,

    #[error("Would create cycle")]
    WouldCreateCycle,

    #[error("Node already exists")]
    NodeExists,
}

/// Result type for DAG operations.
pub type Result<T> = std::result::Result<T, DagError>;

/// Directed acyclic graph.
#[derive(Debug, Clone)]
pub struct Dag<N, D = ()> {
    nodes: HashMap<N, D>,
    /// Outgoing edges (node -> successors)
    outgoing: HashMap<N, HashSet<N>>,
    /// Incoming edges (node -> predecessors)
    incoming: HashMap<N, HashSet<N>>,
}

impl<N: Hash + Eq + Clone, D> Dag<N, D> {
    /// Create empty DAG.
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            outgoing: HashMap::new(),
            incoming: HashMap::new(),
        }
    }

    /// Add node with data.
    pub fn add_node(&mut self, node: N, data: D) -> bool {
        if self.nodes.contains_key(&node) {
            return false;
        }
        self.nodes.insert(node.clone(), data);
        self.outgoing.insert(node.clone(), HashSet::new());
        self.incoming.insert(node, HashSet::new());
        true
    }

    /// Add edge (validates no cycle would be created).
    pub fn add_edge(&mut self, from: N, to: N) -> Result<()> {
        if !self.nodes.contains_key(&from) || !self.nodes.contains_key(&to) {
            return Err(DagError::NodeNotFound);
        }

        // Check if adding this edge would create a cycle
        if self.would_create_cycle(&from, &to) {
            return Err(DagError::WouldCreateCycle);
        }

        self.outgoing
            .entry(from.clone())
            .or_default()
            .insert(to.clone());
        self.incoming.entry(to).or_default().insert(from);

        Ok(())
    }

    /// Check if adding edge would create cycle.
    fn would_create_cycle(&self, from: &N, to: &N) -> bool {
        // If `from` is reachable from `to`, adding `from -> to` creates cycle
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(to.clone());

        while let Some(node) = queue.pop_front() {
            if &node == from {
                return true;
            }
            if visited.contains(&node) {
                continue;
            }
            visited.insert(node.clone());

            if let Some(successors) = self.outgoing.get(&node) {
                for successor in successors {
                    queue.push_back(successor.clone());
                }
            }
        }

        false
    }

    /// Remove node.
    pub fn remove_node(&mut self, node: &N) -> Option<D> {
        let data = self.nodes.remove(node)?;

        // Remove outgoing edges
        if let Some(successors) = self.outgoing.remove(node) {
            for successor in successors {
                if let Some(incoming) = self.incoming.get_mut(&successor) {
                    incoming.remove(node);
                }
            }
        }

        // Remove incoming edges
        if let Some(predecessors) = self.incoming.remove(node) {
            for predecessor in predecessors {
                if let Some(outgoing) = self.outgoing.get_mut(&predecessor) {
                    outgoing.remove(node);
                }
            }
        }

        Some(data)
    }

    /// Remove edge.
    pub fn remove_edge(&mut self, from: &N, to: &N) -> bool {
        let removed = self
            .outgoing
            .get_mut(from)
            .map(|s| s.remove(to))
            .unwrap_or(false);

        if removed {
            self.incoming.get_mut(to).map(|s| s.remove(from));
        }

        removed
    }

    /// Get node data.
    pub fn get(&self, node: &N) -> Option<&D> {
        self.nodes.get(node)
    }

    /// Get mutable node data.
    pub fn get_mut(&mut self, node: &N) -> Option<&mut D> {
        self.nodes.get_mut(node)
    }

    /// Check if node exists.
    pub fn has_node(&self, node: &N) -> bool {
        self.nodes.contains_key(node)
    }

    /// Check if edge exists.
    pub fn has_edge(&self, from: &N, to: &N) -> bool {
        self.outgoing
            .get(from)
            .map(|s| s.contains(to))
            .unwrap_or(false)
    }

    /// Get successors (direct dependencies).
    pub fn successors(&self, node: &N) -> impl Iterator<Item = &N> {
        self.outgoing.get(node).into_iter().flat_map(|s| s.iter())
    }

    /// Get predecessors (direct dependents).
    pub fn predecessors(&self, node: &N) -> impl Iterator<Item = &N> {
        self.incoming.get(node).into_iter().flat_map(|s| s.iter())
    }

    /// Get all dependencies (transitive).
    pub fn all_successors(&self, node: &N) -> HashSet<N> {
        let mut result = HashSet::new();
        let mut queue = VecDeque::new();

        if let Some(direct) = self.outgoing.get(node) {
            for s in direct {
                queue.push_back(s.clone());
            }
        }

        while let Some(n) = queue.pop_front() {
            if result.contains(&n) {
                continue;
            }
            result.insert(n.clone());

            if let Some(successors) = self.outgoing.get(&n) {
                for s in successors {
                    queue.push_back(s.clone());
                }
            }
        }

        result
    }

    /// Get all dependents (transitive).
    pub fn all_predecessors(&self, node: &N) -> HashSet<N> {
        let mut result = HashSet::new();
        let mut queue = VecDeque::new();

        if let Some(direct) = self.incoming.get(node) {
            for p in direct {
                queue.push_back(p.clone());
            }
        }

        while let Some(n) = queue.pop_front() {
            if result.contains(&n) {
                continue;
            }
            result.insert(n.clone());

            if let Some(predecessors) = self.incoming.get(&n) {
                for p in predecessors {
                    queue.push_back(p.clone());
                }
            }
        }

        result
    }

    /// Get root nodes (no predecessors).
    pub fn roots(&self) -> impl Iterator<Item = &N> {
        self.incoming
            .iter()
            .filter(|(_, preds)| preds.is_empty())
            .map(|(node, _)| node)
    }

    /// Get leaf nodes (no successors).
    pub fn leaves(&self) -> impl Iterator<Item = &N> {
        self.outgoing
            .iter()
            .filter(|(_, succs)| succs.is_empty())
            .map(|(node, _)| node)
    }

    /// Topological sort.
    pub fn topological_order(&self) -> Vec<N> {
        let mut result = Vec::new();
        let mut in_degree: HashMap<N, usize> = self
            .nodes
            .keys()
            .map(|n| {
                (
                    n.clone(),
                    self.incoming.get(n).map(|s| s.len()).unwrap_or(0),
                )
            })
            .collect();

        let mut queue: VecDeque<N> = in_degree
            .iter()
            .filter(|(_, &deg)| deg == 0)
            .map(|(n, _)| n.clone())
            .collect();

        while let Some(node) = queue.pop_front() {
            result.push(node.clone());

            if let Some(successors) = self.outgoing.get(&node) {
                for successor in successors {
                    if let Some(deg) = in_degree.get_mut(successor) {
                        *deg -= 1;
                        if *deg == 0 {
                            queue.push_back(successor.clone());
                        }
                    }
                }
            }
        }

        result
    }

    /// Get execution levels (nodes that can be processed in parallel).
    pub fn levels(&self) -> Vec<Vec<N>> {
        let mut result = Vec::new();
        let mut remaining: HashSet<N> = self.nodes.keys().cloned().collect();
        let mut processed = HashSet::new();

        while !remaining.is_empty() {
            // Find nodes whose predecessors are all processed
            let level: Vec<N> = remaining
                .iter()
                .filter(|n| {
                    self.incoming
                        .get(*n)
                        .map(|preds| preds.iter().all(|p| processed.contains(p)))
                        .unwrap_or(true)
                })
                .cloned()
                .collect();

            for node in &level {
                remaining.remove(node);
                processed.insert(node.clone());
            }

            if !level.is_empty() {
                result.push(level);
            } else {
                break; // Should not happen in valid DAG
            }
        }

        result
    }

    /// Get node count.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Get edge count.
    pub fn edge_count(&self) -> usize {
        self.outgoing.values().map(|s| s.len()).sum()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Clear DAG.
    pub fn clear(&mut self) {
        self.nodes.clear();
        self.outgoing.clear();
        self.incoming.clear();
    }

    /// Iterate over nodes.
    pub fn nodes(&self) -> impl Iterator<Item = (&N, &D)> {
        self.nodes.iter()
    }
}

impl<N: Hash + Eq + Clone, D> Default for Dag<N, D> {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Kani Formal Verification Proofs
// ============================================================================

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    // ========================================================================
    // DAG Basic Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_dag_empty_initially() {
        let dag: Dag<i32, ()> = Dag::new();

        kani::assert!(dag.is_empty(), "New DAG is empty");
        kani::assert!(dag.node_count() == 0, "New DAG has 0 nodes");
        kani::assert!(dag.edge_count() == 0, "New DAG has 0 edges");
    }

    #[kani::proof]
    fn proof_dag_add_node_increases_count() {
        let mut dag: Dag<i32, ()> = Dag::new();

        let added = dag.add_node(1, ());

        kani::assert!(added, "First add returns true");
        kani::assert!(dag.node_count() == 1, "Node count is 1");
        kani::assert!(dag.has_node(&1), "Node exists");
    }

    #[kani::proof]
    fn proof_dag_add_duplicate_node_fails() {
        let mut dag: Dag<i32, ()> = Dag::new();

        dag.add_node(1, ());
        let added = dag.add_node(1, ());

        kani::assert!(!added, "Duplicate add returns false");
        kani::assert!(dag.node_count() == 1, "Node count still 1");
    }

    #[kani::proof]
    fn proof_dag_add_edge_requires_nodes() {
        let mut dag: Dag<i32, ()> = Dag::new();

        let result = dag.add_edge(1, 2);

        kani::assert!(result.is_err(), "Edge without nodes fails");
    }

    #[kani::proof]
    fn proof_dag_add_edge_success() {
        let mut dag: Dag<i32, ()> = Dag::new();

        dag.add_node(1, ());
        dag.add_node(2, ());
        let result = dag.add_edge(1, 2);

        kani::assert!(result.is_ok(), "Edge with nodes succeeds");
        kani::assert!(dag.has_edge(&1, &2), "Edge exists");
        kani::assert!(dag.edge_count() == 1, "Edge count is 1");
    }

    #[kani::proof]
    fn proof_dag_edge_bidirectional_tracking() {
        let mut dag: Dag<i32, ()> = Dag::new();

        dag.add_node(1, ());
        dag.add_node(2, ());
        dag.add_edge(1, 2).unwrap();

        // 1 is predecessor of 2
        let preds: Vec<_> = dag.predecessors(&2).collect();
        kani::assert!(preds.contains(&&1), "1 is predecessor of 2");

        // 2 is successor of 1
        let succs: Vec<_> = dag.successors(&1).collect();
        kani::assert!(succs.contains(&&2), "2 is successor of 1");
    }

    // ========================================================================
    // DAG Cycle Prevention Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_dag_self_loop_prevented() {
        let mut dag: Dag<i32, ()> = Dag::new();

        dag.add_node(1, ());
        let result = dag.add_edge(1, 1);

        kani::assert!(result.is_err(), "Self-loop prevented");
    }

    #[kani::proof]
    fn proof_dag_direct_cycle_prevented() {
        let mut dag: Dag<i32, ()> = Dag::new();

        dag.add_node(1, ());
        dag.add_node(2, ());
        dag.add_edge(1, 2).unwrap();

        let result = dag.add_edge(2, 1);

        kani::assert!(result.is_err(), "Direct cycle prevented");
    }

    #[kani::proof]
    fn proof_dag_indirect_cycle_prevented() {
        let mut dag: Dag<i32, ()> = Dag::new();

        dag.add_node(1, ());
        dag.add_node(2, ());
        dag.add_node(3, ());
        dag.add_edge(1, 2).unwrap();
        dag.add_edge(2, 3).unwrap();

        let result = dag.add_edge(3, 1);

        kani::assert!(result.is_err(), "Indirect cycle (3->1) prevented");
    }

    // ========================================================================
    // DAG Remove Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_dag_remove_node_decreases_count() {
        let mut dag: Dag<i32, i32> = Dag::new();

        dag.add_node(1, 42);
        let removed = dag.remove_node(&1);

        kani::assert!(removed == Some(42), "Remove returns data");
        kani::assert!(dag.node_count() == 0, "Node count is 0");
        kani::assert!(!dag.has_node(&1), "Node no longer exists");
    }

    #[kani::proof]
    fn proof_dag_remove_node_removes_edges() {
        let mut dag: Dag<i32, ()> = Dag::new();

        dag.add_node(1, ());
        dag.add_node(2, ());
        dag.add_edge(1, 2).unwrap();

        dag.remove_node(&1);

        kani::assert!(!dag.has_edge(&1, &2), "Edge removed with node");
        kani::assert!(dag.edge_count() == 0, "Edge count is 0");
    }

    #[kani::proof]
    fn proof_dag_remove_edge() {
        let mut dag: Dag<i32, ()> = Dag::new();

        dag.add_node(1, ());
        dag.add_node(2, ());
        dag.add_edge(1, 2).unwrap();

        let removed = dag.remove_edge(&1, &2);

        kani::assert!(removed, "Remove edge returns true");
        kani::assert!(!dag.has_edge(&1, &2), "Edge no longer exists");
        kani::assert!(dag.node_count() == 2, "Nodes still exist");
    }

    #[kani::proof]
    fn proof_dag_remove_nonexistent() {
        let mut dag: Dag<i32, ()> = Dag::new();

        let removed_node = dag.remove_node(&1);
        let removed_edge = dag.remove_edge(&1, &2);

        kani::assert!(
            removed_node.is_none(),
            "Remove nonexistent node returns None"
        );
        kani::assert!(!removed_edge, "Remove nonexistent edge returns false");
    }

    // ========================================================================
    // DAG Roots and Leaves Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_dag_single_node_is_root_and_leaf() {
        let mut dag: Dag<i32, ()> = Dag::new();

        dag.add_node(1, ());

        let roots: Vec<_> = dag.roots().collect();
        let leaves: Vec<_> = dag.leaves().collect();

        kani::assert!(roots.contains(&&1), "Single node is root");
        kani::assert!(leaves.contains(&&1), "Single node is leaf");
    }

    #[kani::proof]
    fn proof_dag_roots_have_no_predecessors() {
        let mut dag: Dag<i32, ()> = Dag::new();

        dag.add_node(1, ());
        dag.add_node(2, ());
        dag.add_edge(1, 2).unwrap();

        let roots: Vec<_> = dag.roots().collect();

        kani::assert!(roots.contains(&&1), "1 is root (no predecessors)");
        kani::assert!(!roots.contains(&&2), "2 is not root (has predecessor)");
    }

    #[kani::proof]
    fn proof_dag_leaves_have_no_successors() {
        let mut dag: Dag<i32, ()> = Dag::new();

        dag.add_node(1, ());
        dag.add_node(2, ());
        dag.add_edge(1, 2).unwrap();

        let leaves: Vec<_> = dag.leaves().collect();

        kani::assert!(!leaves.contains(&&1), "1 is not leaf (has successor)");
        kani::assert!(leaves.contains(&&2), "2 is leaf (no successors)");
    }

    // ========================================================================
    // DAG Clear Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_dag_clear() {
        let mut dag: Dag<i32, ()> = Dag::new();

        dag.add_node(1, ());
        dag.add_node(2, ());
        dag.add_edge(1, 2).unwrap();

        dag.clear();

        kani::assert!(dag.is_empty(), "DAG empty after clear");
        kani::assert!(dag.node_count() == 0, "Node count is 0");
        kani::assert!(dag.edge_count() == 0, "Edge count is 0");
    }

    // ========================================================================
    // DAG Count Consistency Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_dag_node_count_consistency() {
        let mut dag: Dag<i32, ()> = Dag::new();

        dag.add_node(1, ());
        dag.add_node(2, ());
        dag.add_node(3, ());

        kani::assert!(dag.node_count() == 3, "Node count matches inserts");

        dag.remove_node(&2);

        kani::assert!(dag.node_count() == 2, "Node count after remove");
    }

    #[kani::proof]
    fn proof_dag_edge_count_consistency() {
        let mut dag: Dag<i32, ()> = Dag::new();

        dag.add_node(1, ());
        dag.add_node(2, ());
        dag.add_node(3, ());

        dag.add_edge(1, 2).unwrap();
        dag.add_edge(2, 3).unwrap();

        kani::assert!(dag.edge_count() == 2, "Edge count matches adds");

        dag.remove_edge(&1, &2);

        kani::assert!(dag.edge_count() == 1, "Edge count after remove");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_operations() {
        let mut dag: Dag<&str, i32> = Dag::new();
        dag.add_node("a", 1);
        dag.add_node("b", 2);
        dag.add_node("c", 3);

        assert!(dag.add_edge("a", "b").is_ok());
        assert!(dag.add_edge("b", "c").is_ok());
        assert!(dag.has_edge(&"a", &"b"));
    }

    #[test]
    fn test_cycle_prevention() {
        let mut dag: Dag<&str, ()> = Dag::new();
        dag.add_node("a", ());
        dag.add_node("b", ());
        dag.add_node("c", ());

        dag.add_edge("a", "b").unwrap();
        dag.add_edge("b", "c").unwrap();

        // This would create a cycle
        let result = dag.add_edge("c", "a");
        assert!(matches!(result, Err(DagError::WouldCreateCycle)));
    }

    #[test]
    fn test_topological_order() {
        let mut dag: Dag<&str, ()> = Dag::new();
        dag.add_node("a", ());
        dag.add_node("b", ());
        dag.add_node("c", ());
        dag.add_node("d", ());

        dag.add_edge("a", "b").unwrap();
        dag.add_edge("a", "c").unwrap();
        dag.add_edge("b", "d").unwrap();
        dag.add_edge("c", "d").unwrap();

        let order = dag.topological_order();
        let a_idx = order.iter().position(|n| *n == "a").unwrap();
        let d_idx = order.iter().position(|n| *n == "d").unwrap();
        assert!(a_idx < d_idx);
    }

    #[test]
    fn test_levels() {
        let mut dag: Dag<&str, ()> = Dag::new();
        dag.add_node("a", ());
        dag.add_node("b", ());
        dag.add_node("c", ());
        dag.add_node("d", ());

        dag.add_edge("a", "c").unwrap();
        dag.add_edge("b", "c").unwrap();
        dag.add_edge("c", "d").unwrap();

        let levels = dag.levels();
        assert_eq!(levels.len(), 3);
        assert!(levels[0].contains(&"a") || levels[0].contains(&"b"));
    }

    #[test]
    fn test_transitive() {
        let mut dag: Dag<i32, ()> = Dag::new();
        dag.add_node(1, ());
        dag.add_node(2, ());
        dag.add_node(3, ());
        dag.add_node(4, ());

        dag.add_edge(1, 2).unwrap();
        dag.add_edge(2, 3).unwrap();
        dag.add_edge(3, 4).unwrap();

        let all = dag.all_successors(&1);
        assert!(all.contains(&2));
        assert!(all.contains(&3));
        assert!(all.contains(&4));
    }
}
