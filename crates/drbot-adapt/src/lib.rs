//! Personalization and adaptation system for drbot
//!
//! Learns user preferences, adapts responses, and handles accessibility.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;

#[derive(Error, Debug)]
pub enum AdaptError {
    #[error("User not found: {0}")]
    UserNotFound(String),
    #[error("Preference not found: {0}")]
    PreferenceNotFound(String),
    #[error("Profile error: {0}")]
    ProfileError(String),
    #[error("Provider error: {0}")]
    ProviderError(String),
}

pub type Result<T> = std::result::Result<T, AdaptError>;

// ============================================================================
// User Profile
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfile {
    pub id: String,
    pub name: Option<String>,
    pub preferences: UserPreferences,
    pub accessibility: AccessibilitySettings,
    pub learning_profile: LearningProfile,
    pub interaction_history: InteractionHistory,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserPreferences {
    pub language: String,
    pub timezone: String,
    pub response_style: ResponseStyle,
    pub verbosity: Verbosity,
    pub formality: Formality,
    pub humor_level: HumorLevel,
    pub expertise_areas: Vec<String>,
    pub interests: Vec<String>,
    pub avoided_topics: Vec<String>,
    pub preferred_formats: Vec<ContentFormat>,
    pub notification_preferences: NotificationPreferences,
    pub custom: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum ResponseStyle {
    Concise,
    Balanced,
    Detailed,
    Tutorial,
    Conversational,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum Verbosity {
    Minimal,
    Brief,
    Standard,
    Detailed,
    Comprehensive,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum Formality {
    VeryFormal,
    Formal,
    Neutral,
    Casual,
    VeryCasual,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum HumorLevel {
    None,
    Subtle,
    Moderate,
    Frequent,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ContentFormat {
    Text,
    Bullet,
    Numbered,
    Table,
    Code,
    Markdown,
    Plain,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationPreferences {
    pub enabled: bool,
    pub quiet_hours: Option<(String, String)>,
    pub channels: Vec<String>,
    pub urgency_threshold: UrgencyThreshold,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum UrgencyThreshold {
    All,
    Important,
    Critical,
    None,
}

// ============================================================================
// Accessibility
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessibilitySettings {
    pub visual: VisualAccessibility,
    pub auditory: AuditoryAccessibility,
    pub motor: MotorAccessibility,
    pub cognitive: CognitiveAccessibility,
    pub custom_accommodations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisualAccessibility {
    pub screen_reader: bool,
    pub high_contrast: bool,
    pub large_text: bool,
    pub text_size_multiplier: f32,
    pub reduce_motion: bool,
    pub alt_text_preference: AltTextPreference,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum AltTextPreference {
    Detailed,
    Standard,
    Brief,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditoryAccessibility {
    pub captions: bool,
    pub transcripts: bool,
    pub visual_alerts: bool,
    pub mono_audio: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MotorAccessibility {
    pub keyboard_only: bool,
    pub voice_control: bool,
    pub extended_timeouts: bool,
    pub simplified_interactions: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CognitiveAccessibility {
    pub simple_language: bool,
    pub reading_level: ReadingLevel,
    pub chunked_content: bool,
    pub summary_first: bool,
    pub progress_indicators: bool,
    pub consistent_layout: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum ReadingLevel {
    Elementary,
    MiddleSchool,
    HighSchool,
    College,
    Professional,
    Academic,
}

// ============================================================================
// Learning Profile
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningProfile {
    pub learned_preferences: Vec<LearnedPreference>,
    pub behavior_patterns: Vec<BehaviorPattern>,
    pub skill_levels: HashMap<String, SkillLevel>,
    pub feedback_history: Vec<Feedback>,
    pub adaptation_confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearnedPreference {
    pub category: String,
    pub preference: String,
    pub confidence: f32,
    pub evidence_count: usize,
    pub last_observed: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehaviorPattern {
    pub pattern_type: PatternType,
    pub description: String,
    pub frequency: f32,
    pub time_context: Option<TimeContext>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PatternType {
    TimeOfDay,
    DayOfWeek,
    TaskType,
    ResponseLength,
    TopicInterest,
    InteractionStyle,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeContext {
    pub hours: Option<Vec<u8>>,
    pub days: Option<Vec<u8>>,
    pub months: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum SkillLevel {
    Beginner,
    Intermediate,
    Advanced,
    Expert,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Feedback {
    pub id: String,
    pub feedback_type: FeedbackType,
    pub context: String,
    pub rating: Option<i8>,
    pub comment: Option<String>,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FeedbackType {
    Positive,
    Negative,
    Correction,
    Preference,
    Suggestion,
}

// ============================================================================
// Interaction History
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractionHistory {
    pub total_interactions: u64,
    pub recent_topics: Vec<String>,
    pub common_requests: Vec<CommonRequest>,
    pub session_patterns: Vec<SessionPattern>,
    pub satisfaction_trend: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommonRequest {
    pub request_type: String,
    pub frequency: u32,
    pub last_used: u64,
    pub avg_satisfaction: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionPattern {
    pub avg_duration_seconds: u32,
    pub avg_messages: u32,
    pub common_start_times: Vec<String>,
    pub common_end_reasons: Vec<String>,
}

// ============================================================================
// Adaptation
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdaptationContext {
    pub user_id: String,
    pub current_task: Option<String>,
    pub time_of_day: String,
    pub device_type: Option<String>,
    pub session_length: u32,
    pub recent_feedback: Vec<Feedback>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdaptedResponse {
    pub original: String,
    pub adapted: String,
    pub adaptations_applied: Vec<AdaptationType>,
    pub accessibility_adjustments: Vec<String>,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AdaptationType {
    Verbosity,
    Formality,
    Style,
    Format,
    Simplification,
    DetailLevel,
    ToneAdjustment,
    AccessibilityEnhancement,
}

// ============================================================================
// Provider Trait
// ============================================================================

#[async_trait]
pub trait AdaptProvider: Send + Sync {
    async fn adapt_response(
        &self,
        content: &str,
        profile: &UserProfile,
        context: &AdaptationContext,
    ) -> Result<AdaptedResponse>;
    async fn infer_preferences(&self, interactions: &[String]) -> Result<Vec<LearnedPreference>>;
    async fn suggest_accessibility(&self, behavior: &[String]) -> Result<AccessibilitySettings>;
    async fn simplify_for_reading_level(&self, text: &str, level: ReadingLevel) -> Result<String>;
}

// ============================================================================
// Adapt Engine
// ============================================================================

pub struct AdaptEngine {
    provider: Arc<dyn AdaptProvider>,
    profiles: Arc<RwLock<HashMap<String, UserProfile>>>,
    session_contexts: Arc<RwLock<HashMap<String, AdaptationContext>>>,
}

impl AdaptEngine {
    pub fn new(provider: Arc<dyn AdaptProvider>) -> Self {
        Self {
            provider,
            profiles: Arc::new(RwLock::new(HashMap::new())),
            session_contexts: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    fn now() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    // Profile Management
    pub async fn create_profile(&self, user_id: &str) -> Result<UserProfile> {
        let profile = UserProfile {
            id: user_id.to_string(),
            name: None,
            preferences: UserPreferences::default(),
            accessibility: AccessibilitySettings::default(),
            learning_profile: LearningProfile::default(),
            interaction_history: InteractionHistory::default(),
            created_at: Self::now(),
            updated_at: Self::now(),
        };

        let mut profiles = self.profiles.write().await;
        profiles.insert(user_id.to_string(), profile.clone());

        Ok(profile)
    }

    pub async fn get_profile(&self, user_id: &str) -> Result<UserProfile> {
        let profiles = self.profiles.read().await;
        profiles
            .get(user_id)
            .cloned()
            .ok_or_else(|| AdaptError::UserNotFound(user_id.to_string()))
    }

    pub async fn get_or_create_profile(&self, user_id: &str) -> Result<UserProfile> {
        match self.get_profile(user_id).await {
            Ok(profile) => Ok(profile),
            Err(_) => self.create_profile(user_id).await,
        }
    }

    pub async fn update_preferences(
        &self,
        user_id: &str,
        preferences: UserPreferences,
    ) -> Result<UserProfile> {
        let mut profiles = self.profiles.write().await;
        let profile = profiles
            .get_mut(user_id)
            .ok_or_else(|| AdaptError::UserNotFound(user_id.to_string()))?;

        profile.preferences = preferences;
        profile.updated_at = Self::now();

        Ok(profile.clone())
    }

    pub async fn update_accessibility(
        &self,
        user_id: &str,
        settings: AccessibilitySettings,
    ) -> Result<UserProfile> {
        let mut profiles = self.profiles.write().await;
        let profile = profiles
            .get_mut(user_id)
            .ok_or_else(|| AdaptError::UserNotFound(user_id.to_string()))?;

        profile.accessibility = settings;
        profile.updated_at = Self::now();

        Ok(profile.clone())
    }

    // Preference Setting
    pub async fn set_verbosity(&self, user_id: &str, verbosity: Verbosity) -> Result<()> {
        let mut profiles = self.profiles.write().await;
        let profile = profiles
            .get_mut(user_id)
            .ok_or_else(|| AdaptError::UserNotFound(user_id.to_string()))?;

        profile.preferences.verbosity = verbosity;
        profile.updated_at = Self::now();
        Ok(())
    }

    pub async fn set_formality(&self, user_id: &str, formality: Formality) -> Result<()> {
        let mut profiles = self.profiles.write().await;
        let profile = profiles
            .get_mut(user_id)
            .ok_or_else(|| AdaptError::UserNotFound(user_id.to_string()))?;

        profile.preferences.formality = formality;
        profile.updated_at = Self::now();
        Ok(())
    }

    pub async fn set_response_style(&self, user_id: &str, style: ResponseStyle) -> Result<()> {
        let mut profiles = self.profiles.write().await;
        let profile = profiles
            .get_mut(user_id)
            .ok_or_else(|| AdaptError::UserNotFound(user_id.to_string()))?;

        profile.preferences.response_style = style;
        profile.updated_at = Self::now();
        Ok(())
    }

    pub async fn add_interest(&self, user_id: &str, interest: &str) -> Result<()> {
        let mut profiles = self.profiles.write().await;
        let profile = profiles
            .get_mut(user_id)
            .ok_or_else(|| AdaptError::UserNotFound(user_id.to_string()))?;

        if !profile
            .preferences
            .interests
            .contains(&interest.to_string())
        {
            profile.preferences.interests.push(interest.to_string());
            profile.updated_at = Self::now();
        }
        Ok(())
    }

    pub async fn add_avoided_topic(&self, user_id: &str, topic: &str) -> Result<()> {
        let mut profiles = self.profiles.write().await;
        let profile = profiles
            .get_mut(user_id)
            .ok_or_else(|| AdaptError::UserNotFound(user_id.to_string()))?;

        if !profile
            .preferences
            .avoided_topics
            .contains(&topic.to_string())
        {
            profile.preferences.avoided_topics.push(topic.to_string());
            profile.updated_at = Self::now();
        }
        Ok(())
    }

    // Accessibility
    pub async fn enable_screen_reader(&self, user_id: &str, enabled: bool) -> Result<()> {
        let mut profiles = self.profiles.write().await;
        let profile = profiles
            .get_mut(user_id)
            .ok_or_else(|| AdaptError::UserNotFound(user_id.to_string()))?;

        profile.accessibility.visual.screen_reader = enabled;
        profile.updated_at = Self::now();
        Ok(())
    }

    pub async fn set_reading_level(&self, user_id: &str, level: ReadingLevel) -> Result<()> {
        let mut profiles = self.profiles.write().await;
        let profile = profiles
            .get_mut(user_id)
            .ok_or_else(|| AdaptError::UserNotFound(user_id.to_string()))?;

        profile.accessibility.cognitive.reading_level = level;
        profile.updated_at = Self::now();
        Ok(())
    }

    pub async fn suggest_accessibility_settings(
        &self,
        user_id: &str,
        behaviors: &[String],
    ) -> Result<AccessibilitySettings> {
        self.provider.suggest_accessibility(behaviors).await
    }

    // Adaptation
    pub async fn adapt_for_user(&self, user_id: &str, content: &str) -> Result<AdaptedResponse> {
        let profile = self.get_or_create_profile(user_id).await?;

        let context = self.get_or_create_context(user_id).await?;

        self.provider
            .adapt_response(content, &profile, &context)
            .await
    }

    pub async fn simplify(&self, user_id: &str, text: &str) -> Result<String> {
        let profile = self.get_or_create_profile(user_id).await?;
        let level = profile.accessibility.cognitive.reading_level;
        self.provider.simplify_for_reading_level(text, level).await
    }

    // Learning
    pub async fn record_feedback(&self, user_id: &str, feedback: Feedback) -> Result<()> {
        let mut profiles = self.profiles.write().await;
        let profile = profiles
            .get_mut(user_id)
            .ok_or_else(|| AdaptError::UserNotFound(user_id.to_string()))?;

        profile.learning_profile.feedback_history.push(feedback);
        profile.updated_at = Self::now();
        Ok(())
    }

    pub async fn learn_from_interactions(
        &self,
        user_id: &str,
        interactions: &[String],
    ) -> Result<Vec<LearnedPreference>> {
        let learned = self.provider.infer_preferences(interactions).await?;

        let mut profiles = self.profiles.write().await;
        let profile = profiles
            .get_mut(user_id)
            .ok_or_else(|| AdaptError::UserNotFound(user_id.to_string()))?;

        for pref in &learned {
            // Update or add preference
            if let Some(existing) = profile
                .learning_profile
                .learned_preferences
                .iter_mut()
                .find(|p| p.category == pref.category && p.preference == pref.preference)
            {
                existing.confidence = (existing.confidence + pref.confidence) / 2.0;
                existing.evidence_count += pref.evidence_count;
                existing.last_observed = pref.last_observed;
            } else {
                profile
                    .learning_profile
                    .learned_preferences
                    .push(pref.clone());
            }
        }

        profile.updated_at = Self::now();
        Ok(learned)
    }

    pub async fn set_skill_level(
        &self,
        user_id: &str,
        skill: &str,
        level: SkillLevel,
    ) -> Result<()> {
        let mut profiles = self.profiles.write().await;
        let profile = profiles
            .get_mut(user_id)
            .ok_or_else(|| AdaptError::UserNotFound(user_id.to_string()))?;

        profile
            .learning_profile
            .skill_levels
            .insert(skill.to_string(), level);
        profile.updated_at = Self::now();
        Ok(())
    }

    // Context Management
    async fn get_or_create_context(&self, user_id: &str) -> Result<AdaptationContext> {
        let contexts = self.session_contexts.read().await;
        if let Some(ctx) = contexts.get(user_id) {
            return Ok(ctx.clone());
        }
        drop(contexts);

        let context = AdaptationContext {
            user_id: user_id.to_string(),
            current_task: None,
            time_of_day: "day".to_string(),
            device_type: None,
            session_length: 0,
            recent_feedback: vec![],
        };

        let mut contexts = self.session_contexts.write().await;
        contexts.insert(user_id.to_string(), context.clone());

        Ok(context)
    }

    pub async fn update_context(
        &self,
        user_id: &str,
        task: Option<String>,
        device: Option<String>,
    ) -> Result<()> {
        let mut contexts = self.session_contexts.write().await;

        if let Some(ctx) = contexts.get_mut(user_id) {
            ctx.current_task = task;
            ctx.device_type = device;
        } else {
            contexts.insert(
                user_id.to_string(),
                AdaptationContext {
                    user_id: user_id.to_string(),
                    current_task: task,
                    time_of_day: "day".to_string(),
                    device_type: device,
                    session_length: 0,
                    recent_feedback: vec![],
                },
            );
        }

        Ok(())
    }

    // Queries
    pub async fn get_effective_settings(&self, user_id: &str) -> Result<EffectiveSettings> {
        let profile = self.get_or_create_profile(user_id).await?;

        Ok(EffectiveSettings {
            verbosity: profile.preferences.verbosity,
            formality: profile.preferences.formality,
            response_style: profile.preferences.response_style,
            reading_level: profile.accessibility.cognitive.reading_level,
            use_simple_language: profile.accessibility.cognitive.simple_language,
            screen_reader_mode: profile.accessibility.visual.screen_reader,
            preferred_formats: profile.preferences.preferred_formats.clone(),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectiveSettings {
    pub verbosity: Verbosity,
    pub formality: Formality,
    pub response_style: ResponseStyle,
    pub reading_level: ReadingLevel,
    pub use_simple_language: bool,
    pub screen_reader_mode: bool,
    pub preferred_formats: Vec<ContentFormat>,
}

impl Default for UserPreferences {
    fn default() -> Self {
        Self {
            language: "en".to_string(),
            timezone: "UTC".to_string(),
            response_style: ResponseStyle::Balanced,
            verbosity: Verbosity::Standard,
            formality: Formality::Neutral,
            humor_level: HumorLevel::Subtle,
            expertise_areas: vec![],
            interests: vec![],
            avoided_topics: vec![],
            preferred_formats: vec![ContentFormat::Text],
            notification_preferences: NotificationPreferences {
                enabled: true,
                quiet_hours: None,
                channels: vec![],
                urgency_threshold: UrgencyThreshold::Important,
            },
            custom: HashMap::new(),
        }
    }
}

impl Default for AccessibilitySettings {
    fn default() -> Self {
        Self {
            visual: VisualAccessibility {
                screen_reader: false,
                high_contrast: false,
                large_text: false,
                text_size_multiplier: 1.0,
                reduce_motion: false,
                alt_text_preference: AltTextPreference::Standard,
            },
            auditory: AuditoryAccessibility {
                captions: false,
                transcripts: false,
                visual_alerts: false,
                mono_audio: false,
            },
            motor: MotorAccessibility {
                keyboard_only: false,
                voice_control: false,
                extended_timeouts: false,
                simplified_interactions: false,
            },
            cognitive: CognitiveAccessibility {
                simple_language: false,
                reading_level: ReadingLevel::College,
                chunked_content: false,
                summary_first: false,
                progress_indicators: false,
                consistent_layout: true,
            },
            custom_accommodations: vec![],
        }
    }
}

impl Default for LearningProfile {
    fn default() -> Self {
        Self {
            learned_preferences: vec![],
            behavior_patterns: vec![],
            skill_levels: HashMap::new(),
            feedback_history: vec![],
            adaptation_confidence: 0.0,
        }
    }
}

impl Default for InteractionHistory {
    fn default() -> Self {
        Self {
            total_interactions: 0,
            recent_topics: vec![],
            common_requests: vec![],
            session_patterns: vec![],
            satisfaction_trend: 0.5,
        }
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
    impl AdaptProvider for MockProvider {
        async fn adapt_response(
            &self,
            content: &str,
            profile: &UserProfile,
            _context: &AdaptationContext,
        ) -> Result<AdaptedResponse> {
            let adapted = match profile.preferences.verbosity {
                Verbosity::Minimal | Verbosity::Brief => {
                    format!("Brief: {}", &content[..content.len().min(50)])
                }
                Verbosity::Detailed | Verbosity::Comprehensive => {
                    format!("Detailed: {} [with more explanation]", content)
                }
                _ => content.to_string(),
            };

            Ok(AdaptedResponse {
                original: content.to_string(),
                adapted,
                adaptations_applied: vec![AdaptationType::Verbosity],
                accessibility_adjustments: vec![],
                confidence: 0.9,
            })
        }

        async fn infer_preferences(
            &self,
            _interactions: &[String],
        ) -> Result<Vec<LearnedPreference>> {
            Ok(vec![LearnedPreference {
                category: "response_style".to_string(),
                preference: "concise".to_string(),
                confidence: 0.8,
                evidence_count: 5,
                last_observed: 0,
            }])
        }

        async fn suggest_accessibility(
            &self,
            _behavior: &[String],
        ) -> Result<AccessibilitySettings> {
            Ok(AccessibilitySettings::default())
        }

        async fn simplify_for_reading_level(
            &self,
            text: &str,
            level: ReadingLevel,
        ) -> Result<String> {
            let simplified = match level {
                ReadingLevel::Elementary => text.split('.').next().unwrap_or(text).to_string(),
                ReadingLevel::MiddleSchool | ReadingLevel::HighSchool => text.to_string(),
                _ => text.to_string(),
            };
            Ok(simplified)
        }
    }

    #[tokio::test]
    async fn test_profile_creation() {
        let provider = Arc::new(MockProvider);
        let engine = AdaptEngine::new(provider);

        let profile = engine.create_profile("user-1").await.unwrap();
        assert_eq!(profile.id, "user-1");
        assert_eq!(profile.preferences.verbosity, Verbosity::Standard);
    }

    #[tokio::test]
    async fn test_preference_updates() {
        let provider = Arc::new(MockProvider);
        let engine = AdaptEngine::new(provider);

        engine.create_profile("user-1").await.unwrap();

        engine
            .set_verbosity("user-1", Verbosity::Brief)
            .await
            .unwrap();
        engine
            .set_formality("user-1", Formality::Casual)
            .await
            .unwrap();

        let profile = engine.get_profile("user-1").await.unwrap();
        assert_eq!(profile.preferences.verbosity, Verbosity::Brief);
        assert_eq!(profile.preferences.formality, Formality::Casual);
    }

    #[tokio::test]
    async fn test_interests() {
        let provider = Arc::new(MockProvider);
        let engine = AdaptEngine::new(provider);

        engine.create_profile("user-1").await.unwrap();

        engine.add_interest("user-1", "programming").await.unwrap();
        engine.add_interest("user-1", "music").await.unwrap();
        engine.add_interest("user-1", "programming").await.unwrap(); // Duplicate

        let profile = engine.get_profile("user-1").await.unwrap();
        assert_eq!(profile.preferences.interests.len(), 2);
    }

    #[tokio::test]
    async fn test_adaptation() {
        let provider = Arc::new(MockProvider);
        let engine = AdaptEngine::new(provider);

        engine.create_profile("user-1").await.unwrap();
        engine
            .set_verbosity("user-1", Verbosity::Brief)
            .await
            .unwrap();

        let adapted = engine
            .adapt_for_user(
                "user-1",
                "This is a long response that should be shortened.",
            )
            .await
            .unwrap();
        assert!(adapted.adapted.starts_with("Brief:"));
    }

    #[tokio::test]
    async fn test_accessibility() {
        let provider = Arc::new(MockProvider);
        let engine = AdaptEngine::new(provider);

        engine.create_profile("user-1").await.unwrap();

        engine.enable_screen_reader("user-1", true).await.unwrap();
        engine
            .set_reading_level("user-1", ReadingLevel::HighSchool)
            .await
            .unwrap();

        let profile = engine.get_profile("user-1").await.unwrap();
        assert!(profile.accessibility.visual.screen_reader);
        assert_eq!(
            profile.accessibility.cognitive.reading_level,
            ReadingLevel::HighSchool
        );
    }

    #[tokio::test]
    async fn test_learning() {
        let provider = Arc::new(MockProvider);
        let engine = AdaptEngine::new(provider);

        engine.create_profile("user-1").await.unwrap();

        let learned = engine
            .learn_from_interactions(
                "user-1",
                &[
                    "Be brief please".to_string(),
                    "Just the summary".to_string(),
                ],
            )
            .await
            .unwrap();

        assert!(!learned.is_empty());

        let profile = engine.get_profile("user-1").await.unwrap();
        assert!(!profile.learning_profile.learned_preferences.is_empty());
    }

    #[tokio::test]
    async fn test_skill_levels() {
        let provider = Arc::new(MockProvider);
        let engine = AdaptEngine::new(provider);

        engine.create_profile("user-1").await.unwrap();

        engine
            .set_skill_level("user-1", "rust", SkillLevel::Advanced)
            .await
            .unwrap();
        engine
            .set_skill_level("user-1", "python", SkillLevel::Intermediate)
            .await
            .unwrap();

        let profile = engine.get_profile("user-1").await.unwrap();
        assert_eq!(
            profile.learning_profile.skill_levels.get("rust"),
            Some(&SkillLevel::Advanced)
        );
    }

    #[tokio::test]
    async fn test_effective_settings() {
        let provider = Arc::new(MockProvider);
        let engine = AdaptEngine::new(provider);

        engine.create_profile("user-1").await.unwrap();
        engine
            .set_verbosity("user-1", Verbosity::Detailed)
            .await
            .unwrap();
        engine.enable_screen_reader("user-1", true).await.unwrap();

        let settings = engine.get_effective_settings("user-1").await.unwrap();
        assert_eq!(settings.verbosity, Verbosity::Detailed);
        assert!(settings.screen_reader_mode);
    }
}
