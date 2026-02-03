//! Response variants for drbot.
//!
//! Generate and manage response variations.
//!
//! # Features
//!
//! - Multiple response generation
//! - Variant selection
//! - A/B testing
//! - User preference learning

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Variants result type.
pub type Result<T> = std::result::Result<T, VariantError>;

/// Variant errors.
#[derive(Debug, thiserror::Error)]
pub enum VariantError {
    #[error("No variants available")]
    NoVariants,
    #[error("Variant not found: {0}")]
    NotFound(Uuid),
    #[error("Generation failed: {0}")]
    GenerationFailed(String),
}

/// A response variant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseVariant {
    /// Variant ID.
    pub id: Uuid,
    /// Variant content.
    pub content: String,
    /// Variant type.
    pub variant_type: VariantType,
    /// Tone.
    pub tone: Tone,
    /// Length category.
    pub length: Length,
    /// Score (0-1).
    pub score: f32,
    /// Selection count.
    pub selection_count: u64,
    /// Created at.
    pub created_at: DateTime<Utc>,
    /// Metadata.
    pub metadata: HashMap<String, serde_json::Value>,
}

impl ResponseVariant {
    /// Create a new variant.
    pub fn new(content: &str, variant_type: VariantType) -> Self {
        let length = if content.len() < 100 {
            Length::Short
        } else if content.len() < 500 {
            Length::Medium
        } else {
            Length::Long
        };

        Self {
            id: Uuid::new_v4(),
            content: content.to_string(),
            variant_type,
            tone: Tone::Neutral,
            length,
            score: 0.5,
            selection_count: 0,
            created_at: Utc::now(),
            metadata: HashMap::new(),
        }
    }

    /// Set tone.
    pub fn with_tone(mut self, tone: Tone) -> Self {
        self.tone = tone;
        self
    }

    /// Set score.
    pub fn with_score(mut self, score: f32) -> Self {
        self.score = score.clamp(0.0, 1.0);
        self
    }

    /// Record selection.
    pub fn record_selection(&mut self) {
        self.selection_count += 1;
    }
}

/// Variant types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VariantType {
    /// Original response.
    Original,
    /// Simplified version.
    Simplified,
    /// Detailed version.
    Detailed,
    /// Technical version.
    Technical,
    /// Casual version.
    Casual,
    /// Formal version.
    Formal,
    /// Bullet points.
    BulletPoints,
    /// Step by step.
    StepByStep,
}

/// Tone options.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tone {
    Neutral,
    Friendly,
    Professional,
    Enthusiastic,
    Empathetic,
    Direct,
}

/// Length categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Length {
    Short,
    Medium,
    Long,
}

/// Variant set for a query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariantSet {
    /// Set ID.
    pub id: Uuid,
    /// Original query.
    pub query: String,
    /// Variants.
    pub variants: Vec<ResponseVariant>,
    /// Selected variant ID.
    pub selected: Option<Uuid>,
    /// Created at.
    pub created_at: DateTime<Utc>,
}

impl VariantSet {
    /// Create a new set.
    pub fn new(query: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            query: query.to_string(),
            variants: Vec::new(),
            selected: None,
            created_at: Utc::now(),
        }
    }

    /// Add a variant.
    pub fn add_variant(&mut self, variant: ResponseVariant) {
        self.variants.push(variant);
    }

    /// Get variant by ID.
    pub fn get_variant(&self, id: Uuid) -> Option<&ResponseVariant> {
        self.variants.iter().find(|v| v.id == id)
    }

    /// Get best variant by score.
    pub fn best_variant(&self) -> Option<&ResponseVariant> {
        self.variants
            .iter()
            .max_by(|a, b| a.score.partial_cmp(&b.score).unwrap())
    }

    /// Select a variant.
    pub fn select(&mut self, variant_id: Uuid) -> bool {
        if let Some(variant) = self.variants.iter_mut().find(|v| v.id == variant_id) {
            variant.record_selection();
            self.selected = Some(variant_id);
            true
        } else {
            false
        }
    }
}

/// Variant configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariantConfig {
    /// Number of variants to generate.
    pub num_variants: usize,
    /// Variant types to generate.
    pub types: Vec<VariantType>,
    /// Include scores.
    pub include_scores: bool,
    /// Enable learning.
    pub learning_enabled: bool,
}

impl Default for VariantConfig {
    fn default() -> Self {
        Self {
            num_variants: 3,
            types: vec![
                VariantType::Original,
                VariantType::Simplified,
                VariantType::Detailed,
            ],
            include_scores: true,
            learning_enabled: true,
        }
    }
}

/// User preferences for variants.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VariantPreferences {
    /// Preferred types.
    pub preferred_types: HashMap<VariantType, f32>,
    /// Preferred tones.
    pub preferred_tones: HashMap<Tone, f32>,
    /// Preferred length.
    pub preferred_length: Option<Length>,
}

impl VariantPreferences {
    /// Update preferences based on selection.
    pub fn update(&mut self, variant: &ResponseVariant) {
        // Boost selected type
        let type_score = self
            .preferred_types
            .entry(variant.variant_type)
            .or_insert(0.5);
        *type_score = (*type_score + 0.1).min(1.0);

        // Boost selected tone
        let tone_score = self.preferred_tones.entry(variant.tone).or_insert(0.5);
        *tone_score = (*tone_score + 0.1).min(1.0);

        // Update length preference
        self.preferred_length = Some(variant.length);
    }

    /// Score a variant based on preferences.
    pub fn score_variant(&self, variant: &ResponseVariant) -> f32 {
        let mut score = variant.score;

        // Add type preference
        if let Some(&pref) = self.preferred_types.get(&variant.variant_type) {
            score += pref * 0.2;
        }

        // Add tone preference
        if let Some(&pref) = self.preferred_tones.get(&variant.tone) {
            score += pref * 0.1;
        }

        // Add length preference
        if let Some(preferred) = self.preferred_length {
            if preferred == variant.length {
                score += 0.1;
            }
        }

        score.min(1.0)
    }
}

/// Variant manager.
pub struct VariantManager {
    config: VariantConfig,
    sets: Arc<RwLock<HashMap<Uuid, VariantSet>>>,
    preferences: Arc<RwLock<HashMap<String, VariantPreferences>>>,
}

impl VariantManager {
    /// Create a new manager.
    pub fn new(config: VariantConfig) -> Self {
        Self {
            config,
            sets: Arc::new(RwLock::new(HashMap::new())),
            preferences: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Generate variants for a response.
    pub async fn generate(&self, query: &str, original: &str) -> VariantSet {
        let mut set = VariantSet::new(query);

        // Add original
        let original_variant =
            ResponseVariant::new(original, VariantType::Original).with_score(0.8);
        set.add_variant(original_variant);

        // Generate other variants based on config
        for variant_type in &self.config.types {
            if *variant_type == VariantType::Original {
                continue;
            }

            let content = self.transform(original, *variant_type);
            let tone = self.determine_tone(*variant_type);
            let variant = ResponseVariant::new(&content, *variant_type)
                .with_tone(tone)
                .with_score(0.7);
            set.add_variant(variant);
        }

        self.sets.write().await.insert(set.id, set.clone());

        set
    }

    fn transform(&self, content: &str, variant_type: VariantType) -> String {
        match variant_type {
            VariantType::Simplified => {
                // Simplify (basic implementation)
                let sentences: Vec<_> = content.split('.').take(2).collect();
                sentences.join(". ") + "."
            }
            VariantType::Detailed => {
                format!("{}\n\nFor more context: This response provides the core information you requested.", content)
            }
            VariantType::BulletPoints => {
                let points: Vec<_> = content
                    .split(". ")
                    .filter(|s| !s.is_empty())
                    .map(|s| format!("• {}", s.trim_end_matches('.')))
                    .collect();
                points.join("\n")
            }
            VariantType::StepByStep => {
                let steps: Vec<_> = content
                    .split(". ")
                    .filter(|s| !s.is_empty())
                    .enumerate()
                    .map(|(i, s)| format!("{}. {}", i + 1, s.trim_end_matches('.')))
                    .collect();
                steps.join("\n")
            }
            VariantType::Casual => {
                format!("So basically, {}", content.to_lowercase())
            }
            VariantType::Formal => {
                format!("Please note that {}.", content.trim_end_matches('.'))
            }
            VariantType::Technical => content.to_string(),
            VariantType::Original => content.to_string(),
        }
    }

    fn determine_tone(&self, variant_type: VariantType) -> Tone {
        match variant_type {
            VariantType::Casual => Tone::Friendly,
            VariantType::Formal | VariantType::Technical => Tone::Professional,
            _ => Tone::Neutral,
        }
    }

    /// Select a variant.
    pub async fn select(
        &self,
        set_id: Uuid,
        variant_id: Uuid,
        user_id: &str,
    ) -> Result<ResponseVariant> {
        let mut sets = self.sets.write().await;
        let set = sets
            .get_mut(&set_id)
            .ok_or(VariantError::NotFound(set_id))?;

        if !set.select(variant_id) {
            return Err(VariantError::NotFound(variant_id));
        }

        let variant = set.get_variant(variant_id).unwrap().clone();

        // Update preferences
        if self.config.learning_enabled {
            let mut prefs = self.preferences.write().await;
            let user_prefs = prefs.entry(user_id.to_string()).or_default();
            user_prefs.update(&variant);
        }

        Ok(variant)
    }

    /// Get best variant for user.
    pub async fn best_for_user(&self, set_id: Uuid, user_id: &str) -> Result<ResponseVariant> {
        let sets = self.sets.read().await;
        let set = sets.get(&set_id).ok_or(VariantError::NotFound(set_id))?;

        let prefs = self.preferences.read().await;
        let user_prefs = prefs.get(user_id);

        let best = if let Some(prefs) = user_prefs {
            set.variants.iter().max_by(|a, b| {
                prefs
                    .score_variant(a)
                    .partial_cmp(&prefs.score_variant(b))
                    .unwrap()
            })
        } else {
            set.best_variant()
        };

        best.cloned().ok_or(VariantError::NoVariants)
    }

    /// Get user preferences.
    pub async fn get_preferences(&self, user_id: &str) -> VariantPreferences {
        self.preferences
            .read()
            .await
            .get(user_id)
            .cloned()
            .unwrap_or_default()
    }

    /// Get variant set.
    pub async fn get_set(&self, set_id: Uuid) -> Option<VariantSet> {
        self.sets.read().await.get(&set_id).cloned()
    }

    /// Get statistics.
    pub async fn stats(&self) -> VariantStats {
        let sets = self.sets.read().await;
        let prefs = self.preferences.read().await;

        let total_variants: usize = sets.values().map(|s| s.variants.len()).sum();
        let total_selections: u64 = sets
            .values()
            .flat_map(|s| &s.variants)
            .map(|v| v.selection_count)
            .sum();

        VariantStats {
            total_sets: sets.len(),
            total_variants,
            total_selections,
            users_with_preferences: prefs.len(),
        }
    }
}

/// Variant statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariantStats {
    pub total_sets: usize,
    pub total_variants: usize,
    pub total_selections: u64,
    pub users_with_preferences: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_generate_variants() {
        let manager = VariantManager::new(VariantConfig::default());

        let set = manager
            .generate("What is Rust?", "Rust is a systems programming language.")
            .await;

        assert!(!set.variants.is_empty());
        assert!(set
            .variants
            .iter()
            .any(|v| v.variant_type == VariantType::Original));
    }

    #[tokio::test]
    async fn test_select_variant() {
        let manager = VariantManager::new(VariantConfig::default());

        let set = manager.generate("Query", "Response content.").await;
        let variant_id = set.variants[0].id;

        let selected = manager.select(set.id, variant_id, "user1").await.unwrap();
        assert_eq!(selected.id, variant_id);
    }

    #[tokio::test]
    async fn test_preferences_learning() {
        let config = VariantConfig {
            learning_enabled: true,
            ..Default::default()
        };
        let manager = VariantManager::new(config);

        let set = manager.generate("Query", "Response.").await;
        let variant = set
            .variants
            .iter()
            .find(|v| v.variant_type == VariantType::Simplified);

        if let Some(v) = variant {
            manager.select(set.id, v.id, "user1").await.unwrap();
        }

        let prefs = manager.get_preferences("user1").await;
        assert!(prefs.preferred_types.contains_key(&VariantType::Simplified));
    }

    #[test]
    fn test_variant_transforms() {
        let manager = VariantManager::new(VariantConfig::default());

        let original = "First point. Second point. Third point.";

        let bullet = manager.transform(original, VariantType::BulletPoints);
        assert!(bullet.contains("•"));

        let steps = manager.transform(original, VariantType::StepByStep);
        assert!(steps.contains("1."));
    }
}
