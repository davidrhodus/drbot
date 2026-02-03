//! Voice I/O for drbot.
//!
//! Provides speech-to-text (STT) and text-to-speech (TTS) capabilities.

mod audio;
mod conversation;
mod manager;
mod stt;
mod tts;
mod wakeword;

pub use audio::{AudioBuffer, AudioFormat, AudioProcessor, VadResult, VoiceActivityDetector};
pub use conversation::{
    ConversationConfig, ConversationEvent, ConversationState, EndReason, VoiceConversation,
    VoiceConversationManager, VoiceTurn,
};
pub use manager::{VoiceEvent, VoiceManager, VoiceManagerBuilder, VoiceState};
pub use stt::{SpeechToText, SttProvider, TranscriptionResult, WhisperStt};
pub use tts::{
    ElevenLabsTts, ElevenLabsVoiceSettings, OpenAiTts, SynthesisResult, SystemTts, TextToSpeech,
    TtsProvider, TtsVoice,
};
#[cfg(feature = "porcupine")]
pub use wakeword::PorcupineWakeWordDetector;
#[cfg(feature = "vosk")]
pub use wakeword::VoskWakeWordDetector;
pub use wakeword::{
    ContinuousMode, DetectorState, PorcupineConfig, SimpleWakeWordDetector, VoskConfig,
    WakeWordBackend, WakeWordConfig, WakeWordDetector, WakeWordEvent, WakeWordModel,
};

use serde::{Deserialize, Serialize};

/// Voice processing result.
pub type Result<T> = std::result::Result<T, VoiceError>;

/// Voice processing errors.
#[derive(Debug, thiserror::Error)]
pub enum VoiceError {
    #[error("Transcription failed: {0}")]
    TranscriptionFailed(String),
    #[error("Synthesis failed: {0}")]
    SynthesisFailed(String),
    #[error("Invalid audio format: {0}")]
    InvalidFormat(String),
    #[error("Provider error: {0}")]
    ProviderError(String),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Voice configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceConfig {
    /// STT provider to use.
    pub stt_provider: String,
    /// TTS provider to use.
    pub tts_provider: String,
    /// Default voice for TTS.
    pub default_voice: Option<String>,
    /// Sample rate for audio.
    pub sample_rate: u32,
    /// Language code (e.g., "en-US").
    pub language: String,
}

impl Default for VoiceConfig {
    fn default() -> Self {
        Self {
            stt_provider: "whisper".to_string(),
            tts_provider: "system".to_string(),
            default_voice: None,
            sample_rate: 16000,
            language: "en-US".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_voice_config_default() {
        let config = VoiceConfig::default();
        assert_eq!(config.sample_rate, 16000);
        assert_eq!(config.language, "en-US");
    }
}
