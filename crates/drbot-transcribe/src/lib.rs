//! Audio/video transcription with speaker diarization.
//!
//! This crate provides:
//! - Audio transcription
//! - Video transcription
//! - Speaker identification
//! - Timestamp alignment
//! - Multiple language support

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Transcription errors.
#[derive(Debug, Error)]
pub enum TranscribeError {
    #[error("Transcription failed: {0}")]
    TranscriptionFailed(String),

    #[error("Invalid media: {0}")]
    InvalidMedia(String),

    #[error("Unsupported format: {0}")]
    UnsupportedFormat(String),

    #[error("Provider error: {0}")]
    ProviderError(String),
}

/// Result type for transcription operations.
pub type Result<T> = std::result::Result<T, TranscribeError>;

/// Media to transcribe.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Media {
    /// Media identifier.
    pub id: String,
    /// Media type.
    pub media_type: MediaType,
    /// Source path or URL.
    pub source: String,
    /// Duration in seconds.
    pub duration_secs: f64,
    /// Sample rate.
    pub sample_rate: Option<u32>,
    /// Channels.
    pub channels: Option<u8>,
    /// Format.
    pub format: String,
}

/// Media types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MediaType {
    Audio,
    Video,
}

/// Transcription result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transcript {
    /// Transcript identifier.
    pub id: String,
    /// Source media.
    pub media_id: String,
    /// Full text.
    pub text: String,
    /// Segments with timing.
    pub segments: Vec<Segment>,
    /// Detected language.
    pub language: String,
    /// Confidence score.
    pub confidence: f64,
    /// Speakers identified.
    pub speakers: Vec<Speaker>,
    /// Processing time.
    pub processing_time_ms: u64,
    /// Created at.
    pub created_at: DateTime<Utc>,
}

/// A segment of transcription.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Segment {
    /// Segment identifier.
    pub id: String,
    /// Start time in seconds.
    pub start: f64,
    /// End time in seconds.
    pub end: f64,
    /// Transcribed text.
    pub text: String,
    /// Speaker identifier.
    pub speaker_id: Option<String>,
    /// Confidence.
    pub confidence: f64,
    /// Words with timing.
    pub words: Vec<Word>,
}

/// A word with timing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Word {
    /// The word.
    pub word: String,
    /// Start time.
    pub start: f64,
    /// End time.
    pub end: f64,
    /// Confidence.
    pub confidence: f64,
}

/// An identified speaker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Speaker {
    /// Speaker identifier.
    pub id: String,
    /// Speaker label/name.
    pub label: String,
    /// Total speaking time.
    pub speaking_time_secs: f64,
    /// Segment count.
    pub segment_count: usize,
    /// Voice characteristics.
    pub characteristics: VoiceCharacteristics,
}

/// Voice characteristics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VoiceCharacteristics {
    /// Estimated gender.
    pub gender: Option<String>,
    /// Estimated age range.
    pub age_range: Option<String>,
    /// Speaking pace (words per minute).
    pub pace_wpm: Option<f64>,
    /// Pitch level.
    pub pitch: Option<String>,
}

/// Transcription options.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscribeOptions {
    /// Target language (for translation).
    pub language: Option<String>,
    /// Enable speaker diarization.
    pub diarize: bool,
    /// Expected speaker count.
    pub speaker_count: Option<usize>,
    /// Word-level timestamps.
    pub word_timestamps: bool,
    /// Punctuation.
    pub punctuate: bool,
    /// Profanity filter.
    pub filter_profanity: bool,
    /// Custom vocabulary.
    pub vocabulary: Vec<String>,
}

impl Default for TranscribeOptions {
    fn default() -> Self {
        Self {
            language: None,
            diarize: true,
            speaker_count: None,
            word_timestamps: true,
            punctuate: true,
            filter_profanity: false,
            vocabulary: Vec::new(),
        }
    }
}

/// Provider for transcription.
#[async_trait]
pub trait TranscribeProvider: Send + Sync {
    /// Transcribe media.
    async fn transcribe(&self, media: &Media, options: &TranscribeOptions) -> Result<Transcript>;

    /// Identify speakers.
    async fn identify_speakers(&self, transcript: &Transcript) -> Result<Vec<Speaker>>;

    /// Detect language.
    async fn detect_language(&self, media: &Media) -> Result<String>;

    /// Get supported languages.
    fn supported_languages(&self) -> Vec<String>;
}

/// The transcription engine.
pub struct TranscribeEngine {
    /// Transcription provider.
    provider: Arc<dyn TranscribeProvider>,
    /// Transcript cache.
    transcripts: Arc<RwLock<HashMap<String, Transcript>>>,
    /// Default options.
    default_options: TranscribeOptions,
}

impl TranscribeEngine {
    /// Create a new transcription engine.
    pub fn new(provider: Arc<dyn TranscribeProvider>) -> Self {
        Self {
            provider,
            transcripts: Arc::new(RwLock::new(HashMap::new())),
            default_options: TranscribeOptions::default(),
        }
    }

    /// Set default options.
    pub fn with_options(mut self, options: TranscribeOptions) -> Self {
        self.default_options = options;
        self
    }

    /// Transcribe media.
    pub async fn transcribe(
        &self,
        media: Media,
        options: Option<TranscribeOptions>,
    ) -> Result<Transcript> {
        let opts = options.unwrap_or_else(|| self.default_options.clone());

        let transcript = self.provider.transcribe(&media, &opts).await?;

        // Cache transcript
        let mut cache = self.transcripts.write().await;
        cache.insert(transcript.id.clone(), transcript.clone());

        Ok(transcript)
    }

    /// Transcribe from file path.
    pub async fn transcribe_file(
        &self,
        path: PathBuf,
        options: Option<TranscribeOptions>,
    ) -> Result<Transcript> {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

        let media_type = match ext.to_lowercase().as_str() {
            "mp3" | "wav" | "m4a" | "flac" | "ogg" | "aac" => MediaType::Audio,
            "mp4" | "mov" | "avi" | "mkv" | "webm" => MediaType::Video,
            _ => return Err(TranscribeError::UnsupportedFormat(ext.to_string())),
        };

        let media = Media {
            id: Uuid::new_v4().to_string(),
            media_type,
            source: path.to_string_lossy().to_string(),
            duration_secs: 0.0, // Would be determined by actual file
            sample_rate: None,
            channels: None,
            format: ext.to_string(),
        };

        self.transcribe(media, options).await
    }

    /// Get transcript by ID.
    pub async fn get_transcript(&self, id: &str) -> Option<Transcript> {
        let cache = self.transcripts.read().await;
        cache.get(id).cloned()
    }

    /// Search transcripts.
    pub async fn search(&self, query: &str) -> Vec<SearchResult> {
        let cache = self.transcripts.read().await;
        let query_lower = query.to_lowercase();

        let mut results = Vec::new();
        for transcript in cache.values() {
            for segment in &transcript.segments {
                if segment.text.to_lowercase().contains(&query_lower) {
                    results.push(SearchResult {
                        transcript_id: transcript.id.clone(),
                        segment_id: segment.id.clone(),
                        text: segment.text.clone(),
                        start: segment.start,
                        end: segment.end,
                        speaker_id: segment.speaker_id.clone(),
                    });
                }
            }
        }
        results
    }

    /// Get speaker statistics.
    pub async fn speaker_stats(&self, transcript_id: &str) -> Option<Vec<SpeakerStats>> {
        let cache = self.transcripts.read().await;
        let transcript = cache.get(transcript_id)?;

        let mut stats: HashMap<String, SpeakerStats> = HashMap::new();

        for segment in &transcript.segments {
            let speaker_id = segment
                .speaker_id
                .clone()
                .unwrap_or_else(|| "unknown".to_string());
            let stat = stats.entry(speaker_id.clone()).or_insert(SpeakerStats {
                speaker_id,
                total_time_secs: 0.0,
                word_count: 0,
                segment_count: 0,
            });

            stat.total_time_secs += segment.end - segment.start;
            stat.word_count += segment.words.len();
            stat.segment_count += 1;
        }

        Some(stats.into_values().collect())
    }

    /// Export transcript as SRT.
    pub async fn export_srt(&self, transcript_id: &str) -> Option<String> {
        let cache = self.transcripts.read().await;
        let transcript = cache.get(transcript_id)?;

        let mut srt = String::new();
        for (i, segment) in transcript.segments.iter().enumerate() {
            srt.push_str(&format!("{}\n", i + 1));
            srt.push_str(&format!(
                "{} --> {}\n",
                format_srt_time(segment.start),
                format_srt_time(segment.end)
            ));
            if let Some(speaker) = &segment.speaker_id {
                srt.push_str(&format!("[{}] ", speaker));
            }
            srt.push_str(&segment.text);
            srt.push_str("\n\n");
        }

        Some(srt)
    }

    /// Get supported languages.
    pub fn supported_languages(&self) -> Vec<String> {
        self.provider.supported_languages()
    }
}

/// Format time for SRT.
fn format_srt_time(secs: f64) -> String {
    let hours = (secs / 3600.0) as u32;
    let minutes = ((secs % 3600.0) / 60.0) as u32;
    let seconds = (secs % 60.0) as u32;
    let millis = ((secs % 1.0) * 1000.0) as u32;
    format!("{:02}:{:02}:{:02},{:03}", hours, minutes, seconds, millis)
}

/// Search result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    /// Transcript ID.
    pub transcript_id: String,
    /// Segment ID.
    pub segment_id: String,
    /// Matched text.
    pub text: String,
    /// Start time.
    pub start: f64,
    /// End time.
    pub end: f64,
    /// Speaker ID.
    pub speaker_id: Option<String>,
}

/// Speaker statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpeakerStats {
    /// Speaker ID.
    pub speaker_id: String,
    /// Total speaking time.
    pub total_time_secs: f64,
    /// Word count.
    pub word_count: usize,
    /// Segment count.
    pub segment_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockProvider;

    #[async_trait]
    impl TranscribeProvider for MockProvider {
        async fn transcribe(
            &self,
            media: &Media,
            _options: &TranscribeOptions,
        ) -> Result<Transcript> {
            Ok(Transcript {
                id: Uuid::new_v4().to_string(),
                media_id: media.id.clone(),
                text: "Hello world. This is a test transcription.".to_string(),
                segments: vec![
                    Segment {
                        id: "seg1".to_string(),
                        start: 0.0,
                        end: 2.0,
                        text: "Hello world.".to_string(),
                        speaker_id: Some("speaker1".to_string()),
                        confidence: 0.95,
                        words: vec![
                            Word {
                                word: "Hello".to_string(),
                                start: 0.0,
                                end: 0.5,
                                confidence: 0.98,
                            },
                            Word {
                                word: "world.".to_string(),
                                start: 0.6,
                                end: 1.0,
                                confidence: 0.97,
                            },
                        ],
                    },
                    Segment {
                        id: "seg2".to_string(),
                        start: 2.5,
                        end: 5.0,
                        text: "This is a test transcription.".to_string(),
                        speaker_id: Some("speaker2".to_string()),
                        confidence: 0.92,
                        words: vec![],
                    },
                ],
                language: "en".to_string(),
                confidence: 0.93,
                speakers: vec![
                    Speaker {
                        id: "speaker1".to_string(),
                        label: "Speaker 1".to_string(),
                        speaking_time_secs: 2.0,
                        segment_count: 1,
                        characteristics: VoiceCharacteristics::default(),
                    },
                    Speaker {
                        id: "speaker2".to_string(),
                        label: "Speaker 2".to_string(),
                        speaking_time_secs: 2.5,
                        segment_count: 1,
                        characteristics: VoiceCharacteristics::default(),
                    },
                ],
                processing_time_ms: 1000,
                created_at: Utc::now(),
            })
        }

        async fn identify_speakers(&self, _transcript: &Transcript) -> Result<Vec<Speaker>> {
            Ok(vec![])
        }

        async fn detect_language(&self, _media: &Media) -> Result<String> {
            Ok("en".to_string())
        }

        fn supported_languages(&self) -> Vec<String> {
            vec!["en".to_string(), "es".to_string(), "fr".to_string()]
        }
    }

    fn create_test_media() -> Media {
        Media {
            id: Uuid::new_v4().to_string(),
            media_type: MediaType::Audio,
            source: "/test/audio.mp3".to_string(),
            duration_secs: 60.0,
            sample_rate: Some(44100),
            channels: Some(2),
            format: "mp3".to_string(),
        }
    }

    #[tokio::test]
    async fn test_transcribe() {
        let provider = Arc::new(MockProvider);
        let engine = TranscribeEngine::new(provider);

        let media = create_test_media();
        let transcript = engine.transcribe(media, None).await.unwrap();

        assert!(!transcript.text.is_empty());
        assert_eq!(transcript.segments.len(), 2);
    }

    #[tokio::test]
    async fn test_search() {
        let provider = Arc::new(MockProvider);
        let engine = TranscribeEngine::new(provider);

        let media = create_test_media();
        engine.transcribe(media, None).await.unwrap();

        let results = engine.search("hello").await;
        assert!(!results.is_empty());
    }

    #[tokio::test]
    async fn test_speaker_stats() {
        let provider = Arc::new(MockProvider);
        let engine = TranscribeEngine::new(provider);

        let media = create_test_media();
        let transcript = engine.transcribe(media, None).await.unwrap();

        let stats = engine.speaker_stats(&transcript.id).await.unwrap();
        assert_eq!(stats.len(), 2);
    }

    #[tokio::test]
    async fn test_export_srt() {
        let provider = Arc::new(MockProvider);
        let engine = TranscribeEngine::new(provider);

        let media = create_test_media();
        let transcript = engine.transcribe(media, None).await.unwrap();

        let srt = engine.export_srt(&transcript.id).await.unwrap();
        assert!(srt.contains("-->"));
        assert!(srt.contains("Hello world"));
    }

    #[test]
    fn test_srt_time_format() {
        assert_eq!(format_srt_time(0.0), "00:00:00,000");
        assert_eq!(format_srt_time(61.5), "00:01:01,500");
        assert_eq!(format_srt_time(3661.123), "01:01:01,123");
    }
}
