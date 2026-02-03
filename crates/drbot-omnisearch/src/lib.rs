//! Unified personal search for drbot.
//!
//! Search across all data sources with a single query.
//!
//! # Features
//!
//! - Multi-source search (files, emails, messages, notes, etc.)
//! - Semantic search with embeddings
//! - Faceted filtering
//! - Real-time indexing
//! - Ranking and relevance scoring

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Omnisearch result type.
pub type Result<T> = std::result::Result<T, SearchError>;

/// Search errors.
#[derive(Debug, thiserror::Error)]
pub enum SearchError {
    #[error("Source not found: {0}")]
    SourceNotFound(String),
    #[error("Index error: {0}")]
    IndexError(String),
    #[error("Query error: {0}")]
    QueryError(String),
    #[error("Timeout")]
    Timeout,
}

/// Data source types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceType {
    Files,
    Emails,
    Messages,
    Notes,
    Calendar,
    Contacts,
    Bookmarks,
    Clipboard,
    Browser,
    Code,
    Custom,
}

/// Search result item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    /// Result ID.
    pub id: Uuid,
    /// Source type.
    pub source: SourceType,
    /// Title.
    pub title: String,
    /// Content snippet.
    pub snippet: String,
    /// Full content (optional).
    pub content: Option<String>,
    /// URL or path.
    pub location: String,
    /// Relevance score (0-1).
    pub score: f32,
    /// Metadata.
    pub metadata: HashMap<String, String>,
    /// Created/modified date.
    pub date: Option<DateTime<Utc>>,
    /// Highlights (matched terms).
    pub highlights: Vec<Highlight>,
}

/// Text highlight.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Highlight {
    /// Field name.
    pub field: String,
    /// Start offset.
    pub start: usize,
    /// End offset.
    pub end: usize,
    /// Matched text.
    pub text: String,
}

/// Search query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchQuery {
    /// Query text.
    pub text: String,
    /// Source filters.
    pub sources: Option<Vec<SourceType>>,
    /// Date range.
    pub date_from: Option<DateTime<Utc>>,
    pub date_to: Option<DateTime<Utc>>,
    /// Metadata filters.
    pub filters: HashMap<String, String>,
    /// Maximum results.
    pub limit: usize,
    /// Offset for pagination.
    pub offset: usize,
    /// Enable semantic search.
    pub semantic: bool,
}

impl Default for SearchQuery {
    fn default() -> Self {
        Self {
            text: String::new(),
            sources: None,
            date_from: None,
            date_to: None,
            filters: HashMap::new(),
            limit: 20,
            offset: 0,
            semantic: true,
        }
    }
}

impl SearchQuery {
    /// Create a new query.
    pub fn new(text: &str) -> Self {
        Self {
            text: text.to_string(),
            ..Default::default()
        }
    }

    /// Filter by sources.
    pub fn sources(mut self, sources: Vec<SourceType>) -> Self {
        self.sources = Some(sources);
        self
    }

    /// Set limit.
    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }

    /// Enable/disable semantic search.
    pub fn semantic(mut self, enabled: bool) -> Self {
        self.semantic = enabled;
        self
    }
}

/// Search response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResponse {
    /// Query that was executed.
    pub query: String,
    /// Total matches (may be more than returned).
    pub total: usize,
    /// Results.
    pub results: Vec<SearchResult>,
    /// Facets (counts by source).
    pub facets: HashMap<SourceType, usize>,
    /// Query time (ms).
    pub took_ms: u64,
    /// Suggestions for query refinement.
    pub suggestions: Vec<String>,
}

/// Indexable document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    /// Document ID.
    pub id: Uuid,
    /// Source type.
    pub source: SourceType,
    /// Title.
    pub title: String,
    /// Content.
    pub content: String,
    /// Location (path/URL).
    pub location: String,
    /// Metadata.
    pub metadata: HashMap<String, String>,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
    /// Embedding vector (optional).
    pub embedding: Option<Vec<f32>>,
}

impl Document {
    /// Create a new document.
    pub fn new(source: SourceType, title: &str, content: &str, location: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            source,
            title: title.to_string(),
            content: content.to_string(),
            location: location.to_string(),
            metadata: HashMap::new(),
            timestamp: Utc::now(),
            embedding: None,
        }
    }
}

/// Search configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchConfig {
    /// Enable fuzzy matching.
    pub fuzzy: bool,
    /// Fuzzy distance.
    pub fuzzy_distance: usize,
    /// Snippet length.
    pub snippet_length: usize,
    /// Boost recent results.
    pub recency_boost: f32,
    /// Default limit.
    pub default_limit: usize,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            fuzzy: true,
            fuzzy_distance: 2,
            snippet_length: 200,
            recency_boost: 0.1,
            default_limit: 20,
        }
    }
}

/// Trait for data source providers.
#[async_trait]
pub trait SourceProvider: Send + Sync {
    /// Get source type.
    fn source_type(&self) -> SourceType;
    /// Index documents from source.
    async fn index(&self) -> Result<Vec<Document>>;
    /// Search within source.
    async fn search(&self, query: &SearchQuery) -> Result<Vec<SearchResult>>;
    /// Get document by ID.
    async fn get(&self, id: Uuid) -> Result<Option<Document>>;
}

/// Trait for embedding providers.
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// Generate embedding for text.
    async fn embed(&self, text: &str) -> Result<Vec<f32>>;
    /// Generate embeddings for multiple texts.
    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;
}

/// Omnisearch engine.
pub struct OmniSearch {
    config: SearchConfig,
    providers: Arc<RwLock<HashMap<SourceType, Box<dyn SourceProvider>>>>,
    index: Arc<RwLock<HashMap<Uuid, Document>>>,
    embeddings: Arc<RwLock<HashMap<Uuid, Vec<f32>>>>,
}

impl OmniSearch {
    /// Create a new search engine.
    pub fn new(config: SearchConfig) -> Self {
        Self {
            config,
            providers: Arc::new(RwLock::new(HashMap::new())),
            index: Arc::new(RwLock::new(HashMap::new())),
            embeddings: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a source provider.
    pub async fn register_provider(&self, provider: Box<dyn SourceProvider>) {
        let source_type = provider.source_type();
        self.providers.write().await.insert(source_type, provider);
    }

    /// Index all sources.
    pub async fn index_all(&self) -> Result<usize> {
        let providers = self.providers.read().await;
        let mut total = 0;

        for provider in providers.values() {
            let docs = provider.index().await?;
            let mut index = self.index.write().await;
            for doc in docs {
                index.insert(doc.id, doc);
                total += 1;
            }
        }

        Ok(total)
    }

    /// Index a single document.
    pub async fn index_document(&self, doc: Document) {
        self.index.write().await.insert(doc.id, doc);
    }

    /// Search across all sources.
    pub async fn search(&self, query: SearchQuery) -> Result<SearchResponse> {
        let start = std::time::Instant::now();
        let query_lower = query.text.to_lowercase();
        let query_words: Vec<&str> = query_lower.split_whitespace().collect();

        let index = self.index.read().await;
        let mut results: Vec<SearchResult> = Vec::new();
        let mut facets: HashMap<SourceType, usize> = HashMap::new();

        for doc in index.values() {
            // Filter by source
            if let Some(ref sources) = query.sources {
                if !sources.contains(&doc.source) {
                    continue;
                }
            }

            // Filter by date
            if let Some(from) = query.date_from {
                if doc.timestamp < from {
                    continue;
                }
            }
            if let Some(to) = query.date_to {
                if doc.timestamp > to {
                    continue;
                }
            }

            // Calculate score
            let title_lower = doc.title.to_lowercase();
            let content_lower = doc.content.to_lowercase();

            let mut score = 0.0f32;
            let mut highlights = Vec::new();

            for word in &query_words {
                // Title match (higher weight)
                if title_lower.contains(word) {
                    score += 0.5;
                    if let Some(pos) = title_lower.find(word) {
                        highlights.push(Highlight {
                            field: "title".to_string(),
                            start: pos,
                            end: pos + word.len(),
                            text: word.to_string(),
                        });
                    }
                }
                // Content match
                if content_lower.contains(word) {
                    score += 0.3;
                    if let Some(pos) = content_lower.find(word) {
                        highlights.push(Highlight {
                            field: "content".to_string(),
                            start: pos,
                            end: pos + word.len(),
                            text: word.to_string(),
                        });
                    }
                }
            }

            // Normalize score
            if !query_words.is_empty() {
                score /= query_words.len() as f32;
            }

            // Only include documents that match the query
            if score > 0.0 {
                // Recency boost
                let age_days = (Utc::now() - doc.timestamp).num_days().max(1) as f32;
                score += self.config.recency_boost / age_days.sqrt();
                // Create snippet
                let snippet = if doc.content.len() > self.config.snippet_length {
                    format!("{}...", &doc.content[..self.config.snippet_length])
                } else {
                    doc.content.clone()
                };

                results.push(SearchResult {
                    id: doc.id,
                    source: doc.source,
                    title: doc.title.clone(),
                    snippet,
                    content: Some(doc.content.clone()),
                    location: doc.location.clone(),
                    score,
                    metadata: doc.metadata.clone(),
                    date: Some(doc.timestamp),
                    highlights,
                });

                *facets.entry(doc.source).or_insert(0) += 1;
            }
        }

        // Sort by score
        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());

        let total = results.len();

        // Apply pagination
        let results: Vec<_> = results
            .into_iter()
            .skip(query.offset)
            .take(query.limit)
            .collect();

        let took_ms = start.elapsed().as_millis() as u64;

        // Generate suggestions
        let suggestions = self.generate_suggestions(&query.text, &results);

        Ok(SearchResponse {
            query: query.text,
            total,
            results,
            facets,
            took_ms,
            suggestions,
        })
    }

    fn generate_suggestions(&self, query: &str, results: &[SearchResult]) -> Vec<String> {
        let mut suggestions = Vec::new();

        // If few results, suggest related terms
        if results.len() < 3 {
            suggestions.push(format!("Try: {} files", query));
            suggestions.push(format!("Try: {} notes", query));
        }

        // Extract common terms from results
        let mut term_counts: HashMap<String, usize> = HashMap::new();
        for result in results.iter().take(5) {
            for word in result.title.split_whitespace() {
                if word.len() > 3 && !query.to_lowercase().contains(&word.to_lowercase()) {
                    *term_counts.entry(word.to_lowercase()).or_insert(0) += 1;
                }
            }
        }

        let mut common: Vec<_> = term_counts.into_iter().collect();
        common.sort_by_key(|(_, count)| std::cmp::Reverse(*count));

        for (term, _) in common.into_iter().take(2) {
            suggestions.push(format!("{} {}", query, term));
        }

        suggestions.truncate(3);
        suggestions
    }

    /// Get document by ID.
    pub async fn get(&self, id: Uuid) -> Option<Document> {
        self.index.read().await.get(&id).cloned()
    }

    /// Delete document.
    pub async fn delete(&self, id: Uuid) -> bool {
        self.index.write().await.remove(&id).is_some()
    }

    /// Clear all indexed documents.
    pub async fn clear(&self) {
        self.index.write().await.clear();
        self.embeddings.write().await.clear();
    }

    /// Get statistics.
    pub async fn stats(&self) -> SearchStats {
        let index = self.index.read().await;
        let providers = self.providers.read().await;

        let mut by_source: HashMap<SourceType, usize> = HashMap::new();
        for doc in index.values() {
            *by_source.entry(doc.source).or_insert(0) += 1;
        }

        SearchStats {
            total_documents: index.len(),
            total_sources: providers.len(),
            by_source,
        }
    }
}

/// Search statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchStats {
    pub total_documents: usize,
    pub total_sources: usize,
    pub by_source: HashMap<SourceType, usize>,
}

/// Simple file source provider for testing.
pub struct FileSourceProvider {
    #[allow(dead_code)]
    base_path: String,
}

impl FileSourceProvider {
    pub fn new(base_path: &str) -> Self {
        Self {
            base_path: base_path.to_string(),
        }
    }
}

#[async_trait]
impl SourceProvider for FileSourceProvider {
    fn source_type(&self) -> SourceType {
        SourceType::Files
    }

    async fn index(&self) -> Result<Vec<Document>> {
        // In real impl, would scan filesystem
        Ok(Vec::new())
    }

    async fn search(&self, _query: &SearchQuery) -> Result<Vec<SearchResult>> {
        Ok(Vec::new())
    }

    async fn get(&self, _id: Uuid) -> Result<Option<Document>> {
        Ok(None)
    }
}

/// Simple embedding provider using word vectors.
pub struct SimpleEmbedding;

#[async_trait]
impl EmbeddingProvider for SimpleEmbedding {
    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        // Simple bag-of-words style embedding
        let lowercase = text.to_lowercase();
        let words: Vec<_> = lowercase.split_whitespace().collect();
        let mut embedding = vec![0.0f32; 128];

        for (i, word) in words.iter().enumerate() {
            let hash = word.bytes().fold(0u32, |acc, b| acc.wrapping_add(b as u32));
            let idx = (hash as usize) % 128;
            embedding[idx] += 1.0 / (i + 1) as f32;
        }

        // Normalize
        let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for v in &mut embedding {
                *v /= norm;
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
    async fn test_index_document() {
        let search = OmniSearch::new(SearchConfig::default());

        let doc = Document::new(
            SourceType::Notes,
            "Test Note",
            "This is a test note content",
            "/notes/test.md",
        );
        search.index_document(doc).await;

        let stats = search.stats().await;
        assert_eq!(stats.total_documents, 1);
    }

    #[tokio::test]
    async fn test_basic_search() {
        let search = OmniSearch::new(SearchConfig::default());

        search
            .index_document(Document::new(
                SourceType::Notes,
                "Meeting Notes",
                "Discussion about project timeline",
                "/notes/meeting.md",
            ))
            .await;
        search
            .index_document(Document::new(
                SourceType::Files,
                "Report",
                "Quarterly report data",
                "/docs/report.pdf",
            ))
            .await;

        let query = SearchQuery::new("meeting");
        let response = search.search(query).await.unwrap();

        assert_eq!(response.total, 1);
        assert_eq!(response.results[0].title, "Meeting Notes");
    }

    #[tokio::test]
    async fn test_source_filter() {
        let search = OmniSearch::new(SearchConfig::default());

        search
            .index_document(Document::new(
                SourceType::Notes,
                "Note about Rust",
                "Rust programming",
                "/notes/rust.md",
            ))
            .await;
        search
            .index_document(Document::new(
                SourceType::Files,
                "Rust Book",
                "Rust programming language",
                "/books/rust.pdf",
            ))
            .await;

        let query = SearchQuery::new("rust").sources(vec![SourceType::Notes]);
        let response = search.search(query).await.unwrap();

        assert_eq!(response.total, 1);
        assert_eq!(response.results[0].source, SourceType::Notes);
    }

    #[tokio::test]
    async fn test_facets() {
        let search = OmniSearch::new(SearchConfig::default());

        search
            .index_document(Document::new(
                SourceType::Notes,
                "Note 1",
                "test content",
                "/a",
            ))
            .await;
        search
            .index_document(Document::new(
                SourceType::Notes,
                "Note 2",
                "test content",
                "/b",
            ))
            .await;
        search
            .index_document(Document::new(
                SourceType::Files,
                "File 1",
                "test content",
                "/c",
            ))
            .await;

        let query = SearchQuery::new("test");
        let response = search.search(query).await.unwrap();

        assert_eq!(response.facets.get(&SourceType::Notes), Some(&2));
        assert_eq!(response.facets.get(&SourceType::Files), Some(&1));
    }

    #[tokio::test]
    async fn test_pagination() {
        let search = OmniSearch::new(SearchConfig::default());

        for i in 0..10 {
            search
                .index_document(Document::new(
                    SourceType::Notes,
                    &format!("Note {}", i),
                    "test content",
                    &format!("/note{}", i),
                ))
                .await;
        }

        let query = SearchQuery::new("test").limit(3);
        let response = search.search(query).await.unwrap();

        assert_eq!(response.total, 10);
        assert_eq!(response.results.len(), 3);
    }

    #[tokio::test]
    async fn test_embedding() {
        let embedder = SimpleEmbedding;

        let e1 = embedder.embed("hello world").await.unwrap();
        let e2 = embedder.embed("hello world").await.unwrap();
        let e3 = embedder.embed("goodbye universe").await.unwrap();

        // Same text should have same embedding
        assert_eq!(e1, e2);
        // Different text should have different embedding
        assert_ne!(e1, e3);
    }
}
