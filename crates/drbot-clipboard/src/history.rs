//! Clipboard history tracking.

use crate::content::{ClipboardContent, ContentType};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// A history entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    /// Entry ID.
    pub id: u64,
    /// Content snapshot.
    pub content: ClipboardContent,
    /// Application that set the clipboard.
    pub source_app: Option<String>,
    /// Whether this was pinned.
    pub pinned: bool,
    /// Number of times pasted.
    pub paste_count: u32,
}

impl HistoryEntry {
    /// Create a new history entry.
    pub fn new(id: u64, content: ClipboardContent) -> Self {
        Self {
            id,
            content,
            source_app: None,
            pinned: false,
            paste_count: 0,
        }
    }

    /// Set source application.
    pub fn with_source_app(mut self, app: impl Into<String>) -> Self {
        self.source_app = Some(app.into());
        self
    }

    /// Pin this entry.
    pub fn pin(&mut self) {
        self.pinned = true;
    }

    /// Unpin this entry.
    pub fn unpin(&mut self) {
        self.pinned = false;
    }

    /// Increment paste count.
    pub fn record_paste(&mut self) {
        self.paste_count += 1;
    }
}

/// Clipboard history manager.
#[derive(Debug)]
pub struct ClipboardHistory {
    /// History entries.
    entries: VecDeque<HistoryEntry>,
    /// Maximum history size.
    max_size: usize,
    /// Next entry ID.
    next_id: u64,
}

impl ClipboardHistory {
    /// Create a new clipboard history.
    pub fn new(max_size: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            max_size,
            next_id: 1,
        }
    }

    /// Add a new entry.
    pub fn add(&mut self, content: ClipboardContent) -> u64 {
        // Check for duplicates
        if let Some(existing) = self.entries.front() {
            if let (Some(new_text), Some(old_text)) = (&content.text, &existing.content.text) {
                if new_text == old_text {
                    return existing.id;
                }
            }
        }

        let id = self.next_id;
        self.next_id += 1;

        let entry = HistoryEntry::new(id, content);
        self.entries.push_front(entry);

        // Remove old entries (but keep pinned ones)
        while self.entries.len() > self.max_size {
            // Find a non-pinned entry to remove from the back
            if let Some(pos) = self.entries.iter().rposition(|e| !e.pinned) {
                self.entries.remove(pos);
            } else {
                // All entries are pinned, remove the oldest anyway
                self.entries.pop_back();
            }
        }

        id
    }

    /// Get the most recent entry.
    pub fn latest(&self) -> Option<&HistoryEntry> {
        self.entries.front()
    }

    /// Get an entry by ID.
    pub fn get(&self, id: u64) -> Option<&HistoryEntry> {
        self.entries.iter().find(|e| e.id == id)
    }

    /// Get a mutable entry by ID.
    pub fn get_mut(&mut self, id: u64) -> Option<&mut HistoryEntry> {
        self.entries.iter_mut().find(|e| e.id == id)
    }

    /// Get all entries.
    pub fn all(&self) -> impl Iterator<Item = &HistoryEntry> {
        self.entries.iter()
    }

    /// Get entries by content type.
    pub fn by_type(&self, content_type: ContentType) -> Vec<&HistoryEntry> {
        self.entries
            .iter()
            .filter(|e| e.content.content_type == content_type)
            .collect()
    }

    /// Search history.
    pub fn search(&self, query: &str) -> Vec<&HistoryEntry> {
        let query_lower = query.to_lowercase();
        self.entries
            .iter()
            .filter(|e| {
                e.content
                    .text
                    .as_ref()
                    .map(|t| t.to_lowercase().contains(&query_lower))
                    .unwrap_or(false)
            })
            .collect()
    }

    /// Get pinned entries.
    pub fn pinned(&self) -> Vec<&HistoryEntry> {
        self.entries.iter().filter(|e| e.pinned).collect()
    }

    /// Clear history (except pinned).
    pub fn clear(&mut self) {
        self.entries.retain(|e| e.pinned);
    }

    /// Clear all history including pinned.
    pub fn clear_all(&mut self) {
        self.entries.clear();
    }

    /// Get history size.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Delete an entry by ID.
    pub fn delete(&mut self, id: u64) -> bool {
        if let Some(pos) = self.entries.iter().position(|e| e.id == id) {
            self.entries.remove(pos);
            true
        } else {
            false
        }
    }
}

impl Default for ClipboardHistory {
    fn default() -> Self {
        Self::new(100)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_history_add() {
        let mut history = ClipboardHistory::new(10);

        history.add(ClipboardContent::from_text("first"));
        history.add(ClipboardContent::from_text("second"));

        assert_eq!(history.len(), 2);
        assert_eq!(
            history.latest().unwrap().content.text.as_deref(),
            Some("second")
        );
    }

    #[test]
    fn test_history_deduplication() {
        let mut history = ClipboardHistory::new(10);

        let id1 = history.add(ClipboardContent::from_text("same"));
        let id2 = history.add(ClipboardContent::from_text("same"));

        assert_eq!(id1, id2);
        assert_eq!(history.len(), 1);
    }

    #[test]
    fn test_history_max_size() {
        let mut history = ClipboardHistory::new(3);

        for i in 0..5 {
            history.add(ClipboardContent::from_text(format!("item {}", i)));
        }

        assert_eq!(history.len(), 3);
    }

    #[test]
    fn test_history_search() {
        let mut history = ClipboardHistory::new(10);

        history.add(ClipboardContent::from_text("hello world"));
        history.add(ClipboardContent::from_text("goodbye world"));
        history.add(ClipboardContent::from_text("hello there"));

        let results = history.search("hello");
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_pinned_entries() {
        let mut history = ClipboardHistory::new(3);

        let id1 = history.add(ClipboardContent::from_text("pinned"));
        history.add(ClipboardContent::from_text("normal1"));
        history.add(ClipboardContent::from_text("normal2"));

        history.get_mut(id1).unwrap().pin();

        // Add more to trigger eviction
        history.add(ClipboardContent::from_text("normal3"));

        // Pinned should still be there
        assert!(history.get(id1).is_some());
        assert!(history.get(id1).unwrap().pinned);
    }
}
