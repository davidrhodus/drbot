//! Program watcher for monitoring on-chain programs.
//!
//! Monitors Solana programs for upgrades, authority changes,
//! and other significant events.

use crate::{Result, SolanaError};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Default programs to watch.
pub const DEFAULT_WATCH_LIST: &[(&str, &str)] = &[
    ("Solend", "So1endDq2YkqhipRh3WViPa8hdiSpxWy6z3Z6tMCpAo"),
    ("Marginfi", "MFv2hWf31Z9kbCa1snEPYctwafyhdvnV7FZnsebVacA"),
    ("Marinade", "MarBmsSgKXdrN1egZf5sqe1TMai9K1rChYNDJgjq7aD"),
    ("Jupiter", "JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4"),
    ("Kamino", "KLend2g3cP87ber41GFZMoD7yPu45R8LRFH9Wb9fjXp"),
    ("Jito", "Jito4APyf642JPZPx3hGc6WWJ8zPKtRbRs4P815Awbb"),
    ("Orca", "whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc"),
    ("Raydium", "CAMMCzo5YL8w4VFF8KVHrK22GGUsp5VTaW7grrKgrWqK"),
    ("Pyth", "FsJ3A3u2vn5cTVofAjvy6y5kwABJAqYWpe4975bi2epH"),
];

/// Program watcher for monitoring on-chain programs.
pub struct ProgramWatcher {
    rpc_client: Arc<RpcClient>,
    watched_programs: Arc<RwLock<HashMap<Pubkey, WatchedProgram>>>,
    events: Arc<RwLock<Vec<ProgramEvent>>>,
    max_events: usize,
}

impl ProgramWatcher {
    /// Create a new program watcher.
    pub fn new(rpc_client: Arc<RpcClient>) -> Self {
        Self {
            rpc_client,
            watched_programs: Arc::new(RwLock::new(HashMap::new())),
            events: Arc::new(RwLock::new(Vec::new())),
            max_events: 1000,
        }
    }

    /// Create with default watch list.
    pub async fn with_defaults(rpc_client: Arc<RpcClient>) -> Result<Self> {
        let watcher = Self::new(rpc_client);

        for (name, address) in DEFAULT_WATCH_LIST {
            let pubkey = Pubkey::from_str(address)
                .map_err(|e| SolanaError::InvalidAddress(e.to_string()))?;
            watcher.watch(pubkey, name.to_string()).await?;
        }

        Ok(watcher)
    }

    /// Add a program to the watch list.
    pub async fn watch(&self, program_id: Pubkey, name: String) -> Result<()> {
        // Fetch initial program state
        let program_info = self.fetch_program_info(&program_id).await?;

        let watched = WatchedProgram {
            address: program_id,
            name,
            upgrade_authority: program_info.upgrade_authority,
            last_deployed_slot: program_info.last_deployed_slot,
            data_hash: program_info.data_hash,
            executable_data_address: program_info.executable_data_address,
            is_upgradeable: program_info.is_upgradeable,
            added_at: Utc::now(),
            last_checked_at: Utc::now(),
        };

        self.watched_programs
            .write()
            .await
            .insert(program_id, watched);

        info!(
            program = %program_id,
            name = %program_info.name,
            "Added program to watch list"
        );

        Ok(())
    }

    /// Remove a program from the watch list.
    pub async fn unwatch(&self, program_id: &Pubkey) -> bool {
        self.watched_programs
            .write()
            .await
            .remove(program_id)
            .is_some()
    }

    /// Get watched programs.
    pub async fn get_watched(&self) -> Vec<WatchedProgram> {
        self.watched_programs
            .read()
            .await
            .values()
            .cloned()
            .collect()
    }

    /// Get a specific watched program.
    pub async fn get_program(&self, program_id: &Pubkey) -> Option<WatchedProgram> {
        self.watched_programs.read().await.get(program_id).cloned()
    }

    /// Check all watched programs for changes.
    pub async fn check_all(&self) -> Result<Vec<ProgramEvent>> {
        let programs: Vec<WatchedProgram> = self
            .watched_programs
            .read()
            .await
            .values()
            .cloned()
            .collect();

        let mut events = Vec::new();

        for program in programs {
            match self.check_program(&program.address).await {
                Ok(program_events) => events.extend(program_events),
                Err(e) => {
                    warn!(
                        program = %program.address,
                        error = %e,
                        "Failed to check program"
                    );
                }
            }
        }

        Ok(events)
    }

    /// Check a specific program for changes.
    pub async fn check_program(&self, program_id: &Pubkey) -> Result<Vec<ProgramEvent>> {
        let mut events = Vec::new();

        let current_info = self.fetch_program_info(program_id).await?;

        let mut programs = self.watched_programs.write().await;
        let watched = programs.get_mut(program_id).ok_or_else(|| {
            SolanaError::ConfigError(format!("Program {} not in watch list", program_id))
        })?;

        // Check for program upgrade
        if current_info.last_deployed_slot > watched.last_deployed_slot {
            let event = ProgramEvent::ProgramUpgraded {
                program: *program_id,
                name: watched.name.clone(),
                old_slot: watched.last_deployed_slot,
                new_slot: current_info.last_deployed_slot,
                old_hash: watched.data_hash.clone(),
                new_hash: current_info.data_hash.clone(),
                timestamp: Utc::now(),
            };

            info!(
                program = %program_id,
                name = %watched.name,
                old_slot = watched.last_deployed_slot,
                new_slot = current_info.last_deployed_slot,
                "Program upgraded"
            );

            events.push(event);
            watched.last_deployed_slot = current_info.last_deployed_slot;
            watched.data_hash = current_info.data_hash.clone();
        }

        // Check for upgrade authority change
        if current_info.upgrade_authority != watched.upgrade_authority {
            let event = ProgramEvent::UpgradeAuthorityChanged {
                program: *program_id,
                name: watched.name.clone(),
                old_authority: watched.upgrade_authority,
                new_authority: current_info.upgrade_authority,
                timestamp: Utc::now(),
            };

            info!(
                program = %program_id,
                name = %watched.name,
                old = ?watched.upgrade_authority,
                new = ?current_info.upgrade_authority,
                "Upgrade authority changed"
            );

            events.push(event);
            watched.upgrade_authority = current_info.upgrade_authority;

            // Check if program became immutable
            if current_info.upgrade_authority.is_none() && watched.upgrade_authority.is_some() {
                events.push(ProgramEvent::ImmutableSet {
                    program: *program_id,
                    name: watched.name.clone(),
                    timestamp: Utc::now(),
                });
            }
        }

        watched.last_checked_at = Utc::now();

        // Store events
        let mut all_events = self.events.write().await;
        for event in &events {
            all_events.push(event.clone());
        }

        // Trim old events
        while all_events.len() > self.max_events {
            all_events.remove(0);
        }

        Ok(events)
    }

    /// Fetch program info from chain.
    async fn fetch_program_info(&self, program_id: &Pubkey) -> Result<ProgramInfo> {
        // Fetch program account
        let account = self.rpc_client.get_account(program_id).await?;

        // Check if it's an upgradeable program (BPF Loader Upgradeable)
        let bpf_upgradeable_loader = solana_sdk::bpf_loader_upgradeable::id();

        let (is_upgradeable, upgrade_authority, executable_data_address) =
            if account.owner == bpf_upgradeable_loader {
                // Parse program data account
                // The program account data contains a pointer to the executable data
                let program_data_address = if account.data.len() >= 36 {
                    // Skip the 4-byte discriminator
                    let data_slice: [u8; 32] = account.data[4..36].try_into().unwrap_or([0; 32]);
                    Some(Pubkey::new_from_array(data_slice))
                } else {
                    None
                };

                let authority = if let Some(data_addr) = program_data_address {
                    // Fetch the program data account to get upgrade authority
                    match self.rpc_client.get_account(&data_addr).await {
                        Ok(data_account) => {
                            if data_account.data.len() >= 45 {
                                // Check if authority is set (not all zeros)
                                let auth_option = data_account.data[4]; // 0 = None, 1 = Some
                                if auth_option == 1 {
                                    let auth_slice: [u8; 32] =
                                        data_account.data[5..37].try_into().unwrap_or([0; 32]);
                                    Some(Pubkey::new_from_array(auth_slice))
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        }
                        Err(_) => None,
                    }
                } else {
                    None
                };

                (true, authority, program_data_address)
            } else {
                (false, None, None)
            };

        // Calculate data hash
        let data_hash = calculate_hash(&account.data);

        // Get the slot when last deployed
        let signatures = self
            .rpc_client
            .get_signatures_for_address(program_id)
            .await
            .unwrap_or_default();

        let last_deployed_slot = signatures.first().map(|s| s.slot).unwrap_or(0);

        Ok(ProgramInfo {
            name: String::new(),
            address: *program_id,
            upgrade_authority,
            executable_data_address,
            is_upgradeable,
            last_deployed_slot,
            data_hash,
        })
    }

    /// Get recent events.
    pub async fn get_events(&self, limit: Option<usize>) -> Vec<ProgramEvent> {
        let events = self.events.read().await;
        let limit = limit.unwrap_or(100);

        events.iter().rev().take(limit).cloned().collect()
    }

    /// Get events for a specific program.
    pub async fn get_program_events(&self, program_id: &Pubkey) -> Vec<ProgramEvent> {
        self.events
            .read()
            .await
            .iter()
            .filter(|e| &e.program_id() == program_id)
            .cloned()
            .collect()
    }
}

/// A watched program.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchedProgram {
    /// Program address.
    pub address: Pubkey,
    /// Human-readable name.
    pub name: String,
    /// Current upgrade authority.
    pub upgrade_authority: Option<Pubkey>,
    /// Slot when last deployed.
    pub last_deployed_slot: u64,
    /// Hash of program data.
    pub data_hash: String,
    /// Address of executable data (for upgradeable programs).
    pub executable_data_address: Option<Pubkey>,
    /// Whether the program is upgradeable.
    pub is_upgradeable: bool,
    /// When added to watch list.
    pub added_at: DateTime<Utc>,
    /// When last checked.
    pub last_checked_at: DateTime<Utc>,
}

impl WatchedProgram {
    /// Check if the program is immutable (no upgrade authority).
    pub fn is_immutable(&self) -> bool {
        !self.is_upgradeable || self.upgrade_authority.is_none()
    }
}

/// Program information fetched from chain.
#[derive(Debug, Clone)]
pub struct ProgramInfo {
    pub name: String,
    pub address: Pubkey,
    pub upgrade_authority: Option<Pubkey>,
    pub executable_data_address: Option<Pubkey>,
    pub is_upgradeable: bool,
    pub last_deployed_slot: u64,
    pub data_hash: String,
}

/// Events from program monitoring.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProgramEvent {
    /// Program was upgraded.
    ProgramUpgraded {
        program: Pubkey,
        name: String,
        old_slot: u64,
        new_slot: u64,
        old_hash: String,
        new_hash: String,
        timestamp: DateTime<Utc>,
    },

    /// Upgrade authority was changed.
    UpgradeAuthorityChanged {
        program: Pubkey,
        name: String,
        old_authority: Option<Pubkey>,
        new_authority: Option<Pubkey>,
        timestamp: DateTime<Utc>,
    },

    /// Program was set to immutable.
    ImmutableSet {
        program: Pubkey,
        name: String,
        timestamp: DateTime<Utc>,
    },

    /// Upgrade buffer created (potential upgrade incoming).
    BufferCreated {
        program: Pubkey,
        buffer: Pubkey,
        authority: Pubkey,
        timestamp: DateTime<Utc>,
    },
}

impl ProgramEvent {
    /// Get the program ID for this event.
    pub fn program_id(&self) -> Pubkey {
        match self {
            Self::ProgramUpgraded { program, .. } => *program,
            Self::UpgradeAuthorityChanged { program, .. } => *program,
            Self::ImmutableSet { program, .. } => *program,
            Self::BufferCreated { program, .. } => *program,
        }
    }

    /// Get the event timestamp.
    pub fn timestamp(&self) -> DateTime<Utc> {
        match self {
            Self::ProgramUpgraded { timestamp, .. } => *timestamp,
            Self::UpgradeAuthorityChanged { timestamp, .. } => *timestamp,
            Self::ImmutableSet { timestamp, .. } => *timestamp,
            Self::BufferCreated { timestamp, .. } => *timestamp,
        }
    }

    /// Get event type as string.
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::ProgramUpgraded { .. } => "program_upgraded",
            Self::UpgradeAuthorityChanged { .. } => "authority_changed",
            Self::ImmutableSet { .. } => "immutable_set",
            Self::BufferCreated { .. } => "buffer_created",
        }
    }
}

/// Calculate a simple hash of data for comparison.
fn calculate_hash(data: &[u8]) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    data.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_hash() {
        let data1 = b"hello world";
        let data2 = b"hello world";
        let data3 = b"different data";

        assert_eq!(calculate_hash(data1), calculate_hash(data2));
        assert_ne!(calculate_hash(data1), calculate_hash(data3));
    }

    #[test]
    fn test_watched_program_immutable() {
        let mut program = WatchedProgram {
            address: Pubkey::new_unique(),
            name: "Test".to_string(),
            upgrade_authority: Some(Pubkey::new_unique()),
            last_deployed_slot: 100,
            data_hash: "abc123".to_string(),
            executable_data_address: None,
            is_upgradeable: true,
            added_at: Utc::now(),
            last_checked_at: Utc::now(),
        };

        assert!(!program.is_immutable());

        program.upgrade_authority = None;
        assert!(program.is_immutable());
    }

    #[test]
    fn test_program_event_type() {
        let event = ProgramEvent::ProgramUpgraded {
            program: Pubkey::new_unique(),
            name: "Test".to_string(),
            old_slot: 100,
            new_slot: 200,
            old_hash: "abc".to_string(),
            new_hash: "def".to_string(),
            timestamp: Utc::now(),
        };

        assert_eq!(event.event_type(), "program_upgraded");
    }
}
