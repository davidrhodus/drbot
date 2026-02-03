//! Wake word detection for voice activation.
//!
//! Provides always-on listening for wake word detection.

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};

/// Wake word configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WakeWordConfig {
    /// Wake word phrases.
    pub wake_words: Vec<String>,
    /// Sensitivity (0.0 - 1.0).
    pub sensitivity: f32,
    /// Enable/disable.
    pub enabled: bool,
    /// Audio sample rate.
    pub sample_rate: u32,
    /// Detection model.
    pub model: WakeWordModel,
    /// Require confirmation after wake.
    pub require_confirmation: bool,
    /// Timeout after wake word (seconds).
    pub listen_timeout_secs: u32,
}

impl Default for WakeWordConfig {
    fn default() -> Self {
        Self {
            wake_words: vec![
                "hey doctor".to_string(),
                "hey dr bot".to_string(),
                "okay doctor".to_string(),
            ],
            sensitivity: 0.5,
            enabled: true,
            sample_rate: 16000,
            model: WakeWordModel::Builtin,
            require_confirmation: false,
            listen_timeout_secs: 10,
        }
    }
}

/// Wake word detection model/backend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WakeWordModel {
    /// Built-in simple detector (energy-based, demo only).
    Builtin,
    /// Porcupine wake word engine (Picovoice).
    Porcupine,
    /// Vosk offline speech recognition.
    Vosk,
    /// Custom model path.
    Custom,
}

/// Porcupine-specific configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PorcupineConfig {
    /// Picovoice access key.
    pub access_key: String,
    /// Path to custom keyword files (.ppn).
    pub keyword_paths: Vec<String>,
    /// Built-in keywords to use.
    pub builtin_keywords: Vec<String>,
    /// Sensitivities for each keyword (0.0 - 1.0).
    pub sensitivities: Vec<f32>,
    /// Path to custom model file (.pv).
    pub model_path: Option<String>,
}

impl Default for PorcupineConfig {
    fn default() -> Self {
        Self {
            access_key: String::new(),
            keyword_paths: Vec::new(),
            builtin_keywords: vec!["computer".to_string()],
            sensitivities: vec![0.5],
            model_path: None,
        }
    }
}

/// Vosk-specific configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoskConfig {
    /// Path to Vosk model directory.
    pub model_path: String,
    /// Sample rate (must match audio input).
    pub sample_rate: u32,
    /// Wake phrases to detect.
    pub wake_phrases: Vec<String>,
    /// Minimum confidence threshold (0.0 - 1.0).
    pub min_confidence: f32,
}

impl Default for VoskConfig {
    fn default() -> Self {
        Self {
            model_path: "~/.config/drbot/vosk-model".to_string(),
            sample_rate: 16000,
            wake_phrases: vec!["hey doctor".to_string(), "hey dr bot".to_string()],
            min_confidence: 0.7,
        }
    }
}

/// Wake word backend abstraction.
pub enum WakeWordBackend {
    /// Simple built-in detector.
    Simple(SimpleWakeWordDetector),
    /// Porcupine backend (requires feature flag).
    #[cfg(feature = "porcupine")]
    Porcupine(PorcupineWakeWordDetector),
    /// Vosk backend (requires feature flag).
    #[cfg(feature = "vosk")]
    Vosk(VoskWakeWordDetector),
}

/// Simple energy-based wake word detector (demo/fallback).
pub struct SimpleWakeWordDetector {
    config: WakeWordConfig,
    threshold: f32,
}

impl SimpleWakeWordDetector {
    /// Create a new simple detector.
    pub fn new(config: WakeWordConfig) -> Self {
        let threshold = 0.05 * (1.0 - config.sensitivity);
        Self { config, threshold }
    }

    /// Process audio and check for wake word.
    pub fn process(&self, samples: &[f32]) -> Option<(String, f32)> {
        if samples.len() < 8000 {
            return None;
        }

        let recent = &samples[samples.len().saturating_sub(8000)..];
        let energy: f32 = recent.iter().map(|s| s.abs()).sum::<f32>() / recent.len() as f32;

        if energy > self.threshold {
            // Simple pattern matching would go here
            // For now, just return the first wake word with confidence based on energy
            let confidence = (energy / 0.1).min(1.0);
            if confidence > 0.5 {
                return Some((self.config.wake_words[0].clone(), confidence));
            }
        }

        None
    }
}

/// Porcupine wake word detector wrapper.
#[cfg(feature = "porcupine")]
pub struct PorcupineWakeWordDetector {
    config: PorcupineConfig,
    // In a real implementation, this would hold the Porcupine instance
    // porcupine: porcupine::Porcupine,
}

#[cfg(feature = "porcupine")]
impl PorcupineWakeWordDetector {
    /// Create a new Porcupine detector.
    pub fn new(config: PorcupineConfig) -> crate::Result<Self> {
        // Real implementation would initialize Porcupine here:
        // let porcupine = porcupine::PorcupineBuilder::new_with_keywords(
        //     &config.access_key,
        //     &config.builtin_keywords,
        // )
        // .sensitivities(&config.sensitivities)
        // .init()
        // .map_err(|e| crate::VoiceError::ProviderError(e.to_string()))?;

        Ok(Self { config })
    }

    /// Process audio frame and check for wake word.
    pub fn process(&self, samples: &[i16]) -> Option<(String, f32)> {
        // Real implementation would call:
        // match self.porcupine.process(samples) {
        //     Ok(keyword_index) if keyword_index >= 0 => {
        //         let keyword = &self.config.builtin_keywords[keyword_index as usize];
        //         Some((keyword.clone(), 1.0))
        //     }
        //     _ => None,
        // }
        None
    }

    /// Get required frame length.
    pub fn frame_length(&self) -> usize {
        512 // Porcupine default
    }

    /// Get required sample rate.
    pub fn sample_rate(&self) -> u32 {
        16000 // Porcupine requirement
    }
}

/// Vosk wake word detector wrapper.
#[cfg(feature = "vosk")]
pub struct VoskWakeWordDetector {
    config: VoskConfig,
    // In a real implementation, this would hold the Vosk recognizer
    // recognizer: vosk::Recognizer,
}

#[cfg(feature = "vosk")]
impl VoskWakeWordDetector {
    /// Create a new Vosk detector.
    pub fn new(config: VoskConfig) -> crate::Result<Self> {
        // Real implementation would initialize Vosk here:
        // let model = vosk::Model::new(&config.model_path)
        //     .map_err(|e| crate::VoiceError::ProviderError(e.to_string()))?;
        // let recognizer = vosk::Recognizer::new(&model, config.sample_rate as f32)
        //     .map_err(|e| crate::VoiceError::ProviderError(e.to_string()))?;

        Ok(Self { config })
    }

    /// Process audio and check for wake phrase.
    pub fn process(&self, samples: &[i16]) -> Option<(String, f32)> {
        // Real implementation would call:
        // self.recognizer.accept_waveform(samples);
        // if let Some(result) = self.recognizer.partial_result() {
        //     for phrase in &self.config.wake_phrases {
        //         if result.partial.to_lowercase().contains(&phrase.to_lowercase()) {
        //             return Some((phrase.clone(), result.confidence));
        //         }
        //     }
        // }
        None
    }

    /// Reset the recognizer for a new utterance.
    pub fn reset(&mut self) {
        // self.recognizer.reset();
    }
}

/// Wake word detection event.
#[derive(Debug, Clone)]
pub struct WakeWordEvent {
    /// Detected wake word.
    pub wake_word: String,
    /// Confidence score.
    pub confidence: f32,
    /// Timestamp (microseconds since start).
    pub timestamp_us: u64,
    /// Audio buffer with wake word.
    pub audio: Option<Vec<f32>>,
}

/// Wake word detector state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetectorState {
    /// Not initialized.
    Uninitialized,
    /// Listening for wake word.
    Listening,
    /// Wake word detected, waiting for command.
    Activated,
    /// Processing command.
    Processing,
    /// Paused.
    Paused,
    /// Error state.
    Error,
}

/// Wake word detector.
pub struct WakeWordDetector {
    config: WakeWordConfig,
    state: Arc<RwLock<DetectorState>>,
    event_sender: broadcast::Sender<WakeWordEvent>,
    audio_buffer: Arc<RwLock<Vec<f32>>>,
    sample_count: Arc<RwLock<u64>>,
}

impl WakeWordDetector {
    /// Create a new wake word detector.
    pub fn new(config: WakeWordConfig) -> Self {
        let (sender, _) = broadcast::channel(16);
        Self {
            config,
            state: Arc::new(RwLock::new(DetectorState::Uninitialized)),
            event_sender: sender,
            audio_buffer: Arc::new(RwLock::new(Vec::new())),
            sample_count: Arc::new(RwLock::new(0)),
        }
    }

    /// Start the detector.
    pub async fn start(&self) -> crate::Result<()> {
        let mut state = self.state.write().await;
        *state = DetectorState::Listening;
        Ok(())
    }

    /// Stop the detector.
    pub async fn stop(&self) {
        let mut state = self.state.write().await;
        *state = DetectorState::Uninitialized;
    }

    /// Pause detection.
    pub async fn pause(&self) {
        let mut state = self.state.write().await;
        if *state == DetectorState::Listening {
            *state = DetectorState::Paused;
        }
    }

    /// Resume detection.
    pub async fn resume(&self) {
        let mut state = self.state.write().await;
        if *state == DetectorState::Paused {
            *state = DetectorState::Listening;
        }
    }

    /// Get current state.
    pub async fn state(&self) -> DetectorState {
        *self.state.read().await
    }

    /// Subscribe to wake word events.
    pub fn subscribe(&self) -> broadcast::Receiver<WakeWordEvent> {
        self.event_sender.subscribe()
    }

    /// Process audio samples.
    pub async fn process_audio(&self, samples: &[f32]) -> Option<WakeWordEvent> {
        let state = *self.state.read().await;
        if state != DetectorState::Listening {
            return None;
        }

        // Update sample count
        let mut count = self.sample_count.write().await;
        let timestamp_us = (*count as f64 / self.config.sample_rate as f64 * 1_000_000.0) as u64;
        *count += samples.len() as u64;

        // Update buffer
        let mut buffer = self.audio_buffer.write().await;
        buffer.extend_from_slice(samples);

        // Keep only last 2 seconds
        let max_samples = self.config.sample_rate as usize * 2;
        let current_len = buffer.len();
        if current_len > max_samples {
            buffer.drain(0..current_len - max_samples);
        }

        // Detect wake word (simple energy-based detection for now)
        if let Some(event) = self.detect_wake_word(&buffer, timestamp_us).await {
            let mut state = self.state.write().await;
            *state = DetectorState::Activated;

            let _ = self.event_sender.send(event.clone());
            return Some(event);
        }

        None
    }

    async fn detect_wake_word(&self, audio: &[f32], timestamp_us: u64) -> Option<WakeWordEvent> {
        // Simple energy-based detection (placeholder for real wake word detection)
        // In a real implementation, this would use a proper wake word engine

        if audio.len() < self.config.sample_rate as usize / 2 {
            return None;
        }

        // Calculate energy of recent audio
        let recent = &audio[audio.len().saturating_sub(8000)..];
        let energy: f32 = recent.iter().map(|s| s.abs()).sum::<f32>() / recent.len() as f32;

        // Threshold based on sensitivity
        let threshold = 0.05 * (1.0 - self.config.sensitivity);

        if energy > threshold {
            // Check if wake word pattern matches (simplified)
            // Real implementation would use actual speech recognition

            // For demo, randomly trigger on high energy (1% chance per high-energy frame)
            // This is just a placeholder - real detection would use proper models
            if rand_like(timestamp_us) < 0.01 {
                return Some(WakeWordEvent {
                    wake_word: self.config.wake_words[0].clone(),
                    confidence: 0.8,
                    timestamp_us,
                    audio: Some(audio.to_vec()),
                });
            }
        }

        None
    }

    /// Mark as done processing (return to listening).
    pub async fn done_processing(&self) {
        let mut state = self.state.write().await;
        if *state == DetectorState::Activated || *state == DetectorState::Processing {
            *state = DetectorState::Listening;
        }
    }

    /// Set to processing state.
    pub async fn set_processing(&self) {
        let mut state = self.state.write().await;
        if *state == DetectorState::Activated {
            *state = DetectorState::Processing;
        }
    }
}

/// Simple pseudo-random for demo (deterministic based on timestamp).
fn rand_like(seed: u64) -> f32 {
    let x = seed.wrapping_mul(1103515245).wrapping_add(12345);
    (x % 1000) as f32 / 1000.0
}

/// Continuous listening mode.
pub struct ContinuousMode {
    /// Whether continuous mode is active.
    active: Arc<RwLock<bool>>,
    /// Silence timeout (seconds).
    silence_timeout_secs: u32,
    /// Last speech timestamp.
    last_speech_us: Arc<RwLock<u64>>,
}

impl ContinuousMode {
    /// Create new continuous mode handler.
    pub fn new(silence_timeout_secs: u32) -> Self {
        Self {
            active: Arc::new(RwLock::new(false)),
            silence_timeout_secs,
            last_speech_us: Arc::new(RwLock::new(0)),
        }
    }

    /// Start continuous mode.
    pub async fn start(&self) {
        *self.active.write().await = true;
        *self.last_speech_us.write().await = current_time_us();
    }

    /// Stop continuous mode.
    pub async fn stop(&self) {
        *self.active.write().await = false;
    }

    /// Check if continuous mode is active.
    pub async fn is_active(&self) -> bool {
        if !*self.active.read().await {
            return false;
        }

        // Check for timeout
        let last_speech = *self.last_speech_us.read().await;
        let now = current_time_us();
        let silence_us = self.silence_timeout_secs as u64 * 1_000_000;

        if now - last_speech > silence_us {
            *self.active.write().await = false;
            return false;
        }

        true
    }

    /// Update last speech time.
    pub async fn speech_detected(&self) {
        *self.last_speech_us.write().await = current_time_us();
    }

    /// Get remaining time before timeout (seconds).
    pub async fn remaining_secs(&self) -> u32 {
        let last_speech = *self.last_speech_us.read().await;
        let now = current_time_us();
        let elapsed_secs = (now - last_speech) / 1_000_000;
        self.silence_timeout_secs
            .saturating_sub(elapsed_secs as u32)
    }
}

/// Get current time in microseconds.
fn current_time_us() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_wake_word_detector() {
        let config = WakeWordConfig::default();
        let detector = WakeWordDetector::new(config);

        detector.start().await.unwrap();
        assert_eq!(detector.state().await, DetectorState::Listening);

        detector.pause().await;
        assert_eq!(detector.state().await, DetectorState::Paused);

        detector.resume().await;
        assert_eq!(detector.state().await, DetectorState::Listening);

        detector.stop().await;
        assert_eq!(detector.state().await, DetectorState::Uninitialized);
    }

    #[tokio::test]
    async fn test_continuous_mode() {
        let mode = ContinuousMode::new(5);

        mode.start().await;
        assert!(mode.is_active().await);

        mode.speech_detected().await;
        assert!(mode.remaining_secs().await <= 5);

        mode.stop().await;
        assert!(!mode.is_active().await);
    }
}
