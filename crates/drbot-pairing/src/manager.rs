//! Main pairing manager orchestrating all pairing operations.

use crate::{
    AllowlistEntry, AllowlistManager, ApprovalCode, ApprovalCodeGenerator, ChannelPairingConfig,
    ChannelPairingState, PairedSender, PairingConfig, PairingError, PairingMode, PairingStore,
    PendingApproval, Result,
};
use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

/// Result of a pairing check.
#[derive(Debug, Clone)]
pub enum PairingDecision {
    /// Sender is allowed to interact.
    Allowed,
    /// Sender needs to provide an approval code.
    NeedsApproval { code: ApprovalCode },
    /// Sender is not on the allowlist.
    NotAllowed { reason: String },
    /// Pairing mode is disabled.
    Disabled,
    /// Sender is already paired.
    AlreadyPaired,
}

/// Result of a pairing operation.
#[derive(Debug, Clone)]
pub struct PairingResult {
    /// The decision made.
    pub decision: PairingDecision,
    /// Message to send to the user.
    pub message: Option<String>,
}

/// Main pairing manager.
pub struct PairingManager<S: PairingStore> {
    /// Persistent storage.
    store: Arc<S>,
    /// Configuration.
    config: PairingConfig,
    /// Allowlist manager.
    allowlist: AllowlistManager,
    /// Channel-specific state.
    channel_state: ChannelPairingState,
    /// Approval code generator.
    code_generator: ApprovalCodeGenerator,
    /// Rate limiting state (sender_id -> (count, window_start)).
    rate_limits: Arc<RwLock<HashMap<String, (usize, i64)>>>,
}

impl<S: PairingStore> PairingManager<S> {
    /// Create a new pairing manager.
    pub async fn new(store: S, config: PairingConfig) -> Result<Self> {
        let store = Arc::new(store);

        // Load allowlist from store
        let allowlist_data = store.load_allowlist().await?;
        let allowlist = AllowlistManager::from_allowlist(allowlist_data);

        let code_generator =
            ApprovalCodeGenerator::new(config.code_length, config.code_validity_secs);

        let channel_state = ChannelPairingState::new(config.default_mode);

        Ok(Self {
            store,
            config,
            allowlist,
            channel_state,
            code_generator,
            rate_limits: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Check if a sender is allowed to interact.
    pub async fn check_sender(&self, sender_id: &str, channel: &str) -> Result<PairingResult> {
        // Check rate limiting first
        if self.config.rate_limiting_enabled {
            if !self.check_rate_limit(sender_id).await {
                return Ok(PairingResult {
                    decision: PairingDecision::NotAllowed {
                        reason: "Rate limit exceeded".into(),
                    },
                    message: Some("Too many requests. Please try again later.".into()),
                });
            }
        }

        let mode = self.channel_state.effective_mode(channel).await;

        match mode {
            PairingMode::Open => {
                // Auto-pair if configured
                if let Some(config) = self.channel_state.get_config(channel).await {
                    if config.auto_pair {
                        self.create_pairing(sender_id, channel, None).await?;
                    }
                }
                Ok(PairingResult {
                    decision: PairingDecision::Allowed,
                    message: None,
                })
            }

            PairingMode::Disabled => Ok(PairingResult {
                decision: PairingDecision::Disabled,
                message: Some("This bot is not accepting new conversations at this time.".into()),
            }),

            PairingMode::Allowlist => {
                if self.allowlist.is_allowed(sender_id, Some(channel)).await {
                    Ok(PairingResult {
                        decision: PairingDecision::Allowed,
                        message: None,
                    })
                } else {
                    Ok(PairingResult {
                        decision: PairingDecision::NotAllowed {
                            reason: "Not on allowlist".into(),
                        },
                        message: Some(
                            "You are not authorized to use this bot. Please contact an administrator."
                                .into(),
                        ),
                    })
                }
            }

            PairingMode::ApprovalCode | PairingMode::Hybrid => {
                // Check if already paired
                if let Some(_sender) = self.store.get_paired(sender_id, channel).await? {
                    return Ok(PairingResult {
                        decision: PairingDecision::Allowed,
                        message: None,
                    });
                }

                // For Hybrid mode, also check allowlist
                if mode == PairingMode::Hybrid
                    && self.allowlist.is_allowed(sender_id, Some(channel)).await
                {
                    return Ok(PairingResult {
                        decision: PairingDecision::Allowed,
                        message: None,
                    });
                }

                // Check for existing pending approval
                if let Some(pending) = self.store.get_pending(sender_id, channel).await? {
                    if !pending.code.is_expired() && pending.can_attempt() {
                        let remaining_secs = pending.code.remaining_secs();
                        return Ok(PairingResult {
                            decision: PairingDecision::NeedsApproval {
                                code: pending.code,
                            },
                            message: Some(format!(
                                "An approval code has already been sent. Please enter it to continue, or wait {} seconds for a new code.",
                                remaining_secs
                            )),
                        });
                    }
                }

                // Generate new approval code
                let code = self.generate_approval_code(sender_id, channel).await?;

                Ok(PairingResult {
                    decision: PairingDecision::NeedsApproval { code: code.clone() },
                    message: Some(format!(
                        "Please enter the approval code to start chatting. Your code is: {}",
                        code.code
                    )),
                })
            }
        }
    }

    /// Verify an approval code.
    pub async fn verify_approval_code(
        &self,
        sender_id: &str,
        channel: &str,
        code: &str,
    ) -> Result<bool> {
        let mut pending = self
            .store
            .get_pending(sender_id, channel)
            .await?
            .ok_or(PairingError::InvalidCode)?;

        pending.record_attempt();

        if pending.code.is_expired() {
            self.store.delete_pending(pending.id).await?;
            return Err(PairingError::CodeExpired);
        }

        if !pending.can_attempt() {
            self.store.delete_pending(pending.id).await?;
            return Err(PairingError::InvalidCode);
        }

        if pending.code.code == code {
            pending.consume();
            self.store.save_pending(&pending).await?;

            // Create the pairing
            self.create_pairing(sender_id, channel, Some("approval_code"))
                .await?;

            // For Hybrid mode, also add to allowlist
            let mode = self.channel_state.effective_mode(channel).await;
            if mode == PairingMode::Hybrid {
                let entry = AllowlistEntry::new(sender_id)
                    .with_channel(channel)
                    .with_added_by("approval_code");
                self.allowlist.add(entry.clone()).await;
                self.store.add_allowlist_entry(&entry).await?;
            }

            // Clean up pending approval
            self.store.delete_pending(pending.id).await?;

            if self.config.audit_logging {
                info!(
                    sender_id = %sender_id,
                    channel = %channel,
                    "Sender paired via approval code"
                );
            }

            Ok(true)
        } else {
            self.store.save_pending(&pending).await?;

            if self.config.audit_logging {
                warn!(
                    sender_id = %sender_id,
                    channel = %channel,
                    attempts = pending.attempts,
                    "Invalid approval code attempt"
                );
            }

            Ok(false)
        }
    }

    /// Generate a new approval code for a sender.
    pub async fn generate_approval_code(
        &self,
        sender_id: &str,
        channel: &str,
    ) -> Result<ApprovalCode> {
        // Delete any existing pending approval
        if let Some(pending) = self.store.get_pending(sender_id, channel).await? {
            self.store.delete_pending(pending.id).await?;
        }

        let code = self.code_generator.generate()?;
        let pending = PendingApproval::new(sender_id, channel, code.clone());

        self.store.save_pending(&pending).await?;

        if self.config.audit_logging {
            info!(
                sender_id = %sender_id,
                channel = %channel,
                expires_in = code.remaining_secs(),
                "Generated approval code"
            );
        }

        Ok(code)
    }

    /// Create a pairing record.
    async fn create_pairing(
        &self,
        sender_id: &str,
        channel: &str,
        approved_by: Option<&str>,
    ) -> Result<()> {
        // Check if already paired
        if self.store.get_paired(sender_id, channel).await?.is_some() {
            return Ok(()); // Already paired
        }

        // Check channel limits
        if !self.channel_state.can_add_pairing(channel).await {
            return Err(PairingError::ConfigError(
                "Maximum pairings reached for this channel".into(),
            ));
        }

        let sender = PairedSender {
            sender_id: sender_id.to_string(),
            channel: channel.to_string(),
            paired_at: Utc::now(),
            approved_by: approved_by.map(String::from),
            metadata: None,
        };

        self.store.save_paired(&sender).await?;
        self.channel_state.increment_pairing_count(channel).await;

        if self.config.audit_logging {
            info!(
                sender_id = %sender_id,
                channel = %channel,
                approved_by = ?approved_by,
                "Sender paired"
            );
        }

        Ok(())
    }

    /// Remove a pairing.
    pub async fn remove_pairing(&self, sender_id: &str, channel: &str) -> Result<bool> {
        let removed = self.store.delete_paired(sender_id, channel).await?;

        if removed {
            self.channel_state.decrement_pairing_count(channel).await;

            if self.config.audit_logging {
                info!(
                    sender_id = %sender_id,
                    channel = %channel,
                    "Sender pairing removed"
                );
            }
        }

        Ok(removed)
    }

    /// Add a sender to the allowlist.
    pub async fn add_to_allowlist(&self, entry: AllowlistEntry) -> Result<()> {
        self.allowlist.add(entry.clone()).await;
        self.store.add_allowlist_entry(&entry).await?;

        if self.config.audit_logging {
            info!(
                sender_id = %entry.sender_id,
                channel = ?entry.channel,
                added_by = ?entry.added_by,
                "Added to allowlist"
            );
        }

        Ok(())
    }

    /// Remove a sender from the allowlist.
    pub async fn remove_from_allowlist(
        &self,
        sender_id: &str,
        channel: Option<&str>,
    ) -> Result<bool> {
        self.allowlist.remove(sender_id, channel).await;
        let removed = self
            .store
            .remove_allowlist_entry(sender_id, channel)
            .await?;

        if removed && self.config.audit_logging {
            info!(
                sender_id = %sender_id,
                channel = ?channel,
                "Removed from allowlist"
            );
        }

        Ok(removed)
    }

    /// List all paired senders.
    pub async fn list_paired(&self, channel: Option<&str>) -> Result<Vec<PairedSender>> {
        self.store.list_paired(channel).await
    }

    /// List all allowlist entries.
    pub async fn list_allowlist(&self) -> Vec<AllowlistEntry> {
        self.allowlist.list_active().await
    }

    /// Set channel-specific configuration.
    pub async fn set_channel_config(&self, config: ChannelPairingConfig) {
        self.channel_state.set_config(config).await;
    }

    /// Get channel configuration.
    pub async fn get_channel_config(&self, channel: &str) -> Option<ChannelPairingConfig> {
        self.channel_state.get_config(channel).await
    }

    /// Set the pairing mode for a channel.
    pub async fn set_channel_mode(&self, channel: &str, mode: PairingMode) {
        let config = self
            .channel_state
            .get_config(channel)
            .await
            .unwrap_or_else(|| ChannelPairingConfig::new(channel))
            .with_mode(mode);

        self.channel_state.set_config(config).await;
    }

    /// Set the default pairing mode.
    pub async fn set_default_mode(&self, mode: PairingMode) {
        self.channel_state.set_default_mode(mode).await;
    }

    /// Clean up expired data.
    pub async fn cleanup(&self) -> Result<usize> {
        let expired_pending = self.store.delete_expired_pending().await?;
        let expired_allowlist = self.allowlist.cleanup().await;

        if self.config.audit_logging && (expired_pending > 0 || expired_allowlist > 0) {
            info!(
                expired_pending = expired_pending,
                expired_allowlist = expired_allowlist,
                "Cleaned up expired pairing data"
            );
        }

        Ok(expired_pending + expired_allowlist)
    }

    /// Check rate limiting for a sender.
    async fn check_rate_limit(&self, sender_id: &str) -> bool {
        let now = Utc::now().timestamp();
        let window_secs = self.config.rate_limit_window_secs as i64;
        let max_requests = self.config.rate_limit_max_requests;

        let mut limits = self.rate_limits.write().await;

        let (count, window_start) = limits.entry(sender_id.to_string()).or_insert((0, now));

        // Reset window if expired
        if now - *window_start >= window_secs {
            *count = 0;
            *window_start = now;
        }

        *count += 1;

        *count <= max_requests
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SqlitePairingStore;

    #[tokio::test]
    async fn test_pairing_manager_open_mode() {
        let store = SqlitePairingStore::in_memory().unwrap();
        let config = PairingConfig {
            default_mode: PairingMode::Open,
            ..Default::default()
        };
        let manager = PairingManager::new(store, config).await.unwrap();

        let result = manager.check_sender("user1", "telegram").await.unwrap();
        assert!(matches!(result.decision, PairingDecision::Allowed));
    }

    #[tokio::test]
    async fn test_pairing_manager_approval_code() {
        let store = SqlitePairingStore::in_memory().unwrap();
        let config = PairingConfig {
            default_mode: PairingMode::ApprovalCode,
            audit_logging: false,
            rate_limiting_enabled: false,
            ..Default::default()
        };
        let manager = PairingManager::new(store, config).await.unwrap();

        // First check should return a code
        let result = manager.check_sender("user1", "telegram").await.unwrap();
        let code = match result.decision {
            PairingDecision::NeedsApproval { code } => code.code,
            _ => panic!("Expected NeedsApproval"),
        };

        // Verify with correct code
        let verified = manager
            .verify_approval_code("user1", "telegram", &code)
            .await
            .unwrap();
        assert!(verified);

        // Now should be allowed
        let result = manager.check_sender("user1", "telegram").await.unwrap();
        assert!(matches!(result.decision, PairingDecision::Allowed));
    }

    #[tokio::test]
    async fn test_pairing_manager_allowlist() {
        let store = SqlitePairingStore::in_memory().unwrap();
        let config = PairingConfig {
            default_mode: PairingMode::Allowlist,
            audit_logging: false,
            rate_limiting_enabled: false,
            ..Default::default()
        };
        let manager = PairingManager::new(store, config).await.unwrap();

        // Not on allowlist
        let result = manager.check_sender("user1", "telegram").await.unwrap();
        assert!(matches!(
            result.decision,
            PairingDecision::NotAllowed { .. }
        ));

        // Add to allowlist
        manager
            .add_to_allowlist(AllowlistEntry::new("user1").with_channel("telegram"))
            .await
            .unwrap();

        // Now allowed
        let result = manager.check_sender("user1", "telegram").await.unwrap();
        assert!(matches!(result.decision, PairingDecision::Allowed));
    }
}
