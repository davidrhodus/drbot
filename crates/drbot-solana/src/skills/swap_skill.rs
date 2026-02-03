//! Swap skill for Jupiter DEX operations.

use crate::{
    trading::{JupiterClient, SwapExecutor, SwapExecutorConfig, SwapQuote},
    wallet::KeypairManager,
    Result, SolanaError,
};
use async_trait::async_trait;
use drbot_skills::{
    ManifestCapability, ManifestInput, ManifestOutput, Skill, SkillContext, SkillInput,
    SkillManifest, SkillOutput,
};
use serde::{Deserialize, Serialize};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;
use std::sync::Arc;

/// Swap skill for Jupiter DEX operations.
pub struct SwapSkill {
    manifest: SkillManifest,
    rpc_client: Arc<RpcClient>,
    jupiter: JupiterClient,
    keypair_manager: Option<KeypairManager>,
    default_slippage_bps: u16,
}

impl SwapSkill {
    /// Create a new swap skill.
    pub fn new(
        rpc_client: Arc<RpcClient>,
        jupiter: JupiterClient,
        keypair_manager: Option<KeypairManager>,
    ) -> Self {
        let manifest = SkillManifest {
            name: "swap".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            description: "Get quotes and execute token swaps via Jupiter".to_string(),
            author: Some("drbot".to_string()),
            license: Some("MIT".to_string()),
            homepage: None,
            repository: None,
            tags: vec![
                "solana".to_string(),
                "swap".to_string(),
                "jupiter".to_string(),
                "dex".to_string(),
            ],
            inputs: vec![
                ManifestInput {
                    name: "action".to_string(),
                    description: "Action to perform: quote, execute".to_string(),
                    param_type: "string".to_string(),
                    required: true,
                    default: None,
                    pattern: None,
                    enum_values: vec![],
                },
                ManifestInput {
                    name: "input_mint".to_string(),
                    description: "Input token mint address (or 'SOL' for native SOL)".to_string(),
                    param_type: "string".to_string(),
                    required: true,
                    default: None,
                    pattern: None,
                    enum_values: vec![],
                },
                ManifestInput {
                    name: "output_mint".to_string(),
                    description: "Output token mint address (or 'SOL' for native SOL)".to_string(),
                    param_type: "string".to_string(),
                    required: true,
                    default: None,
                    pattern: None,
                    enum_values: vec![],
                },
                ManifestInput {
                    name: "amount".to_string(),
                    description: "Amount to swap (in UI units, e.g., 1.5 SOL)".to_string(),
                    param_type: "number".to_string(),
                    required: true,
                    default: None,
                    pattern: None,
                    enum_values: vec![],
                },
                ManifestInput {
                    name: "slippage_bps".to_string(),
                    description: "Slippage tolerance in basis points (default 50 = 0.5%)"
                        .to_string(),
                    param_type: "number".to_string(),
                    required: false,
                    default: Some(serde_json::json!(50)),
                    pattern: None,
                    enum_values: vec![],
                },
            ],
            outputs: vec![ManifestOutput {
                name: "result".to_string(),
                description: "Quote or swap result".to_string(),
                output_type: "object".to_string(),
            }],
            capabilities: vec![
                ManifestCapability::required("blockchain"),
                ManifestCapability::required("defi"),
            ],
            entry_point: None,
            runtime: None,
        };

        Self {
            manifest,
            rpc_client,
            jupiter,
            keypair_manager,
            default_slippage_bps: 50,
        }
    }

    /// Set default slippage.
    pub fn with_slippage_bps(mut self, bps: u16) -> Self {
        self.default_slippage_bps = bps;
        self
    }
}

#[async_trait]
impl Skill for SwapSkill {
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
            "quote" | "execute" => {
                if input
                    .params
                    .get("input_mint")
                    .and_then(|v| v.as_str())
                    .is_none()
                {
                    return Err(drbot_skills::SkillError::ValidationFailed(
                        "Missing input_mint".to_string(),
                    ));
                }
                if input
                    .params
                    .get("output_mint")
                    .and_then(|v| v.as_str())
                    .is_none()
                {
                    return Err(drbot_skills::SkillError::ValidationFailed(
                        "Missing output_mint".to_string(),
                    ));
                }
                if input
                    .params
                    .get("amount")
                    .and_then(|v| v.as_f64())
                    .is_none()
                {
                    return Err(drbot_skills::SkillError::ValidationFailed(
                        "Missing amount".to_string(),
                    ));
                }
                Ok(())
            }
            _ => Err(drbot_skills::SkillError::ValidationFailed(format!(
                "Unknown action: {}. Use 'quote' or 'execute'",
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
            .unwrap_or("quote");

        match action {
            "quote" => self.handle_quote(&input).await,
            "execute" => self.handle_execute(&input).await,
            _ => Err(drbot_skills::SkillError::ExecutionFailed(format!(
                "Unknown action: {}",
                action
            ))),
        }
    }
}

impl SwapSkill {
    fn parse_mint(&self, value: &str) -> drbot_skills::Result<Pubkey> {
        // Handle common aliases
        let address = match value.to_uppercase().as_str() {
            "SOL" | "WSOL" => "So11111111111111111111111111111111111111112",
            "USDC" => "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
            "USDT" => "Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB",
            _ => value,
        };

        Pubkey::from_str(address)
            .map_err(|e| drbot_skills::SkillError::ValidationFailed(format!("Invalid mint: {}", e)))
    }

    async fn handle_quote(&self, input: &SkillInput) -> drbot_skills::Result<SkillOutput> {
        let input_mint = self.parse_mint(
            input
                .params
                .get("input_mint")
                .and_then(|v| v.as_str())
                .unwrap(),
        )?;
        let output_mint = self.parse_mint(
            input
                .params
                .get("output_mint")
                .and_then(|v| v.as_str())
                .unwrap(),
        )?;
        let amount_ui = input.params.get("amount").and_then(|v| v.as_f64()).unwrap();
        let slippage_bps = input
            .params
            .get("slippage_bps")
            .and_then(|v| v.as_u64())
            .map(|v| v as u16)
            .unwrap_or(self.default_slippage_bps);

        // Convert UI amount to raw amount (assuming 9 decimals for SOL, need to query for tokens)
        // This is simplified - real implementation should query token decimals
        let decimals = if input_mint.to_string() == "So11111111111111111111111111111111111111112" {
            9
        } else {
            6 // Default to 6 for most tokens
        };
        let amount_raw = (amount_ui * 10f64.powi(decimals as i32)) as u64;

        let executor = SwapExecutor::new(
            self.rpc_client.clone(),
            JupiterClient::new(self.jupiter.base_url().to_string()),
            SwapExecutorConfig {
                default_slippage_bps: slippage_bps,
                ..Default::default()
            },
        );

        let quote = executor
            .get_quote(input_mint, output_mint, amount_raw, Some(slippage_bps))
            .await
            .map_err(|e| drbot_skills::SkillError::ExecutionFailed(e.to_string()))?;

        let output = QuoteOutput::from_quote(&quote, decimals, 6); // Assuming output is 6 decimals

        Ok(SkillOutput::new(output))
    }

    async fn handle_execute(&self, input: &SkillInput) -> drbot_skills::Result<SkillOutput> {
        let keypair_manager = self.keypair_manager.as_ref().ok_or_else(|| {
            drbot_skills::SkillError::ExecutionFailed("No keypair configured for swaps".to_string())
        })?;

        let keypair = keypair_manager
            .load_keypair()
            .await
            .map_err(|e| drbot_skills::SkillError::ExecutionFailed(e.to_string()))?;

        let input_mint = self.parse_mint(
            input
                .params
                .get("input_mint")
                .and_then(|v| v.as_str())
                .unwrap(),
        )?;
        let output_mint = self.parse_mint(
            input
                .params
                .get("output_mint")
                .and_then(|v| v.as_str())
                .unwrap(),
        )?;
        let amount_ui = input.params.get("amount").and_then(|v| v.as_f64()).unwrap();
        let slippage_bps = input
            .params
            .get("slippage_bps")
            .and_then(|v| v.as_u64())
            .map(|v| v as u16)
            .unwrap_or(self.default_slippage_bps);

        // Convert UI amount to raw amount
        let decimals = if input_mint.to_string() == "So11111111111111111111111111111111111111112" {
            9
        } else {
            6
        };
        let amount_raw = (amount_ui * 10f64.powi(decimals as i32)) as u64;

        let executor = SwapExecutor::new(
            self.rpc_client.clone(),
            JupiterClient::new(self.jupiter.base_url().to_string()),
            SwapExecutorConfig {
                default_slippage_bps: slippage_bps,
                ..Default::default()
            },
        );

        let result = executor
            .swap(
                &keypair,
                input_mint,
                output_mint,
                amount_raw,
                Some(slippage_bps),
            )
            .await
            .map_err(|e| drbot_skills::SkillError::ExecutionFailed(e.to_string()))?;

        let output = SwapOutput {
            signature: result.signature.to_string(),
            input_mint: result.input_mint.to_string(),
            output_mint: result.output_mint.to_string(),
            input_amount: amount_ui,
            output_amount: result.output_amount as f64 / 10f64.powi(6), // Assuming 6 decimals
            price_impact_pct: result.price_impact_pct,
            confirmed: result.confirmed,
            explorer_url: result.explorer_url("mainnet-beta"),
        };

        Ok(SkillOutput::new(output))
    }
}

/// Quote output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuoteOutput {
    /// Input token mint.
    pub input_mint: String,
    /// Output token mint.
    pub output_mint: String,
    /// Input amount (UI).
    pub input_amount: f64,
    /// Output amount (UI).
    pub output_amount: f64,
    /// Minimum output amount after slippage.
    pub min_output_amount: f64,
    /// Price impact percentage.
    pub price_impact_pct: f64,
    /// Effective price (output per input).
    pub effective_price: f64,
    /// Slippage tolerance in basis points.
    pub slippage_bps: u16,
    /// Route summary.
    pub route_summary: String,
}

impl QuoteOutput {
    fn from_quote(quote: &SwapQuote, in_decimals: u8, out_decimals: u8) -> Self {
        let in_amount = quote.in_amount as f64 / 10f64.powi(in_decimals as i32);
        let out_amount = quote.out_amount as f64 / 10f64.powi(out_decimals as i32);
        let min_out = quote.other_amount_threshold as f64 / 10f64.powi(out_decimals as i32);

        let route_summary = if quote.route_plan.is_empty() {
            "Direct".to_string()
        } else {
            quote
                .route_plan
                .iter()
                .filter_map(|r| r.swap_info.label.clone())
                .collect::<Vec<_>>()
                .join(" → ")
        };

        Self {
            input_mint: quote.input_mint.to_string(),
            output_mint: quote.output_mint.to_string(),
            input_amount: in_amount,
            output_amount: out_amount,
            min_output_amount: min_out,
            price_impact_pct: quote.price_impact(),
            effective_price: out_amount / in_amount,
            slippage_bps: quote.slippage_bps,
            route_summary,
        }
    }
}

/// Swap output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwapOutput {
    /// Transaction signature.
    pub signature: String,
    /// Input token mint.
    pub input_mint: String,
    /// Output token mint.
    pub output_mint: String,
    /// Input amount (UI).
    pub input_amount: f64,
    /// Output amount (UI).
    pub output_amount: f64,
    /// Price impact percentage.
    pub price_impact_pct: f64,
    /// Whether transaction was confirmed.
    pub confirmed: bool,
    /// Explorer URL.
    pub explorer_url: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_swap_skill_manifest() {
        let rpc = Arc::new(RpcClient::new("https://api.devnet.solana.com".to_string()));
        let jupiter = JupiterClient::new("https://quote-api.jup.ag/v6".to_string());
        let skill = SwapSkill::new(rpc, jupiter, None);
        let manifest = skill.manifest();

        assert_eq!(manifest.name, "swap");
        assert!(manifest.inputs.iter().any(|i| i.name == "input_mint"));
        assert!(manifest.inputs.iter().any(|i| i.name == "output_mint"));
    }

    #[test]
    fn test_parse_mint_aliases() {
        let rpc = Arc::new(RpcClient::new("https://api.devnet.solana.com".to_string()));
        let jupiter = JupiterClient::new("https://quote-api.jup.ag/v6".to_string());
        let skill = SwapSkill::new(rpc, jupiter, None);

        let sol = skill.parse_mint("SOL").unwrap();
        assert_eq!(
            sol.to_string(),
            "So11111111111111111111111111111111111111112"
        );

        let usdc = skill.parse_mint("USDC").unwrap();
        assert_eq!(
            usdc.to_string(),
            "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"
        );
    }
}
