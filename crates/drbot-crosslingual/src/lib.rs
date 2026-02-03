//! Cross-lingual intelligence for drbot.
//!
//! Seamless multilingual support.
//!
//! # Features
//!
//! - Language detection
//! - Translation
//! - Cross-lingual understanding
//! - Multilingual response generation

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Cross-lingual result type.
pub type Result<T> = std::result::Result<T, LingualError>;

/// Cross-lingual errors.
#[derive(Debug, thiserror::Error)]
pub enum LingualError {
    #[error("Language not supported: {0}")]
    UnsupportedLanguage(String),
    #[error("Detection failed: {0}")]
    DetectionFailed(String),
    #[error("Translation failed: {0}")]
    TranslationFailed(String),
    #[error("No translation available")]
    NoTranslation,
}

/// Detected language.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedLanguage {
    /// Language code (ISO 639-1).
    pub code: String,
    /// Language name.
    pub name: String,
    /// Confidence (0-1).
    pub confidence: f32,
    /// Alternative languages.
    pub alternatives: Vec<(String, f32)>,
}

/// Translation result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Translation {
    /// Translation ID.
    pub id: Uuid,
    /// Source text.
    pub source: String,
    /// Source language.
    pub source_lang: String,
    /// Translated text.
    pub target: String,
    /// Target language.
    pub target_lang: String,
    /// Translation quality (0-1).
    pub quality: f32,
    /// Translated at.
    pub translated_at: DateTime<Utc>,
}

/// Language profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanguageProfile {
    /// Primary language.
    pub primary: String,
    /// Secondary languages.
    pub secondary: Vec<String>,
    /// Preferred response language.
    pub response_lang: Option<String>,
    /// Translation preferences.
    pub preferences: TranslationPreferences,
}

impl Default for LanguageProfile {
    fn default() -> Self {
        Self {
            primary: "en".to_string(),
            secondary: Vec::new(),
            response_lang: None,
            preferences: TranslationPreferences::default(),
        }
    }
}

/// Translation preferences.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslationPreferences {
    /// Auto-translate responses.
    pub auto_translate: bool,
    /// Preserve original with translation.
    pub show_original: bool,
    /// Formality level.
    pub formality: Formality,
}

impl Default for TranslationPreferences {
    fn default() -> Self {
        Self {
            auto_translate: true,
            show_original: false,
            formality: Formality::Neutral,
        }
    }
}

/// Formality levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Formality {
    Informal,
    Neutral,
    Formal,
}

/// Supported language info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanguageInfo {
    /// Language code.
    pub code: String,
    /// Language name.
    pub name: String,
    /// Native name.
    pub native_name: String,
    /// Right-to-left.
    pub rtl: bool,
    /// Translation supported.
    pub translation_supported: bool,
}

/// Cross-lingual configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossLingualConfig {
    /// Default language.
    pub default_lang: String,
    /// Enable auto-detection.
    pub auto_detect: bool,
    /// Cache translations.
    pub cache_translations: bool,
    /// Minimum detection confidence.
    pub min_confidence: f32,
}

impl Default for CrossLingualConfig {
    fn default() -> Self {
        Self {
            default_lang: "en".to_string(),
            auto_detect: true,
            cache_translations: true,
            min_confidence: 0.7,
        }
    }
}

/// Trait for language detectors.
#[async_trait]
pub trait LanguageDetector: Send + Sync {
    /// Detect language of text.
    async fn detect(&self, text: &str) -> Result<DetectedLanguage>;
}

/// Trait for translators.
#[async_trait]
pub trait Translator: Send + Sync {
    /// Translate text.
    async fn translate(
        &self,
        text: &str,
        source_lang: &str,
        target_lang: &str,
    ) -> Result<Translation>;
}

/// Cross-lingual engine.
pub struct CrossLingualEngine<D: LanguageDetector, T: Translator> {
    config: CrossLingualConfig,
    detector: D,
    translator: T,
    languages: Arc<RwLock<HashMap<String, LanguageInfo>>>,
    profiles: Arc<RwLock<HashMap<String, LanguageProfile>>>,
    cache: Arc<RwLock<HashMap<String, Translation>>>,
}

impl<D: LanguageDetector, T: Translator> CrossLingualEngine<D, T> {
    /// Create a new cross-lingual engine.
    pub fn new(config: CrossLingualConfig, detector: D, translator: T) -> Self {
        let engine = Self {
            config,
            detector,
            translator,
            languages: Arc::new(RwLock::new(HashMap::new())),
            profiles: Arc::new(RwLock::new(HashMap::new())),
            cache: Arc::new(RwLock::new(HashMap::new())),
        };

        // Initialize with common languages
        tokio::spawn({
            let languages = engine.languages.clone();
            async move {
                let common = vec![
                    LanguageInfo {
                        code: "en".to_string(),
                        name: "English".to_string(),
                        native_name: "English".to_string(),
                        rtl: false,
                        translation_supported: true,
                    },
                    LanguageInfo {
                        code: "es".to_string(),
                        name: "Spanish".to_string(),
                        native_name: "Español".to_string(),
                        rtl: false,
                        translation_supported: true,
                    },
                    LanguageInfo {
                        code: "fr".to_string(),
                        name: "French".to_string(),
                        native_name: "Français".to_string(),
                        rtl: false,
                        translation_supported: true,
                    },
                    LanguageInfo {
                        code: "de".to_string(),
                        name: "German".to_string(),
                        native_name: "Deutsch".to_string(),
                        rtl: false,
                        translation_supported: true,
                    },
                    LanguageInfo {
                        code: "zh".to_string(),
                        name: "Chinese".to_string(),
                        native_name: "中文".to_string(),
                        rtl: false,
                        translation_supported: true,
                    },
                    LanguageInfo {
                        code: "ja".to_string(),
                        name: "Japanese".to_string(),
                        native_name: "日本語".to_string(),
                        rtl: false,
                        translation_supported: true,
                    },
                    LanguageInfo {
                        code: "ar".to_string(),
                        name: "Arabic".to_string(),
                        native_name: "العربية".to_string(),
                        rtl: true,
                        translation_supported: true,
                    },
                    LanguageInfo {
                        code: "pt".to_string(),
                        name: "Portuguese".to_string(),
                        native_name: "Português".to_string(),
                        rtl: false,
                        translation_supported: true,
                    },
                    LanguageInfo {
                        code: "ru".to_string(),
                        name: "Russian".to_string(),
                        native_name: "Русский".to_string(),
                        rtl: false,
                        translation_supported: true,
                    },
                    LanguageInfo {
                        code: "ko".to_string(),
                        name: "Korean".to_string(),
                        native_name: "한국어".to_string(),
                        rtl: false,
                        translation_supported: true,
                    },
                ];

                let mut langs = languages.write().await;
                for lang in common {
                    langs.insert(lang.code.clone(), lang);
                }
            }
        });

        engine
    }

    /// Detect language.
    pub async fn detect(&self, text: &str) -> Result<DetectedLanguage> {
        let detected = self.detector.detect(text).await?;

        if detected.confidence < self.config.min_confidence {
            return Ok(DetectedLanguage {
                code: self.config.default_lang.clone(),
                name: "Unknown".to_string(),
                confidence: detected.confidence,
                alternatives: vec![(detected.code, detected.confidence)],
            });
        }

        Ok(detected)
    }

    /// Translate text.
    pub async fn translate(&self, text: &str, target_lang: &str) -> Result<Translation> {
        // Check cache
        let cache_key = format!("{}:{}", text, target_lang);
        if self.config.cache_translations {
            let cache = self.cache.read().await;
            if let Some(cached) = cache.get(&cache_key) {
                return Ok(cached.clone());
            }
        }

        // Detect source language
        let detected = self.detect(text).await?;

        // Skip if same language
        if detected.code == target_lang {
            return Ok(Translation {
                id: Uuid::new_v4(),
                source: text.to_string(),
                source_lang: detected.code,
                target: text.to_string(),
                target_lang: target_lang.to_string(),
                quality: 1.0,
                translated_at: Utc::now(),
            });
        }

        // Translate
        let translation = self
            .translator
            .translate(text, &detected.code, target_lang)
            .await?;

        // Cache result
        if self.config.cache_translations {
            self.cache
                .write()
                .await
                .insert(cache_key, translation.clone());
        }

        Ok(translation)
    }

    /// Process text with language handling.
    pub async fn process(&self, text: &str, user_id: &str) -> Result<ProcessedText> {
        let detected = self.detect(text).await?;

        let profile = self.get_profile(user_id).await;
        let target_lang = profile.response_lang.as_ref().unwrap_or(&profile.primary);

        let translation = if detected.code != *target_lang && profile.preferences.auto_translate {
            Some(self.translate(text, target_lang).await?)
        } else {
            None
        };

        Ok(ProcessedText {
            original: text.to_string(),
            detected_lang: detected,
            translation,
            target_lang: target_lang.clone(),
        })
    }

    /// Set user profile.
    pub async fn set_profile(&self, user_id: &str, profile: LanguageProfile) {
        self.profiles
            .write()
            .await
            .insert(user_id.to_string(), profile);
    }

    /// Get user profile.
    pub async fn get_profile(&self, user_id: &str) -> LanguageProfile {
        self.profiles
            .read()
            .await
            .get(user_id)
            .cloned()
            .unwrap_or_default()
    }

    /// List supported languages.
    pub async fn list_languages(&self) -> Vec<LanguageInfo> {
        self.languages.read().await.values().cloned().collect()
    }

    /// Check if language is supported.
    pub async fn is_supported(&self, lang: &str) -> bool {
        self.languages.read().await.contains_key(lang)
    }

    /// Get statistics.
    pub async fn stats(&self) -> CrossLingualStats {
        let cache = self.cache.read().await;
        let profiles = self.profiles.read().await;
        let languages = self.languages.read().await;

        CrossLingualStats {
            supported_languages: languages.len(),
            cached_translations: cache.len(),
            user_profiles: profiles.len(),
        }
    }
}

/// Processed text result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessedText {
    /// Original text.
    pub original: String,
    /// Detected language.
    pub detected_lang: DetectedLanguage,
    /// Translation (if needed).
    pub translation: Option<Translation>,
    /// Target language.
    pub target_lang: String,
}

/// Cross-lingual statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossLingualStats {
    pub supported_languages: usize,
    pub cached_translations: usize,
    pub user_profiles: usize,
}

/// Simple language detector.
pub struct SimpleDetector;

#[async_trait]
impl LanguageDetector for SimpleDetector {
    async fn detect(&self, text: &str) -> Result<DetectedLanguage> {
        // Simple heuristic detection
        let text_lower = text.to_lowercase();

        let (code, name, confidence) = if text.chars().any(|c| c >= '\u{4e00}' && c <= '\u{9fff}') {
            ("zh", "Chinese", 0.9)
        } else if text.chars().any(|c| c >= '\u{3040}' && c <= '\u{30ff}') {
            ("ja", "Japanese", 0.9)
        } else if text.chars().any(|c| c >= '\u{ac00}' && c <= '\u{d7af}') {
            ("ko", "Korean", 0.9)
        } else if text.chars().any(|c| c >= '\u{0600}' && c <= '\u{06ff}') {
            ("ar", "Arabic", 0.9)
        } else if text.chars().any(|c| c >= '\u{0400}' && c <= '\u{04ff}') {
            ("ru", "Russian", 0.9)
        } else if text_lower.contains("ñ") || text_lower.contains("¿") || text_lower.contains("¡")
        {
            ("es", "Spanish", 0.8)
        } else if text_lower.contains("ü") || text_lower.contains("ö") || text_lower.contains("ä")
        {
            ("de", "German", 0.7)
        } else if text_lower.contains("ç") || text_lower.contains("é") || text_lower.contains("è")
        {
            ("fr", "French", 0.7)
        } else {
            ("en", "English", 0.8)
        };

        Ok(DetectedLanguage {
            code: code.to_string(),
            name: name.to_string(),
            confidence,
            alternatives: Vec::new(),
        })
    }
}

/// Simple translator (mock).
pub struct SimpleTranslator;

#[async_trait]
impl Translator for SimpleTranslator {
    async fn translate(
        &self,
        text: &str,
        source_lang: &str,
        target_lang: &str,
    ) -> Result<Translation> {
        // Mock translation (in real implementation, call translation API)
        Ok(Translation {
            id: Uuid::new_v4(),
            source: text.to_string(),
            source_lang: source_lang.to_string(),
            target: format!("[{}->{}] {}", source_lang, target_lang, text),
            target_lang: target_lang.to_string(),
            quality: 0.85,
            translated_at: Utc::now(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_language_detection() {
        let engine = CrossLingualEngine::new(
            CrossLingualConfig::default(),
            SimpleDetector,
            SimpleTranslator,
        );

        let result = engine.detect("Hello world").await.unwrap();
        assert_eq!(result.code, "en");

        let result = engine.detect("¡Hola, España!").await.unwrap();
        assert_eq!(result.code, "es");

        let result = engine.detect("こんにちは").await.unwrap();
        assert_eq!(result.code, "ja");
    }

    #[tokio::test]
    async fn test_translation() {
        let engine = CrossLingualEngine::new(
            CrossLingualConfig::default(),
            SimpleDetector,
            SimpleTranslator,
        );

        let translation = engine.translate("Hello", "es").await.unwrap();
        assert_eq!(translation.source, "Hello");
        assert_eq!(translation.target_lang, "es");
    }

    #[tokio::test]
    async fn test_user_profile() {
        let engine = CrossLingualEngine::new(
            CrossLingualConfig::default(),
            SimpleDetector,
            SimpleTranslator,
        );

        let profile = LanguageProfile {
            primary: "es".to_string(),
            secondary: vec!["en".to_string()],
            response_lang: Some("es".to_string()),
            preferences: TranslationPreferences::default(),
        };

        engine.set_profile("user1", profile).await;

        let retrieved = engine.get_profile("user1").await;
        assert_eq!(retrieved.primary, "es");
    }

    #[tokio::test]
    async fn test_process_text() {
        let engine = CrossLingualEngine::new(
            CrossLingualConfig::default(),
            SimpleDetector,
            SimpleTranslator,
        );

        let profile = LanguageProfile {
            primary: "es".to_string(),
            response_lang: Some("es".to_string()),
            preferences: TranslationPreferences {
                auto_translate: true,
                ..Default::default()
            },
            ..Default::default()
        };

        engine.set_profile("user1", profile).await;

        let processed = engine.process("Hello world", "user1").await.unwrap();
        assert_eq!(processed.detected_lang.code, "en");
        assert!(processed.translation.is_some());
    }
}
