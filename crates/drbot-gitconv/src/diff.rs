//! Diff capabilities for conversations.

use crate::conversation::Conversation;
use crate::message::Message;
use serde::{Deserialize, Serialize};

/// Type of diff entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiffType {
    /// Message was added.
    Added,
    /// Message was removed.
    Removed,
    /// Message was modified.
    Modified,
    /// No change.
    Unchanged,
}

/// A single diff entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffEntry {
    /// Diff type.
    pub diff_type: DiffType,
    /// Message ID.
    pub message_id: Option<String>,
    /// Old content (for modified/removed).
    pub old_content: Option<String>,
    /// New content (for modified/added).
    pub new_content: Option<String>,
    /// Position in conversation.
    pub position: usize,
}

impl DiffEntry {
    /// Create an "added" entry.
    pub fn added(message: &Message, position: usize) -> Self {
        Self {
            diff_type: DiffType::Added,
            message_id: Some(message.id.clone()),
            old_content: None,
            new_content: Some(message.content.clone()),
            position,
        }
    }

    /// Create a "removed" entry.
    pub fn removed(message: &Message, position: usize) -> Self {
        Self {
            diff_type: DiffType::Removed,
            message_id: Some(message.id.clone()),
            old_content: Some(message.content.clone()),
            new_content: None,
            position,
        }
    }

    /// Create a "modified" entry.
    pub fn modified(old: &Message, new: &Message, position: usize) -> Self {
        Self {
            diff_type: DiffType::Modified,
            message_id: Some(new.id.clone()),
            old_content: Some(old.content.clone()),
            new_content: Some(new.content.clone()),
            position,
        }
    }
}

/// Diff between two conversation versions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationDiff {
    /// Conversation ID.
    pub conversation_id: String,
    /// Old version (e.g., commit hash).
    pub old_version: Option<String>,
    /// New version.
    pub new_version: Option<String>,
    /// Diff entries.
    pub entries: Vec<DiffEntry>,
    /// Summary statistics.
    pub stats: DiffStats,
}

/// Diff statistics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DiffStats {
    /// Number of added messages.
    pub added: usize,
    /// Number of removed messages.
    pub removed: usize,
    /// Number of modified messages.
    pub modified: usize,
    /// Number of unchanged messages.
    pub unchanged: usize,
}

impl ConversationDiff {
    /// Create a diff between two conversations.
    pub fn diff(old: &Conversation, new: &Conversation) -> Self {
        let mut entries = Vec::new();
        let mut stats = DiffStats::default();

        // Build a map of old messages by ID
        let old_messages: std::collections::HashMap<_, _> =
            old.messages.iter().map(|m| (m.id.clone(), m)).collect();

        let new_messages: std::collections::HashMap<_, _> =
            new.messages.iter().map(|m| (m.id.clone(), m)).collect();

        // Check for added/modified messages
        for (pos, msg) in new.messages.iter().enumerate() {
            if let Some(old_msg) = old_messages.get(&msg.id) {
                if old_msg.content != msg.content {
                    entries.push(DiffEntry::modified(old_msg, msg, pos));
                    stats.modified += 1;
                } else {
                    stats.unchanged += 1;
                }
            } else {
                entries.push(DiffEntry::added(msg, pos));
                stats.added += 1;
            }
        }

        // Check for removed messages
        for (pos, msg) in old.messages.iter().enumerate() {
            if !new_messages.contains_key(&msg.id) {
                entries.push(DiffEntry::removed(msg, pos));
                stats.removed += 1;
            }
        }

        // Sort entries by position
        entries.sort_by_key(|e| e.position);

        Self {
            conversation_id: new.id.clone(),
            old_version: None,
            new_version: None,
            entries,
            stats,
        }
    }

    /// Check if there are any changes.
    pub fn has_changes(&self) -> bool {
        !self.entries.is_empty()
    }

    /// Get added entries.
    pub fn added(&self) -> Vec<&DiffEntry> {
        self.entries
            .iter()
            .filter(|e| e.diff_type == DiffType::Added)
            .collect()
    }

    /// Get removed entries.
    pub fn removed(&self) -> Vec<&DiffEntry> {
        self.entries
            .iter()
            .filter(|e| e.diff_type == DiffType::Removed)
            .collect()
    }

    /// Get modified entries.
    pub fn modified(&self) -> Vec<&DiffEntry> {
        self.entries
            .iter()
            .filter(|e| e.diff_type == DiffType::Modified)
            .collect()
    }

    /// Format as unified diff style.
    pub fn to_unified_diff(&self) -> String {
        let mut output = String::new();

        for entry in &self.entries {
            match entry.diff_type {
                DiffType::Added => {
                    if let Some(content) = &entry.new_content {
                        output.push_str(&format!("+ {}\n", content));
                    }
                }
                DiffType::Removed => {
                    if let Some(content) = &entry.old_content {
                        output.push_str(&format!("- {}\n", content));
                    }
                }
                DiffType::Modified => {
                    if let Some(old) = &entry.old_content {
                        output.push_str(&format!("- {}\n", old));
                    }
                    if let Some(new) = &entry.new_content {
                        output.push_str(&format!("+ {}\n", new));
                    }
                }
                DiffType::Unchanged => {}
            }
        }

        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::Message;

    #[test]
    fn test_diff_added() {
        let old = Conversation::new("Test");
        let mut new = Conversation::new("Test");
        new.id = old.id.clone();
        new.add_message(Message::user("Hello"));

        let diff = ConversationDiff::diff(&old, &new);

        assert!(diff.has_changes());
        assert_eq!(diff.stats.added, 1);
        assert_eq!(diff.added().len(), 1);
    }

    #[test]
    fn test_diff_modified() {
        let mut old = Conversation::new("Test");
        let mut msg = Message::user("Hello");
        let msg_id = msg.id.clone();
        old.add_message(msg);

        let mut new = Conversation::new("Test");
        new.id = old.id.clone();
        let mut new_msg = Message::user("Hello World");
        new_msg.id = msg_id;
        new.add_message(new_msg);

        let diff = ConversationDiff::diff(&old, &new);

        assert!(diff.has_changes());
        assert_eq!(diff.stats.modified, 1);
    }

    #[test]
    fn test_unified_diff() {
        let old = Conversation::new("Test");
        let mut new = Conversation::new("Test");
        new.id = old.id.clone();
        new.add_message(Message::user("New message"));

        let diff = ConversationDiff::diff(&old, &new);
        let unified = diff.to_unified_diff();

        assert!(unified.contains("+ New message"));
    }
}
