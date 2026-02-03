//! Diff utilities for comparing branches.

use crate::branch::{Branch, BranchMessage};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Diff between two branches.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchDiff {
    /// Name of branch A.
    pub branch_a: String,
    /// Name of branch B.
    pub branch_b: String,
    /// Index where branches diverge.
    pub divergence_point: usize,
    /// Messages only in branch A.
    pub only_in_a: Vec<MessageSummary>,
    /// Messages only in branch B.
    pub only_in_b: Vec<MessageSummary>,
    /// Messages that differ between branches.
    pub differences: Vec<MessageDiff>,
    /// Common messages.
    pub common_count: usize,
}

impl BranchDiff {
    /// Compare two branches and produce a diff.
    pub fn compare(a: &Branch, b: &Branch) -> Self {
        let mut divergence_point = 0;
        let common_len = a.messages.len().min(b.messages.len());

        // Find divergence point
        for i in 0..common_len {
            if a.messages[i].id == b.messages[i].id {
                divergence_point = i + 1;
            } else {
                break;
            }
        }

        // Messages only in A (after divergence)
        let only_in_a: Vec<_> = a
            .messages
            .iter()
            .skip(divergence_point)
            .map(MessageSummary::from)
            .collect();

        // Messages only in B (after divergence)
        let only_in_b: Vec<_> = b
            .messages
            .iter()
            .skip(divergence_point)
            .map(MessageSummary::from)
            .collect();

        // Find messages that exist in both but differ
        let differences: Vec<_> = (0..divergence_point)
            .filter_map(|i| {
                let msg_a = &a.messages[i];
                let msg_b = &b.messages[i];
                if msg_a.content != msg_b.content {
                    Some(MessageDiff {
                        index: i,
                        message_id: msg_a.id,
                        diff_type: DiffType::Modified,
                        content_a: Some(msg_a.content.clone()),
                        content_b: Some(msg_b.content.clone()),
                    })
                } else {
                    None
                }
            })
            .collect();

        Self {
            branch_a: a.name.clone(),
            branch_b: b.name.clone(),
            divergence_point,
            only_in_a,
            only_in_b,
            differences,
            common_count: divergence_point,
        }
    }

    /// Check if branches are identical.
    pub fn is_identical(&self) -> bool {
        self.only_in_a.is_empty() && self.only_in_b.is_empty() && self.differences.is_empty()
    }

    /// Get total number of differences.
    pub fn diff_count(&self) -> usize {
        self.only_in_a.len() + self.only_in_b.len() + self.differences.len()
    }

    /// Get a human-readable summary.
    pub fn summary(&self) -> String {
        if self.is_identical() {
            return "Branches are identical".to_string();
        }

        let mut parts = Vec::new();

        if !self.only_in_a.is_empty() {
            parts.push(format!(
                "{} messages only in '{}'",
                self.only_in_a.len(),
                self.branch_a
            ));
        }
        if !self.only_in_b.is_empty() {
            parts.push(format!(
                "{} messages only in '{}'",
                self.only_in_b.len(),
                self.branch_b
            ));
        }
        if !self.differences.is_empty() {
            parts.push(format!("{} modified messages", self.differences.len()));
        }

        format!(
            "Diverged at message {}: {}",
            self.divergence_point,
            parts.join(", ")
        )
    }
}

/// Summary of a message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageSummary {
    /// Message ID.
    pub id: Uuid,
    /// Role.
    pub role: String,
    /// Content preview (truncated).
    pub preview: String,
    /// Full content length.
    pub content_length: usize,
}

impl From<&BranchMessage> for MessageSummary {
    fn from(msg: &BranchMessage) -> Self {
        let preview = if msg.content.len() > 100 {
            format!("{}...", &msg.content[..100])
        } else {
            msg.content.clone()
        };

        Self {
            id: msg.id,
            role: msg.role.clone(),
            preview,
            content_length: msg.content.len(),
        }
    }
}

/// Diff for a single message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageDiff {
    /// Message index.
    pub index: usize,
    /// Message ID.
    pub message_id: Uuid,
    /// Type of difference.
    pub diff_type: DiffType,
    /// Content in branch A.
    pub content_a: Option<String>,
    /// Content in branch B.
    pub content_b: Option<String>,
}

impl MessageDiff {
    /// Create a line-by-line diff.
    pub fn line_diff(&self) -> Vec<LineDiff> {
        match (&self.content_a, &self.content_b) {
            (Some(a), Some(b)) => compute_line_diff(a, b),
            (Some(a), None) => a
                .lines()
                .map(|l| LineDiff::Removed(l.to_string()))
                .collect(),
            (None, Some(b)) => b.lines().map(|l| LineDiff::Added(l.to_string())).collect(),
            (None, None) => Vec::new(),
        }
    }
}

/// Type of difference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffType {
    /// Message was added.
    Added,
    /// Message was removed.
    Removed,
    /// Message was modified.
    Modified,
}

/// A line-level diff.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LineDiff {
    /// Line is the same in both.
    Same(String),
    /// Line was added.
    Added(String),
    /// Line was removed.
    Removed(String),
}

/// Compute a simple line-by-line diff.
fn compute_line_diff(a: &str, b: &str) -> Vec<LineDiff> {
    let lines_a: Vec<&str> = a.lines().collect();
    let lines_b: Vec<&str> = b.lines().collect();

    let mut result = Vec::new();
    let max_len = lines_a.len().max(lines_b.len());

    for i in 0..max_len {
        match (lines_a.get(i), lines_b.get(i)) {
            (Some(la), Some(lb)) if la == lb => {
                result.push(LineDiff::Same(la.to_string()));
            }
            (Some(la), Some(lb)) => {
                result.push(LineDiff::Removed(la.to_string()));
                result.push(LineDiff::Added(lb.to_string()));
            }
            (Some(la), None) => {
                result.push(LineDiff::Removed(la.to_string()));
            }
            (None, Some(lb)) => {
                result.push(LineDiff::Added(lb.to_string()));
            }
            (None, None) => {}
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identical_branches() {
        let mut a = Branch::new("a");
        a.add_message("user", "Hello");
        a.add_message("assistant", "Hi!");

        // Clone for identical branch
        let b = Branch {
            name: "b".to_string(),
            ..a.clone()
        };

        let diff = BranchDiff::compare(&a, &b);
        assert!(diff.is_identical());
        assert_eq!(diff.common_count, 2);
    }

    #[test]
    fn test_divergent_branches() {
        let mut a = Branch::new("a");
        a.add_message("user", "Hello");
        a.add_message("assistant", "Hi!");

        let mut b = Branch::from_parent("b", &a, 0);
        b.add_message("assistant", "Different response");

        let diff = BranchDiff::compare(&a, &b);
        assert!(!diff.is_identical());
        assert_eq!(diff.divergence_point, 1); // Diverges after first message
        assert_eq!(diff.only_in_a.len(), 1);
        assert_eq!(diff.only_in_b.len(), 1);
    }

    #[test]
    fn test_diff_summary() {
        let a = Branch::new("main");
        let mut b = Branch::new("feature");
        b.add_message("user", "Extra message");

        let diff = BranchDiff::compare(&a, &b);
        let summary = diff.summary();
        assert!(summary.contains("feature"));
    }

    #[test]
    fn test_line_diff() {
        let diff = MessageDiff {
            index: 0,
            message_id: Uuid::new_v4(),
            diff_type: DiffType::Modified,
            content_a: Some("Hello\nWorld".to_string()),
            content_b: Some("Hello\nRust".to_string()),
        };

        let lines = diff.line_diff();
        assert!(lines
            .iter()
            .any(|l| matches!(l, LineDiff::Same(s) if s == "Hello")));
        assert!(lines
            .iter()
            .any(|l| matches!(l, LineDiff::Removed(s) if s == "World")));
        assert!(lines
            .iter()
            .any(|l| matches!(l, LineDiff::Added(s) if s == "Rust")));
    }
}
