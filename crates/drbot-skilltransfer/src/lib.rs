//! Skill transfer for drbot.
//!
//! Transfer learned skills between domains and tasks.
//!
//! # Features
//!
//! - Skill extraction
//! - Cross-domain transfer
//! - Skill adaptation
//! - Performance tracking

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Skill transfer result type.
pub type Result<T> = std::result::Result<T, TransferError>;

/// Transfer errors.
#[derive(Debug, thiserror::Error)]
pub enum TransferError {
    #[error("Skill not found: {0}")]
    SkillNotFound(Uuid),
    #[error("Transfer failed: {0}")]
    TransferFailed(String),
    #[error("Incompatible domains: {0} -> {1}")]
    IncompatibleDomains(String, String),
    #[error("Adaptation failed: {0}")]
    AdaptationFailed(String),
}

/// A learned skill.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    /// Skill ID.
    pub id: Uuid,
    /// Skill name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Source domain.
    pub domain: String,
    /// Skill type.
    pub skill_type: SkillType,
    /// Core patterns.
    pub patterns: Vec<SkillPattern>,
    /// Required context.
    pub required_context: Vec<String>,
    /// Proficiency level (0-1).
    pub proficiency: f32,
    /// Usage count.
    pub usage_count: u64,
    /// Success rate.
    pub success_rate: f32,
    /// Learned at.
    pub learned_at: DateTime<Utc>,
    /// Last used.
    pub last_used: Option<DateTime<Utc>>,
    /// Metadata.
    pub metadata: HashMap<String, serde_json::Value>,
}

impl Skill {
    /// Create a new skill.
    pub fn new(name: &str, domain: &str, skill_type: SkillType) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.to_string(),
            description: String::new(),
            domain: domain.to_string(),
            skill_type,
            patterns: Vec::new(),
            required_context: Vec::new(),
            proficiency: 0.5,
            usage_count: 0,
            success_rate: 0.0,
            learned_at: Utc::now(),
            last_used: None,
            metadata: HashMap::new(),
        }
    }

    /// Add a pattern.
    pub fn with_pattern(mut self, pattern: SkillPattern) -> Self {
        self.patterns.push(pattern);
        self
    }

    /// Set description.
    pub fn with_description(mut self, description: &str) -> Self {
        self.description = description.to_string();
        self
    }

    /// Record usage.
    pub fn record_usage(&mut self, success: bool) {
        self.usage_count += 1;
        self.last_used = Some(Utc::now());

        let n = self.usage_count as f32;
        self.success_rate = (self.success_rate * (n - 1.0) + if success { 1.0 } else { 0.0 }) / n;

        // Update proficiency based on success
        if success {
            self.proficiency = (self.proficiency + 0.01).min(1.0);
        } else {
            self.proficiency = (self.proficiency - 0.005).max(0.0);
        }
    }
}

/// Skill types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillType {
    /// Reasoning pattern.
    Reasoning,
    /// Problem solving.
    ProblemSolving,
    /// Code generation.
    Coding,
    /// Analysis.
    Analysis,
    /// Communication.
    Communication,
    /// Knowledge application.
    Knowledge,
    /// Creativity.
    Creative,
    /// Other.
    Other,
}

/// A pattern within a skill.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillPattern {
    /// Pattern name.
    pub name: String,
    /// Pattern description.
    pub description: String,
    /// Input template.
    pub input_template: String,
    /// Output template.
    pub output_template: String,
    /// Confidence.
    pub confidence: f32,
}

impl SkillPattern {
    /// Create a new pattern.
    pub fn new(name: &str, input: &str, output: &str) -> Self {
        Self {
            name: name.to_string(),
            description: String::new(),
            input_template: input.to_string(),
            output_template: output.to_string(),
            confidence: 0.8,
        }
    }
}

/// A skill transfer operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillTransfer {
    /// Transfer ID.
    pub id: Uuid,
    /// Source skill.
    pub source_skill: Uuid,
    /// Target domain.
    pub target_domain: String,
    /// Adapted skill (result).
    pub adapted_skill: Option<Skill>,
    /// Transfer success.
    pub success: bool,
    /// Adaptation changes.
    pub adaptations: Vec<Adaptation>,
    /// Similarity score.
    pub similarity: f32,
    /// Created at.
    pub created_at: DateTime<Utc>,
}

/// An adaptation made during transfer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Adaptation {
    /// Adaptation type.
    pub adaptation_type: AdaptationType,
    /// Original.
    pub original: String,
    /// Adapted.
    pub adapted: String,
    /// Reason.
    pub reason: String,
}

/// Adaptation types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdaptationType {
    /// Rename/relabel.
    Rename,
    /// Modify structure.
    Restructure,
    /// Add context.
    AddContext,
    /// Remove irrelevant.
    Remove,
    /// Specialize.
    Specialize,
    /// Generalize.
    Generalize,
}

/// Skill transfer configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferConfig {
    /// Minimum similarity for transfer.
    pub min_similarity: f32,
    /// Enable automatic adaptation.
    pub auto_adapt: bool,
    /// Preserve original skill.
    pub preserve_original: bool,
    /// Maximum adaptations.
    pub max_adaptations: usize,
}

impl Default for TransferConfig {
    fn default() -> Self {
        Self {
            min_similarity: 0.5,
            auto_adapt: true,
            preserve_original: true,
            max_adaptations: 10,
        }
    }
}

/// Trait for skill adapters.
#[async_trait]
pub trait SkillAdapter: Send + Sync {
    /// Adapt a skill to a new domain.
    async fn adapt(&self, skill: &Skill, target_domain: &str) -> Result<(Skill, Vec<Adaptation>)>;
}

/// Trait for domain similarity calculators.
#[async_trait]
pub trait DomainSimilarity: Send + Sync {
    /// Calculate similarity between domains.
    async fn similarity(&self, domain_a: &str, domain_b: &str) -> f32;
}

/// Skill transfer engine.
pub struct SkillTransferEngine<A: SkillAdapter, S: DomainSimilarity> {
    config: TransferConfig,
    adapter: A,
    similarity: S,
    skills: Arc<RwLock<HashMap<Uuid, Skill>>>,
    transfers: Arc<RwLock<Vec<SkillTransfer>>>,
}

impl<A: SkillAdapter, S: DomainSimilarity> SkillTransferEngine<A, S> {
    /// Create a new transfer engine.
    pub fn new(config: TransferConfig, adapter: A, similarity: S) -> Self {
        Self {
            config,
            adapter,
            similarity,
            skills: Arc::new(RwLock::new(HashMap::new())),
            transfers: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Register a skill.
    pub async fn register_skill(&self, skill: Skill) -> Uuid {
        let id = skill.id;
        self.skills.write().await.insert(id, skill);
        id
    }

    /// Get a skill.
    pub async fn get_skill(&self, id: Uuid) -> Option<Skill> {
        self.skills.read().await.get(&id).cloned()
    }

    /// List skills by domain.
    pub async fn skills_by_domain(&self, domain: &str) -> Vec<Skill> {
        self.skills
            .read()
            .await
            .values()
            .filter(|s| s.domain == domain)
            .cloned()
            .collect()
    }

    /// Transfer a skill to a new domain.
    pub async fn transfer(&self, skill_id: Uuid, target_domain: &str) -> Result<SkillTransfer> {
        let skill = self
            .skills
            .read()
            .await
            .get(&skill_id)
            .cloned()
            .ok_or(TransferError::SkillNotFound(skill_id))?;

        // Check domain similarity
        let similarity = self
            .similarity
            .similarity(&skill.domain, target_domain)
            .await;

        if similarity < self.config.min_similarity {
            return Err(TransferError::IncompatibleDomains(
                skill.domain.clone(),
                target_domain.to_string(),
            ));
        }

        // Adapt the skill
        let (adapted_skill, adaptations) = if self.config.auto_adapt {
            self.adapter.adapt(&skill, target_domain).await?
        } else {
            (skill.clone(), Vec::new())
        };

        let transfer = SkillTransfer {
            id: Uuid::new_v4(),
            source_skill: skill_id,
            target_domain: target_domain.to_string(),
            adapted_skill: Some(adapted_skill.clone()),
            success: true,
            adaptations,
            similarity,
            created_at: Utc::now(),
        };

        // Store adapted skill
        self.skills
            .write()
            .await
            .insert(adapted_skill.id, adapted_skill);

        // Store transfer record
        self.transfers.write().await.push(transfer.clone());

        Ok(transfer)
    }

    /// Find transferable skills for a domain.
    pub async fn find_transferable(
        &self,
        target_domain: &str,
        min_similarity: f32,
    ) -> Vec<(Skill, f32)> {
        let skills = self.skills.read().await;
        let mut results = Vec::new();

        for skill in skills.values() {
            if skill.domain != target_domain {
                let sim = self
                    .similarity
                    .similarity(&skill.domain, target_domain)
                    .await;
                if sim >= min_similarity {
                    results.push((skill.clone(), sim));
                }
            }
        }

        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        results
    }

    /// Record skill usage.
    pub async fn record_usage(&self, skill_id: Uuid, success: bool) {
        let mut skills = self.skills.write().await;
        if let Some(skill) = skills.get_mut(&skill_id) {
            skill.record_usage(success);
        }
    }

    /// List all skills.
    pub async fn list_skills(&self) -> Vec<Skill> {
        self.skills.read().await.values().cloned().collect()
    }

    /// List all transfers.
    pub async fn list_transfers(&self) -> Vec<SkillTransfer> {
        self.transfers.read().await.clone()
    }

    /// Get statistics.
    pub async fn stats(&self) -> TransferStats {
        let skills = self.skills.read().await;
        let transfers = self.transfers.read().await;

        let mut by_domain: HashMap<String, usize> = HashMap::new();
        let mut by_type: HashMap<SkillType, usize> = HashMap::new();

        for skill in skills.values() {
            *by_domain.entry(skill.domain.clone()).or_insert(0) += 1;
            *by_type.entry(skill.skill_type).or_insert(0) += 1;
        }

        let avg_proficiency = if !skills.is_empty() {
            skills.values().map(|s| s.proficiency).sum::<f32>() / skills.len() as f32
        } else {
            0.0
        };

        TransferStats {
            total_skills: skills.len(),
            skills_by_domain: by_domain,
            skills_by_type: by_type,
            total_transfers: transfers.len(),
            successful_transfers: transfers.iter().filter(|t| t.success).count(),
            avg_proficiency,
        }
    }
}

/// Transfer statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferStats {
    pub total_skills: usize,
    pub skills_by_domain: HashMap<String, usize>,
    pub skills_by_type: HashMap<SkillType, usize>,
    pub total_transfers: usize,
    pub successful_transfers: usize,
    pub avg_proficiency: f32,
}

/// Simple skill adapter.
pub struct SimpleAdapter;

#[async_trait]
impl SkillAdapter for SimpleAdapter {
    async fn adapt(&self, skill: &Skill, target_domain: &str) -> Result<(Skill, Vec<Adaptation>)> {
        let mut adapted = skill.clone();
        adapted.id = Uuid::new_v4();
        adapted.domain = target_domain.to_string();
        adapted.name = format!("{} ({})", skill.name, target_domain);
        adapted.proficiency *= 0.8; // Reduce proficiency for new domain
        adapted.learned_at = Utc::now();

        let adaptations = vec![Adaptation {
            adaptation_type: AdaptationType::Rename,
            original: skill.name.clone(),
            adapted: adapted.name.clone(),
            reason: "Adapted to new domain".to_string(),
        }];

        Ok((adapted, adaptations))
    }
}

/// Simple domain similarity calculator.
pub struct SimpleSimilarity;

#[async_trait]
impl DomainSimilarity for SimpleSimilarity {
    async fn similarity(&self, domain_a: &str, domain_b: &str) -> f32 {
        if domain_a == domain_b {
            return 1.0;
        }

        // Simple word overlap
        let domain_a_lower = domain_a.to_lowercase();
        let domain_b_lower = domain_b.to_lowercase();
        let words_a: std::collections::HashSet<_> = domain_a_lower.split('_').collect();
        let words_b: std::collections::HashSet<_> = domain_b_lower.split('_').collect();

        let intersection = words_a.intersection(&words_b).count();
        let union = words_a.union(&words_b).count();

        if union == 0 {
            0.3 // Base similarity
        } else {
            0.3 + 0.7 * (intersection as f32 / union as f32)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_skill_registration() {
        let engine =
            SkillTransferEngine::new(TransferConfig::default(), SimpleAdapter, SimpleSimilarity);

        let skill = Skill::new(
            "debugging",
            "software_engineering",
            SkillType::ProblemSolving,
        );
        let id = engine.register_skill(skill).await;

        let retrieved = engine.get_skill(id).await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, "debugging");
    }

    #[tokio::test]
    async fn test_skill_transfer() {
        let engine =
            SkillTransferEngine::new(TransferConfig::default(), SimpleAdapter, SimpleSimilarity);

        let skill = Skill::new("data_analysis", "data_science", SkillType::Analysis);
        let id = engine.register_skill(skill).await;

        let transfer = engine.transfer(id, "data_analytics").await.unwrap();
        assert!(transfer.success);
        assert!(transfer.adapted_skill.is_some());
    }

    #[tokio::test]
    async fn test_find_transferable() {
        let engine =
            SkillTransferEngine::new(TransferConfig::default(), SimpleAdapter, SimpleSimilarity);

        engine
            .register_skill(Skill::new(
                "code_review",
                "software_engineering",
                SkillType::Analysis,
            ))
            .await;
        engine
            .register_skill(Skill::new(
                "writing",
                "content_creation",
                SkillType::Communication,
            ))
            .await;

        let transferable = engine.find_transferable("software_development", 0.3).await;
        assert!(!transferable.is_empty());
    }

    #[test]
    fn test_skill_usage_tracking() {
        let mut skill = Skill::new("test", "domain", SkillType::Other);

        skill.record_usage(true);
        skill.record_usage(true);
        skill.record_usage(false);

        assert_eq!(skill.usage_count, 3);
        assert!((skill.success_rate - 0.666).abs() < 0.01);
    }
}
