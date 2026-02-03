//! Domain expertise system for drbot
//!
//! Specialized knowledge in legal, medical, financial, and technical domains.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;

#[derive(Error, Debug)]
pub enum ExpertError {
    #[error("Domain not supported: {0}")]
    UnsupportedDomain(String),
    #[error("Analysis failed: {0}")]
    AnalysisFailed(String),
    #[error("Insufficient context: {0}")]
    InsufficientContext(String),
    #[error("Provider error: {0}")]
    ProviderError(String),
}

pub type Result<T> = std::result::Result<T, ExpertError>;

// ============================================================================
// Core Domain Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Domain {
    Legal,
    Medical,
    Financial,
    Technical,
    Scientific,
    Academic,
    Business,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpertQuery {
    pub domain: Domain,
    pub question: String,
    pub context: Option<String>,
    pub jurisdiction: Option<String>,
    pub specificity: Specificity,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum Specificity {
    General,
    Detailed,
    Expert,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpertResponse {
    pub domain: Domain,
    pub answer: String,
    pub confidence: f32,
    pub caveats: Vec<String>,
    pub references: Vec<Reference>,
    pub follow_up_questions: Vec<String>,
    pub disclaimer: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reference {
    pub title: String,
    pub source: String,
    pub url: Option<String>,
    pub relevance: f32,
}

// ============================================================================
// Legal Domain
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegalQuery {
    pub question: String,
    pub jurisdiction: String,
    pub area_of_law: AreaOfLaw,
    pub context: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AreaOfLaw {
    Contract,
    Employment,
    Intellectual,
    Corporate,
    Tax,
    RealEstate,
    Criminal,
    Family,
    Immigration,
    Privacy,
    Consumer,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegalAnalysis {
    pub summary: String,
    pub applicable_laws: Vec<ApplicableLaw>,
    pub key_considerations: Vec<String>,
    pub potential_risks: Vec<LegalRisk>,
    pub recommended_actions: Vec<String>,
    pub disclaimer: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplicableLaw {
    pub name: String,
    pub citation: Option<String>,
    pub jurisdiction: String,
    pub relevance: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegalRisk {
    pub description: String,
    pub severity: RiskSeverity,
    pub mitigation: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum RiskSeverity {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractReview {
    pub document_type: String,
    pub parties: Vec<String>,
    pub key_terms: Vec<ContractTerm>,
    pub unusual_clauses: Vec<UnusualClause>,
    pub missing_provisions: Vec<String>,
    pub recommendations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractTerm {
    pub name: String,
    pub description: String,
    pub location: Option<String>,
    pub standard: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnusualClause {
    pub clause: String,
    pub concern: String,
    pub severity: RiskSeverity,
}

// ============================================================================
// Medical Domain
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MedicalQuery {
    pub question: String,
    pub symptoms: Vec<String>,
    pub medical_history: Option<String>,
    pub medications: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MedicalInformation {
    pub topic: String,
    pub explanation: String,
    pub key_points: Vec<String>,
    pub when_to_seek_help: Vec<String>,
    pub related_conditions: Vec<String>,
    pub disclaimer: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymptomAnalysis {
    pub symptoms: Vec<String>,
    pub possible_conditions: Vec<PossibleCondition>,
    pub red_flags: Vec<String>,
    pub recommended_actions: Vec<String>,
    pub urgency: MedicalUrgency,
    pub disclaimer: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PossibleCondition {
    pub name: String,
    pub probability: ConditionProbability,
    pub description: String,
    pub typical_symptoms: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum ConditionProbability {
    Possible,
    Likely,
    VeryLikely,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum MedicalUrgency {
    NonUrgent,
    SoonAppointment,
    SameDay,
    Urgent,
    Emergency,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MedicationInfo {
    pub name: String,
    pub generic_name: Option<String>,
    pub drug_class: String,
    pub uses: Vec<String>,
    pub side_effects: Vec<SideEffect>,
    pub interactions: Vec<DrugInteraction>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SideEffect {
    pub effect: String,
    pub frequency: SideEffectFrequency,
    pub severity: RiskSeverity,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum SideEffectFrequency {
    Common,
    Uncommon,
    Rare,
    VeryRare,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DrugInteraction {
    pub drug: String,
    pub interaction_type: String,
    pub severity: RiskSeverity,
    pub description: String,
}

// ============================================================================
// Financial Domain
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinancialQuery {
    pub question: String,
    pub context: FinancialContext,
    pub jurisdiction: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FinancialContext {
    PersonalFinance,
    Investment,
    Tax,
    Retirement,
    RealEstate,
    Business,
    Insurance,
    Debt,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinancialAnalysis {
    pub summary: String,
    pub considerations: Vec<String>,
    pub options: Vec<FinancialOption>,
    pub risks: Vec<FinancialRisk>,
    pub tax_implications: Option<String>,
    pub disclaimer: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinancialOption {
    pub name: String,
    pub description: String,
    pub pros: Vec<String>,
    pub cons: Vec<String>,
    pub suitability: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinancialRisk {
    pub description: String,
    pub risk_type: FinancialRiskType,
    pub severity: RiskSeverity,
    pub mitigation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FinancialRiskType {
    Market,
    Credit,
    Liquidity,
    Inflation,
    Interest,
    Regulatory,
    Operational,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvestmentAnalysis {
    pub asset: String,
    pub asset_type: String,
    pub risk_profile: RiskProfile,
    pub historical_performance: Option<String>,
    pub key_metrics: HashMap<String, String>,
    pub considerations: Vec<String>,
    pub disclaimer: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum RiskProfile {
    Conservative,
    Moderate,
    Aggressive,
    Speculative,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxAnalysis {
    pub scenario: String,
    pub tax_implications: Vec<TaxImplication>,
    pub deductions: Vec<PotentialDeduction>,
    pub strategies: Vec<String>,
    pub disclaimer: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxImplication {
    pub description: String,
    pub tax_type: String,
    pub estimated_impact: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PotentialDeduction {
    pub name: String,
    pub description: String,
    pub requirements: Vec<String>,
    pub documentation_needed: Vec<String>,
}

// ============================================================================
// Technical Domain
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TechnicalQuery {
    pub question: String,
    pub technology_area: TechnologyArea,
    pub context: Option<String>,
    pub experience_level: ExperienceLevel,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TechnologyArea {
    Programming,
    SystemDesign,
    Database,
    Networking,
    Security,
    Cloud,
    DevOps,
    MachineLearning,
    Mobile,
    Web,
    Custom(String),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum ExperienceLevel {
    Beginner,
    Intermediate,
    Advanced,
    Expert,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TechnicalExplanation {
    pub topic: String,
    pub explanation: String,
    pub key_concepts: Vec<KeyConcept>,
    pub examples: Vec<CodeExample>,
    pub best_practices: Vec<String>,
    pub common_pitfalls: Vec<String>,
    pub resources: Vec<Reference>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyConcept {
    pub name: String,
    pub description: String,
    pub importance: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeExample {
    pub description: String,
    pub language: String,
    pub code: String,
    pub explanation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchitectureReview {
    pub system_name: String,
    pub components: Vec<ArchitectureComponent>,
    pub strengths: Vec<String>,
    pub weaknesses: Vec<String>,
    pub recommendations: Vec<ArchitectureRecommendation>,
    pub scalability_assessment: String,
    pub security_assessment: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchitectureComponent {
    pub name: String,
    pub description: String,
    pub technology: String,
    pub responsibilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchitectureRecommendation {
    pub area: String,
    pub current_state: String,
    pub recommendation: String,
    pub priority: RiskSeverity,
    pub effort: EffortEstimate,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum EffortEstimate {
    Trivial,
    Small,
    Medium,
    Large,
    Major,
}

// ============================================================================
// Provider Trait
// ============================================================================

#[async_trait]
pub trait ExpertProvider: Send + Sync {
    // General
    async fn query(&self, query: ExpertQuery) -> Result<ExpertResponse>;

    // Legal
    async fn analyze_legal(&self, query: LegalQuery) -> Result<LegalAnalysis>;
    async fn review_contract(
        &self,
        contract_text: &str,
        contract_type: &str,
    ) -> Result<ContractReview>;

    // Medical
    async fn get_medical_info(&self, topic: &str) -> Result<MedicalInformation>;
    async fn analyze_symptoms(&self, query: MedicalQuery) -> Result<SymptomAnalysis>;
    async fn get_medication_info(&self, medication: &str) -> Result<MedicationInfo>;

    // Financial
    async fn analyze_financial(&self, query: FinancialQuery) -> Result<FinancialAnalysis>;
    async fn analyze_investment(&self, asset: &str, asset_type: &str)
        -> Result<InvestmentAnalysis>;
    async fn analyze_tax(&self, scenario: &str, jurisdiction: &str) -> Result<TaxAnalysis>;

    // Technical
    async fn explain_technical(&self, query: TechnicalQuery) -> Result<TechnicalExplanation>;
    async fn review_architecture(&self, description: &str) -> Result<ArchitectureReview>;
}

// ============================================================================
// Expert Engine
// ============================================================================

pub struct ExpertEngine {
    provider: Arc<dyn ExpertProvider>,
    domain_context: Arc<RwLock<HashMap<Domain, Vec<String>>>>,
    query_history: Arc<RwLock<Vec<(ExpertQuery, ExpertResponse)>>>,
}

impl ExpertEngine {
    pub fn new(provider: Arc<dyn ExpertProvider>) -> Self {
        Self {
            provider,
            domain_context: Arc::new(RwLock::new(HashMap::new())),
            query_history: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub async fn query(&self, query: ExpertQuery) -> Result<ExpertResponse> {
        let response = self.provider.query(query.clone()).await?;

        // Store in history
        let mut history = self.query_history.write().await;
        history.push((query, response.clone()));

        Ok(response)
    }

    pub async fn add_domain_context(&self, domain: Domain, context: String) {
        let mut contexts = self.domain_context.write().await;
        contexts.entry(domain).or_default().push(context);
    }

    pub async fn get_domain_context(&self, domain: &Domain) -> Vec<String> {
        let contexts = self.domain_context.read().await;
        contexts.get(domain).cloned().unwrap_or_default()
    }

    // Legal helpers
    pub async fn legal_question(
        &self,
        question: &str,
        jurisdiction: &str,
        area: AreaOfLaw,
    ) -> Result<LegalAnalysis> {
        let query = LegalQuery {
            question: question.to_string(),
            jurisdiction: jurisdiction.to_string(),
            area_of_law: area,
            context: None,
        };
        self.provider.analyze_legal(query).await
    }

    pub async fn review_contract(&self, text: &str, contract_type: &str) -> Result<ContractReview> {
        self.provider.review_contract(text, contract_type).await
    }

    // Medical helpers
    pub async fn medical_info(&self, topic: &str) -> Result<MedicalInformation> {
        self.provider.get_medical_info(topic).await
    }

    pub async fn symptom_check(
        &self,
        symptoms: Vec<String>,
        medications: Vec<String>,
    ) -> Result<SymptomAnalysis> {
        let query = MedicalQuery {
            question: "What could these symptoms indicate?".to_string(),
            symptoms,
            medical_history: None,
            medications,
        };
        self.provider.analyze_symptoms(query).await
    }

    pub async fn medication_info(&self, medication: &str) -> Result<MedicationInfo> {
        self.provider.get_medication_info(medication).await
    }

    // Financial helpers
    pub async fn financial_question(
        &self,
        question: &str,
        context: FinancialContext,
    ) -> Result<FinancialAnalysis> {
        let query = FinancialQuery {
            question: question.to_string(),
            context,
            jurisdiction: None,
        };
        self.provider.analyze_financial(query).await
    }

    pub async fn investment_info(
        &self,
        asset: &str,
        asset_type: &str,
    ) -> Result<InvestmentAnalysis> {
        self.provider.analyze_investment(asset, asset_type).await
    }

    pub async fn tax_question(&self, scenario: &str, jurisdiction: &str) -> Result<TaxAnalysis> {
        self.provider.analyze_tax(scenario, jurisdiction).await
    }

    // Technical helpers
    pub async fn technical_question(
        &self,
        question: &str,
        area: TechnologyArea,
        level: ExperienceLevel,
    ) -> Result<TechnicalExplanation> {
        let query = TechnicalQuery {
            question: question.to_string(),
            technology_area: area,
            context: None,
            experience_level: level,
        };
        self.provider.explain_technical(query).await
    }

    pub async fn architecture_review(&self, description: &str) -> Result<ArchitectureReview> {
        self.provider.review_architecture(description).await
    }

    // Cross-domain
    pub async fn get_related_queries(
        &self,
        domain: &Domain,
        limit: usize,
    ) -> Vec<(ExpertQuery, ExpertResponse)> {
        let history = self.query_history.read().await;
        history
            .iter()
            .filter(|(q, _)| &q.domain == domain)
            .take(limit)
            .cloned()
            .collect()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    struct MockProvider;

    #[async_trait]
    impl ExpertProvider for MockProvider {
        async fn query(&self, query: ExpertQuery) -> Result<ExpertResponse> {
            Ok(ExpertResponse {
                domain: query.domain,
                answer: "This is a general expert answer.".to_string(),
                confidence: 0.85,
                caveats: vec!["Consult a professional for specific advice.".to_string()],
                references: vec![],
                follow_up_questions: vec!["Would you like more detail?".to_string()],
                disclaimer: Some("This is general information only.".to_string()),
            })
        }

        async fn analyze_legal(&self, _query: LegalQuery) -> Result<LegalAnalysis> {
            Ok(LegalAnalysis {
                summary: "Legal analysis summary".to_string(),
                applicable_laws: vec![ApplicableLaw {
                    name: "Sample Law".to_string(),
                    citation: Some("123 USC 456".to_string()),
                    jurisdiction: "US".to_string(),
                    relevance: "Directly applicable".to_string(),
                }],
                key_considerations: vec!["Consider timing".to_string()],
                potential_risks: vec![LegalRisk {
                    description: "Compliance risk".to_string(),
                    severity: RiskSeverity::Medium,
                    mitigation: Some("Regular audits".to_string()),
                }],
                recommended_actions: vec!["Consult an attorney".to_string()],
                disclaimer: "This is not legal advice.".to_string(),
            })
        }

        async fn review_contract(
            &self,
            _contract_text: &str,
            contract_type: &str,
        ) -> Result<ContractReview> {
            Ok(ContractReview {
                document_type: contract_type.to_string(),
                parties: vec!["Party A".to_string(), "Party B".to_string()],
                key_terms: vec![ContractTerm {
                    name: "Payment Terms".to_string(),
                    description: "Net 30".to_string(),
                    location: Some("Section 3".to_string()),
                    standard: true,
                }],
                unusual_clauses: vec![],
                missing_provisions: vec!["Dispute resolution".to_string()],
                recommendations: vec!["Add arbitration clause".to_string()],
            })
        }

        async fn get_medical_info(&self, topic: &str) -> Result<MedicalInformation> {
            Ok(MedicalInformation {
                topic: topic.to_string(),
                explanation: "Medical explanation here.".to_string(),
                key_points: vec!["Key point 1".to_string()],
                when_to_seek_help: vec!["If symptoms worsen".to_string()],
                related_conditions: vec![],
                disclaimer: "Consult a healthcare provider.".to_string(),
            })
        }

        async fn analyze_symptoms(&self, query: MedicalQuery) -> Result<SymptomAnalysis> {
            Ok(SymptomAnalysis {
                symptoms: query.symptoms,
                possible_conditions: vec![PossibleCondition {
                    name: "Common Cold".to_string(),
                    probability: ConditionProbability::Likely,
                    description: "Viral infection".to_string(),
                    typical_symptoms: vec!["runny nose".to_string()],
                }],
                red_flags: vec![],
                recommended_actions: vec!["Rest and hydration".to_string()],
                urgency: MedicalUrgency::NonUrgent,
                disclaimer: "This is not a diagnosis.".to_string(),
            })
        }

        async fn get_medication_info(&self, medication: &str) -> Result<MedicationInfo> {
            Ok(MedicationInfo {
                name: medication.to_string(),
                generic_name: Some("Generic Name".to_string()),
                drug_class: "NSAID".to_string(),
                uses: vec!["Pain relief".to_string()],
                side_effects: vec![SideEffect {
                    effect: "Stomach upset".to_string(),
                    frequency: SideEffectFrequency::Common,
                    severity: RiskSeverity::Low,
                }],
                interactions: vec![],
                warnings: vec!["Take with food".to_string()],
            })
        }

        async fn analyze_financial(&self, query: FinancialQuery) -> Result<FinancialAnalysis> {
            Ok(FinancialAnalysis {
                summary: format!("Analysis for: {}", query.question),
                considerations: vec!["Consider your risk tolerance".to_string()],
                options: vec![FinancialOption {
                    name: "Option A".to_string(),
                    description: "Conservative approach".to_string(),
                    pros: vec!["Lower risk".to_string()],
                    cons: vec!["Lower returns".to_string()],
                    suitability: "Risk-averse investors".to_string(),
                }],
                risks: vec![],
                tax_implications: Some("May have tax consequences".to_string()),
                disclaimer: "Not financial advice.".to_string(),
            })
        }

        async fn analyze_investment(
            &self,
            asset: &str,
            asset_type: &str,
        ) -> Result<InvestmentAnalysis> {
            Ok(InvestmentAnalysis {
                asset: asset.to_string(),
                asset_type: asset_type.to_string(),
                risk_profile: RiskProfile::Moderate,
                historical_performance: Some("10% avg annual return".to_string()),
                key_metrics: HashMap::from([("P/E".to_string(), "25".to_string())]),
                considerations: vec!["Market conditions".to_string()],
                disclaimer: "Past performance...".to_string(),
            })
        }

        async fn analyze_tax(&self, scenario: &str, jurisdiction: &str) -> Result<TaxAnalysis> {
            Ok(TaxAnalysis {
                scenario: scenario.to_string(),
                tax_implications: vec![TaxImplication {
                    description: "Income tax applies".to_string(),
                    tax_type: "Income".to_string(),
                    estimated_impact: Some("15-25%".to_string()),
                }],
                deductions: vec![],
                strategies: vec![format!("Consider {} tax laws", jurisdiction)],
                disclaimer: "Consult a tax professional.".to_string(),
            })
        }

        async fn explain_technical(&self, query: TechnicalQuery) -> Result<TechnicalExplanation> {
            Ok(TechnicalExplanation {
                topic: query.question.clone(),
                explanation: "Technical explanation here.".to_string(),
                key_concepts: vec![KeyConcept {
                    name: "Concept 1".to_string(),
                    description: "Description".to_string(),
                    importance: "Fundamental".to_string(),
                }],
                examples: vec![CodeExample {
                    description: "Basic example".to_string(),
                    language: "rust".to_string(),
                    code: "fn main() {}".to_string(),
                    explanation: Some("Entry point".to_string()),
                }],
                best_practices: vec!["Follow conventions".to_string()],
                common_pitfalls: vec!["Avoid X".to_string()],
                resources: vec![],
            })
        }

        async fn review_architecture(&self, _description: &str) -> Result<ArchitectureReview> {
            Ok(ArchitectureReview {
                system_name: "System".to_string(),
                components: vec![ArchitectureComponent {
                    name: "API".to_string(),
                    description: "REST API".to_string(),
                    technology: "Rust + Axum".to_string(),
                    responsibilities: vec!["Handle requests".to_string()],
                }],
                strengths: vec!["Good separation".to_string()],
                weaknesses: vec!["Single point of failure".to_string()],
                recommendations: vec![ArchitectureRecommendation {
                    area: "Reliability".to_string(),
                    current_state: "Single instance".to_string(),
                    recommendation: "Add redundancy".to_string(),
                    priority: RiskSeverity::Medium,
                    effort: EffortEstimate::Medium,
                }],
                scalability_assessment: "Good horizontal scaling potential".to_string(),
                security_assessment: "Basic security in place".to_string(),
            })
        }
    }

    #[tokio::test]
    async fn test_general_query() {
        let provider = Arc::new(MockProvider);
        let engine = ExpertEngine::new(provider);

        let query = ExpertQuery {
            domain: Domain::Technical,
            question: "How does async/await work?".to_string(),
            context: None,
            jurisdiction: None,
            specificity: Specificity::Detailed,
        };

        let response = engine.query(query).await.unwrap();
        assert_eq!(response.domain, Domain::Technical);
        assert!(response.confidence > 0.0);
    }

    #[tokio::test]
    async fn test_legal_analysis() {
        let provider = Arc::new(MockProvider);
        let engine = ExpertEngine::new(provider);

        let analysis = engine
            .legal_question("What are my rights?", "US", AreaOfLaw::Employment)
            .await
            .unwrap();

        assert!(!analysis.summary.is_empty());
        assert!(!analysis.applicable_laws.is_empty());
    }

    #[tokio::test]
    async fn test_contract_review() {
        let provider = Arc::new(MockProvider);
        let engine = ExpertEngine::new(provider);

        let review = engine
            .review_contract("Sample contract text...", "Employment Agreement")
            .await
            .unwrap();

        assert_eq!(review.document_type, "Employment Agreement");
        assert!(!review.parties.is_empty());
    }

    #[tokio::test]
    async fn test_symptom_analysis() {
        let provider = Arc::new(MockProvider);
        let engine = ExpertEngine::new(provider);

        let analysis = engine
            .symptom_check(vec!["headache".to_string(), "fatigue".to_string()], vec![])
            .await
            .unwrap();

        assert!(!analysis.possible_conditions.is_empty());
        assert_eq!(analysis.urgency, MedicalUrgency::NonUrgent);
    }

    #[tokio::test]
    async fn test_financial_analysis() {
        let provider = Arc::new(MockProvider);
        let engine = ExpertEngine::new(provider);

        let analysis = engine
            .financial_question(
                "Should I invest in index funds?",
                FinancialContext::Investment,
            )
            .await
            .unwrap();

        assert!(!analysis.summary.is_empty());
        assert!(!analysis.options.is_empty());
    }

    #[tokio::test]
    async fn test_technical_explanation() {
        let provider = Arc::new(MockProvider);
        let engine = ExpertEngine::new(provider);

        let explanation = engine
            .technical_question(
                "Explain microservices",
                TechnologyArea::SystemDesign,
                ExperienceLevel::Intermediate,
            )
            .await
            .unwrap();

        assert!(!explanation.explanation.is_empty());
        assert!(!explanation.key_concepts.is_empty());
    }
}
