//! Ambient awareness for drbot.
//!
//! Continuous context capture and environmental awareness.
//!
//! # Features
//!
//! - Screen context capture
//! - Active application monitoring
//! - Clipboard tracking
//! - Browser tab awareness
//! - Calendar integration
//! - Location awareness

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

/// Ambient result type.
pub type Result<T> = std::result::Result<T, AmbientError>;

/// Ambient errors.
#[derive(Debug, thiserror::Error)]
pub enum AmbientError {
    #[error("Capture failed: {0}")]
    CaptureFailed(String),
    #[error("Source unavailable: {0}")]
    SourceUnavailable(String),
    #[error("Privacy blocked: {0}")]
    PrivacyBlocked(String),
}

/// Context source types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextSource {
    Screen,
    ActiveApp,
    Clipboard,
    Browser,
    Calendar,
    Location,
    Audio,
    Notification,
    Custom,
}

/// Context snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextSnapshot {
    /// Snapshot ID.
    pub id: Uuid,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
    /// Active application.
    pub active_app: Option<AppContext>,
    /// Browser context.
    pub browser: Option<BrowserContext>,
    /// Clipboard content.
    pub clipboard: Option<ClipboardContext>,
    /// Calendar context.
    pub calendar: Option<CalendarContext>,
    /// Location.
    pub location: Option<LocationContext>,
    /// Screen text (OCR).
    pub screen_text: Option<String>,
    /// Custom data.
    pub custom: HashMap<String, String>,
}

impl ContextSnapshot {
    /// Create a new empty snapshot.
    pub fn new() -> Self {
        Self {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            active_app: None,
            browser: None,
            clipboard: None,
            calendar: None,
            location: None,
            screen_text: None,
            custom: HashMap::new(),
        }
    }

    /// Check if snapshot has any context.
    pub fn has_context(&self) -> bool {
        self.active_app.is_some()
            || self.browser.is_some()
            || self.clipboard.is_some()
            || self.calendar.is_some()
            || self.location.is_some()
            || self.screen_text.is_some()
    }

    /// Get context summary.
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();

        if let Some(ref app) = self.active_app {
            parts.push(format!("App: {}", app.name));
        }
        if let Some(ref browser) = self.browser {
            if let Some(ref tab) = browser.active_tab {
                parts.push(format!("Tab: {}", tab.title));
            }
        }
        if let Some(ref cal) = self.calendar {
            if let Some(ref event) = cal.current_event {
                parts.push(format!("Event: {}", event.title));
            }
        }
        if let Some(ref loc) = self.location {
            parts.push(format!(
                "Location: {}",
                loc.name.as_deref().unwrap_or("Unknown")
            ));
        }

        if parts.is_empty() {
            "No active context".to_string()
        } else {
            parts.join(" | ")
        }
    }
}

impl Default for ContextSnapshot {
    fn default() -> Self {
        Self::new()
    }
}

/// Application context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppContext {
    /// App name.
    pub name: String,
    /// Bundle ID or path.
    pub identifier: String,
    /// Window title.
    pub window_title: Option<String>,
    /// Is focused.
    pub focused: bool,
    /// Time spent (seconds).
    pub time_spent: u64,
}

/// Browser context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserContext {
    /// Browser name.
    pub browser: String,
    /// Active tab.
    pub active_tab: Option<BrowserTab>,
    /// Recent tabs.
    pub recent_tabs: Vec<BrowserTab>,
    /// Total tabs.
    pub total_tabs: usize,
}

/// Browser tab.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserTab {
    /// Tab ID.
    pub id: String,
    /// Title.
    pub title: String,
    /// URL.
    pub url: String,
    /// Is active.
    pub active: bool,
    /// Domain.
    pub domain: String,
}

/// Clipboard context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardContext {
    /// Content type.
    pub content_type: ClipboardType,
    /// Text content (if applicable).
    pub text: Option<String>,
    /// Has image.
    pub has_image: bool,
    /// Size (bytes).
    pub size: usize,
    /// Changed at.
    pub changed_at: DateTime<Utc>,
}

/// Clipboard content types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClipboardType {
    Text,
    Html,
    Image,
    Files,
    Other,
}

/// Calendar context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalendarContext {
    /// Current event.
    pub current_event: Option<CalendarEvent>,
    /// Next event.
    pub next_event: Option<CalendarEvent>,
    /// Events today.
    pub events_today: usize,
    /// Is in meeting.
    pub in_meeting: bool,
}

/// Calendar event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalendarEvent {
    /// Event ID.
    pub id: String,
    /// Title.
    pub title: String,
    /// Start time.
    pub start: DateTime<Utc>,
    /// End time.
    pub end: DateTime<Utc>,
    /// Location.
    pub location: Option<String>,
    /// Attendees.
    pub attendees: Vec<String>,
    /// Is video call.
    pub is_video_call: bool,
}

/// Location context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocationContext {
    /// Location name (if known).
    pub name: Option<String>,
    /// Location type.
    pub location_type: LocationType,
    /// Latitude.
    pub latitude: Option<f64>,
    /// Longitude.
    pub longitude: Option<f64>,
    /// WiFi network.
    pub wifi_network: Option<String>,
}

/// Location types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocationType {
    Home,
    Office,
    Cafe,
    Transit,
    Unknown,
}

/// Ambient event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AmbientEvent {
    /// App switched.
    AppSwitched { from: Option<String>, to: String },
    /// Tab changed.
    TabChanged { url: String, title: String },
    /// Clipboard changed.
    ClipboardChanged { content_type: ClipboardType },
    /// Meeting started.
    MeetingStarted { event: CalendarEvent },
    /// Meeting ended.
    MeetingEnded { event: CalendarEvent },
    /// Location changed.
    LocationChanged { location: LocationContext },
    /// Custom event.
    Custom {
        name: String,
        data: HashMap<String, String>,
    },
}

/// Privacy settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivacySettings {
    /// Capture screen text.
    pub capture_screen: bool,
    /// Track clipboard.
    pub track_clipboard: bool,
    /// Track browser.
    pub track_browser: bool,
    /// Track location.
    pub track_location: bool,
    /// Excluded apps.
    pub excluded_apps: Vec<String>,
    /// Excluded URLs.
    pub excluded_urls: Vec<String>,
}

impl Default for PrivacySettings {
    fn default() -> Self {
        Self {
            capture_screen: false, // Off by default for privacy
            track_clipboard: true,
            track_browser: true,
            track_location: false, // Off by default
            excluded_apps: vec!["1Password".to_string(), "Keychain".to_string()],
            excluded_urls: vec!["bank".to_string(), "password".to_string()],
        }
    }
}

/// Ambient configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AmbientConfig {
    /// Capture interval (seconds).
    pub capture_interval: u64,
    /// History limit.
    pub history_limit: usize,
    /// Privacy settings.
    pub privacy: PrivacySettings,
}

impl Default for AmbientConfig {
    fn default() -> Self {
        Self {
            capture_interval: 5,
            history_limit: 1000,
            privacy: PrivacySettings::default(),
        }
    }
}

/// Trait for context providers.
#[async_trait]
pub trait ContextProvider: Send + Sync {
    /// Get source type.
    fn source(&self) -> ContextSource;
    /// Capture current context.
    async fn capture(&self) -> Result<Option<serde_json::Value>>;
}

/// Ambient awareness engine.
pub struct AmbientEngine {
    config: AmbientConfig,
    providers: Arc<RwLock<HashMap<ContextSource, Box<dyn ContextProvider>>>>,
    history: Arc<RwLock<VecDeque<ContextSnapshot>>>,
    current: Arc<RwLock<ContextSnapshot>>,
    event_tx: broadcast::Sender<AmbientEvent>,
}

impl AmbientEngine {
    /// Create a new ambient engine.
    pub fn new(config: AmbientConfig) -> Self {
        let (event_tx, _) = broadcast::channel(100);
        Self {
            config,
            providers: Arc::new(RwLock::new(HashMap::new())),
            history: Arc::new(RwLock::new(VecDeque::new())),
            current: Arc::new(RwLock::new(ContextSnapshot::new())),
            event_tx,
        }
    }

    /// Subscribe to events.
    pub fn subscribe(&self) -> broadcast::Receiver<AmbientEvent> {
        self.event_tx.subscribe()
    }

    /// Register a context provider.
    pub async fn register_provider(&self, provider: Box<dyn ContextProvider>) {
        let source = provider.source();
        self.providers.write().await.insert(source, provider);
    }

    /// Capture current context.
    pub async fn capture(&self) -> Result<ContextSnapshot> {
        let mut snapshot = ContextSnapshot::new();
        let providers = self.providers.read().await;

        for (source, provider) in providers.iter() {
            if let Ok(Some(data)) = provider.capture().await {
                match source {
                    ContextSource::ActiveApp => {
                        if let Ok(app) = serde_json::from_value(data) {
                            // Check privacy exclusions
                            let app_ctx: AppContext = app;
                            if !self
                                .config
                                .privacy
                                .excluded_apps
                                .iter()
                                .any(|e| app_ctx.name.to_lowercase().contains(&e.to_lowercase()))
                            {
                                snapshot.active_app = Some(app_ctx);
                            }
                        }
                    }
                    ContextSource::Browser => {
                        if self.config.privacy.track_browser {
                            if let Ok(browser) = serde_json::from_value(data) {
                                snapshot.browser = Some(browser);
                            }
                        }
                    }
                    ContextSource::Clipboard => {
                        if self.config.privacy.track_clipboard {
                            if let Ok(clipboard) = serde_json::from_value(data) {
                                snapshot.clipboard = Some(clipboard);
                            }
                        }
                    }
                    ContextSource::Calendar => {
                        if let Ok(calendar) = serde_json::from_value(data) {
                            snapshot.calendar = Some(calendar);
                        }
                    }
                    ContextSource::Location => {
                        if self.config.privacy.track_location {
                            if let Ok(location) = serde_json::from_value(data) {
                                snapshot.location = Some(location);
                            }
                        }
                    }
                    ContextSource::Screen => {
                        if self.config.privacy.capture_screen {
                            if let Ok(text) = serde_json::from_value::<String>(data) {
                                snapshot.screen_text = Some(text);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        // Detect changes and emit events
        self.detect_changes(&snapshot).await;

        // Store in history
        {
            let mut history = self.history.write().await;
            history.push_back(snapshot.clone());
            while history.len() > self.config.history_limit {
                history.pop_front();
            }
        }

        *self.current.write().await = snapshot.clone();

        Ok(snapshot)
    }

    async fn detect_changes(&self, new: &ContextSnapshot) {
        let current = self.current.read().await;

        // App switch
        if let (Some(old_app), Some(new_app)) = (&current.active_app, &new.active_app) {
            if old_app.name != new_app.name {
                let _ = self.event_tx.send(AmbientEvent::AppSwitched {
                    from: Some(old_app.name.clone()),
                    to: new_app.name.clone(),
                });
            }
        } else if let Some(new_app) = &new.active_app {
            let _ = self.event_tx.send(AmbientEvent::AppSwitched {
                from: None,
                to: new_app.name.clone(),
            });
        }

        // Tab change
        if let (Some(old_browser), Some(new_browser)) = (&current.browser, &new.browser) {
            if let (Some(old_tab), Some(new_tab)) =
                (&old_browser.active_tab, &new_browser.active_tab)
            {
                if old_tab.url != new_tab.url {
                    let _ = self.event_tx.send(AmbientEvent::TabChanged {
                        url: new_tab.url.clone(),
                        title: new_tab.title.clone(),
                    });
                }
            }
        }

        // Meeting changes
        if let (Some(old_cal), Some(new_cal)) = (&current.calendar, &new.calendar) {
            if !old_cal.in_meeting && new_cal.in_meeting {
                if let Some(ref event) = new_cal.current_event {
                    let _ = self.event_tx.send(AmbientEvent::MeetingStarted {
                        event: event.clone(),
                    });
                }
            } else if old_cal.in_meeting && !new_cal.in_meeting {
                if let Some(ref event) = old_cal.current_event {
                    let _ = self.event_tx.send(AmbientEvent::MeetingEnded {
                        event: event.clone(),
                    });
                }
            }
        }

        // Location change
        if let (Some(old_loc), Some(new_loc)) = (&current.location, &new.location) {
            if old_loc.location_type != new_loc.location_type || old_loc.name != new_loc.name {
                let _ = self.event_tx.send(AmbientEvent::LocationChanged {
                    location: new_loc.clone(),
                });
            }
        }
    }

    /// Get current context.
    pub async fn get_current(&self) -> ContextSnapshot {
        self.current.read().await.clone()
    }

    /// Get recent context history.
    pub async fn get_history(&self, limit: usize) -> Vec<ContextSnapshot> {
        self.history
            .read()
            .await
            .iter()
            .rev()
            .take(limit)
            .cloned()
            .collect()
    }

    /// Get context at specific time.
    pub async fn get_at(&self, time: DateTime<Utc>) -> Option<ContextSnapshot> {
        self.history
            .read()
            .await
            .iter()
            .rev()
            .find(|s| s.timestamp <= time)
            .cloned()
    }

    /// Get context summary for AI.
    pub async fn get_context_for_ai(&self) -> String {
        let current = self.current.read().await;
        let mut parts = Vec::new();

        parts.push(format!(
            "Current time: {}",
            Utc::now().format("%Y-%m-%d %H:%M")
        ));

        if let Some(ref app) = current.active_app {
            parts.push(format!(
                "Active app: {} ({})",
                app.name,
                app.window_title.as_deref().unwrap_or("")
            ));
        }

        if let Some(ref browser) = current.browser {
            if let Some(ref tab) = browser.active_tab {
                parts.push(format!("Browser tab: {} ({})", tab.title, tab.domain));
            }
        }

        if let Some(ref cal) = current.calendar {
            if cal.in_meeting {
                if let Some(ref event) = cal.current_event {
                    parts.push(format!("In meeting: {}", event.title));
                }
            } else if let Some(ref next) = cal.next_event {
                let until = next.start - Utc::now();
                parts.push(format!(
                    "Next event in {} mins: {}",
                    until.num_minutes(),
                    next.title
                ));
            }
        }

        if let Some(ref loc) = current.location {
            parts.push(format!("Location: {:?}", loc.location_type));
        }

        parts.join("\n")
    }

    /// Update privacy settings.
    pub async fn update_privacy(&self, settings: PrivacySettings) {
        // Note: In real impl, would need interior mutability for config
        // For now, this is a placeholder
        let _ = settings;
    }

    /// Get statistics.
    pub async fn stats(&self) -> AmbientStats {
        let history = self.history.read().await;

        let mut app_time: HashMap<String, u64> = HashMap::new();
        let mut captures = 0;

        for snapshot in history.iter() {
            captures += 1;
            if let Some(ref app) = snapshot.active_app {
                *app_time.entry(app.name.clone()).or_insert(0) += self.config.capture_interval;
            }
        }

        AmbientStats {
            total_captures: captures,
            history_duration_mins: (captures as u64 * self.config.capture_interval) / 60,
            app_usage: app_time,
        }
    }
}

/// Ambient statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AmbientStats {
    pub total_captures: usize,
    pub history_duration_mins: u64,
    pub app_usage: HashMap<String, u64>,
}

/// Mock app context provider for testing.
pub struct MockAppProvider {
    app_name: String,
}

impl MockAppProvider {
    pub fn new(app_name: &str) -> Self {
        Self {
            app_name: app_name.to_string(),
        }
    }
}

#[async_trait]
impl ContextProvider for MockAppProvider {
    fn source(&self) -> ContextSource {
        ContextSource::ActiveApp
    }

    async fn capture(&self) -> Result<Option<serde_json::Value>> {
        let app = AppContext {
            name: self.app_name.clone(),
            identifier: format!("com.example.{}", self.app_name.to_lowercase()),
            window_title: Some(format!("{} - Main Window", self.app_name)),
            focused: true,
            time_spent: 0,
        };
        Ok(Some(serde_json::to_value(app).unwrap()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_capture() {
        let engine = AmbientEngine::new(AmbientConfig::default());
        engine
            .register_provider(Box::new(MockAppProvider::new("VSCode")))
            .await;

        let snapshot = engine.capture().await.unwrap();
        assert!(snapshot.active_app.is_some());
        assert_eq!(snapshot.active_app.unwrap().name, "VSCode");
    }

    #[tokio::test]
    async fn test_history() {
        let engine = AmbientEngine::new(AmbientConfig::default());
        engine
            .register_provider(Box::new(MockAppProvider::new("Test")))
            .await;

        for _ in 0..5 {
            engine.capture().await.unwrap();
        }

        let history = engine.get_history(10).await;
        assert_eq!(history.len(), 5);
    }

    #[tokio::test]
    async fn test_privacy_exclusion() {
        let mut config = AmbientConfig::default();
        config.privacy.excluded_apps.push("Sensitive".to_string());

        let engine = AmbientEngine::new(config);
        engine
            .register_provider(Box::new(MockAppProvider::new("Sensitive App")))
            .await;

        let snapshot = engine.capture().await.unwrap();
        assert!(snapshot.active_app.is_none()); // Should be excluded
    }

    #[tokio::test]
    async fn test_context_summary() {
        let mut snapshot = ContextSnapshot::new();
        snapshot.active_app = Some(AppContext {
            name: "VSCode".to_string(),
            identifier: "com.microsoft.vscode".to_string(),
            window_title: Some("main.rs".to_string()),
            focused: true,
            time_spent: 100,
        });

        let summary = snapshot.summary();
        assert!(summary.contains("VSCode"));
    }

    #[tokio::test]
    async fn test_context_for_ai() {
        let engine = AmbientEngine::new(AmbientConfig::default());
        engine
            .register_provider(Box::new(MockAppProvider::new("Terminal")))
            .await;

        engine.capture().await.unwrap();

        let context = engine.get_context_for_ai().await;
        assert!(context.contains("Terminal"));
    }

    #[tokio::test]
    async fn test_stats() {
        let engine = AmbientEngine::new(AmbientConfig::default());
        engine
            .register_provider(Box::new(MockAppProvider::new("App1")))
            .await;

        for _ in 0..3 {
            engine.capture().await.unwrap();
        }

        let stats = engine.stats().await;
        assert_eq!(stats.total_captures, 3);
        assert!(stats.app_usage.contains_key("App1"));
    }
}
