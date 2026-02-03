//! Browser session persistence with cookies and storage.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

/// A browser cookie.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cookie {
    /// Cookie name.
    pub name: String,
    /// Cookie value.
    pub value: String,
    /// Cookie domain.
    pub domain: String,
    /// Cookie path.
    pub path: String,
    /// Expiration time.
    pub expires: Option<DateTime<Utc>>,
    /// HTTP only flag.
    pub http_only: bool,
    /// Secure flag.
    pub secure: bool,
    /// Same-site attribute.
    pub same_site: Option<String>,
}

impl Cookie {
    /// Create a new cookie.
    pub fn new(name: &str, value: &str, domain: &str) -> Self {
        Self {
            name: name.to_string(),
            value: value.to_string(),
            domain: domain.to_string(),
            path: "/".to_string(),
            expires: None,
            http_only: false,
            secure: false,
            same_site: None,
        }
    }

    /// Set path.
    pub fn with_path(mut self, path: &str) -> Self {
        self.path = path.to_string();
        self
    }

    /// Set expiration.
    pub fn with_expires(mut self, expires: DateTime<Utc>) -> Self {
        self.expires = Some(expires);
        self
    }

    /// Set HTTP only.
    pub fn http_only(mut self) -> Self {
        self.http_only = true;
        self
    }

    /// Set secure.
    pub fn secure(mut self) -> Self {
        self.secure = true;
        self
    }

    /// Check if cookie is expired.
    pub fn is_expired(&self) -> bool {
        if let Some(expires) = self.expires {
            Utc::now() > expires
        } else {
            false
        }
    }
}

/// Local storage entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageEntry {
    /// Storage key.
    pub key: String,
    /// Storage value.
    pub value: String,
    /// Origin this entry belongs to.
    pub origin: String,
}

/// Browser session data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionData {
    /// Session ID.
    pub id: String,
    /// Session name.
    pub name: String,
    /// Cookies by domain.
    pub cookies: HashMap<String, Vec<Cookie>>,
    /// Local storage by origin.
    pub local_storage: HashMap<String, HashMap<String, String>>,
    /// Session storage by origin.
    pub session_storage: HashMap<String, HashMap<String, String>>,
    /// Created timestamp.
    pub created_at: DateTime<Utc>,
    /// Last updated timestamp.
    pub updated_at: DateTime<Utc>,
    /// Additional metadata.
    pub metadata: HashMap<String, serde_json::Value>,
}

impl SessionData {
    /// Create a new session.
    pub fn new(name: &str) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.to_string(),
            cookies: HashMap::new(),
            local_storage: HashMap::new(),
            session_storage: HashMap::new(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            metadata: HashMap::new(),
        }
    }

    /// Add a cookie.
    pub fn add_cookie(&mut self, cookie: Cookie) {
        let domain = cookie.domain.clone();
        self.cookies
            .entry(domain)
            .or_insert_with(Vec::new)
            .push(cookie);
        self.updated_at = Utc::now();
    }

    /// Get cookies for a domain.
    pub fn get_cookies(&self, domain: &str) -> Vec<&Cookie> {
        self.cookies
            .get(domain)
            .map(|c| c.iter().filter(|c| !c.is_expired()).collect())
            .unwrap_or_default()
    }

    /// Set local storage value.
    pub fn set_local_storage(&mut self, origin: &str, key: &str, value: &str) {
        self.local_storage
            .entry(origin.to_string())
            .or_insert_with(HashMap::new)
            .insert(key.to_string(), value.to_string());
        self.updated_at = Utc::now();
    }

    /// Get local storage value.
    pub fn get_local_storage(&self, origin: &str, key: &str) -> Option<&str> {
        self.local_storage
            .get(origin)
            .and_then(|m| m.get(key))
            .map(|s| s.as_str())
    }

    /// Clean up expired cookies.
    pub fn cleanup_expired(&mut self) {
        for cookies in self.cookies.values_mut() {
            cookies.retain(|c| !c.is_expired());
        }
    }
}

/// Browser session manager.
pub struct SessionManager {
    /// Sessions by ID.
    sessions: Arc<RwLock<HashMap<String, SessionData>>>,
    /// Storage path for persistence.
    storage_path: Option<PathBuf>,
}

impl SessionManager {
    /// Create a new session manager.
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            storage_path: None,
        }
    }

    /// Create with persistent storage.
    pub fn with_storage(path: PathBuf) -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            storage_path: Some(path),
        }
    }

    /// Create a new session.
    pub async fn create_session(&self, name: &str) -> SessionData {
        let session = SessionData::new(name);
        let mut sessions = self.sessions.write().await;
        sessions.insert(session.id.clone(), session.clone());
        session
    }

    /// Get a session by ID.
    pub async fn get_session(&self, id: &str) -> Option<SessionData> {
        let sessions = self.sessions.read().await;
        sessions.get(id).cloned()
    }

    /// Get a session by name.
    pub async fn get_session_by_name(&self, name: &str) -> Option<SessionData> {
        let sessions = self.sessions.read().await;
        sessions.values().find(|s| s.name == name).cloned()
    }

    /// Update a session.
    pub async fn update_session(&self, session: SessionData) {
        let mut sessions = self.sessions.write().await;
        sessions.insert(session.id.clone(), session);
    }

    /// Delete a session.
    pub async fn delete_session(&self, id: &str) -> bool {
        let mut sessions = self.sessions.write().await;
        sessions.remove(id).is_some()
    }

    /// List all sessions.
    pub async fn list_sessions(&self) -> Vec<SessionData> {
        let sessions = self.sessions.read().await;
        sessions.values().cloned().collect()
    }

    /// Load sessions from storage.
    pub async fn load(&self) -> drbot_core::Result<usize> {
        let Some(path) = &self.storage_path else {
            return Ok(0);
        };

        if !path.exists() {
            return Ok(0);
        }

        let data = tokio::fs::read_to_string(path).await?;
        let loaded: HashMap<String, SessionData> = serde_json::from_str(&data)?;
        let count = loaded.len();

        let mut sessions = self.sessions.write().await;
        *sessions = loaded;

        Ok(count)
    }

    /// Save sessions to storage.
    pub async fn save(&self) -> drbot_core::Result<()> {
        let Some(path) = &self.storage_path else {
            return Ok(());
        };

        let sessions = self.sessions.read().await;
        let data = serde_json::to_string_pretty(&*sessions)?;

        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        tokio::fs::write(path, data).await?;

        Ok(())
    }

    /// Cleanup expired data in all sessions.
    pub async fn cleanup_expired(&self) {
        let mut sessions = self.sessions.write().await;
        for session in sessions.values_mut() {
            session.cleanup_expired();
        }
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Browser session with page reference.
pub struct BrowserSession {
    /// Session data.
    pub data: SessionData,
    /// User agent string.
    pub user_agent: Option<String>,
    /// Viewport dimensions.
    pub viewport: Option<(u32, u32)>,
}

impl BrowserSession {
    /// Create a new browser session.
    pub fn new(data: SessionData) -> Self {
        Self {
            data,
            user_agent: None,
            viewport: None,
        }
    }

    /// Set user agent.
    pub fn with_user_agent(mut self, user_agent: &str) -> Self {
        self.user_agent = Some(user_agent.to_string());
        self
    }

    /// Set viewport.
    pub fn with_viewport(mut self, width: u32, height: u32) -> Self {
        self.viewport = Some((width, height));
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cookie() {
        let cookie = Cookie::new("session", "abc123", "example.com")
            .with_path("/app")
            .secure();

        assert_eq!(cookie.name, "session");
        assert_eq!(cookie.domain, "example.com");
        assert!(cookie.secure);
        assert!(!cookie.is_expired());
    }

    #[test]
    fn test_session_data() {
        let mut session = SessionData::new("test_session");

        session.add_cookie(Cookie::new("token", "xyz", "api.example.com"));
        session.set_local_storage("https://example.com", "theme", "dark");

        assert!(!session.get_cookies("api.example.com").is_empty());
        assert_eq!(
            session.get_local_storage("https://example.com", "theme"),
            Some("dark")
        );
    }

    #[tokio::test]
    async fn test_session_manager() {
        let manager = SessionManager::new();

        let session = manager.create_session("test").await;
        assert_eq!(session.name, "test");

        let loaded = manager.get_session(&session.id).await;
        assert!(loaded.is_some());

        manager.delete_session(&session.id).await;
        let loaded = manager.get_session(&session.id).await;
        assert!(loaded.is_none());
    }
}
