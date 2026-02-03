//! Semantic diffs for drbot.
//!
//! Compare responses semantically, not just textually.
//!
//! # Features
//!
//! - Semantic similarity comparison
//! - Meaning-based diff highlighting
//! - Change categorization
//! - Diff visualization

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Semantic diff result type.
pub type Result<T> = std::result::Result<T, SemDiffError>;

/// Semantic diff errors.
#[derive(Debug, thiserror::Error)]
pub enum SemDiffError {
    #[error("Embedding failed: {0}")]
    EmbeddingFailed(String),
    #[error("Diff failed: {0}")]
    DiffFailed(String),
    #[error("Invalid input")]
    InvalidInput,
}

/// Semantic diff result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticDiff {
    /// Diff ID.
    pub id: Uuid,
    /// Original text.
    pub original: String,
    /// Modified text.
    pub modified: String,
    /// Overall semantic similarity (0-1).
    pub similarity: f32,
    /// Semantic changes.
    pub changes: Vec<SemanticChange>,
    /// Change categories.
    pub categories: Vec<ChangeCategory>,
    /// Summary of changes.
    pub summary: String,
    /// Diffed at.
    pub diffed_at: DateTime<Utc>,
}

impl SemanticDiff {
    /// Check if texts are semantically equivalent.
    pub fn is_equivalent(&self, threshold: f32) -> bool {
        self.similarity >= threshold
    }

    /// Get changes by type.
    pub fn changes_by_type(&self, change_type: ChangeType) -> Vec<&SemanticChange> {
        self.changes
            .iter()
            .filter(|c| c.change_type == change_type)
            .collect()
    }
}

/// A semantic change.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticChange {
    /// Change ID.
    pub id: Uuid,
    /// Change type.
    pub change_type: ChangeType,
    /// Original text segment.
    pub original_segment: String,
    /// Modified text segment.
    pub modified_segment: String,
    /// Semantic similarity of this segment.
    pub segment_similarity: f32,
    /// Impact on overall meaning (0-1).
    pub semantic_impact: f32,
    /// Explanation of the change.
    pub explanation: String,
    /// Start position in original.
    pub original_start: usize,
    /// Start position in modified.
    pub modified_start: usize,
}

/// Change types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeType {
    /// Content added.
    Addition,
    /// Content removed.
    Deletion,
    /// Content modified but meaning preserved.
    Rephrase,
    /// Content modified with meaning change.
    SemanticChange,
    /// Content moved.
    Reorder,
    /// Formatting change.
    Formatting,
    /// No change.
    Unchanged,
}

/// Change category.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeCategory {
    /// Category name.
    pub name: String,
    /// Number of changes in this category.
    pub count: usize,
    /// Total impact.
    pub total_impact: f32,
}

/// Diff configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemDiffConfig {
    /// Similarity threshold for "same meaning".
    pub similarity_threshold: f32,
    /// Minimum segment size for comparison.
    pub min_segment_size: usize,
    /// Compare at sentence level.
    pub sentence_level: bool,
    /// Include formatting changes.
    pub include_formatting: bool,
    /// Generate explanations.
    pub generate_explanations: bool,
}

impl Default for SemDiffConfig {
    fn default() -> Self {
        Self {
            similarity_threshold: 0.85,
            min_segment_size: 10,
            sentence_level: true,
            include_formatting: false,
            generate_explanations: true,
        }
    }
}

/// Semantic diff engine.
pub struct SemDiffEngine {
    config: SemDiffConfig,
    history: Arc<RwLock<Vec<SemanticDiff>>>,
}

impl SemDiffEngine {
    /// Create a new semantic diff engine.
    pub fn new(config: SemDiffConfig) -> Self {
        Self {
            config,
            history: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Compare two texts semantically.
    pub async fn diff(&self, original: &str, modified: &str) -> Result<SemanticDiff> {
        if original.is_empty() && modified.is_empty() {
            return Err(SemDiffError::InvalidInput);
        }

        // Split into segments
        let original_segments = self.segment(original);
        let modified_segments = self.segment(modified);

        // Compare segments
        let mut changes = Vec::new();
        let mut matched_modified: Vec<bool> = vec![false; modified_segments.len()];

        for (orig_idx, orig_seg) in original_segments.iter().enumerate() {
            let mut best_match: Option<(usize, f32)> = None;

            for (mod_idx, mod_seg) in modified_segments.iter().enumerate() {
                if matched_modified[mod_idx] {
                    continue;
                }

                let similarity = self.segment_similarity(orig_seg, mod_seg);
                if similarity > 0.5 && (best_match.is_none() || similarity > best_match.unwrap().1)
                {
                    best_match = Some((mod_idx, similarity));
                }
            }

            if let Some((mod_idx, similarity)) = best_match {
                matched_modified[mod_idx] = true;

                let change_type = if similarity >= self.config.similarity_threshold {
                    if orig_seg == &modified_segments[mod_idx] {
                        ChangeType::Unchanged
                    } else {
                        ChangeType::Rephrase
                    }
                } else {
                    ChangeType::SemanticChange
                };

                if change_type != ChangeType::Unchanged {
                    changes.push(SemanticChange {
                        id: Uuid::new_v4(),
                        change_type,
                        original_segment: orig_seg.clone(),
                        modified_segment: modified_segments[mod_idx].clone(),
                        segment_similarity: similarity,
                        semantic_impact: 1.0 - similarity,
                        explanation: self.explain_change(
                            change_type,
                            orig_seg,
                            &modified_segments[mod_idx],
                        ),
                        original_start: original.find(orig_seg).unwrap_or(0),
                        modified_start: modified.find(&modified_segments[mod_idx]).unwrap_or(0),
                    });
                }
            } else {
                // Deletion
                changes.push(SemanticChange {
                    id: Uuid::new_v4(),
                    change_type: ChangeType::Deletion,
                    original_segment: orig_seg.clone(),
                    modified_segment: String::new(),
                    segment_similarity: 0.0,
                    semantic_impact: 0.8,
                    explanation: format!("Removed: \"{}\"", Self::truncate(orig_seg, 50)),
                    original_start: original.find(orig_seg).unwrap_or(0),
                    modified_start: 0,
                });
            }
        }

        // Find additions
        for (mod_idx, mod_seg) in modified_segments.iter().enumerate() {
            if !matched_modified[mod_idx] {
                changes.push(SemanticChange {
                    id: Uuid::new_v4(),
                    change_type: ChangeType::Addition,
                    original_segment: String::new(),
                    modified_segment: mod_seg.clone(),
                    segment_similarity: 0.0,
                    semantic_impact: 0.6,
                    explanation: format!("Added: \"{}\"", Self::truncate(mod_seg, 50)),
                    original_start: 0,
                    modified_start: modified.find(mod_seg).unwrap_or(0),
                });
            }
        }

        // Calculate overall similarity
        let similarity = self.calculate_similarity(original, modified, &changes);

        // Categorize changes
        let categories = self.categorize_changes(&changes);

        // Generate summary
        let summary = self.generate_summary(&changes, similarity);

        let diff = SemanticDiff {
            id: Uuid::new_v4(),
            original: original.to_string(),
            modified: modified.to_string(),
            similarity,
            changes,
            categories,
            summary,
            diffed_at: Utc::now(),
        };

        self.history.write().await.push(diff.clone());

        Ok(diff)
    }

    fn segment(&self, text: &str) -> Vec<String> {
        if self.config.sentence_level {
            // Split by sentences
            text.split(|c| c == '.' || c == '!' || c == '?')
                .map(|s| s.trim().to_string())
                .filter(|s| s.len() >= self.config.min_segment_size)
                .collect()
        } else {
            // Split by paragraphs
            text.split("\n\n")
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        }
    }

    fn segment_similarity(&self, a: &str, b: &str) -> f32 {
        if a == b {
            return 1.0;
        }

        // Simple word-based Jaccard similarity
        let a_lower = a.to_lowercase();
        let b_lower = b.to_lowercase();
        let words_a: std::collections::HashSet<_> = a_lower.split_whitespace().collect();
        let words_b: std::collections::HashSet<_> = b_lower.split_whitespace().collect();

        let intersection = words_a.intersection(&words_b).count();
        let union = words_a.union(&words_b).count();

        if union == 0 {
            0.0
        } else {
            intersection as f32 / union as f32
        }
    }

    fn calculate_similarity(
        &self,
        original: &str,
        modified: &str,
        changes: &[SemanticChange],
    ) -> f32 {
        if original == modified {
            return 1.0;
        }

        if original.is_empty() || modified.is_empty() {
            return 0.0;
        }

        let total_impact: f32 = changes.iter().map(|c| c.semantic_impact).sum();
        let max_impact = changes.len() as f32;

        if max_impact == 0.0 {
            1.0
        } else {
            (1.0 - total_impact / max_impact).clamp(0.0, 1.0)
        }
    }

    fn categorize_changes(&self, changes: &[SemanticChange]) -> Vec<ChangeCategory> {
        let mut by_type: HashMap<ChangeType, (usize, f32)> = HashMap::new();

        for change in changes {
            let entry = by_type.entry(change.change_type).or_insert((0, 0.0));
            entry.0 += 1;
            entry.1 += change.semantic_impact;
        }

        by_type
            .into_iter()
            .map(|(t, (count, impact))| ChangeCategory {
                name: format!("{:?}", t),
                count,
                total_impact: impact,
            })
            .collect()
    }

    fn explain_change(&self, change_type: ChangeType, original: &str, modified: &str) -> String {
        match change_type {
            ChangeType::Rephrase => format!(
                "Rephrased: \"{}\" -> \"{}\"",
                Self::truncate(original, 30),
                Self::truncate(modified, 30)
            ),
            ChangeType::SemanticChange => format!(
                "Meaning changed from \"{}\" to \"{}\"",
                Self::truncate(original, 30),
                Self::truncate(modified, 30)
            ),
            ChangeType::Addition => format!("Added: \"{}\"", Self::truncate(modified, 50)),
            ChangeType::Deletion => format!("Removed: \"{}\"", Self::truncate(original, 50)),
            _ => "Change detected".to_string(),
        }
    }

    fn generate_summary(&self, changes: &[SemanticChange], similarity: f32) -> String {
        let additions = changes
            .iter()
            .filter(|c| c.change_type == ChangeType::Addition)
            .count();
        let deletions = changes
            .iter()
            .filter(|c| c.change_type == ChangeType::Deletion)
            .count();
        let modifications = changes
            .iter()
            .filter(|c| {
                c.change_type == ChangeType::SemanticChange || c.change_type == ChangeType::Rephrase
            })
            .count();

        format!(
            "{:.0}% similar. {} additions, {} deletions, {} modifications.",
            similarity * 100.0,
            additions,
            deletions,
            modifications
        )
    }

    fn truncate(s: &str, max_len: usize) -> String {
        if s.len() <= max_len {
            s.to_string()
        } else {
            format!("{}...", &s[..max_len])
        }
    }

    /// Render diff as markdown.
    pub fn render_markdown(&self, diff: &SemanticDiff) -> String {
        let mut output = String::new();

        output.push_str(&format!("## Semantic Diff\n\n"));
        output.push_str(&format!(
            "**Similarity:** {:.1}%\n\n",
            diff.similarity * 100.0
        ));
        output.push_str(&format!("**Summary:** {}\n\n", diff.summary));

        output.push_str("### Changes\n\n");

        for change in &diff.changes {
            let icon = match change.change_type {
                ChangeType::Addition => "+",
                ChangeType::Deletion => "-",
                ChangeType::Rephrase => "~",
                ChangeType::SemanticChange => "!",
                _ => " ",
            };

            output.push_str(&format!("{} {}\n", icon, change.explanation));
        }

        output
    }

    /// Get diff history.
    pub async fn history(&self, limit: usize) -> Vec<SemanticDiff> {
        let history = self.history.read().await;
        history.iter().rev().take(limit).cloned().collect()
    }

    /// Get statistics.
    pub async fn stats(&self) -> SemDiffStats {
        let history = self.history.read().await;

        let avg_similarity = if !history.is_empty() {
            history.iter().map(|d| d.similarity).sum::<f32>() / history.len() as f32
        } else {
            0.0
        };

        let total_changes: usize = history.iter().map(|d| d.changes.len()).sum();

        SemDiffStats {
            total_diffs: history.len(),
            avg_similarity,
            total_changes,
        }
    }
}

/// Semantic diff statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemDiffStats {
    pub total_diffs: usize,
    pub avg_similarity: f32,
    pub total_changes: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_identical_texts() {
        let engine = SemDiffEngine::new(SemDiffConfig::default());
        let diff = engine.diff("Hello world", "Hello world").await.unwrap();
        assert_eq!(diff.similarity, 1.0);
    }

    #[tokio::test]
    async fn test_different_texts() {
        let engine = SemDiffEngine::new(SemDiffConfig::default());
        let diff = engine
            .diff("The cat sat on the mat.", "A dog lay on the rug.")
            .await
            .unwrap();

        assert!(diff.similarity < 1.0);
        assert!(!diff.changes.is_empty());
    }

    #[tokio::test]
    async fn test_additions_deletions() {
        let engine = SemDiffEngine::new(SemDiffConfig::default());
        let diff = engine
            .diff(
                "First sentence. Second sentence.",
                "First sentence. Third sentence. Fourth sentence.",
            )
            .await
            .unwrap();

        let additions = diff.changes_by_type(ChangeType::Addition);
        let deletions = diff.changes_by_type(ChangeType::Deletion);

        assert!(!additions.is_empty() || !deletions.is_empty());
    }

    #[tokio::test]
    async fn test_semantic_equivalence() {
        let engine = SemDiffEngine::new(SemDiffConfig::default());
        // Test with sentences that share more words to pass the similarity threshold
        let diff = engine
            .diff(
                "The brown fox jumps over the dog.",
                "The brown fox leaps over the dog.",
            )
            .await
            .unwrap();

        // These share most words and should be detected as similar
        assert!(diff.similarity > 0.5);
        assert!(diff.similarity < 1.0);
    }

    #[test]
    fn test_render_markdown() {
        let engine = SemDiffEngine::new(SemDiffConfig::default());

        let diff = SemanticDiff {
            id: Uuid::new_v4(),
            original: "Original".to_string(),
            modified: "Modified".to_string(),
            similarity: 0.75,
            changes: vec![SemanticChange {
                id: Uuid::new_v4(),
                change_type: ChangeType::Rephrase,
                original_segment: "Original".to_string(),
                modified_segment: "Modified".to_string(),
                segment_similarity: 0.5,
                semantic_impact: 0.5,
                explanation: "Changed wording".to_string(),
                original_start: 0,
                modified_start: 0,
            }],
            categories: Vec::new(),
            summary: "Test summary".to_string(),
            diffed_at: Utc::now(),
        };

        let md = engine.render_markdown(&diff);
        assert!(md.contains("75.0%"));
        assert!(md.contains("Changed wording"));
    }
}
