//! Allowlist management for sender verification.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// An entry in the allowlist.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllowlistEntry {
    /// Sender identifier.
    pub sender_id: String,
    /// Channel this entry applies to (None = all channels).
    pub channel: Option<String>,
    /// Display name or alias.
    pub display_name: Option<String>,
    /// When this entry was added.
    pub added_at: DateTime<Utc>,
    /// Who added this entry.
    pub added_by: Option<String>,
    /// When this entry expires (None = never).
    pub expires_at: Option<DateTime<Utc>>,
    /// Whether this entry is currently active.
    pub active: bool,
    /// Additional notes.
    pub notes: Option<String>,
}

impl AllowlistEntry {
    /// Create a new allowlist entry.
    pub fn new(sender_id: &str) -> Self {
        Self {
            sender_id: sender_id.to_string(),
            channel: None,
            display_name: None,
            added_at: Utc::now(),
            added_by: None,
            expires_at: None,
            active: true,
            notes: None,
        }
    }

    /// Set the channel.
    pub fn with_channel(mut self, channel: &str) -> Self {
        self.channel = Some(channel.to_string());
        self
    }

    /// Set the display name.
    pub fn with_display_name(mut self, name: &str) -> Self {
        self.display_name = Some(name.to_string());
        self
    }

    /// Set who added this entry.
    pub fn with_added_by(mut self, added_by: &str) -> Self {
        self.added_by = Some(added_by.to_string());
        self
    }

    /// Set an expiry time.
    pub fn with_expiry(mut self, expires_at: DateTime<Utc>) -> Self {
        self.expires_at = Some(expires_at);
        self
    }

    /// Set notes.
    pub fn with_notes(mut self, notes: &str) -> Self {
        self.notes = Some(notes.to_string());
        self
    }

    /// Check if this entry is currently valid.
    pub fn is_valid(&self) -> bool {
        if !self.active {
            return false;
        }
        if let Some(expires) = self.expires_at {
            if Utc::now() > expires {
                return false;
            }
        }
        true
    }

    /// Check if this entry matches a sender on a channel.
    pub fn matches(&self, sender_id: &str, channel: Option<&str>) -> bool {
        if self.sender_id != sender_id {
            return false;
        }
        if !self.is_valid() {
            return false;
        }
        match (&self.channel, channel) {
            (None, _) => true, // Entry applies to all channels
            (Some(entry_ch), Some(ch)) => entry_ch == ch,
            (Some(_), None) => false, // Entry is channel-specific but no channel provided
        }
    }
}

/// The allowlist containing all entries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Allowlist {
    /// All entries in the list.
    pub entries: Vec<AllowlistEntry>,
}

impl Default for Allowlist {
    fn default() -> Self {
        Self::new()
    }
}

impl Allowlist {
    /// Create a new empty allowlist.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Add an entry to the allowlist.
    pub fn add(&mut self, entry: AllowlistEntry) {
        // Remove any existing entry for the same sender/channel combination
        self.entries
            .retain(|e| !(e.sender_id == entry.sender_id && e.channel == entry.channel));
        self.entries.push(entry);
    }

    /// Remove an entry by sender ID.
    pub fn remove(&mut self, sender_id: &str, channel: Option<&str>) -> bool {
        let initial_len = self.entries.len();
        self.entries.retain(|e| {
            if e.sender_id != sender_id {
                return true;
            }
            match (channel, &e.channel) {
                (None, _) => false, // Remove all entries for this sender
                (Some(ch), Some(entry_ch)) => ch != entry_ch,
                (Some(_), None) => true, // Don't remove global entries when specific channel given
            }
        });
        self.entries.len() < initial_len
    }

    /// Check if a sender is allowed.
    pub fn is_allowed(&self, sender_id: &str, channel: Option<&str>) -> bool {
        self.entries.iter().any(|e| e.matches(sender_id, channel))
    }

    /// Get all entries for a sender.
    pub fn get_entries(&self, sender_id: &str) -> Vec<&AllowlistEntry> {
        self.entries
            .iter()
            .filter(|e| e.sender_id == sender_id)
            .collect()
    }

    /// Get all active entries.
    pub fn active_entries(&self) -> Vec<&AllowlistEntry> {
        self.entries.iter().filter(|e| e.is_valid()).collect()
    }

    /// Clean up expired entries.
    pub fn cleanup_expired(&mut self) -> usize {
        let initial_len = self.entries.len();
        self.entries.retain(|e| e.is_valid());
        initial_len - self.entries.len()
    }
}

/// Thread-safe allowlist manager.
pub struct AllowlistManager {
    allowlist: Arc<RwLock<Allowlist>>,
    /// Cache for quick lookups (sender_id -> allowed).
    cache: Arc<RwLock<HashMap<String, bool>>>,
}

impl AllowlistManager {
    /// Create a new allowlist manager.
    pub fn new() -> Self {
        Self {
            allowlist: Arc::new(RwLock::new(Allowlist::new())),
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create from an existing allowlist.
    pub fn from_allowlist(allowlist: Allowlist) -> Self {
        Self {
            allowlist: Arc::new(RwLock::new(allowlist)),
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Check if a sender is allowed.
    pub async fn is_allowed(&self, sender_id: &str, channel: Option<&str>) -> bool {
        // Check cache first for simple lookups
        if channel.is_none() {
            let cache = self.cache.read().await;
            if let Some(&allowed) = cache.get(sender_id) {
                return allowed;
            }
        }

        let allowlist = self.allowlist.read().await;
        let allowed = allowlist.is_allowed(sender_id, channel);

        // Update cache for global checks
        if channel.is_none() {
            drop(allowlist);
            let mut cache = self.cache.write().await;
            cache.insert(sender_id.to_string(), allowed);
        }

        allowed
    }

    /// Add a sender to the allowlist.
    pub async fn add(&self, entry: AllowlistEntry) {
        let sender_id = entry.sender_id.clone();
        let mut allowlist = self.allowlist.write().await;
        allowlist.add(entry);

        // Invalidate cache
        drop(allowlist);
        let mut cache = self.cache.write().await;
        cache.remove(&sender_id);
    }

    /// Remove a sender from the allowlist.
    pub async fn remove(&self, sender_id: &str, channel: Option<&str>) -> bool {
        let mut allowlist = self.allowlist.write().await;
        let removed = allowlist.remove(sender_id, channel);

        if removed {
            drop(allowlist);
            let mut cache = self.cache.write().await;
            cache.remove(sender_id);
        }

        removed
    }

    /// List all entries.
    pub async fn list(&self) -> Vec<AllowlistEntry> {
        let allowlist = self.allowlist.read().await;
        allowlist.entries.clone()
    }

    /// List active entries.
    pub async fn list_active(&self) -> Vec<AllowlistEntry> {
        let allowlist = self.allowlist.read().await;
        allowlist.active_entries().into_iter().cloned().collect()
    }

    /// Clean up expired entries.
    pub async fn cleanup(&self) -> usize {
        let mut allowlist = self.allowlist.write().await;
        let removed = allowlist.cleanup_expired();

        if removed > 0 {
            drop(allowlist);
            let mut cache = self.cache.write().await;
            cache.clear();
        }

        removed
    }

    /// Clear the cache.
    pub async fn clear_cache(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
    }
}

impl Default for AllowlistManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allowlist_entry() {
        let entry = AllowlistEntry::new("user1")
            .with_channel("telegram")
            .with_display_name("Test User");

        assert!(entry.is_valid());
        assert!(entry.matches("user1", Some("telegram")));
        assert!(!entry.matches("user1", Some("discord")));
        assert!(!entry.matches("user2", Some("telegram")));
    }

    #[test]
    fn test_allowlist() {
        let mut allowlist = Allowlist::new();

        allowlist.add(AllowlistEntry::new("user1"));
        allowlist.add(AllowlistEntry::new("user2").with_channel("telegram"));

        assert!(allowlist.is_allowed("user1", Some("telegram")));
        assert!(allowlist.is_allowed("user1", Some("discord")));
        assert!(allowlist.is_allowed("user2", Some("telegram")));
        assert!(!allowlist.is_allowed("user2", Some("discord")));
        assert!(!allowlist.is_allowed("user3", None));
    }

    #[tokio::test]
    async fn test_allowlist_manager() {
        let manager = AllowlistManager::new();

        manager.add(AllowlistEntry::new("user1")).await;

        assert!(manager.is_allowed("user1", None).await);
        assert!(!manager.is_allowed("user2", None).await);

        manager.remove("user1", None).await;
        assert!(!manager.is_allowed("user1", None).await);
    }
}
