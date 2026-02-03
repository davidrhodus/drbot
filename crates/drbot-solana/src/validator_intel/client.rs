//! RPC client for validator intelligence.

use super::analytics::{
    validator_info_from_intel, ClientDistribution, NetworkOverview, ValidatorInfo,
};
use super::scoring::score_validator;
use super::types::{
    EpochCredits, ValidatorIntel, ValidatorIntelOptions, ValidatorIntelSnapshot, ValidatorNodeInfo,
    ValidatorPerformance, ValidatorVoteInfo,
};
use crate::{Result, SolanaError};
use chrono::Utc;
use solana_client::{
    nonblocking::rpc_client::RpcClient,
    rpc_response::{RpcBlockProduction, RpcContactInfo, RpcVoteAccountInfo, RpcVoteAccountStatus},
};
use solana_sdk::{native_token::LAMPORTS_PER_SOL, pubkey::Pubkey};
use std::{collections::HashMap, str::FromStr, sync::Arc};

/// Validator intel client backed by a Solana RPC endpoint.
pub struct ValidatorIntelClient {
    rpc_client: Arc<RpcClient>,
}

impl ValidatorIntelClient {
    /// Create a new validator intel client.
    pub fn new(rpc_client: Arc<RpcClient>) -> Self {
        Self { rpc_client }
    }

    /// Fetch a snapshot of validator intelligence.
    pub async fn snapshot(&self, options: ValidatorIntelOptions) -> Result<ValidatorIntelSnapshot> {
        let fetched_at = Utc::now();

        let (vote_accounts, cluster_nodes, block_production) = if options.with_performance {
            let (vote_accounts, cluster_nodes, block_production) = tokio::try_join!(
                self.rpc_client.get_vote_accounts(),
                self.rpc_client.get_cluster_nodes(),
                self.rpc_client.get_block_production()
            )?;
            (vote_accounts, cluster_nodes, Some(block_production.value))
        } else {
            let (vote_accounts, cluster_nodes) = tokio::try_join!(
                self.rpc_client.get_vote_accounts(),
                self.rpc_client.get_cluster_nodes()
            )?;
            (vote_accounts, cluster_nodes, None)
        };

        let (validators, total_stake_lamports) =
            build_validator_intel(vote_accounts, cluster_nodes, block_production, options)?;

        Ok(ValidatorIntelSnapshot {
            fetched_at,
            total_stake_lamports,
            validators,
        })
    }

    /// List validators (vote accounts) with their joined intelligence fields.
    pub async fn list_validators(
        &self,
        options: ValidatorIntelOptions,
    ) -> Result<Vec<ValidatorIntel>> {
        Ok(self.snapshot(options).await?.validators)
    }

    /// Get a single validator by identity or vote-account pubkey.
    pub async fn get_validator(
        &self,
        identity_or_vote_pubkey: &str,
        mut options: ValidatorIntelOptions,
    ) -> Result<Option<ValidatorIntel>> {
        options.include_delinquent = true;

        let snapshot = self.snapshot(options).await?;

        let key = Pubkey::from_str(identity_or_vote_pubkey)
            .map_err(|_| SolanaError::InvalidPubkey(identity_or_vote_pubkey.to_string()))?;

        Ok(snapshot.validators.into_iter().find(|v| {
            if v.identity == key {
                return true;
            }
            v.vote.as_ref().is_some_and(|vote| vote.vote_pubkey == key)
        }))
    }

    /// Fetch validators in the compact `validator-intel` shape (client classification included).
    pub async fn fetch_validator_info(
        &self,
        include_delinquent: bool,
    ) -> Result<Vec<ValidatorInfo>> {
        let options = ValidatorIntelOptions {
            include_delinquent,
            with_performance: false,
            compute_scores: false,
        };

        let validators = self.list_validators(options).await?;
        let mut infos: Vec<ValidatorInfo> = validators
            .iter()
            .filter_map(validator_info_from_intel)
            .collect();

        infos.sort_by(|a, b| b.activated_stake.total_cmp(&a.activated_stake));
        Ok(infos)
    }

    /// Fetch a network overview (total stake, Nakamoto coefficient, epoch progress, client distribution).
    pub async fn fetch_network_overview(
        &self,
        include_delinquent: bool,
    ) -> Result<NetworkOverview> {
        let (validators, epoch_info) =
            tokio::try_join!(self.fetch_validator_info(include_delinquent), async {
                self.rpc_client
                    .get_epoch_info()
                    .await
                    .map_err(SolanaError::from)
            })?;

        let total_stake: f64 = validators.iter().map(|v| v.activated_stake).sum();

        let nakamoto_coefficient = if total_stake <= 0.0 {
            0
        } else {
            let mut cumulative = 0.0;
            let mut nakamoto = 0usize;
            for v in &validators {
                cumulative += v.activated_stake;
                nakamoto += 1;
                if cumulative > total_stake / 3.0 {
                    break;
                }
            }
            nakamoto
        };

        let mut client_map: HashMap<super::analytics::ClientType, (usize, f64)> = HashMap::new();
        for v in &validators {
            let entry = client_map.entry(v.client_type).or_insert((0, 0.0));
            entry.0 += 1;
            entry.1 += v.activated_stake;
        }

        let total_count = validators.len().max(1) as f64;
        let mut client_distribution: Vec<ClientDistribution> = client_map
            .into_iter()
            .map(|(client_type, (count, stake))| ClientDistribution {
                client_type,
                count,
                stake,
                percentage: (count as f64 / total_count) * 100.0,
            })
            .collect();

        client_distribution.sort_by(|a, b| b.stake.total_cmp(&a.stake));

        let epoch_progress = if epoch_info.slots_in_epoch == 0 {
            0.0
        } else {
            (epoch_info.slot_index as f64 / epoch_info.slots_in_epoch as f64) * 100.0
        };

        Ok(NetworkOverview {
            total_validators: validators.len(),
            total_stake,
            nakamoto_coefficient,
            entity_nakamoto: 15,
            current_epoch: epoch_info.epoch,
            current_slot: epoch_info.absolute_slot,
            epoch_progress,
            client_distribution,
        })
    }
}

fn to_addr(opt: Option<std::net::SocketAddr>) -> Option<String> {
    opt.map(|a| a.to_string())
}

fn node_info_from_rpc(node: RpcContactInfo) -> ValidatorNodeInfo {
    ValidatorNodeInfo {
        gossip: to_addr(node.gossip),
        tvu: to_addr(node.tvu),
        tpu: to_addr(node.tpu),
        tpu_quic: to_addr(node.tpu_quic),
        tpu_forwards: to_addr(node.tpu_forwards),
        tpu_forwards_quic: to_addr(node.tpu_forwards_quic),
        tpu_vote: to_addr(node.tpu_vote),
        serve_repair: to_addr(node.serve_repair),
        rpc: to_addr(node.rpc),
        pubsub: to_addr(node.pubsub),
        version: node.version,
        feature_set: node.feature_set,
        shred_version: node.shred_version,
    }
}

fn vote_info_from_rpc(vote: RpcVoteAccountInfo, delinquent: bool) -> Result<ValidatorVoteInfo> {
    let vote_pubkey = Pubkey::from_str(&vote.vote_pubkey)
        .map_err(|_| SolanaError::InvalidPubkey(vote.vote_pubkey.clone()))?;
    let node_pubkey = Pubkey::from_str(&vote.node_pubkey)
        .map_err(|_| SolanaError::InvalidPubkey(vote.node_pubkey.clone()))?;

    let epoch_credits = vote
        .epoch_credits
        .into_iter()
        .map(|(epoch, credits, previous_credits)| EpochCredits {
            epoch: epoch as u64,
            credits,
            previous_credits,
            delta: credits.saturating_sub(previous_credits),
        })
        .collect();

    Ok(ValidatorVoteInfo {
        vote_pubkey,
        node_pubkey,
        activated_stake_lamports: vote.activated_stake,
        activated_stake_sol: vote.activated_stake as f64 / LAMPORTS_PER_SOL as f64,
        commission: vote.commission,
        epoch_vote_account: vote.epoch_vote_account,
        epoch_credits,
        last_vote: vote.last_vote,
        root_slot: vote.root_slot,
        delinquent,
    })
}

fn performance_from_rpc(leader_slots: usize, blocks_produced: usize) -> ValidatorPerformance {
    let leader_slots_u64 = leader_slots as u64;
    let produced_u64 = blocks_produced as u64;
    let skipped = leader_slots_u64.saturating_sub(produced_u64);
    let skip_rate = if leader_slots_u64 == 0 {
        0.0
    } else {
        skipped as f64 / leader_slots_u64 as f64
    };

    ValidatorPerformance {
        leader_slots: leader_slots_u64,
        blocks_produced: produced_u64,
        skipped_slots: skipped,
        skip_rate,
    }
}

pub(crate) fn build_validator_intel(
    vote_accounts: RpcVoteAccountStatus,
    cluster_nodes: Vec<RpcContactInfo>,
    block_production: Option<RpcBlockProduction>,
    options: ValidatorIntelOptions,
) -> Result<(Vec<ValidatorIntel>, u64)> {
    let mut by_identity: HashMap<Pubkey, ValidatorIntel> = HashMap::new();

    // Cluster nodes (contact info)
    for node in cluster_nodes {
        let Ok(identity) = Pubkey::from_str(&node.pubkey) else {
            continue;
        };
        let entry = by_identity
            .entry(identity)
            .or_insert_with(|| ValidatorIntel::new(identity));
        entry.node = Some(node_info_from_rpc(node));
    }

    // Vote accounts (current + delinquent). For rare cases where an identity has multiple vote
    // accounts, prefer the one with the highest activated stake.
    for vote in vote_accounts.current {
        upsert_vote(&mut by_identity, vote, false)?;
    }
    for vote in vote_accounts.delinquent {
        upsert_vote(&mut by_identity, vote, true)?;
    }

    // Optional performance stats
    if let Some(production) = block_production {
        for (identity_str, (leader_slots, blocks_produced)) in production.by_identity {
            let Ok(identity) = Pubkey::from_str(&identity_str) else {
                continue;
            };
            let Some(entry) = by_identity.get_mut(&identity) else {
                continue;
            };
            entry.performance = Some(performance_from_rpc(leader_slots, blocks_produced));
        }
    }

    // Keep only identities that are validators (have a vote account).
    let mut validators: Vec<ValidatorIntel> = by_identity
        .into_values()
        .filter(|v| v.vote.is_some())
        .collect();

    if !options.include_delinquent {
        validators.retain(|v| v.vote.as_ref().is_some_and(|vote| !vote.delinquent));
    }

    let total_stake_lamports: u64 = validators
        .iter()
        .filter_map(|v| v.vote.as_ref().map(|vote| vote.activated_stake_lamports))
        .sum();

    for v in validators.iter_mut() {
        if let Some(vote) = v.vote.as_ref() {
            v.stake_percent = Some(if total_stake_lamports == 0 {
                0.0
            } else {
                (vote.activated_stake_lamports as f64 / total_stake_lamports as f64) * 100.0
            });
        }

        if options.compute_scores {
            v.score = Some(score_validator(v));
        }
    }

    Ok((validators, total_stake_lamports))
}

fn upsert_vote(
    by_identity: &mut HashMap<Pubkey, ValidatorIntel>,
    vote: RpcVoteAccountInfo,
    delinquent: bool,
) -> Result<()> {
    let vote_info = vote_info_from_rpc(vote, delinquent)?;
    let identity = vote_info.node_pubkey;

    let entry = by_identity
        .entry(identity)
        .or_insert_with(|| ValidatorIntel::new(identity));

    let should_replace = match entry.vote.as_ref() {
        None => true,
        Some(existing) => vote_info.activated_stake_lamports > existing.activated_stake_lamports,
    };

    if should_replace {
        entry.vote = Some(vote_info);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_client::rpc_response::RpcBlockProductionRange;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    fn rpc_node(identity: Pubkey) -> RpcContactInfo {
        RpcContactInfo {
            pubkey: identity.to_string(),
            gossip: Some(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8001)),
            tvu: None,
            tpu: Some(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8003)),
            tpu_quic: None,
            tpu_forwards: None,
            tpu_forwards_quic: None,
            tpu_vote: None,
            serve_repair: None,
            rpc: None,
            pubsub: None,
            version: Some("2.3.13".to_string()),
            feature_set: Some(123),
            shred_version: Some(5000),
        }
    }

    fn rpc_vote(
        identity: Pubkey,
        vote_pubkey: Pubkey,
        stake_sol: u64,
        delinquent: bool,
    ) -> (RpcVoteAccountInfo, bool) {
        (
            RpcVoteAccountInfo {
                vote_pubkey: vote_pubkey.to_string(),
                node_pubkey: identity.to_string(),
                activated_stake: stake_sol * LAMPORTS_PER_SOL,
                commission: 5,
                epoch_vote_account: true,
                epoch_credits: vec![(0, 100, 50)],
                last_vote: 123,
                root_slot: 120,
            },
            delinquent,
        )
    }

    #[test]
    fn test_build_merges_vote_and_node() {
        let identity = Pubkey::new_unique();
        let vote_pk = Pubkey::new_unique();

        let (vote_info, _) = rpc_vote(identity, vote_pk, 10, false);
        let vote_accounts = RpcVoteAccountStatus {
            current: vec![vote_info],
            delinquent: vec![],
        };

        let cluster_nodes = vec![rpc_node(identity)];

        let options = ValidatorIntelOptions {
            include_delinquent: true,
            with_performance: false,
            compute_scores: true,
        };

        let (validators, total_stake) =
            build_validator_intel(vote_accounts, cluster_nodes, None, options).unwrap();

        assert_eq!(validators.len(), 1);
        assert_eq!(total_stake, 10 * LAMPORTS_PER_SOL);
        assert_eq!(validators[0].identity, identity);
        assert!(validators[0].vote.is_some());
        assert!(validators[0].node.is_some());
        assert!(validators[0].score.is_some());
    }

    #[test]
    fn test_build_prefers_highest_stake_vote_account() {
        let identity = Pubkey::new_unique();

        let (v1, _) = rpc_vote(identity, Pubkey::new_unique(), 5, false);
        let (v2, _) = rpc_vote(identity, Pubkey::new_unique(), 20, false);

        let vote_accounts = RpcVoteAccountStatus {
            current: vec![v1, v2],
            delinquent: vec![],
        };

        let options = ValidatorIntelOptions {
            include_delinquent: true,
            with_performance: false,
            compute_scores: false,
        };

        let (validators, total_stake) =
            build_validator_intel(vote_accounts, vec![], None, options).unwrap();

        assert_eq!(validators.len(), 1);
        assert_eq!(total_stake, 20 * LAMPORTS_PER_SOL);
        let vote = validators[0].vote.as_ref().unwrap();
        assert_eq!(vote.activated_stake_lamports, 20 * LAMPORTS_PER_SOL);
    }

    #[test]
    fn test_build_filters_delinquent_by_default() {
        let identity = Pubkey::new_unique();
        let (vote_info, _) = rpc_vote(identity, Pubkey::new_unique(), 10, true);

        let vote_accounts = RpcVoteAccountStatus {
            current: vec![],
            delinquent: vec![vote_info],
        };

        let options = ValidatorIntelOptions {
            include_delinquent: false,
            with_performance: false,
            compute_scores: false,
        };

        let (validators, _total_stake) =
            build_validator_intel(vote_accounts, vec![], None, options).unwrap();

        assert!(validators.is_empty());
    }

    #[test]
    fn test_build_attaches_block_production() {
        let identity = Pubkey::new_unique();
        let (vote_info, _) = rpc_vote(identity, Pubkey::new_unique(), 10, false);

        let vote_accounts = RpcVoteAccountStatus {
            current: vec![vote_info],
            delinquent: vec![],
        };

        let mut by_identity = HashMap::new();
        by_identity.insert(identity.to_string(), (100usize, 95usize));

        let production = RpcBlockProduction {
            by_identity,
            range: RpcBlockProductionRange {
                first_slot: 0,
                last_slot: 100,
            },
        };

        let options = ValidatorIntelOptions {
            include_delinquent: true,
            with_performance: true,
            compute_scores: false,
        };

        let (validators, _total_stake) =
            build_validator_intel(vote_accounts, vec![], Some(production), options).unwrap();

        let perf = validators[0].performance.as_ref().unwrap();
        assert_eq!(perf.leader_slots, 100);
        assert_eq!(perf.blocks_produced, 95);
        assert!(perf.skip_rate > 0.0);
    }
}
