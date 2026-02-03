//! Semantic + keyword + fuzzy combined search.
//!
//! This crate provides:
//! - Hybrid search combining multiple strategies
//! - Weighted scoring
//! - Result fusion
//! - Query understanding

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Search errors.
#[derive(Debug, Error)]
pub enum SearchError {
    #[error("Search failed: {0}")]
    SearchFailed(String),

    #[error("Indexing failed: {0}")]
    IndexingFailed(String),

    #[error("Invalid query: {0}")]
    InvalidQuery(String),
}

/// Result type for search operations.
pub type Result<T> = std::result::Result<T, SearchError>;

/// Search result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    /// Document identifier.
    pub id: String,
    /// Document content.
    pub content: String,
    /// Overall score.
    pub score: f64,
    /// Score breakdown.
    pub score_breakdown: ScoreBreakdown,
    /// Highlights.
    pub highlights: Vec<String>,
    /// Metadata.
    pub metadata: HashMap<String, String>,
}

/// Score breakdown by search type.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScoreBreakdown {
    /// Semantic similarity score.
    pub semantic: f64,
    /// Keyword match score.
    pub keyword: f64,
    /// Fuzzy match score.
    pub fuzzy: f64,
    /// Recency boost.
    pub recency: f64,
    /// Popularity boost.
    pub popularity: f64,
}

/// Search query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchQuery {
    /// Query text.
    pub text: String,
    /// Search strategy weights.
    pub weights: SearchWeights,
    /// Filters.
    pub filters: Vec<Filter>,
    /// Maximum results.
    pub limit: usize,
    /// Offset.
    pub offset: usize,
    /// Enable fuzzy matching.
    pub fuzzy: bool,
    /// Minimum score threshold.
    pub min_score: Option<f64>,
}

/// Search strategy weights.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchWeights {
    /// Semantic search weight.
    pub semantic: f64,
    /// Keyword search weight.
    pub keyword: f64,
    /// Fuzzy search weight.
    pub fuzzy: f64,
}

impl Default for SearchWeights {
    fn default() -> Self {
        Self {
            semantic: 0.6,
            keyword: 0.3,
            fuzzy: 0.1,
        }
    }
}

/// Search filter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Filter {
    /// Field to filter on.
    pub field: String,
    /// Filter operation.
    pub op: FilterOp,
    /// Value.
    pub value: String,
}

/// Filter operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FilterOp {
    Equals,
    NotEquals,
    Contains,
    StartsWith,
    EndsWith,
    GreaterThan,
    LessThan,
    In,
}

/// Document for indexing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    /// Document identifier.
    pub id: String,
    /// Content.
    pub content: String,
    /// Title.
    pub title: Option<String>,
    /// Metadata.
    pub metadata: HashMap<String, String>,
    /// Embedding (if pre-computed).
    pub embedding: Option<Vec<f32>>,
    /// Created at.
    pub created_at: DateTime<Utc>,
    /// Updated at.
    pub updated_at: DateTime<Utc>,
}

/// Search provider trait.
#[async_trait]
pub trait SearchProvider: Send + Sync {
    /// Semantic search.
    async fn semantic_search(
        &self,
        query: &str,
        embedding: &[f32],
        limit: usize,
    ) -> Result<Vec<(String, f64)>>;

    /// Keyword search.
    async fn keyword_search(&self, query: &str, limit: usize) -> Result<Vec<(String, f64)>>;

    /// Fuzzy search.
    async fn fuzzy_search(&self, query: &str, limit: usize) -> Result<Vec<(String, f64)>>;

    /// Generate embedding.
    async fn embed(&self, text: &str) -> Result<Vec<f32>>;
}

/// The hybrid search engine.
pub struct HybridSearch {
    /// Search provider.
    provider: Arc<dyn SearchProvider>,
    /// Document store.
    documents: Arc<RwLock<HashMap<String, Document>>>,
    /// Default weights.
    default_weights: SearchWeights,
}

impl HybridSearch {
    /// Create a new hybrid search engine.
    pub fn new(provider: Arc<dyn SearchProvider>) -> Self {
        Self {
            provider,
            documents: Arc::new(RwLock::new(HashMap::new())),
            default_weights: SearchWeights::default(),
        }
    }

    /// Set default weights.
    pub fn with_weights(mut self, weights: SearchWeights) -> Self {
        self.default_weights = weights;
        self
    }

    /// Index a document.
    pub async fn index(&self, mut document: Document) -> Result<String> {
        // Generate embedding if not provided
        if document.embedding.is_none() {
            let embedding = self.provider.embed(&document.content).await?;
            document.embedding = Some(embedding);
        }

        let id = document.id.clone();
        let mut documents = self.documents.write().await;
        documents.insert(id.clone(), document);

        Ok(id)
    }

    /// Remove a document.
    pub async fn remove(&self, id: &str) -> Result<()> {
        let mut documents = self.documents.write().await;
        documents.remove(id);
        Ok(())
    }

    /// Search with hybrid strategy.
    pub async fn search(&self, query: SearchQuery) -> Result<Vec<SearchResult>> {
        let weights = query.weights.clone();

        // Get query embedding
        let query_embedding = self.provider.embed(&query.text).await?;

        // Run all searches in parallel
        let (semantic_results, keyword_results, fuzzy_results) = tokio::join!(
            self.provider
                .semantic_search(&query.text, &query_embedding, query.limit * 2),
            self.provider.keyword_search(&query.text, query.limit * 2),
            async {
                if query.fuzzy {
                    self.provider
                        .fuzzy_search(&query.text, query.limit * 2)
                        .await
                } else {
                    Ok(vec![])
                }
            }
        );

        let semantic_results = semantic_results?;
        let keyword_results = keyword_results?;
        let fuzzy_results = fuzzy_results?;

        // Fuse results
        let fused = self
            .fuse_results(
                &semantic_results,
                &keyword_results,
                &fuzzy_results,
                &weights,
            )
            .await;

        // Apply filters and get documents
        let documents = self.documents.read().await;
        let mut results: Vec<SearchResult> = fused
            .into_iter()
            .filter_map(|(id, scores)| {
                documents
                    .get(&id)
                    .map(|doc| {
                        // Check filters
                        if !self.matches_filters(doc, &query.filters) {
                            return None;
                        }

                        let total_weight = weights.semantic + weights.keyword + weights.fuzzy;
                        let overall_score = (scores.semantic * weights.semantic
                            + scores.keyword * weights.keyword
                            + scores.fuzzy * weights.fuzzy)
                            / total_weight;

                        // Check minimum score
                        if let Some(min) = query.min_score {
                            if overall_score < min {
                                return None;
                            }
                        }

                        Some(SearchResult {
                            id: id.clone(),
                            content: doc.content.clone(),
                            score: overall_score,
                            score_breakdown: scores,
                            highlights: self.generate_highlights(&doc.content, &query.text),
                            metadata: doc.metadata.clone(),
                        })
                    })
                    .flatten()
            })
            .collect();

        // Sort by score
        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());

        // Apply offset and limit
        let results: Vec<_> = results
            .into_iter()
            .skip(query.offset)
            .take(query.limit)
            .collect();

        Ok(results)
    }

    /// Fuse results from different search strategies.
    async fn fuse_results(
        &self,
        semantic: &[(String, f64)],
        keyword: &[(String, f64)],
        fuzzy: &[(String, f64)],
        _weights: &SearchWeights,
    ) -> Vec<(String, ScoreBreakdown)> {
        let mut scores: HashMap<String, ScoreBreakdown> = HashMap::new();

        for (id, score) in semantic {
            scores.entry(id.clone()).or_default().semantic = *score;
        }

        for (id, score) in keyword {
            scores.entry(id.clone()).or_default().keyword = *score;
        }

        for (id, score) in fuzzy {
            scores.entry(id.clone()).or_default().fuzzy = *score;
        }

        scores.into_iter().collect()
    }

    /// Check if document matches filters.
    fn matches_filters(&self, doc: &Document, filters: &[Filter]) -> bool {
        for filter in filters {
            let value = doc.metadata.get(&filter.field);
            let matches = match (&filter.op, value) {
                (FilterOp::Equals, Some(v)) => v == &filter.value,
                (FilterOp::Equals, None) => false,
                (FilterOp::NotEquals, Some(v)) => v != &filter.value,
                (FilterOp::NotEquals, None) => true,
                (FilterOp::Contains, Some(v)) => v.contains(&filter.value),
                (FilterOp::Contains, None) => false,
                (FilterOp::StartsWith, Some(v)) => v.starts_with(&filter.value),
                (FilterOp::StartsWith, None) => false,
                (FilterOp::EndsWith, Some(v)) => v.ends_with(&filter.value),
                (FilterOp::EndsWith, None) => false,
                _ => true,
            };
            if !matches {
                return false;
            }
        }
        true
    }

    /// Generate highlights for search results.
    fn generate_highlights(&self, content: &str, query: &str) -> Vec<String> {
        let query_terms: Vec<&str> = query.split_whitespace().collect();
        let sentences: Vec<&str> = content.split('.').collect();

        sentences
            .into_iter()
            .filter(|s| {
                query_terms
                    .iter()
                    .any(|t| s.to_lowercase().contains(&t.to_lowercase()))
            })
            .take(3)
            .map(|s| s.trim().to_string())
            .collect()
    }

    /// Simple query helper.
    pub async fn simple_search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        self.search(SearchQuery {
            text: query.to_string(),
            weights: self.default_weights.clone(),
            filters: vec![],
            limit,
            offset: 0,
            fuzzy: true,
            min_score: None,
        })
        .await
    }

    /// Get document by ID.
    pub async fn get_document(&self, id: &str) -> Option<Document> {
        let documents = self.documents.read().await;
        documents.get(id).cloned()
    }

    /// Get document count.
    pub async fn document_count(&self) -> usize {
        let documents = self.documents.read().await;
        documents.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockProvider;

    #[async_trait]
    impl SearchProvider for MockProvider {
        async fn semantic_search(
            &self,
            _query: &str,
            _embedding: &[f32],
            _limit: usize,
        ) -> Result<Vec<(String, f64)>> {
            Ok(vec![("doc1".to_string(), 0.9), ("doc2".to_string(), 0.7)])
        }

        async fn keyword_search(&self, query: &str, _limit: usize) -> Result<Vec<(String, f64)>> {
            if query.contains("rust") {
                Ok(vec![("doc1".to_string(), 1.0)])
            } else {
                Ok(vec![])
            }
        }

        async fn fuzzy_search(&self, _query: &str, _limit: usize) -> Result<Vec<(String, f64)>> {
            Ok(vec![("doc1".to_string(), 0.8)])
        }

        async fn embed(&self, _text: &str) -> Result<Vec<f32>> {
            Ok(vec![0.1, 0.2, 0.3, 0.4])
        }
    }

    async fn setup_engine() -> HybridSearch {
        let provider = Arc::new(MockProvider);
        let engine = HybridSearch::new(provider);

        let doc1 = Document {
            id: "doc1".to_string(),
            content: "Rust is a systems programming language.".to_string(),
            title: Some("Rust".to_string()),
            metadata: HashMap::from([("type".to_string(), "article".to_string())]),
            embedding: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let doc2 = Document {
            id: "doc2".to_string(),
            content: "Python is a scripting language.".to_string(),
            title: Some("Python".to_string()),
            metadata: HashMap::new(),
            embedding: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        engine.index(doc1).await.unwrap();
        engine.index(doc2).await.unwrap();

        engine
    }

    #[tokio::test]
    async fn test_index_and_search() {
        let engine = setup_engine().await;

        let results = engine.simple_search("rust programming", 10).await.unwrap();
        assert!(!results.is_empty());
    }

    #[tokio::test]
    async fn test_search_with_filters() {
        let engine = setup_engine().await;

        let results = engine
            .search(SearchQuery {
                text: "programming".to_string(),
                weights: SearchWeights::default(),
                filters: vec![Filter {
                    field: "type".to_string(),
                    op: FilterOp::Equals,
                    value: "article".to_string(),
                }],
                limit: 10,
                offset: 0,
                fuzzy: true,
                min_score: None,
            })
            .await
            .unwrap();

        // Only doc1 has type=article
        for result in &results {
            assert!(result.metadata.get("type") == Some(&"article".to_string()));
        }
    }

    #[tokio::test]
    async fn test_document_count() {
        let engine = setup_engine().await;
        assert_eq!(engine.document_count().await, 2);
    }

    #[tokio::test]
    async fn test_remove_document() {
        let engine = setup_engine().await;

        engine.remove("doc1").await.unwrap();
        assert_eq!(engine.document_count().await, 1);
    }
}
