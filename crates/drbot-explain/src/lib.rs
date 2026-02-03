//! Explainability and reasoning transparency for drbot.
//!
//! Provides insights into AI reasoning and decision-making.
//!
//! # Features
//!
//! - Reasoning chain visualization
//! - Source citation
//! - Confidence scores
//! - Decision explanation

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Explanation result type.
pub type Result<T> = std::result::Result<T, ExplainError>;

/// Explanation errors.
#[derive(Debug, thiserror::Error)]
pub enum ExplainError {
    #[error("No reasoning available")]
    NoReasoning,
    #[error("Invalid response format")]
    InvalidFormat,
}

/// Reasoning chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningChain {
    /// Chain ID.
    pub id: Uuid,
    /// Response ID this explains.
    pub response_id: Uuid,
    /// Steps in the reasoning.
    pub steps: Vec<ReasoningStep>,
    /// Overall confidence.
    pub confidence: f32,
    /// Sources used.
    pub sources: Vec<Source>,
    /// Assumptions made.
    pub assumptions: Vec<String>,
    /// Limitations.
    pub limitations: Vec<String>,
    /// Created at.
    pub created_at: DateTime<Utc>,
}

impl ReasoningChain {
    /// Create a new reasoning chain.
    pub fn new(response_id: Uuid) -> Self {
        Self {
            id: Uuid::new_v4(),
            response_id,
            steps: Vec::new(),
            confidence: 1.0,
            sources: Vec::new(),
            assumptions: Vec::new(),
            limitations: Vec::new(),
            created_at: Utc::now(),
        }
    }

    /// Add a reasoning step.
    pub fn add_step(&mut self, step: ReasoningStep) {
        self.steps.push(step);
        self.recalculate_confidence();
    }

    /// Add a source.
    pub fn add_source(&mut self, source: Source) {
        self.sources.push(source);
    }

    /// Add an assumption.
    pub fn add_assumption(&mut self, assumption: &str) {
        self.assumptions.push(assumption.to_string());
    }

    /// Add a limitation.
    pub fn add_limitation(&mut self, limitation: &str) {
        self.limitations.push(limitation.to_string());
    }

    fn recalculate_confidence(&mut self) {
        if self.steps.is_empty() {
            self.confidence = 1.0;
            return;
        }

        // Confidence is product of step confidences
        self.confidence = self.steps.iter().map(|s| s.confidence).product();
    }

    /// Format as human-readable explanation.
    pub fn format(&self) -> String {
        let mut output = String::new();

        output.push_str("## Reasoning\n\n");

        for (i, step) in self.steps.iter().enumerate() {
            output.push_str(&format!(
                "{}. **{}** (confidence: {:.0}%)\n",
                i + 1,
                step.description,
                step.confidence * 100.0
            ));
            if let Some(detail) = &step.detail {
                output.push_str(&format!("   {}\n", detail));
            }
            output.push('\n');
        }

        if !self.sources.is_empty() {
            output.push_str("## Sources\n\n");
            for source in &self.sources {
                output.push_str(&format!("- {}: {}\n", source.source_type, source.reference));
            }
            output.push('\n');
        }

        if !self.assumptions.is_empty() {
            output.push_str("## Assumptions\n\n");
            for assumption in &self.assumptions {
                output.push_str(&format!("- {}\n", assumption));
            }
            output.push('\n');
        }

        if !self.limitations.is_empty() {
            output.push_str("## Limitations\n\n");
            for limitation in &self.limitations {
                output.push_str(&format!("- {}\n", limitation));
            }
        }

        output.push_str(&format!(
            "\n**Overall confidence: {:.0}%**\n",
            self.confidence * 100.0
        ));

        output
    }
}

/// A step in the reasoning chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningStep {
    /// Step ID.
    pub id: Uuid,
    /// Step type.
    pub step_type: StepType,
    /// Description.
    pub description: String,
    /// Detailed explanation.
    pub detail: Option<String>,
    /// Confidence for this step.
    pub confidence: f32,
    /// Evidence supporting this step.
    pub evidence: Vec<String>,
    /// Dependencies on previous steps.
    pub depends_on: Vec<Uuid>,
}

impl ReasoningStep {
    /// Create a new reasoning step.
    pub fn new(step_type: StepType, description: &str, confidence: f32) -> Self {
        Self {
            id: Uuid::new_v4(),
            step_type,
            description: description.to_string(),
            detail: None,
            confidence: confidence.clamp(0.0, 1.0),
            evidence: Vec::new(),
            depends_on: Vec::new(),
        }
    }

    /// Add detail.
    pub fn with_detail(mut self, detail: &str) -> Self {
        self.detail = Some(detail.to_string());
        self
    }

    /// Add evidence.
    pub fn with_evidence(mut self, evidence: &str) -> Self {
        self.evidence.push(evidence.to_string());
        self
    }
}

/// Step types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepType {
    /// Understanding the query.
    Understanding,
    /// Retrieving information.
    Retrieval,
    /// Analyzing information.
    Analysis,
    /// Making an inference.
    Inference,
    /// Synthesizing answer.
    Synthesis,
    /// Verifying answer.
    Verification,
}

/// Source citation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    /// Source ID.
    pub id: Uuid,
    /// Source type.
    pub source_type: SourceType,
    /// Reference (URL, document name, etc.).
    pub reference: String,
    /// Relevance score.
    pub relevance: f32,
    /// Excerpt used.
    pub excerpt: Option<String>,
    /// Page/section.
    pub location: Option<String>,
}

impl Source {
    /// Create a new source.
    pub fn new(source_type: SourceType, reference: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            source_type,
            reference: reference.to_string(),
            relevance: 1.0,
            excerpt: None,
            location: None,
        }
    }
}

/// Source types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceType {
    /// Web page.
    Web,
    /// Document.
    Document,
    /// Knowledge base.
    KnowledgeBase,
    /// Previous conversation.
    Conversation,
    /// User memory.
    Memory,
    /// Model knowledge.
    ModelKnowledge,
}

impl std::fmt::Display for SourceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SourceType::Web => write!(f, "Web"),
            SourceType::Document => write!(f, "Document"),
            SourceType::KnowledgeBase => write!(f, "Knowledge Base"),
            SourceType::Conversation => write!(f, "Conversation"),
            SourceType::Memory => write!(f, "Memory"),
            SourceType::ModelKnowledge => write!(f, "Model Knowledge"),
        }
    }
}

/// Confidence level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfidenceLevel {
    VeryHigh,
    High,
    Medium,
    Low,
    VeryLow,
}

impl From<f32> for ConfidenceLevel {
    fn from(score: f32) -> Self {
        match score {
            s if s >= 0.9 => ConfidenceLevel::VeryHigh,
            s if s >= 0.7 => ConfidenceLevel::High,
            s if s >= 0.5 => ConfidenceLevel::Medium,
            s if s >= 0.3 => ConfidenceLevel::Low,
            _ => ConfidenceLevel::VeryLow,
        }
    }
}

impl ConfidenceLevel {
    /// Get description.
    pub fn description(&self) -> &str {
        match self {
            ConfidenceLevel::VeryHigh => "Very confident in this answer",
            ConfidenceLevel::High => "Confident in this answer",
            ConfidenceLevel::Medium => "Moderately confident",
            ConfidenceLevel::Low => "Low confidence, may need verification",
            ConfidenceLevel::VeryLow => "Very uncertain, please verify",
        }
    }
}

/// Explanation generator.
pub struct ExplanationGenerator {
    config: ExplanationConfig,
}

impl ExplanationGenerator {
    /// Create a new generator.
    pub fn new(config: ExplanationConfig) -> Self {
        Self { config }
    }

    /// Generate explanation for a response.
    pub fn generate(
        &self,
        response_id: Uuid,
        query: &str,
        response: &str,
        context: &ExplanationContext,
    ) -> ReasoningChain {
        let mut chain = ReasoningChain::new(response_id);

        // Understanding step
        chain.add_step(
            ReasoningStep::new(
                StepType::Understanding,
                "Analyzed the query to understand intent",
                0.95,
            )
            .with_detail(&format!("Query: \"{}\"", query)),
        );

        // Retrieval step if sources were used
        if !context.sources.is_empty() {
            chain.add_step(ReasoningStep::new(
                StepType::Retrieval,
                &format!("Retrieved {} relevant sources", context.sources.len()),
                0.85,
            ));

            for source in &context.sources {
                chain.add_source(source.clone());
            }
        }

        // Analysis step
        chain.add_step(ReasoningStep::new(
            StepType::Analysis,
            "Analyzed available information",
            0.90,
        ));

        // Synthesis step
        chain.add_step(ReasoningStep::new(
            StepType::Synthesis,
            "Synthesized response based on analysis",
            0.85,
        ));

        // Add context assumptions
        for assumption in &context.assumptions {
            chain.add_assumption(assumption);
        }

        // Add limitations
        if context.sources.is_empty() {
            chain.add_limitation("No external sources were consulted");
        }

        chain
    }

    /// Get confidence explanation.
    pub fn explain_confidence(&self, confidence: f32) -> String {
        let level: ConfidenceLevel = confidence.into();
        format!(
            "Confidence: {:.0}% - {}",
            confidence * 100.0,
            level.description()
        )
    }
}

/// Explanation configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExplanationConfig {
    /// Include reasoning steps.
    pub include_reasoning: bool,
    /// Include sources.
    pub include_sources: bool,
    /// Include assumptions.
    pub include_assumptions: bool,
    /// Include confidence scores.
    pub include_confidence: bool,
    /// Verbosity level.
    pub verbosity: Verbosity,
}

impl Default for ExplanationConfig {
    fn default() -> Self {
        Self {
            include_reasoning: true,
            include_sources: true,
            include_assumptions: true,
            include_confidence: true,
            verbosity: Verbosity::Normal,
        }
    }
}

/// Verbosity level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verbosity {
    Minimal,
    Normal,
    Detailed,
}

/// Context for explanation generation.
#[derive(Debug, Clone, Default)]
pub struct ExplanationContext {
    /// Sources used.
    pub sources: Vec<Source>,
    /// Assumptions made.
    pub assumptions: Vec<String>,
    /// Extra context.
    pub extra: HashMap<String, String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reasoning_chain() {
        let mut chain = ReasoningChain::new(Uuid::new_v4());

        chain.add_step(ReasoningStep::new(
            StepType::Understanding,
            "Understood query",
            0.9,
        ));

        chain.add_step(ReasoningStep::new(StepType::Analysis, "Analyzed data", 0.8));

        // Confidence should be 0.9 * 0.8 = 0.72
        assert!((chain.confidence - 0.72).abs() < 0.01);
    }

    #[test]
    fn test_confidence_level() {
        assert_eq!(ConfidenceLevel::from(0.95), ConfidenceLevel::VeryHigh);
        assert_eq!(ConfidenceLevel::from(0.75), ConfidenceLevel::High);
        assert_eq!(ConfidenceLevel::from(0.55), ConfidenceLevel::Medium);
        assert_eq!(ConfidenceLevel::from(0.35), ConfidenceLevel::Low);
        assert_eq!(ConfidenceLevel::from(0.1), ConfidenceLevel::VeryLow);
    }

    #[test]
    fn test_format() {
        let mut chain = ReasoningChain::new(Uuid::new_v4());

        chain.add_step(ReasoningStep::new(
            StepType::Understanding,
            "Understood the question",
            0.95,
        ));

        chain.add_source(Source::new(SourceType::Document, "user-manual.pdf"));
        chain.add_assumption("User is asking about the current version");

        let formatted = chain.format();
        assert!(formatted.contains("Understood the question"));
        assert!(formatted.contains("user-manual.pdf"));
        assert!(formatted.contains("current version"));
    }
}
