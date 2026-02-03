//! Verification and fact-checking system for drbot
//!
//! Validates claims, provides citations, and measures confidence.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;

#[derive(Error, Debug)]
pub enum VerifyError {
    #[error("Verification failed: {0}")]
    VerificationFailed(String),
    #[error("Source not accessible: {0}")]
    SourceNotAccessible(String),
    #[error("Insufficient evidence: {0}")]
    InsufficientEvidence(String),
    #[error("Provider error: {0}")]
    ProviderError(String),
}

pub type Result<T> = std::result::Result<T, VerifyError>;

// ============================================================================
// Claims and Verification
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claim {
    pub id: String,
    pub text: String,
    pub source: Option<String>,
    pub context: Option<String>,
    pub claim_type: ClaimType,
    pub created_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ClaimType {
    Factual,
    Statistical,
    Temporal,
    Attribution,
    Scientific,
    Historical,
    Legal,
    Technical,
    Opinion,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    pub claim_id: String,
    pub verdict: Verdict,
    pub confidence: f32,
    pub evidence: Vec<Evidence>,
    pub counter_evidence: Vec<Evidence>,
    pub explanation: String,
    pub corrections: Vec<Correction>,
    pub verified_at: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum Verdict {
    Verified,
    PartiallyVerified,
    Unverified,
    Disputed,
    False,
    Outdated,
    Unverifiable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
    pub source: Source,
    pub quote: Option<String>,
    pub relevance: f32,
    pub supports_claim: bool,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    pub id: String,
    pub title: String,
    pub url: Option<String>,
    pub author: Option<String>,
    pub published_date: Option<String>,
    pub source_type: SourceType,
    pub credibility: SourceCredibility,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SourceType {
    PeerReviewed,
    Government,
    News,
    Encyclopedia,
    Book,
    Website,
    SocialMedia,
    Primary,
    Secondary,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum SourceCredibility {
    VeryHigh,
    High,
    Medium,
    Low,
    VeryLow,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Correction {
    pub original: String,
    pub corrected: String,
    pub explanation: String,
    pub source: Option<Source>,
}

// ============================================================================
// Citation Management
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Citation {
    pub id: String,
    pub source: Source,
    pub format: CitationFormat,
    pub formatted: String,
    pub in_text: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum CitationFormat {
    APA,
    MLA,
    Chicago,
    Harvard,
    IEEE,
    Vancouver,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CitationRequest {
    pub title: String,
    pub authors: Vec<String>,
    pub published_date: Option<String>,
    pub url: Option<String>,
    pub publisher: Option<String>,
    pub volume: Option<String>,
    pub issue: Option<String>,
    pub pages: Option<String>,
    pub doi: Option<String>,
    pub source_type: SourceType,
}

// ============================================================================
// Confidence Assessment
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfidenceAssessment {
    pub statement: String,
    pub overall_confidence: f32,
    pub components: Vec<ConfidenceComponent>,
    pub uncertainty_factors: Vec<UncertaintyFactor>,
    pub recommendations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfidenceComponent {
    pub name: String,
    pub score: f32,
    pub weight: f32,
    pub explanation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UncertaintyFactor {
    pub factor: String,
    pub impact: UncertaintyImpact,
    pub description: String,
    pub mitigation: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum UncertaintyImpact {
    Critical,
    High,
    Medium,
    Low,
    Minimal,
}

// ============================================================================
// Bias Detection
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BiasAnalysis {
    pub text: String,
    pub overall_bias_score: f32,
    pub detected_biases: Vec<DetectedBias>,
    pub neutral_alternative: Option<String>,
    pub balance_assessment: BalanceAssessment,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedBias {
    pub bias_type: BiasType,
    pub severity: BiasSeverity,
    pub location: String,
    pub explanation: String,
    pub suggestion: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BiasType {
    Political,
    Confirmation,
    Selection,
    Framing,
    LoadedLanguage,
    Omission,
    Source,
    Cultural,
    Recency,
    Authority,
    Emotional,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum BiasSeverity {
    Severe,
    Significant,
    Moderate,
    Mild,
    Minimal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BalanceAssessment {
    pub perspectives_represented: Vec<String>,
    pub missing_perspectives: Vec<String>,
    pub balance_score: f32,
    pub recommendations: Vec<String>,
}

// ============================================================================
// Consistency Checking
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsistencyCheck {
    pub statements: Vec<String>,
    pub is_consistent: bool,
    pub contradictions: Vec<Contradiction>,
    pub logical_issues: Vec<LogicalIssue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contradiction {
    pub statement_a: String,
    pub statement_b: String,
    pub explanation: String,
    pub resolution: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogicalIssue {
    pub issue_type: LogicalIssueType,
    pub statement: String,
    pub explanation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum LogicalIssueType {
    Contradiction,
    FalseEquivalence,
    CircularReasoning,
    NonSequitur,
    StrawMan,
    AppealToAuthority,
    SlipperySlope,
    FalseDichotomy,
    HastyGeneralization,
    AdHominem,
}

// ============================================================================
// Provider Trait
// ============================================================================

#[async_trait]
pub trait VerifyProvider: Send + Sync {
    async fn verify_claim(&self, claim: &Claim) -> Result<VerificationResult>;
    async fn find_sources(&self, query: &str, source_types: &[SourceType]) -> Result<Vec<Source>>;
    async fn assess_confidence(
        &self,
        statement: &str,
        context: Option<&str>,
    ) -> Result<ConfidenceAssessment>;
    async fn detect_bias(&self, text: &str) -> Result<BiasAnalysis>;
    async fn check_consistency(&self, statements: &[String]) -> Result<ConsistencyCheck>;
    async fn format_citation(
        &self,
        request: CitationRequest,
        format: CitationFormat,
    ) -> Result<Citation>;
}

// ============================================================================
// Verify Engine
// ============================================================================

pub struct VerifyEngine {
    provider: Arc<dyn VerifyProvider>,
    claims: Arc<RwLock<HashMap<String, Claim>>>,
    verifications: Arc<RwLock<HashMap<String, VerificationResult>>>,
    sources: Arc<RwLock<HashMap<String, Source>>>,
    citations: Arc<RwLock<HashMap<String, Citation>>>,
    next_claim_id: Arc<RwLock<u64>>,
}

impl VerifyEngine {
    pub fn new(provider: Arc<dyn VerifyProvider>) -> Self {
        Self {
            provider,
            claims: Arc::new(RwLock::new(HashMap::new())),
            verifications: Arc::new(RwLock::new(HashMap::new())),
            sources: Arc::new(RwLock::new(HashMap::new())),
            citations: Arc::new(RwLock::new(HashMap::new())),
            next_claim_id: Arc::new(RwLock::new(1)),
        }
    }

    fn now() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    async fn generate_claim_id(&self) -> String {
        let mut id = self.next_claim_id.write().await;
        let claim_id = format!("claim-{}", *id);
        *id += 1;
        claim_id
    }

    // Claim Management
    pub async fn create_claim(&self, text: &str, claim_type: ClaimType) -> Result<Claim> {
        let claim = Claim {
            id: self.generate_claim_id().await,
            text: text.to_string(),
            source: None,
            context: None,
            claim_type,
            created_at: Self::now(),
        };

        let mut claims = self.claims.write().await;
        claims.insert(claim.id.clone(), claim.clone());

        Ok(claim)
    }

    pub async fn get_claim(&self, claim_id: &str) -> Result<Claim> {
        let claims = self.claims.read().await;
        claims.get(claim_id).cloned().ok_or_else(|| {
            VerifyError::VerificationFailed(format!("Claim not found: {}", claim_id))
        })
    }

    // Verification
    pub async fn verify(&self, text: &str, claim_type: ClaimType) -> Result<VerificationResult> {
        let claim = self.create_claim(text, claim_type).await?;
        self.verify_claim(&claim.id).await
    }

    pub async fn verify_claim(&self, claim_id: &str) -> Result<VerificationResult> {
        let claim = self.get_claim(claim_id).await?;
        let result = self.provider.verify_claim(&claim).await?;

        // Store sources
        {
            let mut sources = self.sources.write().await;
            for evidence in &result.evidence {
                sources.insert(evidence.source.id.clone(), evidence.source.clone());
            }
            for evidence in &result.counter_evidence {
                sources.insert(evidence.source.id.clone(), evidence.source.clone());
            }
        }

        // Store result
        let mut verifications = self.verifications.write().await;
        verifications.insert(claim_id.to_string(), result.clone());

        Ok(result)
    }

    pub async fn get_verification(&self, claim_id: &str) -> Option<VerificationResult> {
        let verifications = self.verifications.read().await;
        verifications.get(claim_id).cloned()
    }

    pub async fn verify_multiple(&self, texts: &[&str]) -> Result<Vec<VerificationResult>> {
        let mut results = Vec::new();
        for text in texts {
            let result = self.verify(text, ClaimType::Factual).await?;
            results.push(result);
        }
        Ok(results)
    }

    // Quick verification
    pub async fn is_true(&self, statement: &str) -> Result<bool> {
        let result = self.verify(statement, ClaimType::Factual).await?;
        Ok(matches!(
            result.verdict,
            Verdict::Verified | Verdict::PartiallyVerified
        ))
    }

    pub async fn fact_check(&self, text: &str) -> Result<FactCheckSummary> {
        // Extract claims from text (simplified - in practice would use NLP)
        let sentences: Vec<&str> = text.split('.').filter(|s| !s.trim().is_empty()).collect();

        let mut verified_count = 0;
        let mut unverified_count = 0;
        let mut false_count = 0;
        let mut claim_results = Vec::new();

        for sentence in sentences.iter().take(5) {
            // Limit for demo
            let result = self.verify(sentence.trim(), ClaimType::Factual).await?;
            match result.verdict {
                Verdict::Verified | Verdict::PartiallyVerified => verified_count += 1,
                Verdict::False | Verdict::Disputed => false_count += 1,
                _ => unverified_count += 1,
            }
            claim_results.push((sentence.to_string(), result));
        }

        let total = claim_results.len() as f32;
        let accuracy = if total > 0.0 {
            verified_count as f32 / total
        } else {
            0.0
        };

        Ok(FactCheckSummary {
            total_claims: claim_results.len(),
            verified_count,
            unverified_count,
            false_count,
            accuracy_score: accuracy,
            claims: claim_results,
        })
    }

    // Source Management
    pub async fn find_sources(&self, query: &str) -> Result<Vec<Source>> {
        self.provider.find_sources(query, &[]).await
    }

    pub async fn find_credible_sources(
        &self,
        query: &str,
        min_credibility: SourceCredibility,
    ) -> Result<Vec<Source>> {
        let sources = self
            .provider
            .find_sources(
                query,
                &[
                    SourceType::PeerReviewed,
                    SourceType::Government,
                    SourceType::Encyclopedia,
                ],
            )
            .await?;

        let min_cred_ord = credibility_to_ord(min_credibility);
        Ok(sources
            .into_iter()
            .filter(|s| credibility_to_ord(s.credibility) >= min_cred_ord)
            .collect())
    }

    // Confidence Assessment
    pub async fn assess_confidence(&self, statement: &str) -> Result<ConfidenceAssessment> {
        self.provider.assess_confidence(statement, None).await
    }

    pub async fn get_confidence_level(&self, statement: &str) -> Result<ConfidenceLevel> {
        let assessment = self.assess_confidence(statement).await?;
        let level = if assessment.overall_confidence >= 0.9 {
            ConfidenceLevel::VeryHigh
        } else if assessment.overall_confidence >= 0.75 {
            ConfidenceLevel::High
        } else if assessment.overall_confidence >= 0.5 {
            ConfidenceLevel::Medium
        } else if assessment.overall_confidence >= 0.25 {
            ConfidenceLevel::Low
        } else {
            ConfidenceLevel::VeryLow
        };
        Ok(level)
    }

    // Bias Detection
    pub async fn detect_bias(&self, text: &str) -> Result<BiasAnalysis> {
        self.provider.detect_bias(text).await
    }

    pub async fn is_biased(&self, text: &str) -> Result<bool> {
        let analysis = self.detect_bias(text).await?;
        Ok(analysis.overall_bias_score > 0.3)
    }

    pub async fn get_neutral_version(&self, text: &str) -> Result<Option<String>> {
        let analysis = self.detect_bias(text).await?;
        Ok(analysis.neutral_alternative)
    }

    // Consistency Checking
    pub async fn check_consistency(&self, statements: &[String]) -> Result<ConsistencyCheck> {
        self.provider.check_consistency(statements).await
    }

    pub async fn are_consistent(&self, statements: &[String]) -> Result<bool> {
        let check = self.check_consistency(statements).await?;
        Ok(check.is_consistent)
    }

    // Citations
    pub async fn create_citation(
        &self,
        request: CitationRequest,
        format: CitationFormat,
    ) -> Result<Citation> {
        let citation = self.provider.format_citation(request, format).await?;

        let mut citations = self.citations.write().await;
        citations.insert(citation.id.clone(), citation.clone());

        Ok(citation)
    }

    pub async fn get_citation(&self, citation_id: &str) -> Option<Citation> {
        let citations = self.citations.read().await;
        citations.get(citation_id).cloned()
    }

    // Statistics
    pub async fn get_verification_stats(&self) -> VerificationStats {
        let verifications = self.verifications.read().await;

        let mut stats = VerificationStats::default();

        for result in verifications.values() {
            stats.total += 1;
            match result.verdict {
                Verdict::Verified => stats.verified += 1,
                Verdict::PartiallyVerified => stats.partially_verified += 1,
                Verdict::False => stats.false_claims += 1,
                Verdict::Disputed => stats.disputed += 1,
                Verdict::Unverified => stats.unverified += 1,
                Verdict::Outdated => stats.outdated += 1,
                Verdict::Unverifiable => stats.unverifiable += 1,
            }
            stats.avg_confidence += result.confidence;
        }

        if stats.total > 0 {
            stats.avg_confidence /= stats.total as f32;
        }

        stats
    }
}

fn credibility_to_ord(cred: SourceCredibility) -> u8 {
    match cred {
        SourceCredibility::VeryHigh => 5,
        SourceCredibility::High => 4,
        SourceCredibility::Medium => 3,
        SourceCredibility::Low => 2,
        SourceCredibility::VeryLow => 1,
        SourceCredibility::Unknown => 0,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactCheckSummary {
    pub total_claims: usize,
    pub verified_count: usize,
    pub unverified_count: usize,
    pub false_count: usize,
    pub accuracy_score: f32,
    pub claims: Vec<(String, VerificationResult)>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum ConfidenceLevel {
    VeryHigh,
    High,
    Medium,
    Low,
    VeryLow,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VerificationStats {
    pub total: usize,
    pub verified: usize,
    pub partially_verified: usize,
    pub false_claims: usize,
    pub disputed: usize,
    pub unverified: usize,
    pub outdated: usize,
    pub unverifiable: usize,
    pub avg_confidence: f32,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    struct MockProvider;

    #[async_trait]
    impl VerifyProvider for MockProvider {
        async fn verify_claim(&self, claim: &Claim) -> Result<VerificationResult> {
            let is_true = claim.text.contains("Earth") || claim.text.contains("water");

            Ok(VerificationResult {
                claim_id: claim.id.clone(),
                verdict: if is_true {
                    Verdict::Verified
                } else {
                    Verdict::Unverified
                },
                confidence: if is_true { 0.95 } else { 0.3 },
                evidence: vec![Evidence {
                    source: Source {
                        id: "src-1".to_string(),
                        title: "Scientific Source".to_string(),
                        url: Some("https://example.com".to_string()),
                        author: Some("Dr. Test".to_string()),
                        published_date: Some("2023-01-01".to_string()),
                        source_type: SourceType::PeerReviewed,
                        credibility: SourceCredibility::High,
                    },
                    quote: Some("Supporting quote".to_string()),
                    relevance: 0.9,
                    supports_claim: is_true,
                    notes: None,
                }],
                counter_evidence: vec![],
                explanation: "Verification explanation".to_string(),
                corrections: vec![],
                verified_at: 0,
            })
        }

        async fn find_sources(
            &self,
            query: &str,
            _source_types: &[SourceType],
        ) -> Result<Vec<Source>> {
            Ok(vec![Source {
                id: "src-search".to_string(),
                title: format!("Source for: {}", query),
                url: Some("https://example.com".to_string()),
                author: None,
                published_date: None,
                source_type: SourceType::Website,
                credibility: SourceCredibility::Medium,
            }])
        }

        async fn assess_confidence(
            &self,
            _statement: &str,
            _context: Option<&str>,
        ) -> Result<ConfidenceAssessment> {
            Ok(ConfidenceAssessment {
                statement: "Test statement".to_string(),
                overall_confidence: 0.8,
                components: vec![ConfidenceComponent {
                    name: "Source quality".to_string(),
                    score: 0.85,
                    weight: 0.4,
                    explanation: "High quality sources".to_string(),
                }],
                uncertainty_factors: vec![],
                recommendations: vec!["Verify with additional sources".to_string()],
            })
        }

        async fn detect_bias(&self, text: &str) -> Result<BiasAnalysis> {
            let has_bias = text.contains("always") || text.contains("never");

            Ok(BiasAnalysis {
                text: text.to_string(),
                overall_bias_score: if has_bias { 0.6 } else { 0.1 },
                detected_biases: if has_bias {
                    vec![DetectedBias {
                        bias_type: BiasType::LoadedLanguage,
                        severity: BiasSeverity::Moderate,
                        location: "absolute terms".to_string(),
                        explanation: "Use of absolute language".to_string(),
                        suggestion: Some("Use more measured language".to_string()),
                    }]
                } else {
                    vec![]
                },
                neutral_alternative: if has_bias {
                    Some(text.replace("always", "often").replace("never", "rarely"))
                } else {
                    None
                },
                balance_assessment: BalanceAssessment {
                    perspectives_represented: vec!["main perspective".to_string()],
                    missing_perspectives: vec![],
                    balance_score: 0.7,
                    recommendations: vec![],
                },
            })
        }

        async fn check_consistency(&self, statements: &[String]) -> Result<ConsistencyCheck> {
            let has_contradiction = statements.iter().any(|s| s.contains("not"));

            Ok(ConsistencyCheck {
                statements: statements.to_vec(),
                is_consistent: !has_contradiction,
                contradictions: if has_contradiction {
                    vec![Contradiction {
                        statement_a: statements.first().cloned().unwrap_or_default(),
                        statement_b: statements.last().cloned().unwrap_or_default(),
                        explanation: "Potential contradiction".to_string(),
                        resolution: Some("Clarify the statements".to_string()),
                    }]
                } else {
                    vec![]
                },
                logical_issues: vec![],
            })
        }

        async fn format_citation(
            &self,
            request: CitationRequest,
            format: CitationFormat,
        ) -> Result<Citation> {
            let pub_date = request
                .published_date
                .clone()
                .unwrap_or_else(|| "n.d.".to_string());
            let formatted = match format {
                CitationFormat::APA => format!(
                    "{}. ({}). {}.",
                    request.authors.join(", "),
                    pub_date,
                    request.title,
                ),
                _ => format!("{} - {}", request.title, request.authors.join(", ")),
            };

            Ok(Citation {
                id: "cite-1".to_string(),
                source: Source {
                    id: "src-cite".to_string(),
                    title: request.title.clone(),
                    url: request.url,
                    author: Some(request.authors.join(", ")),
                    published_date: request.published_date,
                    source_type: request.source_type,
                    credibility: SourceCredibility::Unknown,
                },
                format,
                formatted,
                in_text: format!(
                    "({}, 2023)",
                    request
                        .authors
                        .first()
                        .map(|s| s.as_str())
                        .unwrap_or("Unknown")
                ),
            })
        }
    }

    #[tokio::test]
    async fn test_verify_claim() {
        let provider = Arc::new(MockProvider);
        let engine = VerifyEngine::new(provider);

        let result = engine
            .verify("The Earth is round", ClaimType::Factual)
            .await
            .unwrap();
        assert_eq!(result.verdict, Verdict::Verified);
        assert!(result.confidence > 0.9);
    }

    #[tokio::test]
    async fn test_is_true() {
        let provider = Arc::new(MockProvider);
        let engine = VerifyEngine::new(provider);

        assert!(engine.is_true("Earth orbits the sun").await.unwrap());
        assert!(!engine.is_true("Random false statement").await.unwrap());
    }

    #[tokio::test]
    async fn test_confidence_assessment() {
        let provider = Arc::new(MockProvider);
        let engine = VerifyEngine::new(provider);

        let level = engine.get_confidence_level("Test statement").await.unwrap();
        assert_eq!(level, ConfidenceLevel::High);
    }

    #[tokio::test]
    async fn test_bias_detection() {
        let provider = Arc::new(MockProvider);
        let engine = VerifyEngine::new(provider);

        let biased = engine.is_biased("They always do this").await.unwrap();
        assert!(biased);

        let not_biased = engine
            .is_biased("This is a neutral statement")
            .await
            .unwrap();
        assert!(!not_biased);
    }

    #[tokio::test]
    async fn test_neutral_version() {
        let provider = Arc::new(MockProvider);
        let engine = VerifyEngine::new(provider);

        let neutral = engine
            .get_neutral_version("They always fail")
            .await
            .unwrap();
        assert!(neutral.is_some());
        assert!(neutral.unwrap().contains("often"));
    }

    #[tokio::test]
    async fn test_consistency_check() {
        let provider = Arc::new(MockProvider);
        let engine = VerifyEngine::new(provider);

        let consistent = engine
            .are_consistent(&["The sky is blue".to_string(), "Grass is green".to_string()])
            .await
            .unwrap();
        assert!(consistent);

        let inconsistent = engine
            .are_consistent(&["It is true".to_string(), "It is not true".to_string()])
            .await
            .unwrap();
        assert!(!inconsistent);
    }

    #[tokio::test]
    async fn test_citation() {
        let provider = Arc::new(MockProvider);
        let engine = VerifyEngine::new(provider);

        let citation = engine
            .create_citation(
                CitationRequest {
                    title: "Test Article".to_string(),
                    authors: vec!["Smith, J.".to_string()],
                    published_date: Some("2023".to_string()),
                    url: None,
                    publisher: None,
                    volume: None,
                    issue: None,
                    pages: None,
                    doi: None,
                    source_type: SourceType::PeerReviewed,
                },
                CitationFormat::APA,
            )
            .await
            .unwrap();

        assert!(citation.formatted.contains("Smith"));
        assert!(citation.formatted.contains("2023"));
    }

    #[tokio::test]
    async fn test_verification_stats() {
        let provider = Arc::new(MockProvider);
        let engine = VerifyEngine::new(provider);

        engine
            .verify("Earth has water", ClaimType::Factual)
            .await
            .unwrap();
        engine
            .verify("Random claim", ClaimType::Factual)
            .await
            .unwrap();

        let stats = engine.get_verification_stats().await;
        assert_eq!(stats.total, 2);
        assert!(stats.verified > 0);
    }
}
