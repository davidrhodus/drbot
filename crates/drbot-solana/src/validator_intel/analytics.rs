//! Higher-level analytics ported from the `validator-intel` reference project.
//!
//! This includes:
//! - Validator `ClientType` classification heuristics based on version strings
//! - A compact `ValidatorInfo` view (similar to `/api/v1/validators`)
//! - Network overview computation (similar to dashboard)
//! - Static research datasets (block quality + SFDP overview)

use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;
use std::{collections::HashMap, str::FromStr};

/// Validator client type (ported from the reference `validator-intel` project).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ClientType {
    #[serde(rename = "Harmonix")]
    Harmonix,
    #[serde(rename = "Jito Classic")]
    JitoClassic,
    #[serde(rename = "Jito BAM")]
    JitoBam,
    #[serde(rename = "Agave 3.0")]
    Agave3_0,
    #[serde(rename = "Agave 3.1")]
    Agave3_1,
    #[serde(rename = "Firedancer")]
    Firedancer,
    #[serde(rename = "v4.0.0 Unknown")]
    V4_0_0Unknown,
    #[serde(rename = "Unknown")]
    Unknown,
}

impl ClientType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Harmonix => "Harmonix",
            Self::JitoClassic => "Jito Classic",
            Self::JitoBam => "Jito BAM",
            Self::Agave3_0 => "Agave 3.0",
            Self::Agave3_1 => "Agave 3.1",
            Self::Firedancer => "Firedancer",
            Self::V4_0_0Unknown => "v4.0.0 Unknown",
            Self::Unknown => "Unknown",
        }
    }
}

impl std::fmt::Display for ClientType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

lazy_static! {
    /// Known Harmonix validator identities (identity pubkey -> name).
    static ref KNOWN_HARMONIX_VALIDATORS: HashMap<Pubkey, &'static str> = {
        let mut m = HashMap::new();
        m.insert(
            Pubkey::from_str("beefKGBWeSpHzYBHZXwp5So7wdQGX6mu4ZHDsGq7dTg").unwrap(),
            "Coinbase",
        );
        m.insert(
            Pubkey::from_str("23XqhxChUQBHi1BEEqkW21v8MvDfKoLfpWRFyzYCG8zT").unwrap(),
            "23Xqhx",
        );
        m.insert(
            Pubkey::from_str("CaveybfMBL6J5YNydRWmk3T6WTUhHDDShiSsaf3Bc2T8").unwrap(),
            "Cavey",
        );
        m
    };

    /// Static Harmonix stake estimates (identity pubkey -> stake in SOL).
    static ref HARMONIX_STAKES_SOL: HashMap<Pubkey, u64> = {
        let mut m = HashMap::new();
        m.insert(
            Pubkey::from_str("beefKGBWeSpHzYBHZXwp5So7wdQGX6mu4ZHDsGq7dTg").unwrap(),
            3_000_000,
        );
        m.insert(
            Pubkey::from_str("23XqhxChUQBHi1BEEqkW21v8MvDfKoLfpWRFyzYCG8zT").unwrap(),
            1_080_000,
        );
        m.insert(
            Pubkey::from_str("CaveybfMBL6J5YNydRWmk3T6WTUhHDDShiSsaf3Bc2T8").unwrap(),
            875_000,
        );
        m
    };
}

/// If the identity is a known Harmonix validator, return its display name.
pub fn harmonix_name(identity: &Pubkey) -> Option<&'static str> {
    KNOWN_HARMONIX_VALIDATORS.get(identity).copied()
}

/// Static Harmonix stake estimate (SOL) for known Harmonix validators.
pub fn harmonix_stake_sol(identity: &Pubkey) -> Option<u64> {
    HARMONIX_STAKES_SOL.get(identity).copied()
}

/// Client classification heuristic (ported from the reference `validator-intel` project).
pub fn classify_client(version: Option<&str>, identity: &Pubkey) -> ClientType {
    if harmonix_name(identity).is_some() {
        return ClientType::Harmonix;
    }

    let Some(version) = version else {
        return ClientType::Unknown;
    };

    // Firedancer detection
    if version.contains("fd_") || version.contains("firedancer") || version.starts_with("0.") {
        return ClientType::Firedancer;
    }

    // v4.0.0 unknown client
    if version == "4.0.0" || version.starts_with("4.0.") {
        return ClientType::V4_0_0Unknown;
    }

    // Agave versioning and Jito heuristics
    if version.starts_with("2.1") || version.starts_with("2.2") {
        let patch: u64 = version
            .split('.')
            .nth(2)
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);
        if (10..=20).contains(&patch) {
            return ClientType::JitoBam;
        }
        if (4..10).contains(&patch) {
            return ClientType::JitoClassic;
        }
        return ClientType::Agave3_1;
    }

    if version.starts_with("2.0") || version.starts_with("1.18") {
        return ClientType::Agave3_0;
    }

    if version.starts_with("2.") {
        return ClientType::Agave3_1;
    }

    ClientType::Unknown
}

/// Compact validator view similar to `/api/v1/validators` in the reference project.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidatorInfo {
    pub identity_pubkey: Pubkey,
    pub vote_account_pubkey: Pubkey,
    /// Activated stake in SOL.
    pub activated_stake: f64,
    pub commission: u8,
    pub epoch_vote_account: bool,
    pub version: Option<String>,
    pub client_type: ClientType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_vote: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root_slot: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delinquent: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
}

/// Convert a full [`super::types::ValidatorIntel`] record into a compact [`ValidatorInfo`].
pub fn validator_info_from_intel(intel: &super::types::ValidatorIntel) -> Option<ValidatorInfo> {
    let vote = intel.vote.as_ref()?;
    let version = intel.node.as_ref().and_then(|n| n.version.clone());

    Some(ValidatorInfo {
        identity_pubkey: intel.identity,
        vote_account_pubkey: vote.vote_pubkey,
        activated_stake: vote.activated_stake_sol,
        commission: vote.commission,
        epoch_vote_account: vote.epoch_vote_account,
        version: version.clone(),
        client_type: classify_client(version.as_deref(), &intel.identity),
        last_vote: Some(vote.last_vote),
        root_slot: Some(vote.root_slot),
        delinquent: Some(vote.delinquent),
        name: harmonix_name(&intel.identity).map(|s| s.to_string()),
        country: None,
    })
}

/// Client distribution item (count + stake).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientDistribution {
    pub client_type: ClientType,
    pub count: usize,
    pub stake: f64,
    pub percentage: f64,
}

/// Network overview for a given snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkOverview {
    pub total_validators: usize,
    pub total_stake: f64,
    pub nakamoto_coefficient: usize,
    pub entity_nakamoto: usize,
    pub current_epoch: u64,
    pub current_slot: u64,
    pub epoch_progress: f64,
    pub client_distribution: Vec<ClientDistribution>,
}

/// Static block quality metrics (research dataset).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockQualityMetrics {
    pub client_type: ClientType,
    pub avg_txs_per_block: f64,
    pub success_rate: f64,
    pub avg_fees_per_slot: f64,
    pub jito_tip_count: f64,
    pub user_tx_ratio: f64,
    pub sandwich_count: u64,
    pub sample_size: u64,
}

/// Return the static block quality comparison dataset.
pub fn block_quality_comparison() -> Vec<BlockQualityMetrics> {
    vec![
        BlockQualityMetrics {
            client_type: ClientType::JitoClassic,
            avg_txs_per_block: 1847.0,
            success_rate: 87.7,
            avg_fees_per_slot: 0.019,
            jito_tip_count: 57.6,
            user_tx_ratio: 62.3,
            sandwich_count: 4,
            sample_size: 500,
        },
        BlockQualityMetrics {
            client_type: ClientType::JitoBam,
            avg_txs_per_block: 2134.0,
            success_rate: 84.0,
            avg_fees_per_slot: 0.021,
            jito_tip_count: 114.0,
            user_tx_ratio: 58.7,
            sandwich_count: 4,
            sample_size: 500,
        },
        BlockQualityMetrics {
            client_type: ClientType::Harmonix,
            avg_txs_per_block: 1692.0,
            success_rate: 79.8,
            avg_fees_per_slot: 0.027,
            jito_tip_count: 61.3,
            user_tx_ratio: 71.2,
            sandwich_count: 0,
            sample_size: 200,
        },
        BlockQualityMetrics {
            client_type: ClientType::V4_0_0Unknown,
            avg_txs_per_block: 1523.0,
            success_rate: 82.1,
            avg_fees_per_slot: 0.040,
            jito_tip_count: 43.8,
            user_tx_ratio: 68.9,
            sandwich_count: 1,
            sample_size: 150,
        },
        BlockQualityMetrics {
            client_type: ClientType::Agave3_0,
            avg_txs_per_block: 1756.0,
            success_rate: 86.2,
            avg_fees_per_slot: 0.018,
            jito_tip_count: 52.1,
            user_tx_ratio: 60.8,
            sandwich_count: 3,
            sample_size: 500,
        },
        BlockQualityMetrics {
            client_type: ClientType::Firedancer,
            avg_txs_per_block: 2310.0,
            success_rate: 89.4,
            avg_fees_per_slot: 0.022,
            jito_tip_count: 68.2,
            user_tx_ratio: 64.1,
            sandwich_count: 2,
            sample_size: 300,
        },
    ]
}

/// SFDP stake pool overlap.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StakePoolOverlap {
    pub pool: String,
    pub overlap_percentage: f64,
    pub color: String,
}

/// SFDP country distribution.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CountryDistribution {
    pub country: String,
    pub count: u64,
    pub stake: u64,
}

/// SFDP overview response (static dataset).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SfdpOverview {
    pub summary: SfdpSummary,
    pub stake_pool_overlap: Vec<StakePoolOverlap>,
    pub top_countries: Vec<CountryDistribution>,
    pub key_findings: Vec<String>,
}

/// SFDP summary (static dataset).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SfdpSummary {
    pub total_validators: u64,
    pub total_stake: u64,
    pub total_stake_formatted: String,
    pub network_percentage: f64,
    pub countries: u64,
    pub questions_resolved: u64,
    pub total_questions: u64,
    pub resolution_rate: String,
}

/// Return the static SFDP overview dataset.
pub fn sfdp_overview() -> SfdpOverview {
    let total_validators = 454;
    let total_stake = 84_800_000;
    let network_percentage = 19.9;
    let countries = 29;
    let questions_resolved = 35;
    let total_questions = 38;
    let resolution_rate = format!(
        "{:.1}%",
        (questions_resolved as f64 / total_questions as f64) * 100.0
    );

    SfdpOverview {
        summary: SfdpSummary {
            total_validators,
            total_stake,
            total_stake_formatted: "84.8M SOL".to_string(),
            network_percentage,
            countries,
            questions_resolved,
            total_questions,
            resolution_rate,
        },
        stake_pool_overlap: vec![
            StakePoolOverlap {
                pool: "BlazeStake".to_string(),
                overlap_percentage: 76.0,
                color: "#FF6B35".to_string(),
            },
            StakePoolOverlap {
                pool: "JPool".to_string(),
                overlap_percentage: 43.8,
                color: "#00D4FF".to_string(),
            },
            StakePoolOverlap {
                pool: "Marinade".to_string(),
                overlap_percentage: 26.6,
                color: "#14F195".to_string(),
            },
            StakePoolOverlap {
                pool: "Sanctum".to_string(),
                overlap_percentage: 21.6,
                color: "#9945FF".to_string(),
            },
        ],
        top_countries: vec![
            CountryDistribution {
                country: "United States".to_string(),
                count: 87,
                stake: 18_500_000,
            },
            CountryDistribution {
                country: "Germany".to_string(),
                count: 52,
                stake: 11_200_000,
            },
            CountryDistribution {
                country: "Netherlands".to_string(),
                count: 38,
                stake: 8_900_000,
            },
            CountryDistribution {
                country: "Finland".to_string(),
                count: 31,
                stake: 7_200_000,
            },
            CountryDistribution {
                country: "France".to_string(),
                count: 28,
                stake: 6_100_000,
            },
            CountryDistribution {
                country: "United Kingdom".to_string(),
                count: 24,
                stake: 5_800_000,
            },
            CountryDistribution {
                country: "Canada".to_string(),
                count: 22,
                stake: 4_200_000,
            },
            CountryDistribution {
                country: "Japan".to_string(),
                count: 19,
                stake: 3_900_000,
            },
            CountryDistribution {
                country: "Singapore".to_string(),
                count: 17,
                stake: 3_100_000,
            },
            CountryDistribution {
                country: "Switzerland".to_string(),
                count: 15,
                stake: 2_800_000,
            },
        ],
        key_findings: vec![
            "SFDP controls 19.9% of total network stake through 454 matched validators".to_string(),
            "Geographic spread across 29 countries".to_string(),
            "Entity-level Nakamoto coefficient ~15 (validator-level: 19)".to_string(),
            "BlazeStake has 76% overlap with SFDP — highest concentration risk".to_string(),
            "JPool at 43.8% overlap, Marinade at 26.6%, Sanctum at 21.6%".to_string(),
            "35 of 38 research questions resolved (92.1%)".to_string(),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_harmonix_by_identity() {
        let identity = Pubkey::from_str("beefKGBWeSpHzYBHZXwp5So7wdQGX6mu4ZHDsGq7dTg").unwrap();
        assert_eq!(
            classify_client(Some("2.2.10"), &identity),
            ClientType::Harmonix
        );
    }

    #[test]
    fn test_classify_jito_patch_heuristics() {
        let identity = Pubkey::new_unique();
        assert_eq!(
            classify_client(Some("2.2.8"), &identity),
            ClientType::JitoClassic
        );
        assert_eq!(
            classify_client(Some("2.2.10"), &identity),
            ClientType::JitoBam
        );
        assert_eq!(
            classify_client(Some("2.2.30"), &identity),
            ClientType::Agave3_1
        );
    }

    #[test]
    fn test_block_quality_has_expected_clients() {
        let data = block_quality_comparison();
        assert!(data.iter().any(|m| m.client_type == ClientType::Harmonix));
        assert!(data
            .iter()
            .any(|m| m.client_type == ClientType::JitoClassic));
    }

    #[test]
    fn test_sfdp_overview_resolution_rate_format() {
        let overview = sfdp_overview();
        assert!(overview.summary.resolution_rate.ends_with('%'));
    }
}
