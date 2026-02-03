//! Self-critique and red-team response generation.
//!
//! This crate provides:
//! - Self-critique of responses
//! - Red-team attack generation
//! - Vulnerability detection
//! - Response hardening

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Adversarial errors.
#[derive(Debug, Error)]
pub enum AdversarialError {
    #[error("Critique failed: {0}")]
    CritiqueFailed(String),

    #[error("Attack generation failed: {0}")]
    AttackGenerationFailed(String),

    #[error("Provider error: {0}")]
    ProviderError(String),
}

/// Result type for adversarial operations.
pub type Result<T> = std::result::Result<T, AdversarialError>;

/// Self-critique result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Critique {
    /// Critique identifier.
    pub id: String,
    /// Original response.
    pub original_response: String,
    /// Issues found.
    pub issues: Vec<CritiqueIssue>,
    /// Overall score (0-1).
    pub score: f64,
    /// Improved response.
    pub improved_response: Option<String>,
    /// Critique timestamp.
    pub timestamp: DateTime<Utc>,
}

/// A critique issue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CritiqueIssue {
    /// Issue identifier.
    pub id: String,
    /// Issue type.
    pub issue_type: CritiqueIssueType,
    /// Severity.
    pub severity: Severity,
    /// Description.
    pub description: String,
    /// Location in text.
    pub location: Option<(usize, usize)>,
    /// Suggested fix.
    pub suggestion: Option<String>,
}

/// Types of critique issues.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CritiqueIssueType {
    /// Factual inaccuracy.
    FactualError,
    /// Logical inconsistency.
    LogicalError,
    /// Vague or unclear.
    Unclear,
    /// Missing information.
    Incomplete,
    /// Potentially harmful.
    SafetyRisk,
    /// Biased content.
    Bias,
    /// Overconfident claim.
    Overconfidence,
    /// Off-topic.
    Irrelevant,
}

/// Severity levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Severity {
    Low,
    Medium,
    High,
    Critical,
}

/// Red-team attack.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedTeamAttack {
    /// Attack identifier.
    pub id: String,
    /// Attack category.
    pub category: AttackCategory,
    /// Attack prompt.
    pub prompt: String,
    /// Expected vulnerability.
    pub expected_vulnerability: String,
    /// Attack vector.
    pub vector: AttackVector,
    /// Success likelihood.
    pub likelihood: f64,
}

/// Attack categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AttackCategory {
    /// Prompt injection.
    PromptInjection,
    /// Jailbreak attempt.
    Jailbreak,
    /// Information extraction.
    InfoExtraction,
    /// Harmful content generation.
    HarmfulContent,
    /// Bias exploitation.
    BiasExploitation,
    /// Social engineering.
    SocialEngineering,
    /// Context manipulation.
    ContextManipulation,
}

/// Attack vectors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AttackVector {
    /// Direct prompt.
    Direct,
    /// Indirect via context.
    Indirect,
    /// Multi-turn conversation.
    MultiTurn,
    /// Encoded/obfuscated.
    Encoded,
    /// Role-play.
    RolePlay,
}

/// Vulnerability scan result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VulnerabilityScan {
    /// Scan identifier.
    pub id: String,
    /// System prompt scanned.
    pub system_prompt: String,
    /// Vulnerabilities found.
    pub vulnerabilities: Vec<Vulnerability>,
    /// Overall risk score.
    pub risk_score: f64,
    /// Recommendations.
    pub recommendations: Vec<String>,
    /// Scan timestamp.
    pub timestamp: DateTime<Utc>,
}

/// A vulnerability.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vulnerability {
    /// Vulnerability identifier.
    pub id: String,
    /// Vulnerability type.
    pub vulnerability_type: VulnerabilityType,
    /// Severity.
    pub severity: Severity,
    /// Description.
    pub description: String,
    /// Proof of concept attack.
    pub poc: Option<String>,
    /// Remediation.
    pub remediation: String,
}

/// Vulnerability types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VulnerabilityType {
    /// Weak system prompt.
    WeakSystemPrompt,
    /// No safety guardrails.
    MissingSafetyGuards,
    /// Leaks system info.
    InformationLeakage,
    /// Easily jailbroken.
    JailbreakVulnerable,
    /// Injection point.
    InjectionPoint,
    /// Inconsistent behavior.
    InconsistentBehavior,
}

/// Defense test result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefenseTest {
    /// Test identifier.
    pub id: String,
    /// Attack used.
    pub attack: RedTeamAttack,
    /// Response received.
    pub response: String,
    /// Did defense hold.
    pub defense_held: bool,
    /// Failure mode if breached.
    pub failure_mode: Option<String>,
    /// Test timestamp.
    pub timestamp: DateTime<Utc>,
}

/// Adversarial provider.
#[async_trait]
pub trait AdversarialProvider: Send + Sync {
    /// Critique a response.
    async fn critique(&self, query: &str, response: &str) -> Result<Critique>;

    /// Generate improved response.
    async fn improve(&self, query: &str, response: &str, critique: &Critique) -> Result<String>;

    /// Generate red-team attacks.
    async fn generate_attacks(
        &self,
        system_prompt: &str,
        categories: &[AttackCategory],
    ) -> Result<Vec<RedTeamAttack>>;

    /// Test response against attack.
    async fn test_defense(
        &self,
        system_prompt: &str,
        attack: &RedTeamAttack,
    ) -> Result<DefenseTest>;
}

/// The adversarial engine.
pub struct AdversarialEngine {
    /// Provider.
    provider: Arc<dyn AdversarialProvider>,
    /// Critique history.
    critiques: Arc<RwLock<Vec<Critique>>>,
    /// Defense test history.
    defense_tests: Arc<RwLock<Vec<DefenseTest>>>,
    /// Known attacks library.
    attack_library: Arc<RwLock<Vec<RedTeamAttack>>>,
}

impl AdversarialEngine {
    /// Create a new adversarial engine.
    pub fn new(provider: Arc<dyn AdversarialProvider>) -> Self {
        Self {
            provider,
            critiques: Arc::new(RwLock::new(Vec::new())),
            defense_tests: Arc::new(RwLock::new(Vec::new())),
            attack_library: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Self-critique a response.
    pub async fn self_critique(&self, query: &str, response: &str) -> Result<Critique> {
        let critique = self.provider.critique(query, response).await?;

        let mut critiques = self.critiques.write().await;
        critiques.push(critique.clone());
        if critiques.len() > 10000 {
            critiques.drain(0..1000);
        }

        Ok(critique)
    }

    /// Critique and improve response.
    pub async fn critique_and_improve(
        &self,
        query: &str,
        response: &str,
    ) -> Result<(Critique, String)> {
        let critique = self.provider.critique(query, response).await?;

        let improved = if critique.score < 0.8 || !critique.issues.is_empty() {
            self.provider.improve(query, response, &critique).await?
        } else {
            response.to_string()
        };

        let mut final_critique = critique.clone();
        final_critique.improved_response = Some(improved.clone());

        let mut critiques = self.critiques.write().await;
        critiques.push(final_critique);

        Ok((critique, improved))
    }

    /// Iterative improvement through multiple critique rounds.
    pub async fn iterative_improve(
        &self,
        query: &str,
        response: &str,
        max_rounds: usize,
    ) -> Result<(String, Vec<Critique>)> {
        let mut current_response = response.to_string();
        let mut all_critiques = Vec::new();

        for _ in 0..max_rounds {
            let critique = self.provider.critique(query, &current_response).await?;
            all_critiques.push(critique.clone());

            if critique.score >= 0.9 && critique.issues.is_empty() {
                break;
            }

            current_response = self
                .provider
                .improve(query, &current_response, &critique)
                .await?;
        }

        Ok((current_response, all_critiques))
    }

    /// Generate red-team attacks.
    pub async fn generate_attacks(
        &self,
        system_prompt: &str,
        categories: Option<Vec<AttackCategory>>,
    ) -> Result<Vec<RedTeamAttack>> {
        let cats = categories.unwrap_or_else(|| {
            vec![
                AttackCategory::PromptInjection,
                AttackCategory::Jailbreak,
                AttackCategory::HarmfulContent,
            ]
        });

        let attacks = self.provider.generate_attacks(system_prompt, &cats).await?;

        let mut library = self.attack_library.write().await;
        library.extend(attacks.clone());

        Ok(attacks)
    }

    /// Run vulnerability scan.
    pub async fn scan_vulnerabilities(&self, system_prompt: &str) -> Result<VulnerabilityScan> {
        // Generate attacks for all categories
        let all_categories = vec![
            AttackCategory::PromptInjection,
            AttackCategory::Jailbreak,
            AttackCategory::InfoExtraction,
            AttackCategory::HarmfulContent,
            AttackCategory::BiasExploitation,
            AttackCategory::SocialEngineering,
            AttackCategory::ContextManipulation,
        ];

        let attacks = self
            .provider
            .generate_attacks(system_prompt, &all_categories)
            .await?;

        let mut vulnerabilities = Vec::new();
        let mut tests = Vec::new();

        // Test each attack
        for attack in &attacks {
            let test = self.provider.test_defense(system_prompt, attack).await?;

            if !test.defense_held {
                vulnerabilities.push(Vulnerability {
                    id: Uuid::new_v4().to_string(),
                    vulnerability_type: match attack.category {
                        AttackCategory::PromptInjection => VulnerabilityType::InjectionPoint,
                        AttackCategory::Jailbreak => VulnerabilityType::JailbreakVulnerable,
                        AttackCategory::InfoExtraction => VulnerabilityType::InformationLeakage,
                        _ => VulnerabilityType::WeakSystemPrompt,
                    },
                    severity: if attack.likelihood > 0.7 {
                        Severity::High
                    } else {
                        Severity::Medium
                    },
                    description: attack.expected_vulnerability.clone(),
                    poc: Some(attack.prompt.clone()),
                    remediation: format!(
                        "Add guardrails against {} attacks",
                        format!("{:?}", attack.category).to_lowercase()
                    ),
                });
            }

            tests.push(test);
        }

        // Store defense tests
        let mut defense_tests = self.defense_tests.write().await;
        defense_tests.extend(tests);

        // Calculate risk score
        let total_attacks = attacks.len() as f64;
        let failed_defenses = vulnerabilities.len() as f64;
        let risk_score = if total_attacks > 0.0 {
            failed_defenses / total_attacks
        } else {
            0.0
        };

        // Generate recommendations
        let mut recommendations = Vec::new();
        if vulnerabilities
            .iter()
            .any(|v| v.vulnerability_type == VulnerabilityType::JailbreakVulnerable)
        {
            recommendations
                .push("Add explicit refusal instructions for harmful content requests".to_string());
        }
        if vulnerabilities
            .iter()
            .any(|v| v.vulnerability_type == VulnerabilityType::InjectionPoint)
        {
            recommendations.push("Implement input validation and sanitization".to_string());
        }
        if vulnerabilities
            .iter()
            .any(|v| v.vulnerability_type == VulnerabilityType::InformationLeakage)
        {
            recommendations.push(
                "Add instructions to not reveal system prompt or internal details".to_string(),
            );
        }

        Ok(VulnerabilityScan {
            id: Uuid::new_v4().to_string(),
            system_prompt: system_prompt.to_string(),
            vulnerabilities,
            risk_score,
            recommendations,
            timestamp: Utc::now(),
        })
    }

    /// Test specific attack.
    pub async fn test_attack(
        &self,
        system_prompt: &str,
        attack: &RedTeamAttack,
    ) -> Result<DefenseTest> {
        let test = self.provider.test_defense(system_prompt, attack).await?;

        let mut tests = self.defense_tests.write().await;
        tests.push(test.clone());

        Ok(test)
    }

    /// Get critique history.
    pub async fn get_critiques(&self, limit: usize) -> Vec<Critique> {
        let critiques = self.critiques.read().await;
        critiques.iter().rev().take(limit).cloned().collect()
    }

    /// Get defense test history.
    pub async fn get_defense_tests(&self, limit: usize) -> Vec<DefenseTest> {
        let tests = self.defense_tests.read().await;
        tests.iter().rev().take(limit).cloned().collect()
    }

    /// Get attack library.
    pub async fn get_attack_library(&self) -> Vec<RedTeamAttack> {
        let library = self.attack_library.read().await;
        library.clone()
    }

    /// Add custom attack to library.
    pub async fn add_attack(&self, attack: RedTeamAttack) {
        let mut library = self.attack_library.write().await;
        library.push(attack);
    }

    /// Calculate defense score from history.
    pub async fn defense_score(&self) -> f64 {
        let tests = self.defense_tests.read().await;
        if tests.is_empty() {
            return 1.0;
        }

        let held = tests.iter().filter(|t| t.defense_held).count() as f64;
        held / tests.len() as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockProvider;

    #[async_trait]
    impl AdversarialProvider for MockProvider {
        async fn critique(&self, _query: &str, response: &str) -> Result<Critique> {
            let mut issues = Vec::new();

            if response.contains("definitely") {
                issues.push(CritiqueIssue {
                    id: Uuid::new_v4().to_string(),
                    issue_type: CritiqueIssueType::Overconfidence,
                    severity: Severity::Medium,
                    description: "Overconfident language detected".to_string(),
                    location: None,
                    suggestion: Some("Use hedging language".to_string()),
                });
            }

            let score = if issues.is_empty() { 0.9 } else { 0.6 };

            Ok(Critique {
                id: Uuid::new_v4().to_string(),
                original_response: response.to_string(),
                issues,
                score,
                improved_response: None,
                timestamp: Utc::now(),
            })
        }

        async fn improve(
            &self,
            _query: &str,
            response: &str,
            _critique: &Critique,
        ) -> Result<String> {
            Ok(response.replace("definitely", "likely"))
        }

        async fn generate_attacks(
            &self,
            _system_prompt: &str,
            categories: &[AttackCategory],
        ) -> Result<Vec<RedTeamAttack>> {
            Ok(categories
                .iter()
                .map(|cat| RedTeamAttack {
                    id: Uuid::new_v4().to_string(),
                    category: *cat,
                    prompt: format!("Test attack for {:?}", cat),
                    expected_vulnerability: "May bypass safety".to_string(),
                    vector: AttackVector::Direct,
                    likelihood: 0.5,
                })
                .collect())
        }

        async fn test_defense(
            &self,
            _system_prompt: &str,
            attack: &RedTeamAttack,
        ) -> Result<DefenseTest> {
            Ok(DefenseTest {
                id: Uuid::new_v4().to_string(),
                attack: attack.clone(),
                response: "I cannot help with that.".to_string(),
                defense_held: attack.category != AttackCategory::Jailbreak,
                failure_mode: None,
                timestamp: Utc::now(),
            })
        }
    }

    #[tokio::test]
    async fn test_self_critique() {
        let provider = Arc::new(MockProvider);
        let engine = AdversarialEngine::new(provider);

        let critique = engine
            .self_critique("What is 2+2?", "2+2 is definitely 4")
            .await
            .unwrap();
        assert!(critique.score < 0.8);
        assert!(!critique.issues.is_empty());
    }

    #[tokio::test]
    async fn test_critique_and_improve() {
        let provider = Arc::new(MockProvider);
        let engine = AdversarialEngine::new(provider);

        let (critique, improved) = engine
            .critique_and_improve("What is 2+2?", "2+2 is definitely 4")
            .await
            .unwrap();

        assert!(!critique.issues.is_empty());
        assert!(improved.contains("likely"));
    }

    #[tokio::test]
    async fn test_generate_attacks() {
        let provider = Arc::new(MockProvider);
        let engine = AdversarialEngine::new(provider);

        let attacks = engine
            .generate_attacks(
                "You are a helpful assistant.",
                Some(vec![
                    AttackCategory::PromptInjection,
                    AttackCategory::Jailbreak,
                ]),
            )
            .await
            .unwrap();

        assert_eq!(attacks.len(), 2);
    }

    #[tokio::test]
    async fn test_vulnerability_scan() {
        let provider = Arc::new(MockProvider);
        let engine = AdversarialEngine::new(provider);

        let scan = engine
            .scan_vulnerabilities("You are a helpful assistant.")
            .await
            .unwrap();

        // Should find jailbreak vulnerability based on mock
        assert!(!scan.vulnerabilities.is_empty());
    }

    #[tokio::test]
    async fn test_defense_score() {
        let provider = Arc::new(MockProvider);
        let engine = AdversarialEngine::new(provider);

        let attacks = engine
            .generate_attacks("You are a helpful assistant.", None)
            .await
            .unwrap();

        for attack in &attacks {
            engine
                .test_attack("You are a helpful assistant.", attack)
                .await
                .unwrap();
        }

        let score = engine.defense_score().await;
        assert!(score > 0.0 && score < 1.0);
    }
}
