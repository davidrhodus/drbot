//! Self-aware reasoning with epistemic uncertainty tracking.
//!
//! This crate provides metacognitive capabilities that allow the AI to:
//! - Track confidence levels in its own knowledge and reasoning
//! - Identify knowledge gaps and blind spots
//! - Recognize when to ask for clarification vs proceed
//! - Calibrate uncertainty based on domain and context

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Reflection errors.
#[derive(Debug, Error)]
pub enum ReflectError {
    #[error("Analysis failed: {0}")]
    AnalysisFailed(String),

    #[error("Calibration error: {0}")]
    CalibrationError(String),

    #[error("Provider error: {0}")]
    ProviderError(String),

    #[error("Invalid confidence value: {0}")]
    InvalidConfidence(f64),
}

/// Result type for reflection operations.
pub type Result<T> = std::result::Result<T, ReflectError>;

/// Confidence level with calibration metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Confidence {
    /// Raw confidence score (0.0 - 1.0).
    pub score: f64,
    /// Calibrated confidence after domain adjustment.
    pub calibrated: f64,
    /// Confidence interval lower bound.
    pub lower_bound: f64,
    /// Confidence interval upper bound.
    pub upper_bound: f64,
    /// Factors contributing to confidence.
    pub factors: Vec<ConfidenceFactor>,
}

impl Confidence {
    /// Create a new confidence score.
    pub fn new(score: f64) -> Result<Self> {
        if !(0.0..=1.0).contains(&score) {
            return Err(ReflectError::InvalidConfidence(score));
        }

        Ok(Self {
            score,
            calibrated: score,
            lower_bound: (score - 0.1).max(0.0),
            upper_bound: (score + 0.1).min(1.0),
            factors: Vec::new(),
        })
    }

    /// Add a contributing factor.
    pub fn with_factor(mut self, factor: ConfidenceFactor) -> Self {
        self.factors.push(factor);
        self
    }

    /// Apply calibration.
    pub fn calibrate(mut self, calibration: &DomainCalibration) -> Self {
        self.calibrated =
            (self.score * calibration.multiplier + calibration.offset).clamp(0.0, 1.0);
        self.lower_bound = (self.calibrated - calibration.uncertainty).max(0.0);
        self.upper_bound = (self.calibrated + calibration.uncertainty).min(1.0);
        self
    }

    /// Check if this is high confidence.
    pub fn is_high(&self) -> bool {
        self.calibrated >= 0.8
    }

    /// Check if this is low confidence.
    pub fn is_low(&self) -> bool {
        self.calibrated < 0.5
    }
}

/// A factor contributing to confidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfidenceFactor {
    /// Factor name.
    pub name: String,
    /// Impact on confidence (-1.0 to 1.0).
    pub impact: f64,
    /// Reason for this factor.
    pub reason: String,
}

/// Domain-specific calibration parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainCalibration {
    /// Domain identifier.
    pub domain: String,
    /// Multiplier for raw confidence.
    pub multiplier: f64,
    /// Offset to add after multiplication.
    pub offset: f64,
    /// Base uncertainty to add to bounds.
    pub uncertainty: f64,
    /// Number of calibration samples.
    pub sample_count: u32,
}

impl Default for DomainCalibration {
    fn default() -> Self {
        Self {
            domain: "general".to_string(),
            multiplier: 1.0,
            offset: 0.0,
            uncertainty: 0.1,
            sample_count: 0,
        }
    }
}

/// Types of epistemic uncertainty.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UncertaintyType {
    /// Lack of training data in this area.
    DataSparsity,
    /// Known knowledge cutoff issues.
    TemporalGap { cutoff: DateTime<Utc> },
    /// Ambiguous or conflicting information.
    Ambiguity { conflicting_sources: Vec<String> },
    /// Domain expertise limitations.
    DomainBoundary { domain: String },
    /// Reasoning chain uncertainty.
    ReasoningUncertainty { step: String },
    /// Hallucination risk.
    HallucinationRisk { indicators: Vec<String> },
    /// Personal or subjective question.
    SubjectiveDomain,
    /// Rapidly changing information.
    VolatileInformation,
}

/// A knowledge gap identified during reflection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeGap {
    /// Unique identifier.
    pub id: String,
    /// Topic of the gap.
    pub topic: String,
    /// Type of uncertainty.
    pub uncertainty_type: UncertaintyType,
    /// Severity (0.0 - 1.0).
    pub severity: f64,
    /// Suggested remediation.
    pub remediation: GapRemediation,
    /// Related context.
    pub context: String,
}

/// How to remediate a knowledge gap.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GapRemediation {
    /// Ask user for clarification.
    AskUser { question: String },
    /// Search for information.
    Search { query: String },
    /// Consult external tool.
    UseTool {
        tool: String,
        parameters: HashMap<String, String>,
    },
    /// Acknowledge limitation.
    Acknowledge { message: String },
    /// Defer to expert.
    DeferToExpert { domain: String },
    /// Proceed with caution.
    ProceedWithCaution { warnings: Vec<String> },
}

/// A reasoning step with reflection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningStep {
    /// Step identifier.
    pub id: String,
    /// Step description.
    pub description: String,
    /// Input to this step.
    pub input: String,
    /// Output from this step.
    pub output: String,
    /// Confidence in this step.
    pub confidence: Confidence,
    /// Assumptions made.
    pub assumptions: Vec<Assumption>,
    /// Identified gaps.
    pub gaps: Vec<KnowledgeGap>,
}

/// An assumption in reasoning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Assumption {
    /// Assumption statement.
    pub statement: String,
    /// How critical this assumption is.
    pub criticality: AssumptionCriticality,
    /// Whether it's been validated.
    pub validated: bool,
}

/// How critical an assumption is.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum AssumptionCriticality {
    Low,
    Medium,
    High,
    Critical,
}

/// Result of a reflection analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReflectionResult {
    /// Unique identifier.
    pub id: String,
    /// Query that was analyzed.
    pub query: String,
    /// Overall confidence.
    pub confidence: Confidence,
    /// Reasoning chain.
    pub reasoning: Vec<ReasoningStep>,
    /// Identified knowledge gaps.
    pub gaps: Vec<KnowledgeGap>,
    /// Recommendation for proceeding.
    pub recommendation: ActionRecommendation,
    /// Analysis timestamp.
    pub timestamp: DateTime<Utc>,
}

/// Recommendation for how to proceed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ActionRecommendation {
    /// Proceed with high confidence.
    Proceed { message: String },
    /// Proceed but note limitations.
    ProceedWithCaution { warnings: Vec<String> },
    /// Ask for clarification first.
    SeekClarification { questions: Vec<String> },
    /// Gather more information first.
    GatherInformation { sources: Vec<String> },
    /// Decline to answer.
    Decline { reason: String },
}

/// Provider for reflection capabilities.
#[async_trait]
pub trait ReflectionProvider: Send + Sync {
    /// Analyze a query for epistemic uncertainty.
    async fn analyze(&self, query: &str, context: &ReflectionContext) -> Result<ReflectionResult>;

    /// Calibrate confidence for a domain.
    async fn calibrate_domain(
        &self,
        domain: &str,
        samples: &[CalibrationSample],
    ) -> Result<DomainCalibration>;

    /// Identify knowledge gaps in a response.
    async fn identify_gaps(&self, query: &str, response: &str) -> Result<Vec<KnowledgeGap>>;
}

/// Context for reflection analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReflectionContext {
    /// Domain being queried.
    pub domain: Option<String>,
    /// User expertise level.
    pub user_expertise: ExpertiseLevel,
    /// Stakes of the answer.
    pub stakes: Stakes,
    /// Previous interactions for calibration.
    pub history: Vec<InteractionHistory>,
}

/// User's expertise level.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ExpertiseLevel {
    Novice,
    Intermediate,
    Expert,
    Unknown,
}

/// Stakes of getting the answer right.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Stakes {
    Low,
    Medium,
    High,
    Critical,
}

/// Historical interaction for calibration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractionHistory {
    /// Query asked.
    pub query: String,
    /// Response given.
    pub response: String,
    /// Predicted confidence.
    pub predicted_confidence: f64,
    /// Whether it was correct.
    pub was_correct: Option<bool>,
    /// User feedback.
    pub feedback: Option<String>,
}

/// Sample for calibration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalibrationSample {
    /// Predicted confidence.
    pub predicted: f64,
    /// Actual accuracy.
    pub actual: f64,
    /// Domain of the sample.
    pub domain: String,
}

/// The reflection engine.
pub struct ReflectionEngine {
    /// Provider for analysis.
    provider: Arc<dyn ReflectionProvider>,
    /// Domain calibrations.
    calibrations: Arc<RwLock<HashMap<String, DomainCalibration>>>,
    /// Reflection history.
    history: Arc<RwLock<Vec<ReflectionResult>>>,
}

impl ReflectionEngine {
    /// Create a new reflection engine.
    pub fn new(provider: Arc<dyn ReflectionProvider>) -> Self {
        Self {
            provider,
            calibrations: Arc::new(RwLock::new(HashMap::new())),
            history: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Reflect on a query before answering.
    pub async fn reflect(
        &self,
        query: &str,
        context: ReflectionContext,
    ) -> Result<ReflectionResult> {
        let mut result = self.provider.analyze(query, &context).await?;

        // Apply domain calibration if available
        if let Some(domain) = &context.domain {
            let calibrations = self.calibrations.read().await;
            if let Some(calibration) = calibrations.get(domain) {
                result.confidence = result.confidence.calibrate(calibration);
            }
        }

        // Adjust based on stakes
        result.recommendation = self.adjust_for_stakes(&result, context.stakes);

        // Store in history
        let mut history = self.history.write().await;
        history.push(result.clone());

        Ok(result)
    }

    /// Adjust recommendation based on stakes.
    fn adjust_for_stakes(&self, result: &ReflectionResult, stakes: Stakes) -> ActionRecommendation {
        let confidence = result.confidence.calibrated;

        match stakes {
            Stakes::Critical => {
                if confidence < 0.95 || !result.gaps.is_empty() {
                    ActionRecommendation::SeekClarification {
                        questions: result
                            .gaps
                            .iter()
                            .filter_map(|g| match &g.remediation {
                                GapRemediation::AskUser { question } => Some(question.clone()),
                                _ => None,
                            })
                            .collect(),
                    }
                } else {
                    ActionRecommendation::ProceedWithCaution {
                        warnings: vec!["High-stakes decision - please verify".to_string()],
                    }
                }
            }
            Stakes::High => {
                if confidence < 0.8 {
                    ActionRecommendation::SeekClarification {
                        questions: vec!["Could you provide more context?".to_string()],
                    }
                } else if !result.gaps.is_empty() {
                    ActionRecommendation::ProceedWithCaution {
                        warnings: result.gaps.iter().map(|g| g.topic.clone()).collect(),
                    }
                } else {
                    ActionRecommendation::Proceed {
                        message: "Proceeding with high confidence".to_string(),
                    }
                }
            }
            Stakes::Medium | Stakes::Low => {
                if confidence < 0.5 {
                    ActionRecommendation::ProceedWithCaution {
                        warnings: vec!["Lower confidence - consider verification".to_string()],
                    }
                } else {
                    ActionRecommendation::Proceed {
                        message: "Ready to proceed".to_string(),
                    }
                }
            }
        }
    }

    /// Update calibration with new feedback.
    pub async fn update_calibration(&self, domain: &str, sample: CalibrationSample) -> Result<()> {
        let samples = vec![sample];
        let calibration = self.provider.calibrate_domain(domain, &samples).await?;

        let mut calibrations = self.calibrations.write().await;
        calibrations.insert(domain.to_string(), calibration);

        Ok(())
    }

    /// Get current calibration for a domain.
    pub async fn get_calibration(&self, domain: &str) -> Option<DomainCalibration> {
        let calibrations = self.calibrations.read().await;
        calibrations.get(domain).cloned()
    }

    /// Check if response should include uncertainty disclaimer.
    pub fn should_disclaim(&self, result: &ReflectionResult) -> bool {
        result.confidence.calibrated < 0.7
            || !result.gaps.is_empty()
            || result.reasoning.iter().any(|r| r.confidence.is_low())
    }

    /// Generate uncertainty disclaimer.
    pub fn generate_disclaimer(&self, result: &ReflectionResult) -> String {
        let mut parts = Vec::new();

        if result.confidence.calibrated < 0.7 {
            parts.push(format!(
                "My confidence in this response is {:.0}%",
                result.confidence.calibrated * 100.0
            ));
        }

        for gap in &result.gaps {
            match &gap.uncertainty_type {
                UncertaintyType::TemporalGap { cutoff } => {
                    parts.push(format!(
                        "My knowledge may be outdated (cutoff: {})",
                        cutoff.format("%Y-%m")
                    ));
                }
                UncertaintyType::DomainBoundary { domain } => {
                    parts.push(format!(
                        "This touches on {} which is outside my core expertise",
                        domain
                    ));
                }
                UncertaintyType::HallucinationRisk { .. } => {
                    parts.push("I recommend verifying specific facts and figures".to_string());
                }
                _ => {}
            }
        }

        if parts.is_empty() {
            String::new()
        } else {
            format!("Note: {}", parts.join(". "))
        }
    }

    /// Get reflection history.
    pub async fn get_history(&self) -> Vec<ReflectionResult> {
        let history = self.history.read().await;
        history.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockProvider;

    #[async_trait]
    impl ReflectionProvider for MockProvider {
        async fn analyze(
            &self,
            query: &str,
            _context: &ReflectionContext,
        ) -> Result<ReflectionResult> {
            let confidence = if query.contains("uncertain") {
                Confidence::new(0.4)?
            } else {
                Confidence::new(0.85)?
            };

            Ok(ReflectionResult {
                id: Uuid::new_v4().to_string(),
                query: query.to_string(),
                confidence,
                reasoning: vec![ReasoningStep {
                    id: "step1".to_string(),
                    description: "Analyze query".to_string(),
                    input: query.to_string(),
                    output: "Analysis complete".to_string(),
                    confidence: Confidence::new(0.9)?,
                    assumptions: vec![],
                    gaps: vec![],
                }],
                gaps: vec![],
                recommendation: ActionRecommendation::Proceed {
                    message: "Ready".to_string(),
                },
                timestamp: Utc::now(),
            })
        }

        async fn calibrate_domain(
            &self,
            domain: &str,
            _samples: &[CalibrationSample],
        ) -> Result<DomainCalibration> {
            Ok(DomainCalibration {
                domain: domain.to_string(),
                multiplier: 0.9,
                offset: 0.0,
                uncertainty: 0.15,
                sample_count: 10,
            })
        }

        async fn identify_gaps(&self, _query: &str, _response: &str) -> Result<Vec<KnowledgeGap>> {
            Ok(vec![])
        }
    }

    #[tokio::test]
    async fn test_confidence_creation() {
        let conf = Confidence::new(0.85).unwrap();
        assert!(conf.is_high());
        assert!(!conf.is_low());
    }

    #[tokio::test]
    async fn test_confidence_calibration() {
        let conf = Confidence::new(0.8).unwrap();
        let calibration = DomainCalibration {
            domain: "test".to_string(),
            multiplier: 0.9,
            offset: 0.0,
            uncertainty: 0.1,
            sample_count: 5,
        };

        let calibrated = conf.calibrate(&calibration);
        assert!((calibrated.calibrated - 0.72).abs() < 0.01);
    }

    #[tokio::test]
    async fn test_reflect_high_confidence() {
        let provider = Arc::new(MockProvider);
        let engine = ReflectionEngine::new(provider);

        let context = ReflectionContext {
            domain: None,
            user_expertise: ExpertiseLevel::Intermediate,
            stakes: Stakes::Medium,
            history: vec![],
        };

        let result = engine.reflect("normal question", context).await.unwrap();
        assert!(result.confidence.is_high());
        assert!(matches!(
            result.recommendation,
            ActionRecommendation::Proceed { .. }
        ));
    }

    #[tokio::test]
    async fn test_reflect_low_confidence() {
        let provider = Arc::new(MockProvider);
        let engine = ReflectionEngine::new(provider);

        let context = ReflectionContext {
            domain: None,
            user_expertise: ExpertiseLevel::Intermediate,
            stakes: Stakes::High,
            history: vec![],
        };

        let result = engine.reflect("uncertain question", context).await.unwrap();
        assert!(result.confidence.is_low());
        assert!(matches!(
            result.recommendation,
            ActionRecommendation::SeekClarification { .. }
        ));
    }

    #[tokio::test]
    async fn test_critical_stakes() {
        let provider = Arc::new(MockProvider);
        let engine = ReflectionEngine::new(provider);

        let context = ReflectionContext {
            domain: None,
            user_expertise: ExpertiseLevel::Expert,
            stakes: Stakes::Critical,
            history: vec![],
        };

        let result = engine.reflect("normal question", context).await.unwrap();
        // Even high confidence gets cautious treatment for critical stakes
        assert!(matches!(
            result.recommendation,
            ActionRecommendation::SeekClarification { .. }
                | ActionRecommendation::ProceedWithCaution { .. }
        ));
    }

    #[tokio::test]
    async fn test_disclaimer_generation() {
        let provider = Arc::new(MockProvider);
        let engine = ReflectionEngine::new(provider);

        let context = ReflectionContext {
            domain: None,
            user_expertise: ExpertiseLevel::Novice,
            stakes: Stakes::Low,
            history: vec![],
        };

        let result = engine.reflect("uncertain question", context).await.unwrap();
        let disclaimer = engine.generate_disclaimer(&result);
        assert!(disclaimer.contains("confidence"));
    }

    #[tokio::test]
    async fn test_domain_calibration() {
        let provider = Arc::new(MockProvider);
        let engine = ReflectionEngine::new(provider);

        let sample = CalibrationSample {
            predicted: 0.8,
            actual: 0.7,
            domain: "tech".to_string(),
        };

        engine.update_calibration("tech", sample).await.unwrap();
        let cal = engine.get_calibration("tech").await.unwrap();
        assert_eq!(cal.domain, "tech");
    }

    #[test]
    fn test_uncertainty_types() {
        let temporal = UncertaintyType::TemporalGap { cutoff: Utc::now() };
        let domain = UncertaintyType::DomainBoundary {
            domain: "medical".to_string(),
        };
        let risk = UncertaintyType::HallucinationRisk {
            indicators: vec!["specific numbers".to_string()],
        };

        // Ensure serialization works
        let _ = serde_json::to_string(&temporal).unwrap();
        let _ = serde_json::to_string(&domain).unwrap();
        let _ = serde_json::to_string(&risk).unwrap();
    }
}
