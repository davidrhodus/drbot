//! Deep document understanding for drbot.
//!
//! AI-powered document analysis and Q&A.
//!
//! # Features
//!
//! - PDF/Word/Excel understanding
//! - Document Q&A
//! - Summary generation
//! - Key information extraction
//! - Cross-document analysis

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Document AI result type.
pub type Result<T> = std::result::Result<T, DocError>;

/// Document AI errors.
#[derive(Debug, thiserror::Error)]
pub enum DocError {
    #[error("Document not found: {0}")]
    NotFound(String),
    #[error("Unsupported format: {0}")]
    UnsupportedFormat(String),
    #[error("Parse error: {0}")]
    ParseError(String),
    #[error("Analysis failed: {0}")]
    AnalysisFailed(String),
}

/// Document types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentType {
    Pdf,
    Word,
    Excel,
    PowerPoint,
    Text,
    Markdown,
    Html,
    Image,
    Unknown,
}

impl DocumentType {
    /// Detect from extension.
    pub fn from_extension(ext: &str) -> Self {
        match ext.to_lowercase().as_str() {
            "pdf" => DocumentType::Pdf,
            "doc" | "docx" => DocumentType::Word,
            "xls" | "xlsx" => DocumentType::Excel,
            "ppt" | "pptx" => DocumentType::PowerPoint,
            "txt" => DocumentType::Text,
            "md" => DocumentType::Markdown,
            "html" | "htm" => DocumentType::Html,
            "png" | "jpg" | "jpeg" | "gif" | "webp" => DocumentType::Image,
            _ => DocumentType::Unknown,
        }
    }
}

/// A document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    /// Document ID.
    pub id: Uuid,
    /// Filename.
    pub filename: String,
    /// Document type.
    pub doc_type: DocumentType,
    /// File size.
    pub size_bytes: usize,
    /// Extracted text content.
    pub content: String,
    /// Page count (if applicable).
    pub page_count: Option<usize>,
    /// Metadata.
    pub metadata: DocumentMetadata,
    /// Indexed at.
    pub indexed_at: DateTime<Utc>,
}

impl Document {
    /// Create a new document.
    pub fn new(filename: &str, content: &str) -> Self {
        let ext = filename.rsplit('.').next().unwrap_or("");
        Self {
            id: Uuid::new_v4(),
            filename: filename.to_string(),
            doc_type: DocumentType::from_extension(ext),
            size_bytes: content.len(),
            content: content.to_string(),
            page_count: None,
            metadata: DocumentMetadata::default(),
            indexed_at: Utc::now(),
        }
    }

    /// Word count.
    pub fn word_count(&self) -> usize {
        self.content.split_whitespace().count()
    }
}

/// Document metadata.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DocumentMetadata {
    /// Title.
    pub title: Option<String>,
    /// Author.
    pub author: Option<String>,
    /// Created date.
    pub created: Option<DateTime<Utc>>,
    /// Modified date.
    pub modified: Option<DateTime<Utc>>,
    /// Tags.
    pub tags: Vec<String>,
    /// Custom fields.
    pub custom: HashMap<String, String>,
}

/// Document summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentSummary {
    /// Document ID.
    pub doc_id: Uuid,
    /// Brief summary.
    pub brief: String,
    /// Key points.
    pub key_points: Vec<String>,
    /// Topics covered.
    pub topics: Vec<String>,
    /// Important entities.
    pub entities: Vec<ExtractedEntity>,
    /// Generated at.
    pub generated_at: DateTime<Utc>,
}

/// Extracted entity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedEntity {
    /// Entity type.
    pub entity_type: EntityType,
    /// Value.
    pub value: String,
    /// Confidence.
    pub confidence: f32,
    /// Context.
    pub context: Option<String>,
}

/// Entity types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityType {
    Person,
    Organization,
    Location,
    Date,
    Money,
    Percentage,
    Email,
    Phone,
    Url,
    Custom,
}

/// Question answer result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QAResult {
    /// Question.
    pub question: String,
    /// Answer.
    pub answer: String,
    /// Confidence.
    pub confidence: f32,
    /// Source passages.
    pub sources: Vec<SourcePassage>,
    /// Related questions.
    pub related_questions: Vec<String>,
}

/// Source passage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourcePassage {
    /// Document ID.
    pub doc_id: Uuid,
    /// Page number (if applicable).
    pub page: Option<usize>,
    /// Text excerpt.
    pub text: String,
    /// Relevance score.
    pub relevance: f32,
}

/// Document comparison.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentComparison {
    /// Document IDs being compared.
    pub doc_ids: Vec<Uuid>,
    /// Similarities.
    pub similarities: Vec<String>,
    /// Differences.
    pub differences: Vec<String>,
    /// Overlap score (0-1).
    pub overlap_score: f32,
}

/// Document AI configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocAIConfig {
    /// Enable OCR for images.
    pub enable_ocr: bool,
    /// Maximum document size (bytes).
    pub max_size: usize,
    /// Enable entity extraction.
    pub extract_entities: bool,
    /// Summary length (words).
    pub summary_length: usize,
    /// Chunk size for indexing.
    pub chunk_size: usize,
}

impl Default for DocAIConfig {
    fn default() -> Self {
        Self {
            enable_ocr: true,
            max_size: 50 * 1024 * 1024, // 50MB
            extract_entities: true,
            summary_length: 200,
            chunk_size: 500,
        }
    }
}

/// Document chunk for indexing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentChunk {
    /// Chunk ID.
    pub id: Uuid,
    /// Document ID.
    pub doc_id: Uuid,
    /// Chunk index.
    pub index: usize,
    /// Text content.
    pub content: String,
    /// Page number.
    pub page: Option<usize>,
    /// Embedding (if computed).
    pub embedding: Option<Vec<f32>>,
}

/// Trait for document parsers.
#[async_trait]
pub trait DocumentParser: Send + Sync {
    /// Parse document content.
    async fn parse(&self, data: &[u8], doc_type: DocumentType) -> Result<ParsedContent>;
}

/// Parsed content.
#[derive(Debug, Clone)]
pub struct ParsedContent {
    /// Text content.
    pub text: String,
    /// Page count.
    pub pages: Option<usize>,
    /// Metadata.
    pub metadata: DocumentMetadata,
}

/// Trait for document analyzers.
#[async_trait]
pub trait DocumentAnalyzer: Send + Sync {
    /// Generate summary.
    async fn summarize(&self, doc: &Document) -> Result<DocumentSummary>;
    /// Extract entities.
    async fn extract_entities(&self, doc: &Document) -> Result<Vec<ExtractedEntity>>;
    /// Answer question.
    async fn answer(&self, question: &str, docs: &[Document]) -> Result<QAResult>;
}

/// Document AI engine.
pub struct DocAIEngine<P: DocumentParser, A: DocumentAnalyzer> {
    config: DocAIConfig,
    parser: P,
    analyzer: A,
    documents: Arc<RwLock<HashMap<Uuid, Document>>>,
    chunks: Arc<RwLock<Vec<DocumentChunk>>>,
    summaries: Arc<RwLock<HashMap<Uuid, DocumentSummary>>>,
}

impl<P: DocumentParser, A: DocumentAnalyzer> DocAIEngine<P, A> {
    /// Create a new document AI engine.
    pub fn new(config: DocAIConfig, parser: P, analyzer: A) -> Self {
        Self {
            config,
            parser,
            analyzer,
            documents: Arc::new(RwLock::new(HashMap::new())),
            chunks: Arc::new(RwLock::new(Vec::new())),
            summaries: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Index a document.
    pub async fn index(&self, filename: &str, data: &[u8]) -> Result<Document> {
        let ext = filename.rsplit('.').next().unwrap_or("");
        let doc_type = DocumentType::from_extension(ext);

        if doc_type == DocumentType::Unknown {
            return Err(DocError::UnsupportedFormat(ext.to_string()));
        }

        if data.len() > self.config.max_size {
            return Err(DocError::ParseError("Document too large".to_string()));
        }

        // Parse document
        let parsed = self.parser.parse(data, doc_type).await?;

        let mut doc = Document::new(filename, &parsed.text);
        doc.page_count = parsed.pages;
        doc.metadata = parsed.metadata;

        // Create chunks
        let doc_chunks = self.create_chunks(&doc);
        self.chunks.write().await.extend(doc_chunks);

        // Store document
        self.documents.write().await.insert(doc.id, doc.clone());

        Ok(doc)
    }

    fn create_chunks(&self, doc: &Document) -> Vec<DocumentChunk> {
        let words: Vec<_> = doc.content.split_whitespace().collect();
        let mut chunks = Vec::new();

        for (i, chunk_words) in words.chunks(self.config.chunk_size).enumerate() {
            chunks.push(DocumentChunk {
                id: Uuid::new_v4(),
                doc_id: doc.id,
                index: i,
                content: chunk_words.join(" "),
                page: None,
                embedding: None,
            });
        }

        chunks
    }

    /// Get document.
    pub async fn get(&self, id: Uuid) -> Option<Document> {
        self.documents.read().await.get(&id).cloned()
    }

    /// Get document summary.
    pub async fn summarize(&self, doc_id: Uuid) -> Result<DocumentSummary> {
        // Check cache
        if let Some(summary) = self.summaries.read().await.get(&doc_id) {
            return Ok(summary.clone());
        }

        let doc = self
            .documents
            .read()
            .await
            .get(&doc_id)
            .cloned()
            .ok_or(DocError::NotFound(doc_id.to_string()))?;

        let summary = self.analyzer.summarize(&doc).await?;
        self.summaries.write().await.insert(doc_id, summary.clone());

        Ok(summary)
    }

    /// Ask a question about documents.
    pub async fn ask(&self, question: &str, doc_ids: Option<Vec<Uuid>>) -> Result<QAResult> {
        let docs = self.documents.read().await;

        let target_docs: Vec<Document> = if let Some(ids) = doc_ids {
            ids.iter().filter_map(|id| docs.get(id).cloned()).collect()
        } else {
            docs.values().cloned().collect()
        };

        if target_docs.is_empty() {
            return Err(DocError::NotFound("No documents to search".to_string()));
        }

        self.analyzer.answer(question, &target_docs).await
    }

    /// Compare documents.
    pub async fn compare(&self, doc_ids: Vec<Uuid>) -> Result<DocumentComparison> {
        let docs = self.documents.read().await;

        let target_docs: Vec<_> = doc_ids.iter().filter_map(|id| docs.get(id)).collect();

        if target_docs.len() < 2 {
            return Err(DocError::NotFound("Need at least 2 documents".to_string()));
        }

        // Simple comparison based on word overlap
        let words_sets: Vec<std::collections::HashSet<_>> = target_docs
            .iter()
            .map(|d| {
                d.content
                    .to_lowercase()
                    .split_whitespace()
                    .map(String::from)
                    .collect()
            })
            .collect();

        let common: std::collections::HashSet<_> = words_sets[0]
            .intersection(&words_sets[1])
            .cloned()
            .collect();

        let all: std::collections::HashSet<_> =
            words_sets[0].union(&words_sets[1]).cloned().collect();

        let overlap = if all.is_empty() {
            0.0
        } else {
            common.len() as f32 / all.len() as f32
        };

        Ok(DocumentComparison {
            doc_ids,
            similarities: vec![format!("{} common terms", common.len())],
            differences: vec![format!("{} unique terms", all.len() - common.len())],
            overlap_score: overlap,
        })
    }

    /// Search documents.
    pub async fn search(&self, query: &str, limit: usize) -> Vec<(Document, f32)> {
        let docs = self.documents.read().await;
        let query_lower = query.to_lowercase();
        let query_words: std::collections::HashSet<_> = query_lower.split_whitespace().collect();

        let mut results: Vec<_> = docs
            .values()
            .map(|doc| {
                let doc_lower = doc.content.to_lowercase();
                let doc_words: std::collections::HashSet<_> =
                    doc_lower.split_whitespace().collect();
                let matches = query_words.intersection(&doc_words).count();
                let score = matches as f32 / query_words.len().max(1) as f32;
                (doc.clone(), score)
            })
            .filter(|(_, score)| *score > 0.0)
            .collect();

        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        results.truncate(limit);
        results
    }

    /// Get statistics.
    pub async fn stats(&self) -> DocStats {
        let docs = self.documents.read().await;
        let chunks = self.chunks.read().await;

        let total_size: usize = docs.values().map(|d| d.size_bytes).sum();
        let total_words: usize = docs.values().map(|d| d.word_count()).sum();

        let mut by_type: HashMap<DocumentType, usize> = HashMap::new();
        for doc in docs.values() {
            *by_type.entry(doc.doc_type).or_insert(0) += 1;
        }

        DocStats {
            total_documents: docs.len(),
            total_chunks: chunks.len(),
            total_size_bytes: total_size,
            total_words,
            by_type,
        }
    }
}

/// Document statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocStats {
    pub total_documents: usize,
    pub total_chunks: usize,
    pub total_size_bytes: usize,
    pub total_words: usize,
    pub by_type: HashMap<DocumentType, usize>,
}

/// Simple document parser for testing.
pub struct SimpleParser;

#[async_trait]
impl DocumentParser for SimpleParser {
    async fn parse(&self, data: &[u8], _doc_type: DocumentType) -> Result<ParsedContent> {
        let text = String::from_utf8_lossy(data).to_string();
        Ok(ParsedContent {
            text,
            pages: Some(1),
            metadata: DocumentMetadata::default(),
        })
    }
}

/// Simple document analyzer for testing.
pub struct SimpleAnalyzer;

#[async_trait]
impl DocumentAnalyzer for SimpleAnalyzer {
    async fn summarize(&self, doc: &Document) -> Result<DocumentSummary> {
        let words: Vec<_> = doc.content.split_whitespace().take(50).collect();

        Ok(DocumentSummary {
            doc_id: doc.id,
            brief: words.join(" "),
            key_points: vec!["Document processed".to_string()],
            topics: vec!["General".to_string()],
            entities: Vec::new(),
            generated_at: Utc::now(),
        })
    }

    async fn extract_entities(&self, doc: &Document) -> Result<Vec<ExtractedEntity>> {
        let mut entities = Vec::new();

        // Simple email extraction
        for word in doc.content.split_whitespace() {
            if word.contains('@') && word.contains('.') {
                entities.push(ExtractedEntity {
                    entity_type: EntityType::Email,
                    value: word.to_string(),
                    confidence: 0.9,
                    context: None,
                });
            }
        }

        Ok(entities)
    }

    async fn answer(&self, question: &str, docs: &[Document]) -> Result<QAResult> {
        let question_lower = question.to_lowercase();
        let question_words: std::collections::HashSet<_> =
            question_lower.split_whitespace().collect();

        let mut best_match: Option<(String, f32, Uuid)> = None;

        for doc in docs {
            for sentence in doc.content.split('.') {
                let sentence_lower = sentence.to_lowercase();
                let sentence_words: std::collections::HashSet<_> =
                    sentence_lower.split_whitespace().collect();
                let overlap = question_words.intersection(&sentence_words).count();
                let score = overlap as f32 / question_words.len().max(1) as f32;

                if best_match
                    .as_ref()
                    .map(|(_, s, _)| score > *s)
                    .unwrap_or(true)
                    && score > 0.0
                {
                    best_match = Some((sentence.trim().to_string(), score, doc.id));
                }
            }
        }

        let (answer, confidence, doc_id) = best_match.unwrap_or((
            "I couldn't find a relevant answer.".to_string(),
            0.0,
            Uuid::nil(),
        ));

        Ok(QAResult {
            question: question.to_string(),
            answer,
            confidence,
            sources: if confidence > 0.0 {
                vec![SourcePassage {
                    doc_id,
                    page: None,
                    text: "Matched passage".to_string(),
                    relevance: confidence,
                }]
            } else {
                Vec::new()
            },
            related_questions: Vec::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_index_document() {
        let engine = DocAIEngine::new(DocAIConfig::default(), SimpleParser, SimpleAnalyzer);

        let doc = engine
            .index("test.txt", b"Hello world, this is a test document.")
            .await
            .unwrap();
        assert_eq!(doc.doc_type, DocumentType::Text);
        assert!(doc.word_count() > 0);
    }

    #[tokio::test]
    async fn test_summarize() {
        let engine = DocAIEngine::new(DocAIConfig::default(), SimpleParser, SimpleAnalyzer);

        let doc = engine
            .index(
                "test.txt",
                b"This is a document about AI and machine learning.",
            )
            .await
            .unwrap();
        let summary = engine.summarize(doc.id).await.unwrap();
        assert!(!summary.brief.is_empty());
    }

    #[tokio::test]
    async fn test_search() {
        let engine = DocAIEngine::new(DocAIConfig::default(), SimpleParser, SimpleAnalyzer);

        engine
            .index("doc1.txt", b"The quick brown fox jumps over the lazy dog.")
            .await
            .unwrap();
        engine
            .index("doc2.txt", b"A lazy cat sleeps all day.")
            .await
            .unwrap();

        let results = engine.search("lazy", 10).await;
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn test_qa() {
        let engine = DocAIEngine::new(DocAIConfig::default(), SimpleParser, SimpleAnalyzer);

        engine
            .index(
                "facts.txt",
                b"The capital of France is Paris. The Eiffel Tower is located in Paris.",
            )
            .await
            .unwrap();

        let result = engine
            .ask("What is the capital of France?", None)
            .await
            .unwrap();
        assert!(result.answer.to_lowercase().contains("paris"));
    }
}
