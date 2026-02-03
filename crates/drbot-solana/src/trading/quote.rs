//! Quote types for Jupiter swaps.

use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;

/// A swap quote from Jupiter.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwapQuote {
    /// Input token mint.
    #[serde(with = "pubkey_string")]
    pub input_mint: Pubkey,
    /// Output token mint.
    #[serde(with = "pubkey_string")]
    pub output_mint: Pubkey,
    /// Input amount in smallest units.
    #[serde(deserialize_with = "deserialize_string_to_u64")]
    pub in_amount: u64,
    /// Output amount in smallest units.
    #[serde(deserialize_with = "deserialize_string_to_u64")]
    pub out_amount: u64,
    /// Other amount threshold (minimum output after slippage).
    #[serde(deserialize_with = "deserialize_string_to_u64")]
    pub other_amount_threshold: u64,
    /// Swap mode (ExactIn or ExactOut).
    pub swap_mode: String,
    /// Slippage in basis points.
    pub slippage_bps: u16,
    /// Price impact percentage.
    #[serde(default)]
    pub price_impact_pct: Option<String>,
    /// Route plan.
    #[serde(default)]
    pub route_plan: Vec<RoutePlan>,
}

impl SwapQuote {
    /// Get price impact as a float percentage.
    pub fn price_impact(&self) -> f64 {
        self.price_impact_pct
            .as_ref()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.0)
    }

    /// Get the effective price (output per input).
    pub fn effective_price(&self, in_decimals: u8, out_decimals: u8) -> f64 {
        let in_amount = self.in_amount as f64 / 10f64.powi(in_decimals as i32);
        let out_amount = self.out_amount as f64 / 10f64.powi(out_decimals as i32);
        out_amount / in_amount
    }

    /// Check if price impact is acceptable.
    pub fn is_price_impact_acceptable(&self, max_impact_pct: f64) -> bool {
        self.price_impact() <= max_impact_pct
    }
}

/// Route plan for a swap.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutePlan {
    /// Swap info for this leg.
    pub swap_info: SwapInfo,
    /// Percentage of the swap going through this route.
    pub percent: u8,
}

/// Swap information for a route leg.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwapInfo {
    /// AMM key.
    pub amm_key: String,
    /// Label (DEX name).
    pub label: Option<String>,
    /// Input mint.
    pub input_mint: String,
    /// Output mint.
    pub output_mint: String,
    /// Input amount.
    #[serde(default, deserialize_with = "deserialize_string_to_u64_opt")]
    pub in_amount: Option<u64>,
    /// Output amount.
    #[serde(default, deserialize_with = "deserialize_string_to_u64_opt")]
    pub out_amount: Option<u64>,
    /// Fee amount.
    #[serde(default, deserialize_with = "deserialize_string_to_u64_opt")]
    pub fee_amount: Option<u64>,
    /// Fee mint.
    pub fee_mint: Option<String>,
}

/// Swap fees breakdown.
#[derive(Debug, Clone, Default)]
pub struct SwapFees {
    /// Total platform fees.
    pub platform_fee: u64,
    /// Total LP fees.
    pub lp_fee: u64,
    /// Network transaction fee estimate.
    pub network_fee: u64,
}

// Serialization helpers
mod pubkey_string {
    use serde::{self, Deserialize, Deserializer, Serializer};
    use solana_sdk::pubkey::Pubkey;
    use std::str::FromStr;

    pub fn serialize<S>(pubkey: &Pubkey, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&pubkey.to_string())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Pubkey, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Pubkey::from_str(&s).map_err(serde::de::Error::custom)
    }
}

fn deserialize_string_to_u64<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s: String = serde::Deserialize::deserialize(deserializer)?;
    s.parse().map_err(serde::de::Error::custom)
}

fn deserialize_string_to_u64_opt<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let opt: Option<String> = serde::Deserialize::deserialize(deserializer)?;
    match opt {
        Some(s) => s.parse().map(Some).map_err(serde::de::Error::custom),
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quote_price_impact() {
        let quote = SwapQuote {
            input_mint: Pubkey::new_unique(),
            output_mint: Pubkey::new_unique(),
            in_amount: 1_000_000_000,
            out_amount: 100_000_000,
            other_amount_threshold: 99_000_000,
            swap_mode: "ExactIn".to_string(),
            slippage_bps: 50,
            price_impact_pct: Some("0.5".to_string()),
            route_plan: vec![],
        };

        assert_eq!(quote.price_impact(), 0.5);
        assert!(quote.is_price_impact_acceptable(1.0));
        assert!(!quote.is_price_impact_acceptable(0.3));
    }

    #[test]
    fn test_effective_price() {
        let quote = SwapQuote {
            input_mint: Pubkey::new_unique(),
            output_mint: Pubkey::new_unique(),
            in_amount: 1_000_000_000, // 1 SOL (9 decimals)
            out_amount: 100_000_000,  // 100 USDC (6 decimals)
            other_amount_threshold: 99_000_000,
            swap_mode: "ExactIn".to_string(),
            slippage_bps: 50,
            price_impact_pct: None,
            route_plan: vec![],
        };

        let price = quote.effective_price(9, 6);
        assert!((price - 100.0).abs() < 0.001);
    }
}
