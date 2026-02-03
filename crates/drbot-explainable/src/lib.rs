//! Decision explanations with source attribution.
//!
//! This crate provides explainability capabilities:
//! - Explain reasoning behind decisions
//! - Attribute sources for claims
//! - Show confidence levels
//! - Provide alternative perspectives

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Explainability errors.
#[derive(Debug, Error)]
pub enum ExplainError {
    #[error("Explanation generation failed: {0}")]
    GenerationFailed(String),

    #[error("Source not found: {0}")]
    SourceNotFound(String),

    #[error("Invalid decision: {0}")]
    InvalidDecision(String),

    #[error("Provider error: {0}")]
    ProviderError(String),
}

/// Result type for explainability operations.
pub type Result<T> = std::result::Result<T, ExplainError>;

/// A decision that can be explained.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decision {
    /// Decision identifier.
    pub id: String,
    /// Decision statement.
    pub statement: String,
    /// Decision type.
    pub decision_type: DecisionType,
    /// Confidence (0.0-1.0).
    pub confidence: f64,
    /// Alternatives considered.
    pub alternatives: Vec<Alternative>,
    /// Factors that influenced the decision.
    pub factors: Vec<Factor>,
    /// Sources used.
    pub sources: Vec<Source>,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
}

/// Types of decisions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DecisionType {
    /// Factual assertion.
    Factual,
    /// Recommendation.
    Recommendation,
    /// Classification.
    Classification,
    /// Prediction.
    Prediction,
    /// Analysis.
    Analysis,
    /// Opinion.
    Opinion,
}

/// An alternative option.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alternative {
    /// Alternative statement.
    pub statement: String,
    /// Why not chosen.
    pub rejection_reason: String,
    /// Confidence if chosen.
    pub confidence: f64,
}

/// A factor influencing a decision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Factor {
    /// Factor name.
    pub name: String,
    /// Factor description.
    pub description: String,
    /// Weight/importance (0.0-1.0).
    pub weight: f64,
    /// Direction of influence.
    pub direction: InfluenceDirection,
}

/// Direction of influence.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum InfluenceDirection {
    Positive,
    Negative,
    Neutral,
}

/// A source of information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    /// Source identifier.
    pub id: String,
    /// Source name.
    pub name: String,
    /// Source type.
    pub source_type: SourceType,
    /// URI/reference.
    pub reference: Option<String>,
    /// Reliability score (0.0-1.0).
    pub reliability: f64,
    /// Citation text.
    pub citation: String,
}

/// Types of sources.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SourceType {
    /// Internal knowledge.
    Knowledge,
    /// External document.
    Document,
    /// Web source.
    Web,
    /// User input.
    UserInput,
    /// Previous conversation.
    Conversation,
    /// Computation/calculation.
    Computation,
}

/// An explanation of a decision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Explanation {
    /// Explanation identifier.
    pub id: String,
    /// Decision being explained.
    pub decision_id: String,
    /// Summary explanation.
    pub summary: String,
    /// Detailed explanation.
    pub detailed: String,
    /// Explanation depth.
    pub depth: ExplanationDepth,
    /// Reasoning chain.
    pub reasoning_chain: Vec<ReasoningStep>,
    /// Confidence breakdown.
    pub confidence_breakdown: ConfidenceBreakdown,
    /// Limitations and caveats.
    pub limitations: Vec<String>,
    /// Generated timestamp.
    pub generated_at: DateTime<Utc>,
}

/// Explanation depth.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ExplanationDepth {
    /// Brief, high-level.
    Brief,
    /// Standard detail.
    Standard,
    /// Full technical detail.
    Technical,
    /// Academic/rigorous.
    Academic,
}

/// A step in the reasoning chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningStep {
    /// Step number.
    pub step: u32,
    /// Step description.
    pub description: String,
    /// Premises used.
    pub premises: Vec<String>,
    /// Conclusion.
    pub conclusion: String,
    /// Confidence in this step.
    pub confidence: f64,
    /// Sources supporting this step.
    pub sources: Vec<String>,
}

/// Breakdown of confidence score.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfidenceBreakdown {
    /// Overall confidence.
    pub overall: f64,
    /// Source reliability component.
    pub source_reliability: f64,
    /// Reasoning soundness component.
    pub reasoning_soundness: f64,
    /// Data completeness component.
    pub data_completeness: f64,
    /// Consistency component.
    pub consistency: f64,
}

/// Request for explanation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExplanationRequest {
    /// Decision to explain.
    pub decision: Decision,
    /// Requested depth.
    pub depth: ExplanationDepth,
    /// Target audience.
    pub audience: Audience,
    /// Include alternatives.
    pub include_alternatives: bool,
    /// Include confidence breakdown.
    pub include_confidence: bool,
}

/// Target audience for explanation.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Audience {
    General,
    Technical,
    Expert,
    Child,
}

/// Provider for explanations.
#[async_trait]
pub trait ExplainProvider: Send + Sync {
    /// Generate an explanation.
    async fn explain(&self, request: &ExplanationRequest) -> Result<Explanation>;

    /// Validate source.
    async fn validate_source(&self, source: &Source) -> Result<f64>;

    /// Find supporting sources.
    async fn find_sources(&self, claim: &str) -> Result<Vec<Source>>;
}

/// The explainability engine.
pub struct ExplainEngine {
    /// Provider for explanations.
    provider: Arc<dyn ExplainProvider>,
    /// Decision history.
    decisions: Arc<RwLock<HashMap<String, Decision>>>,
    /// Explanation cache.
    explanations: Arc<RwLock<HashMap<String, Explanation>>>,
    /// Source registry.
    sources: Arc<RwLock<HashMap<String, Source>>>,
}

impl ExplainEngine {
    /// Create a new explainability engine.
    pub fn new(provider: Arc<dyn ExplainProvider>) -> Self {
        Self {
            provider,
            decisions: Arc::new(RwLock::new(HashMap::new())),
            explanations: Arc::new(RwLock::new(HashMap::new())),
            sources: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Record a decision.
    pub async fn record_decision(&self, decision: Decision) -> Result<String> {
        let id = decision.id.clone();

        // Register sources
        let mut sources = self.sources.write().await;
        for source in &decision.sources {
            sources.insert(source.id.clone(), source.clone());
        }
        drop(sources);

        let mut decisions = self.decisions.write().await;
        decisions.insert(id.clone(), decision);

        Ok(id)
    }

    /// Explain a decision.
    pub async fn explain(
        &self,
        decision_id: &str,
        depth: ExplanationDepth,
        audience: Audience,
    ) -> Result<Explanation> {
        let decisions = self.decisions.read().await;
        let decision = decisions
            .get(decision_id)
            .ok_or_else(|| ExplainError::InvalidDecision(decision_id.to_string()))?
            .clone();
        drop(decisions);

        // Check cache
        let cache_key = format!("{}:{:?}:{:?}", decision_id, depth, audience);
        {
            let cache = self.explanations.read().await;
            if let Some(cached) = cache.get(&cache_key) {
                return Ok(cached.clone());
            }
        }

        // Generate explanation
        let request = ExplanationRequest {
            decision,
            depth,
            audience,
            include_alternatives: true,
            include_confidence: true,
        };

        let explanation = self.provider.explain(&request).await?;

        // Cache
        let mut cache = self.explanations.write().await;
        cache.insert(cache_key, explanation.clone());

        Ok(explanation)
    }

    /// Get simple "why" explanation.
    pub async fn why(&self, decision_id: &str) -> Result<String> {
        let explanation = self
            .explain(decision_id, ExplanationDepth::Brief, Audience::General)
            .await?;
        Ok(explanation.summary)
    }

    /// Get sources for a decision.
    pub async fn get_sources(&self, decision_id: &str) -> Result<Vec<Source>> {
        let decisions = self.decisions.read().await;
        let decision = decisions
            .get(decision_id)
            .ok_or_else(|| ExplainError::InvalidDecision(decision_id.to_string()))?;

        Ok(decision.sources.clone())
    }

    /// Validate sources for a decision.
    pub async fn validate_sources(&self, decision_id: &str) -> Result<HashMap<String, f64>> {
        let sources = self.get_sources(decision_id).await?;

        let mut validations = HashMap::new();
        for source in sources {
            let reliability = self.provider.validate_source(&source).await?;
            validations.insert(source.id, reliability);
        }

        Ok(validations)
    }

    /// Find sources for a claim.
    pub async fn find_sources_for(&self, claim: &str) -> Result<Vec<Source>> {
        self.provider.find_sources(claim).await
    }

    /// Get confidence breakdown.
    pub async fn confidence_breakdown(&self, decision_id: &str) -> Result<ConfidenceBreakdown> {
        let decisions = self.decisions.read().await;
        let decision = decisions
            .get(decision_id)
            .ok_or_else(|| ExplainError::InvalidDecision(decision_id.to_string()))?;

        // Calculate breakdown
        let source_reliability: f64 = if decision.sources.is_empty() {
            0.5
        } else {
            decision.sources.iter().map(|s| s.reliability).sum::<f64>()
                / decision.sources.len() as f64
        };

        let factor_confidence: f64 = decision.factors.iter().map(|f| f.weight).sum::<f64>()
            / decision.factors.len().max(1) as f64;

        Ok(ConfidenceBreakdown {
            overall: decision.confidence,
            source_reliability,
            reasoning_soundness: factor_confidence,
            data_completeness: if decision.sources.is_empty() {
                0.3
            } else {
                0.8
            },
            consistency: 0.85,
        })
    }

    /// Get alternative explanations.
    pub async fn get_alternatives(&self, decision_id: &str) -> Result<Vec<Alternative>> {
        let decisions = self.decisions.read().await;
        let decision = decisions
            .get(decision_id)
            .ok_or_else(|| ExplainError::InvalidDecision(decision_id.to_string()))?;

        Ok(decision.alternatives.clone())
    }
}

/// Builder for decisions.
pub struct DecisionBuilder {
    decision: Decision,
}

impl DecisionBuilder {
    /// Create a new decision builder.
    pub fn new(statement: &str, decision_type: DecisionType) -> Self {
        Self {
            decision: Decision {
                id: Uuid::new_v4().to_string(),
                statement: statement.to_string(),
                decision_type,
                confidence: 0.5,
                alternatives: Vec::new(),
                factors: Vec::new(),
                sources: Vec::new(),
                timestamp: Utc::now(),
            },
        }
    }

    /// Set confidence.
    pub fn confidence(mut self, confidence: f64) -> Self {
        self.decision.confidence = confidence.clamp(0.0, 1.0);
        self
    }

    /// Add a factor.
    pub fn factor(
        mut self,
        name: &str,
        description: &str,
        weight: f64,
        direction: InfluenceDirection,
    ) -> Self {
        self.decision.factors.push(Factor {
            name: name.to_string(),
            description: description.to_string(),
            weight: weight.clamp(0.0, 1.0),
            direction,
        });
        self
    }

    /// Add a source.
    pub fn source(mut self, source: Source) -> Self {
        self.decision.sources.push(source);
        self
    }

    /// Add an alternative.
    pub fn alternative(mut self, statement: &str, rejection: &str, confidence: f64) -> Self {
        self.decision.alternatives.push(Alternative {
            statement: statement.to_string(),
            rejection_reason: rejection.to_string(),
            confidence,
        });
        self
    }

    /// Build the decision.
    pub fn build(self) -> Decision {
        self.decision
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockProvider;

    #[async_trait]
    impl ExplainProvider for MockProvider {
        async fn explain(&self, request: &ExplanationRequest) -> Result<Explanation> {
            Ok(Explanation {
                id: Uuid::new_v4().to_string(),
                decision_id: request.decision.id.clone(),
                summary: format!("Because: {}", request.decision.statement),
                detailed: "Detailed explanation here".to_string(),
                depth: request.depth,
                reasoning_chain: vec![ReasoningStep {
                    step: 1,
                    description: "Initial analysis".to_string(),
                    premises: vec!["Input data".to_string()],
                    conclusion: request.decision.statement.clone(),
                    confidence: 0.85,
                    sources: vec![],
                }],
                confidence_breakdown: ConfidenceBreakdown {
                    overall: request.decision.confidence,
                    source_reliability: 0.8,
                    reasoning_soundness: 0.85,
                    data_completeness: 0.9,
                    consistency: 0.9,
                },
                limitations: vec!["Limited data".to_string()],
                generated_at: Utc::now(),
            })
        }

        async fn validate_source(&self, source: &Source) -> Result<f64> {
            Ok(source.reliability)
        }

        async fn find_sources(&self, _claim: &str) -> Result<Vec<Source>> {
            Ok(vec![])
        }
    }

    #[tokio::test]
    async fn test_record_decision() {
        let provider = Arc::new(MockProvider);
        let engine = ExplainEngine::new(provider);

        let decision = DecisionBuilder::new("Test decision", DecisionType::Factual)
            .confidence(0.9)
            .build();

        let id = engine.record_decision(decision).await.unwrap();
        assert!(!id.is_empty());
    }

    #[tokio::test]
    async fn test_explain() {
        let provider = Arc::new(MockProvider);
        let engine = ExplainEngine::new(provider);

        let decision = DecisionBuilder::new("The sky is blue", DecisionType::Factual)
            .confidence(0.95)
            .factor(
                "observation",
                "Direct observation",
                0.9,
                InfluenceDirection::Positive,
            )
            .build();

        let id = engine.record_decision(decision).await.unwrap();
        let explanation = engine
            .explain(&id, ExplanationDepth::Standard, Audience::General)
            .await
            .unwrap();

        assert!(!explanation.summary.is_empty());
    }

    #[tokio::test]
    async fn test_why() {
        let provider = Arc::new(MockProvider);
        let engine = ExplainEngine::new(provider);

        let decision = DecisionBuilder::new("Use Rust", DecisionType::Recommendation)
            .confidence(0.85)
            .build();

        let id = engine.record_decision(decision).await.unwrap();
        let why = engine.why(&id).await.unwrap();

        assert!(why.contains("Use Rust"));
    }

    #[tokio::test]
    async fn test_confidence_breakdown() {
        let provider = Arc::new(MockProvider);
        let engine = ExplainEngine::new(provider);

        let source = Source {
            id: "s1".to_string(),
            name: "Test Source".to_string(),
            source_type: SourceType::Document,
            reference: Some("test.pdf".to_string()),
            reliability: 0.9,
            citation: "Test citation".to_string(),
        };

        let decision = DecisionBuilder::new("Test", DecisionType::Analysis)
            .confidence(0.8)
            .source(source)
            .build();

        let id = engine.record_decision(decision).await.unwrap();
        let breakdown = engine.confidence_breakdown(&id).await.unwrap();

        assert!(breakdown.overall > 0.0);
        assert!(breakdown.source_reliability > 0.0);
    }

    #[test]
    fn test_decision_builder() {
        let decision = DecisionBuilder::new("Test", DecisionType::Prediction)
            .confidence(0.7)
            .factor("data", "Historical data", 0.8, InfluenceDirection::Positive)
            .alternative("Other option", "Less likely", 0.3)
            .build();

        assert_eq!(decision.confidence, 0.7);
        assert_eq!(decision.factors.len(), 1);
        assert_eq!(decision.alternatives.len(), 1);
    }
}
