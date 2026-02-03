//! Session-to-container mapping.

use crate::{ContainerPool, ExecuteRequest, Result, SandboxContainer, SandboxError};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Session sandbox state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSandbox {
    /// Session ID.
    pub session_id: String,
    /// Container ID.
    pub container_id: String,
    /// Image used.
    pub image: String,
    /// When the session started.
    pub started_at: DateTime<Utc>,
    /// When the session was last active.
    pub last_active: DateTime<Utc>,
    /// Number of executions in this session.
    pub execution_count: u64,
    /// Whether the session is currently executing.
    pub executing: bool,
}

impl SessionSandbox {
    /// Create a new session sandbox.
    pub fn new(session_id: &str, container_id: &str, image: &str) -> Self {
        let now = Utc::now();
        Self {
            session_id: session_id.to_string(),
            container_id: container_id.to_string(),
            image: image.to_string(),
            started_at: now,
            last_active: now,
            execution_count: 0,
            executing: false,
        }
    }

    /// Update last active time.
    pub fn touch(&mut self) {
        self.last_active = Utc::now();
    }

    /// Increment execution count.
    pub fn increment_executions(&mut self) {
        self.execution_count += 1;
        self.touch();
    }

    /// Get session age in seconds.
    pub fn age_secs(&self) -> i64 {
        (Utc::now() - self.started_at).num_seconds()
    }

    /// Get idle time in seconds.
    pub fn idle_secs(&self) -> i64 {
        (Utc::now() - self.last_active).num_seconds()
    }
}

/// Manages session-to-container mappings.
pub struct SessionManager {
    /// Container pool.
    pool: Arc<ContainerPool>,
    /// Active sessions.
    sessions: Arc<RwLock<HashMap<String, SessionSandbox>>>,
    /// Container to session mapping.
    container_to_session: Arc<RwLock<HashMap<String, String>>>,
    /// Active containers (owned by sessions).
    containers: Arc<RwLock<HashMap<String, Arc<SandboxContainer>>>>,
    /// Maximum idle time before session timeout (seconds).
    session_timeout_secs: u64,
}

impl SessionManager {
    /// Create a new session manager.
    pub fn new(pool: Arc<ContainerPool>, session_timeout_secs: u64) -> Self {
        Self {
            pool,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            container_to_session: Arc::new(RwLock::new(HashMap::new())),
            containers: Arc::new(RwLock::new(HashMap::new())),
            session_timeout_secs,
        }
    }

    /// Get or create a sandbox for a session.
    pub async fn get_or_create(
        &self,
        session_id: &str,
        image: Option<&str>,
    ) -> Result<Arc<SandboxContainer>> {
        // Check for existing session
        let existing_container_id = {
            let sessions = self.sessions.read().await;
            if let Some(session) = sessions.get(session_id) {
                let containers = self.containers.read().await;
                if let Some(container) = containers.get(&session.container_id) {
                    if container.is_healthy().await {
                        Some(session.container_id.clone())
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            }
        };

        if let Some(container_id) = existing_container_id {
            // Update last active
            {
                let mut sessions = self.sessions.write().await;
                if let Some(session) = sessions.get_mut(session_id) {
                    session.touch();
                }
            }

            let containers = self.containers.read().await;
            if let Some(container) = containers.get(&container_id) {
                return Ok(container.clone());
            }
        }

        // Create new container
        let container = self.pool.acquire(image).await?;
        let container_id = container.id().to_string();
        let image_name = image.unwrap_or("default").to_string();

        // Store container
        {
            let mut containers = self.containers.write().await;
            containers.insert(container_id.clone(), container.clone());
        }

        // Create session record
        let session = SessionSandbox::new(session_id, &container_id, &image_name);

        {
            let mut sessions = self.sessions.write().await;
            let mut c2s = self.container_to_session.write().await;

            sessions.insert(session_id.to_string(), session);
            c2s.insert(container_id.clone(), session_id.to_string());
        }

        tracing::debug!(
            session_id = %session_id,
            container_id = %container_id,
            "Created sandbox for session"
        );

        Ok(container)
    }

    /// Get existing sandbox for a session.
    pub async fn get(&self, session_id: &str) -> Option<Arc<SandboxContainer>> {
        let sessions = self.sessions.read().await;
        let session = sessions.get(session_id)?;

        let containers = self.containers.read().await;
        containers.get(&session.container_id).cloned()
    }

    /// End a session and release its container.
    pub async fn end_session(&self, session_id: &str) -> Result<()> {
        let session = {
            let mut sessions = self.sessions.write().await;
            sessions.remove(session_id)
        };

        if let Some(session) = session {
            // Remove container mapping
            {
                let mut c2s = self.container_to_session.write().await;
                c2s.remove(&session.container_id);
            }

            // Remove and release container
            let container = {
                let mut containers = self.containers.write().await;
                containers.remove(&session.container_id)
            };

            if let Some(container) = container {
                // Return to pool
                self.pool.release(container.id()).await;
            }

            tracing::debug!(
                session_id = %session_id,
                "Ended sandbox session"
            );
        }

        Ok(())
    }

    /// Get session info.
    pub async fn get_session_info(&self, session_id: &str) -> Option<SessionSandbox> {
        let sessions = self.sessions.read().await;
        sessions.get(session_id).cloned()
    }

    /// List all active sessions.
    pub async fn list_sessions(&self) -> Vec<SessionSandbox> {
        let sessions = self.sessions.read().await;
        sessions.values().cloned().collect()
    }

    /// Reset a session's container state.
    pub async fn reset_session(&self, session_id: &str) -> Result<()> {
        let container = self
            .get(session_id)
            .await
            .ok_or_else(|| SandboxError::SessionNotFound(session_id.to_string()))?;

        container.reset().await?;

        tracing::debug!(session_id = %session_id, "Reset sandbox session");
        Ok(())
    }

    /// Cleanup timed out sessions.
    pub async fn cleanup_timeouts(&self) -> usize {
        let now = Utc::now();
        let timeout = chrono::Duration::seconds(self.session_timeout_secs as i64);

        let mut to_remove = Vec::new();

        {
            let sessions = self.sessions.read().await;
            for (session_id, session) in sessions.iter() {
                if !session.executing && now - session.last_active > timeout {
                    to_remove.push(session_id.clone());
                }
            }
        }

        let count = to_remove.len();
        for session_id in to_remove {
            let _ = self.end_session(&session_id).await;
        }

        if count > 0 {
            tracing::info!(count = count, "Cleaned up timed out sessions");
        }

        count
    }

    /// Mark a session as executing.
    pub async fn set_executing(&self, session_id: &str, executing: bool) {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(session_id) {
            session.executing = executing;
            if executing {
                session.increment_executions();
            }
        }
    }

    /// Get the number of active sessions.
    pub async fn session_count(&self) -> usize {
        let sessions = self.sessions.read().await;
        sessions.len()
    }

    /// Shutdown all sessions.
    pub async fn shutdown(&self) -> Result<()> {
        let session_ids: Vec<String> = {
            let sessions = self.sessions.read().await;
            sessions.keys().cloned().collect()
        };

        for session_id in session_ids {
            let _ = self.end_session(&session_id).await;
        }

        tracing::info!("Session manager shutdown complete");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_sandbox() {
        let session = SessionSandbox::new("session1", "container1", "python:3.11");

        assert_eq!(session.session_id, "session1");
        assert_eq!(session.container_id, "container1");
        assert_eq!(session.execution_count, 0);
        assert!(!session.executing);
    }
}
