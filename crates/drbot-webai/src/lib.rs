//! Browser companion for drbot.
//!
//! AI-powered web assistance.
//!
//! # Features
//!
//! - Page summarization
//! - Form filling
//! - Research assistance
//! - Content extraction
//! - Reading mode

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Web AI result type.
pub type Result<T> = std::result::Result<T, WebError>;

/// Web AI errors.
#[derive(Debug, thiserror::Error)]
pub enum WebError {
    #[error("Page not found: {0}")]
    PageNotFound(String),
    #[error("Extraction failed: {0}")]
    ExtractionFailed(String),
    #[error("Summarization failed: {0}")]
    SummarizationFailed(String),
}

/// Web page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebPage {
    /// Page ID.
    pub id: Uuid,
    /// URL.
    pub url: String,
    /// Title.
    pub title: String,
    /// Raw HTML.
    pub html: String,
    /// Extracted text.
    pub text: String,
    /// Metadata.
    pub metadata: PageMetadata,
    /// Captured at.
    pub captured_at: DateTime<Utc>,
}

impl WebPage {
    /// Create from HTML.
    pub fn from_html(url: &str, title: &str, html: &str) -> Self {
        // Simple text extraction (remove tags)
        let text = html
            .split('<')
            .filter_map(|s| s.split('>').nth(1))
            .collect::<Vec<_>>()
            .join(" ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");

        Self {
            id: Uuid::new_v4(),
            url: url.to_string(),
            title: title.to_string(),
            html: html.to_string(),
            text,
            metadata: PageMetadata::default(),
            captured_at: Utc::now(),
        }
    }

    /// Word count.
    pub fn word_count(&self) -> usize {
        self.text.split_whitespace().count()
    }
}

/// Page metadata.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PageMetadata {
    /// Description.
    pub description: Option<String>,
    /// Author.
    pub author: Option<String>,
    /// Published date.
    pub published: Option<DateTime<Utc>>,
    /// Keywords.
    pub keywords: Vec<String>,
    /// Language.
    pub language: Option<String>,
    /// Image URL.
    pub image: Option<String>,
}

/// Page summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageSummary {
    /// Page ID.
    pub page_id: Uuid,
    /// Brief summary.
    pub brief: String,
    /// Key points.
    pub key_points: Vec<String>,
    /// Reading time (minutes).
    pub reading_time: u32,
    /// Topics.
    pub topics: Vec<String>,
    /// Sentiment.
    pub sentiment: Option<String>,
}

/// Extracted content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedContent {
    /// Main article text.
    pub article: String,
    /// Headlines.
    pub headlines: Vec<String>,
    /// Links.
    pub links: Vec<ExtractedLink>,
    /// Images.
    pub images: Vec<ExtractedImage>,
    /// Tables.
    pub tables: Vec<ExtractedTable>,
    /// Code blocks.
    pub code_blocks: Vec<String>,
}

/// Extracted link.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedLink {
    /// Link text.
    pub text: String,
    /// URL.
    pub url: String,
    /// Is external.
    pub external: bool,
}

/// Extracted image.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedImage {
    /// Source URL.
    pub src: String,
    /// Alt text.
    pub alt: Option<String>,
    /// Caption.
    pub caption: Option<String>,
}

/// Extracted table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedTable {
    /// Headers.
    pub headers: Vec<String>,
    /// Rows.
    pub rows: Vec<Vec<String>>,
}

/// Form field.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormField {
    /// Field name.
    pub name: String,
    /// Field type.
    pub field_type: FieldType,
    /// Label.
    pub label: Option<String>,
    /// Placeholder.
    pub placeholder: Option<String>,
    /// Is required.
    pub required: bool,
    /// Current value.
    pub value: Option<String>,
    /// Options (for select).
    pub options: Vec<String>,
}

/// Field types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldType {
    Text,
    Email,
    Password,
    Number,
    Phone,
    Date,
    Select,
    Checkbox,
    Radio,
    Textarea,
    File,
    Hidden,
}

/// Form fill suggestion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormFillSuggestion {
    /// Field name.
    pub field_name: String,
    /// Suggested value.
    pub value: String,
    /// Confidence.
    pub confidence: f32,
    /// Source.
    pub source: String,
}

/// Research result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchResult {
    /// Query.
    pub query: String,
    /// Sources.
    pub sources: Vec<ResearchSource>,
    /// Synthesis.
    pub synthesis: String,
    /// Key findings.
    pub findings: Vec<String>,
    /// Related queries.
    pub related: Vec<String>,
}

/// Research source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchSource {
    /// URL.
    pub url: String,
    /// Title.
    pub title: String,
    /// Relevant excerpt.
    pub excerpt: String,
    /// Credibility score.
    pub credibility: f32,
}

/// Web AI configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebAIConfig {
    /// Enable auto-summarize.
    pub auto_summarize: bool,
    /// Enable reading mode.
    pub reading_mode: bool,
    /// Summary length.
    pub summary_length: usize,
    /// Cache pages.
    pub cache_pages: bool,
}

impl Default for WebAIConfig {
    fn default() -> Self {
        Self {
            auto_summarize: true,
            reading_mode: true,
            summary_length: 200,
            cache_pages: true,
        }
    }
}

/// Trait for content extractors.
#[async_trait]
pub trait ContentExtractor: Send + Sync {
    /// Extract content from page.
    async fn extract(&self, page: &WebPage) -> Result<ExtractedContent>;
    /// Extract forms.
    async fn extract_forms(&self, page: &WebPage) -> Result<Vec<Vec<FormField>>>;
}

/// Trait for page analyzers.
#[async_trait]
pub trait PageAnalyzer: Send + Sync {
    /// Summarize page.
    async fn summarize(&self, page: &WebPage) -> Result<PageSummary>;
    /// Suggest form fills.
    fn suggest_fills(
        &self,
        fields: &[FormField],
        context: &HashMap<String, String>,
    ) -> Vec<FormFillSuggestion>;
}

/// Web AI engine.
pub struct WebAIEngine<E: ContentExtractor, A: PageAnalyzer> {
    config: WebAIConfig,
    extractor: E,
    analyzer: A,
    pages: Arc<RwLock<HashMap<Uuid, WebPage>>>,
    summaries: Arc<RwLock<HashMap<Uuid, PageSummary>>>,
}

impl<E: ContentExtractor, A: PageAnalyzer> WebAIEngine<E, A> {
    /// Create a new web AI engine.
    pub fn new(config: WebAIConfig, extractor: E, analyzer: A) -> Self {
        Self {
            config,
            extractor,
            analyzer,
            pages: Arc::new(RwLock::new(HashMap::new())),
            summaries: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Process a page.
    pub async fn process(&self, url: &str, title: &str, html: &str) -> Result<WebPage> {
        let page = WebPage::from_html(url, title, html);

        if self.config.cache_pages {
            self.pages.write().await.insert(page.id, page.clone());
        }

        Ok(page)
    }

    /// Get page summary.
    pub async fn summarize(&self, page_id: Uuid) -> Result<PageSummary> {
        // Check cache
        if let Some(summary) = self.summaries.read().await.get(&page_id) {
            return Ok(summary.clone());
        }

        let page = self
            .pages
            .read()
            .await
            .get(&page_id)
            .cloned()
            .ok_or(WebError::PageNotFound(page_id.to_string()))?;

        let summary = self.analyzer.summarize(&page).await?;

        if self.config.cache_pages {
            self.summaries
                .write()
                .await
                .insert(page_id, summary.clone());
        }

        Ok(summary)
    }

    /// Extract content.
    pub async fn extract(&self, page_id: Uuid) -> Result<ExtractedContent> {
        let page = self
            .pages
            .read()
            .await
            .get(&page_id)
            .cloned()
            .ok_or(WebError::PageNotFound(page_id.to_string()))?;

        self.extractor.extract(&page).await
    }

    /// Get reading mode content.
    pub async fn reading_mode(&self, page_id: Uuid) -> Result<String> {
        let content = self.extract(page_id).await?;
        let page = self
            .pages
            .read()
            .await
            .get(&page_id)
            .cloned()
            .ok_or(WebError::PageNotFound(page_id.to_string()))?;

        let mut reading = format!("# {}\n\n", page.title);
        reading.push_str(&content.article);

        Ok(reading)
    }

    /// Suggest form fills.
    pub async fn fill_form(
        &self,
        page_id: Uuid,
        user_data: &HashMap<String, String>,
    ) -> Result<Vec<FormFillSuggestion>> {
        let page = self
            .pages
            .read()
            .await
            .get(&page_id)
            .cloned()
            .ok_or(WebError::PageNotFound(page_id.to_string()))?;

        let forms = self.extractor.extract_forms(&page).await?;
        let fields: Vec<_> = forms.into_iter().flatten().collect();

        Ok(self.analyzer.suggest_fills(&fields, user_data))
    }

    /// Search cached pages.
    pub async fn search(&self, query: &str) -> Vec<(WebPage, f32)> {
        let query_lower = query.to_lowercase();
        let query_words: std::collections::HashSet<_> = query_lower.split_whitespace().collect();

        self.pages
            .read()
            .await
            .values()
            .filter_map(|page| {
                let page_text_lower = page.text.to_lowercase();
                let page_words: std::collections::HashSet<_> =
                    page_text_lower.split_whitespace().collect();
                let matches = query_words.intersection(&page_words).count();

                if matches > 0 {
                    let score = matches as f32 / query_words.len() as f32;
                    Some((page.clone(), score))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Get statistics.
    pub async fn stats(&self) -> WebStats {
        let pages = self.pages.read().await;
        let summaries = self.summaries.read().await;

        let total_words: usize = pages.values().map(|p| p.word_count()).sum();

        WebStats {
            total_pages: pages.len(),
            total_summaries: summaries.len(),
            total_words,
        }
    }
}

/// Web statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebStats {
    pub total_pages: usize,
    pub total_summaries: usize,
    pub total_words: usize,
}

/// Simple content extractor for testing.
pub struct SimpleExtractor;

#[async_trait]
impl ContentExtractor for SimpleExtractor {
    async fn extract(&self, page: &WebPage) -> Result<ExtractedContent> {
        Ok(ExtractedContent {
            article: page.text.clone(),
            headlines: vec![page.title.clone()],
            links: Vec::new(),
            images: Vec::new(),
            tables: Vec::new(),
            code_blocks: Vec::new(),
        })
    }

    async fn extract_forms(&self, _page: &WebPage) -> Result<Vec<Vec<FormField>>> {
        Ok(Vec::new())
    }
}

/// Simple page analyzer for testing.
pub struct SimpleAnalyzer;

#[async_trait]
impl PageAnalyzer for SimpleAnalyzer {
    async fn summarize(&self, page: &WebPage) -> Result<PageSummary> {
        let words: Vec<_> = page.text.split_whitespace().take(50).collect();
        let reading_time = (page.word_count() / 200).max(1) as u32;

        Ok(PageSummary {
            page_id: page.id,
            brief: words.join(" "),
            key_points: vec![format!("Page about: {}", page.title)],
            reading_time,
            topics: Vec::new(),
            sentiment: None,
        })
    }

    fn suggest_fills(
        &self,
        fields: &[FormField],
        context: &HashMap<String, String>,
    ) -> Vec<FormFillSuggestion> {
        fields
            .iter()
            .filter_map(|field| {
                let key = field.name.to_lowercase();
                context.get(&key).map(|value| FormFillSuggestion {
                    field_name: field.name.clone(),
                    value: value.clone(),
                    confidence: 0.9,
                    source: "user_profile".to_string(),
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_process_page() {
        let engine = WebAIEngine::new(WebAIConfig::default(), SimpleExtractor, SimpleAnalyzer);

        let page = engine
            .process(
                "https://example.com",
                "Example Page",
                "<html><body><p>Hello world</p></body></html>",
            )
            .await
            .unwrap();

        assert_eq!(page.title, "Example Page");
        assert!(page.text.contains("Hello world"));
    }

    #[tokio::test]
    async fn test_summarize() {
        let engine = WebAIEngine::new(WebAIConfig::default(), SimpleExtractor, SimpleAnalyzer);

        let page = engine
            .process(
                "https://example.com",
                "Test",
                "<p>This is a test page with some content.</p>",
            )
            .await
            .unwrap();

        let summary = engine.summarize(page.id).await.unwrap();
        assert!(summary.reading_time > 0);
    }

    #[tokio::test]
    async fn test_reading_mode() {
        let engine = WebAIEngine::new(WebAIConfig::default(), SimpleExtractor, SimpleAnalyzer);

        let page = engine
            .process(
                "https://example.com",
                "Article Title",
                "<p>Article content here.</p>",
            )
            .await
            .unwrap();

        let reading = engine.reading_mode(page.id).await.unwrap();
        assert!(reading.contains("# Article Title"));
    }

    #[tokio::test]
    async fn test_search() {
        let engine = WebAIEngine::new(WebAIConfig::default(), SimpleExtractor, SimpleAnalyzer);

        engine
            .process(
                "https://a.com",
                "Rust Programming",
                "<p>Rust is a systems programming language.</p>",
            )
            .await
            .unwrap();
        engine
            .process(
                "https://b.com",
                "Python Tutorial",
                "<p>Python is great for beginners.</p>",
            )
            .await
            .unwrap();

        let results = engine.search("rust programming").await;
        assert_eq!(results.len(), 1);
    }
}
