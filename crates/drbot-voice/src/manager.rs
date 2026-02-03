//! Unified voice manager for real-time voice mode.

use crate::{
    AudioBuffer, AudioFormat, AudioProcessor, Result, SpeechToText, SttProvider, TextToSpeech,
    TtsProvider, VadResult, VoiceActivityDetector, VoiceConfig, VoiceError, WhisperStt,
};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, info, warn};

/// Voice mode state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceState {
    /// Voice mode is disabled.
    Disabled,
    /// Ready but not listening.
    Idle,
    /// Actively listening for speech.
    Listening,
    /// Processing speech.
    Processing,
    /// Playing response.
    Speaking,
}

/// Voice event emitted by the manager.
#[derive(Debug, Clone)]
pub enum VoiceEvent {
    /// Voice mode state changed.
    StateChanged(VoiceState),
    /// Speech detected and transcribed.
    Transcription { text: String, is_final: bool },
    /// Speech synthesis completed.
    SynthesisReady { audio: Vec<u8>, format: String },
    /// Error occurred.
    Error(String),
}

/// Unified voice manager.
pub struct VoiceManager {
    config: VoiceConfig,
    state: Arc<RwLock<VoiceState>>,
    stt: Option<SpeechToText>,
    tts: Option<TextToSpeech>,
    vad: Arc<RwLock<VoiceActivityDetector>>,
    processor: Arc<RwLock<AudioProcessor>>,
    event_tx: mpsc::Sender<VoiceEvent>,
    event_rx: Option<mpsc::Receiver<VoiceEvent>>,
}

impl VoiceManager {
    /// Create a new voice manager.
    pub fn new(config: VoiceConfig) -> Self {
        let (event_tx, event_rx) = mpsc::channel(32);
        let format = AudioFormat {
            sample_rate: config.sample_rate,
            channels: 1,
            bits_per_sample: 16,
        };

        Self {
            config,
            state: Arc::new(RwLock::new(VoiceState::Disabled)),
            stt: None,
            tts: None,
            vad: Arc::new(RwLock::new(VoiceActivityDetector::new(format.sample_rate))),
            processor: Arc::new(RwLock::new(AudioProcessor::new(format, format))),
            event_tx,
            event_rx: Some(event_rx),
        }
    }

    /// Initialize with providers.
    pub async fn initialize(
        &mut self,
        stt_provider: Box<dyn SttProvider>,
        tts_provider: Box<dyn TtsProvider>,
    ) -> Result<()> {
        self.stt = Some(SpeechToText::new(stt_provider));
        self.tts = Some(TextToSpeech::new(tts_provider));

        let mut state = self.state.write().await;
        *state = VoiceState::Idle;

        info!("Voice manager initialized");
        Ok(())
    }

    /// Initialize with default providers (requires API keys).
    pub async fn initialize_defaults(&mut self, openai_key: Option<&str>) -> Result<()> {
        // STT with Whisper
        let stt: Box<dyn SttProvider> = if let Some(key) = openai_key {
            Box::new(WhisperStt::openai(key))
        } else {
            return Err(VoiceError::ProviderError("No API key provided".to_string()));
        };

        // TTS with OpenAI or System
        let tts: Box<dyn TtsProvider> = if let Some(key) = openai_key {
            Box::new(crate::tts::OpenAiTts::new(key))
        } else {
            Box::new(crate::tts::SystemTts)
        };

        self.initialize(stt, tts).await
    }

    /// Take the event receiver (can only be called once).
    pub fn take_event_receiver(&mut self) -> Option<mpsc::Receiver<VoiceEvent>> {
        self.event_rx.take()
    }

    /// Get current state.
    pub async fn state(&self) -> VoiceState {
        *self.state.read().await
    }

    /// Start listening for voice input.
    pub async fn start_listening(&self) -> Result<()> {
        let state = *self.state.read().await;
        if state == VoiceState::Disabled {
            return Err(VoiceError::ProviderError(
                "Voice not initialized".to_string(),
            ));
        }

        let mut state = self.state.write().await;
        *state = VoiceState::Listening;

        self.emit(VoiceEvent::StateChanged(VoiceState::Listening))
            .await;
        info!("Started listening");

        Ok(())
    }

    /// Stop listening.
    pub async fn stop_listening(&self) -> Result<()> {
        let mut state = self.state.write().await;
        *state = VoiceState::Idle;

        self.emit(VoiceEvent::StateChanged(VoiceState::Idle)).await;
        info!("Stopped listening");

        Ok(())
    }

    /// Process incoming audio chunk.
    pub async fn process_audio(&self, chunk: &[u8]) -> Result<()> {
        let state = *self.state.read().await;
        if state != VoiceState::Listening {
            return Ok(());
        }

        let format = AudioFormat {
            sample_rate: self.config.sample_rate,
            channels: 1,
            bits_per_sample: 16,
        };
        let buffer = AudioBuffer::from_bytes(chunk, format);

        // Voice activity detection
        let vad_result = {
            let mut vad = self.vad.write().await;
            vad.process(&buffer)
        };

        match vad_result {
            VadResult::SpeechStart => {
                debug!("Speech started");
            }
            VadResult::SpeechEnd => {
                debug!("Speech ended, processing...");
                self.process_speech().await?;
            }
            VadResult::Speech => {
                // Continue accumulating audio
                let mut processor = self.processor.write().await;
                processor.process(chunk);
            }
            VadResult::Silence => {
                // Nothing to do
            }
        }

        Ok(())
    }

    /// Process accumulated speech.
    async fn process_speech(&self) -> Result<()> {
        {
            let mut state = self.state.write().await;
            *state = VoiceState::Processing;
        }
        self.emit(VoiceEvent::StateChanged(VoiceState::Processing))
            .await;

        // Get accumulated audio
        let audio = {
            let mut processor = self.processor.write().await;
            processor.flush()
        };

        if let Some(buffer) = audio {
            let audio_bytes = buffer.to_bytes();

            // Transcribe
            if let Some(stt) = &self.stt {
                match stt.transcribe(&audio_bytes, "wav").await {
                    Ok(result) => {
                        info!(text = %result.text, "Transcription complete");
                        self.emit(VoiceEvent::Transcription {
                            text: result.text,
                            is_final: true,
                        })
                        .await;
                    }
                    Err(e) => {
                        warn!("Transcription failed: {}", e);
                        self.emit(VoiceEvent::Error(e.to_string())).await;
                    }
                }
            }
        }

        // Return to listening state
        {
            let mut state = self.state.write().await;
            *state = VoiceState::Listening;
        }
        self.emit(VoiceEvent::StateChanged(VoiceState::Listening))
            .await;

        Ok(())
    }

    /// Speak text aloud.
    pub async fn speak(&self, text: &str) -> Result<()> {
        let tts = self
            .tts
            .as_ref()
            .ok_or_else(|| VoiceError::ProviderError("TTS not initialized".to_string()))?;

        {
            let mut state = self.state.write().await;
            *state = VoiceState::Speaking;
        }
        self.emit(VoiceEvent::StateChanged(VoiceState::Speaking))
            .await;

        match tts.synthesize(text, None).await {
            Ok(result) => {
                info!(duration = result.duration_secs, "Speech synthesized");
                self.emit(VoiceEvent::SynthesisReady {
                    audio: result.audio,
                    format: result.format,
                })
                .await;
            }
            Err(e) => {
                warn!("Speech synthesis failed: {}", e);
                self.emit(VoiceEvent::Error(e.to_string())).await;
            }
        }

        // Return to idle or listening
        {
            let mut state = self.state.write().await;
            *state = VoiceState::Idle;
        }
        self.emit(VoiceEvent::StateChanged(VoiceState::Idle)).await;

        Ok(())
    }

    /// Emit an event.
    async fn emit(&self, event: VoiceEvent) {
        let _ = self.event_tx.send(event).await;
    }
}

/// Builder for VoiceManager.
pub struct VoiceManagerBuilder {
    config: VoiceConfig,
    openai_key: Option<String>,
}

impl VoiceManagerBuilder {
    /// Create a new builder.
    pub fn new() -> Self {
        Self {
            config: VoiceConfig::default(),
            openai_key: None,
        }
    }

    /// Set configuration.
    pub fn with_config(mut self, config: VoiceConfig) -> Self {
        self.config = config;
        self
    }

    /// Set OpenAI API key.
    pub fn with_openai_key(mut self, key: impl Into<String>) -> Self {
        self.openai_key = Some(key.into());
        self
    }

    /// Build the manager.
    pub async fn build(self) -> Result<VoiceManager> {
        let mut manager = VoiceManager::new(self.config);

        if let Some(key) = self.openai_key {
            manager.initialize_defaults(Some(&key)).await?;
        }

        Ok(manager)
    }
}

impl Default for VoiceManagerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_voice_state() {
        let state = VoiceState::Idle;
        assert_ne!(state, VoiceState::Listening);
    }

    #[tokio::test]
    async fn test_manager_creation() {
        let config = VoiceConfig::default();
        let manager = VoiceManager::new(config);

        assert_eq!(manager.state().await, VoiceState::Disabled);
    }

    #[test]
    fn test_builder() {
        let builder = VoiceManagerBuilder::new().with_openai_key("test-key");

        assert!(builder.openai_key.is_some());
    }
}
