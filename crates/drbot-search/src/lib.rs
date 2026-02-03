//! Universal semantic search for drbot.
//!
//! Search across all conversations and documents.
//!
//! # Features
//!
//! - Semantic search with embeddings
//! - Full-text search
//! - Faceted search
//! - Search across channels
//! - Search history

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Search result type.
pub type Result<T> = std::result::Result<T, SearchError>;

/// Search errors.
#[derive(Debug, thiserror::Error)]
pub enum SearchError {
    #[error("Index not found: {0}")]
    IndexNotFound(String),
    #[error("Embedding failed: {0}")]
    EmbeddingFailed(String),
    #[error("Query invalid: {0}")]
    InvalidQuery(String),
    #[error("Search failed: {0}")]
    SearchFailed(String),
}

/// Searchable document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchDocument {
    /// Document ID.
    pub id: Uuid,
    /// Document content.
    pub content: String,
    /// Document title.
    pub title: Option<String>,
    /// Source type.
    pub source: DocumentSource,
    /// Source ID (conversation, channel, etc.).
    pub source_id: String,
    /// Embedding vector.
    pub embedding: Option<Vec<f32>>,
    /// Metadata.
    pub metadata: HashMap<String, serde_json::Value>,
    /// Created at.
    pub created_at: DateTime<Utc>,
    /// Updated at.
    pub updated_at: DateTime<Utc>,
}

impl SearchDocument {
    /// Create a new document.
    pub fn new(content: &str, source: DocumentSource, source_id: &str) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            content: content.to_string(),
            title: None,
            source,
            source_id: source_id.to_string(),
            embedding: None,
            metadata: HashMap::new(),
            created_at: now,
            updated_at: now,
        }
    }

    /// Set title.
    pub fn with_title(mut self, title: &str) -> Self {
        self.title = Some(title.to_string());
        self
    }

    /// Add metadata.
    pub fn with_metadata(mut self, key: &str, value: serde_json::Value) -> Self {
        self.metadata.insert(key.to_string(), value);
        self
    }

    /// Set embedding.
    pub fn with_embedding(mut self, embedding: Vec<f32>) -> Self {
        self.embedding = Some(embedding);
        self
    }
}

/// Document source type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentSource {
    Conversation,
    Message,
    File,
    Memory,
    Note,
    Bookmark,
    External,
}

/// Search query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchQuery {
    /// Query text.
    pub text: String,
    /// Search mode.
    pub mode: SearchMode,
    /// Filters.
    pub filters: Vec<SearchFilter>,
    /// Number of results.
    pub limit: usize,
    /// Offset for pagination.
    pub offset: usize,
    /// Sort order.
    pub sort: SortOrder,
    /// Include snippets.
    pub include_snippets: bool,
    /// Highlight matches.
    pub highlight: bool,
}

impl SearchQuery {
    /// Create a new query.
    pub fn new(text: &str) -> Self {
        Self {
            text: text.to_string(),
            mode: SearchMode::Hybrid,
            filters: Vec::new(),
            limit: 10,
            offset: 0,
            sort: SortOrder::Relevance,
            include_snippets: true,
            highlight: true,
        }
    }

    /// Set search mode.
    pub fn with_mode(mut self, mode: SearchMode) -> Self {
        self.mode = mode;
        self
    }

    /// Add a filter.
    pub fn with_filter(mut self, filter: SearchFilter) -> Self {
        self.filters.push(filter);
        self
    }

    /// Set limit.
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }

    /// Set offset.
    pub fn with_offset(mut self, offset: usize) -> Self {
        self.offset = offset;
        self
    }
}

/// Search mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchMode {
    /// Keyword/full-text search.
    Keyword,
    /// Semantic/vector search.
    Semantic,
    /// Hybrid (both).
    Hybrid,
    /// Exact match.
    Exact,
}

/// Search filter.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SearchFilter {
    /// Filter by source.
    Source { sources: Vec<DocumentSource> },
    /// Filter by date range.
    DateRange {
        start: Option<DateTime<Utc>>,
        end: Option<DateTime<Utc>>,
    },
    /// Filter by source ID.
    SourceId { id: String },
    /// Filter by metadata.
    Metadata {
        key: String,
        value: serde_json::Value,
    },
    /// Filter by has embedding.
    HasEmbedding { required: bool },
}

/// Sort order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortOrder {
    Relevance,
    DateAsc,
    DateDesc,
    TitleAsc,
    TitleDesc,
}

/// Search result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    /// Query ID.
    pub query_id: Uuid,
    /// Query text.
    pub query: String,
    /// Total matches.
    pub total: usize,
    /// Results.
    pub hits: Vec<SearchHit>,
    /// Facets.
    pub facets: HashMap<String, Vec<Facet>>,
    /// Search time in ms.
    pub took_ms: u64,
}

/// Individual search hit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    /// Document.
    pub document: SearchDocument,
    /// Relevance score.
    pub score: f32,
    /// Matched snippets.
    pub snippets: Vec<Snippet>,
    /// Highlighted content.
    pub highlighted: Option<String>,
}

/// Text snippet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snippet {
    /// Snippet text.
    pub text: String,
    /// Start position in document.
    pub start: usize,
    /// End position.
    pub end: usize,
    /// Matched terms.
    pub matched_terms: Vec<String>,
}

/// Facet value.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Facet {
    /// Facet value.
    pub value: String,
    /// Count.
    pub count: usize,
}

/// Search configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchConfig {
    /// Enable semantic search.
    pub semantic_enabled: bool,
    /// Embedding dimension.
    pub embedding_dim: usize,
    /// Similarity threshold.
    pub similarity_threshold: f32,
    /// Snippet length.
    pub snippet_length: usize,
    /// Max results.
    pub max_results: usize,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            semantic_enabled: true,
            embedding_dim: 1536,
            similarity_threshold: 0.7,
            snippet_length: 200,
            max_results: 100,
        }
    }
}

/// Trait for embedding providers.
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// Generate embedding for text.
    async fn embed(&self, text: &str) -> Result<Vec<f32>>;

    /// Generate embeddings for multiple texts.
    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;
}

/// Search index.
pub struct SearchIndex {
    config: SearchConfig,
    documents: Arc<RwLock<HashMap<Uuid, SearchDocument>>>,
    inverted_index: Arc<RwLock<HashMap<String, Vec<Uuid>>>>,
}

impl SearchIndex {
    /// Create a new search index.
    pub fn new(config: SearchConfig) -> Self {
        Self {
            config,
            documents: Arc::new(RwLock::new(HashMap::new())),
            inverted_index: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Add a document to the index.
    pub async fn add(&self, document: SearchDocument) {
        let id = document.id;

        // Update inverted index
        let terms = self.tokenize(&document.content);
        let mut index = self.inverted_index.write().await;
        for term in terms {
            index.entry(term).or_default().push(id);
        }

        // Store document
        self.documents.write().await.insert(id, document);
    }

    /// Remove a document from the index.
    pub async fn remove(&self, id: Uuid) -> Option<SearchDocument> {
        let doc = self.documents.write().await.remove(&id);

        if let Some(ref document) = doc {
            let terms = self.tokenize(&document.content);
            let mut index = self.inverted_index.write().await;
            for term in terms {
                if let Some(ids) = index.get_mut(&term) {
                    ids.retain(|&doc_id| doc_id != id);
                }
            }
        }

        doc
    }

    /// Update a document.
    pub async fn update(&self, document: SearchDocument) {
        self.remove(document.id).await;
        self.add(document).await;
    }

    /// Search the index.
    pub async fn search(&self, query: SearchQuery) -> Result<SearchResult> {
        let start = std::time::Instant::now();
        let query_id = Uuid::new_v4();

        let hits = match query.mode {
            SearchMode::Keyword => self.keyword_search(&query).await?,
            SearchMode::Semantic => self.semantic_search(&query).await?,
            SearchMode::Hybrid => self.hybrid_search(&query).await?,
            SearchMode::Exact => self.exact_search(&query).await?,
        };

        // Apply filters
        let filtered: Vec<_> = hits
            .into_iter()
            .filter(|hit| self.matches_filters(&hit.document, &query.filters))
            .collect();

        // Sort results
        let mut sorted = filtered;
        self.sort_results(&mut sorted, query.sort);

        // Paginate
        let total = sorted.len();
        let paginated: Vec<_> = sorted
            .into_iter()
            .skip(query.offset)
            .take(query.limit)
            .collect();

        // Generate facets
        let facets = self.generate_facets(&paginated).await;

        Ok(SearchResult {
            query_id,
            query: query.text,
            total,
            hits: paginated,
            facets,
            took_ms: start.elapsed().as_millis() as u64,
        })
    }

    async fn keyword_search(&self, query: &SearchQuery) -> Result<Vec<SearchHit>> {
        let terms = self.tokenize(&query.text);
        let index = self.inverted_index.read().await;
        let documents = self.documents.read().await;

        let mut scores: HashMap<Uuid, f32> = HashMap::new();

        for term in &terms {
            if let Some(doc_ids) = index.get(term) {
                for &id in doc_ids {
                    *scores.entry(id).or_default() += 1.0;
                }
            }
        }

        let mut hits: Vec<SearchHit> = scores
            .into_iter()
            .filter_map(|(id, score)| {
                documents.get(&id).map(|doc| SearchHit {
                    document: doc.clone(),
                    score: score / terms.len() as f32,
                    snippets: self.extract_snippets(&doc.content, &terms),
                    highlighted: if query.highlight {
                        Some(self.highlight(&doc.content, &terms))
                    } else {
                        None
                    },
                })
            })
            .collect();

        hits.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Ok(hits)
    }

    async fn semantic_search(&self, query: &SearchQuery) -> Result<Vec<SearchHit>> {
        // For semantic search, we would use embeddings
        // This is a simplified version that falls back to keyword search
        self.keyword_search(query).await
    }

    async fn hybrid_search(&self, query: &SearchQuery) -> Result<Vec<SearchHit>> {
        // Combine keyword and semantic search
        let keyword_hits = self.keyword_search(query).await?;
        // In a real implementation, we'd also do semantic search and merge results
        Ok(keyword_hits)
    }

    async fn exact_search(&self, query: &SearchQuery) -> Result<Vec<SearchHit>> {
        let documents = self.documents.read().await;
        let query_lower = query.text.to_lowercase();

        let hits: Vec<SearchHit> = documents
            .values()
            .filter(|doc| doc.content.to_lowercase().contains(&query_lower))
            .map(|doc| {
                let terms = vec![query.text.clone()];
                SearchHit {
                    document: doc.clone(),
                    score: 1.0,
                    snippets: self.extract_snippets(&doc.content, &terms),
                    highlighted: if query.highlight {
                        Some(self.highlight(&doc.content, &terms))
                    } else {
                        None
                    },
                }
            })
            .collect();

        Ok(hits)
    }

    fn matches_filters(&self, doc: &SearchDocument, filters: &[SearchFilter]) -> bool {
        for filter in filters {
            match filter {
                SearchFilter::Source { sources } => {
                    if !sources.contains(&doc.source) {
                        return false;
                    }
                }
                SearchFilter::DateRange { start, end } => {
                    if let Some(start) = start {
                        if doc.created_at < *start {
                            return false;
                        }
                    }
                    if let Some(end) = end {
                        if doc.created_at > *end {
                            return false;
                        }
                    }
                }
                SearchFilter::SourceId { id } => {
                    if doc.source_id != *id {
                        return false;
                    }
                }
                SearchFilter::Metadata { key, value } => {
                    if doc.metadata.get(key) != Some(value) {
                        return false;
                    }
                }
                SearchFilter::HasEmbedding { required } => {
                    if *required && doc.embedding.is_none() {
                        return false;
                    }
                }
            }
        }
        true
    }

    fn sort_results(&self, hits: &mut [SearchHit], order: SortOrder) {
        match order {
            SortOrder::Relevance => {
                hits.sort_by(|a, b| {
                    b.score
                        .partial_cmp(&a.score)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
            }
            SortOrder::DateAsc => {
                hits.sort_by(|a, b| a.document.created_at.cmp(&b.document.created_at));
            }
            SortOrder::DateDesc => {
                hits.sort_by(|a, b| b.document.created_at.cmp(&a.document.created_at));
            }
            SortOrder::TitleAsc => {
                hits.sort_by(|a, b| a.document.title.cmp(&b.document.title));
            }
            SortOrder::TitleDesc => {
                hits.sort_by(|a, b| b.document.title.cmp(&a.document.title));
            }
        }
    }

    async fn generate_facets(&self, hits: &[SearchHit]) -> HashMap<String, Vec<Facet>> {
        let mut facets: HashMap<String, HashMap<String, usize>> = HashMap::new();

        for hit in hits {
            // Source facet
            let source = format!("{:?}", hit.document.source);
            *facets
                .entry("source".to_string())
                .or_default()
                .entry(source)
                .or_default() += 1;
        }

        facets
            .into_iter()
            .map(|(name, counts)| {
                let values: Vec<Facet> = counts
                    .into_iter()
                    .map(|(value, count)| Facet { value, count })
                    .collect();
                (name, values)
            })
            .collect()
    }

    fn tokenize(&self, text: &str) -> Vec<String> {
        text.to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| !s.is_empty() && s.len() > 2)
            .map(|s| s.to_string())
            .collect()
    }

    fn extract_snippets(&self, content: &str, terms: &[String]) -> Vec<Snippet> {
        let mut snippets = Vec::new();
        let content_lower = content.to_lowercase();

        for term in terms {
            if let Some(pos) = content_lower.find(&term.to_lowercase()) {
                let start = pos.saturating_sub(self.config.snippet_length / 2);
                let end = (pos + term.len() + self.config.snippet_length / 2).min(content.len());

                snippets.push(Snippet {
                    text: content[start..end].to_string(),
                    start,
                    end,
                    matched_terms: vec![term.clone()],
                });
            }
        }

        snippets
    }

    fn highlight(&self, content: &str, terms: &[String]) -> String {
        let mut result = content.to_string();

        for term in terms {
            let pattern = regex::Regex::new(&format!(r"(?i)({})", regex::escape(term))).unwrap();
            result = pattern.replace_all(&result, "**$1**").to_string();
        }

        result
    }

    /// Get document count.
    pub async fn count(&self) -> usize {
        self.documents.read().await.len()
    }

    /// Get all documents.
    pub async fn all_documents(&self) -> Vec<SearchDocument> {
        self.documents.read().await.values().cloned().collect()
    }

    /// Clear the index.
    pub async fn clear(&self) {
        self.documents.write().await.clear();
        self.inverted_index.write().await.clear();
    }
}

/// Simple embedding provider for testing.
pub struct SimpleEmbeddingProvider {
    dim: usize,
}

impl SimpleEmbeddingProvider {
    /// Create a new provider.
    pub fn new(dim: usize) -> Self {
        Self { dim }
    }
}

#[async_trait]
impl EmbeddingProvider for SimpleEmbeddingProvider {
    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        // Simple hash-based embedding for testing
        let mut embedding = vec![0.0f32; self.dim];
        for (i, c) in text.chars().enumerate() {
            embedding[i % self.dim] += (c as u32 as f32) / 1000.0;
        }
        // Normalize
        let mag: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        if mag > 0.0 {
            for x in &mut embedding {
                *x /= mag;
            }
        }
        Ok(embedding)
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let mut results = Vec::new();
        for text in texts {
            results.push(self.embed(text).await?);
        }
        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_search_index() {
        let index = SearchIndex::new(SearchConfig::default());

        let doc1 = SearchDocument::new("Hello world test", DocumentSource::Message, "conv-1");
        let doc2 = SearchDocument::new("Another test document", DocumentSource::Message, "conv-1");

        index.add(doc1).await;
        index.add(doc2).await;

        assert_eq!(index.count().await, 2);
    }

    #[tokio::test]
    async fn test_keyword_search() {
        let index = SearchIndex::new(SearchConfig::default());

        index
            .add(SearchDocument::new(
                "Rust programming language",
                DocumentSource::Note,
                "n-1",
            ))
            .await;
        index
            .add(SearchDocument::new(
                "Python programming guide",
                DocumentSource::Note,
                "n-2",
            ))
            .await;
        index
            .add(SearchDocument::new(
                "JavaScript basics",
                DocumentSource::Note,
                "n-3",
            ))
            .await;

        let query = SearchQuery::new("programming");
        let result = index.search(query).await.unwrap();

        assert_eq!(result.total, 2);
    }

    #[tokio::test]
    async fn test_exact_search() {
        let index = SearchIndex::new(SearchConfig::default());

        index
            .add(SearchDocument::new(
                "Find this exact phrase here",
                DocumentSource::Note,
                "n-1",
            ))
            .await;

        let query = SearchQuery::new("exact phrase").with_mode(SearchMode::Exact);
        let result = index.search(query).await.unwrap();

        assert_eq!(result.total, 1);
    }

    #[tokio::test]
    async fn test_embedding_provider() {
        let provider = SimpleEmbeddingProvider::new(128);
        let embedding = provider.embed("Test text").await.unwrap();

        assert_eq!(embedding.len(), 128);
    }
}
