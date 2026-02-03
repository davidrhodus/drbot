//! Audio processing utilities.

use crate::{Result, VoiceError};
use serde::{Deserialize, Serialize};

/// Audio format specification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioFormat {
    /// Sample rate in Hz.
    pub sample_rate: u32,
    /// Number of channels.
    pub channels: u16,
    /// Bits per sample.
    pub bits_per_sample: u16,
}

impl AudioFormat {
    /// Standard mono 16kHz format (good for speech recognition).
    pub fn mono_16k() -> Self {
        Self {
            sample_rate: 16000,
            channels: 1,
            bits_per_sample: 16,
        }
    }

    /// Standard stereo 44.1kHz CD quality.
    pub fn stereo_44k() -> Self {
        Self {
            sample_rate: 44100,
            channels: 2,
            bits_per_sample: 16,
        }
    }

    /// Calculate bytes per second.
    pub fn bytes_per_second(&self) -> u32 {
        self.sample_rate * self.channels as u32 * (self.bits_per_sample as u32 / 8)
    }
}

impl Default for AudioFormat {
    fn default() -> Self {
        Self::mono_16k()
    }
}

/// Audio buffer for storing and manipulating audio data.
#[derive(Debug, Clone)]
pub struct AudioBuffer {
    /// Raw audio samples.
    pub samples: Vec<i16>,
    /// Audio format.
    pub format: AudioFormat,
}

impl AudioBuffer {
    /// Create a new empty buffer.
    pub fn new(format: AudioFormat) -> Self {
        Self {
            samples: Vec::new(),
            format,
        }
    }

    /// Create from raw bytes (assuming 16-bit little-endian PCM).
    pub fn from_bytes(bytes: &[u8], format: AudioFormat) -> Self {
        let samples: Vec<i16> = bytes
            .chunks_exact(2)
            .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
            .collect();

        Self { samples, format }
    }

    /// Convert to raw bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        self.samples.iter().flat_map(|s| s.to_le_bytes()).collect()
    }

    /// Get duration in seconds.
    pub fn duration_secs(&self) -> f32 {
        self.samples.len() as f32 / self.format.sample_rate as f32 / self.format.channels as f32
    }

    /// Append another buffer.
    pub fn append(&mut self, other: &AudioBuffer) {
        // Simple append - assumes same format
        self.samples.extend(&other.samples);
    }

    /// Get a slice of the buffer.
    pub fn slice(&self, start_secs: f32, duration_secs: f32) -> AudioBuffer {
        let samples_per_sec = self.format.sample_rate as f32 * self.format.channels as f32;
        let start_sample = (start_secs * samples_per_sec) as usize;
        let end_sample = ((start_secs + duration_secs) * samples_per_sec) as usize;

        let samples = self.samples
            [start_sample.min(self.samples.len())..end_sample.min(self.samples.len())]
            .to_vec();

        AudioBuffer {
            samples,
            format: self.format,
        }
    }

    /// Calculate RMS (root mean square) amplitude.
    pub fn rms(&self) -> f32 {
        if self.samples.is_empty() {
            return 0.0;
        }

        let sum: f64 = self.samples.iter().map(|s| (*s as f64).powi(2)).sum();
        (sum / self.samples.len() as f64).sqrt() as f32
    }

    /// Normalize audio to a target RMS level.
    pub fn normalize(&mut self, target_rms: f32) {
        let current_rms = self.rms();
        if current_rms < 1.0 {
            return;
        }

        let scale = target_rms / current_rms;
        for sample in &mut self.samples {
            *sample = ((*sample as f32) * scale).clamp(i16::MIN as f32, i16::MAX as f32) as i16;
        }
    }

    /// Resample to a different sample rate.
    pub fn resample(&self, target_rate: u32) -> AudioBuffer {
        if self.format.sample_rate == target_rate {
            return self.clone();
        }

        let ratio = target_rate as f64 / self.format.sample_rate as f64;
        let new_len = (self.samples.len() as f64 * ratio) as usize;
        let mut resampled = Vec::with_capacity(new_len);

        for i in 0..new_len {
            let src_idx = i as f64 / ratio;
            let src_floor = src_idx.floor() as usize;
            let frac = src_idx - src_floor as f64;

            let sample = if src_floor + 1 < self.samples.len() {
                // Linear interpolation
                let s1 = self.samples[src_floor] as f64;
                let s2 = self.samples[src_floor + 1] as f64;
                (s1 * (1.0 - frac) + s2 * frac) as i16
            } else if src_floor < self.samples.len() {
                self.samples[src_floor]
            } else {
                0
            };

            resampled.push(sample);
        }

        AudioBuffer {
            samples: resampled,
            format: AudioFormat {
                sample_rate: target_rate,
                ..self.format
            },
        }
    }
}

/// Audio processor for real-time audio manipulation.
pub struct AudioProcessor {
    /// Input format.
    pub input_format: AudioFormat,
    /// Output format.
    pub output_format: AudioFormat,
    /// Buffer for accumulating audio.
    buffer: AudioBuffer,
}

impl AudioProcessor {
    /// Create a new processor.
    pub fn new(input_format: AudioFormat, output_format: AudioFormat) -> Self {
        Self {
            input_format,
            output_format,
            buffer: AudioBuffer::new(input_format),
        }
    }

    /// Process incoming audio chunk.
    pub fn process(&mut self, chunk: &[u8]) -> Option<AudioBuffer> {
        let input_buffer = AudioBuffer::from_bytes(chunk, self.input_format);
        self.buffer.append(&input_buffer);

        // Check if we have enough audio for processing
        if self.buffer.duration_secs() >= 0.5 {
            let mut output = self.buffer.clone();

            // Resample if needed
            if self.input_format.sample_rate != self.output_format.sample_rate {
                output = output.resample(self.output_format.sample_rate);
            }

            // Clear buffer
            self.buffer = AudioBuffer::new(self.input_format);

            Some(output)
        } else {
            None
        }
    }

    /// Flush remaining audio.
    pub fn flush(&mut self) -> Option<AudioBuffer> {
        if self.buffer.samples.is_empty() {
            return None;
        }

        let mut output = std::mem::replace(&mut self.buffer, AudioBuffer::new(self.input_format));

        if self.input_format.sample_rate != self.output_format.sample_rate {
            output = output.resample(self.output_format.sample_rate);
        }

        Some(output)
    }
}

/// Voice activity detection (simple energy-based).
pub struct VoiceActivityDetector {
    /// Energy threshold.
    threshold: f32,
    /// Minimum speech duration in samples.
    min_speech_samples: usize,
    /// Speech detected state.
    is_speaking: bool,
    /// Silence counter.
    silence_count: usize,
    /// Maximum silence before end of speech.
    max_silence: usize,
}

impl VoiceActivityDetector {
    /// Create a new VAD.
    pub fn new(sample_rate: u32) -> Self {
        Self {
            threshold: 500.0,
            min_speech_samples: (sample_rate as f32 * 0.1) as usize, // 100ms
            is_speaking: false,
            silence_count: 0,
            max_silence: (sample_rate as f32 * 0.5) as usize, // 500ms
        }
    }

    /// Set energy threshold.
    pub fn with_threshold(mut self, threshold: f32) -> Self {
        self.threshold = threshold;
        self
    }

    /// Process a chunk and detect speech.
    pub fn process(&mut self, buffer: &AudioBuffer) -> VadResult {
        let rms = buffer.rms();

        if rms > self.threshold {
            self.silence_count = 0;

            if !self.is_speaking {
                self.is_speaking = true;
                return VadResult::SpeechStart;
            }

            VadResult::Speech
        } else {
            if self.is_speaking {
                self.silence_count += buffer.samples.len();

                if self.silence_count > self.max_silence {
                    self.is_speaking = false;
                    self.silence_count = 0;
                    return VadResult::SpeechEnd;
                }

                return VadResult::Speech;
            }

            VadResult::Silence
        }
    }

    /// Reset state.
    pub fn reset(&mut self) {
        self.is_speaking = false;
        self.silence_count = 0;
    }
}

/// VAD result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VadResult {
    /// Silence detected.
    Silence,
    /// Speech detected.
    Speech,
    /// Speech just started.
    SpeechStart,
    /// Speech just ended.
    SpeechEnd,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_format() {
        let format = AudioFormat::mono_16k();
        assert_eq!(format.bytes_per_second(), 32000);
    }

    #[test]
    fn test_audio_buffer() {
        let format = AudioFormat::mono_16k();
        let mut buffer = AudioBuffer::new(format);
        buffer.samples = vec![0, 1000, -1000, 500, -500];

        assert!(buffer.duration_secs() > 0.0);
        assert!(buffer.rms() > 0.0);
    }

    #[test]
    fn test_resample() {
        let format = AudioFormat::mono_16k();
        let mut buffer = AudioBuffer::new(format);
        buffer.samples = (0..16000).map(|i| (i % 100) as i16).collect();

        let resampled = buffer.resample(8000);
        assert_eq!(resampled.format.sample_rate, 8000);
        assert!(resampled.samples.len() < buffer.samples.len());
    }
}
