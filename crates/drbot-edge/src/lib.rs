//! Edge computing and local inference for drbot
//!
//! Local model inference, offline capabilities, and battery-aware processing.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;

#[derive(Error, Debug)]
pub enum EdgeError {
    #[error("Model not found: {0}")]
    ModelNotFound(String),
    #[error("Model loading failed: {0}")]
    ModelLoadFailed(String),
    #[error("Inference failed: {0}")]
    InferenceFailed(String),
    #[error("Insufficient resources: {0}")]
    InsufficientResources(String),
    #[error("Offline unavailable: {0}")]
    OfflineUnavailable(String),
    #[error("Provider error: {0}")]
    ProviderError(String),
}

pub type Result<T> = std::result::Result<T, EdgeError>;

// ============================================================================
// Model Management
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalModel {
    pub id: String,
    pub name: String,
    pub model_type: ModelType,
    pub version: String,
    pub size_bytes: u64,
    pub capabilities: Vec<ModelCapability>,
    pub requirements: ModelRequirements,
    pub status: ModelStatus,
    pub quantization: Option<Quantization>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ModelType {
    TextGeneration,
    TextEmbedding,
    ImageClassification,
    ObjectDetection,
    SpeechToText,
    TextToSpeech,
    Translation,
    Summarization,
    Sentiment,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ModelCapability {
    Chat,
    Completion,
    Embedding,
    Classification,
    Extraction,
    Summarization,
    Translation,
    CodeGeneration,
    QuestionAnswering,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRequirements {
    pub min_ram_mb: u32,
    pub min_storage_mb: u32,
    pub gpu_required: bool,
    pub min_compute_units: Option<u32>,
    pub supported_platforms: Vec<Platform>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Platform {
    MacOS,
    MacOSAppleSilicon,
    Windows,
    Linux,
    iOS,
    Android,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum ModelStatus {
    NotDownloaded,
    Downloading,
    Downloaded,
    Loading,
    Loaded,
    Unloading,
    Error,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum Quantization {
    None,
    Int8,
    Int4,
    Float16,
    BFloat16,
}

// ============================================================================
// Device Resources
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceResources {
    pub cpu: CpuInfo,
    pub memory: MemoryInfo,
    pub storage: StorageInfo,
    pub gpu: Option<GpuInfo>,
    pub battery: Option<BatteryInfo>,
    pub network: NetworkInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuInfo {
    pub cores: u32,
    pub threads: u32,
    pub architecture: String,
    pub current_usage_percent: f32,
    pub has_neural_engine: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryInfo {
    pub total_mb: u64,
    pub available_mb: u64,
    pub used_mb: u64,
    pub usage_percent: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageInfo {
    pub total_gb: u64,
    pub available_gb: u64,
    pub model_cache_gb: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuInfo {
    pub name: String,
    pub memory_mb: u32,
    pub available_memory_mb: u32,
    pub compute_capability: Option<String>,
    pub metal_support: bool,
    pub cuda_support: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatteryInfo {
    pub level_percent: u8,
    pub is_charging: bool,
    pub time_remaining_minutes: Option<u32>,
    pub power_mode: PowerMode,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum PowerMode {
    Normal,
    LowPower,
    Performance,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkInfo {
    pub is_online: bool,
    pub connection_type: ConnectionType,
    pub bandwidth_mbps: Option<f32>,
    pub is_metered: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum ConnectionType {
    Wifi,
    Ethernet,
    Cellular,
    None,
    Unknown,
}

// ============================================================================
// Inference
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceRequest {
    pub model_id: String,
    pub input: InferenceInput,
    pub parameters: InferenceParameters,
    pub constraints: InferenceConstraints,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InferenceInput {
    Text(String),
    Tokens(Vec<u32>),
    Image(Vec<u8>),
    Audio(Vec<u8>),
    Embedding(Vec<f32>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceParameters {
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub top_k: Option<u32>,
    pub stop_sequences: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceConstraints {
    pub max_latency_ms: Option<u32>,
    pub max_memory_mb: Option<u32>,
    pub battery_aware: bool,
    pub prefer_quality: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceResult {
    pub model_id: String,
    pub output: InferenceOutput,
    pub metrics: InferenceMetrics,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InferenceOutput {
    Text(String),
    Tokens(Vec<u32>),
    Classification(Vec<ClassificationResult>),
    Embedding(Vec<f32>),
    Detection(Vec<DetectionResult>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassificationResult {
    pub label: String,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectionResult {
    pub label: String,
    pub confidence: f32,
    pub bbox: BoundingBox,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoundingBox {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceMetrics {
    pub latency_ms: u32,
    pub tokens_per_second: Option<f32>,
    pub memory_used_mb: u32,
    pub energy_impact: EnergyImpact,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum EnergyImpact {
    Low,
    Medium,
    High,
}

// ============================================================================
// Offline Capabilities
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OfflineCapabilities {
    pub available_capabilities: Vec<ModelCapability>,
    pub cached_data: CachedData,
    pub sync_status: SyncStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedData {
    pub conversations: u32,
    pub documents: u32,
    pub embeddings: u32,
    pub last_sync: u64,
    pub pending_sync: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum SyncStatus {
    Synced,
    Pending,
    Syncing,
    Conflict,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncOperation {
    pub id: String,
    pub operation_type: SyncOperationType,
    pub data_type: String,
    pub created_at: u64,
    pub status: SyncStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SyncOperationType {
    Upload,
    Download,
    Merge,
    Delete,
}

// ============================================================================
// Power Management
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PowerSettings {
    pub mode: InferenceMode,
    pub battery_threshold: u8,
    pub auto_unload_models: bool,
    pub prefer_cloud_when_charging: bool,
    pub max_background_power: f32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum InferenceMode {
    Quality,
    Balanced,
    Efficiency,
    UltraEfficiency,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PowerRecommendation {
    pub current_mode: InferenceMode,
    pub recommended_mode: InferenceMode,
    pub reason: String,
    pub estimated_battery_impact: String,
}

// ============================================================================
// Provider Trait
// ============================================================================

#[async_trait]
pub trait EdgeProvider: Send + Sync {
    async fn list_available_models(&self) -> Result<Vec<LocalModel>>;
    async fn download_model(&self, model_id: &str) -> Result<()>;
    async fn load_model(&self, model_id: &str) -> Result<()>;
    async fn unload_model(&self, model_id: &str) -> Result<()>;
    async fn run_inference(&self, request: InferenceRequest) -> Result<InferenceResult>;
    async fn get_device_resources(&self) -> Result<DeviceResources>;
}

// ============================================================================
// Edge Engine
// ============================================================================

pub struct EdgeEngine {
    provider: Arc<dyn EdgeProvider>,
    models: Arc<RwLock<HashMap<String, LocalModel>>>,
    loaded_models: Arc<RwLock<Vec<String>>>,
    power_settings: Arc<RwLock<PowerSettings>>,
    offline_queue: Arc<RwLock<Vec<SyncOperation>>>,
    resources_cache: Arc<RwLock<Option<DeviceResources>>>,
}

impl EdgeEngine {
    pub fn new(provider: Arc<dyn EdgeProvider>) -> Self {
        Self {
            provider,
            models: Arc::new(RwLock::new(HashMap::new())),
            loaded_models: Arc::new(RwLock::new(Vec::new())),
            power_settings: Arc::new(RwLock::new(PowerSettings::default())),
            offline_queue: Arc::new(RwLock::new(Vec::new())),
            resources_cache: Arc::new(RwLock::new(None)),
        }
    }

    // Model Management
    pub async fn refresh_models(&self) -> Result<Vec<LocalModel>> {
        let models = self.provider.list_available_models().await?;

        let mut cache = self.models.write().await;
        cache.clear();
        for model in &models {
            cache.insert(model.id.clone(), model.clone());
        }

        Ok(models)
    }

    pub async fn get_model(&self, model_id: &str) -> Result<LocalModel> {
        let models = self.models.read().await;
        models
            .get(model_id)
            .cloned()
            .ok_or_else(|| EdgeError::ModelNotFound(model_id.to_string()))
    }

    pub async fn download_model(&self, model_id: &str) -> Result<()> {
        // Check resources first
        let resources = self.get_device_resources().await?;
        let model = self.get_model(model_id).await?;

        let required_gb = (model.size_bytes / 1_000_000_000) as u64 + 1;
        if resources.storage.available_gb < required_gb {
            return Err(EdgeError::InsufficientResources(format!(
                "Need {} GB, have {} GB available",
                required_gb, resources.storage.available_gb
            )));
        }

        // Update status
        {
            let mut models = self.models.write().await;
            if let Some(m) = models.get_mut(model_id) {
                m.status = ModelStatus::Downloading;
            }
        }

        self.provider.download_model(model_id).await?;

        // Update status
        {
            let mut models = self.models.write().await;
            if let Some(m) = models.get_mut(model_id) {
                m.status = ModelStatus::Downloaded;
            }
        }

        Ok(())
    }

    pub async fn load_model(&self, model_id: &str) -> Result<()> {
        let model = self.get_model(model_id).await?;

        if model.status != ModelStatus::Downloaded {
            return Err(EdgeError::ModelLoadFailed(
                "Model not downloaded".to_string(),
            ));
        }

        // Check memory
        let resources = self.get_device_resources().await?;
        if resources.memory.available_mb < model.requirements.min_ram_mb as u64 {
            return Err(EdgeError::InsufficientResources(format!(
                "Need {} MB RAM, have {} MB available",
                model.requirements.min_ram_mb, resources.memory.available_mb
            )));
        }

        self.provider.load_model(model_id).await?;

        // Track loaded model
        {
            let mut loaded = self.loaded_models.write().await;
            if !loaded.contains(&model_id.to_string()) {
                loaded.push(model_id.to_string());
            }
        }

        // Update status
        {
            let mut models = self.models.write().await;
            if let Some(m) = models.get_mut(model_id) {
                m.status = ModelStatus::Loaded;
            }
        }

        Ok(())
    }

    pub async fn unload_model(&self, model_id: &str) -> Result<()> {
        self.provider.unload_model(model_id).await?;

        {
            let mut loaded = self.loaded_models.write().await;
            loaded.retain(|id| id != model_id);
        }

        {
            let mut models = self.models.write().await;
            if let Some(m) = models.get_mut(model_id) {
                m.status = ModelStatus::Downloaded;
            }
        }

        Ok(())
    }

    pub async fn get_loaded_models(&self) -> Vec<String> {
        let loaded = self.loaded_models.read().await;
        loaded.clone()
    }

    // Inference
    pub async fn infer(&self, model_id: &str, input: InferenceInput) -> Result<InferenceResult> {
        let model = self.get_model(model_id).await?;

        if model.status != ModelStatus::Loaded {
            // Auto-load if downloaded
            if model.status == ModelStatus::Downloaded {
                self.load_model(model_id).await?;
            } else {
                return Err(EdgeError::InferenceFailed("Model not loaded".to_string()));
            }
        }

        let settings = self.power_settings.read().await;
        let constraints = self.get_constraints_for_mode(settings.mode);

        let request = InferenceRequest {
            model_id: model_id.to_string(),
            input,
            parameters: InferenceParameters::default(),
            constraints,
        };

        self.provider.run_inference(request).await
    }

    pub async fn infer_text(&self, model_id: &str, text: &str) -> Result<String> {
        let result = self
            .infer(model_id, InferenceInput::Text(text.to_string()))
            .await?;

        match result.output {
            InferenceOutput::Text(t) => Ok(t),
            _ => Err(EdgeError::InferenceFailed(
                "Unexpected output type".to_string(),
            )),
        }
    }

    pub async fn embed(&self, model_id: &str, text: &str) -> Result<Vec<f32>> {
        let result = self
            .infer(model_id, InferenceInput::Text(text.to_string()))
            .await?;

        match result.output {
            InferenceOutput::Embedding(e) => Ok(e),
            _ => Err(EdgeError::InferenceFailed(
                "Model does not support embeddings".to_string(),
            )),
        }
    }

    fn get_constraints_for_mode(&self, mode: InferenceMode) -> InferenceConstraints {
        match mode {
            InferenceMode::Quality => InferenceConstraints {
                max_latency_ms: None,
                max_memory_mb: None,
                battery_aware: false,
                prefer_quality: true,
            },
            InferenceMode::Balanced => InferenceConstraints {
                max_latency_ms: Some(5000),
                max_memory_mb: Some(2048),
                battery_aware: true,
                prefer_quality: false,
            },
            InferenceMode::Efficiency => InferenceConstraints {
                max_latency_ms: Some(2000),
                max_memory_mb: Some(1024),
                battery_aware: true,
                prefer_quality: false,
            },
            InferenceMode::UltraEfficiency => InferenceConstraints {
                max_latency_ms: Some(1000),
                max_memory_mb: Some(512),
                battery_aware: true,
                prefer_quality: false,
            },
        }
    }

    // Resource Management
    pub async fn get_device_resources(&self) -> Result<DeviceResources> {
        // Check cache
        {
            let cache = self.resources_cache.read().await;
            if cache.is_some() {
                return Ok(cache.clone().unwrap());
            }
        }

        let resources = self.provider.get_device_resources().await?;

        {
            let mut cache = self.resources_cache.write().await;
            *cache = Some(resources.clone());
        }

        Ok(resources)
    }

    pub async fn invalidate_resource_cache(&self) {
        let mut cache = self.resources_cache.write().await;
        *cache = None;
    }

    pub async fn can_run_model(&self, model_id: &str) -> Result<bool> {
        let model = self.get_model(model_id).await?;
        let resources = self.get_device_resources().await?;

        let has_memory = resources.memory.available_mb >= model.requirements.min_ram_mb as u64;
        let has_storage =
            resources.storage.available_gb >= (model.size_bytes / 1_000_000_000) as u64;
        let has_gpu = !model.requirements.gpu_required || resources.gpu.is_some();

        Ok(has_memory && has_storage && has_gpu)
    }

    // Power Management
    pub async fn set_power_mode(&self, mode: InferenceMode) -> Result<()> {
        let mut settings = self.power_settings.write().await;
        settings.mode = mode;
        Ok(())
    }

    pub async fn get_power_recommendation(&self) -> Result<PowerRecommendation> {
        let resources = self.get_device_resources().await?;
        let settings = self.power_settings.read().await;

        let recommended = if let Some(battery) = &resources.battery {
            if !battery.is_charging && battery.level_percent < 20 {
                InferenceMode::UltraEfficiency
            } else if !battery.is_charging && battery.level_percent < 50 {
                InferenceMode::Efficiency
            } else if battery.is_charging {
                InferenceMode::Quality
            } else {
                InferenceMode::Balanced
            }
        } else {
            // Plugged in (no battery)
            InferenceMode::Quality
        };

        let reason = if recommended != settings.mode {
            match recommended {
                InferenceMode::UltraEfficiency => "Battery critically low".to_string(),
                InferenceMode::Efficiency => "Battery below 50%".to_string(),
                InferenceMode::Quality => "Device is charging".to_string(),
                InferenceMode::Balanced => "Normal operation".to_string(),
            }
        } else {
            "Current mode is optimal".to_string()
        };

        Ok(PowerRecommendation {
            current_mode: settings.mode,
            recommended_mode: recommended,
            reason,
            estimated_battery_impact: match recommended {
                InferenceMode::Quality => "High".to_string(),
                InferenceMode::Balanced => "Medium".to_string(),
                InferenceMode::Efficiency => "Low".to_string(),
                InferenceMode::UltraEfficiency => "Minimal".to_string(),
            },
        })
    }

    pub async fn apply_power_recommendation(&self) -> Result<InferenceMode> {
        let recommendation = self.get_power_recommendation().await?;
        self.set_power_mode(recommendation.recommended_mode).await?;
        Ok(recommendation.recommended_mode)
    }

    // Offline
    pub async fn get_offline_capabilities(&self) -> Result<OfflineCapabilities> {
        let models = self.models.read().await;
        let loaded = self.loaded_models.read().await;
        let queue = self.offline_queue.read().await;

        let available_capabilities: Vec<ModelCapability> = models
            .values()
            .filter(|m| loaded.contains(&m.id))
            .flat_map(|m| m.capabilities.clone())
            .collect();

        Ok(OfflineCapabilities {
            available_capabilities,
            cached_data: CachedData {
                conversations: 10, // Placeholder
                documents: 5,
                embeddings: 100,
                last_sync: 0,
                pending_sync: queue.len() as u32,
            },
            sync_status: if queue.is_empty() {
                SyncStatus::Synced
            } else {
                SyncStatus::Pending
            },
        })
    }

    pub async fn queue_sync_operation(&self, operation: SyncOperation) {
        let mut queue = self.offline_queue.write().await;
        queue.push(operation);
    }

    pub async fn is_online(&self) -> Result<bool> {
        let resources = self.get_device_resources().await?;
        Ok(resources.network.is_online)
    }
}

impl Default for PowerSettings {
    fn default() -> Self {
        Self {
            mode: InferenceMode::Balanced,
            battery_threshold: 20,
            auto_unload_models: true,
            prefer_cloud_when_charging: false,
            max_background_power: 0.1,
        }
    }
}

impl Default for InferenceParameters {
    fn default() -> Self {
        Self {
            max_tokens: Some(256),
            temperature: Some(0.7),
            top_p: Some(0.9),
            top_k: Some(40),
            stop_sequences: vec![],
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    struct MockProvider {
        models: Vec<LocalModel>,
    }

    impl MockProvider {
        fn new() -> Self {
            Self {
                models: vec![LocalModel {
                    id: "model-1".to_string(),
                    name: "Test Model".to_string(),
                    model_type: ModelType::TextGeneration,
                    version: "1.0".to_string(),
                    size_bytes: 1_000_000_000, // 1 GB
                    capabilities: vec![ModelCapability::Chat, ModelCapability::Completion],
                    requirements: ModelRequirements {
                        min_ram_mb: 2048,
                        min_storage_mb: 1500,
                        gpu_required: false,
                        min_compute_units: None,
                        supported_platforms: vec![Platform::MacOS],
                    },
                    status: ModelStatus::Downloaded,
                    quantization: Some(Quantization::Int8),
                }],
            }
        }
    }

    #[async_trait]
    impl EdgeProvider for MockProvider {
        async fn list_available_models(&self) -> Result<Vec<LocalModel>> {
            Ok(self.models.clone())
        }

        async fn download_model(&self, _model_id: &str) -> Result<()> {
            Ok(())
        }

        async fn load_model(&self, _model_id: &str) -> Result<()> {
            Ok(())
        }

        async fn unload_model(&self, _model_id: &str) -> Result<()> {
            Ok(())
        }

        async fn run_inference(&self, request: InferenceRequest) -> Result<InferenceResult> {
            Ok(InferenceResult {
                model_id: request.model_id,
                output: InferenceOutput::Text("Generated response".to_string()),
                metrics: InferenceMetrics {
                    latency_ms: 150,
                    tokens_per_second: Some(45.0),
                    memory_used_mb: 1800,
                    energy_impact: EnergyImpact::Medium,
                },
            })
        }

        async fn get_device_resources(&self) -> Result<DeviceResources> {
            Ok(DeviceResources {
                cpu: CpuInfo {
                    cores: 8,
                    threads: 8,
                    architecture: "arm64".to_string(),
                    current_usage_percent: 25.0,
                    has_neural_engine: true,
                },
                memory: MemoryInfo {
                    total_mb: 16384,
                    available_mb: 8192,
                    used_mb: 8192,
                    usage_percent: 50.0,
                },
                storage: StorageInfo {
                    total_gb: 512,
                    available_gb: 200,
                    model_cache_gb: 5.0,
                },
                gpu: Some(GpuInfo {
                    name: "Apple M1".to_string(),
                    memory_mb: 8192,
                    available_memory_mb: 4096,
                    compute_capability: None,
                    metal_support: true,
                    cuda_support: false,
                }),
                battery: Some(BatteryInfo {
                    level_percent: 75,
                    is_charging: false,
                    time_remaining_minutes: Some(180),
                    power_mode: PowerMode::Normal,
                }),
                network: NetworkInfo {
                    is_online: true,
                    connection_type: ConnectionType::Wifi,
                    bandwidth_mbps: Some(100.0),
                    is_metered: false,
                },
            })
        }
    }

    #[tokio::test]
    async fn test_model_listing() {
        let provider = Arc::new(MockProvider::new());
        let engine = EdgeEngine::new(provider);

        let models = engine.refresh_models().await.unwrap();
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].name, "Test Model");
    }

    #[tokio::test]
    async fn test_model_loading() {
        let provider = Arc::new(MockProvider::new());
        let engine = EdgeEngine::new(provider);

        engine.refresh_models().await.unwrap();
        engine.load_model("model-1").await.unwrap();

        let loaded = engine.get_loaded_models().await;
        assert!(loaded.contains(&"model-1".to_string()));
    }

    #[tokio::test]
    async fn test_inference() {
        let provider = Arc::new(MockProvider::new());
        let engine = EdgeEngine::new(provider);

        engine.refresh_models().await.unwrap();
        engine.load_model("model-1").await.unwrap();

        let result = engine.infer_text("model-1", "Hello, world!").await.unwrap();
        assert_eq!(result, "Generated response");
    }

    #[tokio::test]
    async fn test_resource_check() {
        let provider = Arc::new(MockProvider::new());
        let engine = EdgeEngine::new(provider);

        engine.refresh_models().await.unwrap();

        let can_run = engine.can_run_model("model-1").await.unwrap();
        assert!(can_run);
    }

    #[tokio::test]
    async fn test_power_recommendation() {
        let provider = Arc::new(MockProvider::new());
        let engine = EdgeEngine::new(provider);

        let recommendation = engine.get_power_recommendation().await.unwrap();
        // Battery at 75%, not charging -> should recommend Balanced
        assert_eq!(recommendation.recommended_mode, InferenceMode::Balanced);
    }

    #[tokio::test]
    async fn test_offline_capabilities() {
        let provider = Arc::new(MockProvider::new());
        let engine = EdgeEngine::new(provider);

        engine.refresh_models().await.unwrap();
        engine.load_model("model-1").await.unwrap();

        let capabilities = engine.get_offline_capabilities().await.unwrap();
        assert!(capabilities
            .available_capabilities
            .contains(&ModelCapability::Chat));
    }

    #[tokio::test]
    async fn test_model_unloading() {
        let provider = Arc::new(MockProvider::new());
        let engine = EdgeEngine::new(provider);

        engine.refresh_models().await.unwrap();
        engine.load_model("model-1").await.unwrap();
        engine.unload_model("model-1").await.unwrap();

        let loaded = engine.get_loaded_models().await;
        assert!(!loaded.contains(&"model-1".to_string()));
    }
}
