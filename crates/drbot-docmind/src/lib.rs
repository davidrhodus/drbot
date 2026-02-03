//! Document intelligence with cross-document reference tracking.
//!
//! This crate provides document understanding capabilities:
//! - Extract structure and content from documents
//! - Track references across documents
//! - Build document knowledge graphs
//! - Answer questions about document collections

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Document intelligence errors.
#[derive(Debug, Error)]
pub enum DocMindError {
    #[error("Document not found: {0}")]
    DocumentNotFound(String),

    #[error("Extraction failed: {0}")]
    ExtractionFailed(String),

    #[error("Reference resolution failed: {0}")]
    ReferenceResolutionFailed(String),

    #[error("Query failed: {0}")]
    QueryFailed(String),

    #[error("Provider error: {0}")]
    ProviderError(String),
}

/// Result type for document operations.
pub type Result<T> = std::result::Result<T, DocMindError>;

/// A document in the system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    /// Document identifier.
    pub id: String,
    /// Document title.
    pub title: String,
    /// Document type.
    pub doc_type: DocumentType,
    /// Source path or URL.
    pub source: String,
    /// Raw content.
    pub content: String,
    /// Extracted structure.
    pub structure: DocumentStructure,
    /// Extracted entities.
    pub entities: Vec<Entity>,
    /// References to other documents.
    pub references: Vec<DocumentReference>,
    /// Document metadata.
    pub metadata: DocumentMetadata,
    /// Last processed timestamp.
    pub processed_at: DateTime<Utc>,
}

/// Types of documents.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DocumentType {
    Pdf,
    Word,
    Markdown,
    Html,
    PlainText,
    Spreadsheet,
    Presentation,
    Email,
    Code,
    Custom(String),
}

/// Structure of a document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentStructure {
    /// Sections in the document.
    pub sections: Vec<Section>,
    /// Table of contents.
    pub toc: Vec<TocEntry>,
    /// Tables found.
    pub tables: Vec<TableContent>,
    /// Figures/images.
    pub figures: Vec<Figure>,
}

/// A section in a document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Section {
    /// Section identifier.
    pub id: String,
    /// Section title.
    pub title: String,
    /// Section level (1-6).
    pub level: u8,
    /// Section content.
    pub content: String,
    /// Parent section ID.
    pub parent: Option<String>,
    /// Page number if applicable.
    pub page: Option<u32>,
}

/// Table of contents entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TocEntry {
    /// Entry title.
    pub title: String,
    /// Level.
    pub level: u8,
    /// Page number.
    pub page: Option<u32>,
    /// Section ID.
    pub section_id: String,
}

/// A table in a document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableContent {
    /// Table identifier.
    pub id: String,
    /// Table title/caption.
    pub caption: Option<String>,
    /// Column headers.
    pub headers: Vec<String>,
    /// Table rows.
    pub rows: Vec<Vec<String>>,
    /// Page number.
    pub page: Option<u32>,
}

/// A figure/image in a document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Figure {
    /// Figure identifier.
    pub id: String,
    /// Figure caption.
    pub caption: Option<String>,
    /// Image description (alt text or OCR).
    pub description: Option<String>,
    /// Page number.
    pub page: Option<u32>,
}

/// An entity extracted from a document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    /// Entity identifier.
    pub id: String,
    /// Entity text.
    pub text: String,
    /// Entity type.
    pub entity_type: EntityType,
    /// Location in document.
    pub location: EntityLocation,
    /// Confidence score.
    pub confidence: f64,
}

/// Types of entities.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum EntityType {
    Person,
    Organization,
    Location,
    Date,
    Money,
    Percentage,
    Product,
    Technology,
    Concept,
    Citation,
    Url,
    Email,
    PhoneNumber,
    Custom(String),
}

/// Location of an entity in a document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityLocation {
    /// Section ID.
    pub section_id: Option<String>,
    /// Page number.
    pub page: Option<u32>,
    /// Character offset.
    pub offset: usize,
    /// Length.
    pub length: usize,
}

/// A reference to another document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentReference {
    /// Reference identifier.
    pub id: String,
    /// Referenced document ID (if resolved).
    pub target_doc_id: Option<String>,
    /// Reference text as it appears.
    pub reference_text: String,
    /// Reference type.
    pub ref_type: ReferenceType,
    /// Location in source document.
    pub location: EntityLocation,
    /// Whether resolved.
    pub resolved: bool,
}

/// Types of references.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ReferenceType {
    /// Citation (e.g., [1], Smith 2020).
    Citation,
    /// Hyperlink.
    Hyperlink,
    /// Cross-reference within document.
    CrossReference,
    /// Footnote.
    Footnote,
    /// See also reference.
    SeeAlso,
    /// Defined term.
    Definition,
}

/// Document metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentMetadata {
    /// Author(s).
    pub authors: Vec<String>,
    /// Creation date.
    pub created_at: Option<DateTime<Utc>>,
    /// Last modified.
    pub modified_at: Option<DateTime<Utc>>,
    /// Page count.
    pub page_count: Option<u32>,
    /// Word count.
    pub word_count: Option<u32>,
    /// Language.
    pub language: Option<String>,
    /// Keywords.
    pub keywords: Vec<String>,
    /// Custom metadata.
    pub custom: HashMap<String, serde_json::Value>,
}

/// Query about documents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentQuery {
    /// Natural language question.
    pub question: String,
    /// Document IDs to search (empty = all).
    pub document_ids: Vec<String>,
    /// Include context in answer.
    pub include_context: bool,
    /// Maximum results.
    pub max_results: usize,
}

/// Answer to a document query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryAnswer {
    /// The answer.
    pub answer: String,
    /// Confidence (0.0-1.0).
    pub confidence: f64,
    /// Supporting passages.
    pub passages: Vec<SupportingPassage>,
    /// Documents used.
    pub source_documents: Vec<String>,
}

/// A passage supporting an answer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupportingPassage {
    /// Document ID.
    pub document_id: String,
    /// Section ID.
    pub section_id: Option<String>,
    /// Passage text.
    pub text: String,
    /// Page number.
    pub page: Option<u32>,
    /// Relevance score.
    pub relevance: f64,
}

/// Provider for document intelligence.
#[async_trait]
pub trait DocMindProvider: Send + Sync {
    /// Extract structure from document content.
    async fn extract_structure(
        &self,
        content: &str,
        doc_type: &DocumentType,
    ) -> Result<DocumentStructure>;

    /// Extract entities from content.
    async fn extract_entities(&self, content: &str) -> Result<Vec<Entity>>;

    /// Extract references from content.
    async fn extract_references(&self, content: &str) -> Result<Vec<DocumentReference>>;

    /// Answer a question about documents.
    async fn answer_query(
        &self,
        query: &DocumentQuery,
        documents: &[Document],
    ) -> Result<QueryAnswer>;

    /// Generate a summary of a document.
    async fn summarize(&self, document: &Document, max_words: usize) -> Result<String>;
}

/// The document intelligence engine.
pub struct DocMindEngine {
    /// Provider for analysis.
    provider: Arc<dyn DocMindProvider>,
    /// Indexed documents.
    documents: Arc<RwLock<HashMap<String, Document>>>,
    /// Reference graph: doc_id -> referenced doc_ids.
    reference_graph: Arc<RwLock<HashMap<String, HashSet<String>>>>,
    /// Reverse reference graph: doc_id -> docs that reference it.
    cited_by: Arc<RwLock<HashMap<String, HashSet<String>>>>,
}

impl DocMindEngine {
    /// Create a new document intelligence engine.
    pub fn new(provider: Arc<dyn DocMindProvider>) -> Self {
        Self {
            provider,
            documents: Arc::new(RwLock::new(HashMap::new())),
            reference_graph: Arc::new(RwLock::new(HashMap::new())),
            cited_by: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Process and index a document.
    pub async fn process_document(
        &self,
        title: &str,
        content: &str,
        doc_type: DocumentType,
        source: &str,
    ) -> Result<Document> {
        // Extract structure
        let structure = self.provider.extract_structure(content, &doc_type).await?;

        // Extract entities
        let entities = self.provider.extract_entities(content).await?;

        // Extract references
        let references = self.provider.extract_references(content).await?;

        let doc = Document {
            id: Uuid::new_v4().to_string(),
            title: title.to_string(),
            doc_type,
            source: source.to_string(),
            content: content.to_string(),
            structure,
            entities,
            references,
            metadata: DocumentMetadata {
                authors: Vec::new(),
                created_at: Some(Utc::now()),
                modified_at: None,
                page_count: None,
                word_count: Some(content.split_whitespace().count() as u32),
                language: None,
                keywords: Vec::new(),
                custom: HashMap::new(),
            },
            processed_at: Utc::now(),
        };

        // Store document
        let mut documents = self.documents.write().await;
        documents.insert(doc.id.clone(), doc.clone());

        Ok(doc)
    }

    /// Get a document by ID.
    pub async fn get_document(&self, id: &str) -> Option<Document> {
        let documents = self.documents.read().await;
        documents.get(id).cloned()
    }

    /// Search documents by title or content.
    pub async fn search(&self, query: &str) -> Vec<Document> {
        let documents = self.documents.read().await;
        let query_lower = query.to_lowercase();

        documents
            .values()
            .filter(|d| {
                d.title.to_lowercase().contains(&query_lower)
                    || d.content.to_lowercase().contains(&query_lower)
            })
            .cloned()
            .collect()
    }

    /// Resolve references between documents.
    pub async fn resolve_references(&self) -> Result<usize> {
        let mut resolved_count = 0;

        let mut documents = self.documents.write().await;
        let doc_ids: Vec<_> = documents.keys().cloned().collect();

        // Build title index for matching
        let title_index: HashMap<String, String> = documents
            .values()
            .map(|d| (d.title.to_lowercase(), d.id.clone()))
            .collect();

        for doc_id in &doc_ids {
            if let Some(doc) = documents.get_mut(doc_id) {
                for reference in &mut doc.references {
                    if reference.resolved {
                        continue;
                    }

                    // Try to match reference to a document
                    let ref_lower = reference.reference_text.to_lowercase();
                    if let Some(target_id) = title_index.get(&ref_lower) {
                        reference.target_doc_id = Some(target_id.clone());
                        reference.resolved = true;
                        resolved_count += 1;
                    }
                }
            }
        }
        drop(documents);

        // Update reference graphs
        self.rebuild_reference_graph().await;

        Ok(resolved_count)
    }

    /// Rebuild the reference graph.
    async fn rebuild_reference_graph(&self) {
        let documents = self.documents.read().await;
        let mut ref_graph = self.reference_graph.write().await;
        let mut cited = self.cited_by.write().await;

        ref_graph.clear();
        cited.clear();

        for doc in documents.values() {
            let refs: HashSet<_> = doc
                .references
                .iter()
                .filter_map(|r| r.target_doc_id.clone())
                .collect();

            for target_id in &refs {
                cited
                    .entry(target_id.clone())
                    .or_insert_with(HashSet::new)
                    .insert(doc.id.clone());
            }

            ref_graph.insert(doc.id.clone(), refs);
        }
    }

    /// Get documents that reference a document.
    pub async fn get_cited_by(&self, doc_id: &str) -> Vec<Document> {
        let cited = self.cited_by.read().await;
        let documents = self.documents.read().await;

        cited
            .get(doc_id)
            .map(|citing_ids| {
                citing_ids
                    .iter()
                    .filter_map(|id| documents.get(id).cloned())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get documents referenced by a document.
    pub async fn get_references(&self, doc_id: &str) -> Vec<Document> {
        let ref_graph = self.reference_graph.read().await;
        let documents = self.documents.read().await;

        ref_graph
            .get(doc_id)
            .map(|ref_ids| {
                ref_ids
                    .iter()
                    .filter_map(|id| documents.get(id).cloned())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Answer a question about documents.
    pub async fn ask(&self, query: DocumentQuery) -> Result<QueryAnswer> {
        let documents = self.documents.read().await;

        let target_docs: Vec<_> = if query.document_ids.is_empty() {
            documents.values().cloned().collect()
        } else {
            query
                .document_ids
                .iter()
                .filter_map(|id| documents.get(id).cloned())
                .collect()
        };

        if target_docs.is_empty() {
            return Err(DocMindError::QueryFailed(
                "No documents to search".to_string(),
            ));
        }

        self.provider.answer_query(&query, &target_docs).await
    }

    /// Summarize a document.
    pub async fn summarize(&self, doc_id: &str, max_words: usize) -> Result<String> {
        let document = self
            .get_document(doc_id)
            .await
            .ok_or_else(|| DocMindError::DocumentNotFound(doc_id.to_string()))?;

        self.provider.summarize(&document, max_words).await
    }

    /// Get statistics about the document collection.
    pub async fn get_stats(&self) -> CollectionStats {
        let documents = self.documents.read().await;
        let ref_graph = self.reference_graph.read().await;

        let total_references: usize = ref_graph.values().map(|r| r.len()).sum();
        let total_entities: usize = documents.values().map(|d| d.entities.len()).sum();

        let mut entity_types: HashMap<String, usize> = HashMap::new();
        for doc in documents.values() {
            for entity in &doc.entities {
                *entity_types
                    .entry(format!("{:?}", entity.entity_type))
                    .or_insert(0) += 1;
            }
        }

        CollectionStats {
            document_count: documents.len(),
            total_words: documents
                .values()
                .filter_map(|d| d.metadata.word_count)
                .sum::<u32>() as usize,
            total_references,
            resolved_references: documents
                .values()
                .flat_map(|d| &d.references)
                .filter(|r| r.resolved)
                .count(),
            total_entities,
            entity_types,
        }
    }
}

/// Statistics about the document collection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionStats {
    /// Total documents.
    pub document_count: usize,
    /// Total words.
    pub total_words: usize,
    /// Total references.
    pub total_references: usize,
    /// Resolved references.
    pub resolved_references: usize,
    /// Total entities.
    pub total_entities: usize,
    /// Entity counts by type.
    pub entity_types: HashMap<String, usize>,
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockProvider;

    #[async_trait]
    impl DocMindProvider for MockProvider {
        async fn extract_structure(
            &self,
            content: &str,
            _doc_type: &DocumentType,
        ) -> Result<DocumentStructure> {
            Ok(DocumentStructure {
                sections: vec![Section {
                    id: "s1".to_string(),
                    title: "Introduction".to_string(),
                    level: 1,
                    content: content.to_string(),
                    parent: None,
                    page: Some(1),
                }],
                toc: vec![],
                tables: vec![],
                figures: vec![],
            })
        }

        async fn extract_entities(&self, _content: &str) -> Result<Vec<Entity>> {
            Ok(vec![Entity {
                id: "e1".to_string(),
                text: "Example Corp".to_string(),
                entity_type: EntityType::Organization,
                location: EntityLocation {
                    section_id: Some("s1".to_string()),
                    page: Some(1),
                    offset: 0,
                    length: 12,
                },
                confidence: 0.95,
            }])
        }

        async fn extract_references(&self, _content: &str) -> Result<Vec<DocumentReference>> {
            Ok(vec![])
        }

        async fn answer_query(
            &self,
            query: &DocumentQuery,
            _documents: &[Document],
        ) -> Result<QueryAnswer> {
            Ok(QueryAnswer {
                answer: format!("Answer to: {}", query.question),
                confidence: 0.85,
                passages: vec![],
                source_documents: vec![],
            })
        }

        async fn summarize(&self, document: &Document, max_words: usize) -> Result<String> {
            let words: Vec<_> = document
                .content
                .split_whitespace()
                .take(max_words)
                .collect();
            Ok(words.join(" "))
        }
    }

    #[tokio::test]
    async fn test_process_document() {
        let provider = Arc::new(MockProvider);
        let engine = DocMindEngine::new(provider);

        let doc = engine
            .process_document(
                "Test Document",
                "This is test content about Example Corp.",
                DocumentType::PlainText,
                "/path/to/doc.txt",
            )
            .await
            .unwrap();

        assert_eq!(doc.title, "Test Document");
        assert!(!doc.entities.is_empty());
    }

    #[tokio::test]
    async fn test_search() {
        let provider = Arc::new(MockProvider);
        let engine = DocMindEngine::new(provider);

        engine
            .process_document(
                "First Document",
                "Content about topic A",
                DocumentType::PlainText,
                "/doc1.txt",
            )
            .await
            .unwrap();

        engine
            .process_document(
                "Second Document",
                "Content about topic B",
                DocumentType::PlainText,
                "/doc2.txt",
            )
            .await
            .unwrap();

        let results = engine.search("First").await;
        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn test_ask() {
        let provider = Arc::new(MockProvider);
        let engine = DocMindEngine::new(provider);

        engine
            .process_document(
                "Test Doc",
                "Information about something",
                DocumentType::PlainText,
                "/doc.txt",
            )
            .await
            .unwrap();

        let query = DocumentQuery {
            question: "What is this about?".to_string(),
            document_ids: vec![],
            include_context: true,
            max_results: 5,
        };

        let answer = engine.ask(query).await.unwrap();
        assert!(!answer.answer.is_empty());
    }

    #[tokio::test]
    async fn test_stats() {
        let provider = Arc::new(MockProvider);
        let engine = DocMindEngine::new(provider);

        engine
            .process_document(
                "Doc 1",
                "First document content",
                DocumentType::PlainText,
                "/doc1.txt",
            )
            .await
            .unwrap();

        let stats = engine.get_stats().await;
        assert_eq!(stats.document_count, 1);
        assert!(stats.total_entities > 0);
    }

    #[test]
    fn test_document_types() {
        let pdf = DocumentType::Pdf;
        let custom = DocumentType::Custom("LaTeX".to_string());

        let _ = serde_json::to_string(&pdf).unwrap();
        let _ = serde_json::to_string(&custom).unwrap();
    }
}
