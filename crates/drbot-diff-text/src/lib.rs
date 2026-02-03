//! Text diffing for drbot.
//!
//! This crate provides:
//! - Line-based diff
//! - Word-based diff
//! - Character-based diff
//! - Unified diff format

use std::fmt;
use thiserror::Error;

/// Diff error types.
#[derive(Error, Debug)]
pub enum DiffError {
    #[error("Diff error: {0}")]
    Error(String),
}

/// Result type for diff operations.
pub type Result<T> = std::result::Result<T, DiffError>;

/// Diff operation type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffOp {
    /// Item is equal in both.
    Equal,
    /// Item was inserted (in new, not in old).
    Insert,
    /// Item was deleted (in old, not in new).
    Delete,
}

/// A single diff change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change<T> {
    /// The operation type.
    pub op: DiffOp,
    /// The value.
    pub value: T,
}

impl<T> Change<T> {
    /// Create new change.
    pub fn new(op: DiffOp, value: T) -> Self {
        Self { op, value }
    }

    /// Check if equal.
    pub fn is_equal(&self) -> bool {
        self.op == DiffOp::Equal
    }

    /// Check if insert.
    pub fn is_insert(&self) -> bool {
        self.op == DiffOp::Insert
    }

    /// Check if delete.
    pub fn is_delete(&self) -> bool {
        self.op == DiffOp::Delete
    }
}

/// Line diff result.
#[derive(Debug, Clone)]
pub struct LineDiff {
    /// The changes.
    pub changes: Vec<Change<String>>,
}

impl LineDiff {
    /// Create new line diff.
    pub fn new(changes: Vec<Change<String>>) -> Self {
        Self { changes }
    }

    /// Get added lines.
    pub fn added(&self) -> Vec<&str> {
        self.changes
            .iter()
            .filter(|c| c.is_insert())
            .map(|c| c.value.as_str())
            .collect()
    }

    /// Get removed lines.
    pub fn removed(&self) -> Vec<&str> {
        self.changes
            .iter()
            .filter(|c| c.is_delete())
            .map(|c| c.value.as_str())
            .collect()
    }

    /// Get unchanged lines.
    pub fn unchanged(&self) -> Vec<&str> {
        self.changes
            .iter()
            .filter(|c| c.is_equal())
            .map(|c| c.value.as_str())
            .collect()
    }

    /// Check if there are any changes.
    pub fn has_changes(&self) -> bool {
        self.changes.iter().any(|c| !c.is_equal())
    }

    /// Count added lines.
    pub fn added_count(&self) -> usize {
        self.changes.iter().filter(|c| c.is_insert()).count()
    }

    /// Count removed lines.
    pub fn removed_count(&self) -> usize {
        self.changes.iter().filter(|c| c.is_delete()).count()
    }

    /// Format as unified diff.
    pub fn to_unified(&self, old_name: &str, new_name: &str) -> String {
        let mut output = String::new();
        output.push_str(&format!("--- {}\n", old_name));
        output.push_str(&format!("+++ {}\n", new_name));

        // Simple format without proper hunks
        for change in &self.changes {
            match change.op {
                DiffOp::Equal => output.push_str(&format!(" {}\n", change.value)),
                DiffOp::Insert => output.push_str(&format!("+{}\n", change.value)),
                DiffOp::Delete => output.push_str(&format!("-{}\n", change.value)),
            }
        }

        output
    }

    /// Format as simple diff.
    pub fn to_simple(&self) -> String {
        let mut output = String::new();

        for change in &self.changes {
            match change.op {
                DiffOp::Equal => {}
                DiffOp::Insert => output.push_str(&format!("+ {}\n", change.value)),
                DiffOp::Delete => output.push_str(&format!("- {}\n", change.value)),
            }
        }

        output
    }
}

impl fmt::Display for LineDiff {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_simple())
    }
}

/// Text differ.
pub struct Diff;

impl Diff {
    /// Compute line-based diff using LCS algorithm.
    pub fn lines(old: &str, new: &str) -> LineDiff {
        let old_lines: Vec<&str> = old.lines().collect();
        let new_lines: Vec<&str> = new.lines().collect();

        let changes = Self::diff_sequences(&old_lines, &new_lines)
            .into_iter()
            .map(|c| Change::new(c.op, c.value.to_string()))
            .collect();

        LineDiff::new(changes)
    }

    /// Compute word-based diff.
    pub fn words(old: &str, new: &str) -> Vec<Change<String>> {
        let old_words: Vec<&str> = old.split_whitespace().collect();
        let new_words: Vec<&str> = new.split_whitespace().collect();

        Self::diff_sequences(&old_words, &new_words)
            .into_iter()
            .map(|c| Change::new(c.op, c.value.to_string()))
            .collect()
    }

    /// Compute character-based diff.
    pub fn chars(old: &str, new: &str) -> Vec<Change<char>> {
        let old_chars: Vec<char> = old.chars().collect();
        let new_chars: Vec<char> = new.chars().collect();

        Self::diff_sequences(&old_chars, &new_chars)
    }

    /// Generic sequence diff using LCS.
    fn diff_sequences<T: Clone + PartialEq>(old: &[T], new: &[T]) -> Vec<Change<T>> {
        let m = old.len();
        let n = new.len();

        // Build LCS table
        let mut lcs = vec![vec![0usize; n + 1]; m + 1];

        for i in 1..=m {
            for j in 1..=n {
                if old[i - 1] == new[j - 1] {
                    lcs[i][j] = lcs[i - 1][j - 1] + 1;
                } else {
                    lcs[i][j] = lcs[i - 1][j].max(lcs[i][j - 1]);
                }
            }
        }

        // Backtrack to build diff
        let mut changes = Vec::new();
        let mut i = m;
        let mut j = n;

        while i > 0 || j > 0 {
            if i > 0 && j > 0 && old[i - 1] == new[j - 1] {
                changes.push(Change::new(DiffOp::Equal, old[i - 1].clone()));
                i -= 1;
                j -= 1;
            } else if j > 0 && (i == 0 || lcs[i][j - 1] >= lcs[i - 1][j]) {
                changes.push(Change::new(DiffOp::Insert, new[j - 1].clone()));
                j -= 1;
            } else if i > 0 {
                changes.push(Change::new(DiffOp::Delete, old[i - 1].clone()));
                i -= 1;
            }
        }

        changes.reverse();
        changes
    }

    /// Get edit distance (Levenshtein distance).
    pub fn edit_distance(old: &str, new: &str) -> usize {
        let old_chars: Vec<char> = old.chars().collect();
        let new_chars: Vec<char> = new.chars().collect();

        let m = old_chars.len();
        let n = new_chars.len();

        let mut dp = vec![vec![0usize; n + 1]; m + 1];

        for i in 0..=m {
            dp[i][0] = i;
        }
        for j in 0..=n {
            dp[0][j] = j;
        }

        for i in 1..=m {
            for j in 1..=n {
                if old_chars[i - 1] == new_chars[j - 1] {
                    dp[i][j] = dp[i - 1][j - 1];
                } else {
                    dp[i][j] = 1 + dp[i - 1][j].min(dp[i][j - 1]).min(dp[i - 1][j - 1]);
                }
            }
        }

        dp[m][n]
    }

    /// Get similarity ratio (0.0 to 1.0).
    pub fn similarity(old: &str, new: &str) -> f64 {
        if old.is_empty() && new.is_empty() {
            return 1.0;
        }

        let max_len = old.len().max(new.len());
        if max_len == 0 {
            return 1.0;
        }

        let distance = Self::edit_distance(old, new);
        1.0 - (distance as f64 / max_len as f64)
    }

    /// Check if strings are similar (above threshold).
    pub fn is_similar(old: &str, new: &str, threshold: f64) -> bool {
        Self::similarity(old, new) >= threshold
    }
}

/// Patch representation.
#[derive(Debug, Clone)]
pub struct Patch {
    /// Original text.
    pub original: String,
    /// Changes to apply.
    pub changes: Vec<Change<String>>,
}

impl Patch {
    /// Create patch from diff.
    pub fn from_diff(original: &str, diff: &LineDiff) -> Self {
        Self {
            original: original.to_string(),
            changes: diff.changes.clone(),
        }
    }

    /// Apply patch to get new text.
    pub fn apply(&self) -> String {
        let mut result = Vec::new();

        for change in &self.changes {
            match change.op {
                DiffOp::Equal | DiffOp::Insert => {
                    result.push(change.value.as_str());
                }
                DiffOp::Delete => {}
            }
        }

        result.join("\n")
    }

    /// Reverse patch (swap inserts and deletes).
    pub fn reverse(&self) -> Self {
        let changes = self
            .changes
            .iter()
            .map(|c| {
                let op = match c.op {
                    DiffOp::Insert => DiffOp::Delete,
                    DiffOp::Delete => DiffOp::Insert,
                    DiffOp::Equal => DiffOp::Equal,
                };
                Change::new(op, c.value.clone())
            })
            .collect();

        Self {
            original: self.apply(),
            changes,
        }
    }
}

/// Side-by-side diff display.
pub struct SideBySide {
    /// Left side lines.
    pub left: Vec<(DiffOp, String)>,
    /// Right side lines.
    pub right: Vec<(DiffOp, String)>,
}

impl SideBySide {
    /// Create side-by-side view from line diff.
    pub fn from_diff(diff: &LineDiff) -> Self {
        let mut left = Vec::new();
        let mut right = Vec::new();

        for change in &diff.changes {
            match change.op {
                DiffOp::Equal => {
                    left.push((DiffOp::Equal, change.value.clone()));
                    right.push((DiffOp::Equal, change.value.clone()));
                }
                DiffOp::Delete => {
                    left.push((DiffOp::Delete, change.value.clone()));
                    right.push((DiffOp::Delete, String::new()));
                }
                DiffOp::Insert => {
                    left.push((DiffOp::Insert, String::new()));
                    right.push((DiffOp::Insert, change.value.clone()));
                }
            }
        }

        Self { left, right }
    }

    /// Format as string with given width.
    pub fn format(&self, width: usize) -> String {
        let mut output = String::new();
        let half_width = width / 2 - 2;

        for (l, r) in self.left.iter().zip(self.right.iter()) {
            let left_str = Self::truncate_pad(&l.1, half_width);
            let right_str = Self::truncate_pad(&r.1, half_width);

            let left_marker = match l.0 {
                DiffOp::Delete => "-",
                DiffOp::Insert => " ",
                DiffOp::Equal => " ",
            };
            let right_marker = match r.0 {
                DiffOp::Insert => "+",
                DiffOp::Delete => " ",
                DiffOp::Equal => " ",
            };

            output.push_str(&format!(
                "{}{} | {}{}\n",
                left_marker, left_str, right_marker, right_str
            ));
        }

        output
    }

    fn truncate_pad(s: &str, width: usize) -> String {
        if s.len() > width {
            format!("{}...", &s[..width.saturating_sub(3)])
        } else {
            format!("{:width$}", s, width = width)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_line_diff() {
        let old = "a\nb\nc";
        let new = "a\nd\nc";

        let diff = Diff::lines(old, new);
        assert!(diff.has_changes());
        assert_eq!(diff.added_count(), 1);
        assert_eq!(diff.removed_count(), 1);
    }

    #[test]
    fn test_no_changes() {
        let old = "hello\nworld";
        let new = "hello\nworld";

        let diff = Diff::lines(old, new);
        assert!(!diff.has_changes());
    }

    #[test]
    fn test_edit_distance() {
        assert_eq!(Diff::edit_distance("kitten", "sitting"), 3);
        assert_eq!(Diff::edit_distance("", "abc"), 3);
        assert_eq!(Diff::edit_distance("abc", "abc"), 0);
    }

    #[test]
    fn test_similarity() {
        assert_eq!(Diff::similarity("hello", "hello"), 1.0);
        assert!(Diff::similarity("hello", "hallo") > 0.5);
        assert!(Diff::is_similar("hello", "hallo", 0.5));
    }

    #[test]
    fn test_word_diff() {
        let old = "hello world";
        let new = "hello there";

        let changes = Diff::words(old, new);
        assert!(changes.iter().any(|c| c.is_delete()));
        assert!(changes.iter().any(|c| c.is_insert()));
    }

    #[test]
    fn test_char_diff() {
        let old = "abc";
        let new = "adc";

        let changes = Diff::chars(old, new);
        assert!(changes.iter().any(|c| c.is_delete() && c.value == 'b'));
        assert!(changes.iter().any(|c| c.is_insert() && c.value == 'd'));
    }

    #[test]
    fn test_unified_format() {
        let old = "a\nb";
        let new = "a\nc";

        let diff = Diff::lines(old, new);
        let unified = diff.to_unified("old.txt", "new.txt");

        assert!(unified.contains("--- old.txt"));
        assert!(unified.contains("+++ new.txt"));
        assert!(unified.contains("-b"));
        assert!(unified.contains("+c"));
    }

    #[test]
    fn test_patch_apply() {
        let old = "a\nb\nc";
        let new = "a\nd\nc";

        let diff = Diff::lines(old, new);
        let patch = Patch::from_diff(old, &diff);
        let result = patch.apply();

        assert_eq!(result, new);
    }
}
