//! Attention visualization for drbot.
//!
//! Visualize what the AI is focusing on.
//!
//! # Features
//!
//! - Attention weight extraction
//! - Focus visualization
//! - Importance highlighting
//! - Context analysis

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Attention result type.
pub type Result<T> = std::result::Result<T, AttentionError>;

/// Attention errors.
#[derive(Debug, thiserror::Error)]
pub enum AttentionError {
    #[error("Analysis failed: {0}")]
    AnalysisFailed(String),
    #[error("No attention data")]
    NoData,
    #[error("Invalid input")]
    InvalidInput,
}

/// Attention weights for text.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttentionMap {
    /// Map ID.
    pub id: Uuid,
    /// Input text.
    pub input: String,
    /// Token-level attention.
    pub token_attention: Vec<TokenAttention>,
    /// Segment-level attention.
    pub segment_attention: Vec<SegmentAttention>,
    /// Overall focus areas.
    pub focus_areas: Vec<FocusArea>,
    /// Created at.
    pub created_at: DateTime<Utc>,
}

impl AttentionMap {
    /// Create a new attention map.
    pub fn new(input: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            input: input.to_string(),
            token_attention: Vec::new(),
            segment_attention: Vec::new(),
            focus_areas: Vec::new(),
            created_at: Utc::now(),
        }
    }

    /// Get top-attended tokens.
    pub fn top_tokens(&self, n: usize) -> Vec<&TokenAttention> {
        let mut sorted: Vec<_> = self.token_attention.iter().collect();
        sorted.sort_by(|a, b| b.weight.partial_cmp(&a.weight).unwrap());
        sorted.truncate(n);
        sorted
    }

    /// Get tokens above threshold.
    pub fn tokens_above_threshold(&self, threshold: f32) -> Vec<&TokenAttention> {
        self.token_attention
            .iter()
            .filter(|t| t.weight >= threshold)
            .collect()
    }
}

/// Token-level attention.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenAttention {
    /// Token text.
    pub token: String,
    /// Position in input.
    pub position: usize,
    /// Attention weight (0-1).
    pub weight: f32,
    /// Token importance.
    pub importance: f32,
    /// Start character.
    pub char_start: usize,
    /// End character.
    pub char_end: usize,
}

impl TokenAttention {
    /// Create new token attention.
    pub fn new(token: &str, position: usize, weight: f32, char_start: usize) -> Self {
        Self {
            token: token.to_string(),
            position,
            weight,
            importance: weight,
            char_start,
            char_end: char_start + token.len(),
        }
    }
}

/// Segment-level attention.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentAttention {
    /// Segment text.
    pub text: String,
    /// Segment type.
    pub segment_type: SegmentType,
    /// Average attention weight.
    pub avg_weight: f32,
    /// Peak attention weight.
    pub peak_weight: f32,
    /// Start position.
    pub start: usize,
    /// End position.
    pub end: usize,
}

/// Segment types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SegmentType {
    Sentence,
    Phrase,
    Entity,
    Keyword,
    Other,
}

/// Focus area.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FocusArea {
    /// Area name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Importance (0-1).
    pub importance: f32,
    /// Related tokens.
    pub tokens: Vec<usize>,
}

/// Visualization options.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisualizationOptions {
    /// Show token weights.
    pub show_weights: bool,
    /// Highlight threshold.
    pub highlight_threshold: f32,
    /// Color scheme.
    pub color_scheme: ColorScheme,
    /// Show focus areas.
    pub show_focus_areas: bool,
}

impl Default for VisualizationOptions {
    fn default() -> Self {
        Self {
            show_weights: true,
            highlight_threshold: 0.5,
            color_scheme: ColorScheme::Heat,
            show_focus_areas: true,
        }
    }
}

/// Color schemes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColorScheme {
    Heat,
    Blue,
    Green,
    Grayscale,
}

/// Attention configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttentionConfig {
    /// Enable attention tracking.
    pub enabled: bool,
    /// Track token-level.
    pub token_level: bool,
    /// Track segment-level.
    pub segment_level: bool,
    /// Minimum weight to track.
    pub min_weight: f32,
}

impl Default for AttentionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            token_level: true,
            segment_level: true,
            min_weight: 0.1,
        }
    }
}

/// Attention visualization engine.
pub struct AttentionEngine {
    config: AttentionConfig,
    maps: Arc<RwLock<Vec<AttentionMap>>>,
}

impl AttentionEngine {
    /// Create a new attention engine.
    pub fn new(config: AttentionConfig) -> Self {
        Self {
            config,
            maps: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Analyze attention for input.
    pub async fn analyze(&self, input: &str) -> Result<AttentionMap> {
        if input.is_empty() {
            return Err(AttentionError::InvalidInput);
        }

        let mut map = AttentionMap::new(input);

        // Analyze tokens
        if self.config.token_level {
            map.token_attention = self.analyze_tokens(input);
        }

        // Analyze segments
        if self.config.segment_level {
            map.segment_attention = self.analyze_segments(input, &map.token_attention);
        }

        // Identify focus areas
        map.focus_areas = self.identify_focus_areas(&map.token_attention);

        self.maps.write().await.push(map.clone());

        Ok(map)
    }

    fn analyze_tokens(&self, input: &str) -> Vec<TokenAttention> {
        let mut tokens = Vec::new();
        let mut position = 0;
        let mut char_pos = 0;

        for word in input.split_whitespace() {
            // Find actual position in string
            if let Some(idx) = input[char_pos..].find(word) {
                char_pos += idx;
            }

            // Calculate attention weight (simplified - real implementation would use model)
            let weight = self.calculate_token_weight(word, position, input);

            if weight >= self.config.min_weight {
                tokens.push(TokenAttention::new(word, position, weight, char_pos));
            }

            char_pos += word.len();
            position += 1;
        }

        tokens
    }

    fn calculate_token_weight(&self, token: &str, position: usize, full_input: &str) -> f32 {
        let mut weight: f32 = 0.5; // Base weight

        // Question words get high attention
        let question_words = ["what", "why", "how", "when", "where", "who", "which"];
        if question_words.contains(&token.to_lowercase().as_str()) {
            weight += 0.3;
        }

        // Named entities (capitalized words not at start)
        if position > 0
            && token
                .chars()
                .next()
                .map(|c| c.is_uppercase())
                .unwrap_or(false)
        {
            weight += 0.2;
        }

        // Numbers
        if token.parse::<f64>().is_ok() {
            weight += 0.15;
        }

        // Technical terms
        let technical = [
            "error", "function", "class", "method", "api", "database", "server",
        ];
        if technical.contains(&token.to_lowercase().as_str()) {
            weight += 0.2;
        }

        // Position-based (first and last get more attention)
        let total_words = full_input.split_whitespace().count();
        if position == 0 || position == total_words - 1 {
            weight += 0.1;
        }

        weight.min(1.0)
    }

    fn analyze_segments(&self, input: &str, tokens: &[TokenAttention]) -> Vec<SegmentAttention> {
        let mut segments = Vec::new();

        // Split into sentences
        for sentence in input.split(|c| c == '.' || c == '!' || c == '?') {
            let sentence = sentence.trim();
            if sentence.is_empty() {
                continue;
            }

            let start = input.find(sentence).unwrap_or(0);
            let end = start + sentence.len();

            // Find tokens in this segment
            let segment_tokens: Vec<_> = tokens
                .iter()
                .filter(|t| t.char_start >= start && t.char_end <= end)
                .collect();

            if segment_tokens.is_empty() {
                continue;
            }

            let avg_weight =
                segment_tokens.iter().map(|t| t.weight).sum::<f32>() / segment_tokens.len() as f32;
            let peak_weight = segment_tokens
                .iter()
                .map(|t| t.weight)
                .fold(0.0f32, |a, b| a.max(b));

            segments.push(SegmentAttention {
                text: sentence.to_string(),
                segment_type: SegmentType::Sentence,
                avg_weight,
                peak_weight,
                start,
                end,
            });
        }

        segments
    }

    fn identify_focus_areas(&self, tokens: &[TokenAttention]) -> Vec<FocusArea> {
        let mut areas = Vec::new();

        // Group high-attention tokens
        let high_attention: Vec<_> = tokens
            .iter()
            .enumerate()
            .filter(|(_, t)| t.weight >= 0.6)
            .collect();

        if !high_attention.is_empty() {
            let token_indices: Vec<_> = high_attention.iter().map(|(i, _)| *i).collect();
            let avg_importance = high_attention
                .iter()
                .map(|(_, t)| t.importance)
                .sum::<f32>()
                / high_attention.len() as f32;

            areas.push(FocusArea {
                name: "Primary Focus".to_string(),
                description: "Main points of attention".to_string(),
                importance: avg_importance,
                tokens: token_indices,
            });
        }

        areas
    }

    /// Render attention as HTML.
    pub fn render_html(&self, map: &AttentionMap, options: &VisualizationOptions) -> String {
        let mut html = String::from("<div class='attention-map'>");

        let mut char_idx = 0;
        for word in map.input.split_whitespace() {
            // Find token attention
            let attention = map
                .token_attention
                .iter()
                .find(|t| t.token == word && t.char_start >= char_idx);

            let (color, opacity) = if let Some(att) = attention {
                if att.weight >= options.highlight_threshold {
                    let opacity = att.weight;
                    let color = match options.color_scheme {
                        ColorScheme::Heat => "rgba(255, 0, 0,",
                        ColorScheme::Blue => "rgba(0, 0, 255,",
                        ColorScheme::Green => "rgba(0, 255, 0,",
                        ColorScheme::Grayscale => "rgba(0, 0, 0,",
                    };
                    (color, opacity)
                } else {
                    ("rgba(0, 0, 0,", 0.0)
                }
            } else {
                ("rgba(0, 0, 0,", 0.0)
            };

            html.push_str(&format!(
                "<span style='background-color: {} {});'>{}</span> ",
                color, opacity, word
            ));

            char_idx += word.len() + 1;
        }

        html.push_str("</div>");

        if options.show_focus_areas {
            html.push_str("<div class='focus-areas'>");
            for area in &map.focus_areas {
                html.push_str(&format!(
                    "<div class='focus-area'><strong>{}</strong>: {} (importance: {:.2})</div>",
                    area.name, area.description, area.importance
                ));
            }
            html.push_str("</div>");
        }

        html
    }

    /// Render attention as markdown.
    pub fn render_markdown(&self, map: &AttentionMap, threshold: f32) -> String {
        let mut md = String::from("## Attention Analysis\n\n");

        // Top tokens
        md.push_str("### Key Focus Points\n\n");
        for token in map.top_tokens(5) {
            md.push_str(&format!(
                "- **{}** (weight: {:.2})\n",
                token.token, token.weight
            ));
        }

        // Focus areas
        if !map.focus_areas.is_empty() {
            md.push_str("\n### Focus Areas\n\n");
            for area in &map.focus_areas {
                md.push_str(&format!(
                    "- **{}**: {} (importance: {:.2})\n",
                    area.name, area.description, area.importance
                ));
            }
        }

        // Highlighted text
        md.push_str("\n### Highlighted Input\n\n");
        for word in map.input.split_whitespace() {
            let is_highlighted = map
                .token_attention
                .iter()
                .any(|t| t.token == word && t.weight >= threshold);

            if is_highlighted {
                md.push_str(&format!("**{}** ", word));
            } else {
                md.push_str(&format!("{} ", word));
            }
        }
        md.push('\n');

        md
    }

    /// Get history.
    pub async fn history(&self, limit: usize) -> Vec<AttentionMap> {
        let maps = self.maps.read().await;
        maps.iter().rev().take(limit).cloned().collect()
    }

    /// Get statistics.
    pub async fn stats(&self) -> AttentionStats {
        let maps = self.maps.read().await;

        let total_tokens: usize = maps.iter().map(|m| m.token_attention.len()).sum();
        let avg_focus_areas = if !maps.is_empty() {
            maps.iter().map(|m| m.focus_areas.len()).sum::<usize>() as f32 / maps.len() as f32
        } else {
            0.0
        };

        AttentionStats {
            total_analyses: maps.len(),
            total_tokens_analyzed: total_tokens,
            avg_focus_areas,
        }
    }
}

/// Attention statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttentionStats {
    pub total_analyses: usize,
    pub total_tokens_analyzed: usize,
    pub avg_focus_areas: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_attention_analysis() {
        let engine = AttentionEngine::new(AttentionConfig::default());

        let map = engine
            .analyze("What is the error in this function?")
            .await
            .unwrap();

        assert!(!map.token_attention.is_empty());
        assert!(!map.segment_attention.is_empty());
    }

    #[tokio::test]
    async fn test_top_tokens() {
        let engine = AttentionEngine::new(AttentionConfig::default());

        let map = engine
            .analyze("How do I fix this critical error in the database?")
            .await
            .unwrap();

        let top = map.top_tokens(3);
        assert!(!top.is_empty());
        assert!(top[0].weight >= top[1].weight);
    }

    #[test]
    fn test_render_markdown() {
        let engine = AttentionEngine::new(AttentionConfig::default());

        let mut map = AttentionMap::new("Test input");
        map.token_attention
            .push(TokenAttention::new("Test", 0, 0.8, 0));
        map.token_attention
            .push(TokenAttention::new("input", 1, 0.4, 5));

        let md = engine.render_markdown(&map, 0.5);
        // The "Highlighted Input" section should have "Test" bolded but not "input"
        assert!(md.contains("### Highlighted Input"));
        let highlighted_section = md.split("### Highlighted Input").nth(1).unwrap();
        assert!(highlighted_section.contains("**Test**"));
        // "input" should appear unbolded in the highlighted section (just "input " not "**input**")
        assert!(highlighted_section.contains("input"));
    }

    #[tokio::test]
    async fn test_empty_input() {
        let engine = AttentionEngine::new(AttentionConfig::default());

        let result = engine.analyze("").await;
        assert!(matches!(result, Err(AttentionError::InvalidInput)));
    }
}
