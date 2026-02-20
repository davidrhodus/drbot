//! Retrieval strategies for knowledge search.

use crate::embeddings::cosine_similarity;
use crate::store::KnowledgeStore;
use crate::{KnowledgeEntry, Result, SearchResult};
use uuid::Uuid;

/// Retrieval options.
#[derive(Debug, Clone)]
pub struct RetrievalOptions {
    /// Maximum results.
    pub limit: usize,
    /// Minimum score threshold.
    pub min_score: f32,
    /// Hybrid search weight (0 = pure semantic, 1 = pure keyword).
    pub hybrid_weight: f32,
    /// Search filters.
    pub filters: SearchFilters,
}

impl Default for RetrievalOptions {
    fn default() -> Self {
        Self {
            limit: 10,
            min_score: 0.0,
            hybrid_weight: 0.3,
            filters: SearchFilters::default(),
        }
    }
}

/// Search filters.
#[derive(Debug, Clone, Default)]
pub struct SearchFilters {
    /// Filter by source type.
    pub source_type: Option<String>,
    /// Filter by tags.
    pub tags: Option<Vec<String>>,
    /// Filter by document IDs.
    pub document_ids: Option<Vec<Uuid>>,
}

/// Retrieval result with metadata.
#[derive(Debug, Clone)]
pub struct RetrievalResult {
    /// The entry.
    pub entry: KnowledgeEntry,
    /// Semantic similarity score.
    pub semantic_score: f32,
    /// Keyword match score.
    pub keyword_score: f32,
    /// Combined score.
    pub combined_score: f32,
}

/// Knowledge retriever.
pub struct Retriever {
    // Could add caching, query expansion, etc.
}

impl Retriever {
    pub fn new() -> Self {
        Self {}
    }

    /// Search the knowledge store.
    pub async fn search(
        &self,
        store: &KnowledgeStore,
        query: &str,
        query_embedding: &[f32],
        options: RetrievalOptions,
    ) -> Result<Vec<SearchResult>> {
        // Get all entries (in production, use ANN index)
        let entries = store.get_all_entries().await?;

        // Also do keyword search
        let keyword_results = store
            .fts_search(query, options.limit * 2)
            .await
            .unwrap_or_default();
        let keyword_ids: std::collections::HashSet<Uuid> =
            keyword_results.iter().map(|e| e.id).collect();

        let mut results: Vec<(KnowledgeEntry, f32, f32)> = Vec::new();

        for entry in entries {
            // Apply filters
            if !self.matches_filters(&entry, &options.filters) {
                continue;
            }

            // Calculate semantic score
            let semantic_score = if let Some(embedding) = &entry.embedding {
                cosine_similarity(query_embedding, embedding)
            } else {
                0.0
            };

            // Calculate keyword score
            let keyword_score = if keyword_ids.contains(&entry.id) {
                // Simple: if it matched FTS, give it a score based on position
                let pos = keyword_results
                    .iter()
                    .position(|e| e.id == entry.id)
                    .unwrap_or(keyword_results.len());
                1.0 - (pos as f32 / keyword_results.len() as f32)
            } else {
                0.0
            };

            // Combined score
            let combined = semantic_score * (1.0 - options.hybrid_weight)
                + keyword_score * options.hybrid_weight;

            if combined >= options.min_score {
                results.push((entry, semantic_score, keyword_score));
            }
        }

        // Sort by combined score
        results.sort_by(|a, b| {
            let score_a = a.1 * (1.0 - options.hybrid_weight) + a.2 * options.hybrid_weight;
            let score_b = b.1 * (1.0 - options.hybrid_weight) + b.2 * options.hybrid_weight;
            score_b.partial_cmp(&score_a).unwrap()
        });

        // Take top results
        let search_results: Vec<SearchResult> = results
            .into_iter()
            .take(options.limit)
            .map(|(entry, semantic_score, keyword_score)| {
                let combined = semantic_score * (1.0 - options.hybrid_weight)
                    + keyword_score * options.hybrid_weight;
                SearchResult {
                    entry,
                    score: combined,
                    highlights: vec![], // Could add highlighting
                }
            })
            .collect();

        Ok(search_results)
    }

    /// Check if entry matches filters.
    fn matches_filters(&self, entry: &KnowledgeEntry, filters: &SearchFilters) -> bool {
        // Filter by source type
        if let Some(source_type) = &filters.source_type {
            if entry.metadata.source_type.as_ref() != Some(source_type) {
                return false;
            }
        }

        // Filter by document IDs
        if let Some(doc_ids) = &filters.document_ids {
            if !doc_ids.contains(&entry.document_id) {
                return false;
            }
        }

        // Filter by tags
        if let Some(tags) = &filters.tags {
            let entry_tags: std::collections::HashSet<_> = entry.metadata.tags.iter().collect();
            if !tags.iter().any(|t| entry_tags.contains(t)) {
                return false;
            }
        }

        true
    }
}

impl Default for Retriever {
    fn default() -> Self {
        Self::new()
    }
}

/// Query augmentation to improve retrieval.
#[allow(dead_code)]
pub struct QueryAugmenter;

impl QueryAugmenter {
    /// Augment a query with synonyms and related terms.
    #[allow(dead_code)]
    pub fn augment(query: &str) -> Vec<String> {
        let mut queries = vec![query.to_string()];

        // Add variations (simple for now)
        // In production, use an LLM or synonym database

        // Remove common stop words for alternative query
        let stop_words = [
            "the", "a", "an", "is", "are", "was", "were", "be", "been", "being",
        ];
        let filtered: Vec<&str> = query
            .split_whitespace()
            .filter(|w| !stop_words.contains(&w.to_lowercase().as_str()))
            .collect();

        if filtered.len() < query.split_whitespace().count() {
            queries.push(filtered.join(" "));
        }

        queries
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_augmenter() {
        let queries = QueryAugmenter::augment("what is the weather");
        assert!(queries.len() >= 1);
        assert!(queries.contains(&"what is the weather".to_string()));
    }

    #[test]
    fn test_retrieval_options_default() {
        let opts = RetrievalOptions::default();
        assert_eq!(opts.limit, 10);
        assert_eq!(opts.hybrid_weight, 0.3);
    }
}
