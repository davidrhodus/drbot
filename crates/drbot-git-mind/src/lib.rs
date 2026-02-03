//! Deep git integration - understands branches, PRs, blame, commit patterns.
//!
//! This crate provides:
//! - Repository analysis and understanding
//! - Commit pattern detection
//! - Branch relationship mapping
//! - Code ownership tracking
//! - Change impact analysis

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Git mind errors.
#[derive(Debug, Error)]
pub enum GitMindError {
    #[error("Not a git repository: {0}")]
    NotARepo(String),

    #[error("Git operation failed: {0}")]
    GitError(String),

    #[error("Analysis failed: {0}")]
    AnalysisFailed(String),

    #[error("Branch not found: {0}")]
    BranchNotFound(String),
}

/// Result type for git-mind operations.
pub type Result<T> = std::result::Result<T, GitMindError>;

/// A git commit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Commit {
    /// Commit hash.
    pub hash: String,
    /// Short hash.
    pub short_hash: String,
    /// Author name.
    pub author: String,
    /// Author email.
    pub email: String,
    /// Commit message.
    pub message: String,
    /// Commit timestamp.
    pub timestamp: DateTime<Utc>,
    /// Parent commits.
    pub parents: Vec<String>,
    /// Files changed.
    pub files_changed: Vec<FileChange>,
    /// Stats.
    pub stats: CommitStats,
}

/// File change in a commit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChange {
    /// File path.
    pub path: PathBuf,
    /// Change type.
    pub change_type: ChangeType,
    /// Lines added.
    pub additions: usize,
    /// Lines deleted.
    pub deletions: usize,
}

/// Types of changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChangeType {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
}

/// Commit statistics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CommitStats {
    /// Total files changed.
    pub files_changed: usize,
    /// Total insertions.
    pub insertions: usize,
    /// Total deletions.
    pub deletions: usize,
}

/// A git branch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Branch {
    /// Branch name.
    pub name: String,
    /// Is remote.
    pub is_remote: bool,
    /// Is current.
    pub is_current: bool,
    /// Head commit.
    pub head: String,
    /// Upstream branch.
    pub upstream: Option<String>,
    /// Commits ahead of upstream.
    pub ahead: usize,
    /// Commits behind upstream.
    pub behind: usize,
}

/// Blame information for a line.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlameLine {
    /// Line number.
    pub line: usize,
    /// Commit hash.
    pub commit: String,
    /// Author.
    pub author: String,
    /// When.
    pub timestamp: DateTime<Utc>,
    /// Line content.
    pub content: String,
}

/// Code ownership information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeOwnership {
    /// File path.
    pub path: PathBuf,
    /// Primary owner.
    pub primary_owner: String,
    /// Ownership distribution.
    pub ownership: HashMap<String, f64>,
    /// Recent contributors.
    pub recent_contributors: Vec<String>,
    /// Total commits.
    pub total_commits: usize,
}

/// Commit pattern analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitPattern {
    /// Pattern identifier.
    pub id: String,
    /// Pattern type.
    pub pattern_type: CommitPatternType,
    /// Description.
    pub description: String,
    /// Affected authors.
    pub authors: Vec<String>,
    /// Confidence.
    pub confidence: f64,
    /// Examples.
    pub examples: Vec<String>,
}

/// Types of commit patterns.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum CommitPatternType {
    /// Regular commit schedule.
    RegularSchedule,
    /// Large commits.
    LargeCommits,
    /// Many small commits.
    ManySmallCommits,
    /// Files that change together.
    CoChangedFiles,
    /// Bug fix patterns.
    BugFixPattern,
    /// Feature development.
    FeatureDevelopment,
    /// Refactoring.
    Refactoring,
}

/// Repository analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoAnalysis {
    /// Repository root.
    pub root: PathBuf,
    /// Total commits.
    pub total_commits: usize,
    /// Total branches.
    pub total_branches: usize,
    /// Total contributors.
    pub total_contributors: usize,
    /// Top contributors.
    pub top_contributors: Vec<(String, usize)>,
    /// Active branches.
    pub active_branches: Vec<String>,
    /// Detected patterns.
    pub patterns: Vec<CommitPattern>,
    /// Health score.
    pub health_score: f64,
}

/// Provider for git operations.
#[async_trait]
pub trait GitProvider: Send + Sync {
    /// Get commits.
    async fn get_commits(&self, branch: &str, limit: usize) -> Result<Vec<Commit>>;

    /// Get branches.
    async fn get_branches(&self) -> Result<Vec<Branch>>;

    /// Get blame for a file.
    async fn blame(&self, path: &PathBuf) -> Result<Vec<BlameLine>>;

    /// Get file history.
    async fn file_history(&self, path: &PathBuf, limit: usize) -> Result<Vec<Commit>>;

    /// Get diff between refs.
    async fn diff(&self, from: &str, to: &str) -> Result<Vec<FileChange>>;
}

/// The git mind engine.
pub struct GitMind {
    /// Git provider.
    provider: Arc<dyn GitProvider>,
    /// Repository root.
    root: PathBuf,
    /// Commit cache.
    commits: Arc<RwLock<HashMap<String, Commit>>>,
    /// Ownership cache.
    ownership: Arc<RwLock<HashMap<PathBuf, CodeOwnership>>>,
    /// Patterns.
    patterns: Arc<RwLock<Vec<CommitPattern>>>,
}

impl GitMind {
    /// Create a new GitMind instance.
    pub fn new(provider: Arc<dyn GitProvider>, root: PathBuf) -> Self {
        Self {
            provider,
            root,
            commits: Arc::new(RwLock::new(HashMap::new())),
            ownership: Arc::new(RwLock::new(HashMap::new())),
            patterns: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Analyze the repository.
    pub async fn analyze(&self) -> Result<RepoAnalysis> {
        let branches = self.provider.get_branches().await?;
        let commits = self.provider.get_commits("HEAD", 1000).await?;

        // Count contributors
        let mut contributor_counts: HashMap<String, usize> = HashMap::new();
        for commit in &commits {
            *contributor_counts.entry(commit.author.clone()).or_insert(0) += 1;
        }

        let mut top_contributors: Vec<_> = contributor_counts.into_iter().collect();
        top_contributors.sort_by(|a, b| b.1.cmp(&a.1));
        top_contributors.truncate(10);

        // Detect patterns
        let patterns = self.detect_patterns(&commits).await;

        // Calculate health score
        let health_score = self.calculate_health(&commits, &branches);

        Ok(RepoAnalysis {
            root: self.root.clone(),
            total_commits: commits.len(),
            total_branches: branches.len(),
            total_contributors: top_contributors.len(),
            top_contributors,
            active_branches: branches
                .iter()
                .filter(|b| !b.is_remote)
                .map(|b| b.name.clone())
                .collect(),
            patterns,
            health_score,
        })
    }

    /// Get commits for a branch.
    pub async fn get_commits(&self, branch: &str, limit: usize) -> Result<Vec<Commit>> {
        let commits = self.provider.get_commits(branch, limit).await?;

        // Cache commits
        let mut cache = self.commits.write().await;
        for commit in &commits {
            cache.insert(commit.hash.clone(), commit.clone());
        }

        Ok(commits)
    }

    /// Get blame for a file.
    pub async fn blame(&self, path: PathBuf) -> Result<Vec<BlameLine>> {
        self.provider.blame(&path).await
    }

    /// Calculate code ownership for a file.
    pub async fn get_ownership(&self, path: PathBuf) -> Result<CodeOwnership> {
        // Check cache
        {
            let cache = self.ownership.read().await;
            if let Some(ownership) = cache.get(&path) {
                return Ok(ownership.clone());
            }
        }

        let blame = self.provider.blame(&path).await?;

        // Calculate ownership percentages
        let mut author_lines: HashMap<String, usize> = HashMap::new();
        let total_lines = blame.len();

        for line in &blame {
            *author_lines.entry(line.author.clone()).or_insert(0) += 1;
        }

        let ownership_pct: HashMap<String, f64> = author_lines
            .iter()
            .map(|(author, count)| (author.clone(), *count as f64 / total_lines as f64))
            .collect();

        let primary_owner = ownership_pct
            .iter()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .map(|(author, _)| author.clone())
            .unwrap_or_default();

        let history = self.provider.file_history(&path, 10).await?;
        let recent_contributors: Vec<String> = history
            .iter()
            .map(|c| c.author.clone())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        let ownership = CodeOwnership {
            path: path.clone(),
            primary_owner,
            ownership: ownership_pct,
            recent_contributors,
            total_commits: history.len(),
        };

        // Cache
        let mut cache = self.ownership.write().await;
        cache.insert(path, ownership.clone());

        Ok(ownership)
    }

    /// Get file history.
    pub async fn file_history(&self, path: PathBuf, limit: usize) -> Result<Vec<Commit>> {
        self.provider.file_history(&path, limit).await
    }

    /// Detect commit patterns.
    async fn detect_patterns(&self, commits: &[Commit]) -> Vec<CommitPattern> {
        let mut patterns = Vec::new();

        // Detect large commits
        let large_commits: Vec<_> = commits
            .iter()
            .filter(|c| c.stats.files_changed > 20)
            .collect();
        if large_commits.len() > commits.len() / 10 {
            patterns.push(CommitPattern {
                id: Uuid::new_v4().to_string(),
                pattern_type: CommitPatternType::LargeCommits,
                description: "Many commits with large number of files".to_string(),
                authors: large_commits.iter().map(|c| c.author.clone()).collect(),
                confidence: 0.8,
                examples: large_commits
                    .iter()
                    .take(3)
                    .map(|c| c.short_hash.clone())
                    .collect(),
            });
        }

        // Detect co-changed files
        let mut file_pairs: HashMap<(PathBuf, PathBuf), usize> = HashMap::new();
        for commit in commits {
            let files: Vec<_> = commit
                .files_changed
                .iter()
                .map(|f| f.path.clone())
                .collect();
            for i in 0..files.len() {
                for j in (i + 1)..files.len() {
                    let pair = if files[i] < files[j] {
                        (files[i].clone(), files[j].clone())
                    } else {
                        (files[j].clone(), files[i].clone())
                    };
                    *file_pairs.entry(pair).or_insert(0) += 1;
                }
            }
        }

        let frequent_pairs: Vec<_> = file_pairs.iter().filter(|(_, count)| **count > 5).collect();
        if !frequent_pairs.is_empty() {
            patterns.push(CommitPattern {
                id: Uuid::new_v4().to_string(),
                pattern_type: CommitPatternType::CoChangedFiles,
                description: "Files that frequently change together".to_string(),
                authors: vec![],
                confidence: 0.9,
                examples: frequent_pairs
                    .iter()
                    .take(3)
                    .map(|((a, b), _)| format!("{} + {}", a.display(), b.display()))
                    .collect(),
            });
        }

        // Store patterns
        let mut stored = self.patterns.write().await;
        *stored = patterns.clone();

        patterns
    }

    /// Calculate repository health score.
    fn calculate_health(&self, commits: &[Commit], branches: &[Branch]) -> f64 {
        let mut score = 100.0;

        // Penalize stale branches
        let stale_branches = branches.iter().filter(|b| b.behind > 50).count();
        score -= stale_branches as f64 * 2.0;

        // Penalize very large commits
        let huge_commits = commits
            .iter()
            .filter(|c| c.stats.files_changed > 50)
            .count();
        score -= huge_commits as f64 * 1.0;

        // Reward regular activity
        if commits.len() > 10 {
            score += 5.0;
        }

        score.clamp(0.0, 100.0)
    }

    /// Get diff between two refs.
    pub async fn diff(&self, from: &str, to: &str) -> Result<Vec<FileChange>> {
        self.provider.diff(from, to).await
    }

    /// Suggest reviewers for files.
    pub async fn suggest_reviewers(&self, files: &[PathBuf]) -> Result<Vec<String>> {
        let mut reviewer_scores: HashMap<String, f64> = HashMap::new();

        for file in files {
            if let Ok(ownership) = self.get_ownership(file.clone()).await {
                for (author, pct) in ownership.ownership {
                    *reviewer_scores.entry(author).or_insert(0.0) += pct;
                }
            }
        }

        let mut reviewers: Vec<_> = reviewer_scores.into_iter().collect();
        reviewers.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

        Ok(reviewers
            .into_iter()
            .take(5)
            .map(|(name, _)| name)
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockProvider;

    #[async_trait]
    impl GitProvider for MockProvider {
        async fn get_commits(&self, _branch: &str, limit: usize) -> Result<Vec<Commit>> {
            Ok((0..limit.min(10))
                .map(|i| Commit {
                    hash: format!("abc{:04}def", i),
                    short_hash: format!("abc{:04}", i),
                    author: format!("Author {}", i % 3),
                    email: format!("author{}@example.com", i % 3),
                    message: format!("Commit {}", i),
                    timestamp: Utc::now(),
                    parents: vec![],
                    files_changed: vec![FileChange {
                        path: PathBuf::from(format!("file{}.rs", i)),
                        change_type: ChangeType::Modified,
                        additions: 10,
                        deletions: 5,
                    }],
                    stats: CommitStats {
                        files_changed: 1,
                        insertions: 10,
                        deletions: 5,
                    },
                })
                .collect())
        }

        async fn get_branches(&self) -> Result<Vec<Branch>> {
            Ok(vec![
                Branch {
                    name: "main".to_string(),
                    is_remote: false,
                    is_current: true,
                    head: "abc123".to_string(),
                    upstream: Some("origin/main".to_string()),
                    ahead: 0,
                    behind: 0,
                },
                Branch {
                    name: "feature".to_string(),
                    is_remote: false,
                    is_current: false,
                    head: "def456".to_string(),
                    upstream: None,
                    ahead: 3,
                    behind: 1,
                },
            ])
        }

        async fn blame(&self, path: &PathBuf) -> Result<Vec<BlameLine>> {
            Ok((1..=10)
                .map(|i| BlameLine {
                    line: i,
                    commit: "abc123".to_string(),
                    author: format!("Author {}", i % 2),
                    timestamp: Utc::now(),
                    content: format!("Line {} content", i),
                })
                .collect())
        }

        async fn file_history(&self, _path: &PathBuf, limit: usize) -> Result<Vec<Commit>> {
            self.get_commits("HEAD", limit).await
        }

        async fn diff(&self, _from: &str, _to: &str) -> Result<Vec<FileChange>> {
            Ok(vec![FileChange {
                path: PathBuf::from("changed.rs"),
                change_type: ChangeType::Modified,
                additions: 20,
                deletions: 10,
            }])
        }
    }

    #[tokio::test]
    async fn test_analyze() {
        let provider = Arc::new(MockProvider);
        let mind = GitMind::new(provider, PathBuf::from("/repo"));

        let analysis = mind.analyze().await.unwrap();
        assert!(analysis.total_commits > 0);
        assert!(analysis.total_branches > 0);
    }

    #[tokio::test]
    async fn test_get_commits() {
        let provider = Arc::new(MockProvider);
        let mind = GitMind::new(provider, PathBuf::from("/repo"));

        let commits = mind.get_commits("main", 5).await.unwrap();
        assert_eq!(commits.len(), 5);
    }

    #[tokio::test]
    async fn test_blame() {
        let provider = Arc::new(MockProvider);
        let mind = GitMind::new(provider, PathBuf::from("/repo"));

        let blame = mind.blame(PathBuf::from("test.rs")).await.unwrap();
        assert_eq!(blame.len(), 10);
    }

    #[tokio::test]
    async fn test_ownership() {
        let provider = Arc::new(MockProvider);
        let mind = GitMind::new(provider, PathBuf::from("/repo"));

        let ownership = mind.get_ownership(PathBuf::from("test.rs")).await.unwrap();
        assert!(!ownership.primary_owner.is_empty());
    }

    #[tokio::test]
    async fn test_suggest_reviewers() {
        let provider = Arc::new(MockProvider);
        let mind = GitMind::new(provider, PathBuf::from("/repo"));

        let reviewers = mind
            .suggest_reviewers(&[PathBuf::from("test.rs")])
            .await
            .unwrap();
        assert!(!reviewers.is_empty());
    }
}
