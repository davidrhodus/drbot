//! Diff and patch utilities for drbot.
//!
//! This crate provides:
//! - Text diffing
//! - JSON diff/patch
//! - Semantic diff
//! - Change detection

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use thiserror::Error;

/// Diff error types.
#[derive(Error, Debug)]
pub enum DiffError {
    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Patch failed: {0}")]
    PatchFailed(String),

    #[error("Type mismatch at {path}")]
    TypeMismatch { path: String },
}

/// Result type for diff operations.
pub type Result<T> = std::result::Result<T, DiffError>;

/// Change type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChangeType {
    /// Value added.
    Add,
    /// Value removed.
    Remove,
    /// Value modified.
    Modify,
    /// No change.
    None,
}

/// Text diff line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    /// Line number in old text.
    pub old_line: Option<usize>,
    /// Line number in new text.
    pub new_line: Option<usize>,
    /// Line content.
    pub content: String,
    /// Change type.
    pub change: ChangeType,
}

impl DiffLine {
    fn added(new_line: usize, content: String) -> Self {
        Self {
            old_line: None,
            new_line: Some(new_line),
            content,
            change: ChangeType::Add,
        }
    }

    fn removed(old_line: usize, content: String) -> Self {
        Self {
            old_line: Some(old_line),
            new_line: None,
            content,
            change: ChangeType::Remove,
        }
    }

    fn unchanged(old_line: usize, new_line: usize, content: String) -> Self {
        Self {
            old_line: Some(old_line),
            new_line: Some(new_line),
            content,
            change: ChangeType::None,
        }
    }
}

/// Text diff result.
#[derive(Debug, Clone)]
pub struct TextDiff {
    /// Diff lines.
    pub lines: Vec<DiffLine>,
    /// Number of additions.
    pub additions: usize,
    /// Number of deletions.
    pub deletions: usize,
}

impl TextDiff {
    /// Check if there are changes.
    pub fn has_changes(&self) -> bool {
        self.additions > 0 || self.deletions > 0
    }

    /// Format as unified diff.
    pub fn to_unified(&self) -> String {
        let mut result = String::new();

        for line in &self.lines {
            match line.change {
                ChangeType::Add => {
                    result.push_str(&format!("+{}\n", line.content));
                }
                ChangeType::Remove => {
                    result.push_str(&format!("-{}\n", line.content));
                }
                ChangeType::None => {
                    result.push_str(&format!(" {}\n", line.content));
                }
                _ => {}
            }
        }

        result
    }
}

/// Simple line-based diff (Myers algorithm simplified).
pub fn diff_lines(old: &str, new: &str) -> TextDiff {
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();

    let mut result = Vec::new();
    let mut additions = 0;
    let mut deletions = 0;

    // Simple LCS-based diff
    let lcs = longest_common_subsequence(&old_lines, &new_lines);

    let mut old_idx = 0;
    let mut new_idx = 0;
    let mut lcs_idx = 0;

    while old_idx < old_lines.len() || new_idx < new_lines.len() {
        if lcs_idx < lcs.len() && old_idx < old_lines.len() && old_lines[old_idx] == lcs[lcs_idx] {
            // Find the matching new line
            while new_idx < new_lines.len() && new_lines[new_idx] != lcs[lcs_idx] {
                result.push(DiffLine::added(new_idx + 1, new_lines[new_idx].to_string()));
                additions += 1;
                new_idx += 1;
            }

            result.push(DiffLine::unchanged(
                old_idx + 1,
                new_idx + 1,
                old_lines[old_idx].to_string(),
            ));
            old_idx += 1;
            new_idx += 1;
            lcs_idx += 1;
        } else if old_idx < old_lines.len() {
            result.push(DiffLine::removed(
                old_idx + 1,
                old_lines[old_idx].to_string(),
            ));
            deletions += 1;
            old_idx += 1;
        } else if new_idx < new_lines.len() {
            result.push(DiffLine::added(new_idx + 1, new_lines[new_idx].to_string()));
            additions += 1;
            new_idx += 1;
        }
    }

    TextDiff {
        lines: result,
        additions,
        deletions,
    }
}

fn longest_common_subsequence<'a>(a: &[&'a str], b: &[&'a str]) -> Vec<&'a str> {
    let m = a.len();
    let n = b.len();

    let mut dp = vec![vec![0; n + 1]; m + 1];

    for i in 1..=m {
        for j in 1..=n {
            if a[i - 1] == b[j - 1] {
                dp[i][j] = dp[i - 1][j - 1] + 1;
            } else {
                dp[i][j] = dp[i - 1][j].max(dp[i][j - 1]);
            }
        }
    }

    // Backtrack to find LCS
    let mut result = Vec::new();
    let mut i = m;
    let mut j = n;

    while i > 0 && j > 0 {
        if a[i - 1] == b[j - 1] {
            result.push(a[i - 1]);
            i -= 1;
            j -= 1;
        } else if dp[i - 1][j] > dp[i][j - 1] {
            i -= 1;
        } else {
            j -= 1;
        }
    }

    result.reverse();
    result
}

/// JSON patch operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum JsonPatchOp {
    /// Add value.
    Add { path: String, value: Value },
    /// Remove value.
    Remove { path: String },
    /// Replace value.
    Replace { path: String, value: Value },
    /// Move value.
    Move { from: String, path: String },
    /// Copy value.
    Copy { from: String, path: String },
    /// Test value.
    Test { path: String, value: Value },
}

impl JsonPatchOp {
    pub fn add(path: impl Into<String>, value: Value) -> Self {
        Self::Add {
            path: path.into(),
            value,
        }
    }

    pub fn remove(path: impl Into<String>) -> Self {
        Self::Remove { path: path.into() }
    }

    pub fn replace(path: impl Into<String>, value: Value) -> Self {
        Self::Replace {
            path: path.into(),
            value,
        }
    }
}

/// JSON patch (RFC 6902).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct JsonPatch {
    /// Patch operations.
    pub ops: Vec<JsonPatchOp>,
}

impl JsonPatch {
    /// Create empty patch.
    pub fn new() -> Self {
        Self { ops: Vec::new() }
    }

    /// Add operation.
    pub fn add_op(mut self, op: JsonPatchOp) -> Self {
        self.ops.push(op);
        self
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    /// Apply patch to JSON value.
    pub fn apply(&self, mut target: Value) -> Result<Value> {
        for op in &self.ops {
            target = apply_op(target, op)?;
        }
        Ok(target)
    }
}

fn apply_op(mut target: Value, op: &JsonPatchOp) -> Result<Value> {
    match op {
        JsonPatchOp::Add { path, value } => {
            set_path(&mut target, path, value.clone())?;
        }
        JsonPatchOp::Remove { path } => {
            remove_path(&mut target, path)?;
        }
        JsonPatchOp::Replace { path, value } => {
            set_path(&mut target, path, value.clone())?;
        }
        JsonPatchOp::Move { from, path } => {
            let value = get_path(&target, from)?;
            remove_path(&mut target, from)?;
            set_path(&mut target, path, value)?;
        }
        JsonPatchOp::Copy { from, path } => {
            let value = get_path(&target, from)?;
            set_path(&mut target, path, value)?;
        }
        JsonPatchOp::Test { path, value } => {
            let actual = get_path(&target, path)?;
            if actual != *value {
                return Err(DiffError::PatchFailed(format!("Test failed at {}", path)));
            }
        }
    }
    Ok(target)
}

fn parse_path(path: &str) -> Vec<String> {
    if path.is_empty() || path == "/" {
        return Vec::new();
    }

    path.trim_start_matches('/')
        .split('/')
        .map(|s| s.replace("~1", "/").replace("~0", "~"))
        .collect()
}

fn get_path(value: &Value, path: &str) -> Result<Value> {
    let parts = parse_path(path);
    let mut current = value;

    for part in &parts {
        current = match current {
            Value::Object(obj) => obj
                .get(part)
                .ok_or_else(|| DiffError::PatchFailed(format!("Path not found: {}", path)))?,
            Value::Array(arr) => {
                let idx: usize = part.parse().map_err(|_| {
                    DiffError::PatchFailed(format!("Invalid array index: {}", part))
                })?;
                arr.get(idx).ok_or_else(|| {
                    DiffError::PatchFailed(format!("Index out of bounds: {}", idx))
                })?
            }
            _ => return Err(DiffError::PatchFailed(format!("Cannot traverse: {}", path))),
        };
    }

    Ok(current.clone())
}

fn set_path(value: &mut Value, path: &str, new_value: Value) -> Result<()> {
    let parts = parse_path(path);

    if parts.is_empty() {
        *value = new_value;
        return Ok(());
    }

    let mut current = value;

    for (i, part) in parts.iter().enumerate() {
        let is_last = i == parts.len() - 1;

        current = match current {
            Value::Object(ref mut obj) => {
                if is_last {
                    obj.insert(part.clone(), new_value);
                    return Ok(());
                }
                obj.entry(part.clone())
                    .or_insert_with(|| Value::Object(serde_json::Map::new()))
            }
            Value::Array(ref mut arr) => {
                let idx: usize = part.parse().map_err(|_| {
                    DiffError::PatchFailed(format!("Invalid array index: {}", part))
                })?;
                if is_last {
                    if idx == arr.len() {
                        arr.push(new_value);
                    } else if idx < arr.len() {
                        arr[idx] = new_value;
                    } else {
                        return Err(DiffError::PatchFailed("Index out of bounds".to_string()));
                    }
                    return Ok(());
                }
                arr.get_mut(idx)
                    .ok_or_else(|| DiffError::PatchFailed("Index out of bounds".to_string()))?
            }
            _ => return Err(DiffError::PatchFailed("Cannot traverse".to_string())),
        };
    }

    Ok(())
}

fn remove_path(value: &mut Value, path: &str) -> Result<()> {
    let parts = parse_path(path);

    if parts.is_empty() {
        return Err(DiffError::PatchFailed("Cannot remove root".to_string()));
    }

    let parent_parts = &parts[..parts.len() - 1];
    let key = &parts[parts.len() - 1];

    let mut current = value;
    for part in parent_parts {
        current = match current {
            Value::Object(ref mut obj) => obj
                .get_mut(part)
                .ok_or_else(|| DiffError::PatchFailed("Path not found".to_string()))?,
            Value::Array(ref mut arr) => {
                let idx: usize = part
                    .parse()
                    .map_err(|_| DiffError::PatchFailed("Invalid array index".to_string()))?;
                arr.get_mut(idx)
                    .ok_or_else(|| DiffError::PatchFailed("Index out of bounds".to_string()))?
            }
            _ => return Err(DiffError::PatchFailed("Cannot traverse".to_string())),
        };
    }

    match current {
        Value::Object(ref mut obj) => {
            obj.remove(key);
        }
        Value::Array(ref mut arr) => {
            let idx: usize = key
                .parse()
                .map_err(|_| DiffError::PatchFailed("Invalid array index".to_string()))?;
            if idx < arr.len() {
                arr.remove(idx);
            }
        }
        _ => {
            return Err(DiffError::PatchFailed(
                "Cannot remove from non-container".to_string(),
            ))
        }
    }

    Ok(())
}

/// Create JSON diff.
pub fn diff_json(old: &Value, new: &Value) -> JsonPatch {
    let mut patch = JsonPatch::new();
    diff_json_recursive(old, new, "", &mut patch.ops);
    patch
}

fn diff_json_recursive(old: &Value, new: &Value, path: &str, ops: &mut Vec<JsonPatchOp>) {
    if old == new {
        return;
    }

    match (old, new) {
        (Value::Object(old_obj), Value::Object(new_obj)) => {
            // Find removed keys
            for key in old_obj.keys() {
                if !new_obj.contains_key(key) {
                    let key_path = if path.is_empty() {
                        format!("/{}", key)
                    } else {
                        format!("{}/{}", path, key)
                    };
                    ops.push(JsonPatchOp::remove(key_path));
                }
            }

            // Find added or modified keys
            for (key, new_val) in new_obj {
                let key_path = if path.is_empty() {
                    format!("/{}", key)
                } else {
                    format!("{}/{}", path, key)
                };

                if let Some(old_val) = old_obj.get(key) {
                    diff_json_recursive(old_val, new_val, &key_path, ops);
                } else {
                    ops.push(JsonPatchOp::add(key_path, new_val.clone()));
                }
            }
        }
        (Value::Array(old_arr), Value::Array(new_arr)) => {
            // Simple array diff - replace entire array if different
            if old_arr != new_arr {
                ops.push(JsonPatchOp::replace(path, Value::Array(new_arr.clone())));
            }
        }
        _ => {
            ops.push(JsonPatchOp::replace(path, new.clone()));
        }
    }
}

/// Change summary for an object.
#[derive(Debug, Clone, Default)]
pub struct ChangeSummary {
    /// Added fields.
    pub added: Vec<String>,
    /// Removed fields.
    pub removed: Vec<String>,
    /// Modified fields.
    pub modified: Vec<String>,
}

impl ChangeSummary {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.modified.is_empty()
    }
}

/// Get change summary from JSON diff.
pub fn summarize_changes(old: &Value, new: &Value) -> ChangeSummary {
    let patch = diff_json(old, new);
    let mut summary = ChangeSummary::default();

    for op in patch.ops {
        match op {
            JsonPatchOp::Add { path, .. } => summary.added.push(path),
            JsonPatchOp::Remove { path } => summary.removed.push(path),
            JsonPatchOp::Replace { path, .. } => summary.modified.push(path),
            _ => {}
        }
    }

    summary
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diff_lines() {
        let old = "line1\nline2\nline3";
        let new = "line1\nmodified\nline3\nline4";

        let diff = diff_lines(old, new);

        assert!(diff.has_changes());
        assert!(diff.additions > 0);
        assert!(diff.deletions > 0);
    }

    #[test]
    fn test_diff_lines_no_change() {
        let text = "line1\nline2\nline3";
        let diff = diff_lines(text, text);

        assert!(!diff.has_changes());
    }

    #[test]
    fn test_json_patch_add() {
        let mut target = serde_json::json!({"a": 1});
        let patch = JsonPatch::new().add_op(JsonPatchOp::add("/b", serde_json::json!(2)));

        let result = patch.apply(target).unwrap();
        assert_eq!(result.get("b"), Some(&serde_json::json!(2)));
    }

    #[test]
    fn test_json_patch_remove() {
        let target = serde_json::json!({"a": 1, "b": 2});
        let patch = JsonPatch::new().add_op(JsonPatchOp::remove("/b"));

        let result = patch.apply(target).unwrap();
        assert!(result.get("b").is_none());
    }

    #[test]
    fn test_json_patch_replace() {
        let target = serde_json::json!({"a": 1});
        let patch = JsonPatch::new().add_op(JsonPatchOp::replace("/a", serde_json::json!(2)));

        let result = patch.apply(target).unwrap();
        assert_eq!(result.get("a"), Some(&serde_json::json!(2)));
    }

    #[test]
    fn test_diff_json() {
        let old = serde_json::json!({"a": 1, "b": 2});
        let new = serde_json::json!({"a": 1, "c": 3});

        let patch = diff_json(&old, &new);

        assert!(!patch.is_empty());
    }

    #[test]
    fn test_diff_json_nested() {
        let old = serde_json::json!({"user": {"name": "Alice", "age": 30}});
        let new = serde_json::json!({"user": {"name": "Alice", "age": 31}});

        let patch = diff_json(&old, &new);
        let result = patch.apply(old).unwrap();

        assert_eq!(
            result.get("user").unwrap().get("age"),
            Some(&serde_json::json!(31))
        );
    }

    #[test]
    fn test_change_summary() {
        let old = serde_json::json!({"a": 1, "b": 2});
        let new = serde_json::json!({"a": 2, "c": 3});

        let summary = summarize_changes(&old, &new);

        assert!(summary.added.iter().any(|p| p.contains("c")));
        assert!(summary.removed.iter().any(|p| p.contains("b")));
        assert!(summary.modified.iter().any(|p| p.contains("a")));
    }

    #[test]
    fn test_unified_diff() {
        let old = "line1\nline2";
        let new = "line1\nline3";

        let diff = diff_lines(old, new);
        let unified = diff.to_unified();

        assert!(unified.contains("+line3"));
        assert!(unified.contains("-line2"));
    }
}
