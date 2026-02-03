//! Wallet skill for balance queries and transfers.

use crate::{
    wallet::{BalanceQuery, KeypairManager, TransferExecutor, WalletInfo},
    Result, SolanaError,
};
use async_trait::async_trait;
use drbot_skills::{
    ManifestCapability, ManifestInput, ManifestOutput, Skill, SkillContext, SkillInput,
    SkillManifest, SkillOutput,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;
use std::sync::Arc;

/// Wallet skill for Solana operations.
pub struct WalletSkill {
    manifest: SkillManifest,
    rpc_client: Arc<RpcClient>,
    keypair_manager: Option<KeypairManager>,
}

impl WalletSkill {
    /// Create a new wallet skill.
    pub fn new(rpc_client: Arc<RpcClient>, keypair_manager: Option<KeypairManager>) -> Self {
        let manifest = SkillManifest {
            name: "wallet".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            description: "Check Solana wallet balances and perform transfers".to_string(),
            author: Some("drbot".to_string()),
            license: Some("MIT".to_string()),
            homepage: None,
            repository: None,
            tags: vec![
                "solana".to_string(),
                "wallet".to_string(),
                "balance".to_string(),
            ],
            inputs: vec![
                ManifestInput {
                    name: "action".to_string(),
                    description: "Action to perform: balance, transfer".to_string(),
                    param_type: "string".to_string(),
                    required: true,
                    default: None,
                    pattern: None,
                    enum_values: vec![],
                },
                ManifestInput {
                    name: "address".to_string(),
                    description:
                        "Wallet address to query (for balance) or destination (for transfer)"
                            .to_string(),
                    param_type: "string".to_string(),
                    required: false,
                    default: None,
                    pattern: None,
                    enum_values: vec![],
                },
                ManifestInput {
                    name: "amount".to_string(),
                    description: "Amount to transfer in SOL".to_string(),
                    param_type: "number".to_string(),
                    required: false,
                    default: None,
                    pattern: None,
                    enum_values: vec![],
                },
                ManifestInput {
                    name: "token_mint".to_string(),
                    description: "Token mint address for SPL token transfers".to_string(),
                    param_type: "string".to_string(),
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
                ManifestCapability::required("wallet"),
            ],
            entry_point: None,
            runtime: None,
        };

        Self {
            manifest,
            rpc_client,
            keypair_manager,
        }
    }
}

#[async_trait]
impl Skill for WalletSkill {
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
            "balance" => Ok(()),
            "transfer" => {
                if input
                    .params
                    .get("address")
                    .and_then(|v| v.as_str())
                    .is_none()
                {
                    return Err(drbot_skills::SkillError::ValidationFailed(
                        "Transfer requires destination address".to_string(),
                    ));
                }
                if input
                    .params
                    .get("amount")
                    .and_then(|v| v.as_f64())
                    .is_none()
                {
                    return Err(drbot_skills::SkillError::ValidationFailed(
                        "Transfer requires amount".to_string(),
                    ));
                }
                Ok(())
            }
            _ => Err(drbot_skills::SkillError::ValidationFailed(format!(
                "Unknown action: {}. Use 'balance' or 'transfer'",
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
            .unwrap_or("balance");

        match action {
            "balance" => self.handle_balance(&input).await,
            "transfer" => self.handle_transfer(&input).await,
            _ => Err(drbot_skills::SkillError::ExecutionFailed(format!(
                "Unknown action: {}",
                action
            ))),
        }
    }
}

impl WalletSkill {
    async fn handle_balance(&self, input: &SkillInput) -> drbot_skills::Result<SkillOutput> {
        let query = BalanceQuery::new(self.rpc_client.clone());

        // Get address from input or use keypair's address
        let address = if let Some(addr_str) = input.params.get("address").and_then(|v| v.as_str()) {
            Pubkey::from_str(addr_str)
                .map_err(|e| drbot_skills::SkillError::ExecutionFailed(e.to_string()))?
        } else if let Some(ref km) = self.keypair_manager {
            km.pubkey()
                .await
                .map_err(|e| drbot_skills::SkillError::ExecutionFailed(e.to_string()))?
        } else {
            return Err(drbot_skills::SkillError::ValidationFailed(
                "No address provided and no keypair configured".to_string(),
            ));
        };

        let wallet_info = query
            .get_wallet_info(&address)
            .await
            .map_err(|e| drbot_skills::SkillError::ExecutionFailed(e.to_string()))?;

        let result = WalletBalanceOutput {
            address: address.to_string(),
            sol_balance: wallet_info.sol_balance_ui(),
            sol_balance_lamports: wallet_info.sol_balance,
            token_balances: wallet_info
                .token_balances
                .into_iter()
                .map(|tb| TokenBalanceOutput {
                    mint: tb.mint.to_string(),
                    symbol: tb.symbol,
                    balance: tb.ui_amount,
                    raw_balance: tb.balance,
                    decimals: tb.decimals,
                })
                .collect(),
        };

        Ok(SkillOutput::new(result))
    }

    async fn handle_transfer(&self, input: &SkillInput) -> drbot_skills::Result<SkillOutput> {
        let keypair_manager = self.keypair_manager.as_ref().ok_or_else(|| {
            drbot_skills::SkillError::ExecutionFailed(
                "No keypair configured for transfers".to_string(),
            )
        })?;

        let keypair = keypair_manager
            .load_keypair()
            .await
            .map_err(|e| drbot_skills::SkillError::ExecutionFailed(e.to_string()))?;

        let to_address = input
            .params
            .get("address")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                drbot_skills::SkillError::ValidationFailed(
                    "Missing destination address".to_string(),
                )
            })?;

        let to_pubkey = Pubkey::from_str(to_address)
            .map_err(|e| drbot_skills::SkillError::ValidationFailed(e.to_string()))?;

        let amount_sol = input
            .params
            .get("amount")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| {
                drbot_skills::SkillError::ValidationFailed("Missing amount".to_string())
            })?;

        let executor = TransferExecutor::new(self.rpc_client.clone());

        let result = if let Some(mint_str) = input.params.get("token_mint").and_then(|v| v.as_str())
        {
            // Token transfer
            let mint = Pubkey::from_str(mint_str)
                .map_err(|e| drbot_skills::SkillError::ValidationFailed(e.to_string()))?;

            // Note: For token transfers, amount needs to be converted based on decimals
            // This is a simplified version - real implementation should query decimals
            let amount_raw = (amount_sol * 1_000_000.0) as u64; // Assuming 6 decimals

            executor
                .transfer_token(&keypair, &to_pubkey, &mint, amount_raw)
                .await
                .map_err(|e| drbot_skills::SkillError::ExecutionFailed(e.to_string()))?
        } else {
            // SOL transfer
            let lamports = (amount_sol * 1_000_000_000.0) as u64;

            executor
                .transfer_sol(&keypair, &to_pubkey, lamports)
                .await
                .map_err(|e| drbot_skills::SkillError::ExecutionFailed(e.to_string()))?
        };

        let output = TransferOutput {
            signature: result.signature.to_string(),
            from: result.from.to_string(),
            to: result.to.to_string(),
            amount: amount_sol,
            token_mint: result.mint.map(|m| m.to_string()),
            explorer_url: result.explorer_url("mainnet-beta"),
        };

        Ok(SkillOutput::new(output))
    }
}

/// Wallet balance output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletBalanceOutput {
    /// Wallet address.
    pub address: String,
    /// SOL balance.
    pub sol_balance: f64,
    /// SOL balance in lamports.
    pub sol_balance_lamports: u64,
    /// Token balances.
    pub token_balances: Vec<TokenBalanceOutput>,
}

/// Token balance output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenBalanceOutput {
    /// Token mint.
    pub mint: String,
    /// Token symbol (if known).
    pub symbol: Option<String>,
    /// Balance (UI amount).
    pub balance: f64,
    /// Raw balance.
    pub raw_balance: u64,
    /// Decimals.
    pub decimals: u8,
}

/// Transfer output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferOutput {
    /// Transaction signature.
    pub signature: String,
    /// From address.
    pub from: String,
    /// To address.
    pub to: String,
    /// Amount transferred.
    pub amount: f64,
    /// Token mint (if token transfer).
    pub token_mint: Option<String>,
    /// Explorer URL.
    pub explorer_url: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wallet_skill_manifest() {
        let rpc = Arc::new(RpcClient::new("https://api.devnet.solana.com".to_string()));
        let skill = WalletSkill::new(rpc, None);
        let manifest = skill.manifest();

        assert_eq!(manifest.name, "wallet");
        assert!(manifest.inputs.iter().any(|i| i.name == "action"));
    }
}
