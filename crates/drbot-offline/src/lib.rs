//! Offline-first AI support for drbot.
//!
//! Provides seamless operation with local models when cloud is unavailable.
//!
//! # Features
//!
//! - Local model management (Ollama, llama.cpp)
//! - Automatic cloud/local fallback
//! - Network connectivity detection
//! - Model downloading and caching
//! - Offline queue for deferred requests

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

/// Offline result type.
pub type Result<T> = std::result::Result<T, OfflineError>;

/// Offline errors.
#[derive(Debug, thiserror::Error)]
pub enum OfflineError {
    #[error("No local model available")]
    NoLocalModel,
    #[error("Model not downloaded: {0}")]
    ModelNotDownloaded(String),
    #[error("Download failed: {0}")]
    DownloadFailed(String),
    #[error("Network unavailable")]
    NetworkUnavailable,
    #[error("Local inference failed: {0}")]
    InferenceFailed(String),
}

/// Network connectivity status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectivityStatus {
    /// Full internet connectivity.
    Online,
    /// Limited connectivity (local network only).
    Limited,
    /// No network connectivity.
    Offline,
    /// Status unknown/checking.
    Unknown,
}

/// Local model information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalModel {
    /// Model ID.
    pub id: String,
    /// Model name.
    pub name: String,
    /// Model provider (ollama, llamacpp, etc.).
    pub provider: LocalProvider,
    /// Model size in bytes.
    pub size_bytes: u64,
    /// Whether the model is downloaded.
    pub downloaded: bool,
    /// Download progress (0-100).
    pub download_progress: u8,
    /// Model capabilities.
    pub capabilities: ModelCapabilities,
    /// Last used timestamp.
    pub last_used: Option<DateTime<Utc>>,
}

/// Local model providers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalProvider {
    /// Ollama.
    Ollama,
    /// llama.cpp.
    LlamaCpp,
    /// MLX (Apple Silicon).
    Mlx,
    /// Custom local provider.
    Custom,
}

/// Model capabilities.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelCapabilities {
    /// Supports chat/conversation.
    pub chat: bool,
    /// Supports code generation.
    pub code: bool,
    /// Supports embeddings.
    pub embeddings: bool,
    /// Supports vision/images.
    pub vision: bool,
    /// Context window size.
    pub context_window: usize,
    /// Supports function calling.
    pub function_calling: bool,
}

/// Offline configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OfflineConfig {
    /// Enable offline mode.
    pub enabled: bool,
    /// Preferred local provider.
    pub preferred_provider: LocalProvider,
    /// Auto-download models when online.
    pub auto_download: bool,
    /// Models to keep downloaded.
    pub pinned_models: Vec<String>,
    /// Fallback strategy.
    pub fallback_strategy: FallbackStrategy,
    /// Queue requests when offline.
    pub queue_when_offline: bool,
    /// Max queued requests.
    pub max_queue_size: usize,
    /// Connectivity check interval (seconds).
    pub connectivity_check_interval: u64,
}

impl Default for OfflineConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            preferred_provider: LocalProvider::Ollama,
            auto_download: false,
            pinned_models: vec!["llama3.2:3b".to_string()],
            fallback_strategy: FallbackStrategy::LocalFirst,
            queue_when_offline: true,
            max_queue_size: 100,
            connectivity_check_interval: 30,
        }
    }
}

/// Fallback strategy when primary provider fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FallbackStrategy {
    /// Try local models first, then cloud.
    LocalFirst,
    /// Try cloud first, then local.
    CloudFirst,
    /// Only use local models.
    LocalOnly,
    /// Only use cloud models.
    CloudOnly,
    /// Use fastest available.
    Fastest,
    /// Use cheapest available.
    Cheapest,
}

/// Queued request for offline processing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueuedRequest {
    /// Request ID.
    pub id: Uuid,
    /// Request type.
    pub request_type: RequestType,
    /// Request payload.
    pub payload: serde_json::Value,
    /// Priority (higher = more important).
    pub priority: u8,
    /// Queued at.
    pub queued_at: DateTime<Utc>,
    /// Retry count.
    pub retry_count: u32,
    /// Max retries.
    pub max_retries: u32,
}

/// Request types that can be queued.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestType {
    Chat,
    Embedding,
    ImageGeneration,
    Transcription,
    Custom,
}

/// Connectivity event.
#[derive(Debug, Clone)]
pub enum ConnectivityEvent {
    /// Status changed.
    StatusChanged {
        from: ConnectivityStatus,
        to: ConnectivityStatus,
    },
    /// Provider became available.
    ProviderAvailable { provider: String },
    /// Provider became unavailable.
    ProviderUnavailable { provider: String },
}

/// Offline manager.
pub struct OfflineManager {
    config: OfflineConfig,
    connectivity: Arc<RwLock<ConnectivityStatus>>,
    local_models: Arc<RwLock<HashMap<String, LocalModel>>>,
    request_queue: Arc<RwLock<VecDeque<QueuedRequest>>>,
    event_sender: broadcast::Sender<ConnectivityEvent>,
}

impl OfflineManager {
    /// Create a new offline manager.
    pub fn new(config: OfflineConfig) -> Self {
        let (event_sender, _) = broadcast::channel(64);

        Self {
            config,
            connectivity: Arc::new(RwLock::new(ConnectivityStatus::Unknown)),
            local_models: Arc::new(RwLock::new(HashMap::new())),
            request_queue: Arc::new(RwLock::new(VecDeque::new())),
            event_sender,
        }
    }

    /// Get current connectivity status.
    pub async fn connectivity(&self) -> ConnectivityStatus {
        *self.connectivity.read().await
    }

    /// Check connectivity and update status.
    pub async fn check_connectivity(&self) -> ConnectivityStatus {
        let new_status = self.probe_connectivity().await;
        let old_status = {
            let mut conn = self.connectivity.write().await;
            let old = *conn;
            *conn = new_status;
            old
        };

        if old_status != new_status {
            let _ = self.event_sender.send(ConnectivityEvent::StatusChanged {
                from: old_status,
                to: new_status,
            });

            // Process queue if we came online
            if new_status == ConnectivityStatus::Online {
                self.process_queue().await;
            }
        }

        new_status
    }

    async fn probe_connectivity(&self) -> ConnectivityStatus {
        // Try to reach common endpoints
        let endpoints = [
            "https://api.anthropic.com",
            "https://api.openai.com",
            "https://1.1.1.1",
        ];

        for endpoint in endpoints {
            if let Ok(response) = reqwest::Client::new()
                .head(endpoint)
                .timeout(std::time::Duration::from_secs(5))
                .send()
                .await
            {
                if response.status().is_success() || response.status().is_client_error() {
                    return ConnectivityStatus::Online;
                }
            }
        }

        // Check if local network is available
        if let Ok(_) = tokio::net::TcpStream::connect("127.0.0.1:11434").await {
            // Ollama is running locally
            return ConnectivityStatus::Limited;
        }

        ConnectivityStatus::Offline
    }

    /// Register a local model.
    pub async fn register_model(&self, model: LocalModel) {
        self.local_models
            .write()
            .await
            .insert(model.id.clone(), model);
    }

    /// Get available local models.
    pub async fn available_models(&self) -> Vec<LocalModel> {
        self.local_models
            .read()
            .await
            .values()
            .filter(|m| m.downloaded)
            .cloned()
            .collect()
    }

    /// Get best available model for a task.
    pub async fn best_model_for(&self, capabilities: &ModelCapabilities) -> Option<LocalModel> {
        let models = self.local_models.read().await;

        models
            .values()
            .filter(|m| {
                m.downloaded
                    && (!capabilities.chat || m.capabilities.chat)
                    && (!capabilities.code || m.capabilities.code)
                    && (!capabilities.embeddings || m.capabilities.embeddings)
                    && (!capabilities.vision || m.capabilities.vision)
            })
            .max_by_key(|m| m.capabilities.context_window)
            .cloned()
    }

    /// Download a model.
    pub async fn download_model(&self, model_id: &str) -> Result<()> {
        let mut models = self.local_models.write().await;

        if let Some(model) = models.get_mut(model_id) {
            // Simulate download progress
            model.download_progress = 0;

            // In real implementation, would stream download and update progress
            // For now, mark as downloaded
            model.downloaded = true;
            model.download_progress = 100;

            Ok(())
        } else {
            Err(OfflineError::ModelNotDownloaded(model_id.to_string()))
        }
    }

    /// Queue a request for later processing.
    pub async fn queue_request(&self, request: QueuedRequest) -> Result<Uuid> {
        if !self.config.queue_when_offline {
            return Err(OfflineError::NetworkUnavailable);
        }

        let mut queue = self.request_queue.write().await;

        if queue.len() >= self.config.max_queue_size {
            // Remove lowest priority request
            if let Some(pos) = queue.iter().position(|r| r.priority < request.priority) {
                queue.remove(pos);
            } else {
                return Err(OfflineError::NetworkUnavailable);
            }
        }

        let id = request.id;
        queue.push_back(request);

        // Sort by priority
        queue
            .make_contiguous()
            .sort_by(|a, b| b.priority.cmp(&a.priority));

        Ok(id)
    }

    /// Process queued requests.
    pub async fn process_queue(&self) {
        let mut queue = self.request_queue.write().await;

        while let Some(mut request) = queue.pop_front() {
            // Try to process request
            // In real implementation, would actually send request

            if request.retry_count < request.max_retries {
                request.retry_count += 1;
                // If failed, re-queue with lower priority
                // queue.push_back(request);
            }
        }
    }

    /// Get queue status.
    pub async fn queue_status(&self) -> QueueStatus {
        let queue = self.request_queue.read().await;

        QueueStatus {
            pending_count: queue.len(),
            oldest_request: queue.front().map(|r| r.queued_at),
            total_size: queue.len(),
        }
    }

    /// Should use local model based on strategy.
    pub async fn should_use_local(&self) -> bool {
        let connectivity = self.connectivity().await;

        match self.config.fallback_strategy {
            FallbackStrategy::LocalOnly => true,
            FallbackStrategy::CloudOnly => false,
            FallbackStrategy::LocalFirst => true,
            FallbackStrategy::CloudFirst => connectivity != ConnectivityStatus::Online,
            FallbackStrategy::Fastest => {
                // Local is usually faster for small requests
                true
            }
            FallbackStrategy::Cheapest => {
                // Local is always cheaper
                true
            }
        }
    }

    /// Subscribe to connectivity events.
    pub fn subscribe(&self) -> broadcast::Receiver<ConnectivityEvent> {
        self.event_sender.subscribe()
    }

    /// Start background connectivity monitoring.
    pub fn start_monitoring(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        let interval = self.config.connectivity_check_interval;

        tokio::spawn(async move {
            loop {
                self.check_connectivity().await;
                tokio::time::sleep(tokio::time::Duration::from_secs(interval)).await;
            }
        })
    }
}

/// Queue status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueStatus {
    /// Number of pending requests.
    pub pending_count: usize,
    /// Oldest request timestamp.
    pub oldest_request: Option<DateTime<Utc>>,
    /// Total queue size.
    pub total_size: usize,
}

/// Trait for providers that support offline mode.
#[async_trait]
pub trait OfflineCapable: Send + Sync {
    /// Check if provider is available offline.
    fn supports_offline(&self) -> bool;

    /// Get local model requirements.
    fn required_models(&self) -> Vec<String>;

    /// Check if ready for offline use.
    async fn is_offline_ready(&self) -> bool;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_offline_manager() {
        let manager = OfflineManager::new(OfflineConfig::default());

        let model = LocalModel {
            id: "llama3.2:3b".to_string(),
            name: "Llama 3.2 3B".to_string(),
            provider: LocalProvider::Ollama,
            size_bytes: 2_000_000_000,
            downloaded: true,
            download_progress: 100,
            capabilities: ModelCapabilities {
                chat: true,
                code: true,
                context_window: 8192,
                ..Default::default()
            },
            last_used: None,
        };

        manager.register_model(model).await;

        let models = manager.available_models().await;
        assert_eq!(models.len(), 1);
    }

    #[tokio::test]
    async fn test_queue_request() {
        let manager = OfflineManager::new(OfflineConfig::default());

        let request = QueuedRequest {
            id: Uuid::new_v4(),
            request_type: RequestType::Chat,
            payload: serde_json::json!({"message": "hello"}),
            priority: 5,
            queued_at: Utc::now(),
            retry_count: 0,
            max_retries: 3,
        };

        let id = manager.queue_request(request).await.unwrap();
        let status = manager.queue_status().await;

        assert_eq!(status.pending_count, 1);
    }

    #[test]
    fn test_fallback_strategy() {
        let config = OfflineConfig {
            fallback_strategy: FallbackStrategy::LocalFirst,
            ..Default::default()
        };

        assert_eq!(config.fallback_strategy, FallbackStrategy::LocalFirst);
    }
}
