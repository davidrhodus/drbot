//! Automatic source tracking and citation generation.
//!
//! This crate provides:
//! - Source tracking
//! - Citation generation
//! - Bibliography management
//! - Reference linking

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Citation errors.
#[derive(Debug, Error)]
pub enum CitationError {
    #[error("Source not found: {0}")]
    SourceNotFound(String),

    #[error("Invalid citation: {0}")]
    InvalidCitation(String),

    #[error("Format error: {0}")]
    FormatError(String),
}

/// Result type for citation operations.
pub type Result<T> = std::result::Result<T, CitationError>;

/// A source that can be cited.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    /// Source identifier.
    pub id: String,
    /// Source type.
    pub source_type: SourceType,
    /// Title.
    pub title: String,
    /// Authors.
    pub authors: Vec<String>,
    /// Publication date.
    pub date: Option<DateTime<Utc>>,
    /// URL.
    pub url: Option<String>,
    /// Publisher/venue.
    pub publisher: Option<String>,
    /// DOI.
    pub doi: Option<String>,
    /// Page numbers.
    pub pages: Option<String>,
    /// Volume/issue.
    pub volume: Option<String>,
    /// Accessed date.
    pub accessed_at: Option<DateTime<Utc>>,
    /// Additional metadata.
    pub metadata: HashMap<String, String>,
}

/// Source types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SourceType {
    WebPage,
    Article,
    Book,
    Journal,
    Conference,
    Report,
    Dataset,
    Code,
    Other,
}

/// A citation in the text.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Citation {
    /// Citation identifier.
    pub id: String,
    /// Source identifier.
    pub source_id: String,
    /// Citation style.
    pub style: CitationStyle,
    /// Formatted citation.
    pub formatted: String,
    /// Position in text (start, end).
    pub position: Option<(usize, usize)>,
    /// Quote from source.
    pub quote: Option<String>,
    /// Page reference.
    pub page: Option<String>,
}

/// Citation styles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CitationStyle {
    APA,
    MLA,
    Chicago,
    Harvard,
    IEEE,
    Inline,
    Footnote,
}

/// Text with citations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CitedText {
    /// Original text.
    pub text: String,
    /// Text with inline citations.
    pub cited_text: String,
    /// Citations.
    pub citations: Vec<Citation>,
    /// Bibliography.
    pub bibliography: Vec<String>,
}

/// Citation provider for formatting.
#[async_trait]
pub trait CitationFormatter: Send + Sync {
    /// Format a citation.
    async fn format_citation(&self, source: &Source, style: CitationStyle) -> Result<String>;

    /// Format a bibliography entry.
    async fn format_bibliography(&self, source: &Source, style: CitationStyle) -> Result<String>;

    /// Extract citations from text.
    async fn extract_citations(&self, text: &str) -> Result<Vec<(String, usize, usize)>>;
}

/// The citation engine.
pub struct CitationEngine {
    /// Citation formatter.
    formatter: Arc<dyn CitationFormatter>,
    /// Source registry.
    sources: Arc<RwLock<HashMap<String, Source>>>,
    /// Default citation style.
    default_style: CitationStyle,
}

impl CitationEngine {
    /// Create a new citation engine.
    pub fn new(formatter: Arc<dyn CitationFormatter>) -> Self {
        Self {
            formatter,
            sources: Arc::new(RwLock::new(HashMap::new())),
            default_style: CitationStyle::Inline,
        }
    }

    /// Set default citation style.
    pub fn with_style(mut self, style: CitationStyle) -> Self {
        self.default_style = style;
        self
    }

    /// Register a source.
    pub async fn register_source(&self, source: Source) -> String {
        let id = source.id.clone();
        let mut sources = self.sources.write().await;
        sources.insert(id.clone(), source);
        id
    }

    /// Get a source.
    pub async fn get_source(&self, id: &str) -> Option<Source> {
        let sources = self.sources.read().await;
        sources.get(id).cloned()
    }

    /// Create a citation.
    pub async fn cite(
        &self,
        source_id: &str,
        style: Option<CitationStyle>,
        quote: Option<String>,
        page: Option<String>,
    ) -> Result<Citation> {
        let sources = self.sources.read().await;
        let source = sources
            .get(source_id)
            .ok_or_else(|| CitationError::SourceNotFound(source_id.to_string()))?
            .clone();
        drop(sources);

        let style = style.unwrap_or(self.default_style);
        let formatted = self.formatter.format_citation(&source, style).await?;

        Ok(Citation {
            id: Uuid::new_v4().to_string(),
            source_id: source_id.to_string(),
            style,
            formatted,
            position: None,
            quote,
            page,
        })
    }

    /// Add citations to text.
    pub async fn add_citations(
        &self,
        text: &str,
        citations: Vec<(String, usize)>,
        style: Option<CitationStyle>,
    ) -> Result<CitedText> {
        let style = style.unwrap_or(self.default_style);
        let mut cited_text = text.to_string();
        let mut citation_list = Vec::new();
        let mut offset = 0;

        // Sort citations by position
        let mut sorted_citations = citations;
        sorted_citations.sort_by_key(|(_, pos)| *pos);

        for (source_id, position) in sorted_citations {
            let citation = self.cite(&source_id, Some(style), None, None).await?;

            let insert_pos = position + offset;
            let citation_text = format!(" [{}]", citation_list.len() + 1);

            if insert_pos <= cited_text.len() {
                cited_text.insert_str(insert_pos, &citation_text);
                offset += citation_text.len();
            }

            citation_list.push(Citation {
                position: Some((position, position)),
                ..citation
            });
        }

        // Generate bibliography
        let bibliography = self.generate_bibliography(&citation_list, style).await?;

        Ok(CitedText {
            text: text.to_string(),
            cited_text,
            citations: citation_list,
            bibliography,
        })
    }

    /// Generate bibliography.
    pub async fn generate_bibliography(
        &self,
        citations: &[Citation],
        style: CitationStyle,
    ) -> Result<Vec<String>> {
        let sources = self.sources.read().await;
        let mut bibliography = Vec::new();
        let mut seen_sources = std::collections::HashSet::new();

        for citation in citations {
            if seen_sources.insert(citation.source_id.clone()) {
                if let Some(source) = sources.get(&citation.source_id) {
                    let entry = self.formatter.format_bibliography(source, style).await?;
                    bibliography.push(entry);
                }
            }
        }

        // Sort alphabetically
        bibliography.sort();

        Ok(bibliography)
    }

    /// Extract and link citations from text.
    pub async fn extract_and_link(&self, text: &str) -> Result<CitedText> {
        let extracted = self.formatter.extract_citations(text).await?;

        let mut citations = Vec::new();
        let mut cited_text = text.to_string();

        for (source_id, start, end) in extracted {
            if let Ok(citation) = self
                .cite(&source_id, Some(self.default_style), None, None)
                .await
            {
                citations.push(Citation {
                    position: Some((start, end)),
                    ..citation
                });
            }
        }

        let bibliography = self
            .generate_bibliography(&citations, self.default_style)
            .await?;

        Ok(CitedText {
            text: text.to_string(),
            cited_text,
            citations,
            bibliography,
        })
    }

    /// Get all sources.
    pub async fn list_sources(&self) -> Vec<Source> {
        let sources = self.sources.read().await;
        sources.values().cloned().collect()
    }

    /// Remove a source.
    pub async fn remove_source(&self, id: &str) -> Option<Source> {
        let mut sources = self.sources.write().await;
        sources.remove(id)
    }

    /// Clear all sources.
    pub async fn clear_sources(&self) {
        let mut sources = self.sources.write().await;
        sources.clear();
    }
}

/// Builder for creating sources.
pub struct SourceBuilder {
    source: Source,
}

impl SourceBuilder {
    /// Create a new source builder.
    pub fn new(title: &str, source_type: SourceType) -> Self {
        Self {
            source: Source {
                id: Uuid::new_v4().to_string(),
                source_type,
                title: title.to_string(),
                authors: Vec::new(),
                date: None,
                url: None,
                publisher: None,
                doi: None,
                pages: None,
                volume: None,
                accessed_at: None,
                metadata: HashMap::new(),
            },
        }
    }

    /// Add author.
    pub fn author(mut self, author: &str) -> Self {
        self.source.authors.push(author.to_string());
        self
    }

    /// Set URL.
    pub fn url(mut self, url: &str) -> Self {
        self.source.url = Some(url.to_string());
        self.source.accessed_at = Some(Utc::now());
        self
    }

    /// Set date.
    pub fn date(mut self, date: DateTime<Utc>) -> Self {
        self.source.date = Some(date);
        self
    }

    /// Set publisher.
    pub fn publisher(mut self, publisher: &str) -> Self {
        self.source.publisher = Some(publisher.to_string());
        self
    }

    /// Set DOI.
    pub fn doi(mut self, doi: &str) -> Self {
        self.source.doi = Some(doi.to_string());
        self
    }

    /// Build the source.
    pub fn build(self) -> Source {
        self.source
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockFormatter;

    #[async_trait]
    impl CitationFormatter for MockFormatter {
        async fn format_citation(&self, source: &Source, style: CitationStyle) -> Result<String> {
            let author = source
                .authors
                .first()
                .map(|a| a.as_str())
                .unwrap_or("Unknown");
            let year = source
                .date
                .map(|d| d.format("%Y").to_string())
                .unwrap_or_else(|| "n.d.".to_string());

            Ok(match style {
                CitationStyle::APA => format!("({}, {})", author, year),
                CitationStyle::MLA => format!("({} {})", author, year),
                CitationStyle::Inline => format!("[{}]", source.title),
                _ => format!("({})", source.title),
            })
        }

        async fn format_bibliography(
            &self,
            source: &Source,
            style: CitationStyle,
        ) -> Result<String> {
            let author = source
                .authors
                .first()
                .map(|a| a.as_str())
                .unwrap_or("Unknown");
            let year = source
                .date
                .map(|d| d.format("%Y").to_string())
                .unwrap_or_else(|| "n.d.".to_string());

            Ok(match style {
                CitationStyle::APA => format!("{}. ({}). {}.", author, year, source.title),
                _ => format!("{}. {}. {}.", author, source.title, year),
            })
        }

        async fn extract_citations(&self, _text: &str) -> Result<Vec<(String, usize, usize)>> {
            Ok(vec![])
        }
    }

    #[tokio::test]
    async fn test_register_source() {
        let formatter = Arc::new(MockFormatter);
        let engine = CitationEngine::new(formatter);

        let source = SourceBuilder::new("Test Article", SourceType::Article)
            .author("John Doe")
            .build();

        let id = engine.register_source(source).await;
        let retrieved = engine.get_source(&id).await.unwrap();
        assert_eq!(retrieved.title, "Test Article");
    }

    #[tokio::test]
    async fn test_cite() {
        let formatter = Arc::new(MockFormatter);
        let engine = CitationEngine::new(formatter).with_style(CitationStyle::APA);

        let source = SourceBuilder::new("Test", SourceType::Article)
            .author("Smith")
            .date(Utc::now())
            .build();

        let id = engine.register_source(source).await;
        let citation = engine.cite(&id, None, None, None).await.unwrap();

        assert!(citation.formatted.contains("Smith"));
    }

    #[tokio::test]
    async fn test_add_citations() {
        let formatter = Arc::new(MockFormatter);
        let engine = CitationEngine::new(formatter);

        let source = SourceBuilder::new("Reference", SourceType::WebPage)
            .author("Author")
            .build();

        let id = engine.register_source(source).await;

        let cited = engine
            .add_citations(
                "This is a fact.",
                vec![(id, 14)],
                Some(CitationStyle::Inline),
            )
            .await
            .unwrap();

        assert!(cited.cited_text.contains("[1]"));
        assert!(!cited.bibliography.is_empty());
    }

    #[tokio::test]
    async fn test_bibliography() {
        let formatter = Arc::new(MockFormatter);
        let engine = CitationEngine::new(formatter);

        let source1 = SourceBuilder::new("Article A", SourceType::Article)
            .author("Alpha")
            .build();
        let source2 = SourceBuilder::new("Article B", SourceType::Article)
            .author("Beta")
            .build();

        let id1 = engine.register_source(source1).await;
        let id2 = engine.register_source(source2).await;

        let citations = vec![
            engine
                .cite(&id1, Some(CitationStyle::APA), None, None)
                .await
                .unwrap(),
            engine
                .cite(&id2, Some(CitationStyle::APA), None, None)
                .await
                .unwrap(),
        ];

        let bibliography = engine
            .generate_bibliography(&citations, CitationStyle::APA)
            .await
            .unwrap();
        assert_eq!(bibliography.len(), 2);
    }

    #[test]
    fn test_source_builder() {
        let source = SourceBuilder::new("My Article", SourceType::Journal)
            .author("Jane Doe")
            .author("John Smith")
            .url("https://example.com")
            .publisher("Academic Press")
            .build();

        assert_eq!(source.authors.len(), 2);
        assert!(source.url.is_some());
    }
}
