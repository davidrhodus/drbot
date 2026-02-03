//! Learn and match user's writing/coding style.
//!
//! This crate provides:
//! - Style analysis
//! - Style matching
//! - Preference learning
//! - Output adaptation

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Style errors.
#[derive(Debug, Error)]
pub enum StyleError {
    #[error("Analysis failed: {0}")]
    AnalysisFailed(String),

    #[error("Insufficient samples: {0}")]
    InsufficientSamples(String),

    #[error("Profile not found: {0}")]
    ProfileNotFound(String),
}

/// Result type for style operations.
pub type Result<T> = std::result::Result<T, StyleError>;

/// Style profile for a user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StyleProfile {
    /// Profile identifier.
    pub id: String,
    /// User identifier.
    pub user_id: String,
    /// Writing style.
    pub writing_style: WritingStyle,
    /// Coding style.
    pub coding_style: CodingStyle,
    /// Communication preferences.
    pub communication_prefs: CommunicationPrefs,
    /// Sample count.
    pub sample_count: usize,
    /// Last updated.
    pub updated_at: DateTime<Utc>,
    /// Confidence.
    pub confidence: f64,
}

/// Writing style characteristics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WritingStyle {
    /// Average sentence length.
    pub avg_sentence_length: f64,
    /// Vocabulary complexity (0-1).
    pub vocabulary_complexity: f64,
    /// Formality level (0-1).
    pub formality: f64,
    /// Use of contractions.
    pub uses_contractions: bool,
    /// Use of emojis.
    pub uses_emojis: bool,
    /// Preferred punctuation style.
    pub punctuation_style: PunctuationStyle,
    /// Common phrases.
    pub common_phrases: Vec<String>,
    /// Tone.
    pub tone: Tone,
}

/// Punctuation styles.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PunctuationStyle {
    #[default]
    Standard,
    Oxford,
    Minimal,
    Expressive,
}

/// Tone types.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tone {
    #[default]
    Neutral,
    Formal,
    Casual,
    Professional,
    Friendly,
    Technical,
}

/// Coding style characteristics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CodingStyle {
    /// Preferred indentation.
    pub indentation: Indentation,
    /// Naming convention.
    pub naming_convention: NamingConvention,
    /// Brace style.
    pub brace_style: BraceStyle,
    /// Comment style.
    pub comment_style: CommentStyle,
    /// Line length preference.
    pub max_line_length: usize,
    /// Prefers functional style.
    pub functional_style: bool,
    /// Prefers explicit types.
    pub explicit_types: bool,
    /// Common patterns.
    pub common_patterns: Vec<String>,
}

/// Indentation types.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Indentation {
    #[default]
    Spaces4,
    Spaces2,
    Tabs,
}

/// Naming conventions.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum NamingConvention {
    #[default]
    CamelCase,
    SnakeCase,
    PascalCase,
    KebabCase,
}

/// Brace styles.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum BraceStyle {
    #[default]
    SameLine,
    NextLine,
    GNU,
}

/// Comment styles.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommentStyle {
    #[default]
    SingleLine,
    Block,
    Doc,
    Minimal,
}

/// Communication preferences.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CommunicationPrefs {
    /// Preferred response length.
    pub response_length: ResponseLength,
    /// Likes examples.
    pub likes_examples: bool,
    /// Likes step-by-step.
    pub likes_step_by_step: bool,
    /// Prefers bullet points.
    pub prefers_bullets: bool,
    /// Prefers code first.
    pub code_first: bool,
    /// Detail level.
    pub detail_level: DetailLevel,
}

/// Response length preferences.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseLength {
    Concise,
    #[default]
    Balanced,
    Detailed,
}

/// Detail levels.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DetailLevel {
    Minimal,
    #[default]
    Normal,
    Comprehensive,
}

/// Text sample for analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextSample {
    /// Sample identifier.
    pub id: String,
    /// User identifier.
    pub user_id: String,
    /// Sample type.
    pub sample_type: SampleType,
    /// Content.
    pub content: String,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
}

/// Sample types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SampleType {
    Message,
    Code,
    Document,
    Email,
    Comment,
}

/// Style analysis provider.
#[async_trait]
pub trait StyleAnalyzer: Send + Sync {
    /// Analyze text sample.
    async fn analyze_text(&self, text: &str) -> Result<WritingStyle>;

    /// Analyze code sample.
    async fn analyze_code(&self, code: &str, language: &str) -> Result<CodingStyle>;

    /// Match style to text.
    async fn match_style(&self, text: &str, profile: &StyleProfile) -> Result<String>;
}

/// The style matching engine.
pub struct StyleMatcher {
    /// Style analyzer.
    analyzer: Arc<dyn StyleAnalyzer>,
    /// User profiles.
    profiles: Arc<RwLock<HashMap<String, StyleProfile>>>,
    /// Samples.
    samples: Arc<RwLock<Vec<TextSample>>>,
}

impl StyleMatcher {
    /// Create a new style matcher.
    pub fn new(analyzer: Arc<dyn StyleAnalyzer>) -> Self {
        Self {
            analyzer,
            profiles: Arc::new(RwLock::new(HashMap::new())),
            samples: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Add a text sample.
    pub async fn add_sample(
        &self,
        user_id: &str,
        content: &str,
        sample_type: SampleType,
    ) -> Result<()> {
        let sample = TextSample {
            id: Uuid::new_v4().to_string(),
            user_id: user_id.to_string(),
            sample_type,
            content: content.to_string(),
            timestamp: Utc::now(),
        };

        let mut samples = self.samples.write().await;
        samples.push(sample);

        // Keep last 1000 samples per user
        let user_samples = samples.iter().filter(|s| s.user_id == user_id).count();
        if user_samples > 1000 {
            samples.retain(|s| {
                s.user_id != user_id || s.timestamp > Utc::now() - chrono::Duration::days(30)
            });
        }
        drop(samples);

        // Update profile
        self.update_profile(user_id).await?;

        Ok(())
    }

    /// Update user's style profile.
    async fn update_profile(&self, user_id: &str) -> Result<()> {
        let samples = self.samples.read().await;
        let user_samples: Vec<_> = samples.iter().filter(|s| s.user_id == user_id).collect();

        if user_samples.len() < 5 {
            return Err(StyleError::InsufficientSamples(
                "Need at least 5 samples to build profile".to_string(),
            ));
        }

        // Analyze text samples
        let text_samples: Vec<_> = user_samples
            .iter()
            .filter(|s| {
                s.sample_type == SampleType::Message || s.sample_type == SampleType::Document
            })
            .map(|s| s.content.as_str())
            .collect();

        let writing_style = if !text_samples.is_empty() {
            self.analyzer.analyze_text(&text_samples.join("\n")).await?
        } else {
            WritingStyle::default()
        };

        // Analyze code samples
        let code_samples: Vec<_> = user_samples
            .iter()
            .filter(|s| s.sample_type == SampleType::Code)
            .map(|s| s.content.as_str())
            .collect();

        let coding_style = if !code_samples.is_empty() {
            self.analyzer
                .analyze_code(&code_samples.join("\n"), "generic")
                .await?
        } else {
            CodingStyle::default()
        };

        let profile = StyleProfile {
            id: Uuid::new_v4().to_string(),
            user_id: user_id.to_string(),
            writing_style,
            coding_style,
            communication_prefs: CommunicationPrefs::default(),
            sample_count: user_samples.len(),
            updated_at: Utc::now(),
            confidence: (user_samples.len() as f64 / 100.0).min(1.0),
        };

        drop(samples);

        let mut profiles = self.profiles.write().await;
        profiles.insert(user_id.to_string(), profile);

        Ok(())
    }

    /// Get user's style profile.
    pub async fn get_profile(&self, user_id: &str) -> Option<StyleProfile> {
        let profiles = self.profiles.read().await;
        profiles.get(user_id).cloned()
    }

    /// Match output to user's style.
    pub async fn match_style(&self, user_id: &str, text: &str) -> Result<String> {
        let profiles = self.profiles.read().await;
        let profile = profiles
            .get(user_id)
            .ok_or_else(|| StyleError::ProfileNotFound(user_id.to_string()))?
            .clone();
        drop(profiles);

        self.analyzer.match_style(text, &profile).await
    }

    /// Update communication preferences.
    pub async fn update_prefs(&self, user_id: &str, prefs: CommunicationPrefs) -> Result<()> {
        let mut profiles = self.profiles.write().await;
        let profile = profiles
            .get_mut(user_id)
            .ok_or_else(|| StyleError::ProfileNotFound(user_id.to_string()))?;

        profile.communication_prefs = prefs;
        profile.updated_at = Utc::now();

        Ok(())
    }

    /// Get style summary for a user.
    pub async fn get_summary(&self, user_id: &str) -> Option<StyleSummary> {
        let profile = self.get_profile(user_id).await?;

        Some(StyleSummary {
            user_id: user_id.to_string(),
            formality: match profile.writing_style.formality {
                f if f > 0.7 => "Formal".to_string(),
                f if f < 0.3 => "Casual".to_string(),
                _ => "Balanced".to_string(),
            },
            verbosity: match profile.communication_prefs.response_length {
                ResponseLength::Concise => "Concise".to_string(),
                ResponseLength::Balanced => "Balanced".to_string(),
                ResponseLength::Detailed => "Detailed".to_string(),
            },
            coding_convention: format!("{:?}", profile.coding_style.naming_convention),
            sample_count: profile.sample_count,
            confidence: profile.confidence,
        })
    }
}

/// Style summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StyleSummary {
    /// User ID.
    pub user_id: String,
    /// Formality description.
    pub formality: String,
    /// Verbosity description.
    pub verbosity: String,
    /// Coding convention.
    pub coding_convention: String,
    /// Sample count.
    pub sample_count: usize,
    /// Confidence.
    pub confidence: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockAnalyzer;

    #[async_trait]
    impl StyleAnalyzer for MockAnalyzer {
        async fn analyze_text(&self, text: &str) -> Result<WritingStyle> {
            let formality = if text.contains("please") || text.contains("Thank") {
                0.8
            } else {
                0.4
            };
            let uses_emojis = text.contains(':') && text.contains(')');

            Ok(WritingStyle {
                avg_sentence_length: text.split('.').count() as f64,
                vocabulary_complexity: 0.5,
                formality,
                uses_contractions: text.contains("'"),
                uses_emojis,
                punctuation_style: PunctuationStyle::Standard,
                common_phrases: vec![],
                tone: if formality > 0.6 {
                    Tone::Formal
                } else {
                    Tone::Casual
                },
            })
        }

        async fn analyze_code(&self, code: &str, _language: &str) -> Result<CodingStyle> {
            let uses_tabs = code.contains('\t');
            let snake_case = code.contains('_');

            Ok(CodingStyle {
                indentation: if uses_tabs {
                    Indentation::Tabs
                } else {
                    Indentation::Spaces4
                },
                naming_convention: if snake_case {
                    NamingConvention::SnakeCase
                } else {
                    NamingConvention::CamelCase
                },
                brace_style: BraceStyle::SameLine,
                comment_style: CommentStyle::SingleLine,
                max_line_length: 100,
                functional_style: false,
                explicit_types: true,
                common_patterns: vec![],
            })
        }

        async fn match_style(&self, text: &str, profile: &StyleProfile) -> Result<String> {
            let mut result = text.to_string();

            // Apply formality
            if profile.writing_style.formality > 0.7 {
                result = result.replace("Hi", "Hello");
                result = result.replace("thanks", "thank you");
            }

            Ok(result)
        }
    }

    #[tokio::test]
    async fn test_add_samples() {
        let analyzer = Arc::new(MockAnalyzer);
        let matcher = StyleMatcher::new(analyzer);

        for i in 0..6 {
            matcher
                .add_sample(
                    "user1",
                    &format!("Sample message {}", i),
                    SampleType::Message,
                )
                .await
                .ok();
        }

        let profile = matcher.get_profile("user1").await;
        assert!(profile.is_some());
    }

    #[tokio::test]
    async fn test_style_analysis() {
        let analyzer = Arc::new(MockAnalyzer);
        let matcher = StyleMatcher::new(analyzer);

        for i in 0..6 {
            matcher
                .add_sample(
                    "user1",
                    "Please help me with this. Thank you very much!",
                    SampleType::Message,
                )
                .await
                .ok();
        }

        let profile = matcher.get_profile("user1").await.unwrap();
        assert!(profile.writing_style.formality > 0.5);
    }

    #[tokio::test]
    async fn test_match_style() {
        let analyzer = Arc::new(MockAnalyzer);
        let matcher = StyleMatcher::new(analyzer);

        for _ in 0..6 {
            matcher
                .add_sample("user1", "Please review. Thank you.", SampleType::Message)
                .await
                .ok();
        }

        let matched = matcher
            .match_style("user1", "Hi, thanks for help")
            .await
            .unwrap();
        assert!(matched.contains("Hello") || matched.contains("thank you"));
    }

    #[tokio::test]
    async fn test_get_summary() {
        let analyzer = Arc::new(MockAnalyzer);
        let matcher = StyleMatcher::new(analyzer);

        for _ in 0..6 {
            matcher
                .add_sample("user1", "Test message", SampleType::Message)
                .await
                .ok();
        }

        let summary = matcher.get_summary("user1").await.unwrap();
        assert_eq!(summary.user_id, "user1");
    }
}
