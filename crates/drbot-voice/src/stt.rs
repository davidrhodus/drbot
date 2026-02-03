//! Speech-to-text functionality.

use crate::{Result, VoiceError};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Result of transcription.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionResult {
    /// The transcribed text.
    pub text: String,
    /// Confidence score (0-1).
    pub confidence: f32,
    /// Language detected.
    pub language: Option<String>,
    /// Word-level timestamps (if available).
    pub words: Vec<WordTiming>,
    /// Duration of audio in seconds.
    pub duration_secs: f32,
}

/// Word timing information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WordTiming {
    /// The word.
    pub word: String,
    /// Start time in seconds.
    pub start: f32,
    /// End time in seconds.
    pub end: f32,
    /// Confidence for this word.
    pub confidence: f32,
}

/// Speech-to-text provider trait.
#[async_trait]
pub trait SttProvider: Send + Sync {
    /// Provider name.
    fn name(&self) -> &str;

    /// Transcribe audio to text.
    async fn transcribe(&self, audio: &[u8], format: &str) -> Result<TranscriptionResult>;

    /// Supported audio formats.
    fn supported_formats(&self) -> Vec<String>;
}

/// Combined STT interface.
pub struct SpeechToText {
    provider: Box<dyn SttProvider>,
}

impl SpeechToText {
    /// Create with a provider.
    pub fn new(provider: Box<dyn SttProvider>) -> Self {
        Self { provider }
    }

    /// Transcribe audio.
    pub async fn transcribe(&self, audio: &[u8], format: &str) -> Result<TranscriptionResult> {
        self.provider.transcribe(audio, format).await
    }

    /// Get provider name.
    pub fn provider_name(&self) -> &str {
        self.provider.name()
    }
}

/// Whisper-based STT (via OpenAI API or local).
pub struct WhisperStt {
    api_key: Option<String>,
    api_url: String,
    model: String,
}

impl WhisperStt {
    /// Create Whisper STT with OpenAI API.
    pub fn openai(api_key: &str) -> Self {
        Self {
            api_key: Some(api_key.to_string()),
            api_url: "https://api.openai.com/v1/audio/transcriptions".to_string(),
            model: "whisper-1".to_string(),
        }
    }

    /// Create Whisper STT with local server.
    pub fn local(url: &str) -> Self {
        Self {
            api_key: None,
            api_url: url.to_string(),
            model: "whisper".to_string(),
        }
    }
}

#[async_trait]
impl SttProvider for WhisperStt {
    fn name(&self) -> &str {
        "whisper"
    }

    async fn transcribe(&self, audio: &[u8], format: &str) -> Result<TranscriptionResult> {
        let client = reqwest::Client::new();

        // Create multipart form
        let file_part = reqwest::multipart::Part::bytes(audio.to_vec())
            .file_name(format!("audio.{}", format))
            .mime_str(&format!("audio/{}", format))
            .map_err(|e| VoiceError::TranscriptionFailed(e.to_string()))?;

        let form = reqwest::multipart::Form::new()
            .part("file", file_part)
            .text("model", self.model.clone())
            .text("response_format", "verbose_json");

        let mut request = client.post(&self.api_url);

        if let Some(api_key) = &self.api_key {
            request = request.header("Authorization", format!("Bearer {}", api_key));
        }

        let response = request
            .multipart(form)
            .send()
            .await
            .map_err(|e| VoiceError::TranscriptionFailed(e.to_string()))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(VoiceError::TranscriptionFailed(error_text));
        }

        let json: serde_json::Value = response
            .json()
            .await
            .map_err(|e| VoiceError::TranscriptionFailed(e.to_string()))?;

        let text = json["text"].as_str().unwrap_or("").to_string();
        let language = json["language"].as_str().map(|s| s.to_string());
        let duration = json["duration"].as_f64().unwrap_or(0.0) as f32;

        // Parse word timings if available
        let words = if let Some(segments) = json["segments"].as_array() {
            segments
                .iter()
                .flat_map(|seg| {
                    seg["words"]
                        .as_array()
                        .map(|words| {
                            words
                                .iter()
                                .filter_map(|w| {
                                    Some(WordTiming {
                                        word: w["word"].as_str()?.to_string(),
                                        start: w["start"].as_f64()? as f32,
                                        end: w["end"].as_f64()? as f32,
                                        confidence: w["probability"].as_f64().unwrap_or(1.0) as f32,
                                    })
                                })
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default()
                })
                .collect()
        } else {
            vec![]
        };

        Ok(TranscriptionResult {
            text,
            confidence: 1.0, // Whisper doesn't provide overall confidence
            language,
            words,
            duration_secs: duration,
        })
    }

    fn supported_formats(&self) -> Vec<String> {
        vec![
            "mp3".to_string(),
            "mp4".to_string(),
            "mpeg".to_string(),
            "mpga".to_string(),
            "m4a".to_string(),
            "wav".to_string(),
            "webm".to_string(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transcription_result() {
        let result = TranscriptionResult {
            text: "Hello world".to_string(),
            confidence: 0.95,
            language: Some("en".to_string()),
            words: vec![],
            duration_secs: 1.5,
        };
        assert_eq!(result.text, "Hello world");
    }
}
