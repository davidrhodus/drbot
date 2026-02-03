//! Information distillation for drbot.
//!
//! Provides content aggregation and summarization:
//! - Daily briefings
//! - Research summarization
//! - News aggregation with bias detection
//! - Email/message digests
//! - Thread summarization

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Result type for digest operations.
pub type Result<T> = std::result::Result<T, DigestError>;

/// Digest errors.
#[derive(Debug, thiserror::Error)]
pub enum DigestError {
    #[error("Source unavailable: {0}")]
    SourceUnavailable(String),
    #[error("Processing failed: {0}")]
    ProcessingFailed(String),
    #[error("No content available")]
    NoContent,
    #[error("Rate limited")]
    RateLimited,
}

/// Content source type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceType {
    Email,
    News,
    Slack,
    Discord,
    Twitter,
    RSS,
    Calendar,
    GitHub,
    Research,
    Custom,
}

/// Content item from a source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentItem {
    /// Item ID.
    pub id: Uuid,
    /// Source type.
    pub source: SourceType,
    /// Source name.
    pub source_name: String,
    /// Title.
    pub title: String,
    /// Content body.
    pub content: String,
    /// Author.
    pub author: Option<String>,
    /// URL if applicable.
    pub url: Option<String>,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
    /// Priority score.
    pub priority: f32,
    /// Categories/tags.
    pub tags: Vec<String>,
    /// Metadata.
    pub metadata: HashMap<String, serde_json::Value>,
}

impl ContentItem {
    /// Create a new content item.
    pub fn new(source: SourceType, title: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            source,
            source_name: format!("{:?}", source),
            title: title.into(),
            content: content.into(),
            author: None,
            url: None,
            timestamp: Utc::now(),
            priority: 0.5,
            tags: Vec::new(),
            metadata: HashMap::new(),
        }
    }
}

/// Daily briefing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyBriefing {
    /// Briefing ID.
    pub id: Uuid,
    /// Date of briefing.
    pub date: DateTime<Utc>,
    /// Executive summary.
    pub summary: String,
    /// Top priorities.
    pub priorities: Vec<PriorityItem>,
    /// Section summaries.
    pub sections: Vec<BriefingSection>,
    /// Action items.
    pub action_items: Vec<ActionItem>,
    /// Calendar overview.
    pub calendar: Vec<CalendarItem>,
    /// Stats.
    pub stats: BriefingStats,
}

/// Priority item in briefing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriorityItem {
    /// Description.
    pub description: String,
    /// Urgency level.
    pub urgency: Urgency,
    /// Source.
    pub source: String,
    /// Related items.
    pub related: Vec<Uuid>,
}

/// Urgency level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Urgency {
    Low,
    Medium,
    High,
    Critical,
}

/// Section in briefing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BriefingSection {
    /// Section title.
    pub title: String,
    /// Source type.
    pub source_type: SourceType,
    /// Summary text.
    pub summary: String,
    /// Items in section.
    pub items: Vec<SectionItem>,
    /// Item count.
    pub total_items: usize,
}

/// Item in briefing section.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SectionItem {
    /// Original item ID.
    pub item_id: Uuid,
    /// Title.
    pub title: String,
    /// Brief description.
    pub brief: String,
    /// Priority score.
    pub priority: f32,
}

/// Action item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionItem {
    /// Description.
    pub description: String,
    /// Due date if applicable.
    pub due: Option<DateTime<Utc>>,
    /// Source item.
    pub source: Option<Uuid>,
    /// Status.
    pub status: ActionStatus,
}

/// Action status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionStatus {
    Pending,
    InProgress,
    Done,
    Deferred,
}

/// Calendar item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalendarItem {
    /// Event title.
    pub title: String,
    /// Start time.
    pub start: DateTime<Utc>,
    /// End time.
    pub end: Option<DateTime<Utc>>,
    /// Location.
    pub location: Option<String>,
}

/// Briefing stats.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BriefingStats {
    /// Total items processed.
    pub total_items: usize,
    /// Items per source.
    pub by_source: HashMap<String, usize>,
    /// Unread count.
    pub unread: usize,
    /// High priority count.
    pub high_priority: usize,
}

/// News article with bias analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewsArticle {
    /// Article item.
    pub item: ContentItem,
    /// Summary.
    pub summary: String,
    /// Bias analysis.
    pub bias: BiasAnalysis,
    /// Related articles.
    pub related: Vec<Uuid>,
    /// Key facts.
    pub key_facts: Vec<String>,
}

/// Bias analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BiasAnalysis {
    /// Overall bias score (-1 = left, 0 = center, 1 = right).
    pub political_bias: f32,
    /// Factual accuracy score.
    pub factual_score: f32,
    /// Emotional language score.
    pub emotional_score: f32,
    /// Source credibility.
    pub credibility: f32,
    /// Detected bias types.
    pub bias_types: Vec<BiasType>,
}

/// Type of bias detected.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BiasType {
    /// Bias type name.
    pub name: String,
    /// Confidence.
    pub confidence: f32,
    /// Examples from text.
    pub examples: Vec<String>,
}

/// Thread summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadSummary {
    /// Thread ID.
    pub id: Uuid,
    /// Thread source.
    pub source: SourceType,
    /// Participants.
    pub participants: Vec<String>,
    /// Total messages.
    pub message_count: usize,
    /// Summary.
    pub summary: String,
    /// Key points.
    pub key_points: Vec<String>,
    /// Decisions made.
    pub decisions: Vec<String>,
    /// Open questions.
    pub open_questions: Vec<String>,
    /// Action items.
    pub action_items: Vec<ActionItem>,
    /// Sentiment overview.
    pub sentiment: ThreadSentiment,
}

/// Thread sentiment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadSentiment {
    /// Overall sentiment.
    pub overall: String,
    /// Tone descriptors.
    pub tone: Vec<String>,
    /// Conflict level.
    pub conflict_level: f32,
}

/// Research paper summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchSummary {
    /// Paper title.
    pub title: String,
    /// Authors.
    pub authors: Vec<String>,
    /// Abstract/summary.
    pub abstract_text: String,
    /// Key findings.
    pub key_findings: Vec<String>,
    /// Methodology.
    pub methodology: String,
    /// Limitations.
    pub limitations: Vec<String>,
    /// Relevance score.
    pub relevance: f32,
    /// Citation count if available.
    pub citations: Option<u32>,
}

/// Trait for content sources.
#[async_trait]
pub trait ContentSource: Send + Sync {
    /// Get source type.
    fn source_type(&self) -> SourceType;
    /// Fetch new content.
    async fn fetch(&self, since: Option<DateTime<Utc>>) -> Result<Vec<ContentItem>>;
    /// Get item count.
    async fn count(&self) -> Result<usize>;
}

/// Trait for digest providers.
#[async_trait]
pub trait DigestProvider: Send + Sync {
    /// Generate daily briefing.
    async fn generate_briefing(&self, items: &[ContentItem]) -> Result<DailyBriefing>;
    /// Summarize thread.
    async fn summarize_thread(&self, items: &[ContentItem]) -> Result<ThreadSummary>;
    /// Analyze news for bias.
    async fn analyze_news(&self, item: &ContentItem) -> Result<NewsArticle>;
    /// Summarize research.
    async fn summarize_research(&self, item: &ContentItem) -> Result<ResearchSummary>;
    /// Prioritize items.
    async fn prioritize(&self, items: &mut [ContentItem]);
}

/// Digest engine configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DigestConfig {
    /// Maximum items in briefing.
    pub max_briefing_items: usize,
    /// Priority threshold for inclusion.
    pub priority_threshold: f32,
    /// Enable bias detection.
    pub enable_bias_detection: bool,
    /// Briefing time (hour of day).
    pub briefing_hour: u32,
}

impl Default for DigestConfig {
    fn default() -> Self {
        Self {
            max_briefing_items: 50,
            priority_threshold: 0.3,
            enable_bias_detection: true,
            briefing_hour: 8,
        }
    }
}

/// Digest engine.
pub struct DigestEngine<P: DigestProvider> {
    config: DigestConfig,
    provider: P,
    sources: Arc<RwLock<Vec<Box<dyn ContentSource>>>>,
    items: Arc<RwLock<Vec<ContentItem>>>,
}

impl<P: DigestProvider> DigestEngine<P> {
    /// Create new engine.
    pub fn new(config: DigestConfig, provider: P) -> Self {
        Self {
            config,
            provider,
            sources: Arc::new(RwLock::new(Vec::new())),
            items: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Add content source.
    pub async fn add_source(&self, source: Box<dyn ContentSource>) {
        self.sources.write().await.push(source);
    }

    /// Fetch from all sources.
    pub async fn fetch_all(&self, since: Option<DateTime<Utc>>) -> Result<usize> {
        let sources = self.sources.read().await;
        let mut total = 0;

        for source in sources.iter() {
            match source.fetch(since).await {
                Ok(items) => {
                    total += items.len();
                    self.items.write().await.extend(items);
                }
                Err(e) => {
                    tracing::warn!("Failed to fetch from {:?}: {}", source.source_type(), e);
                }
            }
        }

        Ok(total)
    }

    /// Add content item manually.
    pub async fn add_item(&self, item: ContentItem) {
        self.items.write().await.push(item);
    }

    /// Generate daily briefing.
    pub async fn briefing(&self) -> Result<DailyBriefing> {
        let mut items = self.items.write().await;
        self.provider.prioritize(&mut items);

        let briefing_items: Vec<_> = items
            .iter()
            .filter(|i| i.priority >= self.config.priority_threshold)
            .take(self.config.max_briefing_items)
            .cloned()
            .collect();

        if briefing_items.is_empty() {
            return Err(DigestError::NoContent);
        }

        self.provider.generate_briefing(&briefing_items).await
    }

    /// Summarize a thread of messages.
    pub async fn summarize_thread(&self, items: Vec<ContentItem>) -> Result<ThreadSummary> {
        self.provider.summarize_thread(&items).await
    }

    /// Analyze news article.
    pub async fn analyze_news(&self, item: &ContentItem) -> Result<NewsArticle> {
        self.provider.analyze_news(item).await
    }

    /// Summarize research paper.
    pub async fn summarize_research(&self, item: &ContentItem) -> Result<ResearchSummary> {
        self.provider.summarize_research(item).await
    }

    /// Get items by source.
    pub async fn items_by_source(&self, source: SourceType) -> Vec<ContentItem> {
        self.items
            .read()
            .await
            .iter()
            .filter(|i| i.source == source)
            .cloned()
            .collect()
    }

    /// Clear all items.
    pub async fn clear(&self) {
        self.items.write().await.clear();
    }
}

/// Mock digest provider for testing.
pub struct MockDigestProvider;

#[async_trait]
impl DigestProvider for MockDigestProvider {
    async fn generate_briefing(&self, items: &[ContentItem]) -> Result<DailyBriefing> {
        let mut by_source: HashMap<String, usize> = HashMap::new();
        for item in items {
            *by_source.entry(item.source_name.clone()).or_default() += 1;
        }

        Ok(DailyBriefing {
            id: Uuid::new_v4(),
            date: Utc::now(),
            summary: format!("Daily briefing with {} items", items.len()),
            priorities: items
                .iter()
                .take(3)
                .map(|i| PriorityItem {
                    description: i.title.clone(),
                    urgency: Urgency::Medium,
                    source: i.source_name.clone(),
                    related: vec![],
                })
                .collect(),
            sections: vec![BriefingSection {
                title: "All Items".to_string(),
                source_type: SourceType::Custom,
                summary: format!("{} items to review", items.len()),
                items: items
                    .iter()
                    .take(10)
                    .map(|i| SectionItem {
                        item_id: i.id,
                        title: i.title.clone(),
                        brief: i.content.chars().take(100).collect(),
                        priority: i.priority,
                    })
                    .collect(),
                total_items: items.len(),
            }],
            action_items: vec![],
            calendar: vec![],
            stats: BriefingStats {
                total_items: items.len(),
                by_source,
                unread: items.len(),
                high_priority: items.iter().filter(|i| i.priority > 0.7).count(),
            },
        })
    }

    async fn summarize_thread(&self, items: &[ContentItem]) -> Result<ThreadSummary> {
        let participants: Vec<_> = items
            .iter()
            .filter_map(|i| i.author.clone())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        Ok(ThreadSummary {
            id: Uuid::new_v4(),
            source: items
                .first()
                .map(|i| i.source)
                .unwrap_or(SourceType::Custom),
            participants,
            message_count: items.len(),
            summary: format!("Thread with {} messages", items.len()),
            key_points: vec!["Discussion point 1".to_string()],
            decisions: vec![],
            open_questions: vec![],
            action_items: vec![],
            sentiment: ThreadSentiment {
                overall: "neutral".to_string(),
                tone: vec!["professional".to_string()],
                conflict_level: 0.1,
            },
        })
    }

    async fn analyze_news(&self, item: &ContentItem) -> Result<NewsArticle> {
        Ok(NewsArticle {
            item: item.clone(),
            summary: format!("Summary of: {}", item.title),
            bias: BiasAnalysis {
                political_bias: 0.0,
                factual_score: 0.8,
                emotional_score: 0.3,
                credibility: 0.7,
                bias_types: vec![],
            },
            related: vec![],
            key_facts: vec!["Fact 1".to_string()],
        })
    }

    async fn summarize_research(&self, item: &ContentItem) -> Result<ResearchSummary> {
        Ok(ResearchSummary {
            title: item.title.clone(),
            authors: item.author.clone().map(|a| vec![a]).unwrap_or_default(),
            abstract_text: item.content.chars().take(500).collect(),
            key_findings: vec!["Finding 1".to_string()],
            methodology: "Standard methodology".to_string(),
            limitations: vec!["Limitation 1".to_string()],
            relevance: 0.8,
            citations: None,
        })
    }

    async fn prioritize(&self, items: &mut [ContentItem]) {
        for item in items.iter_mut() {
            // Simple priority based on recency
            let age_hours = (Utc::now() - item.timestamp).num_hours() as f32;
            item.priority = 1.0 / (1.0 + age_hours / 24.0);
        }
        items.sort_by(|a, b| b.priority.partial_cmp(&a.priority).unwrap());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_add_item() {
        let engine = DigestEngine::new(DigestConfig::default(), MockDigestProvider);

        engine
            .add_item(ContentItem::new(SourceType::Email, "Test", "Content"))
            .await;
        engine
            .add_item(ContentItem::new(SourceType::News, "News", "Article"))
            .await;

        let email_items = engine.items_by_source(SourceType::Email).await;
        assert_eq!(email_items.len(), 1);
    }

    #[tokio::test]
    async fn test_briefing() {
        let engine = DigestEngine::new(DigestConfig::default(), MockDigestProvider);

        for i in 0..5 {
            engine
                .add_item(ContentItem::new(
                    SourceType::Email,
                    format!("Email {}", i),
                    "Content",
                ))
                .await;
        }

        let briefing = engine.briefing().await.unwrap();
        assert_eq!(briefing.stats.total_items, 5);
    }

    #[tokio::test]
    async fn test_thread_summary() {
        let engine = DigestEngine::new(DigestConfig::default(), MockDigestProvider);

        let items = vec![
            ContentItem::new(SourceType::Slack, "Message 1", "Hello"),
            ContentItem::new(SourceType::Slack, "Message 2", "World"),
        ];

        let summary = engine.summarize_thread(items).await.unwrap();
        assert_eq!(summary.message_count, 2);
    }

    #[tokio::test]
    async fn test_news_analysis() {
        let engine = DigestEngine::new(DigestConfig::default(), MockDigestProvider);

        let item = ContentItem::new(SourceType::News, "Breaking News", "Article content");
        let analysis = engine.analyze_news(&item).await.unwrap();

        assert!(analysis.bias.factual_score > 0.0);
    }

    #[tokio::test]
    async fn test_research_summary() {
        let engine = DigestEngine::new(DigestConfig::default(), MockDigestProvider);

        let item = ContentItem::new(
            SourceType::Research,
            "Research Paper",
            "Abstract and content",
        );
        let summary = engine.summarize_research(&item).await.unwrap();

        assert!(!summary.title.is_empty());
    }

    #[tokio::test]
    async fn test_no_content_error() {
        let engine = DigestEngine::new(DigestConfig::default(), MockDigestProvider);

        let result = engine.briefing().await;
        assert!(matches!(result, Err(DigestError::NoContent)));
    }
}
