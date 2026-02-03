//! Generic tree data structure for drbot.
//!
//! This crate provides:
//! - Generic tree with arbitrary children
//! - Tree traversal (DFS, BFS)
//! - Path operations
//! - Tree transformations

use std::collections::VecDeque;
use thiserror::Error;

/// Tree error types.
#[derive(Error, Debug)]
pub enum TreeError {
    #[error("Node not found")]
    NodeNotFound,

    #[error("Invalid path")]
    InvalidPath,

    #[error("Cycle detected")]
    CycleDetected,
}

/// Result type for tree operations.
pub type Result<T> = std::result::Result<T, TreeError>;

/// Tree node with value and children.
#[derive(Debug, Clone)]
pub struct TreeNode<T> {
    /// Node value.
    pub value: T,
    /// Child nodes.
    pub children: Vec<TreeNode<T>>,
}

impl<T> TreeNode<T> {
    /// Create new leaf node.
    pub fn new(value: T) -> Self {
        Self {
            value,
            children: Vec::new(),
        }
    }

    /// Create node with children.
    pub fn with_children(value: T, children: Vec<TreeNode<T>>) -> Self {
        Self { value, children }
    }

    /// Add child node.
    pub fn add_child(&mut self, child: TreeNode<T>) {
        self.children.push(child);
    }

    /// Check if leaf (no children).
    pub fn is_leaf(&self) -> bool {
        self.children.is_empty()
    }

    /// Get number of children.
    pub fn child_count(&self) -> usize {
        self.children.len()
    }

    /// Get total node count (including self).
    pub fn count(&self) -> usize {
        1 + self.children.iter().map(|c| c.count()).sum::<usize>()
    }

    /// Get tree depth.
    pub fn depth(&self) -> usize {
        if self.children.is_empty() {
            1
        } else {
            1 + self.children.iter().map(|c| c.depth()).max().unwrap_or(0)
        }
    }

    /// Get all leaf values.
    pub fn leaves(&self) -> Vec<&T> {
        if self.is_leaf() {
            vec![&self.value]
        } else {
            self.children.iter().flat_map(|c| c.leaves()).collect()
        }
    }

    /// Depth-first traversal (pre-order).
    pub fn dfs_preorder(&self) -> Vec<&T> {
        let mut result = vec![&self.value];
        for child in &self.children {
            result.extend(child.dfs_preorder());
        }
        result
    }

    /// Depth-first traversal (post-order).
    pub fn dfs_postorder(&self) -> Vec<&T> {
        let mut result: Vec<&T> = self
            .children
            .iter()
            .flat_map(|c| c.dfs_postorder())
            .collect();
        result.push(&self.value);
        result
    }

    /// Breadth-first traversal.
    pub fn bfs(&self) -> Vec<&T> {
        let mut result = Vec::new();
        let mut queue = VecDeque::new();
        queue.push_back(self);

        while let Some(node) = queue.pop_front() {
            result.push(&node.value);
            for child in &node.children {
                queue.push_back(child);
            }
        }

        result
    }

    /// Find node by predicate (DFS).
    pub fn find<F>(&self, predicate: F) -> Option<&TreeNode<T>>
    where
        F: Fn(&T) -> bool + Copy,
    {
        if predicate(&self.value) {
            return Some(self);
        }
        for child in &self.children {
            if let Some(found) = child.find(predicate) {
                return Some(found);
            }
        }
        None
    }

    /// Find mutable node by predicate (DFS).
    pub fn find_mut<F>(&mut self, predicate: F) -> Option<&mut TreeNode<T>>
    where
        F: Fn(&T) -> bool + Copy,
    {
        if predicate(&self.value) {
            return Some(self);
        }
        for child in &mut self.children {
            if let Some(found) = child.find_mut(predicate) {
                return Some(found);
            }
        }
        None
    }

    /// Map values to new tree.
    pub fn map<U, F>(&self, f: F) -> TreeNode<U>
    where
        F: Fn(&T) -> U + Copy,
    {
        TreeNode {
            value: f(&self.value),
            children: self.children.iter().map(|c| c.map(f)).collect(),
        }
    }

    /// Filter children (recursive).
    pub fn filter<F>(&self, predicate: F) -> Option<TreeNode<T>>
    where
        T: Clone,
        F: Fn(&T) -> bool + Copy,
    {
        if !predicate(&self.value) {
            return None;
        }

        Some(TreeNode {
            value: self.value.clone(),
            children: self
                .children
                .iter()
                .filter_map(|c| c.filter(predicate))
                .collect(),
        })
    }

    /// Fold tree into single value (post-order).
    pub fn fold<U, F>(&self, init: U, f: F) -> U
    where
        F: Fn(U, &T, Vec<U>) -> U + Copy,
        U: Clone,
    {
        let child_results: Vec<U> = self
            .children
            .iter()
            .map(|c| c.fold(init.clone(), f))
            .collect();
        f(init, &self.value, child_results)
    }

    /// Get path to node (indices).
    pub fn path_to<F>(&self, predicate: F) -> Option<Vec<usize>>
    where
        F: Fn(&T) -> bool + Copy,
    {
        if predicate(&self.value) {
            return Some(vec![]);
        }

        for (i, child) in self.children.iter().enumerate() {
            if let Some(mut path) = child.path_to(predicate) {
                path.insert(0, i);
                return Some(path);
            }
        }

        None
    }

    /// Get node at path.
    pub fn at_path(&self, path: &[usize]) -> Option<&TreeNode<T>> {
        if path.is_empty() {
            return Some(self);
        }

        let idx = path[0];
        if idx < self.children.len() {
            self.children[idx].at_path(&path[1..])
        } else {
            None
        }
    }
}

impl<T: Default> Default for TreeNode<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

/// Tree wrapper with additional operations.
#[derive(Debug, Clone)]
pub struct Tree<T> {
    root: Option<TreeNode<T>>,
}

impl<T> Tree<T> {
    /// Create empty tree.
    pub fn new() -> Self {
        Self { root: None }
    }

    /// Create tree with root.
    pub fn with_root(root: TreeNode<T>) -> Self {
        Self { root: Some(root) }
    }

    /// Get root reference.
    pub fn root(&self) -> Option<&TreeNode<T>> {
        self.root.as_ref()
    }

    /// Get mutable root reference.
    pub fn root_mut(&mut self) -> Option<&mut TreeNode<T>> {
        self.root.as_mut()
    }

    /// Set root.
    pub fn set_root(&mut self, root: TreeNode<T>) {
        self.root = Some(root);
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.root.is_none()
    }

    /// Get total node count.
    pub fn count(&self) -> usize {
        self.root.as_ref().map(|r| r.count()).unwrap_or(0)
    }

    /// Get tree depth.
    pub fn depth(&self) -> usize {
        self.root.as_ref().map(|r| r.depth()).unwrap_or(0)
    }
}

impl<T> Default for Tree<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_tree() -> TreeNode<i32> {
        TreeNode::with_children(
            1,
            vec![
                TreeNode::with_children(2, vec![TreeNode::new(4), TreeNode::new(5)]),
                TreeNode::new(3),
            ],
        )
    }

    #[test]
    fn test_count_and_depth() {
        let tree = sample_tree();
        assert_eq!(tree.count(), 5);
        assert_eq!(tree.depth(), 3);
    }

    #[test]
    fn test_dfs_preorder() {
        let tree = sample_tree();
        let values: Vec<i32> = tree.dfs_preorder().into_iter().cloned().collect();
        assert_eq!(values, vec![1, 2, 4, 5, 3]);
    }

    #[test]
    fn test_dfs_postorder() {
        let tree = sample_tree();
        let values: Vec<i32> = tree.dfs_postorder().into_iter().cloned().collect();
        assert_eq!(values, vec![4, 5, 2, 3, 1]);
    }

    #[test]
    fn test_bfs() {
        let tree = sample_tree();
        let values: Vec<i32> = tree.bfs().into_iter().cloned().collect();
        assert_eq!(values, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_find() {
        let tree = sample_tree();
        let node = tree.find(|&v| v == 4);
        assert!(node.is_some());
        assert_eq!(node.unwrap().value, 4);
    }

    #[test]
    fn test_path() {
        let tree = sample_tree();
        let path = tree.path_to(|&v| v == 5);
        assert_eq!(path, Some(vec![0, 1]));

        let node = tree.at_path(&[0, 1]);
        assert_eq!(node.map(|n| n.value), Some(5));
    }

    #[test]
    fn test_map() {
        let tree = sample_tree();
        let doubled = tree.map(|&v| v * 2);
        assert_eq!(doubled.value, 2);
        assert_eq!(doubled.children[0].value, 4);
    }

    #[test]
    fn test_leaves() {
        let tree = sample_tree();
        let leaves: Vec<i32> = tree.leaves().into_iter().cloned().collect();
        assert_eq!(leaves, vec![4, 5, 3]);
    }
}
