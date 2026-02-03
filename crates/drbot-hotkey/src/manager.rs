//! Hotkey manager for registering and listening to global hotkeys.

use crate::{Hotkey, HotkeyError, HotkeyEvent, Result};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, info};

/// Manager for global hotkeys.
pub struct HotkeyManager {
    /// Registered hotkeys.
    hotkeys: Arc<RwLock<HashMap<String, Hotkey>>>,
    /// Event receiver.
    event_rx: mpsc::Receiver<HotkeyEvent>,
    /// Event sender (for platform-specific code).
    event_tx: mpsc::Sender<HotkeyEvent>,
    /// Running state.
    running: Arc<RwLock<bool>>,
}

impl HotkeyManager {
    /// Create a new hotkey manager.
    pub async fn new() -> Result<Self> {
        let (event_tx, event_rx) = mpsc::channel(32);

        let manager = Self {
            hotkeys: Arc::new(RwLock::new(HashMap::new())),
            event_rx,
            event_tx,
            running: Arc::new(RwLock::new(false)),
        };

        info!("Hotkey manager initialized");

        Ok(manager)
    }

    /// Register a hotkey.
    pub async fn register(&mut self, hotkey: Hotkey, id: &str) -> Result<()> {
        let mut hotkeys = self.hotkeys.write().await;

        if hotkeys.contains_key(id) {
            return Err(HotkeyError::AlreadyRegistered(id.to_string()));
        }

        // Platform-specific registration would happen here
        #[cfg(target_os = "macos")]
        self.register_macos(&hotkey, id).await?;

        #[cfg(target_os = "windows")]
        self.register_windows(&hotkey, id).await?;

        #[cfg(target_os = "linux")]
        self.register_linux(&hotkey, id).await?;

        hotkeys.insert(id.to_string(), hotkey.clone());
        info!(id = %id, hotkey = %hotkey, "Registered hotkey");

        Ok(())
    }

    /// Unregister a hotkey.
    pub async fn unregister(&mut self, id: &str) -> Result<()> {
        let mut hotkeys = self.hotkeys.write().await;

        if hotkeys.remove(id).is_some() {
            // Platform-specific unregistration would happen here
            debug!(id = %id, "Unregistered hotkey");
        }

        Ok(())
    }

    /// Start listening for hotkey events.
    pub async fn start(&self) -> Result<()> {
        let mut running = self.running.write().await;
        *running = true;

        // Platform-specific event loop would be started here
        info!("Hotkey manager started");

        Ok(())
    }

    /// Stop listening for hotkey events.
    pub async fn stop(&self) -> Result<()> {
        let mut running = self.running.write().await;
        *running = false;

        info!("Hotkey manager stopped");

        Ok(())
    }

    /// Get the next hotkey event.
    pub async fn next_event(&mut self) -> Option<HotkeyEvent> {
        self.event_rx.recv().await
    }

    /// Check if a hotkey is registered.
    pub async fn is_registered(&self, id: &str) -> bool {
        let hotkeys = self.hotkeys.read().await;
        hotkeys.contains_key(id)
    }

    /// Get all registered hotkeys.
    pub async fn list_hotkeys(&self) -> Vec<(String, Hotkey)> {
        let hotkeys = self.hotkeys.read().await;
        hotkeys
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    /// Manually trigger a hotkey event (for testing).
    pub async fn trigger(&self, id: &str) -> Result<()> {
        let hotkeys = self.hotkeys.read().await;

        if let Some(hotkey) = hotkeys.get(id) {
            let event = HotkeyEvent::new(id, hotkey.clone());
            self.event_tx
                .send(event)
                .await
                .map_err(|_| HotkeyError::Internal("Channel closed".to_string()))?;
        }

        Ok(())
    }

    // Platform-specific implementations

    #[cfg(target_os = "macos")]
    async fn register_macos(&self, hotkey: &Hotkey, id: &str) -> Result<()> {
        // In a full implementation, this would use Carbon Events API
        // or CGEventTap for global hotkey registration

        // For now, we'll use a stub that works with the event loop
        debug!(id = %id, "macOS hotkey registration (stub)");
        Ok(())
    }

    #[cfg(target_os = "windows")]
    async fn register_windows(&self, hotkey: &Hotkey, id: &str) -> Result<()> {
        // Would use RegisterHotKey from Windows API
        debug!(id = %id, "Windows hotkey registration (stub)");
        Ok(())
    }

    #[cfg(target_os = "linux")]
    async fn register_linux(&self, hotkey: &Hotkey, id: &str) -> Result<()> {
        // Would use X11 XGrabKey or similar
        debug!(id = %id, "Linux hotkey registration (stub)");
        Ok(())
    }
}

/// Builder for HotkeyManager with predefined hotkeys.
pub struct HotkeyManagerBuilder {
    hotkeys: Vec<(String, Hotkey)>,
}

impl HotkeyManagerBuilder {
    /// Create a new builder.
    pub fn new() -> Self {
        Self {
            hotkeys: Vec::new(),
        }
    }

    /// Add a hotkey.
    pub fn with_hotkey(mut self, id: &str, hotkey: Hotkey) -> Self {
        self.hotkeys.push((id.to_string(), hotkey));
        self
    }

    /// Add a hotkey from string.
    pub fn with_hotkey_str(mut self, id: &str, hotkey_str: &str) -> Self {
        if let Some(hotkey) = Hotkey::parse(hotkey_str) {
            self.hotkeys.push((id.to_string(), hotkey));
        }
        self
    }

    /// Build the manager.
    pub async fn build(self) -> Result<HotkeyManager> {
        let mut manager = HotkeyManager::new().await?;

        for (id, hotkey) in self.hotkeys {
            manager.register(hotkey, &id).await?;
        }

        Ok(manager)
    }
}

impl Default for HotkeyManagerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Key, Modifier};

    #[tokio::test]
    async fn test_manager_creation() {
        let manager = HotkeyManager::new().await.unwrap();
        assert!(manager.list_hotkeys().await.is_empty());
    }

    #[tokio::test]
    async fn test_register_hotkey() {
        let mut manager = HotkeyManager::new().await.unwrap();

        let hotkey = Hotkey::new(Key::Space).with_modifier(Modifier::Meta);
        manager.register(hotkey, "activate").await.unwrap();

        assert!(manager.is_registered("activate").await);
    }

    #[tokio::test]
    async fn test_unregister_hotkey() {
        let mut manager = HotkeyManager::new().await.unwrap();

        let hotkey = Hotkey::new(Key::C).with_modifier(Modifier::Control);
        manager.register(hotkey, "copy").await.unwrap();

        manager.unregister("copy").await.unwrap();
        assert!(!manager.is_registered("copy").await);
    }

    #[tokio::test]
    async fn test_builder() {
        let manager = HotkeyManagerBuilder::new()
            .with_hotkey_str("activate", "Cmd+Space")
            .with_hotkey_str("capture", "Cmd+Shift+C")
            .build()
            .await
            .unwrap();

        assert!(manager.is_registered("activate").await);
        assert!(manager.is_registered("capture").await);
    }

    #[tokio::test]
    async fn test_trigger_event() {
        let mut manager = HotkeyManager::new().await.unwrap();

        let hotkey = Hotkey::new(Key::Space).with_modifier(Modifier::Meta);
        manager.register(hotkey, "test").await.unwrap();

        manager.trigger("test").await.unwrap();

        let event = manager.next_event().await;
        assert!(event.is_some());
        assert_eq!(event.unwrap().id, "test");
    }
}
