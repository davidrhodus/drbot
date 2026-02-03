//! Trie data structure for drbot.
//!
//! This crate provides:
//! - Prefix tree (trie)
//! - Auto-complete functionality
//! - Prefix matching
//! - Word counting

use std::collections::HashMap;
use thiserror::Error;

/// Trie error types.
#[derive(Error, Debug)]
pub enum TrieError {
    #[error("Key not found")]
    NotFound,

    #[error("Empty key")]
    EmptyKey,
}

/// Result type for trie operations.
pub type Result<T> = std::result::Result<T, TrieError>;

/// Trie node.
#[derive(Debug, Clone)]
struct TrieNode<V> {
    children: HashMap<char, TrieNode<V>>,
    value: Option<V>,
    is_end: bool,
}

impl<V> TrieNode<V> {
    fn new() -> Self {
        Self {
            children: HashMap::new(),
            value: None,
            is_end: false,
        }
    }
}

impl<V> Default for TrieNode<V> {
    fn default() -> Self {
        Self::new()
    }
}

/// Trie (prefix tree) implementation.
pub struct Trie<V> {
    root: TrieNode<V>,
    size: usize,
}

impl<V> Trie<V> {
    /// Create new empty trie.
    pub fn new() -> Self {
        Self {
            root: TrieNode::new(),
            size: 0,
        }
    }

    /// Get number of entries.
    pub fn len(&self) -> usize {
        self.size
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    /// Insert key-value pair.
    pub fn insert(&mut self, key: &str, value: V) -> Option<V> {
        let mut current = &mut self.root;

        for ch in key.chars() {
            current = current.children.entry(ch).or_insert_with(TrieNode::new);
        }

        let old = current.value.take();
        current.value = Some(value);

        if !current.is_end {
            current.is_end = true;
            self.size += 1;
        }

        old
    }

    /// Get value for key.
    pub fn get(&self, key: &str) -> Option<&V> {
        let mut current = &self.root;

        for ch in key.chars() {
            current = current.children.get(&ch)?;
        }

        if current.is_end {
            current.value.as_ref()
        } else {
            None
        }
    }

    /// Get mutable value for key.
    pub fn get_mut(&mut self, key: &str) -> Option<&mut V> {
        let mut current = &mut self.root;

        for ch in key.chars() {
            current = current.children.get_mut(&ch)?;
        }

        if current.is_end {
            current.value.as_mut()
        } else {
            None
        }
    }

    /// Check if key exists.
    pub fn contains(&self, key: &str) -> bool {
        self.get(key).is_some()
    }

    /// Check if any key starts with prefix.
    pub fn has_prefix(&self, prefix: &str) -> bool {
        self.find_node(prefix).is_some()
    }

    /// Remove key.
    pub fn remove(&mut self, key: &str) -> Option<V> {
        let chars: Vec<char> = key.chars().collect();
        let (result, should_decrement) = Self::remove_recursive_helper(&mut self.root, &chars, 0);
        if should_decrement {
            self.size -= 1;
        }
        result
    }

    fn remove_recursive_helper(
        node: &mut TrieNode<V>,
        chars: &[char],
        depth: usize,
    ) -> (Option<V>, bool) {
        if depth == chars.len() {
            if node.is_end {
                node.is_end = false;
                return (node.value.take(), true);
            }
            return (None, false);
        }

        let ch = chars[depth];
        if let Some(child) = node.children.get_mut(&ch) {
            let (result, decremented) = Self::remove_recursive_helper(child, chars, depth + 1);

            // Remove child if empty
            if !child.is_end && child.children.is_empty() {
                node.children.remove(&ch);
            }

            return (result, decremented);
        }

        (None, false)
    }

    /// Find all keys with given prefix.
    pub fn keys_with_prefix(&self, prefix: &str) -> Vec<String> {
        let mut results = Vec::new();

        if let Some(node) = self.find_node(prefix) {
            self.collect_keys(node, &mut prefix.to_string(), &mut results);
        }

        results
    }

    /// Get autocomplete suggestions.
    pub fn autocomplete(&self, prefix: &str, limit: usize) -> Vec<String> {
        self.keys_with_prefix(prefix)
            .into_iter()
            .take(limit)
            .collect()
    }

    fn find_node(&self, prefix: &str) -> Option<&TrieNode<V>> {
        let mut current = &self.root;

        for ch in prefix.chars() {
            current = current.children.get(&ch)?;
        }

        Some(current)
    }

    fn collect_keys(&self, node: &TrieNode<V>, current: &mut String, results: &mut Vec<String>) {
        if node.is_end {
            results.push(current.clone());
        }

        for (&ch, child) in &node.children {
            current.push(ch);
            self.collect_keys(child, current, results);
            current.pop();
        }
    }

    /// Get all keys.
    pub fn keys(&self) -> Vec<String> {
        let mut results = Vec::new();
        self.collect_keys(&self.root, &mut String::new(), &mut results);
        results
    }

    /// Clear the trie.
    pub fn clear(&mut self) {
        self.root = TrieNode::new();
        self.size = 0;
    }
}

impl<V> Default for Trie<V> {
    fn default() -> Self {
        Self::new()
    }
}

/// String-only trie for simpler usage.
pub struct StringTrie {
    inner: Trie<()>,
}

impl StringTrie {
    /// Create new string trie.
    pub fn new() -> Self {
        Self { inner: Trie::new() }
    }

    /// Insert string.
    pub fn insert(&mut self, s: &str) {
        self.inner.insert(s, ());
    }

    /// Check if string exists.
    pub fn contains(&self, s: &str) -> bool {
        self.inner.contains(s)
    }

    /// Check if prefix exists.
    pub fn has_prefix(&self, prefix: &str) -> bool {
        self.inner.has_prefix(prefix)
    }

    /// Remove string.
    pub fn remove(&mut self, s: &str) -> bool {
        self.inner.remove(s).is_some()
    }

    /// Get autocomplete suggestions.
    pub fn autocomplete(&self, prefix: &str, limit: usize) -> Vec<String> {
        self.inner.autocomplete(prefix, limit)
    }

    /// Get all strings with prefix.
    pub fn with_prefix(&self, prefix: &str) -> Vec<String> {
        self.inner.keys_with_prefix(prefix)
    }

    /// Get number of entries.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

impl Default for StringTrie {
    fn default() -> Self {
        Self::new()
    }
}

/// Counting trie that tracks frequency.
pub struct CountingTrie {
    inner: Trie<usize>,
}

impl CountingTrie {
    /// Create new counting trie.
    pub fn new() -> Self {
        Self { inner: Trie::new() }
    }

    /// Add word (increment count).
    pub fn add(&mut self, word: &str) {
        if let Some(count) = self.inner.get_mut(word) {
            *count += 1;
        } else {
            self.inner.insert(word, 1);
        }
    }

    /// Get count for word.
    pub fn count(&self, word: &str) -> usize {
        self.inner.get(word).copied().unwrap_or(0)
    }

    /// Get top N words with prefix.
    pub fn top_with_prefix(&self, prefix: &str, n: usize) -> Vec<(String, usize)> {
        let mut results: Vec<_> = self
            .inner
            .keys_with_prefix(prefix)
            .into_iter()
            .filter_map(|k| self.inner.get(&k).map(|&c| (k, c)))
            .collect();

        results.sort_by(|a, b| b.1.cmp(&a.1));
        results.truncate(n);
        results
    }

    /// Get number of unique words.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

impl Default for CountingTrie {
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
    // Trie Basic Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_trie_empty_initially() {
        let trie: Trie<i32> = Trie::new();

        kani::assert!(trie.is_empty(), "New trie is empty");
        kani::assert!(trie.len() == 0, "New trie has len 0");
    }

    #[kani::proof]
    fn proof_trie_insert_increases_len() {
        let mut trie: Trie<i32> = Trie::new();

        let old = trie.insert("hello", 1);

        kani::assert!(old.is_none(), "First insert returns None");
        kani::assert!(trie.len() == 1, "Len is 1 after insert");
        kani::assert!(!trie.is_empty(), "Trie not empty after insert");
    }

    #[kani::proof]
    fn proof_trie_insert_same_key_returns_old() {
        let mut trie: Trie<i32> = Trie::new();

        trie.insert("key", 1);
        let old = trie.insert("key", 2);

        kani::assert!(old == Some(1), "Second insert returns old value");
        kani::assert!(trie.len() == 1, "Len still 1 after update");
    }

    #[kani::proof]
    fn proof_trie_get_after_insert() {
        let mut trie: Trie<i32> = Trie::new();

        trie.insert("hello", 42);

        kani::assert!(trie.get("hello") == Some(&42), "Get returns inserted value");
    }

    #[kani::proof]
    fn proof_trie_get_nonexistent() {
        let trie: Trie<i32> = Trie::new();

        kani::assert!(
            trie.get("hello").is_none(),
            "Get on empty trie returns None"
        );
    }

    #[kani::proof]
    fn proof_trie_contains_after_insert() {
        let mut trie: Trie<i32> = Trie::new();

        trie.insert("hello", 1);

        kani::assert!(
            trie.contains("hello"),
            "Contains returns true for inserted key"
        );
        kani::assert!(
            !trie.contains("help"),
            "Contains returns false for non-inserted key"
        );
    }

    #[kani::proof]
    fn proof_trie_contains_prefix_but_not_key() {
        let mut trie: Trie<i32> = Trie::new();

        trie.insert("hello", 1);

        // "hel" is a prefix but not a key
        kani::assert!(!trie.contains("hel"), "Prefix alone is not a key");
        kani::assert!(
            trie.has_prefix("hel"),
            "has_prefix returns true for valid prefix"
        );
    }

    #[kani::proof]
    fn proof_trie_has_prefix_after_insert() {
        let mut trie: Trie<i32> = Trie::new();

        trie.insert("hello", 1);

        kani::assert!(trie.has_prefix("h"), "has_prefix 'h'");
        kani::assert!(trie.has_prefix("he"), "has_prefix 'he'");
        kani::assert!(trie.has_prefix("hel"), "has_prefix 'hel'");
        kani::assert!(trie.has_prefix("hell"), "has_prefix 'hell'");
        kani::assert!(trie.has_prefix("hello"), "has_prefix 'hello'");
        kani::assert!(!trie.has_prefix("help"), "no prefix 'help'");
    }

    #[kani::proof]
    fn proof_trie_remove_decreases_len() {
        let mut trie: Trie<i32> = Trie::new();

        trie.insert("hello", 1);
        let removed = trie.remove("hello");

        kani::assert!(removed == Some(1), "Remove returns removed value");
        kani::assert!(trie.len() == 0, "Len is 0 after remove");
        kani::assert!(trie.is_empty(), "Trie empty after remove");
    }

    #[kani::proof]
    fn proof_trie_remove_nonexistent() {
        let mut trie: Trie<i32> = Trie::new();

        let removed = trie.remove("nonexistent");

        kani::assert!(removed.is_none(), "Remove nonexistent returns None");
    }

    #[kani::proof]
    fn proof_trie_remove_preserves_other_keys() {
        let mut trie: Trie<i32> = Trie::new();

        trie.insert("hello", 1);
        trie.insert("help", 2);

        trie.remove("hello");

        kani::assert!(
            trie.get("help") == Some(&2),
            "Other key preserved after remove"
        );
        kani::assert!(trie.len() == 1, "Len is 1 after partial remove");
    }

    #[kani::proof]
    fn proof_trie_clear_resets() {
        let mut trie: Trie<i32> = Trie::new();

        trie.insert("hello", 1);
        trie.insert("world", 2);
        trie.clear();

        kani::assert!(trie.is_empty(), "Trie empty after clear");
        kani::assert!(trie.len() == 0, "Len is 0 after clear");
    }

    #[kani::proof]
    fn proof_trie_empty_key() {
        let mut trie: Trie<i32> = Trie::new();

        trie.insert("", 1);

        kani::assert!(trie.contains(""), "Empty string can be a key");
        kani::assert!(trie.get("") == Some(&1), "Get empty key works");
    }

    // ========================================================================
    // StringTrie Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_string_trie_empty_initially() {
        let trie = StringTrie::new();

        kani::assert!(trie.is_empty(), "New StringTrie is empty");
        kani::assert!(trie.len() == 0, "New StringTrie has len 0");
    }

    #[kani::proof]
    fn proof_string_trie_insert_contains() {
        let mut trie = StringTrie::new();

        trie.insert("hello");

        kani::assert!(trie.contains("hello"), "Contains inserted string");
        kani::assert!(trie.len() == 1, "Len is 1 after insert");
    }

    #[kani::proof]
    fn proof_string_trie_remove() {
        let mut trie = StringTrie::new();

        trie.insert("hello");
        let removed = trie.remove("hello");

        kani::assert!(removed, "Remove returns true");
        kani::assert!(!trie.contains("hello"), "String no longer contained");
    }

    // ========================================================================
    // CountingTrie Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_counting_trie_empty_initially() {
        let trie = CountingTrie::new();

        kani::assert!(trie.is_empty(), "New CountingTrie is empty");
        kani::assert!(trie.count("any") == 0, "Count is 0 for non-existent");
    }

    #[kani::proof]
    fn proof_counting_trie_add_increments() {
        let mut trie = CountingTrie::new();

        trie.add("hello");
        kani::assert!(trie.count("hello") == 1, "Count is 1 after first add");

        trie.add("hello");
        kani::assert!(trie.count("hello") == 2, "Count is 2 after second add");
    }

    #[kani::proof]
    fn proof_counting_trie_len_unique_words() {
        let mut trie = CountingTrie::new();

        trie.add("hello");
        trie.add("hello");
        trie.add("world");

        kani::assert!(trie.len() == 2, "Len counts unique words");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_operations() {
        let mut trie = Trie::new();

        trie.insert("hello", 1);
        trie.insert("help", 2);
        trie.insert("world", 3);

        assert_eq!(trie.get("hello"), Some(&1));
        assert_eq!(trie.get("help"), Some(&2));
        assert_eq!(trie.get("world"), Some(&3));
        assert_eq!(trie.get("he"), None);
    }

    #[test]
    fn test_prefix_search() {
        let mut trie = StringTrie::new();

        trie.insert("hello");
        trie.insert("help");
        trie.insert("helper");
        trie.insert("world");

        let results = trie.with_prefix("hel");
        assert_eq!(results.len(), 3);
        assert!(results.contains(&"hello".to_string()));
        assert!(results.contains(&"help".to_string()));
        assert!(results.contains(&"helper".to_string()));
    }

    #[test]
    fn test_autocomplete() {
        let mut trie = StringTrie::new();

        trie.insert("apple");
        trie.insert("application");
        trie.insert("apply");
        trie.insert("banana");

        let suggestions = trie.autocomplete("app", 2);
        assert_eq!(suggestions.len(), 2);
    }

    #[test]
    fn test_remove() {
        let mut trie = Trie::new();

        trie.insert("hello", 1);
        trie.insert("help", 2);

        assert_eq!(trie.remove("hello"), Some(1));
        assert_eq!(trie.get("hello"), None);
        assert_eq!(trie.get("help"), Some(&2));
    }

    #[test]
    fn test_counting_trie() {
        let mut trie = CountingTrie::new();

        trie.add("hello");
        trie.add("hello");
        trie.add("world");

        assert_eq!(trie.count("hello"), 2);
        assert_eq!(trie.count("world"), 1);
        assert_eq!(trie.count("foo"), 0);
    }

    #[test]
    fn test_has_prefix() {
        let mut trie = StringTrie::new();

        trie.insert("hello");

        assert!(trie.has_prefix("hel"));
        assert!(trie.has_prefix("hello"));
        assert!(!trie.has_prefix("help"));
    }
}
