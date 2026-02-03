//! Binary tree for drbot.
//!
//! This crate provides:
//! - Binary search tree
//! - In-order, pre-order, post-order traversal
//! - Tree balancing info

use std::cmp::Ordering;
use thiserror::Error;

/// Binary tree error types.
#[derive(Error, Debug)]
pub enum BTreeError {
    #[error("Key not found")]
    KeyNotFound,

    #[error("Duplicate key")]
    DuplicateKey,
}

/// Result type for btree operations.
pub type Result<T> = std::result::Result<T, BTreeError>;

/// Binary tree node.
#[derive(Debug, Clone)]
pub struct BTreeNode<K, V> {
    pub key: K,
    pub value: V,
    pub left: Option<Box<BTreeNode<K, V>>>,
    pub right: Option<Box<BTreeNode<K, V>>>,
}

impl<K, V> BTreeNode<K, V> {
    /// Create new node.
    pub fn new(key: K, value: V) -> Self {
        Self {
            key,
            value,
            left: None,
            right: None,
        }
    }

    /// Check if leaf.
    pub fn is_leaf(&self) -> bool {
        self.left.is_none() && self.right.is_none()
    }

    /// Get height.
    pub fn height(&self) -> usize {
        let left_height = self.left.as_ref().map(|n| n.height()).unwrap_or(0);
        let right_height = self.right.as_ref().map(|n| n.height()).unwrap_or(0);
        1 + left_height.max(right_height)
    }

    /// Get count.
    pub fn count(&self) -> usize {
        let left_count = self.left.as_ref().map(|n| n.count()).unwrap_or(0);
        let right_count = self.right.as_ref().map(|n| n.count()).unwrap_or(0);
        1 + left_count + right_count
    }

    /// Get balance factor.
    pub fn balance_factor(&self) -> i32 {
        let left_height = self.left.as_ref().map(|n| n.height() as i32).unwrap_or(0);
        let right_height = self.right.as_ref().map(|n| n.height() as i32).unwrap_or(0);
        left_height - right_height
    }
}

/// Binary search tree.
#[derive(Debug, Clone)]
pub struct BinarySearchTree<K, V> {
    root: Option<Box<BTreeNode<K, V>>>,
    size: usize,
}

impl<K: Ord, V> BinarySearchTree<K, V> {
    /// Create empty tree.
    pub fn new() -> Self {
        Self {
            root: None,
            size: 0,
        }
    }

    /// Insert key-value pair.
    pub fn insert(&mut self, key: K, value: V) {
        self.root = Self::insert_recursive(self.root.take(), key, value);
        self.size += 1;
    }

    fn insert_recursive(
        node: Option<Box<BTreeNode<K, V>>>,
        key: K,
        value: V,
    ) -> Option<Box<BTreeNode<K, V>>> {
        match node {
            None => Some(Box::new(BTreeNode::new(key, value))),
            Some(mut n) => {
                match key.cmp(&n.key) {
                    Ordering::Less => {
                        n.left = Self::insert_recursive(n.left.take(), key, value);
                    }
                    Ordering::Greater => {
                        n.right = Self::insert_recursive(n.right.take(), key, value);
                    }
                    Ordering::Equal => {
                        n.value = value; // Update existing
                    }
                }
                Some(n)
            }
        }
    }

    /// Get value by key.
    pub fn get(&self, key: &K) -> Option<&V> {
        Self::get_recursive(self.root.as_ref(), key)
    }

    fn get_recursive<'a>(node: Option<&'a Box<BTreeNode<K, V>>>, key: &K) -> Option<&'a V> {
        match node {
            None => None,
            Some(n) => match key.cmp(&n.key) {
                Ordering::Less => Self::get_recursive(n.left.as_ref(), key),
                Ordering::Greater => Self::get_recursive(n.right.as_ref(), key),
                Ordering::Equal => Some(&n.value),
            },
        }
    }

    /// Check if key exists.
    pub fn contains(&self, key: &K) -> bool {
        self.get(key).is_some()
    }

    /// Get minimum key-value pair.
    pub fn min(&self) -> Option<(&K, &V)> {
        Self::min_recursive(self.root.as_ref())
    }

    fn min_recursive(node: Option<&Box<BTreeNode<K, V>>>) -> Option<(&K, &V)> {
        match node {
            None => None,
            Some(n) => {
                if n.left.is_some() {
                    Self::min_recursive(n.left.as_ref())
                } else {
                    Some((&n.key, &n.value))
                }
            }
        }
    }

    /// Get maximum key-value pair.
    pub fn max(&self) -> Option<(&K, &V)> {
        Self::max_recursive(self.root.as_ref())
    }

    fn max_recursive(node: Option<&Box<BTreeNode<K, V>>>) -> Option<(&K, &V)> {
        match node {
            None => None,
            Some(n) => {
                if n.right.is_some() {
                    Self::max_recursive(n.right.as_ref())
                } else {
                    Some((&n.key, &n.value))
                }
            }
        }
    }

    /// In-order traversal (sorted).
    pub fn inorder(&self) -> Vec<(&K, &V)> {
        let mut result = Vec::new();
        Self::inorder_recursive(self.root.as_ref(), &mut result);
        result
    }

    fn inorder_recursive<'a>(
        node: Option<&'a Box<BTreeNode<K, V>>>,
        result: &mut Vec<(&'a K, &'a V)>,
    ) {
        if let Some(n) = node {
            Self::inorder_recursive(n.left.as_ref(), result);
            result.push((&n.key, &n.value));
            Self::inorder_recursive(n.right.as_ref(), result);
        }
    }

    /// Pre-order traversal.
    pub fn preorder(&self) -> Vec<(&K, &V)> {
        let mut result = Vec::new();
        Self::preorder_recursive(self.root.as_ref(), &mut result);
        result
    }

    fn preorder_recursive<'a>(
        node: Option<&'a Box<BTreeNode<K, V>>>,
        result: &mut Vec<(&'a K, &'a V)>,
    ) {
        if let Some(n) = node {
            result.push((&n.key, &n.value));
            Self::preorder_recursive(n.left.as_ref(), result);
            Self::preorder_recursive(n.right.as_ref(), result);
        }
    }

    /// Post-order traversal.
    pub fn postorder(&self) -> Vec<(&K, &V)> {
        let mut result = Vec::new();
        Self::postorder_recursive(self.root.as_ref(), &mut result);
        result
    }

    fn postorder_recursive<'a>(
        node: Option<&'a Box<BTreeNode<K, V>>>,
        result: &mut Vec<(&'a K, &'a V)>,
    ) {
        if let Some(n) = node {
            Self::postorder_recursive(n.left.as_ref(), result);
            Self::postorder_recursive(n.right.as_ref(), result);
            result.push((&n.key, &n.value));
        }
    }

    /// Get size.
    pub fn len(&self) -> usize {
        self.size
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.root.is_none()
    }

    /// Get height.
    pub fn height(&self) -> usize {
        self.root.as_ref().map(|n| n.height()).unwrap_or(0)
    }

    /// Clear tree.
    pub fn clear(&mut self) {
        self.root = None;
        self.size = 0;
    }
}

impl<K: Ord, V> Default for BinarySearchTree<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Ord, V> FromIterator<(K, V)> for BinarySearchTree<K, V> {
    fn from_iter<I: IntoIterator<Item = (K, V)>>(iter: I) -> Self {
        let mut tree = BinarySearchTree::new();
        for (k, v) in iter {
            tree.insert(k, v);
        }
        tree
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_and_get() {
        let mut tree = BinarySearchTree::new();
        tree.insert(5, "five");
        tree.insert(3, "three");
        tree.insert(7, "seven");

        assert_eq!(tree.get(&5), Some(&"five"));
        assert_eq!(tree.get(&3), Some(&"three"));
        assert_eq!(tree.get(&7), Some(&"seven"));
        assert_eq!(tree.get(&1), None);
    }

    #[test]
    fn test_min_max() {
        let mut tree = BinarySearchTree::new();
        tree.insert(5, "five");
        tree.insert(3, "three");
        tree.insert(7, "seven");

        assert_eq!(tree.min(), Some((&3, &"three")));
        assert_eq!(tree.max(), Some((&7, &"seven")));
    }

    #[test]
    fn test_inorder() {
        let mut tree = BinarySearchTree::new();
        tree.insert(5, 5);
        tree.insert(3, 3);
        tree.insert(7, 7);
        tree.insert(1, 1);
        tree.insert(9, 9);

        let sorted: Vec<i32> = tree.inorder().into_iter().map(|(k, _)| *k).collect();
        assert_eq!(sorted, vec![1, 3, 5, 7, 9]);
    }

    #[test]
    fn test_height() {
        let mut tree = BinarySearchTree::new();
        assert_eq!(tree.height(), 0);

        tree.insert(5, 5);
        assert_eq!(tree.height(), 1);

        tree.insert(3, 3);
        tree.insert(7, 7);
        assert_eq!(tree.height(), 2);
    }
}
