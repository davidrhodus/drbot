//! Personal knowledge base for drbot.
//!
//! Stores and retrieves personal user knowledge including:
//! - User facts and preferences
//! - Notes and bookmarks
//! - Learned behaviors and corrections
//! - User-specific context

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::{KnowledgeError, Result};

/// Personal knowledge entry type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PersonalEntryType {
    /// A fact about the user.
    UserFact,
    /// A user preference.
    Preference,
    /// A saved note.
    Note,
    /// A bookmarked item.
    Bookmark,
    /// A contact entry.
    Contact,
    /// A learned correction.
    Correction,
    /// A custom command or shortcut.
    CustomCommand,
    /// A project or workspace.
    Project,
    /// A goal or objective.
    Goal,
    /// A routine or habit.
    Routine,
}

/// A personal knowledge entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonalEntry {
    /// Entry ID.
    pub id: Uuid,
    /// User ID this belongs to.
    pub user_id: String,
    /// Entry type.
    pub entry_type: PersonalEntryType,
    /// Key/name for lookup.
    pub key: String,
    /// Value/content.
    pub value: String,
    /// Structured data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    /// Embedding for semantic search.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Vec<f32>>,
    /// Confidence score (for learned entries).
    pub confidence: f32,
    /// Source of this knowledge.
    pub source: EntrySource,
    /// Times this was used/accessed.
    pub access_count: u64,
    /// Last accessed time.
    pub last_accessed: DateTime<Utc>,
    /// Created at.
    pub created_at: DateTime<Utc>,
    /// Updated at.
    pub updated_at: DateTime<Utc>,
    /// Tags for categorization.
    pub tags: Vec<String>,
    /// Is this entry active.
    pub active: bool,
}

impl PersonalEntry {
    /// Create a new personal entry.
    pub fn new(user_id: &str, entry_type: PersonalEntryType, key: &str, value: &str) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            user_id: user_id.to_string(),
            entry_type,
            key: key.to_string(),
            value: value.to_string(),
            data: None,
            embedding: None,
            confidence: 1.0,
            source: EntrySource::UserProvided,
            access_count: 0,
            last_accessed: now,
            created_at: now,
            updated_at: now,
            tags: Vec::new(),
            active: true,
        }
    }

    /// Create a user fact.
    pub fn fact(user_id: &str, key: &str, value: &str) -> Self {
        Self::new(user_id, PersonalEntryType::UserFact, key, value)
    }

    /// Create a preference.
    pub fn preference(user_id: &str, key: &str, value: &str) -> Self {
        Self::new(user_id, PersonalEntryType::Preference, key, value)
    }

    /// Create a note.
    pub fn note(user_id: &str, title: &str, content: &str) -> Self {
        Self::new(user_id, PersonalEntryType::Note, title, content)
    }

    /// Create a bookmark.
    pub fn bookmark(user_id: &str, title: &str, url: &str) -> Self {
        let mut entry = Self::new(user_id, PersonalEntryType::Bookmark, title, url);
        entry.data = Some(serde_json::json!({ "url": url }));
        entry
    }

    /// Set structured data.
    pub fn with_data(mut self, data: serde_json::Value) -> Self {
        self.data = Some(data);
        self
    }

    /// Set embedding.
    pub fn with_embedding(mut self, embedding: Vec<f32>) -> Self {
        self.embedding = Some(embedding);
        self
    }

    /// Set source.
    pub fn with_source(mut self, source: EntrySource) -> Self {
        self.source = source;
        self
    }

    /// Set confidence.
    pub fn with_confidence(mut self, confidence: f32) -> Self {
        self.confidence = confidence.clamp(0.0, 1.0);
        self
    }

    /// Add tags.
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    /// Record an access.
    pub fn record_access(&mut self) {
        self.access_count += 1;
        self.last_accessed = Utc::now();
    }

    /// Update the value.
    pub fn update(&mut self, value: &str) {
        self.value = value.to_string();
        self.updated_at = Utc::now();
    }
}

/// Source of a personal entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntrySource {
    /// Explicitly provided by user.
    UserProvided,
    /// Extracted from conversation.
    Extracted,
    /// Inferred from behavior.
    Inferred,
    /// Imported from external source.
    Imported,
    /// Learned from correction.
    Correction,
}

/// A contact in the personal knowledge base.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contact {
    /// Contact ID.
    pub id: Uuid,
    /// Name.
    pub name: String,
    /// Nickname/alias.
    pub nickname: Option<String>,
    /// Relationship (friend, colleague, family, etc.).
    pub relationship: Option<String>,
    /// Email addresses.
    pub emails: Vec<String>,
    /// Phone numbers.
    pub phones: Vec<String>,
    /// Notes about this contact.
    pub notes: Option<String>,
    /// Custom fields.
    pub custom: HashMap<String, String>,
    /// Last mentioned.
    pub last_mentioned: DateTime<Utc>,
    /// Mention count.
    pub mention_count: u64,
}

impl Contact {
    /// Create a new contact.
    pub fn new(name: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.to_string(),
            nickname: None,
            relationship: None,
            emails: Vec::new(),
            phones: Vec::new(),
            notes: None,
            custom: HashMap::new(),
            last_mentioned: Utc::now(),
            mention_count: 0,
        }
    }
}

/// A project or workspace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    /// Project ID.
    pub id: Uuid,
    /// Project name.
    pub name: String,
    /// Description.
    pub description: Option<String>,
    /// Associated paths/directories.
    pub paths: Vec<String>,
    /// Associated URLs.
    pub urls: Vec<String>,
    /// Project context/notes.
    pub context: Option<String>,
    /// Tech stack.
    pub tech_stack: Vec<String>,
    /// Active status.
    pub active: bool,
    /// Created at.
    pub created_at: DateTime<Utc>,
    /// Last accessed.
    pub last_accessed: DateTime<Utc>,
}

impl Project {
    /// Create a new project.
    pub fn new(name: &str) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            name: name.to_string(),
            description: None,
            paths: Vec::new(),
            urls: Vec::new(),
            context: None,
            tech_stack: Vec::new(),
            active: true,
            created_at: now,
            last_accessed: now,
        }
    }
}

/// Personal knowledge base configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonalKnowledgeConfig {
    /// Maximum entries per user.
    pub max_entries_per_user: usize,
    /// Auto-extract facts from conversations.
    pub auto_extract: bool,
    /// Minimum confidence for inferred entries.
    pub min_inference_confidence: f32,
    /// Enable semantic search.
    pub enable_semantic_search: bool,
}

impl Default for PersonalKnowledgeConfig {
    fn default() -> Self {
        Self {
            max_entries_per_user: 10000,
            auto_extract: true,
            min_inference_confidence: 0.7,
            enable_semantic_search: true,
        }
    }
}

/// Personal knowledge base manager.
pub struct PersonalKnowledgeBase {
    /// Entries by user ID.
    entries: Arc<RwLock<HashMap<String, Vec<PersonalEntry>>>>,
    /// Contacts by user ID.
    contacts: Arc<RwLock<HashMap<String, Vec<Contact>>>>,
    /// Projects by user ID.
    projects: Arc<RwLock<HashMap<String, Vec<Project>>>>,
    /// Configuration.
    config: PersonalKnowledgeConfig,
}

impl PersonalKnowledgeBase {
    /// Create a new personal knowledge base.
    pub fn new(config: PersonalKnowledgeConfig) -> Self {
        Self {
            entries: Arc::new(RwLock::new(HashMap::new())),
            contacts: Arc::new(RwLock::new(HashMap::new())),
            projects: Arc::new(RwLock::new(HashMap::new())),
            config,
        }
    }

    /// Add or update an entry.
    pub async fn set(&self, entry: PersonalEntry) -> Result<Uuid> {
        let mut entries = self.entries.write().await;
        let user_entries = entries.entry(entry.user_id.clone()).or_default();

        // Check if entry with same key exists
        if let Some(existing) = user_entries
            .iter_mut()
            .find(|e| e.key == entry.key && e.entry_type == entry.entry_type)
        {
            existing.value = entry.value;
            existing.data = entry.data;
            existing.updated_at = Utc::now();
            return Ok(existing.id);
        }

        // Check limit
        if user_entries.len() >= self.config.max_entries_per_user {
            return Err(KnowledgeError::Storage("Entry limit reached".to_string()));
        }

        let id = entry.id;
        user_entries.push(entry);
        Ok(id)
    }

    /// Get an entry by key.
    pub async fn get(
        &self,
        user_id: &str,
        entry_type: PersonalEntryType,
        key: &str,
    ) -> Option<PersonalEntry> {
        let entries = self.entries.read().await;
        entries.get(user_id).and_then(|user_entries| {
            user_entries
                .iter()
                .find(|e| e.entry_type == entry_type && e.key == key && e.active)
                .cloned()
        })
    }

    /// Get all entries of a type for a user.
    pub async fn get_by_type(
        &self,
        user_id: &str,
        entry_type: PersonalEntryType,
    ) -> Vec<PersonalEntry> {
        let entries = self.entries.read().await;
        entries
            .get(user_id)
            .map(|user_entries| {
                user_entries
                    .iter()
                    .filter(|e| e.entry_type == entry_type && e.active)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Search entries semantically.
    pub async fn search(
        &self,
        user_id: &str,
        query_embedding: &[f32],
        options: PersonalSearchOptions,
    ) -> Vec<PersonalSearchResult> {
        let entries = self.entries.read().await;
        let mut results = Vec::new();

        if let Some(user_entries) = entries.get(user_id) {
            for entry in user_entries.iter().filter(|e| e.active) {
                // Filter by type if specified
                if let Some(ref types) = options.entry_types {
                    if !types.contains(&entry.entry_type) {
                        continue;
                    }
                }

                // Filter by tags if specified
                if let Some(ref tags) = options.tags {
                    if !tags.iter().any(|t| entry.tags.contains(t)) {
                        continue;
                    }
                }

                // Calculate similarity if embedding available
                let similarity = entry.embedding.as_ref().map(|emb| {
                    let dot: f32 = query_embedding
                        .iter()
                        .zip(emb.iter())
                        .map(|(a, b)| a * b)
                        .sum();
                    let mag_q: f32 = query_embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
                    let mag_e: f32 = emb.iter().map(|x| x * x).sum::<f32>().sqrt();
                    if mag_q > 0.0 && mag_e > 0.0 {
                        dot / (mag_q * mag_e)
                    } else {
                        0.0
                    }
                });

                if let Some(sim) = similarity {
                    if sim >= options.min_similarity {
                        results.push(PersonalSearchResult {
                            entry: entry.clone(),
                            similarity: sim,
                        });
                    }
                }
            }
        }

        // Sort by similarity
        results.sort_by(|a, b| {
            b.similarity
                .partial_cmp(&a.similarity)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Apply limit
        if let Some(limit) = options.limit {
            results.truncate(limit);
        }

        results
    }

    /// Delete an entry.
    pub async fn delete(&self, user_id: &str, id: Uuid) -> bool {
        let mut entries = self.entries.write().await;
        if let Some(user_entries) = entries.get_mut(user_id) {
            if let Some(entry) = user_entries.iter_mut().find(|e| e.id == id) {
                entry.active = false;
                return true;
            }
        }
        false
    }

    /// Remember a fact about a user.
    pub async fn remember_fact(&self, user_id: &str, key: &str, value: &str) -> Result<Uuid> {
        let entry = PersonalEntry::fact(user_id, key, value);
        self.set(entry).await
    }

    /// Get all facts about a user.
    pub async fn get_facts(&self, user_id: &str) -> Vec<PersonalEntry> {
        self.get_by_type(user_id, PersonalEntryType::UserFact).await
    }

    /// Set a user preference.
    pub async fn set_preference(&self, user_id: &str, key: &str, value: &str) -> Result<Uuid> {
        let entry = PersonalEntry::preference(user_id, key, value);
        self.set(entry).await
    }

    /// Get a user preference.
    pub async fn get_preference(&self, user_id: &str, key: &str) -> Option<String> {
        self.get(user_id, PersonalEntryType::Preference, key)
            .await
            .map(|e| e.value)
    }

    /// Save a note.
    pub async fn save_note(&self, user_id: &str, title: &str, content: &str) -> Result<Uuid> {
        let entry = PersonalEntry::note(user_id, title, content);
        self.set(entry).await
    }

    /// Get notes.
    pub async fn get_notes(&self, user_id: &str) -> Vec<PersonalEntry> {
        self.get_by_type(user_id, PersonalEntryType::Note).await
    }

    /// Save a bookmark.
    pub async fn save_bookmark(&self, user_id: &str, title: &str, url: &str) -> Result<Uuid> {
        let entry = PersonalEntry::bookmark(user_id, title, url);
        self.set(entry).await
    }

    /// Get bookmarks.
    pub async fn get_bookmarks(&self, user_id: &str) -> Vec<PersonalEntry> {
        self.get_by_type(user_id, PersonalEntryType::Bookmark).await
    }

    /// Add a contact.
    pub async fn add_contact(&self, user_id: &str, contact: Contact) -> Uuid {
        let id = contact.id;
        let mut contacts = self.contacts.write().await;
        contacts
            .entry(user_id.to_string())
            .or_default()
            .push(contact);
        id
    }

    /// Get contacts.
    pub async fn get_contacts(&self, user_id: &str) -> Vec<Contact> {
        let contacts = self.contacts.read().await;
        contacts.get(user_id).cloned().unwrap_or_default()
    }

    /// Find contact by name.
    pub async fn find_contact(&self, user_id: &str, name: &str) -> Option<Contact> {
        let contacts = self.contacts.read().await;
        contacts.get(user_id).and_then(|list| {
            list.iter()
                .find(|c| {
                    c.name.to_lowercase().contains(&name.to_lowercase())
                        || c.nickname
                            .as_ref()
                            .map(|n| n.to_lowercase().contains(&name.to_lowercase()))
                            .unwrap_or(false)
                })
                .cloned()
        })
    }

    /// Add a project.
    pub async fn add_project(&self, user_id: &str, project: Project) -> Uuid {
        let id = project.id;
        let mut projects = self.projects.write().await;
        projects
            .entry(user_id.to_string())
            .or_default()
            .push(project);
        id
    }

    /// Get projects.
    pub async fn get_projects(&self, user_id: &str) -> Vec<Project> {
        let projects = self.projects.read().await;
        projects.get(user_id).cloned().unwrap_or_default()
    }

    /// Get active project.
    pub async fn get_active_project(&self, user_id: &str) -> Option<Project> {
        let projects = self.projects.read().await;
        projects.get(user_id).and_then(|list| {
            list.iter()
                .filter(|p| p.active)
                .max_by_key(|p| p.last_accessed)
                .cloned()
        })
    }

    /// Get user context (aggregated personal knowledge).
    pub async fn get_user_context(&self, user_id: &str) -> UserContext {
        let facts = self.get_facts(user_id).await;
        let preferences = self
            .get_by_type(user_id, PersonalEntryType::Preference)
            .await;
        let active_project = self.get_active_project(user_id).await;

        UserContext {
            user_id: user_id.to_string(),
            facts: facts.into_iter().map(|e| (e.key, e.value)).collect(),
            preferences: preferences.into_iter().map(|e| (e.key, e.value)).collect(),
            active_project,
        }
    }

    /// Get statistics for a user.
    pub async fn stats(&self, user_id: &str) -> PersonalKnowledgeStats {
        let entries = self.entries.read().await;
        let contacts = self.contacts.read().await;
        let projects = self.projects.read().await;

        let user_entries = entries.get(user_id);
        let entry_count = user_entries
            .map(|e| e.iter().filter(|x| x.active).count())
            .unwrap_or(0);

        let mut by_type = HashMap::new();
        if let Some(entries) = user_entries {
            for entry in entries.iter().filter(|e| e.active) {
                *by_type.entry(entry.entry_type).or_insert(0) += 1;
            }
        }

        PersonalKnowledgeStats {
            entry_count,
            contact_count: contacts.get(user_id).map(|c| c.len()).unwrap_or(0),
            project_count: projects.get(user_id).map(|p| p.len()).unwrap_or(0),
            by_type,
        }
    }
}

/// Options for searching personal entries.
#[derive(Debug, Clone, Default)]
pub struct PersonalSearchOptions {
    /// Minimum similarity score.
    pub min_similarity: f32,
    /// Maximum results.
    pub limit: Option<usize>,
    /// Filter by entry types.
    pub entry_types: Option<Vec<PersonalEntryType>>,
    /// Filter by tags.
    pub tags: Option<Vec<String>>,
}

impl PersonalSearchOptions {
    /// Create new search options.
    pub fn new() -> Self {
        Self {
            min_similarity: 0.5,
            limit: Some(10),
            entry_types: None,
            tags: None,
        }
    }

    /// Set minimum similarity.
    pub fn min_similarity(mut self, min: f32) -> Self {
        self.min_similarity = min;
        self
    }

    /// Set limit.
    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Filter by entry types.
    pub fn types(mut self, types: Vec<PersonalEntryType>) -> Self {
        self.entry_types = Some(types);
        self
    }
}

/// Result from personal knowledge search.
#[derive(Debug, Clone)]
pub struct PersonalSearchResult {
    /// The entry.
    pub entry: PersonalEntry,
    /// Similarity score.
    pub similarity: f32,
}

/// Aggregated user context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserContext {
    /// User ID.
    pub user_id: String,
    /// User facts (key-value).
    pub facts: HashMap<String, String>,
    /// User preferences (key-value).
    pub preferences: HashMap<String, String>,
    /// Currently active project.
    pub active_project: Option<Project>,
}

/// Personal knowledge statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonalKnowledgeStats {
    /// Total entry count.
    pub entry_count: usize,
    /// Contact count.
    pub contact_count: usize,
    /// Project count.
    pub project_count: usize,
    /// Entries by type.
    pub by_type: HashMap<PersonalEntryType, usize>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_personal_knowledge_base() {
        let pkb = PersonalKnowledgeBase::new(PersonalKnowledgeConfig::default());

        // Remember facts
        pkb.remember_fact("user1", "name", "Alice").await.unwrap();
        pkb.remember_fact("user1", "occupation", "Engineer")
            .await
            .unwrap();

        let facts = pkb.get_facts("user1").await;
        assert_eq!(facts.len(), 2);
    }

    #[tokio::test]
    async fn test_preferences() {
        let pkb = PersonalKnowledgeBase::new(PersonalKnowledgeConfig::default());

        pkb.set_preference("user1", "theme", "dark").await.unwrap();
        pkb.set_preference("user1", "language", "en").await.unwrap();

        let theme = pkb.get_preference("user1", "theme").await;
        assert_eq!(theme, Some("dark".to_string()));
    }

    #[tokio::test]
    async fn test_notes_and_bookmarks() {
        let pkb = PersonalKnowledgeBase::new(PersonalKnowledgeConfig::default());

        pkb.save_note("user1", "Meeting Notes", "Discussed project timeline")
            .await
            .unwrap();
        pkb.save_bookmark("user1", "Rust Docs", "https://doc.rust-lang.org")
            .await
            .unwrap();

        let notes = pkb.get_notes("user1").await;
        assert_eq!(notes.len(), 1);

        let bookmarks = pkb.get_bookmarks("user1").await;
        assert_eq!(bookmarks.len(), 1);
    }

    #[tokio::test]
    async fn test_contacts() {
        let pkb = PersonalKnowledgeBase::new(PersonalKnowledgeConfig::default());

        let contact = Contact::new("Bob Smith");
        pkb.add_contact("user1", contact).await;

        let found = pkb.find_contact("user1", "bob").await;
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "Bob Smith");
    }

    #[tokio::test]
    async fn test_projects() {
        let pkb = PersonalKnowledgeBase::new(PersonalKnowledgeConfig::default());

        let mut project = Project::new("drbot");
        project.tech_stack = vec!["Rust".to_string(), "TypeScript".to_string()];
        pkb.add_project("user1", project).await;

        let active = pkb.get_active_project("user1").await;
        assert!(active.is_some());
        assert_eq!(active.unwrap().name, "drbot");
    }

    #[tokio::test]
    async fn test_user_context() {
        let pkb = PersonalKnowledgeBase::new(PersonalKnowledgeConfig::default());

        pkb.remember_fact("user1", "timezone", "PST").await.unwrap();
        pkb.set_preference("user1", "verbosity", "concise")
            .await
            .unwrap();

        let context = pkb.get_user_context("user1").await;
        assert_eq!(context.facts.get("timezone"), Some(&"PST".to_string()));
        assert_eq!(
            context.preferences.get("verbosity"),
            Some(&"concise".to_string())
        );
    }
}
