//! File system intelligence - watches changes, understands project structure.
//!
//! This crate provides:
//! - Real-time file system monitoring
//! - Project structure understanding
//! - Change pattern detection
//! - Intelligent file categorization

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

/// File system watch errors.
#[derive(Debug, Error)]
pub enum FsWatchError {
    #[error("Watch failed: {0}")]
    WatchFailed(String),

    #[error("Path not found: {0}")]
    PathNotFound(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Analysis failed: {0}")]
    AnalysisFailed(String),
}

/// Result type for fs-watch operations.
pub type Result<T> = std::result::Result<T, FsWatchError>;

/// A file system event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsEvent {
    /// Event identifier.
    pub id: String,
    /// Event type.
    pub event_type: FsEventType,
    /// Affected path.
    pub path: PathBuf,
    /// Old path (for renames).
    pub old_path: Option<PathBuf>,
    /// File metadata.
    pub metadata: FileMetadata,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
}

/// Types of file system events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FsEventType {
    Created,
    Modified,
    Deleted,
    Renamed,
    MetadataChanged,
}

/// File metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMetadata {
    /// File size in bytes.
    pub size: u64,
    /// Is directory.
    pub is_dir: bool,
    /// File extension.
    pub extension: Option<String>,
    /// MIME type guess.
    pub mime_type: Option<String>,
    /// Last modified.
    pub modified: Option<DateTime<Utc>>,
}

impl Default for FileMetadata {
    fn default() -> Self {
        Self {
            size: 0,
            is_dir: false,
            extension: None,
            mime_type: None,
            modified: None,
        }
    }
}

/// Project structure analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectStructure {
    /// Root path.
    pub root: PathBuf,
    /// Project type.
    pub project_type: ProjectType,
    /// Source directories.
    pub source_dirs: Vec<PathBuf>,
    /// Test directories.
    pub test_dirs: Vec<PathBuf>,
    /// Config files.
    pub config_files: Vec<PathBuf>,
    /// Documentation files.
    pub doc_files: Vec<PathBuf>,
    /// Build artifacts directories.
    pub build_dirs: Vec<PathBuf>,
    /// Total file count.
    pub file_count: usize,
    /// Languages detected.
    pub languages: Vec<String>,
}

/// Project types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProjectType {
    Rust,
    Node,
    Python,
    Go,
    Java,
    CSharp,
    Ruby,
    Mixed,
    Unknown,
}

/// Watch configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchConfig {
    /// Paths to watch.
    pub paths: Vec<PathBuf>,
    /// Patterns to ignore.
    pub ignore_patterns: Vec<String>,
    /// Recursive watching.
    pub recursive: bool,
    /// Debounce delay in milliseconds.
    pub debounce_ms: u64,
}

impl Default for WatchConfig {
    fn default() -> Self {
        Self {
            paths: vec![],
            ignore_patterns: vec![
                "**/node_modules/**".to_string(),
                "**/target/**".to_string(),
                "**/.git/**".to_string(),
                "**/dist/**".to_string(),
                "**/__pycache__/**".to_string(),
            ],
            recursive: true,
            debounce_ms: 100,
        }
    }
}

/// Change pattern detected over time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangePattern {
    /// Pattern identifier.
    pub id: String,
    /// Pattern type.
    pub pattern_type: ChangePatternType,
    /// Files involved.
    pub files: Vec<PathBuf>,
    /// Frequency (events per hour).
    pub frequency: f64,
    /// Confidence.
    pub confidence: f64,
    /// Description.
    pub description: String,
}

/// Types of change patterns.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ChangePatternType {
    /// Files that change together.
    CoChange,
    /// Rapid modifications (hot file).
    HotFile,
    /// Regular save pattern.
    AutoSave,
    /// Build artifact generation.
    BuildOutput,
    /// Test file changes following source.
    TestAfterSource,
}

/// File categorization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileCategory {
    Source,
    Test,
    Config,
    Documentation,
    Data,
    Media,
    Build,
    Dependency,
    Unknown,
}

/// Provider for file system analysis.
#[async_trait]
pub trait FsAnalysisProvider: Send + Sync {
    /// Analyze project structure.
    async fn analyze_structure(&self, root: &PathBuf) -> Result<ProjectStructure>;

    /// Categorize a file.
    async fn categorize_file(&self, path: &PathBuf) -> Result<FileCategory>;

    /// Detect change patterns.
    async fn detect_patterns(&self, events: &[FsEvent]) -> Result<Vec<ChangePattern>>;

    /// Get context for a file.
    async fn get_file_context(&self, path: &PathBuf) -> Result<FileContext>;
}

/// Context information about a file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileContext {
    /// The file path.
    pub path: PathBuf,
    /// Category.
    pub category: FileCategory,
    /// Related files.
    pub related_files: Vec<PathBuf>,
    /// Project role.
    pub role: String,
    /// Recent changes.
    pub recent_changes: Vec<FsEvent>,
    /// Importance score.
    pub importance: f64,
}

/// The file system watcher engine.
pub struct FsWatcher {
    /// Analysis provider.
    provider: Arc<dyn FsAnalysisProvider>,
    /// Watch configuration.
    config: WatchConfig,
    /// Event history.
    events: Arc<RwLock<Vec<FsEvent>>>,
    /// Event broadcaster.
    event_tx: broadcast::Sender<FsEvent>,
    /// Detected patterns.
    patterns: Arc<RwLock<Vec<ChangePattern>>>,
    /// Project structure cache.
    structure_cache: Arc<RwLock<Option<ProjectStructure>>>,
}

impl FsWatcher {
    /// Create a new file system watcher.
    pub fn new(provider: Arc<dyn FsAnalysisProvider>, config: WatchConfig) -> Self {
        let (event_tx, _) = broadcast::channel(1000);
        Self {
            provider,
            config,
            events: Arc::new(RwLock::new(Vec::new())),
            event_tx,
            patterns: Arc::new(RwLock::new(Vec::new())),
            structure_cache: Arc::new(RwLock::new(None)),
        }
    }

    /// Subscribe to file system events.
    pub fn subscribe(&self) -> broadcast::Receiver<FsEvent> {
        self.event_tx.subscribe()
    }

    /// Record an event (called by platform-specific watchers).
    pub async fn record_event(&self, event: FsEvent) -> Result<()> {
        let mut events = self.events.write().await;
        events.push(event.clone());

        // Keep last 10000 events
        if events.len() > 10000 {
            events.drain(0..1000);
        }

        let _ = self.event_tx.send(event);
        Ok(())
    }

    /// Analyze the project structure.
    pub async fn analyze_project(&self, root: PathBuf) -> Result<ProjectStructure> {
        let structure = self.provider.analyze_structure(&root).await?;

        let mut cache = self.structure_cache.write().await;
        *cache = Some(structure.clone());

        Ok(structure)
    }

    /// Get cached project structure.
    pub async fn get_structure(&self) -> Option<ProjectStructure> {
        let cache = self.structure_cache.read().await;
        cache.clone()
    }

    /// Categorize a file.
    pub async fn categorize(&self, path: PathBuf) -> Result<FileCategory> {
        self.provider.categorize_file(&path).await
    }

    /// Get context for a file.
    pub async fn get_context(&self, path: PathBuf) -> Result<FileContext> {
        self.provider.get_file_context(&path).await
    }

    /// Detect patterns in recent events.
    pub async fn detect_patterns(&self) -> Result<Vec<ChangePattern>> {
        let events = self.events.read().await;
        let patterns = self.provider.detect_patterns(&events).await?;

        let mut stored = self.patterns.write().await;
        *stored = patterns.clone();

        Ok(patterns)
    }

    /// Get recently changed files.
    pub async fn get_recent_changes(&self, limit: usize) -> Vec<FsEvent> {
        let events = self.events.read().await;
        events.iter().rev().take(limit).cloned().collect()
    }

    /// Get hot files (frequently modified).
    pub async fn get_hot_files(&self, threshold: usize) -> Vec<(PathBuf, usize)> {
        let events = self.events.read().await;
        let mut counts: HashMap<PathBuf, usize> = HashMap::new();

        for event in events.iter() {
            if event.event_type == FsEventType::Modified {
                *counts.entry(event.path.clone()).or_insert(0) += 1;
            }
        }

        let mut hot: Vec<_> = counts
            .into_iter()
            .filter(|(_, count)| *count >= threshold)
            .collect();
        hot.sort_by(|a, b| b.1.cmp(&a.1));
        hot
    }

    /// Check if path matches ignore patterns.
    pub fn should_ignore(&self, path: &PathBuf) -> bool {
        let path_str = path.to_string_lossy();
        for pattern in &self.config.ignore_patterns {
            if Self::glob_match(pattern, &path_str) {
                return true;
            }
        }
        false
    }

    /// Simple glob matching.
    fn glob_match(pattern: &str, path: &str) -> bool {
        if pattern.contains("**") {
            let parts: Vec<&str> = pattern.split("**").collect();
            if parts.len() == 2 {
                let start = parts[0].trim_end_matches('/');
                let end = parts[1].trim_start_matches('/');
                return (start.is_empty() || path.contains(start))
                    && (end.is_empty() || path.contains(end));
            }
        }
        path.contains(pattern.trim_matches('*'))
    }

    /// Get watch statistics.
    pub async fn stats(&self) -> WatchStats {
        let events = self.events.read().await;
        let patterns = self.patterns.read().await;

        let mut by_type: HashMap<FsEventType, usize> = HashMap::new();
        for event in events.iter() {
            *by_type.entry(event.event_type).or_insert(0) += 1;
        }

        WatchStats {
            total_events: events.len(),
            events_by_type: by_type,
            pattern_count: patterns.len(),
            watched_paths: self.config.paths.len(),
        }
    }
}

/// Watch statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchStats {
    /// Total events recorded.
    pub total_events: usize,
    /// Events by type.
    pub events_by_type: HashMap<FsEventType, usize>,
    /// Detected patterns.
    pub pattern_count: usize,
    /// Number of watched paths.
    pub watched_paths: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockProvider;

    #[async_trait]
    impl FsAnalysisProvider for MockProvider {
        async fn analyze_structure(&self, root: &PathBuf) -> Result<ProjectStructure> {
            Ok(ProjectStructure {
                root: root.clone(),
                project_type: ProjectType::Rust,
                source_dirs: vec![root.join("src")],
                test_dirs: vec![root.join("tests")],
                config_files: vec![root.join("Cargo.toml")],
                doc_files: vec![root.join("README.md")],
                build_dirs: vec![root.join("target")],
                file_count: 100,
                languages: vec!["Rust".to_string()],
            })
        }

        async fn categorize_file(&self, path: &PathBuf) -> Result<FileCategory> {
            let ext = path.extension().and_then(|e| e.to_str());
            Ok(match ext {
                Some("rs") => FileCategory::Source,
                Some("toml") => FileCategory::Config,
                Some("md") => FileCategory::Documentation,
                _ => FileCategory::Unknown,
            })
        }

        async fn detect_patterns(&self, events: &[FsEvent]) -> Result<Vec<ChangePattern>> {
            if events.len() > 5 {
                Ok(vec![ChangePattern {
                    id: Uuid::new_v4().to_string(),
                    pattern_type: ChangePatternType::HotFile,
                    files: vec![],
                    frequency: 10.0,
                    confidence: 0.8,
                    description: "Frequent modifications detected".to_string(),
                }])
            } else {
                Ok(vec![])
            }
        }

        async fn get_file_context(&self, path: &PathBuf) -> Result<FileContext> {
            Ok(FileContext {
                path: path.clone(),
                category: FileCategory::Source,
                related_files: vec![],
                role: "Source file".to_string(),
                recent_changes: vec![],
                importance: 0.5,
            })
        }
    }

    #[tokio::test]
    async fn test_record_event() {
        let provider = Arc::new(MockProvider);
        let watcher = FsWatcher::new(provider, WatchConfig::default());

        let event = FsEvent {
            id: Uuid::new_v4().to_string(),
            event_type: FsEventType::Modified,
            path: PathBuf::from("/test/file.rs"),
            old_path: None,
            metadata: FileMetadata::default(),
            timestamp: Utc::now(),
        };

        watcher.record_event(event).await.unwrap();

        let recent = watcher.get_recent_changes(10).await;
        assert_eq!(recent.len(), 1);
    }

    #[tokio::test]
    async fn test_analyze_project() {
        let provider = Arc::new(MockProvider);
        let watcher = FsWatcher::new(provider, WatchConfig::default());

        let structure = watcher
            .analyze_project(PathBuf::from("/test"))
            .await
            .unwrap();
        assert_eq!(structure.project_type, ProjectType::Rust);
    }

    #[tokio::test]
    async fn test_categorize() {
        let provider = Arc::new(MockProvider);
        let watcher = FsWatcher::new(provider, WatchConfig::default());

        let cat = watcher
            .categorize(PathBuf::from("/test/main.rs"))
            .await
            .unwrap();
        assert_eq!(cat, FileCategory::Source);
    }

    #[tokio::test]
    async fn test_ignore_patterns() {
        let provider = Arc::new(MockProvider);
        let watcher = FsWatcher::new(provider, WatchConfig::default());

        assert!(watcher.should_ignore(&PathBuf::from("/project/node_modules/pkg/index.js")));
        assert!(watcher.should_ignore(&PathBuf::from("/project/target/debug/main")));
        assert!(!watcher.should_ignore(&PathBuf::from("/project/src/main.rs")));
    }

    #[tokio::test]
    async fn test_hot_files() {
        let provider = Arc::new(MockProvider);
        let watcher = FsWatcher::new(provider, WatchConfig::default());

        let path = PathBuf::from("/test/hot.rs");
        for _ in 0..5 {
            watcher
                .record_event(FsEvent {
                    id: Uuid::new_v4().to_string(),
                    event_type: FsEventType::Modified,
                    path: path.clone(),
                    old_path: None,
                    metadata: FileMetadata::default(),
                    timestamp: Utc::now(),
                })
                .await
                .unwrap();
        }

        let hot = watcher.get_hot_files(3).await;
        assert!(!hot.is_empty());
        assert_eq!(hot[0].1, 5);
    }

    #[tokio::test]
    async fn test_stats() {
        let provider = Arc::new(MockProvider);
        let watcher = FsWatcher::new(provider, WatchConfig::default());

        watcher
            .record_event(FsEvent {
                id: Uuid::new_v4().to_string(),
                event_type: FsEventType::Created,
                path: PathBuf::from("/test/new.rs"),
                old_path: None,
                metadata: FileMetadata::default(),
                timestamp: Utc::now(),
            })
            .await
            .unwrap();

        let stats = watcher.stats().await;
        assert_eq!(stats.total_events, 1);
    }
}
