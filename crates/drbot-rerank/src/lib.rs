//! ML-based result re-ranking for relevance.
//!
//! This crate provides:
//! - Cross-encoder re-ranking
//! - Learning to rank
//! - Personalized ranking
//! - Diversity optimization

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Rerank errors.
#[derive(Debug, Error)]
pub enum RerankError {
    #[error("Reranking failed: {0}")]
    RerankFailed(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Model error: {0}")]
    ModelError(String),
}

/// Result type for rerank operations.
pub type Result<T> = std::result::Result<T, RerankError>;

/// Document to rerank.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RerankDocument {
    /// Document identifier.
    pub id: String,
    /// Document text.
    pub text: String,
    /// Original score.
    pub original_score: f64,
    /// Metadata.
    pub metadata: HashMap<String, String>,
}

/// Reranked result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RerankResult {
    /// Document identifier.
    pub id: String,
    /// Document text.
    pub text: String,
    /// Rerank score.
    pub score: f64,
    /// Original score.
    pub original_score: f64,
    /// Rank change.
    pub rank_change: i32,
    /// Relevance breakdown.
    pub relevance: RelevanceBreakdown,
    /// Metadata.
    pub metadata: HashMap<String, String>,
}

/// Relevance score breakdown.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RelevanceBreakdown {
    /// Semantic relevance.
    pub semantic: f64,
    /// Lexical overlap.
    pub lexical: f64,
    /// Query coverage.
    pub query_coverage: f64,
    /// Freshness.
    pub freshness: f64,
    /// Diversity.
    pub diversity: f64,
}

/// Rerank configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RerankConfig {
    /// Maximum documents to rerank.
    pub max_docs: usize,
    /// Diversity factor (0-1).
    pub diversity_factor: f64,
    /// Freshness weight.
    pub freshness_weight: f64,
    /// Personalization weight.
    pub personalization_weight: f64,
    /// Model to use.
    pub model: String,
}

impl Default for RerankConfig {
    fn default() -> Self {
        Self {
            max_docs: 100,
            diversity_factor: 0.1,
            freshness_weight: 0.1,
            personalization_weight: 0.0,
            model: "default".to_string(),
        }
    }
}

/// User preferences for personalized ranking.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UserPreferences {
    /// Preferred topics.
    pub topics: Vec<String>,
    /// Preferred sources.
    pub sources: Vec<String>,
    /// Interaction history.
    pub history: Vec<String>,
}

/// Reranking provider.
#[async_trait]
pub trait RerankProvider: Send + Sync {
    /// Score document relevance to query.
    async fn score(&self, query: &str, document: &str) -> Result<f64>;

    /// Batch score documents.
    async fn batch_score(&self, query: &str, documents: &[String]) -> Result<Vec<f64>>;

    /// Get model info.
    fn model_info(&self) -> &str;
}

/// The reranking engine.
pub struct Reranker {
    /// Reranking provider.
    provider: Arc<dyn RerankProvider>,
    /// Configuration.
    config: RerankConfig,
    /// User preferences.
    preferences: Arc<RwLock<HashMap<String, UserPreferences>>>,
    /// Stats.
    stats: Arc<RwLock<RerankStats>>,
}

/// Reranking statistics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RerankStats {
    /// Total rerank calls.
    pub total_calls: usize,
    /// Total documents reranked.
    pub total_docs: usize,
    /// Average rank change.
    pub avg_rank_change: f64,
    /// Average score improvement.
    pub avg_score_improvement: f64,
}

impl Reranker {
    /// Create a new reranker.
    pub fn new(provider: Arc<dyn RerankProvider>, config: RerankConfig) -> Self {
        Self {
            provider,
            config,
            preferences: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(RerankStats::default())),
        }
    }

    /// Rerank documents.
    pub async fn rerank(
        &self,
        query: &str,
        documents: Vec<RerankDocument>,
    ) -> Result<Vec<RerankResult>> {
        if documents.is_empty() {
            return Ok(vec![]);
        }

        let docs_to_rerank: Vec<_> = documents.iter().take(self.config.max_docs).collect();

        // Get rerank scores
        let texts: Vec<String> = docs_to_rerank.iter().map(|d| d.text.clone()).collect();
        let scores = self.provider.batch_score(query, &texts).await?;

        // Create results with original positions
        let mut results: Vec<(usize, RerankResult)> = docs_to_rerank
            .iter()
            .zip(scores.iter())
            .enumerate()
            .map(|(original_rank, (doc, &score))| {
                (
                    original_rank,
                    RerankResult {
                        id: doc.id.clone(),
                        text: doc.text.clone(),
                        score,
                        original_score: doc.original_score,
                        rank_change: 0,
                        relevance: RelevanceBreakdown {
                            semantic: score,
                            lexical: self.calculate_lexical_score(query, &doc.text),
                            query_coverage: self.calculate_coverage(query, &doc.text),
                            freshness: 0.0,
                            diversity: 0.0,
                        },
                        metadata: doc.metadata.clone(),
                    },
                )
            })
            .collect();

        // Sort by score
        results.sort_by(|a, b| b.1.score.partial_cmp(&a.1.score).unwrap());

        // Apply diversity if configured
        if self.config.diversity_factor > 0.0 {
            self.apply_diversity(&mut results);
        }

        // Calculate rank changes
        let mut final_results: Vec<RerankResult> = results
            .into_iter()
            .enumerate()
            .map(|(new_rank, (original_rank, mut result))| {
                result.rank_change = original_rank as i32 - new_rank as i32;
                result
            })
            .collect();

        // Update stats
        self.update_stats(&final_results).await;

        Ok(final_results)
    }

    /// Calculate lexical overlap score.
    fn calculate_lexical_score(&self, query: &str, doc: &str) -> f64 {
        let query_lower = query.to_lowercase();
        let doc_lower = doc.to_lowercase();
        let query_terms: std::collections::HashSet<&str> = query_lower.split_whitespace().collect();
        let doc_terms: std::collections::HashSet<&str> = doc_lower.split_whitespace().collect();

        let intersection = query_terms.intersection(&doc_terms).count();
        if query_terms.is_empty() {
            0.0
        } else {
            intersection as f64 / query_terms.len() as f64
        }
    }

    /// Calculate query coverage.
    fn calculate_coverage(&self, query: &str, doc: &str) -> f64 {
        let query_lower = query.to_lowercase();
        let query_terms: Vec<&str> = query_lower.split_whitespace().collect();
        let doc_lower = doc.to_lowercase();

        let found = query_terms
            .iter()
            .filter(|t| doc_lower.contains(*t))
            .count();

        if query_terms.is_empty() {
            0.0
        } else {
            found as f64 / query_terms.len() as f64
        }
    }

    /// Apply diversity to results.
    fn apply_diversity(&self, results: &mut [(usize, RerankResult)]) {
        if results.len() < 2 {
            return;
        }

        // Simple diversity: penalize similar consecutive results
        for i in 1..results.len() {
            let similarity = self.text_similarity(&results[i - 1].1.text, &results[i].1.text);
            if similarity > 0.8 {
                results[i].1.score *= 1.0 - (self.config.diversity_factor * similarity);
                results[i].1.relevance.diversity = -similarity * self.config.diversity_factor;
            }
        }

        // Re-sort after diversity adjustment
        results.sort_by(|a, b| b.1.score.partial_cmp(&a.1.score).unwrap());
    }

    /// Calculate text similarity.
    fn text_similarity(&self, a: &str, b: &str) -> f64 {
        let a_lower = a.to_lowercase();
        let b_lower = b.to_lowercase();
        let a_words: std::collections::HashSet<&str> = a_lower.split_whitespace().collect();
        let b_words: std::collections::HashSet<&str> = b_lower.split_whitespace().collect();

        let intersection = a_words.intersection(&b_words).count();
        let union = a_words.union(&b_words).count();

        if union == 0 {
            0.0
        } else {
            intersection as f64 / union as f64
        }
    }

    /// Update statistics.
    async fn update_stats(&self, results: &[RerankResult]) {
        let mut stats = self.stats.write().await;
        stats.total_calls += 1;
        stats.total_docs += results.len();

        let total_rank_change: i32 = results.iter().map(|r| r.rank_change.abs()).sum();
        let avg_change = total_rank_change as f64 / results.len().max(1) as f64;

        let total_improvement: f64 = results.iter().map(|r| r.score - r.original_score).sum();
        let avg_improvement = total_improvement / results.len().max(1) as f64;

        // Running average
        let n = stats.total_calls as f64;
        stats.avg_rank_change = ((stats.avg_rank_change * (n - 1.0)) + avg_change) / n;
        stats.avg_score_improvement =
            ((stats.avg_score_improvement * (n - 1.0)) + avg_improvement) / n;
    }

    /// Set user preferences for personalized ranking.
    pub async fn set_preferences(&self, user_id: &str, prefs: UserPreferences) {
        let mut preferences = self.preferences.write().await;
        preferences.insert(user_id.to_string(), prefs);
    }

    /// Rerank with personalization.
    pub async fn rerank_personalized(
        &self,
        query: &str,
        documents: Vec<RerankDocument>,
        user_id: &str,
    ) -> Result<Vec<RerankResult>> {
        let mut results = self.rerank(query, documents).await?;

        let preferences = self.preferences.read().await;
        if let Some(prefs) = preferences.get(user_id) {
            // Boost results matching user preferences
            for result in &mut results {
                let mut boost = 0.0;

                // Topic boost
                for topic in &prefs.topics {
                    if result.text.to_lowercase().contains(&topic.to_lowercase()) {
                        boost += self.config.personalization_weight * 0.5;
                    }
                }

                // Source boost
                if let Some(source) = result.metadata.get("source") {
                    if prefs.sources.contains(source) {
                        boost += self.config.personalization_weight * 0.5;
                    }
                }

                result.score += boost;
            }

            // Re-sort
            results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        }

        Ok(results)
    }

    /// Get statistics.
    pub async fn get_stats(&self) -> RerankStats {
        let stats = self.stats.read().await;
        stats.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockProvider;

    #[async_trait]
    impl RerankProvider for MockProvider {
        async fn score(&self, query: &str, document: &str) -> Result<f64> {
            // Simple scoring based on word overlap
            let query_lower = query.to_lowercase();
            let doc_lower = document.to_lowercase();
            let query_words: std::collections::HashSet<_> =
                query_lower.split_whitespace().collect();
            let doc_words: std::collections::HashSet<_> = doc_lower.split_whitespace().collect();
            let overlap = query_words.intersection(&doc_words).count();
            Ok(overlap as f64 / query_words.len().max(1) as f64)
        }

        async fn batch_score(&self, query: &str, documents: &[String]) -> Result<Vec<f64>> {
            let mut scores = Vec::new();
            for doc in documents {
                scores.push(self.score(query, doc).await?);
            }
            Ok(scores)
        }

        fn model_info(&self) -> &str {
            "mock-reranker"
        }
    }

    #[tokio::test]
    async fn test_rerank() {
        let provider = Arc::new(MockProvider);
        let reranker = Reranker::new(provider, RerankConfig::default());

        let documents = vec![
            RerankDocument {
                id: "1".to_string(),
                text: "Python is a programming language".to_string(),
                original_score: 0.9,
                metadata: HashMap::new(),
            },
            RerankDocument {
                id: "2".to_string(),
                text: "Rust is a systems programming language".to_string(),
                original_score: 0.8,
                metadata: HashMap::new(),
            },
        ];

        let results = reranker
            .rerank("Rust programming", documents)
            .await
            .unwrap();

        assert!(!results.is_empty());
        // Rust doc should be ranked higher for "Rust programming" query
        assert_eq!(results[0].id, "2");
    }

    #[tokio::test]
    async fn test_personalized_rerank() {
        let provider = Arc::new(MockProvider);
        let reranker = Reranker::new(
            provider,
            RerankConfig {
                personalization_weight: 0.2,
                ..Default::default()
            },
        );

        reranker
            .set_preferences(
                "user1",
                UserPreferences {
                    topics: vec!["python".to_string()],
                    sources: vec![],
                    history: vec![],
                },
            )
            .await;

        let documents = vec![
            RerankDocument {
                id: "1".to_string(),
                text: "Python programming".to_string(),
                original_score: 0.7,
                metadata: HashMap::new(),
            },
            RerankDocument {
                id: "2".to_string(),
                text: "Java programming".to_string(),
                original_score: 0.8,
                metadata: HashMap::new(),
            },
        ];

        let results = reranker
            .rerank_personalized("programming", documents, "user1")
            .await
            .unwrap();

        // Python should be boosted for user1
        assert_eq!(results[0].id, "1");
    }

    #[tokio::test]
    async fn test_stats() {
        let provider = Arc::new(MockProvider);
        let reranker = Reranker::new(provider, RerankConfig::default());

        let documents = vec![RerankDocument {
            id: "1".to_string(),
            text: "Test document".to_string(),
            original_score: 0.5,
            metadata: HashMap::new(),
        }];

        reranker.rerank("test", documents).await.unwrap();

        let stats = reranker.get_stats().await;
        assert_eq!(stats.total_calls, 1);
        assert_eq!(stats.total_docs, 1);
    }
}
