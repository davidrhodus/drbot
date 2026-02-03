//! AI-powered submission resolver.
//!
//! Evaluates OSINT submissions using Claude for objective assessment.

use super::types::{Bounty, EvaluationCriteria, Resolution, ResolutionStatus, Submission};
use crate::{Result, SolanaError};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};
use uuid::Uuid;

/// Resolver configuration.
#[derive(Debug, Clone)]
pub struct ResolverConfig {
    /// Model to use for evaluation (e.g., "claude-opus-4-20250514").
    pub model: String,
    /// Maximum tokens for evaluation response.
    pub max_tokens: u32,
    /// Temperature for evaluation (lower = more deterministic).
    pub temperature: f32,
    /// Minimum confidence threshold for approval.
    pub min_confidence: u8,
    /// Whether to allow partial answers.
    pub allow_partial: bool,
}

impl Default for ResolverConfig {
    fn default() -> Self {
        Self {
            model: "claude-opus-4-20250514".to_string(),
            max_tokens: 2048,
            temperature: 0.3,
            min_confidence: 60,
            allow_partial: true,
        }
    }
}

/// Evaluation result from the AI resolver.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluationResult {
    /// Whether the submission is approved.
    pub approved: bool,
    /// Evaluation criteria assessment.
    pub criteria: EvaluationCriteria,
    /// Detailed reasoning.
    pub reasoning: String,
    /// Suggestions for improvement (if rejected).
    pub suggestions: Option<Vec<String>>,
}

/// AI submission resolver.
pub struct SubmissionResolver {
    config: ResolverConfig,
}

impl SubmissionResolver {
    /// Create a new resolver with default config.
    pub fn new() -> Self {
        Self {
            config: ResolverConfig::default(),
        }
    }

    /// Create with custom config.
    pub fn with_config(config: ResolverConfig) -> Self {
        Self { config }
    }

    /// Build the evaluation prompt for Claude.
    pub fn build_evaluation_prompt(&self, bounty: &Bounty, submission: &Submission) -> String {
        let evidence_list = submission
            .evidence
            .iter()
            .enumerate()
            .map(|(i, e)| {
                format!(
                    "{}. [{}] {}\n   Note: {}",
                    i + 1,
                    format!("{:?}", e.evidence_type).to_uppercase(),
                    e.content,
                    e.note.as_deref().unwrap_or("N/A")
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        format!(
            r#"You are an OBJECTIVE evaluator for an OSINT (Open Source Intelligence) marketplace. Your task is to evaluate whether a submission adequately answers a research bounty.

## BOUNTY DETAILS

**Question:** {question}

**Description:** {description}

**Difficulty:** {difficulty:?}

**Tags:** {tags}

## SUBMISSION

**Answer:** {answer}

**Evidence Provided:**
{evidence}

**Methodology:** {methodology}

**Agent's Confidence:** {confidence}%

## EVALUATION CRITERIA

Evaluate the submission against these criteria:

1. **Answers Question**: Does the submission directly address the bounty question?
2. **Has Evidence**: Is evidence provided to support the answer?
3. **Evidence Supports Answer**: Does the evidence actually support the claimed answer?
4. **Methodology Valid**: Is the research methodology sound and reproducible?

## GUIDELINES

- Be OBJECTIVE and fair
- Partial answers can be acceptable if they provide substantial value
- Correct answers with weak but present evidence should generally be approved
- Fabricated or unsupported claims must be rejected
- Consider the difficulty level when evaluating

## RESPONSE FORMAT

Respond with a JSON object in this exact format:
```json
{{
  "approved": true/false,
  "criteria": {{
    "answers_question": true/false,
    "has_evidence": true/false,
    "evidence_supports_answer": true/false,
    "methodology_valid": true/false,
    "confidence": 0-100
  }},
  "reasoning": "Detailed explanation of your evaluation",
  "suggestions": ["Optional improvement suggestions if rejected"]
}}
```

Evaluate the submission now:"#,
            question = bounty.question,
            description = bounty.description,
            difficulty = bounty.difficulty,
            tags = bounty.tags.join(", "),
            answer = submission.answer,
            evidence = if evidence_list.is_empty() {
                "None provided".to_string()
            } else {
                evidence_list
            },
            methodology = submission.methodology,
            confidence = submission.confidence,
        )
    }

    /// Parse the evaluation response from Claude.
    pub fn parse_evaluation_response(&self, response: &str) -> Option<EvaluationResult> {
        // Extract JSON from the response (may be wrapped in markdown code blocks)
        let json_str = if let Some(start) = response.find('{') {
            if let Some(end) = response.rfind('}') {
                &response[start..=end]
            } else {
                return None;
            }
        } else {
            return None;
        };

        // Parse the JSON
        match serde_json::from_str::<EvaluationResult>(json_str) {
            Ok(result) => Some(result),
            Err(e) => {
                warn!(error = %e, "Failed to parse evaluation response");
                None
            }
        }
    }

    /// Resolve a submission (without API call - for use with external Claude client).
    pub fn resolve_with_evaluation(
        &self,
        bounty_id: Uuid,
        submission: &Submission,
        evaluation: EvaluationResult,
    ) -> Resolution {
        let status = if evaluation.approved {
            ResolutionStatus::Approved
        } else {
            ResolutionStatus::Rejected
        };

        info!(
            bounty_id = %bounty_id,
            submission_id = %submission.id,
            approved = evaluation.approved,
            confidence = evaluation.criteria.confidence,
            "Submission resolved"
        );

        Resolution {
            id: Uuid::new_v4(),
            bounty_id,
            submission_id: submission.id,
            status,
            reasoning: evaluation.reasoning,
            resolver_id: format!("claude-{}", self.config.model),
            payment_tx: None, // Set later after payout
            resolved_at: Utc::now(),
        }
    }

    /// Create a manual resolution.
    pub fn resolve_manually(
        &self,
        bounty_id: Uuid,
        submission_id: Uuid,
        approved: bool,
        reasoning: String,
        resolver_id: String,
    ) -> Resolution {
        Resolution {
            id: Uuid::new_v4(),
            bounty_id,
            submission_id,
            status: if approved {
                ResolutionStatus::Approved
            } else {
                ResolutionStatus::Rejected
            },
            reasoning,
            resolver_id,
            payment_tx: None,
            resolved_at: Utc::now(),
        }
    }

    /// Validate a submission before evaluation.
    pub fn validate_submission(&self, submission: &Submission) -> Vec<String> {
        let mut issues = Vec::new();

        if submission.answer.trim().is_empty() {
            issues.push("Answer is empty".to_string());
        }

        if submission.answer.len() < 50 {
            issues.push("Answer is too short (minimum 50 characters)".to_string());
        }

        if submission.evidence.is_empty() {
            issues.push("No evidence provided".to_string());
        }

        if submission.methodology.trim().is_empty() {
            issues.push("Methodology not described".to_string());
        }

        if submission.confidence == 0 {
            issues.push("Confidence level not set".to_string());
        }

        issues
    }

    /// Get the resolver configuration.
    pub fn config(&self) -> &ResolverConfig {
        &self.config
    }
}

impl Default for SubmissionResolver {
    fn default() -> Self {
        Self::new()
    }
}

/// Evaluation prompt builder for different bounty types.
pub struct PromptBuilder;

impl PromptBuilder {
    /// Build a prompt for fact-checking bounties.
    pub fn fact_check_prompt(claim: &str, context: &str) -> String {
        format!(
            r#"Evaluate the following fact-check submission:

CLAIM TO VERIFY: {claim}

CONTEXT: {context}

Determine if the submission provides adequate verification or refutation of the claim with supporting evidence."#
        )
    }

    /// Build a prompt for person/entity research bounties.
    pub fn entity_research_prompt(entity: &str, questions: &[String]) -> String {
        let questions_list = questions
            .iter()
            .enumerate()
            .map(|(i, q)| format!("{}. {}", i + 1, q))
            .collect::<Vec<_>>()
            .join("\n");

        format!(
            r#"Evaluate research submission about: {entity}

RESEARCH QUESTIONS:
{questions_list}

Assess whether the submission adequately addresses these questions with verifiable evidence."#
        )
    }

    /// Build a prompt for technical investigation bounties.
    pub fn technical_investigation_prompt(target: &str, scope: &str) -> String {
        format!(
            r#"Evaluate technical investigation submission:

TARGET: {target}

SCOPE: {scope}

Assess the technical accuracy, methodology, and evidence quality of the findings."#
        )
    }
}

/// Batch resolver for processing multiple submissions.
pub struct BatchResolver {
    resolver: SubmissionResolver,
    results: Vec<(Uuid, Result<Resolution>)>,
}

impl BatchResolver {
    /// Create a new batch resolver.
    pub fn new(config: ResolverConfig) -> Self {
        Self {
            resolver: SubmissionResolver::with_config(config),
            results: Vec::new(),
        }
    }

    /// Add a resolution result.
    pub fn add_result(&mut self, bounty_id: Uuid, result: Result<Resolution>) {
        self.results.push((bounty_id, result));
    }

    /// Get all results.
    pub fn results(&self) -> &[(Uuid, Result<Resolution>)] {
        &self.results
    }

    /// Get successful resolutions.
    pub fn successful(&self) -> Vec<&Resolution> {
        self.results
            .iter()
            .filter_map(|(_, r)| r.as_ref().ok())
            .collect()
    }

    /// Get failed resolutions.
    pub fn failed(&self) -> Vec<(&Uuid, &SolanaError)> {
        self.results
            .iter()
            .filter_map(|(id, r)| r.as_ref().err().map(|e| (id, e)))
            .collect()
    }

    /// Get the underlying resolver.
    pub fn resolver(&self) -> &SubmissionResolver {
        &self.resolver
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::osint::types::{Difficulty, Evidence, Reward};

    fn test_bounty() -> Bounty {
        Bounty {
            id: Uuid::new_v4(),
            question: "What is the current CEO of Example Corp?".to_string(),
            description: "Find the current CEO of Example Corp as of 2024".to_string(),
            reward: Reward::sol(1.0),
            poster_wallet: solana_sdk::pubkey::Pubkey::new_unique(),
            status: crate::osint::types::BountyStatus::Submitted,
            difficulty: Difficulty::Easy,
            tags: vec!["corporate".to_string(), "leadership".to_string()],
            escrow_tx: Some("test_tx".to_string()),
            created_at: Utc::now(),
            deadline: Utc::now() + chrono::Duration::days(7),
            claimed_by: None,
            claimed_at: None,
            claim_expires_at: None,
            submission: None,
            resolution: None,
        }
    }

    fn test_submission(bounty_id: Uuid) -> Submission {
        Submission::new(
            bounty_id,
            solana_sdk::pubkey::Pubkey::new_unique(),
            "The current CEO of Example Corp is John Smith, appointed in 2022.".to_string(),
            vec![
                Evidence::url(
                    "https://example.com/about/leadership",
                    Some("Official company page".to_string()),
                ),
                Evidence::text(
                    "Press release from January 2022 announcing John Smith as new CEO".to_string(),
                    None,
                ),
            ],
            "Searched company website and verified with news sources.".to_string(),
            85,
        )
    }

    #[test]
    fn test_evaluation_prompt_generation() {
        let resolver = SubmissionResolver::new();
        let bounty = test_bounty();
        let submission = test_submission(bounty.id);

        let prompt = resolver.build_evaluation_prompt(&bounty, &submission);

        assert!(prompt.contains(&bounty.question));
        assert!(prompt.contains(&submission.answer));
        assert!(prompt.contains("OBJECTIVE"));
    }

    #[test]
    fn test_parse_evaluation_response() {
        let resolver = SubmissionResolver::new();

        let response = r#"
Based on my evaluation:

```json
{
  "approved": true,
  "criteria": {
    "answers_question": true,
    "has_evidence": true,
    "evidence_supports_answer": true,
    "methodology_valid": true,
    "confidence": 90
  },
  "reasoning": "The submission correctly identifies John Smith as CEO with supporting evidence.",
  "suggestions": null
}
```
"#;

        let result = resolver.parse_evaluation_response(response);
        assert!(result.is_some());

        let eval = result.unwrap();
        assert!(eval.approved);
        assert!(eval.criteria.answers_question);
        assert_eq!(eval.criteria.confidence, 90);
    }

    #[test]
    fn test_validate_submission() {
        let resolver = SubmissionResolver::new();
        let bounty = test_bounty();
        let submission = test_submission(bounty.id);

        let issues = resolver.validate_submission(&submission);
        assert!(issues.is_empty(), "Valid submission should have no issues");

        // Test with invalid submission
        let invalid = Submission::new(
            bounty.id,
            solana_sdk::pubkey::Pubkey::new_unique(),
            "Short".to_string(),
            vec![],
            "".to_string(),
            0,
        );

        let issues = resolver.validate_submission(&invalid);
        assert!(!issues.is_empty(), "Invalid submission should have issues");
    }

    #[test]
    fn test_resolve_with_evaluation() {
        let resolver = SubmissionResolver::new();
        let bounty = test_bounty();
        let submission = test_submission(bounty.id);

        let evaluation = EvaluationResult {
            approved: true,
            criteria: EvaluationCriteria {
                answers_question: true,
                has_evidence: true,
                evidence_supports_answer: true,
                methodology_valid: true,
                confidence: 90,
            },
            reasoning: "Well-researched submission with good evidence.".to_string(),
            suggestions: None,
        };

        let resolution = resolver.resolve_with_evaluation(bounty.id, &submission, evaluation);

        assert_eq!(resolution.status, ResolutionStatus::Approved);
        assert_eq!(resolution.bounty_id, bounty.id);
        assert_eq!(resolution.submission_id, submission.id);
    }

    #[test]
    fn test_manual_resolution() {
        let resolver = SubmissionResolver::new();
        let bounty_id = Uuid::new_v4();
        let submission_id = Uuid::new_v4();

        let resolution = resolver.resolve_manually(
            bounty_id,
            submission_id,
            false,
            "Manual rejection due to fabricated evidence.".to_string(),
            "admin:alice".to_string(),
        );

        assert_eq!(resolution.status, ResolutionStatus::Rejected);
        assert_eq!(resolution.resolver_id, "admin:alice");
    }
}
