//! Information synthesis and intelligence briefing generation.
//!
//! This crate provides capabilities for:
//! - Synthesizing information from multiple sources
//! - Generating executive summaries and briefings
//! - Identifying key insights and connections
//! - Producing actionable intelligence

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Synthesis errors.
#[derive(Debug, Error)]
pub enum SynthesisError {
    #[error("Synthesis failed: {0}")]
    SynthesisFailed(String),

    #[error("Source not found: {0}")]
    SourceNotFound(String),

    #[error("Insufficient information: {0}")]
    InsufficientInformation(String),

    #[error("Provider error: {0}")]
    ProviderError(String),
}

/// Result type for synthesis operations.
pub type Result<T> = std::result::Result<T, SynthesisError>;

/// A source of information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    /// Source identifier.
    pub id: String,
    /// Source name.
    pub name: String,
    /// Source type.
    pub source_type: SourceType,
    /// Content from this source.
    pub content: String,
    /// Source URL if applicable.
    pub url: Option<String>,
    /// Reliability score (0.0-1.0).
    pub reliability: f64,
    /// Timestamp of the information.
    pub timestamp: DateTime<Utc>,
    /// Additional metadata.
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Types of information sources.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SourceType {
    /// Document/file.
    Document,
    /// Web page.
    WebPage,
    /// API response.
    Api,
    /// Database query result.
    Database,
    /// User input.
    UserInput,
    /// Previous conversation.
    Conversation,
    /// Internal knowledge.
    Knowledge,
    /// Custom source.
    Custom(String),
}

/// A synthesized briefing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Briefing {
    /// Briefing identifier.
    pub id: String,
    /// Briefing title.
    pub title: String,
    /// Executive summary.
    pub executive_summary: String,
    /// Detailed sections.
    pub sections: Vec<BriefingSection>,
    /// Key insights.
    pub insights: Vec<Insight>,
    /// Recommendations.
    pub recommendations: Vec<Recommendation>,
    /// Data points referenced.
    pub data_points: Vec<DataPoint>,
    /// Sources used.
    pub sources: Vec<SourceReference>,
    /// Confidence level.
    pub confidence: f64,
    /// Generation timestamp.
    pub generated_at: DateTime<Utc>,
    /// Briefing metadata.
    pub metadata: BriefingMetadata,
}

/// A section in a briefing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BriefingSection {
    /// Section title.
    pub title: String,
    /// Section content.
    pub content: String,
    /// Importance (1-5).
    pub importance: u8,
    /// Related insights.
    pub related_insights: Vec<String>,
}

/// An insight derived from synthesis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Insight {
    /// Insight identifier.
    pub id: String,
    /// Insight statement.
    pub statement: String,
    /// Supporting evidence.
    pub evidence: Vec<String>,
    /// Confidence (0.0-1.0).
    pub confidence: f64,
    /// Impact assessment.
    pub impact: ImpactLevel,
    /// Actionability.
    pub actionable: bool,
    /// Category.
    pub category: InsightCategory,
}

/// Impact level of an insight.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ImpactLevel {
    Low,
    Medium,
    High,
    Critical,
}

/// Categories of insights.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InsightCategory {
    /// Opportunity identified.
    Opportunity,
    /// Risk identified.
    Risk,
    /// Trend observed.
    Trend,
    /// Anomaly detected.
    Anomaly,
    /// Connection discovered.
    Connection,
    /// Gap identified.
    Gap,
    /// Custom category.
    Custom(String),
}

/// A recommendation from the synthesis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recommendation {
    /// Recommendation identifier.
    pub id: String,
    /// Recommendation statement.
    pub statement: String,
    /// Rationale.
    pub rationale: String,
    /// Priority (1-5).
    pub priority: u8,
    /// Effort level.
    pub effort: EffortLevel,
    /// Expected outcome.
    pub expected_outcome: String,
    /// Supporting insights.
    pub supporting_insights: Vec<String>,
}

/// Effort required for a recommendation.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum EffortLevel {
    Minimal,
    Low,
    Medium,
    High,
    Extensive,
}

/// A quantifiable data point.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataPoint {
    /// Data point name.
    pub name: String,
    /// Value.
    pub value: serde_json::Value,
    /// Unit of measurement.
    pub unit: Option<String>,
    /// Trend direction.
    pub trend: Option<Trend>,
    /// Source.
    pub source: String,
}

/// Trend direction.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Trend {
    Rising,
    Stable,
    Falling,
    Volatile,
}

/// Reference to a source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceReference {
    /// Source ID.
    pub source_id: String,
    /// Source name.
    pub name: String,
    /// Relevance (0.0-1.0).
    pub relevance: f64,
    /// Specific citations.
    pub citations: Vec<String>,
}

/// Metadata for a briefing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BriefingMetadata {
    /// Audience level.
    pub audience: AudienceLevel,
    /// Briefing format.
    pub format: BriefingFormat,
    /// Word count.
    pub word_count: usize,
    /// Processing time in ms.
    pub processing_time_ms: u64,
}

/// Target audience level.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum AudienceLevel {
    Executive,
    Manager,
    Technical,
    General,
}

/// Format of the briefing.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum BriefingFormat {
    /// Short executive summary.
    Executive,
    /// Standard briefing.
    Standard,
    /// Detailed report.
    Detailed,
    /// Technical deep-dive.
    Technical,
}

/// Request for synthesis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynthesisRequest {
    /// Topic or question.
    pub topic: String,
    /// Sources to synthesize.
    pub sources: Vec<Source>,
    /// Target audience.
    pub audience: AudienceLevel,
    /// Desired format.
    pub format: BriefingFormat,
    /// Focus areas.
    pub focus_areas: Vec<String>,
    /// Constraints.
    pub constraints: SynthesisConstraints,
}

/// Constraints on synthesis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynthesisConstraints {
    /// Maximum word count.
    pub max_words: Option<usize>,
    /// Maximum sections.
    pub max_sections: Option<usize>,
    /// Include only sources above this reliability.
    pub min_reliability: f64,
    /// Time cutoff for sources.
    pub time_cutoff: Option<DateTime<Utc>>,
}

impl Default for SynthesisConstraints {
    fn default() -> Self {
        Self {
            max_words: None,
            max_sections: Some(5),
            min_reliability: 0.5,
            time_cutoff: None,
        }
    }
}

/// Provider for synthesis capabilities.
#[async_trait]
pub trait SynthesisProvider: Send + Sync {
    /// Synthesize information into a briefing.
    async fn synthesize(&self, request: &SynthesisRequest) -> Result<Briefing>;

    /// Extract key insights from sources.
    async fn extract_insights(&self, sources: &[Source]) -> Result<Vec<Insight>>;

    /// Generate recommendations.
    async fn generate_recommendations(&self, insights: &[Insight]) -> Result<Vec<Recommendation>>;

    /// Summarize a single source.
    async fn summarize(&self, source: &Source, max_words: usize) -> Result<String>;
}

/// The synthesis engine.
pub struct SynthesisEngine {
    /// Provider for synthesis.
    provider: Arc<dyn SynthesisProvider>,
    /// Cache of recent briefings.
    briefing_cache: Arc<RwLock<HashMap<String, Briefing>>>,
    /// Source registry.
    sources: Arc<RwLock<HashMap<String, Source>>>,
}

impl SynthesisEngine {
    /// Create a new synthesis engine.
    pub fn new(provider: Arc<dyn SynthesisProvider>) -> Self {
        Self {
            provider,
            briefing_cache: Arc::new(RwLock::new(HashMap::new())),
            sources: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a source.
    pub async fn register_source(&self, source: Source) -> Result<String> {
        let id = source.id.clone();
        let mut sources = self.sources.write().await;
        sources.insert(id.clone(), source);
        Ok(id)
    }

    /// Get a registered source.
    pub async fn get_source(&self, id: &str) -> Option<Source> {
        let sources = self.sources.read().await;
        sources.get(id).cloned()
    }

    /// Create a synthesis request.
    pub fn create_request(
        topic: &str,
        sources: Vec<Source>,
        audience: AudienceLevel,
        format: BriefingFormat,
    ) -> SynthesisRequest {
        SynthesisRequest {
            topic: topic.to_string(),
            sources,
            audience,
            format,
            focus_areas: Vec::new(),
            constraints: SynthesisConstraints::default(),
        }
    }

    /// Synthesize a briefing.
    pub async fn synthesize(&self, request: SynthesisRequest) -> Result<Briefing> {
        let start_time = std::time::Instant::now();

        // Filter sources by constraints
        let filtered_sources: Vec<_> = request
            .sources
            .iter()
            .filter(|s| s.reliability >= request.constraints.min_reliability)
            .filter(|s| {
                if let Some(cutoff) = request.constraints.time_cutoff {
                    s.timestamp >= cutoff
                } else {
                    true
                }
            })
            .cloned()
            .collect();

        if filtered_sources.is_empty() {
            return Err(SynthesisError::InsufficientInformation(
                "No sources meet the reliability threshold".to_string(),
            ));
        }

        let mut briefing = self
            .provider
            .synthesize(&SynthesisRequest {
                sources: filtered_sources,
                ..request
            })
            .await?;

        briefing.metadata.processing_time_ms = start_time.elapsed().as_millis() as u64;

        // Cache the briefing
        let mut cache = self.briefing_cache.write().await;
        cache.insert(briefing.id.clone(), briefing.clone());

        Ok(briefing)
    }

    /// Quick synthesis with default settings.
    pub async fn quick_synthesis(&self, topic: &str, sources: Vec<Source>) -> Result<Briefing> {
        let request = Self::create_request(
            topic,
            sources,
            AudienceLevel::General,
            BriefingFormat::Standard,
        );
        self.synthesize(request).await
    }

    /// Generate an executive summary.
    pub async fn executive_summary(&self, topic: &str, sources: Vec<Source>) -> Result<String> {
        let request = SynthesisRequest {
            topic: topic.to_string(),
            sources,
            audience: AudienceLevel::Executive,
            format: BriefingFormat::Executive,
            focus_areas: Vec::new(),
            constraints: SynthesisConstraints {
                max_words: Some(200),
                ..Default::default()
            },
        };

        let briefing = self.synthesize(request).await?;
        Ok(briefing.executive_summary)
    }

    /// Extract insights from sources.
    pub async fn extract_insights(&self, sources: &[Source]) -> Result<Vec<Insight>> {
        self.provider.extract_insights(sources).await
    }

    /// Generate recommendations from insights.
    pub async fn generate_recommendations(
        &self,
        insights: &[Insight],
    ) -> Result<Vec<Recommendation>> {
        self.provider.generate_recommendations(insights).await
    }

    /// Get a cached briefing.
    pub async fn get_briefing(&self, id: &str) -> Option<Briefing> {
        let cache = self.briefing_cache.read().await;
        cache.get(id).cloned()
    }

    /// Compare multiple briefings.
    pub async fn compare_briefings(&self, ids: &[String]) -> Result<ComparisonResult> {
        let cache = self.briefing_cache.read().await;

        let briefings: Vec<_> = ids.iter().filter_map(|id| cache.get(id).cloned()).collect();

        if briefings.len() < 2 {
            return Err(SynthesisError::InsufficientInformation(
                "Need at least 2 briefings to compare".to_string(),
            ));
        }

        // Find common and unique insights
        let all_insights: Vec<_> = briefings.iter().flat_map(|b| b.insights.clone()).collect();

        let common_themes: Vec<String> = all_insights
            .iter()
            .filter(|i| {
                briefings.iter().all(|b| {
                    b.insights.iter().any(|bi| {
                        bi.statement
                            .contains(&i.statement[..20.min(i.statement.len())])
                    })
                })
            })
            .map(|i| i.statement.clone())
            .collect();

        Ok(ComparisonResult {
            briefing_ids: ids.to_vec(),
            common_themes,
            divergent_points: Vec::new(),
            confidence_range: (
                briefings
                    .iter()
                    .map(|b| b.confidence)
                    .fold(f64::MAX, f64::min),
                briefings
                    .iter()
                    .map(|b| b.confidence)
                    .fold(f64::MIN, f64::max),
            ),
        })
    }
}

/// Result of comparing briefings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComparisonResult {
    /// Briefing IDs compared.
    pub briefing_ids: Vec<String>,
    /// Common themes across briefings.
    pub common_themes: Vec<String>,
    /// Points of divergence.
    pub divergent_points: Vec<String>,
    /// Range of confidence levels.
    pub confidence_range: (f64, f64),
}

/// Builder for sources.
pub struct SourceBuilder {
    source: Source,
}

impl SourceBuilder {
    /// Create a new source builder.
    pub fn new(name: &str, source_type: SourceType) -> Self {
        Self {
            source: Source {
                id: Uuid::new_v4().to_string(),
                name: name.to_string(),
                source_type,
                content: String::new(),
                url: None,
                reliability: 0.8,
                timestamp: Utc::now(),
                metadata: HashMap::new(),
            },
        }
    }

    /// Set content.
    pub fn content(mut self, content: &str) -> Self {
        self.source.content = content.to_string();
        self
    }

    /// Set URL.
    pub fn url(mut self, url: &str) -> Self {
        self.source.url = Some(url.to_string());
        self
    }

    /// Set reliability.
    pub fn reliability(mut self, reliability: f64) -> Self {
        self.source.reliability = reliability.clamp(0.0, 1.0);
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

    struct MockProvider;

    #[async_trait]
    impl SynthesisProvider for MockProvider {
        async fn synthesize(&self, request: &SynthesisRequest) -> Result<Briefing> {
            let insights = self.extract_insights(&request.sources).await?;
            let recommendations = self.generate_recommendations(&insights).await?;

            Ok(Briefing {
                id: Uuid::new_v4().to_string(),
                title: format!("Briefing: {}", request.topic),
                executive_summary: format!(
                    "Summary of {} sources on {}",
                    request.sources.len(),
                    request.topic
                ),
                sections: vec![BriefingSection {
                    title: "Overview".to_string(),
                    content: "Overview content".to_string(),
                    importance: 5,
                    related_insights: vec![],
                }],
                insights,
                recommendations,
                data_points: vec![],
                sources: request
                    .sources
                    .iter()
                    .map(|s| SourceReference {
                        source_id: s.id.clone(),
                        name: s.name.clone(),
                        relevance: s.reliability,
                        citations: vec![],
                    })
                    .collect(),
                confidence: 0.85,
                generated_at: Utc::now(),
                metadata: BriefingMetadata {
                    audience: request.audience,
                    format: request.format,
                    word_count: 500,
                    processing_time_ms: 0,
                },
            })
        }

        async fn extract_insights(&self, sources: &[Source]) -> Result<Vec<Insight>> {
            Ok(sources
                .iter()
                .map(|s| Insight {
                    id: Uuid::new_v4().to_string(),
                    statement: format!("Insight from {}", s.name),
                    evidence: vec![s.content.clone()],
                    confidence: s.reliability,
                    impact: ImpactLevel::Medium,
                    actionable: true,
                    category: InsightCategory::Opportunity,
                })
                .collect())
        }

        async fn generate_recommendations(
            &self,
            insights: &[Insight],
        ) -> Result<Vec<Recommendation>> {
            Ok(insights
                .iter()
                .filter(|i| i.actionable)
                .map(|i| Recommendation {
                    id: Uuid::new_v4().to_string(),
                    statement: format!("Recommendation based on: {}", i.statement),
                    rationale: "Based on extracted insight".to_string(),
                    priority: 3,
                    effort: EffortLevel::Medium,
                    expected_outcome: "Positive outcome".to_string(),
                    supporting_insights: vec![i.id.clone()],
                })
                .collect())
        }

        async fn summarize(&self, source: &Source, max_words: usize) -> Result<String> {
            let words: Vec<_> = source.content.split_whitespace().take(max_words).collect();
            Ok(words.join(" "))
        }
    }

    #[tokio::test]
    async fn test_synthesize() {
        let provider = Arc::new(MockProvider);
        let engine = SynthesisEngine::new(provider);

        let sources = vec![
            SourceBuilder::new("Doc1", SourceType::Document)
                .content("Important information about topic")
                .reliability(0.9)
                .build(),
            SourceBuilder::new("Doc2", SourceType::Document)
                .content("Additional data on topic")
                .reliability(0.8)
                .build(),
        ];

        let request = SynthesisEngine::create_request(
            "Test Topic",
            sources,
            AudienceLevel::Manager,
            BriefingFormat::Standard,
        );

        let briefing = engine.synthesize(request).await.unwrap();

        assert!(briefing.title.contains("Test Topic"));
        assert_eq!(briefing.sources.len(), 2);
        assert!(!briefing.insights.is_empty());
    }

    #[tokio::test]
    async fn test_executive_summary() {
        let provider = Arc::new(MockProvider);
        let engine = SynthesisEngine::new(provider);

        let sources = vec![SourceBuilder::new("Report", SourceType::Document)
            .content("Detailed report content")
            .build()];

        let summary = engine
            .executive_summary("Q4 Results", sources)
            .await
            .unwrap();
        assert!(!summary.is_empty());
    }

    #[tokio::test]
    async fn test_source_filtering() {
        let provider = Arc::new(MockProvider);
        let engine = SynthesisEngine::new(provider);

        let sources = vec![
            SourceBuilder::new("Reliable", SourceType::Document)
                .content("Reliable content")
                .reliability(0.9)
                .build(),
            SourceBuilder::new("Unreliable", SourceType::Document)
                .content("Unreliable content")
                .reliability(0.2)
                .build(),
        ];

        let request = SynthesisRequest {
            topic: "Test".to_string(),
            sources,
            audience: AudienceLevel::General,
            format: BriefingFormat::Standard,
            focus_areas: vec![],
            constraints: SynthesisConstraints {
                min_reliability: 0.5,
                ..Default::default()
            },
        };

        let briefing = engine.synthesize(request).await.unwrap();
        assert_eq!(briefing.sources.len(), 1);
    }

    #[tokio::test]
    async fn test_source_builder() {
        let source = SourceBuilder::new("Test", SourceType::WebPage)
            .content("Content here")
            .url("https://example.com")
            .reliability(0.95)
            .build();

        assert_eq!(source.name, "Test");
        assert_eq!(source.reliability, 0.95);
        assert!(source.url.is_some());
    }

    #[tokio::test]
    async fn test_extract_insights() {
        let provider = Arc::new(MockProvider);
        let engine = SynthesisEngine::new(provider);

        let sources = vec![
            SourceBuilder::new("S1", SourceType::Document)
                .content("Finding 1")
                .build(),
            SourceBuilder::new("S2", SourceType::Document)
                .content("Finding 2")
                .build(),
        ];

        let insights = engine.extract_insights(&sources).await.unwrap();
        assert_eq!(insights.len(), 2);
    }

    #[test]
    fn test_impact_levels() {
        let low = ImpactLevel::Low;
        let high = ImpactLevel::High;

        let _ = serde_json::to_string(&low).unwrap();
        let _ = serde_json::to_string(&high).unwrap();
    }
}
