//! Dynamic personality adaptation and contextual voice.
//!
//! This crate provides persona capabilities:
//! - Adapt communication style to context
//! - Maintain consistent personality traits
//! - Adjust formality and tone
//! - Remember user preferences

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Persona errors.
#[derive(Debug, Error)]
pub enum PersonaError {
    #[error("Persona not found: {0}")]
    PersonaNotFound(String),

    #[error("Adaptation failed: {0}")]
    AdaptationFailed(String),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("Provider error: {0}")]
    ProviderError(String),
}

/// Result type for persona operations.
pub type Result<T> = std::result::Result<T, PersonaError>;

/// A persona configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Persona {
    /// Persona identifier.
    pub id: String,
    /// Persona name.
    pub name: String,
    /// Base personality traits.
    pub traits: PersonalityTraits,
    /// Communication style.
    pub style: CommunicationStyle,
    /// Voice characteristics.
    pub voice: VoiceCharacteristics,
    /// Behavioral guidelines.
    pub guidelines: Vec<Guideline>,
    /// Created timestamp.
    pub created_at: DateTime<Utc>,
}

/// Personality traits.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonalityTraits {
    /// Warmth (0.0-1.0): cold to warm.
    pub warmth: f64,
    /// Formality (0.0-1.0): casual to formal.
    pub formality: f64,
    /// Humor (0.0-1.0): serious to playful.
    pub humor: f64,
    /// Directness (0.0-1.0): indirect to direct.
    pub directness: f64,
    /// Empathy (0.0-1.0): detached to empathetic.
    pub empathy: f64,
    /// Confidence (0.0-1.0): tentative to confident.
    pub confidence: f64,
    /// Verbosity (0.0-1.0): concise to verbose.
    pub verbosity: f64,
}

impl Default for PersonalityTraits {
    fn default() -> Self {
        Self {
            warmth: 0.7,
            formality: 0.5,
            humor: 0.3,
            directness: 0.6,
            empathy: 0.7,
            confidence: 0.7,
            verbosity: 0.5,
        }
    }
}

/// Communication style settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommunicationStyle {
    /// Preferred greeting.
    pub greeting: GreetingStyle,
    /// Use of examples.
    pub use_examples: bool,
    /// Use of analogies.
    pub use_analogies: bool,
    /// Question handling.
    pub question_style: QuestionStyle,
    /// Error communication.
    pub error_style: ErrorStyle,
    /// Transition phrases.
    pub use_transitions: bool,
}

impl Default for CommunicationStyle {
    fn default() -> Self {
        Self {
            greeting: GreetingStyle::Friendly,
            use_examples: true,
            use_analogies: true,
            question_style: QuestionStyle::Clarifying,
            error_style: ErrorStyle::Constructive,
            use_transitions: true,
        }
    }
}

/// Greeting styles.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum GreetingStyle {
    None,
    Minimal,
    Friendly,
    Enthusiastic,
    Professional,
}

/// Question handling styles.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum QuestionStyle {
    /// Ask clarifying questions.
    Clarifying,
    /// Proceed with best guess.
    Assumptive,
    /// Ask for confirmation.
    Confirmatory,
}

/// Error communication styles.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ErrorStyle {
    /// Focus on the positive.
    Constructive,
    /// Direct and factual.
    Direct,
    /// Apologetic.
    Apologetic,
}

/// Voice characteristics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceCharacteristics {
    /// Sentence length preference.
    pub sentence_length: SentenceLength,
    /// Vocabulary level.
    pub vocabulary: VocabularyLevel,
    /// Use of contractions.
    pub contractions: bool,
    /// Use of emojis.
    pub emojis: EmojiUsage,
    /// Punctuation style.
    pub punctuation: PunctuationStyle,
}

impl Default for VoiceCharacteristics {
    fn default() -> Self {
        Self {
            sentence_length: SentenceLength::Mixed,
            vocabulary: VocabularyLevel::Moderate,
            contractions: true,
            emojis: EmojiUsage::Minimal,
            punctuation: PunctuationStyle::Standard,
        }
    }
}

/// Sentence length preferences.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum SentenceLength {
    Short,
    Mixed,
    Long,
}

/// Vocabulary levels.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum VocabularyLevel {
    Simple,
    Moderate,
    Advanced,
    Technical,
}

/// Emoji usage.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum EmojiUsage {
    None,
    Minimal,
    Moderate,
    Frequent,
}

/// Punctuation styles.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum PunctuationStyle {
    Minimal,
    Standard,
    Expressive,
}

/// A behavioral guideline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Guideline {
    /// Guideline name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Priority (1-10).
    pub priority: u8,
    /// Applicable contexts.
    pub contexts: Vec<String>,
}

/// Context for persona adaptation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Context {
    /// User preferences.
    pub user_preferences: Option<UserPreferences>,
    /// Conversation history summary.
    pub conversation_mood: Option<Mood>,
    /// Topic being discussed.
    pub topic: Option<String>,
    /// Urgency level.
    pub urgency: Option<Urgency>,
    /// User expertise.
    pub user_expertise: Option<Expertise>,
    /// Time of day.
    pub time_of_day: Option<TimeOfDay>,
    /// Custom context.
    pub custom: HashMap<String, serde_json::Value>,
}

/// User preferences for communication.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserPreferences {
    /// Preferred formality.
    pub formality: Option<f64>,
    /// Preferred verbosity.
    pub verbosity: Option<f64>,
    /// Use of humor.
    pub humor: Option<bool>,
    /// Specific requests.
    pub requests: Vec<String>,
}

/// Conversation mood.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Mood {
    Positive,
    Neutral,
    Negative,
    Urgent,
    Frustrated,
    Curious,
}

/// Urgency levels.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Urgency {
    Low,
    Medium,
    High,
    Critical,
}

/// User expertise levels.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Expertise {
    Beginner,
    Intermediate,
    Advanced,
    Expert,
}

/// Time of day.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum TimeOfDay {
    Morning,
    Afternoon,
    Evening,
    Night,
}

/// Adapted response parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdaptedParams {
    /// Adjusted traits.
    pub traits: PersonalityTraits,
    /// Adjusted style.
    pub style: CommunicationStyle,
    /// Adjusted voice.
    pub voice: VoiceCharacteristics,
    /// Specific instructions.
    pub instructions: Vec<String>,
}

/// Provider for persona adaptation.
#[async_trait]
pub trait PersonaProvider: Send + Sync {
    /// Adapt persona to context.
    async fn adapt(&self, persona: &Persona, context: &Context) -> Result<AdaptedParams>;

    /// Analyze user communication style.
    async fn analyze_user_style(&self, messages: &[String]) -> Result<UserPreferences>;

    /// Transform text to match persona.
    async fn transform(&self, text: &str, params: &AdaptedParams) -> Result<String>;
}

/// The persona engine.
pub struct PersonaEngine {
    /// Provider for adaptation.
    provider: Arc<dyn PersonaProvider>,
    /// Registered personas.
    personas: Arc<RwLock<HashMap<String, Persona>>>,
    /// Active persona per user.
    active_personas: Arc<RwLock<HashMap<String, String>>>,
    /// User style profiles.
    user_styles: Arc<RwLock<HashMap<String, UserPreferences>>>,
}

impl PersonaEngine {
    /// Create a new persona engine.
    pub fn new(provider: Arc<dyn PersonaProvider>) -> Self {
        Self {
            provider,
            personas: Arc::new(RwLock::new(HashMap::new())),
            active_personas: Arc::new(RwLock::new(HashMap::new())),
            user_styles: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a persona.
    pub async fn register_persona(&self, persona: Persona) -> Result<String> {
        let id = persona.id.clone();
        let mut personas = self.personas.write().await;
        personas.insert(id.clone(), persona);
        Ok(id)
    }

    /// Create the default persona.
    pub async fn create_default_persona(&self) -> Persona {
        Persona {
            id: "default".to_string(),
            name: "Default Assistant".to_string(),
            traits: PersonalityTraits::default(),
            style: CommunicationStyle::default(),
            voice: VoiceCharacteristics::default(),
            guidelines: vec![
                Guideline {
                    name: "be_helpful".to_string(),
                    description: "Always aim to be helpful and constructive".to_string(),
                    priority: 10,
                    contexts: vec![],
                },
                Guideline {
                    name: "be_honest".to_string(),
                    description: "Be honest about limitations and uncertainties".to_string(),
                    priority: 10,
                    contexts: vec![],
                },
            ],
            created_at: Utc::now(),
        }
    }

    /// Set active persona for a user.
    pub async fn set_active(&self, user_id: &str, persona_id: &str) -> Result<()> {
        let personas = self.personas.read().await;
        if !personas.contains_key(persona_id) {
            return Err(PersonaError::PersonaNotFound(persona_id.to_string()));
        }
        drop(personas);

        let mut active = self.active_personas.write().await;
        active.insert(user_id.to_string(), persona_id.to_string());
        Ok(())
    }

    /// Get adapted parameters for context.
    pub async fn adapt(&self, user_id: &str, context: Context) -> Result<AdaptedParams> {
        let persona_id = {
            let active = self.active_personas.read().await;
            active
                .get(user_id)
                .cloned()
                .unwrap_or_else(|| "default".to_string())
        };

        let personas = self.personas.read().await;
        let persona = personas.get(&persona_id).cloned().unwrap_or_else(|| {
            // Return default if not found
            Persona {
                id: "default".to_string(),
                name: "Default".to_string(),
                traits: PersonalityTraits::default(),
                style: CommunicationStyle::default(),
                voice: VoiceCharacteristics::default(),
                guidelines: vec![],
                created_at: Utc::now(),
            }
        });
        drop(personas);

        self.provider.adapt(&persona, &context).await
    }

    /// Transform text to match persona.
    pub async fn transform(&self, user_id: &str, text: &str, context: Context) -> Result<String> {
        let params = self.adapt(user_id, context).await?;
        self.provider.transform(text, &params).await
    }

    /// Learn user's communication style.
    pub async fn learn_user_style(&self, user_id: &str, messages: &[String]) -> Result<()> {
        let prefs = self.provider.analyze_user_style(messages).await?;

        let mut styles = self.user_styles.write().await;
        styles.insert(user_id.to_string(), prefs);

        Ok(())
    }

    /// Get user's learned preferences.
    pub async fn get_user_preferences(&self, user_id: &str) -> Option<UserPreferences> {
        let styles = self.user_styles.read().await;
        styles.get(user_id).cloned()
    }

    /// Get all personas.
    pub async fn list_personas(&self) -> Vec<Persona> {
        let personas = self.personas.read().await;
        personas.values().cloned().collect()
    }
}

/// Builder for personas.
pub struct PersonaBuilder {
    persona: Persona,
}

impl PersonaBuilder {
    /// Create a new persona builder.
    pub fn new(name: &str) -> Self {
        Self {
            persona: Persona {
                id: Uuid::new_v4().to_string(),
                name: name.to_string(),
                traits: PersonalityTraits::default(),
                style: CommunicationStyle::default(),
                voice: VoiceCharacteristics::default(),
                guidelines: Vec::new(),
                created_at: Utc::now(),
            },
        }
    }

    /// Set warmth.
    pub fn warmth(mut self, value: f64) -> Self {
        self.persona.traits.warmth = value.clamp(0.0, 1.0);
        self
    }

    /// Set formality.
    pub fn formality(mut self, value: f64) -> Self {
        self.persona.traits.formality = value.clamp(0.0, 1.0);
        self
    }

    /// Set humor.
    pub fn humor(mut self, value: f64) -> Self {
        self.persona.traits.humor = value.clamp(0.0, 1.0);
        self
    }

    /// Set vocabulary level.
    pub fn vocabulary(mut self, level: VocabularyLevel) -> Self {
        self.persona.voice.vocabulary = level;
        self
    }

    /// Add a guideline.
    pub fn guideline(mut self, name: &str, description: &str, priority: u8) -> Self {
        self.persona.guidelines.push(Guideline {
            name: name.to_string(),
            description: description.to_string(),
            priority,
            contexts: vec![],
        });
        self
    }

    /// Build the persona.
    pub fn build(self) -> Persona {
        self.persona
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockProvider;

    #[async_trait]
    impl PersonaProvider for MockProvider {
        async fn adapt(&self, persona: &Persona, context: &Context) -> Result<AdaptedParams> {
            let mut traits = persona.traits.clone();

            // Adapt based on context
            if let Some(mood) = context.conversation_mood {
                match mood {
                    Mood::Frustrated => {
                        traits.empathy = (traits.empathy + 0.2).min(1.0);
                        traits.warmth = (traits.warmth + 0.2).min(1.0);
                    }
                    Mood::Urgent => {
                        traits.directness = (traits.directness + 0.2).min(1.0);
                        traits.verbosity = (traits.verbosity - 0.2).max(0.0);
                    }
                    _ => {}
                }
            }

            Ok(AdaptedParams {
                traits,
                style: persona.style.clone(),
                voice: persona.voice.clone(),
                instructions: vec![],
            })
        }

        async fn analyze_user_style(&self, _messages: &[String]) -> Result<UserPreferences> {
            Ok(UserPreferences {
                formality: Some(0.5),
                verbosity: Some(0.5),
                humor: Some(true),
                requests: vec![],
            })
        }

        async fn transform(&self, text: &str, _params: &AdaptedParams) -> Result<String> {
            Ok(text.to_string())
        }
    }

    #[tokio::test]
    async fn test_register_persona() {
        let provider = Arc::new(MockProvider);
        let engine = PersonaEngine::new(provider);

        let persona = PersonaBuilder::new("Friendly Assistant")
            .warmth(0.9)
            .humor(0.5)
            .build();

        let id = engine.register_persona(persona).await.unwrap();
        assert!(!id.is_empty());
    }

    #[tokio::test]
    async fn test_adapt_to_context() {
        let provider = Arc::new(MockProvider);
        let engine = PersonaEngine::new(provider);

        let persona = engine.create_default_persona().await;
        engine.register_persona(persona).await.unwrap();

        let context = Context {
            user_preferences: None,
            conversation_mood: Some(Mood::Frustrated),
            topic: None,
            urgency: None,
            user_expertise: None,
            time_of_day: None,
            custom: HashMap::new(),
        };

        let params = engine.adapt("user1", context).await.unwrap();
        // Empathy should be increased for frustrated user
        assert!(params.traits.empathy >= 0.7);
    }

    #[tokio::test]
    async fn test_learn_user_style() {
        let provider = Arc::new(MockProvider);
        let engine = PersonaEngine::new(provider);

        let messages = vec!["Hello!".to_string(), "Thanks for helping".to_string()];
        engine.learn_user_style("user1", &messages).await.unwrap();

        let prefs = engine.get_user_preferences("user1").await.unwrap();
        assert!(prefs.formality.is_some());
    }

    #[test]
    fn test_persona_builder() {
        let persona = PersonaBuilder::new("Test")
            .warmth(0.8)
            .formality(0.3)
            .humor(0.6)
            .vocabulary(VocabularyLevel::Simple)
            .guideline("be_brief", "Keep responses short", 8)
            .build();

        assert_eq!(persona.name, "Test");
        assert_eq!(persona.traits.warmth, 0.8);
        assert_eq!(persona.guidelines.len(), 1);
    }

    #[test]
    fn test_default_traits() {
        let traits = PersonalityTraits::default();
        assert!(traits.warmth > 0.0);
        assert!(traits.empathy > 0.0);
    }
}
