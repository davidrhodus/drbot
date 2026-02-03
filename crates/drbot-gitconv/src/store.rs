//! Git-based conversation store.

use crate::conversation::Conversation;
use crate::message::{Message, MessageRole};
use crate::{GitConvError, Result};
use std::path::{Path, PathBuf};
use tokio::fs;
use tracing::{debug, info};

/// Store configuration.
#[derive(Debug, Clone)]
pub struct StoreConfig {
    /// Auto-commit after each message.
    pub auto_commit: bool,
    /// Commit message prefix.
    pub commit_prefix: String,
    /// Export format (json or markdown).
    pub export_format: ExportFormat,
}

/// Export format.
#[derive(Debug, Clone, Copy, Default)]
pub enum ExportFormat {
    #[default]
    Json,
    Markdown,
    Both,
}

impl Default for StoreConfig {
    fn default() -> Self {
        Self {
            auto_commit: false,
            commit_prefix: "conv:".to_string(),
            export_format: ExportFormat::Json,
        }
    }
}

/// Git-based conversation store.
pub struct GitConversationStore {
    root: PathBuf,
    config: StoreConfig,
}

impl GitConversationStore {
    /// Open or create a conversation store.
    pub async fn open(path: impl AsRef<Path>) -> Result<Self> {
        let root = path.as_ref().to_path_buf();

        // Create directory if it doesn't exist
        if !root.exists() {
            fs::create_dir_all(&root).await?;
            info!("Created conversation store at {:?}", root);
        }

        // Initialize git repo if needed
        let git_dir = root.join(".git");
        if !git_dir.exists() {
            Self::init_git_repo(&root).await?;
        }

        Ok(Self {
            root,
            config: StoreConfig::default(),
        })
    }

    /// Open with custom config.
    pub async fn open_with_config(path: impl AsRef<Path>, config: StoreConfig) -> Result<Self> {
        let mut store = Self::open(path).await?;
        store.config = config;
        Ok(store)
    }

    /// Initialize git repository.
    async fn init_git_repo(path: &Path) -> Result<()> {
        let output = tokio::process::Command::new("git")
            .args(["init"])
            .current_dir(path)
            .output()
            .await?;

        if !output.status.success() {
            return Err(GitConvError::GitError("Failed to init git repo".into()));
        }

        // Create .gitignore
        let gitignore = path.join(".gitignore");
        fs::write(&gitignore, "*.tmp\n*.swp\n.DS_Store\n").await?;

        // Initial commit
        let _ = tokio::process::Command::new("git")
            .args(["add", "."])
            .current_dir(path)
            .output()
            .await;

        let _ = tokio::process::Command::new("git")
            .args(["commit", "-m", "Initial commit"])
            .current_dir(path)
            .output()
            .await;

        info!("Initialized git repository");
        Ok(())
    }

    /// Create a new conversation.
    pub async fn create(&self, title: impl Into<String>) -> Result<Conversation> {
        let conv = Conversation::new(title);
        self.save(&conv).await?;

        info!("Created conversation: {}", conv.id);
        Ok(conv)
    }

    /// Save a conversation.
    pub async fn save(&self, conv: &Conversation) -> Result<()> {
        let conv_dir = self.root.join(&conv.id);
        fs::create_dir_all(&conv_dir).await?;

        // Save as JSON
        let json_path = conv_dir.join("conversation.json");
        let json = serde_json::to_string_pretty(conv)
            .map_err(|e| GitConvError::SerializeError(e.to_string()))?;
        fs::write(&json_path, &json).await?;

        // Also save as markdown if configured
        if matches!(
            self.config.export_format,
            ExportFormat::Markdown | ExportFormat::Both
        ) {
            let md_path = conv_dir.join("conversation.md");
            fs::write(&md_path, conv.to_markdown()).await?;
        }

        debug!("Saved conversation: {}", conv.id);
        Ok(())
    }

    /// Load a conversation by ID.
    pub async fn load(&self, id: &str) -> Result<Conversation> {
        let json_path = self.root.join(id).join("conversation.json");

        if !json_path.exists() {
            return Err(GitConvError::NotFound(id.to_string()));
        }

        let json = fs::read_to_string(&json_path).await?;
        let conv: Conversation =
            serde_json::from_str(&json).map_err(|e| GitConvError::SerializeError(e.to_string()))?;

        Ok(conv)
    }

    /// List all conversations.
    pub async fn list(&self) -> Result<Vec<Conversation>> {
        let mut conversations = Vec::new();

        let mut entries = fs::read_dir(&self.root).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.is_dir()
                && !path
                    .file_name()
                    .map(|n| n.to_string_lossy().starts_with('.'))
                    .unwrap_or(true)
            {
                if let Ok(conv) = self
                    .load(&path.file_name().unwrap().to_string_lossy())
                    .await
                {
                    conversations.push(conv);
                }
            }
        }

        // Sort by updated_at descending
        conversations.sort_by(|a, b| b.metadata.updated_at.cmp(&a.metadata.updated_at));

        Ok(conversations)
    }

    /// Delete a conversation.
    pub async fn delete(&self, id: &str) -> Result<()> {
        let conv_dir = self.root.join(id);

        if !conv_dir.exists() {
            return Err(GitConvError::NotFound(id.to_string()));
        }

        fs::remove_dir_all(&conv_dir).await?;

        // Commit the deletion
        self.git_commit(&format!("Deleted conversation {}", id))
            .await?;

        info!("Deleted conversation: {}", id);
        Ok(())
    }

    /// Add a message to a conversation.
    pub async fn add_message(&self, conv_id: &str, role: &str, content: &str) -> Result<()> {
        let mut conv = self.load(conv_id).await?;

        let role = MessageRole::from_str(role)
            .ok_or_else(|| GitConvError::SerializeError(format!("Invalid role: {}", role)))?;

        conv.add_message(Message::new(role, content));
        self.save(&conv).await?;

        if self.config.auto_commit {
            self.commit(conv_id, "Added message").await?;
        }

        Ok(())
    }

    /// Commit changes for a conversation.
    pub async fn commit(&self, conv_id: &str, message: &str) -> Result<()> {
        let conv_dir = self.root.join(conv_id);

        // Git add
        let _ = tokio::process::Command::new("git")
            .args(["add", "."])
            .current_dir(&conv_dir)
            .output()
            .await;

        // Git commit
        let commit_msg = format!("{} {}", self.config.commit_prefix, message);
        self.git_commit(&commit_msg).await
    }

    /// Run a git commit.
    async fn git_commit(&self, message: &str) -> Result<()> {
        let _ = tokio::process::Command::new("git")
            .args(["add", "-A"])
            .current_dir(&self.root)
            .output()
            .await;

        let output = tokio::process::Command::new("git")
            .args(["commit", "-m", message])
            .current_dir(&self.root)
            .output()
            .await?;

        if !output.status.success() {
            // It's ok if there's nothing to commit
            let stderr = String::from_utf8_lossy(&output.stderr);
            if !stderr.contains("nothing to commit") {
                debug!("Git commit note: {}", stderr);
            }
        }

        Ok(())
    }

    /// Get history for a conversation.
    pub async fn history(&self, conv_id: &str) -> Result<Vec<String>> {
        let conv_dir = self.root.join(conv_id);

        let output = tokio::process::Command::new("git")
            .args(["log", "--oneline", "--", "."])
            .current_dir(&conv_dir)
            .output()
            .await?;

        let history = String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(|s| s.to_string())
            .collect();

        Ok(history)
    }

    /// Search conversations.
    pub async fn search(&self, query: &str) -> Result<Vec<Conversation>> {
        let all = self.list().await?;
        let query_lower = query.to_lowercase();

        let matches: Vec<_> = all
            .into_iter()
            .filter(|c| {
                c.metadata.title.to_lowercase().contains(&query_lower)
                    || c.messages
                        .iter()
                        .any(|m| m.content.to_lowercase().contains(&query_lower))
            })
            .collect();

        Ok(matches)
    }

    /// Get the store root path.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Get the config.
    pub fn config(&self) -> &StoreConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_store_operations() {
        let temp_dir = std::env::temp_dir().join("drbot-gitconv-test-ops");
        let _ = std::fs::remove_dir_all(&temp_dir);

        let store = GitConversationStore::open(&temp_dir).await.unwrap();

        // Create conversation
        let conv = store.create("Test Chat").await.unwrap();
        assert_eq!(conv.metadata.title, "Test Chat");

        // Add messages
        store.add_message(&conv.id, "user", "Hello").await.unwrap();
        store
            .add_message(&conv.id, "assistant", "Hi!")
            .await
            .unwrap();

        // Load and verify
        let loaded = store.load(&conv.id).await.unwrap();
        assert_eq!(loaded.message_count(), 2);

        // List
        let all = store.list().await.unwrap();
        assert_eq!(all.len(), 1);

        // Search
        let found = store.search("Hello").await.unwrap();
        assert_eq!(found.len(), 1);

        // Cleanup
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
