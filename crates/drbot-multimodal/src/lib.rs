//! Multi-modal understanding for drbot.
//!
//! True multi-modal AI capabilities including:
//! - Image understanding and analysis
//! - Audio transcription and speaker diarization
//! - Video summarization
//! - Document understanding (PDFs, spreadsheets, diagrams)
//! - Handwriting and whiteboard recognition

use async_trait::async_trait;
use bytes::Bytes;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Result type for multimodal operations.
pub type Result<T> = std::result::Result<T, MultimodalError>;

/// Multimodal errors.
#[derive(Debug, thiserror::Error)]
pub enum MultimodalError {
    #[error("Unsupported media type: {0}")]
    UnsupportedMediaType(String),
    #[error("Processing failed: {0}")]
    ProcessingFailed(String),
    #[error("Model not available: {0}")]
    ModelNotAvailable(String),
    #[error("Content too large: {size} bytes (max: {max})")]
    ContentTooLarge { size: usize, max: usize },
    #[error("Invalid content: {0}")]
    InvalidContent(String),
    #[error("Timeout during processing")]
    Timeout,
}

/// Media type for content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaType {
    /// Image (PNG, JPEG, WebP, GIF)
    Image,
    /// Audio (MP3, WAV, M4A, FLAC)
    Audio,
    /// Video (MP4, WebM, MOV)
    Video,
    /// Document (PDF, DOCX, XLSX)
    Document,
    /// Text (plain text, markdown, code)
    Text,
    /// Structured data (JSON, XML, CSV)
    Structured,
}

/// Content to be analyzed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Content {
    /// Unique content ID.
    pub id: Uuid,
    /// Media type.
    pub media_type: MediaType,
    /// Raw content bytes.
    #[serde(skip)]
    pub data: Bytes,
    /// Content metadata.
    pub metadata: ContentMetadata,
    /// Source information.
    pub source: Option<String>,
    /// Created timestamp.
    pub created_at: DateTime<Utc>,
}

impl Content {
    /// Create new content.
    pub fn new(media_type: MediaType, data: impl Into<Bytes>) -> Self {
        Self {
            id: Uuid::new_v4(),
            media_type,
            data: data.into(),
            metadata: ContentMetadata::default(),
            source: None,
            created_at: Utc::now(),
        }
    }

    /// Set source.
    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }

    /// Set metadata.
    pub fn with_metadata(mut self, metadata: ContentMetadata) -> Self {
        self.metadata = metadata;
        self
    }
}

/// Content metadata.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContentMetadata {
    /// File name if applicable.
    pub filename: Option<String>,
    /// MIME type.
    pub mime_type: Option<String>,
    /// Width for images/video.
    pub width: Option<u32>,
    /// Height for images/video.
    pub height: Option<u32>,
    /// Duration for audio/video in seconds.
    pub duration_secs: Option<f64>,
    /// Page count for documents.
    pub page_count: Option<u32>,
    /// Custom properties.
    pub properties: HashMap<String, serde_json::Value>,
}

/// Image analysis result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageAnalysis {
    /// Detected objects.
    pub objects: Vec<DetectedObject>,
    /// Detected text (OCR).
    pub text: Vec<TextRegion>,
    /// Scene description.
    pub description: String,
    /// Detected faces.
    pub faces: Vec<DetectedFace>,
    /// Dominant colors.
    pub colors: Vec<Color>,
    /// Image categories/tags.
    pub tags: Vec<String>,
    /// Safety assessment.
    pub safety: SafetyAssessment,
    /// Confidence score.
    pub confidence: f32,
}

/// Detected object in image.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedObject {
    /// Object label.
    pub label: String,
    /// Confidence score.
    pub confidence: f32,
    /// Bounding box.
    pub bounding_box: BoundingBox,
}

/// Bounding box coordinates.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct BoundingBox {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Detected text region.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextRegion {
    /// Detected text.
    pub text: String,
    /// Confidence score.
    pub confidence: f32,
    /// Bounding box.
    pub bounding_box: BoundingBox,
    /// Language detected.
    pub language: Option<String>,
}

/// Detected face.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedFace {
    /// Bounding box.
    pub bounding_box: BoundingBox,
    /// Detected emotions.
    pub emotions: HashMap<String, f32>,
    /// Age estimate.
    pub age_estimate: Option<u32>,
    /// Confidence score.
    pub confidence: f32,
}

/// Color information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Color {
    /// Hex color code.
    pub hex: String,
    /// RGB values.
    pub rgb: (u8, u8, u8),
    /// Percentage of image.
    pub percentage: f32,
    /// Color name.
    pub name: Option<String>,
}

/// Safety assessment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyAssessment {
    /// Is content safe for work.
    pub safe_for_work: bool,
    /// Violence score (0-1).
    pub violence: f32,
    /// Adult content score (0-1).
    pub adult: f32,
    /// Medical content score (0-1).
    pub medical: f32,
    /// Flags.
    pub flags: Vec<String>,
}

impl Default for SafetyAssessment {
    fn default() -> Self {
        Self {
            safe_for_work: true,
            violence: 0.0,
            adult: 0.0,
            medical: 0.0,
            flags: Vec::new(),
        }
    }
}

/// Audio analysis result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioAnalysis {
    /// Transcription.
    pub transcription: Transcription,
    /// Detected speakers.
    pub speakers: Vec<Speaker>,
    /// Audio segments.
    pub segments: Vec<AudioSegment>,
    /// Background sounds.
    pub background: Vec<String>,
    /// Music detection.
    pub music: Option<MusicInfo>,
    /// Overall sentiment.
    pub sentiment: Sentiment,
}

/// Transcription result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transcription {
    /// Full text.
    pub text: String,
    /// Language.
    pub language: String,
    /// Confidence score.
    pub confidence: f32,
    /// Word-level timing.
    pub words: Vec<TimedWord>,
}

/// Word with timing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimedWord {
    /// Word text.
    pub word: String,
    /// Start time in seconds.
    pub start: f64,
    /// End time in seconds.
    pub end: f64,
    /// Speaker ID if identified.
    pub speaker_id: Option<String>,
    /// Confidence.
    pub confidence: f32,
}

/// Speaker information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Speaker {
    /// Speaker ID.
    pub id: String,
    /// Speaker label/name.
    pub label: Option<String>,
    /// Speaking duration in seconds.
    pub duration: f64,
    /// Voice characteristics.
    pub voice_profile: VoiceProfile,
}

/// Voice profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceProfile {
    /// Estimated gender.
    pub gender: Option<String>,
    /// Estimated age range.
    pub age_range: Option<String>,
    /// Voice quality descriptors.
    pub qualities: Vec<String>,
}

/// Audio segment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioSegment {
    /// Start time.
    pub start: f64,
    /// End time.
    pub end: f64,
    /// Segment type (speech, music, silence, noise).
    pub segment_type: String,
    /// Speaker ID if speech.
    pub speaker_id: Option<String>,
    /// Transcript if speech.
    pub text: Option<String>,
}

/// Music information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MusicInfo {
    /// Detected genre.
    pub genre: Option<String>,
    /// Tempo (BPM).
    pub tempo: Option<f32>,
    /// Key signature.
    pub key: Option<String>,
    /// Mood descriptors.
    pub mood: Vec<String>,
}

/// Sentiment analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sentiment {
    /// Overall sentiment (positive, negative, neutral).
    pub overall: String,
    /// Sentiment score (-1 to 1).
    pub score: f32,
    /// Emotion breakdown.
    pub emotions: HashMap<String, f32>,
}

/// Video analysis result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoAnalysis {
    /// Video duration.
    pub duration: f64,
    /// Frame rate.
    pub frame_rate: f32,
    /// Resolution.
    pub resolution: (u32, u32),
    /// Key frames.
    pub key_frames: Vec<KeyFrame>,
    /// Scene changes.
    pub scenes: Vec<Scene>,
    /// Audio analysis.
    pub audio: Option<AudioAnalysis>,
    /// Video summary.
    pub summary: String,
    /// Action/activity detection.
    pub activities: Vec<Activity>,
}

/// Key frame from video.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyFrame {
    /// Timestamp in seconds.
    pub timestamp: f64,
    /// Frame analysis.
    pub analysis: ImageAnalysis,
    /// Why this frame is key.
    pub reason: String,
}

/// Video scene.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scene {
    /// Start time.
    pub start: f64,
    /// End time.
    pub end: f64,
    /// Scene description.
    pub description: String,
    /// Scene type.
    pub scene_type: Option<String>,
}

/// Detected activity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Activity {
    /// Activity label.
    pub label: String,
    /// Confidence.
    pub confidence: f32,
    /// Time range.
    pub start: f64,
    pub end: f64,
}

/// Document analysis result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentAnalysis {
    /// Document type.
    pub document_type: DocumentType,
    /// Extracted text.
    pub text: String,
    /// Document structure.
    pub structure: DocumentStructure,
    /// Tables found.
    pub tables: Vec<Table>,
    /// Images/figures found.
    pub figures: Vec<Figure>,
    /// Key-value pairs extracted.
    pub key_values: HashMap<String, String>,
    /// Summary.
    pub summary: String,
    /// Language.
    pub language: String,
}

/// Document type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentType {
    Report,
    Invoice,
    Contract,
    Form,
    Letter,
    Resume,
    Academic,
    Presentation,
    Spreadsheet,
    Other,
}

/// Document structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentStructure {
    /// Title.
    pub title: Option<String>,
    /// Authors.
    pub authors: Vec<String>,
    /// Date.
    pub date: Option<String>,
    /// Sections.
    pub sections: Vec<Section>,
    /// Table of contents.
    pub toc: Vec<TocEntry>,
}

/// Document section.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Section {
    /// Section title.
    pub title: String,
    /// Section level (1 = top level).
    pub level: u32,
    /// Page number.
    pub page: Option<u32>,
    /// Section content.
    pub content: String,
}

/// Table of contents entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TocEntry {
    /// Title.
    pub title: String,
    /// Level.
    pub level: u32,
    /// Page.
    pub page: Option<u32>,
}

/// Extracted table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Table {
    /// Table title/caption.
    pub caption: Option<String>,
    /// Headers.
    pub headers: Vec<String>,
    /// Rows.
    pub rows: Vec<Vec<String>>,
    /// Page number.
    pub page: Option<u32>,
}

/// Extracted figure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Figure {
    /// Figure caption.
    pub caption: Option<String>,
    /// Figure type (chart, diagram, photo, etc.).
    pub figure_type: String,
    /// Page number.
    pub page: Option<u32>,
    /// Analysis if image.
    pub analysis: Option<ImageAnalysis>,
}

/// Trait for multimodal processors.
#[async_trait]
pub trait MultimodalProcessor: Send + Sync {
    /// Analyze image content.
    async fn analyze_image(&self, content: &Content) -> Result<ImageAnalysis>;
    /// Analyze audio content.
    async fn analyze_audio(&self, content: &Content) -> Result<AudioAnalysis>;
    /// Analyze video content.
    async fn analyze_video(&self, content: &Content) -> Result<VideoAnalysis>;
    /// Analyze document content.
    async fn analyze_document(&self, content: &Content) -> Result<DocumentAnalysis>;
    /// Get supported media types.
    fn supported_types(&self) -> Vec<MediaType>;
}

/// Configuration for multimodal engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultimodalConfig {
    /// Maximum content size in bytes.
    pub max_content_size: usize,
    /// Enable OCR for images.
    pub enable_ocr: bool,
    /// Enable face detection.
    pub enable_face_detection: bool,
    /// Enable speaker diarization.
    pub enable_diarization: bool,
    /// Video frame sampling rate.
    pub video_sample_rate: f32,
    /// Preferred language for OCR/transcription.
    pub preferred_language: Option<String>,
}

impl Default for MultimodalConfig {
    fn default() -> Self {
        Self {
            max_content_size: 100 * 1024 * 1024, // 100MB
            enable_ocr: true,
            enable_face_detection: true,
            enable_diarization: true,
            video_sample_rate: 1.0, // 1 frame per second
            preferred_language: None,
        }
    }
}

/// Multimodal engine.
pub struct MultimodalEngine<P: MultimodalProcessor> {
    config: MultimodalConfig,
    processor: P,
    cache: Arc<RwLock<HashMap<Uuid, CachedAnalysis>>>,
}

/// Cached analysis result.
#[derive(Debug, Clone)]
struct CachedAnalysis {
    content_id: Uuid,
    media_type: MediaType,
    result: serde_json::Value,
    created_at: DateTime<Utc>,
}

impl<P: MultimodalProcessor> MultimodalEngine<P> {
    /// Create new engine.
    pub fn new(config: MultimodalConfig, processor: P) -> Self {
        Self {
            config,
            processor,
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Analyze content automatically detecting type.
    pub async fn analyze(&self, content: &Content) -> Result<AnalysisResult> {
        // Check size limit
        if content.data.len() > self.config.max_content_size {
            return Err(MultimodalError::ContentTooLarge {
                size: content.data.len(),
                max: self.config.max_content_size,
            });
        }

        // Check cache
        if let Some(cached) = self.cache.read().await.get(&content.id) {
            return Ok(serde_json::from_value(cached.result.clone())
                .map_err(|e| MultimodalError::ProcessingFailed(e.to_string()))?);
        }

        let result = match content.media_type {
            MediaType::Image => {
                let analysis = self.processor.analyze_image(content).await?;
                AnalysisResult::Image(analysis)
            }
            MediaType::Audio => {
                let analysis = self.processor.analyze_audio(content).await?;
                AnalysisResult::Audio(analysis)
            }
            MediaType::Video => {
                let analysis = self.processor.analyze_video(content).await?;
                AnalysisResult::Video(analysis)
            }
            MediaType::Document => {
                let analysis = self.processor.analyze_document(content).await?;
                AnalysisResult::Document(analysis)
            }
            MediaType::Text | MediaType::Structured => {
                // For text/structured, create a simple document analysis
                let text = String::from_utf8_lossy(&content.data).to_string();
                AnalysisResult::Text(TextAnalysis {
                    text,
                    language: "en".to_string(),
                    word_count: 0,
                    sentiment: None,
                })
            }
        };

        // Cache result
        let cached = CachedAnalysis {
            content_id: content.id,
            media_type: content.media_type,
            result: serde_json::to_value(&result)
                .map_err(|e| MultimodalError::ProcessingFailed(e.to_string()))?,
            created_at: Utc::now(),
        };
        self.cache.write().await.insert(content.id, cached);

        Ok(result)
    }

    /// Extract text from any content type.
    pub async fn extract_text(&self, content: &Content) -> Result<String> {
        let result = self.analyze(content).await?;
        Ok(match result {
            AnalysisResult::Image(a) => a
                .text
                .iter()
                .map(|t| t.text.as_str())
                .collect::<Vec<_>>()
                .join("\n"),
            AnalysisResult::Audio(a) => a.transcription.text,
            AnalysisResult::Video(a) => a.audio.map(|a| a.transcription.text).unwrap_or_default(),
            AnalysisResult::Document(a) => a.text,
            AnalysisResult::Text(a) => a.text,
        })
    }

    /// Get a summary of the content.
    pub async fn summarize(&self, content: &Content) -> Result<String> {
        let result = self.analyze(content).await?;
        Ok(match result {
            AnalysisResult::Image(a) => a.description,
            AnalysisResult::Audio(a) => {
                format!(
                    "Audio ({:.1}s): {} speakers. {}",
                    a.segments.last().map(|s| s.end).unwrap_or(0.0),
                    a.speakers.len(),
                    a.transcription.text.chars().take(200).collect::<String>()
                )
            }
            AnalysisResult::Video(a) => a.summary,
            AnalysisResult::Document(a) => a.summary,
            AnalysisResult::Text(a) => a.text.chars().take(200).collect(),
        })
    }

    /// Clear cache.
    pub async fn clear_cache(&self) {
        self.cache.write().await.clear();
    }
}

/// Analysis result enum.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AnalysisResult {
    Image(ImageAnalysis),
    Audio(AudioAnalysis),
    Video(VideoAnalysis),
    Document(DocumentAnalysis),
    Text(TextAnalysis),
}

/// Simple text analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextAnalysis {
    pub text: String,
    pub language: String,
    pub word_count: usize,
    pub sentiment: Option<Sentiment>,
}

/// Mock processor for testing.
pub struct MockProcessor;

#[async_trait]
impl MultimodalProcessor for MockProcessor {
    async fn analyze_image(&self, content: &Content) -> Result<ImageAnalysis> {
        Ok(ImageAnalysis {
            objects: vec![DetectedObject {
                label: "test object".to_string(),
                confidence: 0.95,
                bounding_box: BoundingBox {
                    x: 0.1,
                    y: 0.1,
                    width: 0.5,
                    height: 0.5,
                },
            }],
            text: vec![],
            description: format!("Image analysis of {} bytes", content.data.len()),
            faces: vec![],
            colors: vec![Color {
                hex: "#FF5733".to_string(),
                rgb: (255, 87, 51),
                percentage: 0.3,
                name: Some("Orange".to_string()),
            }],
            tags: vec!["test".to_string()],
            safety: SafetyAssessment::default(),
            confidence: 0.9,
        })
    }

    async fn analyze_audio(&self, content: &Content) -> Result<AudioAnalysis> {
        Ok(AudioAnalysis {
            transcription: Transcription {
                text: "Mock transcription of audio content".to_string(),
                language: "en".to_string(),
                confidence: 0.95,
                words: vec![],
            },
            speakers: vec![Speaker {
                id: "speaker_1".to_string(),
                label: Some("Speaker 1".to_string()),
                duration: 10.0,
                voice_profile: VoiceProfile {
                    gender: None,
                    age_range: None,
                    qualities: vec![],
                },
            }],
            segments: vec![AudioSegment {
                start: 0.0,
                end: 10.0,
                segment_type: "speech".to_string(),
                speaker_id: Some("speaker_1".to_string()),
                text: Some("Mock transcription".to_string()),
            }],
            background: vec![],
            music: None,
            sentiment: Sentiment {
                overall: "neutral".to_string(),
                score: 0.0,
                emotions: HashMap::new(),
            },
        })
    }

    async fn analyze_video(&self, content: &Content) -> Result<VideoAnalysis> {
        Ok(VideoAnalysis {
            duration: 60.0,
            frame_rate: 30.0,
            resolution: (1920, 1080),
            key_frames: vec![],
            scenes: vec![Scene {
                start: 0.0,
                end: 60.0,
                description: "Main scene".to_string(),
                scene_type: Some("general".to_string()),
            }],
            audio: None,
            summary: format!("Video analysis of {} bytes", content.data.len()),
            activities: vec![],
        })
    }

    async fn analyze_document(&self, content: &Content) -> Result<DocumentAnalysis> {
        let text = String::from_utf8_lossy(&content.data).to_string();
        Ok(DocumentAnalysis {
            document_type: DocumentType::Other,
            text: text.clone(),
            structure: DocumentStructure {
                title: Some("Mock Document".to_string()),
                authors: vec![],
                date: None,
                sections: vec![],
                toc: vec![],
            },
            tables: vec![],
            figures: vec![],
            key_values: HashMap::new(),
            summary: format!("Document with {} characters", text.len()),
            language: "en".to_string(),
        })
    }

    fn supported_types(&self) -> Vec<MediaType> {
        vec![
            MediaType::Image,
            MediaType::Audio,
            MediaType::Video,
            MediaType::Document,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_image_analysis() {
        let engine = MultimodalEngine::new(MultimodalConfig::default(), MockProcessor);
        let content = Content::new(MediaType::Image, vec![0u8; 100]);

        let result = engine.analyze(&content).await.unwrap();
        match result {
            AnalysisResult::Image(analysis) => {
                assert!(!analysis.objects.is_empty());
                assert!(analysis.confidence > 0.0);
            }
            _ => panic!("Expected image analysis"),
        }
    }

    #[tokio::test]
    async fn test_audio_analysis() {
        let engine = MultimodalEngine::new(MultimodalConfig::default(), MockProcessor);
        let content = Content::new(MediaType::Audio, vec![0u8; 100]);

        let result = engine.analyze(&content).await.unwrap();
        match result {
            AnalysisResult::Audio(analysis) => {
                assert!(!analysis.transcription.text.is_empty());
                assert!(!analysis.speakers.is_empty());
            }
            _ => panic!("Expected audio analysis"),
        }
    }

    #[tokio::test]
    async fn test_extract_text() {
        let engine = MultimodalEngine::new(MultimodalConfig::default(), MockProcessor);
        let content = Content::new(MediaType::Audio, vec![0u8; 100]);

        let text = engine.extract_text(&content).await.unwrap();
        assert!(!text.is_empty());
    }

    #[tokio::test]
    async fn test_summarize() {
        let engine = MultimodalEngine::new(MultimodalConfig::default(), MockProcessor);
        let content = Content::new(MediaType::Video, vec![0u8; 100]);

        let summary = engine.summarize(&content).await.unwrap();
        assert!(!summary.is_empty());
    }

    #[tokio::test]
    async fn test_content_too_large() {
        let config = MultimodalConfig {
            max_content_size: 10,
            ..Default::default()
        };
        let engine = MultimodalEngine::new(config, MockProcessor);
        let content = Content::new(MediaType::Image, vec![0u8; 100]);

        let result = engine.analyze(&content).await;
        assert!(matches!(
            result,
            Err(MultimodalError::ContentTooLarge { .. })
        ));
    }

    #[tokio::test]
    async fn test_cache() {
        let engine = MultimodalEngine::new(MultimodalConfig::default(), MockProcessor);
        let content = Content::new(MediaType::Image, vec![0u8; 100]);

        // First call populates cache
        let _ = engine.analyze(&content).await.unwrap();

        // Second call should use cache
        let _ = engine.analyze(&content).await.unwrap();

        // Clear cache
        engine.clear_cache().await;
    }
}
