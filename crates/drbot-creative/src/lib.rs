//! Creative assistance system for drbot
//!
//! Brainstorming, writing assistance, and presentation creation.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;

#[derive(Error, Debug)]
pub enum CreativeError {
    #[error("Generation failed: {0}")]
    GenerationFailed(String),
    #[error("Invalid input: {0}")]
    InvalidInput(String),
    #[error("Style not supported: {0}")]
    UnsupportedStyle(String),
    #[error("Provider error: {0}")]
    ProviderError(String),
}

pub type Result<T> = std::result::Result<T, CreativeError>;

// ============================================================================
// Brainstorming
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrainstormRequest {
    pub topic: String,
    pub context: Option<String>,
    pub constraints: Vec<String>,
    pub num_ideas: usize,
    pub technique: BrainstormTechnique,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BrainstormTechnique {
    FreeAssociation,
    MindMapping,
    SixThinkingHats,
    Scamper,
    ReverseThinking,
    RandomStimulus,
    Analogy,
    WhatIf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrainstormResult {
    pub topic: String,
    pub technique_used: BrainstormTechnique,
    pub ideas: Vec<Idea>,
    pub clusters: Vec<IdeaCluster>,
    pub next_steps: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Idea {
    pub id: String,
    pub title: String,
    pub description: String,
    pub category: Option<String>,
    pub novelty_score: f32,
    pub feasibility_score: f32,
    pub related_ideas: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdeaCluster {
    pub name: String,
    pub theme: String,
    pub idea_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MindMap {
    pub central_topic: String,
    pub nodes: Vec<MindMapNode>,
    pub connections: Vec<MindMapConnection>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MindMapNode {
    pub id: String,
    pub content: String,
    pub level: u32,
    pub parent_id: Option<String>,
    pub color: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MindMapConnection {
    pub from_id: String,
    pub to_id: String,
    pub relationship: Option<String>,
}

// ============================================================================
// Writing Assistance
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WritingRequest {
    pub content_type: ContentType,
    pub topic: String,
    pub context: Option<String>,
    pub style: WritingStyle,
    pub tone: WritingTone,
    pub length: ContentLength,
    pub audience: Option<String>,
    pub keywords: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ContentType {
    Article,
    BlogPost,
    Email,
    Report,
    Proposal,
    Story,
    Poem,
    Script,
    SocialPost,
    ProductDescription,
    Review,
    Summary,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum WritingStyle {
    Formal,
    Casual,
    Academic,
    Creative,
    Technical,
    Journalistic,
    Conversational,
    Persuasive,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum WritingTone {
    Professional,
    Friendly,
    Authoritative,
    Humorous,
    Inspirational,
    Empathetic,
    Urgent,
    Neutral,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ContentLength {
    Brief,    // < 100 words
    Short,    // 100-300 words
    Medium,   // 300-800 words
    Long,     // 800-2000 words
    Extended, // > 2000 words
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WritingResult {
    pub content: String,
    pub word_count: usize,
    pub sections: Vec<ContentSection>,
    pub suggestions: Vec<WritingSuggestion>,
    pub alternative_titles: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentSection {
    pub heading: Option<String>,
    pub content: String,
    pub section_type: SectionType,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SectionType {
    Introduction,
    Body,
    Conclusion,
    CallToAction,
    Quote,
    List,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WritingSuggestion {
    pub suggestion_type: SuggestionType,
    pub original: Option<String>,
    pub suggested: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SuggestionType {
    Grammar,
    Style,
    Clarity,
    Conciseness,
    WordChoice,
    Structure,
    Tone,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditRequest {
    pub content: String,
    pub edit_type: EditType,
    pub preserve_meaning: bool,
    pub target_style: Option<WritingStyle>,
    pub target_tone: Option<WritingTone>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EditType {
    Proofread,
    Improve,
    Simplify,
    Expand,
    Condense,
    Rewrite,
    ChangeStyle,
    ChangeTone,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditResult {
    pub original: String,
    pub edited: String,
    pub changes: Vec<EditChange>,
    pub improvement_score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditChange {
    pub change_type: SuggestionType,
    pub original: String,
    pub replacement: String,
    pub explanation: String,
}

// ============================================================================
// Presentation
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresentationRequest {
    pub topic: String,
    pub context: Option<String>,
    pub audience: String,
    pub duration_minutes: u32,
    pub style: PresentationStyle,
    pub include_notes: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PresentationStyle {
    Business,
    Educational,
    TechTalk,
    Sales,
    Keynote,
    Workshop,
    Informal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Presentation {
    pub title: String,
    pub subtitle: Option<String>,
    pub slides: Vec<Slide>,
    pub estimated_duration: u32,
    pub key_messages: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Slide {
    pub slide_number: u32,
    pub title: String,
    pub content: SlideContent,
    pub speaker_notes: Option<String>,
    pub transition: Option<String>,
    pub duration_seconds: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SlideContent {
    TitleSlide {
        subtitle: Option<String>,
    },
    BulletPoints {
        points: Vec<String>,
    },
    TwoColumn {
        left: Vec<String>,
        right: Vec<String>,
    },
    Image {
        description: String,
        caption: Option<String>,
    },
    Quote {
        quote: String,
        attribution: String,
    },
    Chart {
        chart_type: String,
        description: String,
    },
    Comparison {
        items: Vec<ComparisonItem>,
    },
    Timeline {
        events: Vec<TimelineEvent>,
    },
    ThankYou {
        contact: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComparisonItem {
    pub name: String,
    pub pros: Vec<String>,
    pub cons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineEvent {
    pub date: String,
    pub event: String,
}

// ============================================================================
// Storytelling
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoryRequest {
    pub premise: String,
    pub genre: StoryGenre,
    pub length: ContentLength,
    pub characters: Vec<Character>,
    pub setting: Option<String>,
    pub themes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum StoryGenre {
    Fantasy,
    SciFi,
    Mystery,
    Romance,
    Thriller,
    Horror,
    Comedy,
    Drama,
    Adventure,
    Literary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Character {
    pub name: String,
    pub role: CharacterRole,
    pub traits: Vec<String>,
    pub motivation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CharacterRole {
    Protagonist,
    Antagonist,
    Supporting,
    Minor,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Story {
    pub title: String,
    pub content: String,
    pub word_count: usize,
    pub chapters: Vec<Chapter>,
    pub character_arcs: Vec<CharacterArc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chapter {
    pub number: u32,
    pub title: String,
    pub content: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterArc {
    pub character_name: String,
    pub starting_state: String,
    pub ending_state: String,
    pub key_moments: Vec<String>,
}

// ============================================================================
// Provider Trait
// ============================================================================

#[async_trait]
pub trait CreativeProvider: Send + Sync {
    // Brainstorming
    async fn brainstorm(&self, request: BrainstormRequest) -> Result<BrainstormResult>;
    async fn create_mind_map(&self, topic: &str, depth: u32) -> Result<MindMap>;
    async fn evaluate_ideas(&self, ideas: &[Idea]) -> Result<Vec<IdeaEvaluation>>;

    // Writing
    async fn generate_content(&self, request: WritingRequest) -> Result<WritingResult>;
    async fn edit_content(&self, request: EditRequest) -> Result<EditResult>;
    async fn generate_outline(&self, topic: &str, content_type: ContentType) -> Result<Outline>;

    // Presentations
    async fn create_presentation(&self, request: PresentationRequest) -> Result<Presentation>;
    async fn generate_slide(&self, topic: &str, content_type: &str) -> Result<Slide>;

    // Storytelling
    async fn write_story(&self, request: StoryRequest) -> Result<Story>;
    async fn continue_story(&self, story_so_far: &str, direction: &str) -> Result<String>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdeaEvaluation {
    pub idea_id: String,
    pub novelty: f32,
    pub feasibility: f32,
    pub impact: f32,
    pub overall_score: f32,
    pub feedback: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Outline {
    pub title: String,
    pub sections: Vec<OutlineSection>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutlineSection {
    pub heading: String,
    pub subpoints: Vec<String>,
    pub estimated_words: usize,
}

// ============================================================================
// Creative Engine
// ============================================================================

pub struct CreativeEngine {
    provider: Arc<dyn CreativeProvider>,
    idea_bank: Arc<RwLock<HashMap<String, Idea>>>,
    writing_history: Arc<RwLock<Vec<WritingResult>>>,
    presentations: Arc<RwLock<HashMap<String, Presentation>>>,
}

impl CreativeEngine {
    pub fn new(provider: Arc<dyn CreativeProvider>) -> Self {
        Self {
            provider,
            idea_bank: Arc::new(RwLock::new(HashMap::new())),
            writing_history: Arc::new(RwLock::new(Vec::new())),
            presentations: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    // Brainstorming
    pub async fn brainstorm(&self, topic: &str, num_ideas: usize) -> Result<BrainstormResult> {
        let request = BrainstormRequest {
            topic: topic.to_string(),
            context: None,
            constraints: vec![],
            num_ideas,
            technique: BrainstormTechnique::FreeAssociation,
        };

        let result = self.provider.brainstorm(request).await?;

        // Store ideas in bank
        let mut bank = self.idea_bank.write().await;
        for idea in &result.ideas {
            bank.insert(idea.id.clone(), idea.clone());
        }

        Ok(result)
    }

    pub async fn brainstorm_with_technique(
        &self,
        topic: &str,
        technique: BrainstormTechnique,
        constraints: Vec<String>,
    ) -> Result<BrainstormResult> {
        let request = BrainstormRequest {
            topic: topic.to_string(),
            context: None,
            constraints,
            num_ideas: 10,
            technique,
        };

        self.provider.brainstorm(request).await
    }

    pub async fn create_mind_map(&self, topic: &str, depth: u32) -> Result<MindMap> {
        self.provider.create_mind_map(topic, depth).await
    }

    pub async fn evaluate_ideas(&self) -> Result<Vec<IdeaEvaluation>> {
        let bank = self.idea_bank.read().await;
        let ideas: Vec<Idea> = bank.values().cloned().collect();

        if ideas.is_empty() {
            return Ok(vec![]);
        }

        self.provider.evaluate_ideas(&ideas).await
    }

    pub async fn get_top_ideas(&self, count: usize) -> Result<Vec<(Idea, IdeaEvaluation)>> {
        let evaluations = self.evaluate_ideas().await?;
        let bank = self.idea_bank.read().await;

        let mut results: Vec<_> = evaluations
            .into_iter()
            .filter_map(|eval| bank.get(&eval.idea_id).map(|idea| (idea.clone(), eval)))
            .collect();

        results.sort_by(|a, b| b.1.overall_score.partial_cmp(&a.1.overall_score).unwrap());
        results.truncate(count);

        Ok(results)
    }

    // Writing
    pub async fn write(
        &self,
        content_type: ContentType,
        topic: &str,
        style: WritingStyle,
        tone: WritingTone,
    ) -> Result<WritingResult> {
        let request = WritingRequest {
            content_type,
            topic: topic.to_string(),
            context: None,
            style,
            tone,
            length: ContentLength::Medium,
            audience: None,
            keywords: vec![],
        };

        let result = self.provider.generate_content(request).await?;

        let mut history = self.writing_history.write().await;
        history.push(result.clone());

        Ok(result)
    }

    pub async fn write_email(&self, topic: &str, tone: WritingTone) -> Result<WritingResult> {
        self.write(
            ContentType::Email,
            topic,
            WritingStyle::Conversational,
            tone,
        )
        .await
    }

    pub async fn write_blog_post(
        &self,
        topic: &str,
        keywords: Vec<String>,
    ) -> Result<WritingResult> {
        let request = WritingRequest {
            content_type: ContentType::BlogPost,
            topic: topic.to_string(),
            context: None,
            style: WritingStyle::Conversational,
            tone: WritingTone::Friendly,
            length: ContentLength::Long,
            audience: None,
            keywords,
        };

        self.provider.generate_content(request).await
    }

    pub async fn edit(&self, content: &str, edit_type: EditType) -> Result<EditResult> {
        let request = EditRequest {
            content: content.to_string(),
            edit_type,
            preserve_meaning: true,
            target_style: None,
            target_tone: None,
        };

        self.provider.edit_content(request).await
    }

    pub async fn proofread(&self, content: &str) -> Result<EditResult> {
        self.edit(content, EditType::Proofread).await
    }

    pub async fn simplify(&self, content: &str) -> Result<EditResult> {
        self.edit(content, EditType::Simplify).await
    }

    pub async fn create_outline(&self, topic: &str, content_type: ContentType) -> Result<Outline> {
        self.provider.generate_outline(topic, content_type).await
    }

    // Presentations
    pub async fn create_presentation(
        &self,
        topic: &str,
        audience: &str,
        minutes: u32,
        style: PresentationStyle,
    ) -> Result<Presentation> {
        let request = PresentationRequest {
            topic: topic.to_string(),
            context: None,
            audience: audience.to_string(),
            duration_minutes: minutes,
            style,
            include_notes: true,
        };

        let presentation = self.provider.create_presentation(request).await?;

        let mut presentations = self.presentations.write().await;
        presentations.insert(presentation.title.clone(), presentation.clone());

        Ok(presentation)
    }

    pub async fn get_presentation(&self, title: &str) -> Option<Presentation> {
        let presentations = self.presentations.read().await;
        presentations.get(title).cloned()
    }

    // Storytelling
    pub async fn write_story(
        &self,
        premise: &str,
        genre: StoryGenre,
        length: ContentLength,
    ) -> Result<Story> {
        let request = StoryRequest {
            premise: premise.to_string(),
            genre,
            length,
            characters: vec![],
            setting: None,
            themes: vec![],
        };

        self.provider.write_story(request).await
    }

    pub async fn continue_story(&self, story_so_far: &str, direction: &str) -> Result<String> {
        self.provider.continue_story(story_so_far, direction).await
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
    impl CreativeProvider for MockProvider {
        async fn brainstorm(&self, request: BrainstormRequest) -> Result<BrainstormResult> {
            let ideas: Vec<Idea> = (0..request.num_ideas)
                .map(|i| Idea {
                    id: format!("idea-{}", i),
                    title: format!("Idea {} for {}", i + 1, request.topic),
                    description: format!("Description for idea {}", i + 1),
                    category: Some("General".to_string()),
                    novelty_score: 0.7,
                    feasibility_score: 0.8,
                    related_ideas: vec![],
                })
                .collect();

            Ok(BrainstormResult {
                topic: request.topic,
                technique_used: request.technique,
                ideas,
                clusters: vec![],
                next_steps: vec!["Evaluate ideas".to_string()],
            })
        }

        async fn create_mind_map(&self, topic: &str, _depth: u32) -> Result<MindMap> {
            Ok(MindMap {
                central_topic: topic.to_string(),
                nodes: vec![
                    MindMapNode {
                        id: "center".to_string(),
                        content: topic.to_string(),
                        level: 0,
                        parent_id: None,
                        color: Some("#3498db".to_string()),
                    },
                    MindMapNode {
                        id: "branch1".to_string(),
                        content: "Branch 1".to_string(),
                        level: 1,
                        parent_id: Some("center".to_string()),
                        color: None,
                    },
                ],
                connections: vec![MindMapConnection {
                    from_id: "center".to_string(),
                    to_id: "branch1".to_string(),
                    relationship: None,
                }],
            })
        }

        async fn evaluate_ideas(&self, ideas: &[Idea]) -> Result<Vec<IdeaEvaluation>> {
            Ok(ideas
                .iter()
                .map(|idea| IdeaEvaluation {
                    idea_id: idea.id.clone(),
                    novelty: idea.novelty_score,
                    feasibility: idea.feasibility_score,
                    impact: 0.75,
                    overall_score: (idea.novelty_score + idea.feasibility_score + 0.75) / 3.0,
                    feedback: "Good idea with potential".to_string(),
                })
                .collect())
        }

        async fn generate_content(&self, request: WritingRequest) -> Result<WritingResult> {
            Ok(WritingResult {
                content: format!(
                    "Content about {} in {} style.",
                    request.topic,
                    format!("{:?}", request.style)
                ),
                word_count: 250,
                sections: vec![ContentSection {
                    heading: Some("Introduction".to_string()),
                    content: "Introduction content here.".to_string(),
                    section_type: SectionType::Introduction,
                }],
                suggestions: vec![],
                alternative_titles: vec!["Alternative Title".to_string()],
            })
        }

        async fn edit_content(&self, request: EditRequest) -> Result<EditResult> {
            Ok(EditResult {
                original: request.content.clone(),
                edited: format!("Edited: {}", request.content),
                changes: vec![EditChange {
                    change_type: SuggestionType::Clarity,
                    original: "example".to_string(),
                    replacement: "edited example".to_string(),
                    explanation: "Improved clarity".to_string(),
                }],
                improvement_score: 0.85,
            })
        }

        async fn generate_outline(
            &self,
            topic: &str,
            _content_type: ContentType,
        ) -> Result<Outline> {
            Ok(Outline {
                title: topic.to_string(),
                sections: vec![
                    OutlineSection {
                        heading: "Introduction".to_string(),
                        subpoints: vec!["Hook".to_string(), "Thesis".to_string()],
                        estimated_words: 100,
                    },
                    OutlineSection {
                        heading: "Main Body".to_string(),
                        subpoints: vec!["Point 1".to_string(), "Point 2".to_string()],
                        estimated_words: 400,
                    },
                ],
            })
        }

        async fn create_presentation(&self, request: PresentationRequest) -> Result<Presentation> {
            Ok(Presentation {
                title: request.topic.clone(),
                subtitle: None,
                slides: vec![
                    Slide {
                        slide_number: 1,
                        title: request.topic.clone(),
                        content: SlideContent::TitleSlide {
                            subtitle: Some("Subtitle".to_string()),
                        },
                        speaker_notes: Some("Welcome the audience".to_string()),
                        transition: None,
                        duration_seconds: 30,
                    },
                    Slide {
                        slide_number: 2,
                        title: "Overview".to_string(),
                        content: SlideContent::BulletPoints {
                            points: vec!["Point 1".to_string(), "Point 2".to_string()],
                        },
                        speaker_notes: None,
                        transition: None,
                        duration_seconds: 60,
                    },
                ],
                estimated_duration: request.duration_minutes,
                key_messages: vec!["Key message 1".to_string()],
            })
        }

        async fn generate_slide(&self, topic: &str, _content_type: &str) -> Result<Slide> {
            Ok(Slide {
                slide_number: 1,
                title: topic.to_string(),
                content: SlideContent::BulletPoints {
                    points: vec!["Point 1".to_string()],
                },
                speaker_notes: None,
                transition: None,
                duration_seconds: 60,
            })
        }

        async fn write_story(&self, request: StoryRequest) -> Result<Story> {
            Ok(Story {
                title: format!("A {} Story", format!("{:?}", request.genre)),
                content: format!("Once upon a time, {}...", request.premise),
                word_count: 500,
                chapters: vec![Chapter {
                    number: 1,
                    title: "The Beginning".to_string(),
                    content: "Chapter content here.".to_string(),
                    summary: "The story begins.".to_string(),
                }],
                character_arcs: vec![],
            })
        }

        async fn continue_story(&self, _story_so_far: &str, direction: &str) -> Result<String> {
            Ok(format!(
                "The story continued, following the direction: {}...",
                direction
            ))
        }
    }

    #[tokio::test]
    async fn test_brainstorm() {
        let provider = Arc::new(MockProvider);
        let engine = CreativeEngine::new(provider);

        let result = engine.brainstorm("New product ideas", 5).await.unwrap();
        assert_eq!(result.ideas.len(), 5);
        assert_eq!(result.topic, "New product ideas");
    }

    #[tokio::test]
    async fn test_mind_map() {
        let provider = Arc::new(MockProvider);
        let engine = CreativeEngine::new(provider);

        let map = engine.create_mind_map("Project Planning", 3).await.unwrap();
        assert_eq!(map.central_topic, "Project Planning");
        assert!(!map.nodes.is_empty());
    }

    #[tokio::test]
    async fn test_idea_evaluation() {
        let provider = Arc::new(MockProvider);
        let engine = CreativeEngine::new(provider);

        // First brainstorm to populate idea bank
        engine.brainstorm("Test topic", 3).await.unwrap();

        let evaluations = engine.evaluate_ideas().await.unwrap();
        assert_eq!(evaluations.len(), 3);

        let top = engine.get_top_ideas(2).await.unwrap();
        assert_eq!(top.len(), 2);
    }

    #[tokio::test]
    async fn test_writing() {
        let provider = Arc::new(MockProvider);
        let engine = CreativeEngine::new(provider);

        let result = engine
            .write(
                ContentType::Article,
                "AI in Healthcare",
                WritingStyle::Formal,
                WritingTone::Professional,
            )
            .await
            .unwrap();

        assert!(result.word_count > 0);
        assert!(!result.content.is_empty());
    }

    #[tokio::test]
    async fn test_editing() {
        let provider = Arc::new(MockProvider);
        let engine = CreativeEngine::new(provider);

        let result = engine.proofread("Some text to proofread.").await.unwrap();
        assert!(!result.edited.is_empty());
        assert!(result.improvement_score > 0.0);
    }

    #[tokio::test]
    async fn test_presentation() {
        let provider = Arc::new(MockProvider);
        let engine = CreativeEngine::new(provider);

        let presentation = engine
            .create_presentation(
                "Quarterly Review",
                "Executives",
                15,
                PresentationStyle::Business,
            )
            .await
            .unwrap();

        assert_eq!(presentation.title, "Quarterly Review");
        assert!(!presentation.slides.is_empty());

        let retrieved = engine.get_presentation("Quarterly Review").await;
        assert!(retrieved.is_some());
    }

    #[tokio::test]
    async fn test_storytelling() {
        let provider = Arc::new(MockProvider);
        let engine = CreativeEngine::new(provider);

        let story = engine
            .write_story(
                "A robot discovers emotions",
                StoryGenre::SciFi,
                ContentLength::Short,
            )
            .await
            .unwrap();

        assert!(!story.title.is_empty());
        assert!(story.word_count > 0);
    }
}
