//! AI-powered file organization for drbot.
//!
//! Intelligent file management and tagging.
//!
//! # Features
//!
//! - Auto-organization
//! - Smart tagging
//! - Duplicate detection
//! - Folder suggestions
//! - Cleanup recommendations

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Organize result type.
pub type Result<T> = std::result::Result<T, OrganizeError>;

/// Organize errors.
#[derive(Debug, thiserror::Error)]
pub enum OrganizeError {
    #[error("File not found: {0}")]
    FileNotFound(String),
    #[error("Move failed: {0}")]
    MoveFailed(String),
    #[error("Analysis failed: {0}")]
    AnalysisFailed(String),
}

/// File info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    /// File ID.
    pub id: Uuid,
    /// File path.
    pub path: PathBuf,
    /// File name.
    pub name: String,
    /// Extension.
    pub extension: Option<String>,
    /// Size in bytes.
    pub size: u64,
    /// Created at.
    pub created: DateTime<Utc>,
    /// Modified at.
    pub modified: DateTime<Utc>,
    /// Tags.
    pub tags: Vec<String>,
    /// Category.
    pub category: Option<FileCategory>,
    /// Content hash.
    pub hash: Option<String>,
}

impl FileInfo {
    /// Create from path.
    pub fn new(path: PathBuf, name: &str, size: u64) -> Self {
        let extension = path.extension().map(|e| e.to_string_lossy().to_string());
        Self {
            id: Uuid::new_v4(),
            path,
            name: name.to_string(),
            extension,
            size,
            created: Utc::now(),
            modified: Utc::now(),
            tags: Vec::new(),
            category: None,
            hash: None,
        }
    }
}

/// File category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileCategory {
    Documents,
    Images,
    Videos,
    Audio,
    Code,
    Archives,
    Data,
    Executables,
    Fonts,
    Other,
}

impl FileCategory {
    /// Detect from extension.
    pub fn from_extension(ext: &str) -> Self {
        match ext.to_lowercase().as_str() {
            "pdf" | "doc" | "docx" | "txt" | "rtf" | "odt" | "pages" => FileCategory::Documents,
            "jpg" | "jpeg" | "png" | "gif" | "bmp" | "svg" | "webp" | "ico" | "heic" => {
                FileCategory::Images
            }
            "mp4" | "mov" | "avi" | "mkv" | "wmv" | "flv" | "webm" => FileCategory::Videos,
            "mp3" | "wav" | "flac" | "aac" | "ogg" | "m4a" | "wma" => FileCategory::Audio,
            "rs" | "py" | "js" | "ts" | "java" | "cpp" | "c" | "go" | "rb" | "php" | "swift"
            | "kt" => FileCategory::Code,
            "zip" | "rar" | "7z" | "tar" | "gz" | "bz2" | "xz" => FileCategory::Archives,
            "json" | "xml" | "csv" | "yaml" | "toml" | "sql" | "db" => FileCategory::Data,
            "exe" | "msi" | "dmg" | "app" | "deb" | "rpm" => FileCategory::Executables,
            "ttf" | "otf" | "woff" | "woff2" | "eot" => FileCategory::Fonts,
            _ => FileCategory::Other,
        }
    }
}

/// Organization suggestion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizeSuggestion {
    /// Suggestion ID.
    pub id: Uuid,
    /// File ID.
    pub file_id: Uuid,
    /// Action type.
    pub action: OrganizeAction,
    /// Target path.
    pub target: Option<PathBuf>,
    /// Reason.
    pub reason: String,
    /// Confidence.
    pub confidence: f32,
}

/// Organization action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrganizeAction {
    Move,
    Rename,
    Delete,
    Tag,
    Archive,
    Deduplicate,
}

/// Duplicate group.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateGroup {
    /// Group ID.
    pub id: Uuid,
    /// Files in group.
    pub files: Vec<FileInfo>,
    /// Match type.
    pub match_type: MatchType,
    /// Space wasted.
    pub wasted_bytes: u64,
}

/// Duplicate match type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchType {
    ExactHash,
    SameName,
    SimilarName,
    SameSize,
}

/// Folder stats.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderStats {
    /// Path.
    pub path: PathBuf,
    /// File count.
    pub file_count: usize,
    /// Total size.
    pub total_size: u64,
    /// By category.
    pub by_category: HashMap<FileCategory, usize>,
    /// Oldest file.
    pub oldest: Option<DateTime<Utc>>,
    /// Newest file.
    pub newest: Option<DateTime<Utc>>,
}

/// Cleanup recommendation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupRecommendation {
    /// Recommendation ID.
    pub id: Uuid,
    /// Files affected.
    pub files: Vec<Uuid>,
    /// Potential savings.
    pub savings_bytes: u64,
    /// Reason.
    pub reason: String,
    /// Risk level.
    pub risk: RiskLevel,
}

/// Risk level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Safe,
    Low,
    Medium,
    High,
}

/// Organize configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizeConfig {
    /// Enable auto-tagging.
    pub auto_tag: bool,
    /// Enable duplicate detection.
    pub detect_duplicates: bool,
    /// Minimum duplicate size (bytes).
    pub min_duplicate_size: u64,
    /// Organization rules.
    pub rules: Vec<OrganizeRule>,
}

impl Default for OrganizeConfig {
    fn default() -> Self {
        Self {
            auto_tag: true,
            detect_duplicates: true,
            min_duplicate_size: 1024,
            rules: Vec::new(),
        }
    }
}

/// Organization rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizeRule {
    /// Rule name.
    pub name: String,
    /// Match pattern.
    pub pattern: String,
    /// Target folder.
    pub target: PathBuf,
    /// Enabled.
    pub enabled: bool,
}

/// Trait for file analyzers.
#[async_trait]
pub trait FileAnalyzer: Send + Sync {
    /// Analyze file.
    async fn analyze(&self, file: &FileInfo) -> Result<FileAnalysis>;
    /// Suggest tags.
    async fn suggest_tags(&self, file: &FileInfo) -> Vec<String>;
}

/// File analysis result.
#[derive(Debug, Clone)]
pub struct FileAnalysis {
    /// Detected category.
    pub category: FileCategory,
    /// Suggested tags.
    pub tags: Vec<String>,
    /// Summary.
    pub summary: Option<String>,
}

/// Trait for organizers.
#[async_trait]
pub trait Organizer: Send + Sync {
    /// Suggest organization.
    async fn suggest(&self, files: &[FileInfo], config: &OrganizeConfig)
        -> Vec<OrganizeSuggestion>;
    /// Find duplicates.
    async fn find_duplicates(&self, files: &[FileInfo]) -> Vec<DuplicateGroup>;
}

/// File organizer engine.
pub struct FileOrganizer<A: FileAnalyzer, O: Organizer> {
    config: OrganizeConfig,
    analyzer: A,
    organizer: O,
    files: Arc<RwLock<HashMap<Uuid, FileInfo>>>,
}

impl<A: FileAnalyzer, O: Organizer> FileOrganizer<A, O> {
    /// Create a new file organizer.
    pub fn new(config: OrganizeConfig, analyzer: A, organizer: O) -> Self {
        Self {
            config,
            analyzer,
            organizer,
            files: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Index a file.
    pub async fn index(&self, mut file: FileInfo) -> Result<FileInfo> {
        // Analyze
        let analysis = self.analyzer.analyze(&file).await?;
        file.category = Some(analysis.category);

        // Auto-tag
        if self.config.auto_tag {
            file.tags = analysis.tags;
        }

        self.files.write().await.insert(file.id, file.clone());
        Ok(file)
    }

    /// Get file.
    pub async fn get(&self, id: Uuid) -> Option<FileInfo> {
        self.files.read().await.get(&id).cloned()
    }

    /// List files.
    pub async fn list(&self) -> Vec<FileInfo> {
        self.files.read().await.values().cloned().collect()
    }

    /// List by category.
    pub async fn list_by_category(&self, category: FileCategory) -> Vec<FileInfo> {
        self.files
            .read()
            .await
            .values()
            .filter(|f| f.category == Some(category))
            .cloned()
            .collect()
    }

    /// Search files.
    pub async fn search(&self, query: &str) -> Vec<FileInfo> {
        let query_lower = query.to_lowercase();
        self.files
            .read()
            .await
            .values()
            .filter(|f| {
                f.name.to_lowercase().contains(&query_lower)
                    || f.tags
                        .iter()
                        .any(|t| t.to_lowercase().contains(&query_lower))
            })
            .cloned()
            .collect()
    }

    /// Get organization suggestions.
    pub async fn get_suggestions(&self) -> Vec<OrganizeSuggestion> {
        let files: Vec<_> = self.files.read().await.values().cloned().collect();
        self.organizer.suggest(&files, &self.config).await
    }

    /// Find duplicates.
    pub async fn find_duplicates(&self) -> Vec<DuplicateGroup> {
        if !self.config.detect_duplicates {
            return Vec::new();
        }

        let files: Vec<_> = self
            .files
            .read()
            .await
            .values()
            .filter(|f| f.size >= self.config.min_duplicate_size)
            .cloned()
            .collect();

        self.organizer.find_duplicates(&files).await
    }

    /// Get cleanup recommendations.
    pub async fn get_cleanup_recommendations(&self) -> Vec<CleanupRecommendation> {
        let mut recommendations = Vec::new();
        let files = self.files.read().await;

        // Empty files
        let empty: Vec<_> = files.values().filter(|f| f.size == 0).collect();
        if !empty.is_empty() {
            recommendations.push(CleanupRecommendation {
                id: Uuid::new_v4(),
                files: empty.iter().map(|f| f.id).collect(),
                savings_bytes: 0,
                reason: format!("{} empty files found", empty.len()),
                risk: RiskLevel::Safe,
            });
        }

        // Duplicates
        drop(files);
        let duplicates = self.find_duplicates().await;
        for group in duplicates {
            if group.files.len() > 1 {
                recommendations.push(CleanupRecommendation {
                    id: Uuid::new_v4(),
                    files: group.files.iter().skip(1).map(|f| f.id).collect(),
                    savings_bytes: group.wasted_bytes,
                    reason: format!("{} duplicate files", group.files.len()),
                    risk: RiskLevel::Low,
                });
            }
        }

        recommendations
    }

    /// Tag file.
    pub async fn tag(&self, file_id: Uuid, tags: Vec<String>) -> Result<()> {
        let mut files = self.files.write().await;
        if let Some(file) = files.get_mut(&file_id) {
            file.tags.extend(tags);
            file.tags.sort();
            file.tags.dedup();
        }
        Ok(())
    }

    /// Get folder stats.
    pub async fn folder_stats(&self, folder: &PathBuf) -> FolderStats {
        let files = self.files.read().await;
        let folder_files: Vec<_> = files
            .values()
            .filter(|f| f.path.starts_with(folder))
            .collect();

        let total_size: u64 = folder_files.iter().map(|f| f.size).sum();

        let mut by_category: HashMap<FileCategory, usize> = HashMap::new();
        for file in &folder_files {
            if let Some(cat) = file.category {
                *by_category.entry(cat).or_insert(0) += 1;
            }
        }

        let oldest = folder_files.iter().map(|f| f.created).min();
        let newest = folder_files.iter().map(|f| f.modified).max();

        FolderStats {
            path: folder.clone(),
            file_count: folder_files.len(),
            total_size,
            by_category,
            oldest,
            newest,
        }
    }

    /// Get statistics.
    pub async fn stats(&self) -> OrganizeStats {
        let files = self.files.read().await;

        let total_size: u64 = files.values().map(|f| f.size).sum();
        let tagged = files.values().filter(|f| !f.tags.is_empty()).count();

        let mut by_category: HashMap<FileCategory, usize> = HashMap::new();
        for file in files.values() {
            if let Some(cat) = file.category {
                *by_category.entry(cat).or_insert(0) += 1;
            }
        }

        OrganizeStats {
            total_files: files.len(),
            total_size,
            tagged_files: tagged,
            by_category,
        }
    }
}

/// Organize statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizeStats {
    pub total_files: usize,
    pub total_size: u64,
    pub tagged_files: usize,
    pub by_category: HashMap<FileCategory, usize>,
}

/// Simple file analyzer for testing.
pub struct SimpleAnalyzer;

#[async_trait]
impl FileAnalyzer for SimpleAnalyzer {
    async fn analyze(&self, file: &FileInfo) -> Result<FileAnalysis> {
        let category = file
            .extension
            .as_ref()
            .map(|e| FileCategory::from_extension(e))
            .unwrap_or(FileCategory::Other);

        let tags = self.suggest_tags(file).await;

        Ok(FileAnalysis {
            category,
            tags,
            summary: None,
        })
    }

    async fn suggest_tags(&self, file: &FileInfo) -> Vec<String> {
        let mut tags = Vec::new();

        if let Some(ext) = &file.extension {
            tags.push(ext.clone());
        }

        if let Some(cat) = file.category {
            tags.push(format!("{:?}", cat).to_lowercase());
        }

        if file.size > 100 * 1024 * 1024 {
            tags.push("large".to_string());
        }

        tags
    }
}

/// Simple organizer for testing.
pub struct SimpleOrganizer;

#[async_trait]
impl Organizer for SimpleOrganizer {
    async fn suggest(
        &self,
        files: &[FileInfo],
        config: &OrganizeConfig,
    ) -> Vec<OrganizeSuggestion> {
        let mut suggestions = Vec::new();

        for rule in &config.rules {
            if !rule.enabled {
                continue;
            }

            for file in files {
                if file.name.contains(&rule.pattern) {
                    suggestions.push(OrganizeSuggestion {
                        id: Uuid::new_v4(),
                        file_id: file.id,
                        action: OrganizeAction::Move,
                        target: Some(rule.target.clone()),
                        reason: format!("Matches rule: {}", rule.name),
                        confidence: 0.8,
                    });
                }
            }
        }

        suggestions
    }

    async fn find_duplicates(&self, files: &[FileInfo]) -> Vec<DuplicateGroup> {
        let mut by_size: HashMap<u64, Vec<&FileInfo>> = HashMap::new();

        for file in files {
            by_size.entry(file.size).or_default().push(file);
        }

        by_size
            .into_iter()
            .filter(|(_, files)| files.len() > 1)
            .map(|(size, files)| {
                let wasted = size * (files.len() - 1) as u64;
                DuplicateGroup {
                    id: Uuid::new_v4(),
                    files: files.into_iter().cloned().collect(),
                    match_type: MatchType::SameSize,
                    wasted_bytes: wasted,
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_index_file() {
        let organizer =
            FileOrganizer::new(OrganizeConfig::default(), SimpleAnalyzer, SimpleOrganizer);

        let file = FileInfo::new(PathBuf::from("/docs/report.pdf"), "report.pdf", 1024);
        let indexed = organizer.index(file).await.unwrap();

        assert_eq!(indexed.category, Some(FileCategory::Documents));
    }

    #[tokio::test]
    async fn test_search() {
        let organizer =
            FileOrganizer::new(OrganizeConfig::default(), SimpleAnalyzer, SimpleOrganizer);

        organizer
            .index(FileInfo::new(
                PathBuf::from("/a.txt"),
                "budget_2024.txt",
                100,
            ))
            .await
            .unwrap();
        organizer
            .index(FileInfo::new(PathBuf::from("/b.txt"), "notes.txt", 100))
            .await
            .unwrap();

        let results = organizer.search("budget").await;
        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn test_find_duplicates() {
        let organizer =
            FileOrganizer::new(OrganizeConfig::default(), SimpleAnalyzer, SimpleOrganizer);

        organizer
            .index(FileInfo::new(PathBuf::from("/a.txt"), "file1.txt", 5000))
            .await
            .unwrap();
        organizer
            .index(FileInfo::new(PathBuf::from("/b.txt"), "file2.txt", 5000))
            .await
            .unwrap();

        let duplicates = organizer.find_duplicates().await;
        assert!(!duplicates.is_empty());
    }

    #[tokio::test]
    async fn test_category_detection() {
        assert_eq!(FileCategory::from_extension("pdf"), FileCategory::Documents);
        assert_eq!(FileCategory::from_extension("png"), FileCategory::Images);
        assert_eq!(FileCategory::from_extension("rs"), FileCategory::Code);
        assert_eq!(FileCategory::from_extension("mp3"), FileCategory::Audio);
    }
}
