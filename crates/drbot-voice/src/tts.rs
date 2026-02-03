//! Text-to-speech functionality.

use crate::{Result, VoiceError};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Available TTS voice.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtsVoice {
    /// Voice ID.
    pub id: String,
    /// Voice name.
    pub name: String,
    /// Language code.
    pub language: String,
    /// Gender.
    pub gender: Option<String>,
    /// Description.
    pub description: Option<String>,
    /// Preview URL (if available).
    pub preview_url: Option<String>,
}

/// Result of speech synthesis.
#[derive(Debug, Clone)]
pub struct SynthesisResult {
    /// Audio data.
    pub audio: Vec<u8>,
    /// Audio format.
    pub format: String,
    /// Sample rate.
    pub sample_rate: u32,
    /// Duration in seconds.
    pub duration_secs: f32,
}

/// TTS provider trait.
#[async_trait]
pub trait TtsProvider: Send + Sync {
    /// Provider name.
    fn name(&self) -> &str;

    /// Synthesize text to speech.
    async fn synthesize(&self, text: &str, voice: &str) -> Result<SynthesisResult>;

    /// List available voices.
    async fn list_voices(&self) -> Result<Vec<TtsVoice>>;
}

/// Combined TTS interface.
pub struct TextToSpeech {
    provider: Box<dyn TtsProvider>,
    default_voice: Option<String>,
}

impl TextToSpeech {
    /// Create with a provider.
    pub fn new(provider: Box<dyn TtsProvider>) -> Self {
        Self {
            provider,
            default_voice: None,
        }
    }

    /// Set default voice.
    pub fn with_default_voice(mut self, voice: &str) -> Self {
        self.default_voice = Some(voice.to_string());
        self
    }

    /// Synthesize text.
    pub async fn synthesize(&self, text: &str, voice: Option<&str>) -> Result<SynthesisResult> {
        let voice = voice
            .map(|v| v.to_string())
            .or_else(|| self.default_voice.clone())
            .unwrap_or_else(|| "default".to_string());
        self.provider.synthesize(text, &voice).await
    }

    /// List available voices.
    pub async fn list_voices(&self) -> Result<Vec<TtsVoice>> {
        self.provider.list_voices().await
    }
}

/// OpenAI TTS provider.
pub struct OpenAiTts {
    api_key: String,
    model: String,
}

impl OpenAiTts {
    /// Create OpenAI TTS.
    pub fn new(api_key: &str) -> Self {
        Self {
            api_key: api_key.to_string(),
            model: "tts-1".to_string(),
        }
    }

    /// Use HD model.
    pub fn hd(mut self) -> Self {
        self.model = "tts-1-hd".to_string();
        self
    }
}

#[async_trait]
impl TtsProvider for OpenAiTts {
    fn name(&self) -> &str {
        "openai"
    }

    async fn synthesize(&self, text: &str, voice: &str) -> Result<SynthesisResult> {
        let client = reqwest::Client::new();

        let body = serde_json::json!({
            "model": self.model,
            "input": text,
            "voice": voice,
            "response_format": "mp3"
        });

        let response = client
            .post("https://api.openai.com/v1/audio/speech")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| VoiceError::SynthesisFailed(e.to_string()))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(VoiceError::SynthesisFailed(error_text));
        }

        let audio = response
            .bytes()
            .await
            .map_err(|e| VoiceError::SynthesisFailed(e.to_string()))?
            .to_vec();

        // Estimate duration (rough approximation for MP3)
        let duration_secs = audio.len() as f32 / 16000.0;

        Ok(SynthesisResult {
            audio,
            format: "mp3".to_string(),
            sample_rate: 24000,
            duration_secs,
        })
    }

    async fn list_voices(&self) -> Result<Vec<TtsVoice>> {
        // OpenAI has fixed voices
        Ok(vec![
            TtsVoice {
                id: "alloy".to_string(),
                name: "Alloy".to_string(),
                language: "en".to_string(),
                gender: Some("neutral".to_string()),
                description: Some("Versatile, balanced voice".to_string()),
                preview_url: None,
            },
            TtsVoice {
                id: "echo".to_string(),
                name: "Echo".to_string(),
                language: "en".to_string(),
                gender: Some("male".to_string()),
                description: Some("Warm, confident voice".to_string()),
                preview_url: None,
            },
            TtsVoice {
                id: "fable".to_string(),
                name: "Fable".to_string(),
                language: "en".to_string(),
                gender: Some("male".to_string()),
                description: Some("Expressive, narrative voice".to_string()),
                preview_url: None,
            },
            TtsVoice {
                id: "onyx".to_string(),
                name: "Onyx".to_string(),
                language: "en".to_string(),
                gender: Some("male".to_string()),
                description: Some("Deep, authoritative voice".to_string()),
                preview_url: None,
            },
            TtsVoice {
                id: "nova".to_string(),
                name: "Nova".to_string(),
                language: "en".to_string(),
                gender: Some("female".to_string()),
                description: Some("Friendly, conversational voice".to_string()),
                preview_url: None,
            },
            TtsVoice {
                id: "shimmer".to_string(),
                name: "Shimmer".to_string(),
                language: "en".to_string(),
                gender: Some("female".to_string()),
                description: Some("Clear, expressive voice".to_string()),
                preview_url: None,
            },
        ])
    }
}

/// System TTS (uses OS native TTS).
pub struct SystemTts;

#[async_trait]
impl TtsProvider for SystemTts {
    fn name(&self) -> &str {
        "system"
    }

    async fn synthesize(&self, text: &str, _voice: &str) -> Result<SynthesisResult> {
        // Use system command to generate speech
        #[cfg(target_os = "macos")]
        {
            use std::process::Command;

            let temp_file = std::env::temp_dir().join("drbot_tts.aiff");
            let output = Command::new("say")
                .args(["-o", temp_file.to_str().unwrap(), text])
                .output()
                .map_err(|e| VoiceError::SynthesisFailed(e.to_string()))?;

            if !output.status.success() {
                return Err(VoiceError::SynthesisFailed(
                    String::from_utf8_lossy(&output.stderr).to_string(),
                ));
            }

            let audio = std::fs::read(&temp_file)
                .map_err(|e| VoiceError::SynthesisFailed(e.to_string()))?;
            let _ = std::fs::remove_file(&temp_file);
            let duration_secs = audio.len() as f32 / 44100.0;

            return Ok(SynthesisResult {
                audio,
                format: "aiff".to_string(),
                sample_rate: 22050,
                duration_secs,
            });
        }

        #[cfg(not(target_os = "macos"))]
        {
            Err(VoiceError::SynthesisFailed(
                "System TTS not supported on this platform".to_string(),
            ))
        }
    }

    async fn list_voices(&self) -> Result<Vec<TtsVoice>> {
        #[cfg(target_os = "macos")]
        {
            use std::process::Command;

            let output = Command::new("say")
                .args(["-v", "?"])
                .output()
                .map_err(|e| VoiceError::SynthesisFailed(e.to_string()))?;

            let voices: Vec<TtsVoice> = String::from_utf8_lossy(&output.stdout)
                .lines()
                .filter_map(|line| {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        Some(TtsVoice {
                            id: parts[0].to_string(),
                            name: parts[0].to_string(),
                            language: parts[1].to_string(),
                            gender: None,
                            description: None,
                            preview_url: None,
                        })
                    } else {
                        None
                    }
                })
                .collect();

            return Ok(voices);
        }

        #[cfg(not(target_os = "macos"))]
        {
            Ok(vec![])
        }
    }
}

/// ElevenLabs TTS provider.
pub struct ElevenLabsTts {
    api_key: String,
    model_id: String,
    base_url: String,
}

impl ElevenLabsTts {
    /// Create ElevenLabs TTS.
    pub fn new(api_key: &str) -> Self {
        Self {
            api_key: api_key.to_string(),
            model_id: "eleven_monolingual_v1".to_string(),
            base_url: "https://api.elevenlabs.io/v1".to_string(),
        }
    }

    /// Use multilingual model.
    pub fn multilingual(mut self) -> Self {
        self.model_id = "eleven_multilingual_v2".to_string();
        self
    }

    /// Use turbo model for faster synthesis.
    pub fn turbo(mut self) -> Self {
        self.model_id = "eleven_turbo_v2".to_string();
        self
    }

    /// Set custom model ID.
    pub fn with_model(mut self, model_id: &str) -> Self {
        self.model_id = model_id.to_string();
        self
    }
}

#[async_trait]
impl TtsProvider for ElevenLabsTts {
    fn name(&self) -> &str {
        "elevenlabs"
    }

    async fn synthesize(&self, text: &str, voice: &str) -> Result<SynthesisResult> {
        let client = reqwest::Client::new();

        let body = serde_json::json!({
            "text": text,
            "model_id": self.model_id,
            "voice_settings": {
                "stability": 0.5,
                "similarity_boost": 0.75,
                "style": 0.0,
                "use_speaker_boost": true
            }
        });

        let url = format!("{}/text-to-speech/{}", self.base_url, voice);

        let response = client
            .post(&url)
            .header("xi-api-key", &self.api_key)
            .header("Content-Type", "application/json")
            .header("Accept", "audio/mpeg")
            .json(&body)
            .send()
            .await
            .map_err(|e| VoiceError::SynthesisFailed(e.to_string()))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(VoiceError::SynthesisFailed(format!(
                "ElevenLabs API error: {}",
                error_text
            )));
        }

        let audio = response
            .bytes()
            .await
            .map_err(|e| VoiceError::SynthesisFailed(e.to_string()))?
            .to_vec();

        // Estimate duration (rough approximation for MP3 at 128kbps)
        let duration_secs = audio.len() as f32 / 16000.0;

        Ok(SynthesisResult {
            audio,
            format: "mp3".to_string(),
            sample_rate: 44100,
            duration_secs,
        })
    }

    async fn list_voices(&self) -> Result<Vec<TtsVoice>> {
        let client = reqwest::Client::new();

        let response = client
            .get(&format!("{}/voices", self.base_url))
            .header("xi-api-key", &self.api_key)
            .send()
            .await
            .map_err(|e| VoiceError::ProviderError(e.to_string()))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(VoiceError::ProviderError(format!(
                "ElevenLabs API error: {}",
                error_text
            )));
        }

        #[derive(Deserialize)]
        struct VoicesResponse {
            voices: Vec<ElevenLabsVoice>,
        }

        #[derive(Deserialize)]
        struct ElevenLabsVoice {
            voice_id: String,
            name: String,
            #[serde(default)]
            labels: std::collections::HashMap<String, String>,
            preview_url: Option<String>,
            description: Option<String>,
        }

        let data: VoicesResponse = response
            .json()
            .await
            .map_err(|e| VoiceError::ProviderError(e.to_string()))?;

        let voices = data
            .voices
            .into_iter()
            .map(|v| {
                let gender = v.labels.get("gender").cloned();
                let language = v
                    .labels
                    .get("language")
                    .cloned()
                    .unwrap_or_else(|| "en".to_string());

                TtsVoice {
                    id: v.voice_id,
                    name: v.name,
                    language,
                    gender,
                    description: v.description,
                    preview_url: v.preview_url,
                }
            })
            .collect();

        Ok(voices)
    }
}

/// ElevenLabs voice settings for fine-tuning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElevenLabsVoiceSettings {
    /// Stability (0.0 - 1.0). Higher values make the voice more consistent.
    pub stability: f32,
    /// Similarity boost (0.0 - 1.0). Higher values make the voice closer to original.
    pub similarity_boost: f32,
    /// Style (0.0 - 1.0). Higher values make the voice more expressive.
    pub style: f32,
    /// Use speaker boost for clarity.
    pub use_speaker_boost: bool,
}

impl Default for ElevenLabsVoiceSettings {
    fn default() -> Self {
        Self {
            stability: 0.5,
            similarity_boost: 0.75,
            style: 0.0,
            use_speaker_boost: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tts_voice() {
        let voice = TtsVoice {
            id: "test".to_string(),
            name: "Test Voice".to_string(),
            language: "en".to_string(),
            gender: Some("neutral".to_string()),
            description: None,
            preview_url: None,
        };
        assert_eq!(voice.id, "test");
    }

    #[test]
    fn test_elevenlabs_tts_config() {
        let tts = ElevenLabsTts::new("test_key")
            .multilingual()
            .with_model("custom_model");
        assert_eq!(tts.model_id, "custom_model");
    }

    #[test]
    fn test_elevenlabs_voice_settings() {
        let settings = ElevenLabsVoiceSettings::default();
        assert_eq!(settings.stability, 0.5);
        assert!(settings.use_speaker_boost);
    }
}
