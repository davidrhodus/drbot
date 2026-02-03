//! Types for Solana validator intelligence.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;

/// Options controlling what data is fetched and how it is post-processed.
#[derive(Debug, Clone, Copy)]
pub struct ValidatorIntelOptions {
    /// Include delinquent vote accounts.
    pub include_delinquent: bool,
    /// Fetch block production and compute skip-rate.
    pub with_performance: bool,
    /// Compute a heuristic score.
    pub compute_scores: bool,
}

impl Default for ValidatorIntelOptions {
    fn default() -> Self {
        Self {
            include_delinquent: false,
            with_performance: false,
            compute_scores: true,
        }
    }
}

/// Snapshot of validator intel at a point in time.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidatorIntelSnapshot {
    /// When the snapshot was fetched.
    pub fetched_at: DateTime<Utc>,
    /// Total activated stake (lamports) across the returned validators.
    pub total_stake_lamports: u64,
    /// Validators.
    pub validators: Vec<ValidatorIntel>,
}

/// Epoch credits entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EpochCredits {
    pub epoch: u64,
    pub credits: u64,
    pub previous_credits: u64,
    pub delta: u64,
}

/// Vote-account-derived validator information.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidatorVoteInfo {
    pub vote_pubkey: Pubkey,
    pub node_pubkey: Pubkey,
    pub activated_stake_lamports: u64,
    pub activated_stake_sol: f64,
    pub commission: u8,
    pub epoch_vote_account: bool,
    pub epoch_credits: Vec<EpochCredits>,
    pub last_vote: u64,
    pub root_slot: u64,
    pub delinquent: bool,
}

/// Node contact info (from `getClusterNodes`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidatorNodeInfo {
    pub gossip: Option<String>,
    pub tvu: Option<String>,
    pub tpu: Option<String>,
    pub tpu_quic: Option<String>,
    pub tpu_forwards: Option<String>,
    pub tpu_forwards_quic: Option<String>,
    pub tpu_vote: Option<String>,
    pub serve_repair: Option<String>,
    pub rpc: Option<String>,
    pub pubsub: Option<String>,
    pub version: Option<String>,
    pub feature_set: Option<u32>,
    pub shred_version: Option<u16>,
}

/// Block production derived performance statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidatorPerformance {
    pub leader_slots: u64,
    pub blocks_produced: u64,
    pub skipped_slots: u64,
    pub skip_rate: f64,
}

/// Combined validator intelligence record.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidatorIntel {
    /// Validator identity pubkey.
    pub identity: Pubkey,
    /// Vote-account intel (if the node is a validator).
    pub vote: Option<ValidatorVoteInfo>,
    /// Cluster contact info (if available via RPC).
    pub node: Option<ValidatorNodeInfo>,
    /// Optional block-production performance stats.
    pub performance: Option<ValidatorPerformance>,
    /// Activated stake as a percent of `total_stake_lamports` for the snapshot.
    pub stake_percent: Option<f64>,
    /// Heuristic score (0-100).
    pub score: Option<f64>,
}

impl ValidatorIntel {
    pub(crate) fn new(identity: Pubkey) -> Self {
        Self {
            identity,
            vote: None,
            node: None,
            performance: None,
            stake_percent: None,
            score: None,
        }
    }
}
