//! Social intelligence for drbot
//!
//! Understands communication styles, relationship context, and team dynamics.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;

#[derive(Error, Debug)]
pub enum SocialError {
    #[error("Person not found: {0}")]
    PersonNotFound(String),
    #[error("Analysis failed: {0}")]
    AnalysisFailed(String),
    #[error("Provider error: {0}")]
    ProviderError(String),
}

pub type Result<T> = std::result::Result<T, SocialError>;

// ============================================================================
// Communication Styles
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommunicationStyle {
    pub formality: FormalityLevel,
    pub directness: DirectnessLevel,
    pub detail_preference: DetailPreference,
    pub response_time_expectation: ResponseTimeExpectation,
    pub preferred_channels: Vec<String>,
    pub emoji_usage: EmojiUsage,
    pub greeting_style: GreetingStyle,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum FormalityLevel {
    VeryFormal,
    Formal,
    Neutral,
    Casual,
    VeryCasual,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum DirectnessLevel {
    VeryDirect,
    Direct,
    Neutral,
    Indirect,
    VeryIndirect,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum DetailPreference {
    Comprehensive,
    Detailed,
    Balanced,
    Concise,
    Minimal,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum ResponseTimeExpectation {
    Immediate,
    Quick,
    Normal,
    Flexible,
    NoExpectation,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum EmojiUsage {
    Frequent,
    Moderate,
    Occasional,
    Rare,
    Never,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum GreetingStyle {
    Formal,
    Friendly,
    Brief,
    None,
}

// ============================================================================
// Relationship Context
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Person {
    pub id: String,
    pub name: String,
    pub aliases: Vec<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub organization: Option<String>,
    pub role: Option<String>,
    pub relationship: RelationshipType,
    pub communication_style: Option<CommunicationStyle>,
    pub notes: Vec<String>,
    pub tags: Vec<String>,
    pub last_interaction: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RelationshipType {
    Family,
    Friend,
    Colleague,
    Manager,
    DirectReport,
    Client,
    Vendor,
    Acquaintance,
    Professional,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Interaction {
    pub id: String,
    pub person_id: String,
    pub timestamp: u64,
    pub channel: String,
    pub interaction_type: InteractionType,
    pub sentiment: Sentiment,
    pub topics: Vec<String>,
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum InteractionType {
    Message,
    Call,
    Meeting,
    Email,
    InPerson,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum Sentiment {
    VeryPositive,
    Positive,
    Neutral,
    Negative,
    VeryNegative,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationshipInsight {
    pub person_id: String,
    pub interaction_frequency: InteractionFrequency,
    pub average_sentiment: f32,
    pub common_topics: Vec<String>,
    pub last_interaction: Option<u64>,
    pub suggested_followup: Option<String>,
    pub relationship_health: RelationshipHealth,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum InteractionFrequency {
    Daily,
    Weekly,
    Monthly,
    Quarterly,
    Yearly,
    Rare,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum RelationshipHealth {
    Excellent,
    Good,
    NeedsAttention,
    AtRisk,
    Unknown,
}

// ============================================================================
// Team Dynamics
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Team {
    pub id: String,
    pub name: String,
    pub members: Vec<TeamMember>,
    pub communication_norms: TeamNorms,
    pub active_projects: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamMember {
    pub person_id: String,
    pub role: TeamRole,
    pub expertise: Vec<String>,
    pub availability: Availability,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TeamRole {
    Leader,
    Manager,
    SeniorMember,
    Member,
    Contributor,
    Stakeholder,
    Custom(String),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum Availability {
    Available,
    Busy,
    Away,
    DoNotDisturb,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamNorms {
    pub meeting_days: Vec<String>,
    pub core_hours: Option<(String, String)>,
    pub preferred_channels: Vec<String>,
    pub decision_style: DecisionStyle,
    pub feedback_culture: FeedbackCulture,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum DecisionStyle {
    Consensus,
    Democratic,
    Consultative,
    Directive,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum FeedbackCulture {
    Direct,
    Constructive,
    Formal,
    Informal,
}

// ============================================================================
// Message Adaptation
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageContext {
    pub recipient: Option<String>,
    pub channel: Option<String>,
    pub team: Option<String>,
    pub purpose: MessagePurpose,
    pub urgency: Urgency,
    pub previous_context: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MessagePurpose {
    Inform,
    Request,
    Question,
    Feedback,
    Followup,
    Introduction,
    Gratitude,
    Apology,
    Reminder,
    Announcement,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum Urgency {
    Critical,
    High,
    Normal,
    Low,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdaptedMessage {
    pub original: String,
    pub adapted: String,
    pub adaptations_made: Vec<String>,
    pub tone: String,
    pub formality_level: FormalityLevel,
}

// ============================================================================
// Social Analysis
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationAnalysis {
    pub participants: Vec<String>,
    pub topics: Vec<String>,
    pub overall_sentiment: Sentiment,
    pub tension_points: Vec<TensionPoint>,
    pub action_items: Vec<ActionItem>,
    pub decisions: Vec<Decision>,
    pub unresolved_questions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TensionPoint {
    pub description: String,
    pub participants: Vec<String>,
    pub severity: TensionSeverity,
    pub suggested_resolution: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum TensionSeverity {
    Minor,
    Moderate,
    Significant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionItem {
    pub description: String,
    pub assignee: Option<String>,
    pub due_date: Option<String>,
    pub priority: Urgency,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decision {
    pub description: String,
    pub made_by: Vec<String>,
    pub timestamp: u64,
    pub implications: Vec<String>,
}

// ============================================================================
// Provider Trait
// ============================================================================

#[async_trait]
pub trait SocialProvider: Send + Sync {
    async fn analyze_communication_style(&self, messages: &[String]) -> Result<CommunicationStyle>;
    async fn adapt_message(
        &self,
        message: &str,
        context: &MessageContext,
        style: &CommunicationStyle,
    ) -> Result<AdaptedMessage>;
    async fn analyze_conversation(&self, messages: &[String]) -> Result<ConversationAnalysis>;
    async fn suggest_response(
        &self,
        context: &str,
        style: &CommunicationStyle,
    ) -> Result<Vec<String>>;
    async fn detect_sentiment(&self, text: &str) -> Result<Sentiment>;
}

// ============================================================================
// Social Engine
// ============================================================================

pub struct SocialEngine {
    provider: Arc<dyn SocialProvider>,
    people: Arc<RwLock<HashMap<String, Person>>>,
    teams: Arc<RwLock<HashMap<String, Team>>>,
    interactions: Arc<RwLock<Vec<Interaction>>>,
    style_cache: Arc<RwLock<HashMap<String, CommunicationStyle>>>,
}

impl SocialEngine {
    pub fn new(provider: Arc<dyn SocialProvider>) -> Self {
        Self {
            provider,
            people: Arc::new(RwLock::new(HashMap::new())),
            teams: Arc::new(RwLock::new(HashMap::new())),
            interactions: Arc::new(RwLock::new(Vec::new())),
            style_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn add_person(&self, person: Person) -> Result<()> {
        let mut people = self.people.write().await;
        people.insert(person.id.clone(), person);
        Ok(())
    }

    pub async fn get_person(&self, person_id: &str) -> Result<Person> {
        let people = self.people.read().await;
        people
            .get(person_id)
            .cloned()
            .ok_or_else(|| SocialError::PersonNotFound(person_id.to_string()))
    }

    pub async fn find_person_by_name(&self, name: &str) -> Result<Option<Person>> {
        let people = self.people.read().await;
        let name_lower = name.to_lowercase();

        for person in people.values() {
            if person.name.to_lowercase() == name_lower {
                return Ok(Some(person.clone()));
            }
            for alias in &person.aliases {
                if alias.to_lowercase() == name_lower {
                    return Ok(Some(person.clone()));
                }
            }
        }
        Ok(None)
    }

    pub async fn record_interaction(&self, interaction: Interaction) -> Result<()> {
        // Update last interaction for person
        {
            let mut people = self.people.write().await;
            if let Some(person) = people.get_mut(&interaction.person_id) {
                person.last_interaction = Some(interaction.timestamp);
            }
        }

        let mut interactions = self.interactions.write().await;
        interactions.push(interaction);
        Ok(())
    }

    pub async fn get_relationship_insight(&self, person_id: &str) -> Result<RelationshipInsight> {
        let interactions = self.interactions.read().await;
        let person_interactions: Vec<_> = interactions
            .iter()
            .filter(|i| i.person_id == person_id)
            .collect();

        if person_interactions.is_empty() {
            return Ok(RelationshipInsight {
                person_id: person_id.to_string(),
                interaction_frequency: InteractionFrequency::Rare,
                average_sentiment: 0.0,
                common_topics: vec![],
                last_interaction: None,
                suggested_followup: Some("Consider reaching out to reconnect".to_string()),
                relationship_health: RelationshipHealth::Unknown,
            });
        }

        // Calculate frequency
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let oldest = person_interactions
            .iter()
            .map(|i| i.timestamp)
            .min()
            .unwrap();
        let days_span = ((now - oldest) / 86400).max(1) as f32;
        let interactions_per_day = person_interactions.len() as f32 / days_span;

        let frequency = if interactions_per_day >= 1.0 {
            InteractionFrequency::Daily
        } else if interactions_per_day >= 0.14 {
            InteractionFrequency::Weekly
        } else if interactions_per_day >= 0.033 {
            InteractionFrequency::Monthly
        } else if interactions_per_day >= 0.008 {
            InteractionFrequency::Quarterly
        } else {
            InteractionFrequency::Rare
        };

        // Calculate average sentiment
        let sentiment_sum: f32 = person_interactions
            .iter()
            .map(|i| match i.sentiment {
                Sentiment::VeryPositive => 2.0,
                Sentiment::Positive => 1.0,
                Sentiment::Neutral => 0.0,
                Sentiment::Negative => -1.0,
                Sentiment::VeryNegative => -2.0,
            })
            .sum();
        let avg_sentiment = sentiment_sum / person_interactions.len() as f32;

        // Collect common topics
        let mut topic_counts: HashMap<String, usize> = HashMap::new();
        for interaction in &person_interactions {
            for topic in &interaction.topics {
                *topic_counts.entry(topic.clone()).or_insert(0) += 1;
            }
        }
        let mut topics: Vec<_> = topic_counts.into_iter().collect();
        topics.sort_by(|a, b| b.1.cmp(&a.1));
        let common_topics: Vec<String> = topics.into_iter().take(5).map(|(t, _)| t).collect();

        let last = person_interactions.iter().map(|i| i.timestamp).max();

        let health = if avg_sentiment > 0.5
            && matches!(
                frequency,
                InteractionFrequency::Daily | InteractionFrequency::Weekly
            ) {
            RelationshipHealth::Excellent
        } else if avg_sentiment > 0.0 {
            RelationshipHealth::Good
        } else if avg_sentiment > -0.5 {
            RelationshipHealth::NeedsAttention
        } else {
            RelationshipHealth::AtRisk
        };

        Ok(RelationshipInsight {
            person_id: person_id.to_string(),
            interaction_frequency: frequency,
            average_sentiment: avg_sentiment,
            common_topics,
            last_interaction: last,
            suggested_followup: None,
            relationship_health: health,
        })
    }

    pub async fn learn_communication_style(
        &self,
        person_id: &str,
        messages: &[String],
    ) -> Result<CommunicationStyle> {
        let style = self.provider.analyze_communication_style(messages).await?;

        // Cache the learned style
        {
            let mut cache = self.style_cache.write().await;
            cache.insert(person_id.to_string(), style.clone());
        }

        // Update person record
        {
            let mut people = self.people.write().await;
            if let Some(person) = people.get_mut(person_id) {
                person.communication_style = Some(style.clone());
            }
        }

        Ok(style)
    }

    pub async fn get_communication_style(
        &self,
        person_id: &str,
    ) -> Result<Option<CommunicationStyle>> {
        // Check cache first
        {
            let cache = self.style_cache.read().await;
            if let Some(style) = cache.get(person_id) {
                return Ok(Some(style.clone()));
            }
        }

        // Check person record
        let people = self.people.read().await;
        if let Some(person) = people.get(person_id) {
            return Ok(person.communication_style.clone());
        }

        Ok(None)
    }

    pub async fn adapt_message_for_recipient(
        &self,
        message: &str,
        recipient_id: &str,
        purpose: MessagePurpose,
    ) -> Result<AdaptedMessage> {
        let style =
            self.get_communication_style(recipient_id)
                .await?
                .unwrap_or(CommunicationStyle {
                    formality: FormalityLevel::Neutral,
                    directness: DirectnessLevel::Neutral,
                    detail_preference: DetailPreference::Balanced,
                    response_time_expectation: ResponseTimeExpectation::Normal,
                    preferred_channels: vec![],
                    emoji_usage: EmojiUsage::Occasional,
                    greeting_style: GreetingStyle::Friendly,
                });

        let context = MessageContext {
            recipient: Some(recipient_id.to_string()),
            channel: None,
            team: None,
            purpose,
            urgency: Urgency::Normal,
            previous_context: None,
        };

        self.provider.adapt_message(message, &context, &style).await
    }

    pub async fn add_team(&self, team: Team) -> Result<()> {
        let mut teams = self.teams.write().await;
        teams.insert(team.id.clone(), team);
        Ok(())
    }

    pub async fn get_team(&self, team_id: &str) -> Result<Team> {
        let teams = self.teams.read().await;
        teams
            .get(team_id)
            .cloned()
            .ok_or_else(|| SocialError::PersonNotFound(format!("Team {}", team_id)))
    }

    pub async fn analyze_team_conversation(
        &self,
        team_id: &str,
        messages: &[String],
    ) -> Result<ConversationAnalysis> {
        let _team = self.get_team(team_id).await?;
        self.provider.analyze_conversation(messages).await
    }

    pub async fn suggest_responses(
        &self,
        context: &str,
        recipient_id: Option<&str>,
    ) -> Result<Vec<String>> {
        let style = if let Some(id) = recipient_id {
            self.get_communication_style(id).await?.unwrap_or_default()
        } else {
            CommunicationStyle::default()
        };

        self.provider.suggest_response(context, &style).await
    }

    pub async fn get_followup_suggestions(&self) -> Result<Vec<FollowupSuggestion>> {
        let people = self.people.read().await;
        let mut suggestions = Vec::new();

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        for person in people.values() {
            if let Some(last) = person.last_interaction {
                let days_since = (now - last) / 86400;

                let should_followup = match person.relationship {
                    RelationshipType::Family => days_since > 7,
                    RelationshipType::Friend => days_since > 14,
                    RelationshipType::Colleague | RelationshipType::Manager => days_since > 30,
                    RelationshipType::Client => days_since > 21,
                    _ => days_since > 60,
                };

                if should_followup {
                    suggestions.push(FollowupSuggestion {
                        person_id: person.id.clone(),
                        person_name: person.name.clone(),
                        days_since_contact: days_since as u32,
                        relationship: person.relationship.clone(),
                        suggested_action: format!(
                            "It's been {} days since you last connected with {}",
                            days_since, person.name
                        ),
                    });
                }
            }
        }

        suggestions.sort_by(|a, b| b.days_since_contact.cmp(&a.days_since_contact));
        Ok(suggestions)
    }
}

impl Default for CommunicationStyle {
    fn default() -> Self {
        Self {
            formality: FormalityLevel::Neutral,
            directness: DirectnessLevel::Neutral,
            detail_preference: DetailPreference::Balanced,
            response_time_expectation: ResponseTimeExpectation::Normal,
            preferred_channels: vec![],
            emoji_usage: EmojiUsage::Occasional,
            greeting_style: GreetingStyle::Friendly,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FollowupSuggestion {
    pub person_id: String,
    pub person_name: String,
    pub days_since_contact: u32,
    pub relationship: RelationshipType,
    pub suggested_action: String,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    struct MockProvider;

    #[async_trait]
    impl SocialProvider for MockProvider {
        async fn analyze_communication_style(
            &self,
            _messages: &[String],
        ) -> Result<CommunicationStyle> {
            Ok(CommunicationStyle {
                formality: FormalityLevel::Casual,
                directness: DirectnessLevel::Direct,
                detail_preference: DetailPreference::Concise,
                response_time_expectation: ResponseTimeExpectation::Quick,
                preferred_channels: vec!["slack".to_string()],
                emoji_usage: EmojiUsage::Moderate,
                greeting_style: GreetingStyle::Brief,
            })
        }

        async fn adapt_message(
            &self,
            message: &str,
            _context: &MessageContext,
            style: &CommunicationStyle,
        ) -> Result<AdaptedMessage> {
            let adapted = match style.formality {
                FormalityLevel::VeryFormal | FormalityLevel::Formal => {
                    format!("Dear colleague, {}", message)
                }
                FormalityLevel::Casual | FormalityLevel::VeryCasual => format!("Hey! {}", message),
                FormalityLevel::Neutral => message.to_string(),
            };

            Ok(AdaptedMessage {
                original: message.to_string(),
                adapted,
                adaptations_made: vec!["Adjusted greeting".to_string()],
                tone: "friendly".to_string(),
                formality_level: style.formality,
            })
        }

        async fn analyze_conversation(&self, _messages: &[String]) -> Result<ConversationAnalysis> {
            Ok(ConversationAnalysis {
                participants: vec!["Alice".to_string(), "Bob".to_string()],
                topics: vec!["project timeline".to_string()],
                overall_sentiment: Sentiment::Positive,
                tension_points: vec![],
                action_items: vec![ActionItem {
                    description: "Review proposal".to_string(),
                    assignee: Some("Bob".to_string()),
                    due_date: Some("2024-01-15".to_string()),
                    priority: Urgency::Normal,
                }],
                decisions: vec![],
                unresolved_questions: vec![],
            })
        }

        async fn suggest_response(
            &self,
            _context: &str,
            _style: &CommunicationStyle,
        ) -> Result<Vec<String>> {
            Ok(vec![
                "Sounds good, I'll take a look!".to_string(),
                "Thanks for sharing, let me review this.".to_string(),
            ])
        }

        async fn detect_sentiment(&self, _text: &str) -> Result<Sentiment> {
            Ok(Sentiment::Positive)
        }
    }

    #[tokio::test]
    async fn test_person_management() {
        let provider = Arc::new(MockProvider);
        let engine = SocialEngine::new(provider);

        let person = Person {
            id: "alice".to_string(),
            name: "Alice Smith".to_string(),
            aliases: vec!["Ali".to_string()],
            email: Some("alice@example.com".to_string()),
            phone: None,
            organization: Some("Acme Corp".to_string()),
            role: Some("Engineer".to_string()),
            relationship: RelationshipType::Colleague,
            communication_style: None,
            notes: vec![],
            tags: vec!["team-a".to_string()],
            last_interaction: None,
        };

        engine.add_person(person).await.unwrap();

        let found = engine.find_person_by_name("Alice Smith").await.unwrap();
        assert!(found.is_some());

        let found_alias = engine.find_person_by_name("Ali").await.unwrap();
        assert!(found_alias.is_some());
    }

    #[tokio::test]
    async fn test_communication_style_learning() {
        let provider = Arc::new(MockProvider);
        let engine = SocialEngine::new(provider);

        let person = Person {
            id: "bob".to_string(),
            name: "Bob".to_string(),
            aliases: vec![],
            email: None,
            phone: None,
            organization: None,
            role: None,
            relationship: RelationshipType::Friend,
            communication_style: None,
            notes: vec![],
            tags: vec![],
            last_interaction: None,
        };

        engine.add_person(person).await.unwrap();

        let messages = vec![
            "Hey! How's it going?".to_string(),
            "lol that's hilarious".to_string(),
        ];

        let style = engine
            .learn_communication_style("bob", &messages)
            .await
            .unwrap();
        assert_eq!(style.formality, FormalityLevel::Casual);
        assert_eq!(style.directness, DirectnessLevel::Direct);
    }

    #[tokio::test]
    async fn test_message_adaptation() {
        let provider = Arc::new(MockProvider);
        let engine = SocialEngine::new(provider);

        let person = Person {
            id: "formal-person".to_string(),
            name: "Dr. Formal".to_string(),
            aliases: vec![],
            email: None,
            phone: None,
            organization: None,
            role: None,
            relationship: RelationshipType::Professional,
            communication_style: Some(CommunicationStyle {
                formality: FormalityLevel::Formal,
                directness: DirectnessLevel::Direct,
                detail_preference: DetailPreference::Detailed,
                response_time_expectation: ResponseTimeExpectation::Normal,
                preferred_channels: vec!["email".to_string()],
                emoji_usage: EmojiUsage::Never,
                greeting_style: GreetingStyle::Formal,
            }),
            notes: vec![],
            tags: vec![],
            last_interaction: None,
        };

        engine.add_person(person).await.unwrap();

        let adapted = engine
            .adapt_message_for_recipient(
                "Can we meet tomorrow?",
                "formal-person",
                MessagePurpose::Request,
            )
            .await
            .unwrap();

        assert!(adapted.adapted.contains("Dear colleague"));
    }

    #[tokio::test]
    async fn test_interaction_tracking() {
        let provider = Arc::new(MockProvider);
        let engine = SocialEngine::new(provider);

        let person = Person {
            id: "charlie".to_string(),
            name: "Charlie".to_string(),
            aliases: vec![],
            email: None,
            phone: None,
            organization: None,
            role: None,
            relationship: RelationshipType::Friend,
            communication_style: None,
            notes: vec![],
            tags: vec![],
            last_interaction: None,
        };

        engine.add_person(person).await.unwrap();

        let interaction = Interaction {
            id: "int-1".to_string(),
            person_id: "charlie".to_string(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            channel: "slack".to_string(),
            interaction_type: InteractionType::Message,
            sentiment: Sentiment::Positive,
            topics: vec!["weekend plans".to_string()],
            summary: None,
        };

        engine.record_interaction(interaction).await.unwrap();

        let insight = engine.get_relationship_insight("charlie").await.unwrap();
        assert!(insight.last_interaction.is_some());
    }

    #[tokio::test]
    async fn test_team_management() {
        let provider = Arc::new(MockProvider);
        let engine = SocialEngine::new(provider);

        let team = Team {
            id: "team-alpha".to_string(),
            name: "Team Alpha".to_string(),
            members: vec![TeamMember {
                person_id: "alice".to_string(),
                role: TeamRole::Leader,
                expertise: vec!["rust".to_string()],
                availability: Availability::Available,
            }],
            communication_norms: TeamNorms {
                meeting_days: vec!["Monday".to_string(), "Thursday".to_string()],
                core_hours: Some(("10:00".to_string(), "16:00".to_string())),
                preferred_channels: vec!["slack".to_string()],
                decision_style: DecisionStyle::Consensus,
                feedback_culture: FeedbackCulture::Direct,
            },
            active_projects: vec!["Project X".to_string()],
        };

        engine.add_team(team).await.unwrap();

        let retrieved = engine.get_team("team-alpha").await.unwrap();
        assert_eq!(retrieved.name, "Team Alpha");
        assert_eq!(retrieved.members.len(), 1);
    }
}
