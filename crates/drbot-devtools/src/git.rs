//! Git repository analysis.

use crate::{DevToolsError, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;

/// Git repository information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitInfo {
    /// Current branch name.
    pub branch: String,
    /// Whether working tree is clean.
    pub is_clean: bool,
    /// Current commit hash (short).
    pub commit_hash: String,
    /// Working tree status.
    pub status: GitStatus,
    /// Recent commits.
    pub recent_commits: Vec<GitCommit>,
    /// Remote origin URL.
    pub remote_url: Option<String>,
}

/// Git working tree status.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GitStatus {
    /// Staged files.
    pub staged: Vec<FileStatus>,
    /// Modified (unstaged) files.
    pub modified: Vec<FileStatus>,
    /// Untracked files.
    pub untracked: Vec<String>,
}

impl GitStatus {
    /// Count of changed files.
    pub fn changed_count(&self) -> usize {
        self.staged.len() + self.modified.len() + self.untracked.len()
    }

    /// Check if clean.
    pub fn is_clean(&self) -> bool {
        self.staged.is_empty() && self.modified.is_empty() && self.untracked.is_empty()
    }
}

/// File status in git.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileStatus {
    /// File path.
    pub path: String,
    /// Status (A=added, M=modified, D=deleted, R=renamed).
    pub status: char,
}

/// Git commit information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitCommit {
    /// Short commit hash.
    pub hash: String,
    /// Commit message (first line).
    pub message: String,
    /// Author name.
    pub author: String,
    /// Relative time (e.g., "2 hours ago").
    pub relative_time: String,
}

/// Get git information for a repository.
pub async fn get_git_info(path: &Path) -> Result<GitInfo> {
    // Check if it's a git repo
    if !path.join(".git").exists() {
        return Err(DevToolsError::NotGitRepo);
    }

    let branch = get_current_branch(path)?;
    let commit_hash = get_current_commit(path)?;
    let status = get_git_status(path)?;
    let recent_commits = get_recent_commits(path, 10)?;
    let remote_url = get_remote_url(path).ok();

    Ok(GitInfo {
        branch,
        is_clean: status.is_clean(),
        commit_hash,
        status,
        recent_commits,
        remote_url,
    })
}

fn get_current_branch(path: &Path) -> Result<String> {
    let output = Command::new("git")
        .current_dir(path)
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .map_err(|e| DevToolsError::GitError(e.to_string()))?;

    if !output.status.success() {
        return Err(DevToolsError::GitError("Failed to get branch".to_string()));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn get_current_commit(path: &Path) -> Result<String> {
    let output = Command::new("git")
        .current_dir(path)
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .map_err(|e| DevToolsError::GitError(e.to_string()))?;

    if !output.status.success() {
        return Err(DevToolsError::GitError("Failed to get commit".to_string()));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn get_git_status(path: &Path) -> Result<GitStatus> {
    let output = Command::new("git")
        .current_dir(path)
        .args(["status", "--porcelain"])
        .output()
        .map_err(|e| DevToolsError::GitError(e.to_string()))?;

    if !output.status.success() {
        return Err(DevToolsError::GitError("Failed to get status".to_string()));
    }

    let mut status = GitStatus::default();

    for line in String::from_utf8_lossy(&output.stdout).lines() {
        if line.len() < 3 {
            continue;
        }

        let index_status = line.chars().next().unwrap_or(' ');
        let work_status = line.chars().nth(1).unwrap_or(' ');
        let file_path = line[3..].to_string();

        // Staged changes
        if index_status != ' ' && index_status != '?' {
            status.staged.push(FileStatus {
                path: file_path.clone(),
                status: index_status,
            });
        }

        // Unstaged changes
        if work_status != ' ' && work_status != '?' {
            status.modified.push(FileStatus {
                path: file_path.clone(),
                status: work_status,
            });
        }

        // Untracked
        if index_status == '?' {
            status.untracked.push(file_path);
        }
    }

    Ok(status)
}

fn get_recent_commits(path: &Path, count: usize) -> Result<Vec<GitCommit>> {
    let output = Command::new("git")
        .current_dir(path)
        .args([
            "log",
            &format!("-{}", count),
            "--pretty=format:%h|%s|%an|%ar",
        ])
        .output()
        .map_err(|e| DevToolsError::GitError(e.to_string()))?;

    if !output.status.success() {
        return Ok(Vec::new());
    }

    let commits = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.split('|').collect();
            if parts.len() >= 4 {
                Some(GitCommit {
                    hash: parts[0].to_string(),
                    message: parts[1].to_string(),
                    author: parts[2].to_string(),
                    relative_time: parts[3].to_string(),
                })
            } else {
                None
            }
        })
        .collect();

    Ok(commits)
}

fn get_remote_url(path: &Path) -> Result<String> {
    let output = Command::new("git")
        .current_dir(path)
        .args(["remote", "get-url", "origin"])
        .output()
        .map_err(|e| DevToolsError::GitError(e.to_string()))?;

    if !output.status.success() {
        return Err(DevToolsError::GitError("No remote".to_string()));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_git_status_methods() {
        let status = GitStatus::default();
        assert!(status.is_clean());
        assert_eq!(status.changed_count(), 0);
    }

    #[test]
    fn test_file_status() {
        let fs = FileStatus {
            path: "test.txt".to_string(),
            status: 'M',
        };
        assert_eq!(fs.status, 'M');
    }
}
