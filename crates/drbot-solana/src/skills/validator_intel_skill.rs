//! Validator intelligence skill.

use crate::{
    validator_intel::{
        block_quality_comparison, sfdp_overview, ValidatorIntelClient, ValidatorIntelOptions,
    },
    wallet::KnownValidators,
};
use async_trait::async_trait;
use drbot_skills::{
    ManifestCapability, ManifestInput, ManifestOutput, Skill, SkillContext, SkillInput,
    SkillManifest, SkillOutput,
};
use serde_json::Value;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use std::{str::FromStr, sync::Arc};

/// Validator intelligence skill.
pub struct ValidatorIntelSkill {
    manifest: SkillManifest,
    client: ValidatorIntelClient,
}

impl ValidatorIntelSkill {
    /// Create a new validator intel skill.
    pub fn new(rpc_client: Arc<RpcClient>) -> Self {
        let manifest = SkillManifest {
            name: "validator_intel".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            description: "Fetch and rank Solana validators using RPC-derived intel (stake, commission, delinquency, contact info, and optional performance)".to_string(),
            author: Some("drbot".to_string()),
            license: Some("MIT".to_string()),
            homepage: None,
            repository: None,
            tags: vec![
                "solana".to_string(),
                "validator".to_string(),
                "intel".to_string(),
            ],
            inputs: vec![
                ManifestInput {
                    name: "action".to_string(),
                    description: "Action: list, get, top, summary, validators, overview, blocks_compare, sfdp_overview".to_string(),
                    param_type: "string".to_string(),
                    required: true,
                    default: None,
                    pattern: None,
                    enum_values: vec![],
                },
                ManifestInput {
                    name: "validator".to_string(),
                    description: "Validator identity or vote-account pubkey; also supports known aliases (e.g., 'jito') for get".to_string(),
                    param_type: "string".to_string(),
                    required: false,
                    default: None,
                    pattern: None,
                    enum_values: vec![],
                },
                ManifestInput {
                    name: "client_type".to_string(),
                    description: "Filter for 'validators' action: client type (e.g., 'Harmonix', 'Jito Classic')".to_string(),
                    param_type: "string".to_string(),
                    required: false,
                    default: None,
                    pattern: None,
                    enum_values: vec![],
                },
                ManifestInput {
                    name: "include_delinquent".to_string(),
                    description: "Include delinquent validators in results (default: false)".to_string(),
                    param_type: "boolean".to_string(),
                    required: false,
                    default: Some(serde_json::json!(false)),
                    pattern: None,
                    enum_values: vec![],
                },
                ManifestInput {
                    name: "with_performance".to_string(),
                    description: "Fetch block production and compute skip rate (default: false)".to_string(),
                    param_type: "boolean".to_string(),
                    required: false,
                    default: Some(serde_json::json!(false)),
                    pattern: None,
                    enum_values: vec![],
                },
                ManifestInput {
                    name: "max_commission".to_string(),
                    description: "Filter: maximum commission percent (0-100)".to_string(),
                    param_type: "number".to_string(),
                    required: false,
                    default: None,
                    pattern: None,
                    enum_values: vec![],
                },
                ManifestInput {
                    name: "min_stake_sol".to_string(),
                    description: "Filter: minimum activated stake (SOL)".to_string(),
                    param_type: "number".to_string(),
                    required: false,
                    default: None,
                    pattern: None,
                    enum_values: vec![],
                },
                ManifestInput {
                    name: "sort".to_string(),
                    description: "Sort: stake_desc, commission_asc, score_desc, skip_rate_asc".to_string(),
                    param_type: "string".to_string(),
                    required: false,
                    default: Some(serde_json::json!("stake_desc")),
                    pattern: None,
                    enum_values: vec![],
                },
                ManifestInput {
                    name: "limit".to_string(),
                    description: "Limit number of results".to_string(),
                    param_type: "number".to_string(),
                    required: false,
                    default: None,
                    pattern: None,
                    enum_values: vec![],
                },
                ManifestInput {
                    name: "offset".to_string(),
                    description: "Offset for pagination (validators action)".to_string(),
                    param_type: "number".to_string(),
                    required: false,
                    default: None,
                    pattern: None,
                    enum_values: vec![],
                },
            ],
            outputs: vec![ManifestOutput {
                name: "result".to_string(),
                description: "Operation result".to_string(),
                output_type: "object".to_string(),
            }],
            capabilities: vec![
                ManifestCapability::required("blockchain"),
                ManifestCapability::required("monitoring"),
            ],
            entry_point: None,
            runtime: None,
        };

        Self {
            manifest,
            client: ValidatorIntelClient::new(rpc_client),
        }
    }
}

#[async_trait]
impl Skill for ValidatorIntelSkill {
    fn manifest(&self) -> &SkillManifest {
        &self.manifest
    }

    fn validate_input(&self, input: &SkillInput) -> drbot_skills::Result<()> {
        let action = input
            .params
            .get("action")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                drbot_skills::SkillError::ValidationFailed("Missing action".to_string())
            })?;

        match action {
            "list" | "summary" | "top" | "validators" | "overview" | "blocks_compare"
            | "sfdp_overview" => Ok(()),
            "get" => {
                if input
                    .params
                    .get("validator")
                    .and_then(|v| v.as_str())
                    .is_none()
                {
                    return Err(drbot_skills::SkillError::ValidationFailed(
                        "Missing validator (identity/vote pubkey or alias)".to_string(),
                    ));
                }
                Ok(())
            }
            _ => Err(drbot_skills::SkillError::ValidationFailed(format!(
                "Unknown action: {}",
                action
            ))),
        }
    }

    async fn execute(
        &self,
        input: SkillInput,
        _context: &SkillContext,
    ) -> drbot_skills::Result<SkillOutput> {
        let action = input
            .params
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("list");

        match action {
            "list" => self.handle_list(&input).await,
            "get" => self.handle_get(&input).await,
            "top" => self.handle_top(&input).await,
            "summary" => self.handle_summary(&input).await,
            "validators" => self.handle_validators(&input).await,
            "overview" => self.handle_overview(&input).await,
            "blocks_compare" => self.handle_blocks_compare().await,
            "sfdp_overview" => self.handle_sfdp_overview().await,
            _ => Err(drbot_skills::SkillError::ExecutionFailed(format!(
                "Action '{}' not implemented",
                action
            ))),
        }
    }
}

impl ValidatorIntelSkill {
    fn get_bool(
        params: &std::collections::HashMap<String, Value>,
        key: &str,
        default: bool,
    ) -> bool {
        params.get(key).and_then(|v| v.as_bool()).unwrap_or(default)
    }

    fn get_u8(params: &std::collections::HashMap<String, Value>, key: &str) -> Option<u8> {
        params.get(key).and_then(|v| {
            if let Some(u) = v.as_u64() {
                u8::try_from(u).ok()
            } else if let Some(f) = v.as_f64() {
                u8::try_from(f.round().max(0.0) as u64).ok()
            } else {
                None
            }
        })
    }

    fn get_f64(params: &std::collections::HashMap<String, Value>, key: &str) -> Option<f64> {
        params.get(key).and_then(|v| v.as_f64())
    }

    fn get_usize(params: &std::collections::HashMap<String, Value>, key: &str) -> Option<usize> {
        params.get(key).and_then(|v| v.as_u64()).map(|u| u as usize)
    }

    async fn handle_list(&self, input: &SkillInput) -> drbot_skills::Result<SkillOutput> {
        let params = &input.params;
        let include_delinquent = Self::get_bool(params, "include_delinquent", false);
        let sort = params
            .get("sort")
            .and_then(|v| v.as_str())
            .unwrap_or("stake_desc");
        let with_performance =
            Self::get_bool(params, "with_performance", false) || sort == "skip_rate_asc";
        let limit = Self::get_usize(params, "limit");
        let max_commission = Self::get_u8(params, "max_commission");
        let min_stake_sol = Self::get_f64(params, "min_stake_sol");

        let options = ValidatorIntelOptions {
            include_delinquent,
            with_performance,
            compute_scores: true,
        };

        let mut validators = self
            .client
            .list_validators(options)
            .await
            .map_err(|e| drbot_skills::SkillError::ExecutionFailed(e.to_string()))?;

        if let Some(max_commission) = max_commission {
            validators.retain(|v| {
                v.vote
                    .as_ref()
                    .is_some_and(|vote| vote.commission <= max_commission)
            });
        }
        if let Some(min_stake_sol) = min_stake_sol {
            validators.retain(|v| {
                v.vote
                    .as_ref()
                    .is_some_and(|vote| vote.activated_stake_sol >= min_stake_sol)
            });
        }

        match sort {
            "commission_asc" => {
                validators.sort_by(|a, b| {
                    let a_comm = a.vote.as_ref().map(|v| v.commission).unwrap_or(u8::MAX);
                    let b_comm = b.vote.as_ref().map(|v| v.commission).unwrap_or(u8::MAX);
                    a_comm.cmp(&b_comm)
                });
            }
            "score_desc" => {
                validators.sort_by(|a, b| {
                    let a_score = a.score.unwrap_or(0.0);
                    let b_score = b.score.unwrap_or(0.0);
                    b_score.total_cmp(&a_score)
                });
            }
            "skip_rate_asc" => {
                validators.sort_by(|a, b| {
                    let a_skip = a
                        .performance
                        .as_ref()
                        .map(|p| p.skip_rate)
                        .unwrap_or(f64::INFINITY);
                    let b_skip = b
                        .performance
                        .as_ref()
                        .map(|p| p.skip_rate)
                        .unwrap_or(f64::INFINITY);
                    a_skip.total_cmp(&b_skip)
                });
            }
            "stake_desc" | _ => {
                validators.sort_by(|a, b| {
                    let a_stake = a
                        .vote
                        .as_ref()
                        .map(|v| v.activated_stake_lamports)
                        .unwrap_or(0);
                    let b_stake = b
                        .vote
                        .as_ref()
                        .map(|v| v.activated_stake_lamports)
                        .unwrap_or(0);
                    b_stake.cmp(&a_stake)
                });
            }
        }

        if let Some(limit) = limit {
            validators.truncate(limit);
        }

        Ok(SkillOutput::new(serde_json::to_value(validators).map_err(
            |e| drbot_skills::SkillError::ExecutionFailed(e.to_string()),
        )?))
    }

    async fn handle_get(&self, input: &SkillInput) -> drbot_skills::Result<SkillOutput> {
        let validator_str = input
            .params
            .get("validator")
            .and_then(|v| v.as_str())
            .unwrap();

        let sort = input
            .params
            .get("sort")
            .and_then(|v| v.as_str())
            .unwrap_or("stake_desc");
        let with_performance =
            Self::get_bool(&input.params, "with_performance", false) || sort == "skip_rate_asc";

        let query_pubkey = match Pubkey::from_str(validator_str) {
            Ok(pk) => pk,
            Err(_) => KnownValidators::default()
                .resolve(validator_str)
                .map_err(|_| {
                    drbot_skills::SkillError::ValidationFailed(format!(
                        "Invalid validator pubkey or unknown alias: {}",
                        validator_str
                    ))
                })?,
        };

        let options = ValidatorIntelOptions {
            include_delinquent: true,
            with_performance,
            compute_scores: true,
        };

        let found = self
            .client
            .get_validator(&query_pubkey.to_string(), options)
            .await
            .map_err(|e| drbot_skills::SkillError::ExecutionFailed(e.to_string()))?;

        let Some(found) = found else {
            return Err(drbot_skills::SkillError::ExecutionFailed(format!(
                "Validator not found: {}",
                validator_str
            )));
        };

        Ok(SkillOutput::new(serde_json::to_value(found).map_err(
            |e| drbot_skills::SkillError::ExecutionFailed(e.to_string()),
        )?))
    }

    async fn handle_top(&self, input: &SkillInput) -> drbot_skills::Result<SkillOutput> {
        let mut input = input.clone();
        input
            .params
            .entry("sort".to_string())
            .or_insert_with(|| serde_json::json!("score_desc"));
        input
            .params
            .entry("limit".to_string())
            .or_insert_with(|| serde_json::json!(10));
        self.handle_list(&input).await
    }

    async fn handle_summary(&self, input: &SkillInput) -> drbot_skills::Result<SkillOutput> {
        let include_delinquent = Self::get_bool(&input.params, "include_delinquent", false);
        let with_performance = Self::get_bool(&input.params, "with_performance", false);

        let options = ValidatorIntelOptions {
            include_delinquent,
            with_performance,
            compute_scores: true,
        };

        let snapshot = self
            .client
            .snapshot(options)
            .await
            .map_err(|e| drbot_skills::SkillError::ExecutionFailed(e.to_string()))?;

        let total = snapshot.validators.len();
        let delinquent = snapshot
            .validators
            .iter()
            .filter(|v| v.vote.as_ref().is_some_and(|vote| vote.delinquent))
            .count();

        let commissions: Vec<u8> = snapshot
            .validators
            .iter()
            .filter_map(|v| v.vote.as_ref().map(|vote| vote.commission))
            .collect();

        let (min_commission, max_commission, avg_commission) = if commissions.is_empty() {
            (None, None, None)
        } else {
            let min = *commissions.iter().min().unwrap();
            let max = *commissions.iter().max().unwrap();
            let sum: u64 = commissions.iter().map(|&c| c as u64).sum();
            let avg = sum as f64 / commissions.len() as f64;
            (Some(min), Some(max), Some(avg))
        };

        Ok(SkillOutput::new(serde_json::json!({
            "fetchedAt": snapshot.fetched_at,
            "totalValidators": total,
            "delinquentValidators": delinquent,
            "totalStakeLamports": snapshot.total_stake_lamports,
            "totalStakeSol": snapshot.total_stake_lamports as f64 / solana_sdk::native_token::LAMPORTS_PER_SOL as f64,
            "commission": {
                "min": min_commission,
                "max": max_commission,
                "avg": avg_commission,
            }
        })))
    }

    async fn handle_validators(&self, input: &SkillInput) -> drbot_skills::Result<SkillOutput> {
        let include_delinquent = Self::get_bool(&input.params, "include_delinquent", false);
        let limit = Self::get_usize(&input.params, "limit").unwrap_or(100);
        let offset = Self::get_usize(&input.params, "offset").unwrap_or(0);
        let client_type = input.params.get("client_type").and_then(|v| v.as_str());

        let mut validators = self
            .client
            .fetch_validator_info(include_delinquent)
            .await
            .map_err(|e| drbot_skills::SkillError::ExecutionFailed(e.to_string()))?;

        if let Some(client_type) = client_type {
            validators.retain(|v| v.client_type.as_str().eq_ignore_ascii_case(client_type));
        }

        let total = validators.len();
        let paged: Vec<_> = validators.into_iter().skip(offset).take(limit).collect();

        Ok(SkillOutput::new(serde_json::json!({
            "validators": paged,
            "total": total,
            "limit": limit,
            "offset": offset,
        })))
    }

    async fn handle_overview(&self, input: &SkillInput) -> drbot_skills::Result<SkillOutput> {
        let include_delinquent = Self::get_bool(&input.params, "include_delinquent", false);

        let overview = self
            .client
            .fetch_network_overview(include_delinquent)
            .await
            .map_err(|e| drbot_skills::SkillError::ExecutionFailed(e.to_string()))?;

        Ok(SkillOutput::new(serde_json::to_value(overview).map_err(
            |e| drbot_skills::SkillError::ExecutionFailed(e.to_string()),
        )?))
    }

    async fn handle_blocks_compare(&self) -> drbot_skills::Result<SkillOutput> {
        let comparison = block_quality_comparison();

        Ok(SkillOutput::new(serde_json::json!({
            "comparison": comparison,
            "methodology": {
                "source": "On-chain block analysis via Helius RPC",
                "samplePeriod": "Current epoch",
                "metrics": [
                    "avgTxsPerBlock: Average transactions per block",
                    "successRate: Percentage of successful transactions",
                    "avgFeesPerSlot: Average fees collected per slot in SOL",
                    "jitoTipCount: Average number of Jito tips per block",
                    "userTxRatio: Percentage of user (non-vote) transactions",
                    "sandwichCount: Detected sandwich attack patterns per sample",
                ],
            }
        })))
    }

    async fn handle_sfdp_overview(&self) -> drbot_skills::Result<SkillOutput> {
        Ok(SkillOutput::new(
            serde_json::to_value(sfdp_overview())
                .map_err(|e| drbot_skills::SkillError::ExecutionFailed(e.to_string()))?,
        ))
    }
}
