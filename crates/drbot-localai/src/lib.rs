//! Local AI runtime for drbot.
//!
//! Run AI models locally for privacy and offline operation.
//!
//! # Features
//!
//! - Local model management
//! - Inference engine
//! - Model downloading
//! - Quantization support
//! - Hardware acceleration

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

/// LocalAI result type.
pub type Result<T> = std::result::Result<T, LocalAIError>;

/// LocalAI errors.
#[derive(Debug, thiserror::Error)]
pub enum LocalAIError {
    #[error("Model not found: {0}")]
    ModelNotFound(String),
    #[error("Inference failed: {0}")]
    InferenceFailed(String),
    #[error("Download failed: {0}")]
    DownloadFailed(String),
    #[error("Insufficient resources: {0}")]
    InsufficientResources(String),
    #[error("Invalid model: {0}")]
    InvalidModel(String),
}

/// Model type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelType {
    TextGeneration,
    Embedding,
    Classification,
    Summarization,
    CodeGeneration,
    Translation,
    SpeechToText,
    TextToSpeech,
    ImageGeneration,
    Custom,
}

/// Quantization level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Quantization {
    None,
    Q4_0,
    Q4_1,
    Q5_0,
    Q5_1,
    Q8_0,
    F16,
}

impl Quantization {
    /// Get memory multiplier.
    pub fn memory_factor(&self) -> f32 {
        match self {
            Quantization::None => 4.0,
            Quantization::Q4_0 | Quantization::Q4_1 => 0.5,
            Quantization::Q5_0 | Quantization::Q5_1 => 0.625,
            Quantization::Q8_0 => 1.0,
            Quantization::F16 => 2.0,
        }
    }
}

/// Model information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    /// Model ID.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Model type.
    pub model_type: ModelType,
    /// Parameter count.
    pub parameters: u64,
    /// Quantization.
    pub quantization: Quantization,
    /// Size on disk (bytes).
    pub size_bytes: u64,
    /// Context length.
    pub context_length: usize,
    /// Description.
    pub description: String,
    /// Source URL.
    pub source_url: Option<String>,
    /// License.
    pub license: String,
}

impl ModelInfo {
    /// Estimate memory requirement.
    pub fn estimated_memory(&self) -> u64 {
        // Rough estimate: parameters * bytes per param * quantization factor
        (self.parameters as f64 * self.quantization.memory_factor() as f64 * 1.1) as u64
    }
}

/// Loaded model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadedModel {
    /// Model info.
    pub info: ModelInfo,
    /// Local path.
    pub path: PathBuf,
    /// Loaded at.
    pub loaded_at: DateTime<Utc>,
    /// Is ready.
    pub ready: bool,
    /// Memory usage.
    pub memory_usage: u64,
}

/// Generation parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationParams {
    /// Maximum tokens to generate.
    pub max_tokens: usize,
    /// Temperature.
    pub temperature: f32,
    /// Top-p sampling.
    pub top_p: f32,
    /// Top-k sampling.
    pub top_k: usize,
    /// Repetition penalty.
    pub repetition_penalty: f32,
    /// Stop sequences.
    pub stop_sequences: Vec<String>,
    /// Stream output.
    pub stream: bool,
}

impl Default for GenerationParams {
    fn default() -> Self {
        Self {
            max_tokens: 512,
            temperature: 0.7,
            top_p: 0.9,
            top_k: 40,
            repetition_penalty: 1.1,
            stop_sequences: Vec::new(),
            stream: false,
        }
    }
}

/// Generation result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationResult {
    /// Generated text.
    pub text: String,
    /// Tokens generated.
    pub tokens_generated: usize,
    /// Tokens per second.
    pub tokens_per_second: f32,
    /// Prompt tokens.
    pub prompt_tokens: usize,
    /// Total time (ms).
    pub total_time_ms: u64,
    /// Stop reason.
    pub stop_reason: StopReason,
}

/// Stop reasons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    MaxTokens,
    StopSequence,
    EndOfText,
    Error,
}

/// Embedding result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingResult {
    /// Embedding vector.
    pub embedding: Vec<f32>,
    /// Dimensions.
    pub dimensions: usize,
    /// Tokens processed.
    pub tokens: usize,
}

/// Hardware info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareInfo {
    /// CPU cores.
    pub cpu_cores: usize,
    /// Total RAM (bytes).
    pub total_ram: u64,
    /// Available RAM (bytes).
    pub available_ram: u64,
    /// Has GPU.
    pub has_gpu: bool,
    /// GPU name.
    pub gpu_name: Option<String>,
    /// GPU memory (bytes).
    pub gpu_memory: Option<u64>,
    /// Supports Metal (Apple).
    pub metal_support: bool,
    /// Supports CUDA.
    pub cuda_support: bool,
}

impl Default for HardwareInfo {
    fn default() -> Self {
        Self {
            cpu_cores: 4,
            total_ram: 8 * 1024 * 1024 * 1024,     // 8GB
            available_ram: 4 * 1024 * 1024 * 1024, // 4GB
            has_gpu: false,
            gpu_name: None,
            gpu_memory: None,
            metal_support: cfg!(target_os = "macos"),
            cuda_support: false,
        }
    }
}

/// Local AI configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalAIConfig {
    /// Models directory.
    pub models_dir: PathBuf,
    /// Max loaded models.
    pub max_loaded_models: usize,
    /// Use GPU if available.
    pub use_gpu: bool,
    /// Thread count.
    pub threads: usize,
    /// Batch size.
    pub batch_size: usize,
}

impl Default for LocalAIConfig {
    fn default() -> Self {
        Self {
            models_dir: PathBuf::from("~/.drbot/models"),
            max_loaded_models: 2,
            use_gpu: true,
            threads: 4,
            batch_size: 512,
        }
    }
}

/// Trait for model backends.
#[async_trait]
pub trait ModelBackend: Send + Sync {
    /// Load model.
    async fn load(&self, model: &ModelInfo, path: &PathBuf) -> Result<()>;
    /// Unload model.
    async fn unload(&self, model_id: &str) -> Result<()>;
    /// Generate text.
    async fn generate(
        &self,
        model_id: &str,
        prompt: &str,
        params: &GenerationParams,
    ) -> Result<GenerationResult>;
    /// Generate embedding.
    async fn embed(&self, model_id: &str, text: &str) -> Result<EmbeddingResult>;
    /// Check if model is loaded.
    async fn is_loaded(&self, model_id: &str) -> bool;
}

/// Local AI engine.
pub struct LocalAI<B: ModelBackend> {
    config: LocalAIConfig,
    backend: B,
    available_models: Arc<RwLock<HashMap<String, ModelInfo>>>,
    loaded_models: Arc<RwLock<HashMap<String, LoadedModel>>>,
    hardware: Arc<RwLock<HardwareInfo>>,
}

impl<B: ModelBackend> LocalAI<B> {
    /// Create a new LocalAI instance.
    pub fn new(config: LocalAIConfig, backend: B) -> Self {
        Self {
            config,
            backend,
            available_models: Arc::new(RwLock::new(HashMap::new())),
            loaded_models: Arc::new(RwLock::new(HashMap::new())),
            hardware: Arc::new(RwLock::new(HardwareInfo::default())),
        }
    }

    /// Register a model.
    pub async fn register_model(&self, model: ModelInfo) {
        self.available_models
            .write()
            .await
            .insert(model.id.clone(), model);
    }

    /// List available models.
    pub async fn list_models(&self) -> Vec<ModelInfo> {
        self.available_models
            .read()
            .await
            .values()
            .cloned()
            .collect()
    }

    /// List loaded models.
    pub async fn list_loaded(&self) -> Vec<LoadedModel> {
        self.loaded_models.read().await.values().cloned().collect()
    }

    /// Load a model.
    pub async fn load_model(&self, model_id: &str) -> Result<LoadedModel> {
        let model = self
            .available_models
            .read()
            .await
            .get(model_id)
            .cloned()
            .ok_or_else(|| LocalAIError::ModelNotFound(model_id.to_string()))?;

        // Check resources
        let required_memory = model.estimated_memory();
        {
            let hardware = self.hardware.read().await;
            if required_memory > hardware.available_ram {
                return Err(LocalAIError::InsufficientResources(format!(
                    "Need {}GB, have {}GB available",
                    required_memory / 1_000_000_000,
                    hardware.available_ram / 1_000_000_000
                )));
            }
        }

        // Check if we need to unload models
        {
            let loaded = self.loaded_models.read().await;
            if loaded.len() >= self.config.max_loaded_models {
                // Unload oldest model
                if let Some((oldest_id, _)) = loaded.iter().min_by_key(|(_, m)| m.loaded_at) {
                    let id = oldest_id.clone();
                    drop(loaded);
                    self.unload_model(&id).await?;
                }
            }
        } // Ensure read lock is dropped before write

        // Load the model
        let path = self.config.models_dir.join(&model_id);
        self.backend.load(&model, &path).await?;

        let loaded = LoadedModel {
            info: model,
            path,
            loaded_at: Utc::now(),
            ready: true,
            memory_usage: required_memory,
        };

        self.loaded_models
            .write()
            .await
            .insert(model_id.to_string(), loaded.clone());

        Ok(loaded)
    }

    /// Unload a model.
    pub async fn unload_model(&self, model_id: &str) -> Result<()> {
        self.backend.unload(model_id).await?;
        self.loaded_models.write().await.remove(model_id);
        Ok(())
    }

    /// Generate text.
    pub async fn generate(
        &self,
        model_id: &str,
        prompt: &str,
        params: Option<GenerationParams>,
    ) -> Result<GenerationResult> {
        // Auto-load if not loaded
        if !self.backend.is_loaded(model_id).await {
            self.load_model(model_id).await?;
        }

        let params = params.unwrap_or_default();
        self.backend.generate(model_id, prompt, &params).await
    }

    /// Generate embedding.
    pub async fn embed(&self, model_id: &str, text: &str) -> Result<EmbeddingResult> {
        // Auto-load if not loaded
        if !self.backend.is_loaded(model_id).await {
            self.load_model(model_id).await?;
        }

        self.backend.embed(model_id, text).await
    }

    /// Chat completion (convenience method).
    pub async fn chat(
        &self,
        model_id: &str,
        messages: &[ChatMessage],
        params: Option<GenerationParams>,
    ) -> Result<String> {
        // Format messages as prompt
        let prompt = messages
            .iter()
            .map(|m| format!("{}: {}", m.role, m.content))
            .collect::<Vec<_>>()
            .join("\n");

        let prompt = format!("{}\nassistant:", prompt);
        let result = self.generate(model_id, &prompt, params).await?;
        Ok(result.text)
    }

    /// Update hardware info.
    pub async fn update_hardware(&self, hardware: HardwareInfo) {
        *self.hardware.write().await = hardware;
    }

    /// Get hardware info.
    pub async fn get_hardware(&self) -> HardwareInfo {
        self.hardware.read().await.clone()
    }

    /// Check if model can run.
    pub async fn can_run(&self, model_id: &str) -> Result<bool> {
        let model = self
            .available_models
            .read()
            .await
            .get(model_id)
            .cloned()
            .ok_or_else(|| LocalAIError::ModelNotFound(model_id.to_string()))?;

        let hardware = self.hardware.read().await;
        let required = model.estimated_memory();

        Ok(required <= hardware.available_ram)
    }

    /// Get statistics.
    pub async fn stats(&self) -> LocalAIStats {
        let loaded = self.loaded_models.read().await;
        let available = self.available_models.read().await;
        let hardware = self.hardware.read().await;

        let total_loaded_memory: u64 = loaded.values().map(|m| m.memory_usage).sum();

        LocalAIStats {
            available_models: available.len(),
            loaded_models: loaded.len(),
            total_loaded_memory,
            available_memory: hardware.available_ram,
            gpu_available: hardware.has_gpu,
        }
    }
}

/// Chat message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    /// Role (user, assistant, system).
    pub role: String,
    /// Content.
    pub content: String,
}

impl ChatMessage {
    pub fn user(content: &str) -> Self {
        Self {
            role: "user".to_string(),
            content: content.to_string(),
        }
    }

    pub fn assistant(content: &str) -> Self {
        Self {
            role: "assistant".to_string(),
            content: content.to_string(),
        }
    }

    pub fn system(content: &str) -> Self {
        Self {
            role: "system".to_string(),
            content: content.to_string(),
        }
    }
}

/// LocalAI statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalAIStats {
    pub available_models: usize,
    pub loaded_models: usize,
    pub total_loaded_memory: u64,
    pub available_memory: u64,
    pub gpu_available: bool,
}

/// Mock backend for testing.
pub struct MockBackend {
    loaded: Arc<RwLock<HashMap<String, bool>>>,
}

impl MockBackend {
    pub fn new() -> Self {
        Self {
            loaded: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl Default for MockBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ModelBackend for MockBackend {
    async fn load(&self, model: &ModelInfo, _path: &PathBuf) -> Result<()> {
        self.loaded.write().await.insert(model.id.clone(), true);
        Ok(())
    }

    async fn unload(&self, model_id: &str) -> Result<()> {
        self.loaded.write().await.remove(model_id);
        Ok(())
    }

    async fn generate(
        &self,
        model_id: &str,
        prompt: &str,
        _params: &GenerationParams,
    ) -> Result<GenerationResult> {
        if !self.is_loaded(model_id).await {
            return Err(LocalAIError::ModelNotFound(model_id.to_string()));
        }

        // Mock response
        let response = format!("Mock response to: {}", &prompt[..prompt.len().min(50)]);
        let tokens = response.split_whitespace().count();

        Ok(GenerationResult {
            text: response,
            tokens_generated: tokens,
            tokens_per_second: 50.0,
            prompt_tokens: prompt.split_whitespace().count(),
            total_time_ms: (tokens as f32 / 50.0 * 1000.0) as u64,
            stop_reason: StopReason::MaxTokens,
        })
    }

    async fn embed(&self, model_id: &str, text: &str) -> Result<EmbeddingResult> {
        if !self.is_loaded(model_id).await {
            return Err(LocalAIError::ModelNotFound(model_id.to_string()));
        }

        // Mock embedding
        let mut embedding = vec![0.0f32; 384];
        for (i, c) in text.bytes().enumerate() {
            embedding[i % 384] += c as f32 / 255.0;
        }

        // Normalize
        let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for v in &mut embedding {
                *v /= norm;
            }
        }

        Ok(EmbeddingResult {
            embedding,
            dimensions: 384,
            tokens: text.split_whitespace().count(),
        })
    }

    async fn is_loaded(&self, model_id: &str) -> bool {
        self.loaded
            .read()
            .await
            .get(model_id)
            .copied()
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_model() -> ModelInfo {
        ModelInfo {
            id: "test-model".to_string(),
            name: "Test Model".to_string(),
            model_type: ModelType::TextGeneration,
            parameters: 7_000_000_000,
            quantization: Quantization::Q4_0,
            size_bytes: 4_000_000_000,
            context_length: 4096,
            description: "A test model".to_string(),
            source_url: None,
            license: "MIT".to_string(),
        }
    }

    #[tokio::test]
    async fn test_register_model() {
        let ai = LocalAI::new(LocalAIConfig::default(), MockBackend::new());
        ai.register_model(test_model()).await;

        let models = ai.list_models().await;
        assert_eq!(models.len(), 1);
    }

    #[tokio::test]
    async fn test_load_model() {
        let ai = LocalAI::new(LocalAIConfig::default(), MockBackend::new());
        ai.register_model(test_model()).await;

        let loaded = ai.load_model("test-model").await.unwrap();
        assert!(loaded.ready);

        let loaded_models = ai.list_loaded().await;
        assert_eq!(loaded_models.len(), 1);
    }

    #[tokio::test]
    async fn test_generate() {
        let ai = LocalAI::new(LocalAIConfig::default(), MockBackend::new());
        ai.register_model(test_model()).await;

        let result = ai
            .generate("test-model", "Hello, world!", None)
            .await
            .unwrap();
        assert!(!result.text.is_empty());
        assert!(result.tokens_generated > 0);
    }

    #[tokio::test]
    async fn test_embed() {
        let mut model = test_model();
        model.model_type = ModelType::Embedding;
        model.id = "embed-model".to_string();

        let ai = LocalAI::new(LocalAIConfig::default(), MockBackend::new());
        ai.register_model(model).await;

        let result = ai.embed("embed-model", "Hello, world!").await.unwrap();
        assert_eq!(result.dimensions, 384);
        assert!(!result.embedding.is_empty());
    }

    #[tokio::test]
    async fn test_chat() {
        let ai = LocalAI::new(LocalAIConfig::default(), MockBackend::new());
        ai.register_model(test_model()).await;

        let messages = vec![
            ChatMessage::system("You are a helpful assistant."),
            ChatMessage::user("What is 2+2?"),
        ];

        let response = ai.chat("test-model", &messages, None).await.unwrap();
        assert!(!response.is_empty());
    }

    #[tokio::test]
    async fn test_unload() {
        let ai = LocalAI::new(LocalAIConfig::default(), MockBackend::new());
        ai.register_model(test_model()).await;

        ai.load_model("test-model").await.unwrap();
        assert_eq!(ai.list_loaded().await.len(), 1);

        ai.unload_model("test-model").await.unwrap();
        assert_eq!(ai.list_loaded().await.len(), 0);
    }

    #[tokio::test]
    async fn test_memory_estimate() {
        let model = test_model();
        let memory = model.estimated_memory();

        // 7B params * 0.5 (Q4) * 1.1 overhead ≈ 3.85GB
        assert!(memory > 3_000_000_000);
        assert!(memory < 5_000_000_000);
    }

    #[tokio::test]
    async fn test_stats() {
        let ai = LocalAI::new(LocalAIConfig::default(), MockBackend::new());
        ai.register_model(test_model()).await;
        ai.load_model("test-model").await.unwrap();

        let stats = ai.stats().await;
        assert_eq!(stats.available_models, 1);
        assert_eq!(stats.loaded_models, 1);
        assert!(stats.total_loaded_memory > 0);
    }
}
