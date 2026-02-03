//! Knowledge base and RAG (Retrieval Augmented Generation) for drbot.
//!
//! This crate provides:
//! - Document storage and indexing
//! - Semantic search with embeddings
//! - Knowledge graph for relationships
//! - Smart chunking strategies
//! - Query augmentation

mod chunking;
mod embeddings;
mod graph;
mod personal;
mod retrieval;
mod store;

pub use chunking::{Chunk, Chunker, ChunkingStrategy};
pub use embeddings::{EmbeddingProvider, LocalEmbeddings};
pub use graph::{Edge, KnowledgeGraph, Node, Relation};
pub use personal::{
    Contact, EntrySource, PersonalEntry, PersonalEntryType, PersonalKnowledgeBase,
    PersonalKnowledgeConfig, PersonalKnowledgeStats, PersonalSearchOptions, PersonalSearchResult,
    Project, UserContext,
};
pub use retrieval::{RetrievalOptions, RetrievalResult, Retriever};
pub use store::{Document, DocumentMetadata, KnowledgeStore};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Result type for knowledge operations.
pub type Result<T> = std::result::Result<T, KnowledgeError>;

/// Knowledge base errors.
#[derive(Debug, thiserror::Error)]
pub enum KnowledgeError {
    #[error("Storage error: {0}")]
    Storage(String),
    #[error("Embedding error: {0}")]
    Embedding(String),
    #[error("Document not found: {0}")]
    NotFound(Uuid),
    #[error("Invalid document: {0}")]
    InvalidDocument(String),
    #[error("Search error: {0}")]
    SearchError(String),
}

/// A knowledge entry that can be retrieved.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeEntry {
    /// Unique entry ID.
    pub id: Uuid,
    /// Source document ID.
    pub document_id: Uuid,
    /// Content text.
    pub content: String,
    /// Content embedding vector.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Vec<f32>>,
    /// Entry metadata.
    pub metadata: EntryMetadata,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
}

/// Metadata for a knowledge entry.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EntryMetadata {
    /// Source type (document, conversation, web, etc.).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_type: Option<String>,
    /// Original source URL or path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Section or chapter.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,
    /// Page number if applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    /// Position in source (chunk index).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<usize>,
    /// Tags for categorization.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Custom metadata.
    #[serde(default, flatten)]
    pub custom: serde_json::Map<String, serde_json::Value>,
}

/// Search result with relevance score.
#[derive(Debug, Clone)]
pub struct SearchResult {
    /// The knowledge entry.
    pub entry: KnowledgeEntry,
    /// Relevance score (0-1).
    pub score: f32,
    /// Highlight positions (start, end).
    pub highlights: Vec<(usize, usize)>,
}

/// Options for knowledge search.
#[derive(Debug, Clone, Default)]
pub struct SearchOptions {
    /// Maximum results to return.
    pub limit: Option<usize>,
    /// Minimum relevance score.
    pub min_score: Option<f32>,
    /// Filter by source type.
    pub source_type: Option<String>,
    /// Filter by tags.
    pub tags: Option<Vec<String>>,
    /// Filter by document IDs.
    pub document_ids: Option<Vec<Uuid>>,
    /// Include full content in results.
    pub include_content: bool,
    /// Hybrid search weight (0 = pure semantic, 1 = pure keyword).
    pub hybrid_weight: Option<f32>,
}

/// Main knowledge base manager.
pub struct KnowledgeBase {
    store: store::KnowledgeStore,
    chunker: Box<dyn Chunker>,
    embeddings: Box<dyn EmbeddingProvider>,
    retriever: retrieval::Retriever,
    graph: Option<graph::KnowledgeGraph>,
}

impl KnowledgeBase {
    /// Create a new knowledge base.
    pub fn new(
        store: store::KnowledgeStore,
        chunker: Box<dyn Chunker>,
        embeddings: Box<dyn EmbeddingProvider>,
    ) -> Self {
        let retriever = retrieval::Retriever::new();
        Self {
            store,
            chunker,
            embeddings,
            retriever,
            graph: None,
        }
    }

    /// Enable knowledge graph.
    pub fn with_graph(mut self, graph: graph::KnowledgeGraph) -> Self {
        self.graph = Some(graph);
        self
    }

    /// Add a document to the knowledge base.
    pub async fn add_document(&self, document: Document) -> Result<Uuid> {
        // Chunk the document
        let chunks = self
            .chunker
            .chunk(&document.content, document.metadata.clone());

        // Generate embeddings for each chunk
        let mut entries = Vec::new();
        for (i, chunk) in chunks.into_iter().enumerate() {
            let embedding = self.embeddings.embed(&chunk.content).await?;

            let entry = KnowledgeEntry {
                id: Uuid::new_v4(),
                document_id: document.id,
                content: chunk.content,
                embedding: Some(embedding),
                metadata: EntryMetadata {
                    source_type: document.metadata.source_type.clone(),
                    source: document.metadata.source.clone(),
                    section: chunk.section,
                    page: chunk.page,
                    position: Some(i),
                    tags: document.metadata.tags.clone(),
                    custom: serde_json::Map::new(),
                },
                created_at: Utc::now(),
            };
            entries.push(entry);
        }

        // Store document and entries
        self.store.add_document(&document).await?;
        for entry in &entries {
            self.store.add_entry(entry).await?;
        }

        // Add to knowledge graph if enabled
        if let Some(graph) = &self.graph {
            graph.add_document(&document, &entries).await?;
        }

        Ok(document.id)
    }

    /// Search the knowledge base.
    pub async fn search(&self, query: &str, options: SearchOptions) -> Result<Vec<SearchResult>> {
        // Generate query embedding
        let query_embedding = self.embeddings.embed(query).await?;

        // Search using retriever
        let results = self
            .retriever
            .search(
                &self.store,
                query,
                &query_embedding,
                retrieval::RetrievalOptions {
                    limit: options.limit.unwrap_or(10),
                    min_score: options.min_score.unwrap_or(0.0),
                    hybrid_weight: options.hybrid_weight.unwrap_or(0.3),
                    filters: retrieval::SearchFilters {
                        source_type: options.source_type,
                        tags: options.tags,
                        document_ids: options.document_ids,
                    },
                },
            )
            .await?;

        Ok(results)
    }

    /// Get context for a query (for RAG).
    pub async fn get_context(&self, query: &str, max_tokens: usize) -> Result<String> {
        let results = self
            .search(
                query,
                SearchOptions {
                    limit: Some(10),
                    min_score: Some(0.5),
                    include_content: true,
                    ..Default::default()
                },
            )
            .await?;

        // Combine results up to max tokens (rough estimate: 4 chars per token)
        let max_chars = max_tokens * 4;
        let mut context = String::new();
        let mut total_chars = 0;

        for result in results {
            if total_chars + result.entry.content.len() > max_chars {
                break;
            }
            if !context.is_empty() {
                context.push_str("\n\n---\n\n");
            }
            context.push_str(&result.entry.content);
            total_chars += result.entry.content.len();
        }

        Ok(context)
    }

    /// Delete a document and its entries.
    pub async fn delete_document(&self, id: Uuid) -> Result<()> {
        self.store.delete_document(id).await
    }

    /// List all documents.
    pub async fn list_documents(&self) -> Result<Vec<Document>> {
        self.store.list_documents().await
    }

    /// Get knowledge base statistics.
    pub async fn stats(&self) -> Result<KnowledgeStats> {
        self.store.stats().await
    }
}

/// Knowledge base statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeStats {
    /// Total number of documents.
    pub document_count: usize,
    /// Total number of entries.
    pub entry_count: usize,
    /// Total content size in bytes.
    pub total_size: usize,
    /// Average entries per document.
    pub avg_entries_per_doc: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entry_metadata_default() {
        let meta = EntryMetadata::default();
        assert!(meta.source_type.is_none());
        assert!(meta.tags.is_empty());
    }

    #[test]
    fn test_search_options_default() {
        let opts = SearchOptions::default();
        assert!(opts.limit.is_none());
        assert!(!opts.include_content);
    }
}
