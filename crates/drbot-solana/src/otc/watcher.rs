//! Persistence + background watchers for OTC settlements.
//!
//! For open-network OTC, it's important that a trader can safely recover from
//! crashes/restarts after funding Party A, and still:
//! - observe settlement (receipt status becomes `Settled`)
//! - auto-cancel after expiry if Party B never funds (receipt stays `Open`)
//!
//! This module provides a lightweight on-disk store of "active escrows" plus
//! a polling watcher that can auto-cancel after expiry.

use super::escrow::EscrowManager;
use crate::{Result, SolanaError};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signer::keypair::Keypair;
use solana_sdk::signer::Signer;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::Notify;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};
use uuid::Uuid;

const DEFAULT_WATCH_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);

/// A single escrow to watch for settlement / expiry.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OtcSettlementWatch {
    pub negotiation_id: Uuid,
    pub party_a: Pubkey,
    pub party_b: Pubkey,
    pub escrow_address: Pubkey,
    pub expiry_unix_ts: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl OtcSettlementWatch {
    pub fn new(
        negotiation_id: Uuid,
        party_a: Pubkey,
        party_b: Pubkey,
        escrow_address: Pubkey,
        expiry_unix_ts: i64,
    ) -> Self {
        let now = Utc::now();
        Self {
            negotiation_id,
            party_a,
            party_b,
            escrow_address,
            expiry_unix_ts,
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone)]
struct WatchStorePersistence {
    path: PathBuf,
    dirty: Arc<AtomicBool>,
    notify: Arc<Notify>,
}

/// On-disk store of active settlement watches.
pub struct OtcSettlementWatchStore {
    watches: Arc<RwLock<HashMap<Uuid, OtcSettlementWatch>>>,
    persistence: OnceLock<WatchStorePersistence>,
}

impl OtcSettlementWatchStore {
    pub fn new() -> Self {
        Self {
            watches: Arc::new(RwLock::new(HashMap::new())),
            persistence: OnceLock::new(),
        }
    }

    /// Enable on-disk persistence (crash/restart safety).
    ///
    /// - Loads existing state from `path` (if it exists)
    /// - Spawns an autosave task that flushes whenever state changes
    pub async fn enable_persistence(
        self: &Arc<Self>,
        path: impl Into<PathBuf>,
        flush_interval: std::time::Duration,
    ) -> Result<JoinHandle<()>> {
        let path = path.into();

        let persistence = WatchStorePersistence {
            path: path.clone(),
            dirty: Arc::new(AtomicBool::new(false)),
            notify: Arc::new(Notify::new()),
        };

        if self.persistence.set(persistence).is_err() {
            return Err(SolanaError::ConfigError(
                "OTC settlement watch persistence already enabled".to_string(),
            ));
        }

        let _ = self.load_from_file(&path).await?;

        let store = self.clone();
        Ok(tokio::spawn(async move {
            store.run_autosave(flush_interval).await;
        }))
    }

    /// Best-effort load. Returns `Ok(true)` if a file was loaded, `Ok(false)` if missing.
    pub async fn load_from_file(&self, path: &Path) -> Result<bool> {
        let bytes = match tokio::fs::read(path).await {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(e) => return Err(e.into()),
        };

        let file: WatchStoreFile = serde_json::from_slice(&bytes)?;
        if file.version != WatchStoreFile::VERSION {
            return Err(SolanaError::OTCError(format!(
                "Unsupported settlement watch store version {}",
                file.version
            )));
        }

        let mut map = HashMap::new();
        for w in file.watches {
            map.insert(w.negotiation_id, w);
        }

        *self.watches.write().await = map;
        Ok(true)
    }

    pub async fn save_to_file(&self, path: &Path) -> Result<()> {
        let watches = self.watches.read().await;
        let mut list: Vec<OtcSettlementWatch> = watches.values().cloned().collect();
        list.sort_by_key(|w| w.negotiation_id);

        let file = WatchStoreFile {
            version: WatchStoreFile::VERSION,
            watches: list,
        };

        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let tmp_path = temp_path_for(path);
        tokio::fs::write(&tmp_path, serde_json::to_vec_pretty(&file)?).await?;
        if tokio::fs::rename(&tmp_path, path).await.is_err() {
            let _ = tokio::fs::remove_file(path).await;
            tokio::fs::rename(&tmp_path, path).await?;
        }

        Ok(())
    }

    /// Upsert (insert/update) a watch entry.
    pub async fn upsert(&self, mut watch: OtcSettlementWatch) {
        watch.updated_at = Utc::now();
        self.watches
            .write()
            .await
            .insert(watch.negotiation_id, watch);
        self.mark_dirty();
    }

    /// Remove a watch entry.
    pub async fn remove(&self, negotiation_id: Uuid) {
        self.watches.write().await.remove(&negotiation_id);
        self.mark_dirty();
    }

    /// Current snapshot of all watches.
    pub async fn list(&self) -> Vec<OtcSettlementWatch> {
        self.watches.read().await.values().cloned().collect()
    }

    /// Force a best-effort flush if persistence is enabled.
    pub async fn flush(&self) -> Result<()> {
        let Some(p) = self.persistence.get() else {
            return Ok(());
        };
        self.save_to_file(&p.path).await
    }

    fn mark_dirty(&self) {
        let Some(p) = self.persistence.get() else {
            return;
        };
        p.dirty.store(true, Ordering::Release);
        p.notify.notify_one();
    }

    async fn run_autosave(&self, flush_interval: std::time::Duration) {
        let Some(p) = self.persistence.get() else {
            return;
        };

        let mut tick = tokio::time::interval(flush_interval);
        loop {
            tokio::select! {
                _ = tick.tick() => {}
                _ = p.notify.notified() => {}
            }

            if !p.dirty.swap(false, Ordering::AcqRel) {
                continue;
            }

            if let Err(e) = self.save_to_file(&p.path).await {
                p.dirty.store(true, Ordering::Release);
                warn!(error = %e, path = %p.path.display(), "Failed to persist OTC settlement watches");
            }
        }
    }
}

impl Default for OtcSettlementWatchStore {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WatchStoreFile {
    version: u32,
    watches: Vec<OtcSettlementWatch>,
}

impl WatchStoreFile {
    const VERSION: u32 = 1;
}

fn temp_path_for(path: &Path) -> PathBuf {
    let file_name = path.file_name().unwrap_or_default().to_string_lossy();
    path.with_file_name(format!("{file_name}.tmp"))
}

/// Background watcher that:
/// - removes entries when receipt is Settled/Cancelled
/// - auto-cancels expired Open receipts to refund Party A
///
/// Notes:
/// - This watcher is designed for the **Party A funds first** flow (open network).
/// - It is safe to run multiple times; on-chain receipt status is authoritative.
pub fn spawn_otc_auto_cancel_watcher(
    store: Arc<OtcSettlementWatchStore>,
    escrow_manager: Arc<EscrowManager>,
    caller_keypair: Keypair,
    fee_payer_keypair: Option<Keypair>,
    poll_interval: Option<std::time::Duration>,
) -> JoinHandle<()> {
    let poll_interval = poll_interval.unwrap_or(DEFAULT_WATCH_POLL_INTERVAL);
    let caller_pubkey = caller_keypair.pubkey();
    let fee_payer_pubkey = fee_payer_keypair
        .as_ref()
        .map(|k| k.pubkey())
        .unwrap_or(caller_pubkey);

    tokio::spawn(async move {
        let mut tick = tokio::time::interval(poll_interval);
        loop {
            tick.tick().await;

            let watches = store.list().await;
            if watches.is_empty() {
                continue;
            }

            for w in watches {
                let status = match escrow_manager
                    .get_receipt_status(w.negotiation_id, w.party_a, w.party_b)
                    .await
                {
                    Ok(s) => s,
                    Err(e) => {
                        debug!(error = %e, negotiation_id = %w.negotiation_id, "receipt status read failed");
                        continue;
                    }
                };

                match status {
                    Some(drbot_otc_escrow_program::ReceiptStatus::Settled) => {
                        info!(negotiation_id = %w.negotiation_id, "settled (receipt)");
                        store.remove(w.negotiation_id).await;
                        let _ = store.flush().await;
                        continue;
                    }
                    Some(drbot_otc_escrow_program::ReceiptStatus::Cancelled) => {
                        info!(negotiation_id = %w.negotiation_id, "cancelled (receipt)");
                        store.remove(w.negotiation_id).await;
                        let _ = store.flush().await;
                        continue;
                    }
                    Some(drbot_otc_escrow_program::ReceiptStatus::Open) => {}
                    None => {
                        // No receipt: escrow not created or not our program; drop to avoid infinite loops.
                        warn!(negotiation_id = %w.negotiation_id, "missing receipt; dropping watch entry");
                        store.remove(w.negotiation_id).await;
                        let _ = store.flush().await;
                        continue;
                    }
                }

                let now_ts = Utc::now().timestamp();
                if now_ts <= w.expiry_unix_ts {
                    continue;
                }

                // Expired + Open: attempt cancel.
                info!(
                    negotiation_id = %w.negotiation_id,
                    escrow_address = %w.escrow_address,
                    caller = %caller_pubkey,
                    fee_payer = %fee_payer_pubkey,
                    "auto-cancel: attempting"
                );

                let cancel = match fee_payer_keypair.as_ref() {
                    Some(fee_payer) => {
                        escrow_manager
                            .cancel_with_fee_payer(fee_payer, &caller_keypair, w.escrow_address)
                            .await
                    }
                    None => escrow_manager.cancel(&caller_keypair, w.escrow_address).await,
                };

                match cancel {
                    Ok(sig) => {
                        info!(
                            negotiation_id = %w.negotiation_id,
                            signature = %sig,
                            "auto-cancel: submitted"
                        );
                    }
                    Err(e) => {
                        // Re-check receipt; could have settled/cancelled between our reads.
                        let status2 = escrow_manager
                            .get_receipt_status(w.negotiation_id, w.party_a, w.party_b)
                            .await
                            .ok()
                            .flatten();
                        debug!(
                            error = %e,
                            negotiation_id = %w.negotiation_id,
                            status = ?status2,
                            "auto-cancel failed"
                        );
                    }
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_watch_store_roundtrip() -> Result<()> {
        let dir = tempdir()?;
        let path = dir.path().join("store.json");

        let store = OtcSettlementWatchStore::new();
        let w = OtcSettlementWatch::new(
            Uuid::new_v4(),
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            123,
        );
        store.upsert(w.clone()).await;
        store.save_to_file(&path).await?;

        let store2 = OtcSettlementWatchStore::new();
        assert!(store2.load_from_file(&path).await?);
        let watches = store2.list().await;
        assert_eq!(watches.len(), 1);
        assert_eq!(watches[0].negotiation_id, w.negotiation_id);
        assert_eq!(watches[0].party_a, w.party_a);
        assert_eq!(watches[0].party_b, w.party_b);
        assert_eq!(watches[0].escrow_address, w.escrow_address);
        assert_eq!(watches[0].expiry_unix_ts, 123);
        Ok(())
    }
}

