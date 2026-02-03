//! Per-channel pairing configuration and state.

use crate::{PairingMode, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Per-channel pairing configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelPairingConfig {
    /// Channel identifier.
    pub channel: String,
    /// Pairing mode for this channel.
    pub mode: PairingMode,
    /// Whether this channel's config overrides the default.
    pub override_default: bool,
    /// Custom approval code validity for this channel (seconds).
    pub code_validity_secs: Option<u64>,
    /// Custom message for requesting approval.
    pub approval_message: Option<String>,
    /// Custom message for successful pairing.
    pub success_message: Option<String>,
    /// Custom message for denied access.
    pub denied_message: Option<String>,
    /// Maximum pairings allowed on this channel.
    pub max_pairings: Option<usize>,
    /// Whether to auto-pair on first message (for Open mode).
    pub auto_pair: bool,
}

impl ChannelPairingConfig {
    /// Create a new channel config with default settings.
    pub fn new(channel: &str) -> Self {
        Self {
            channel: channel.to_string(),
            mode: PairingMode::Open,
            override_default: false,
            code_validity_secs: None,
            approval_message: None,
            success_message: None,
            denied_message: None,
            max_pairings: None,
            auto_pair: true,
        }
    }

    /// Set the pairing mode.
    pub fn with_mode(mut self, mode: PairingMode) -> Self {
        self.mode = mode;
        self.override_default = true;
        self
    }

    /// Set code validity.
    pub fn with_code_validity(mut self, secs: u64) -> Self {
        self.code_validity_secs = Some(secs);
        self
    }

    /// Set custom messages.
    pub fn with_messages(
        mut self,
        approval: Option<&str>,
        success: Option<&str>,
        denied: Option<&str>,
    ) -> Self {
        self.approval_message = approval.map(String::from);
        self.success_message = success.map(String::from);
        self.denied_message = denied.map(String::from);
        self
    }

    /// Set maximum pairings.
    pub fn with_max_pairings(mut self, max: usize) -> Self {
        self.max_pairings = Some(max);
        self
    }

    /// Get the effective mode (considering default override).
    pub fn effective_mode(&self, default: PairingMode) -> PairingMode {
        if self.override_default {
            self.mode
        } else {
            default
        }
    }
}

/// Runtime state for channel pairing.
#[derive(Debug)]
pub struct ChannelPairingState {
    /// Channel configs by channel name.
    configs: Arc<RwLock<HashMap<String, ChannelPairingConfig>>>,
    /// Active pairing counts by channel.
    pairing_counts: Arc<RwLock<HashMap<String, usize>>>,
    /// Default pairing mode.
    default_mode: Arc<RwLock<PairingMode>>,
}

impl ChannelPairingState {
    /// Create new channel pairing state.
    pub fn new(default_mode: PairingMode) -> Self {
        Self {
            configs: Arc::new(RwLock::new(HashMap::new())),
            pairing_counts: Arc::new(RwLock::new(HashMap::new())),
            default_mode: Arc::new(RwLock::new(default_mode)),
        }
    }

    /// Set the default pairing mode.
    pub async fn set_default_mode(&self, mode: PairingMode) {
        let mut default = self.default_mode.write().await;
        *default = mode;
    }

    /// Get the default pairing mode.
    pub async fn default_mode(&self) -> PairingMode {
        *self.default_mode.read().await
    }

    /// Add or update a channel config.
    pub async fn set_config(&self, config: ChannelPairingConfig) {
        let mut configs = self.configs.write().await;
        configs.insert(config.channel.clone(), config);
    }

    /// Get a channel's config.
    pub async fn get_config(&self, channel: &str) -> Option<ChannelPairingConfig> {
        let configs = self.configs.read().await;
        configs.get(channel).cloned()
    }

    /// Remove a channel's config.
    pub async fn remove_config(&self, channel: &str) -> Option<ChannelPairingConfig> {
        let mut configs = self.configs.write().await;
        configs.remove(channel)
    }

    /// Get the effective pairing mode for a channel.
    pub async fn effective_mode(&self, channel: &str) -> PairingMode {
        let default = *self.default_mode.read().await;
        let configs = self.configs.read().await;

        if let Some(config) = configs.get(channel) {
            config.effective_mode(default)
        } else {
            default
        }
    }

    /// Get all channel configs.
    pub async fn all_configs(&self) -> Vec<ChannelPairingConfig> {
        let configs = self.configs.read().await;
        configs.values().cloned().collect()
    }

    /// Increment pairing count for a channel.
    pub async fn increment_pairing_count(&self, channel: &str) -> usize {
        let mut counts = self.pairing_counts.write().await;
        let count = counts.entry(channel.to_string()).or_insert(0);
        *count += 1;
        *count
    }

    /// Decrement pairing count for a channel.
    pub async fn decrement_pairing_count(&self, channel: &str) -> usize {
        let mut counts = self.pairing_counts.write().await;
        let count = counts.entry(channel.to_string()).or_insert(0);
        *count = count.saturating_sub(1);
        *count
    }

    /// Get current pairing count for a channel.
    pub async fn pairing_count(&self, channel: &str) -> usize {
        let counts = self.pairing_counts.read().await;
        counts.get(channel).copied().unwrap_or(0)
    }

    /// Check if a channel can accept more pairings.
    pub async fn can_add_pairing(&self, channel: &str) -> bool {
        let configs = self.configs.read().await;
        let counts = self.pairing_counts.read().await;

        if let Some(config) = configs.get(channel) {
            if let Some(max) = config.max_pairings {
                let current = counts.get(channel).copied().unwrap_or(0);
                return current < max;
            }
        }

        true // No limit
    }

    /// Set pairing count directly (useful when loading from store).
    pub async fn set_pairing_count(&self, channel: &str, count: usize) {
        let mut counts = self.pairing_counts.write().await;
        counts.insert(channel.to_string(), count);
    }
}

impl Default for ChannelPairingState {
    fn default() -> Self {
        Self::new(PairingMode::Open)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_channel_config() {
        let config = ChannelPairingConfig::new("telegram")
            .with_mode(PairingMode::ApprovalCode)
            .with_code_validity(600);

        assert_eq!(config.mode, PairingMode::ApprovalCode);
        assert_eq!(config.code_validity_secs, Some(600));
        assert!(config.override_default);
    }

    #[tokio::test]
    async fn test_channel_state() {
        let state = ChannelPairingState::new(PairingMode::Open);

        // Test default mode
        assert_eq!(state.effective_mode("telegram").await, PairingMode::Open);

        // Add channel-specific config
        state
            .set_config(ChannelPairingConfig::new("telegram").with_mode(PairingMode::ApprovalCode))
            .await;

        assert_eq!(
            state.effective_mode("telegram").await,
            PairingMode::ApprovalCode
        );
        assert_eq!(state.effective_mode("discord").await, PairingMode::Open);
    }

    #[tokio::test]
    async fn test_pairing_counts() {
        let state = ChannelPairingState::new(PairingMode::Open);

        state
            .set_config(ChannelPairingConfig::new("telegram").with_max_pairings(2))
            .await;

        assert!(state.can_add_pairing("telegram").await);

        state.increment_pairing_count("telegram").await;
        assert!(state.can_add_pairing("telegram").await);

        state.increment_pairing_count("telegram").await;
        assert!(!state.can_add_pairing("telegram").await);

        state.decrement_pairing_count("telegram").await;
        assert!(state.can_add_pairing("telegram").await);
    }
}
