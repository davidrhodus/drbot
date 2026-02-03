//! Deep personalization for drbot.
//!
//! Learn user preferences and adapt behavior.
//!
//! # Features
//!
//! - Communication style learning
//! - Preference tracking
//! - Behavioral adaptation
//! - Context awareness
//! - Personalized responses

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Personalization result type.
pub type Result<T> = std::result::Result<T, PersonalizeError>;

/// Personalization errors.
#[derive(Debug, thiserror::Error)]
pub enum PersonalizeError {
    #[error("User profile not found: {0}")]
    ProfileNotFound(String),
    #[error("Preference not set: {0}")]
    PreferenceNotFound(String),
    #[error("Learning failed: {0}")]
    LearningFailed(String),
}

/// User profile for personalization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfile {
    /// User ID.
    pub user_id: String,
    /// Display name.
    pub name: Option<String>,
    /// Communication style.
    pub style: CommunicationStyle,
    /// Preferences.
    pub preferences: HashMap<String, PreferenceValue>,
    /// Topics of interest.
    pub interests: Vec<Interest>,
    /// Expertise levels.
    pub expertise: HashMap<String, ExpertiseLevel>,
    /// Interaction history summary.
    pub history: InteractionHistory,
    /// Created at.
    pub created_at: DateTime<Utc>,
    /// Last updated.
    pub updated_at: DateTime<Utc>,
}

impl UserProfile {
    /// Create a new user profile.
    pub fn new(user_id: &str) -> Self {
        let now = Utc::now();
        Self {
            user_id: user_id.to_string(),
            name: None,
            style: CommunicationStyle::default(),
            preferences: HashMap::new(),
            interests: Vec::new(),
            expertise: HashMap::new(),
            history: InteractionHistory::default(),
            created_at: now,
            updated_at: now,
        }
    }

    /// Set user name.
    pub fn with_name(mut self, name: &str) -> Self {
        self.name = Some(name.to_string());
        self
    }

    /// Update a preference.
    pub fn set_preference(&mut self, key: &str, value: PreferenceValue) {
        self.preferences.insert(key.to_string(), value);
        self.updated_at = Utc::now();
    }

    /// Get a preference.
    pub fn get_preference(&self, key: &str) -> Option<&PreferenceValue> {
        self.preferences.get(key)
    }

    /// Add an interest.
    pub fn add_interest(&mut self, interest: Interest) {
        if !self.interests.iter().any(|i| i.topic == interest.topic) {
            self.interests.push(interest);
        }
        self.updated_at = Utc::now();
    }

    /// Set expertise level.
    pub fn set_expertise(&mut self, domain: &str, level: ExpertiseLevel) {
        self.expertise.insert(domain.to_string(), level);
        self.updated_at = Utc::now();
    }
}

/// Communication style preferences.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommunicationStyle {
    /// Verbosity level (0-1, low to high).
    pub verbosity: f32,
    /// Formality level (0-1, casual to formal).
    pub formality: f32,
    /// Technical depth (0-1, simple to complex).
    pub technical_depth: f32,
    /// Emoji usage (0-1, none to frequent).
    pub emoji_usage: f32,
    /// Preferred response length.
    pub preferred_length: ResponseLength,
    /// Humor preference.
    pub humor: HumorStyle,
    /// Language preference.
    pub language: String,
}

impl Default for CommunicationStyle {
    fn default() -> Self {
        Self {
            verbosity: 0.5,
            formality: 0.5,
            technical_depth: 0.5,
            emoji_usage: 0.2,
            preferred_length: ResponseLength::Medium,
            humor: HumorStyle::Occasional,
            language: "en".to_string(),
        }
    }
}

/// Preferred response length.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResponseLength {
    Brief,
    Medium,
    Detailed,
    Comprehensive,
}

/// Humor style preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HumorStyle {
    None,
    Occasional,
    Frequent,
    Witty,
}

/// Preference value types.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PreferenceValue {
    Bool(bool),
    Number(f64),
    String(String),
    List(Vec<String>),
    Map(HashMap<String, String>),
}

/// Topic of interest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Interest {
    /// Topic name.
    pub topic: String,
    /// Interest level (0-1).
    pub level: f32,
    /// How often this topic comes up.
    pub frequency: u32,
    /// Last discussed.
    pub last_discussed: Option<DateTime<Utc>>,
}

impl Interest {
    /// Create a new interest.
    pub fn new(topic: &str) -> Self {
        Self {
            topic: topic.to_string(),
            level: 0.5,
            frequency: 1,
            last_discussed: Some(Utc::now()),
        }
    }

    /// Record a discussion of this topic.
    pub fn record_discussion(&mut self) {
        self.frequency += 1;
        self.last_discussed = Some(Utc::now());
        // Increase interest level with frequency
        self.level = (self.level + 0.1).min(1.0);
    }
}

/// Expertise level in a domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExpertiseLevel {
    Beginner,
    Intermediate,
    Advanced,
    Expert,
}

/// Interaction history summary.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InteractionHistory {
    /// Total interactions.
    pub total_interactions: u64,
    /// Average message length.
    pub avg_message_length: f32,
    /// Most active hours.
    pub active_hours: Vec<u8>,
    /// Common topics.
    pub common_topics: Vec<String>,
    /// Positive feedback count.
    pub positive_feedback: u32,
    /// Negative feedback count.
    pub negative_feedback: u32,
}

/// Learning event for adaptation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningEvent {
    /// Event ID.
    pub id: Uuid,
    /// Event type.
    pub event_type: LearningEventType,
    /// User message.
    pub user_input: String,
    /// Assistant response.
    pub response: String,
    /// User feedback (if any).
    pub feedback: Option<Feedback>,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
}

/// Learning event types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LearningEventType {
    Conversation,
    Correction,
    Preference,
    Feedback,
}

/// User feedback.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Feedback {
    /// Rating (-1 to 1).
    pub rating: f32,
    /// Feedback text.
    pub text: Option<String>,
    /// Specific aspects.
    pub aspects: HashMap<String, f32>,
}

/// Personalization configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonalizeConfig {
    /// Enable learning.
    pub learning_enabled: bool,
    /// Learning rate (0-1).
    pub learning_rate: f32,
    /// Minimum interactions before adapting.
    pub min_interactions: u32,
    /// Privacy level.
    pub privacy_level: PrivacyLevel,
    /// Auto-detect preferences.
    pub auto_detect: bool,
}

impl Default for PersonalizeConfig {
    fn default() -> Self {
        Self {
            learning_enabled: true,
            learning_rate: 0.1,
            min_interactions: 5,
            privacy_level: PrivacyLevel::Standard,
            auto_detect: true,
        }
    }
}

/// Privacy level for personalization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrivacyLevel {
    /// No personal data stored.
    Minimal,
    /// Basic preferences only.
    Standard,
    /// Full personalization.
    Full,
}

/// Personalization engine.
pub struct PersonalizationEngine {
    config: PersonalizeConfig,
    profiles: Arc<RwLock<HashMap<String, UserProfile>>>,
    events: Arc<RwLock<Vec<LearningEvent>>>,
}

impl PersonalizationEngine {
    /// Create a new personalization engine.
    pub fn new(config: PersonalizeConfig) -> Self {
        Self {
            config,
            profiles: Arc::new(RwLock::new(HashMap::new())),
            events: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Get or create a user profile.
    pub async fn get_profile(&self, user_id: &str) -> UserProfile {
        let profiles = self.profiles.read().await;
        profiles
            .get(user_id)
            .cloned()
            .unwrap_or_else(|| UserProfile::new(user_id))
    }

    /// Save a user profile.
    pub async fn save_profile(&self, profile: UserProfile) {
        self.profiles
            .write()
            .await
            .insert(profile.user_id.clone(), profile);
    }

    /// Record a learning event.
    pub async fn record_event(&self, user_id: &str, event: LearningEvent) {
        if !self.config.learning_enabled {
            return;
        }

        self.events.write().await.push(event.clone());

        // Update profile based on event
        let mut profile = self.get_profile(user_id).await;
        self.update_profile_from_event(&mut profile, &event).await;
        self.save_profile(profile).await;
    }

    /// Update profile from a learning event.
    async fn update_profile_from_event(&self, profile: &mut UserProfile, event: &LearningEvent) {
        // Update interaction count
        profile.history.total_interactions += 1;

        // Update average message length
        let msg_len = event.user_input.len() as f32;
        let n = profile.history.total_interactions as f32;
        profile.history.avg_message_length =
            (profile.history.avg_message_length * (n - 1.0) + msg_len) / n;

        // Update active hours
        let hour = event
            .timestamp
            .format("%H")
            .to_string()
            .parse::<u8>()
            .unwrap_or(0);
        if !profile.history.active_hours.contains(&hour) {
            profile.history.active_hours.push(hour);
        }

        // Process feedback
        if let Some(feedback) = &event.feedback {
            if feedback.rating > 0.0 {
                profile.history.positive_feedback += 1;
            } else if feedback.rating < 0.0 {
                profile.history.negative_feedback += 1;
            }

            // Adjust style based on feedback aspects
            if let Some(verbosity) = feedback.aspects.get("verbosity") {
                profile.style.verbosity = self.adjust_value(profile.style.verbosity, *verbosity);
            }
            if let Some(formality) = feedback.aspects.get("formality") {
                profile.style.formality = self.adjust_value(profile.style.formality, *formality);
            }
        }

        // Auto-detect preferences
        if self.config.auto_detect {
            self.auto_detect_style(profile, &event.user_input);
        }

        profile.updated_at = Utc::now();
    }

    /// Adjust a value based on feedback.
    fn adjust_value(&self, current: f32, feedback: f32) -> f32 {
        let delta = feedback * self.config.learning_rate;
        (current + delta).clamp(0.0, 1.0)
    }

    /// Auto-detect communication style from input.
    fn auto_detect_style(&self, profile: &mut UserProfile, input: &str) {
        // Detect formality
        let informal_indicators = ["hey", "hi", "lol", "yeah", "nope", "gonna", "wanna"];
        let formal_indicators = ["please", "kindly", "would you", "could you", "thank you"];

        let informal_count = informal_indicators
            .iter()
            .filter(|w| input.to_lowercase().contains(*w))
            .count();
        let formal_count = formal_indicators
            .iter()
            .filter(|w| input.to_lowercase().contains(*w))
            .count();

        if informal_count > formal_count {
            profile.style.formality = self.adjust_value(profile.style.formality, -0.1);
        } else if formal_count > informal_count {
            profile.style.formality = self.adjust_value(profile.style.formality, 0.1);
        }

        // Detect emoji usage
        let emoji_count = input.chars().filter(|c| c.is_emoji()).count();
        if emoji_count > 0 {
            profile.style.emoji_usage = self.adjust_value(profile.style.emoji_usage, 0.05);
        }

        // Detect technical depth from vocabulary
        let technical_words = [
            "function",
            "algorithm",
            "api",
            "database",
            "async",
            "protocol",
        ];
        let tech_count = technical_words
            .iter()
            .filter(|w| input.to_lowercase().contains(*w))
            .count();
        if tech_count > 0 {
            profile.style.technical_depth = self.adjust_value(profile.style.technical_depth, 0.05);
        }
    }

    /// Generate personalized system prompt additions.
    pub async fn personalized_prompt(&self, user_id: &str) -> String {
        let profile = self.get_profile(user_id).await;

        let mut instructions: Vec<String> = Vec::new();

        // Communication style instructions
        if profile.style.verbosity < 0.3 {
            instructions.push("Be concise and brief.".to_string());
        } else if profile.style.verbosity > 0.7 {
            instructions.push("Provide detailed explanations.".to_string());
        }

        if profile.style.formality < 0.3 {
            instructions.push("Use a casual, friendly tone.".to_string());
        } else if profile.style.formality > 0.7 {
            instructions.push("Use a professional, formal tone.".to_string());
        }

        if profile.style.technical_depth > 0.7 {
            instructions
                .push("The user has technical expertise. Use technical terminology.".to_string());
        } else if profile.style.technical_depth < 0.3 {
            instructions.push("Explain technical concepts in simple terms.".to_string());
        }

        if profile.style.emoji_usage > 0.5 {
            instructions.push("Feel free to use emojis occasionally.".to_string());
        } else {
            instructions.push("Avoid using emojis.".to_string());
        }

        // Expertise instructions
        for (domain, level) in &profile.expertise {
            let level_str = match level {
                ExpertiseLevel::Beginner => "beginner",
                ExpertiseLevel::Intermediate => "intermediate",
                ExpertiseLevel::Advanced => "advanced",
                ExpertiseLevel::Expert => "expert",
            };
            instructions.push(format!("User has {} level in {}.", level_str, domain));
        }

        // Interest instructions
        if !profile.interests.is_empty() {
            let topics: Vec<_> = profile
                .interests
                .iter()
                .filter(|i| i.level > 0.5)
                .map(|i| i.topic.clone())
                .take(3)
                .collect();
            if !topics.is_empty() {
                instructions.push(format!("User is interested in: {}.", topics.join(", ")));
            }
        }

        if let Some(name) = &profile.name {
            instructions.push(format!("The user's name is {}.", name));
        }

        instructions.join(" ")
    }

    /// Get personalization suggestions.
    pub async fn get_suggestions(&self, user_id: &str) -> Vec<PersonalizationSuggestion> {
        let profile = self.get_profile(user_id).await;
        let mut suggestions = Vec::new();

        // Suggest based on interaction patterns
        if profile.history.total_interactions > 10 && profile.name.is_none() {
            suggestions.push(PersonalizationSuggestion {
                id: Uuid::new_v4(),
                suggestion_type: SuggestionType::SetPreference,
                message: "Would you like to tell me your name for more personalized interactions?"
                    .to_string(),
                action: Some("set_name".to_string()),
            });
        }

        // Suggest expertise adjustment
        if profile.history.total_interactions > 20 && profile.expertise.is_empty() {
            suggestions.push(PersonalizationSuggestion {
                id: Uuid::new_v4(),
                suggestion_type: SuggestionType::AdjustExpertise,
                message:
                    "I can adjust my explanations to your expertise level. What's your background?"
                        .to_string(),
                action: Some("set_expertise".to_string()),
            });
        }

        suggestions
    }
}

/// Personalization suggestion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonalizationSuggestion {
    /// Suggestion ID.
    pub id: Uuid,
    /// Suggestion type.
    pub suggestion_type: SuggestionType,
    /// Suggestion message.
    pub message: String,
    /// Action to take.
    pub action: Option<String>,
}

/// Suggestion types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuggestionType {
    SetPreference,
    AdjustExpertise,
    ChangeStyle,
    AddInterest,
}

/// Helper trait to check if char is emoji.
trait IsEmoji {
    fn is_emoji(&self) -> bool;
}

impl IsEmoji for char {
    fn is_emoji(&self) -> bool {
        matches!(self, '\u{1F600}'..='\u{1F64F}' |
                       '\u{1F300}'..='\u{1F5FF}' |
                       '\u{1F680}'..='\u{1F6FF}' |
                       '\u{2600}'..='\u{26FF}' |
                       '\u{2700}'..='\u{27BF}')
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_user_profile() {
        let mut profile = UserProfile::new("user-123").with_name("Alice");

        profile.set_preference("theme", PreferenceValue::String("dark".to_string()));
        profile.set_expertise("rust", ExpertiseLevel::Advanced);
        profile.add_interest(Interest::new("AI"));

        assert_eq!(profile.name, Some("Alice".to_string()));
        assert!(profile.preferences.contains_key("theme"));
        assert_eq!(
            profile.expertise.get("rust"),
            Some(&ExpertiseLevel::Advanced)
        );
    }

    #[tokio::test]
    async fn test_personalization_engine() {
        let engine = PersonalizationEngine::new(PersonalizeConfig::default());

        let mut profile = UserProfile::new("user-123");
        profile.style.formality = 0.8;
        profile.style.technical_depth = 0.9;
        engine.save_profile(profile).await;

        let prompt = engine.personalized_prompt("user-123").await;
        assert!(prompt.contains("professional"));
        assert!(prompt.contains("technical"));
    }

    #[test]
    fn test_communication_style() {
        let style = CommunicationStyle::default();
        assert_eq!(style.verbosity, 0.5);
        assert_eq!(style.preferred_length, ResponseLength::Medium);
    }
}
