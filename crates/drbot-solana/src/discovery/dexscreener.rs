//! DexScreener API client for Solana.

use crate::{Result, SolanaError};
use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::Deserialize;
use tracing::{debug, trace};

/// DexScreener API client.
pub struct DexScreenerClient {
    base_url: String,
    client: Client,
}

impl DexScreenerClient {
    /// Create a new DexScreener client.
    pub fn new(base_url: String) -> Self {
        Self {
            base_url,
            client: Client::new(),
        }
    }

    /// Get pairs for a token by address.
    pub async fn get_token_pairs(&self, address: &str) -> Result<Vec<DexPair>> {
        let url = format!("{}/latest/dex/tokens/{}", self.base_url, address);
        trace!(url = %url, "Fetching token pairs");

        let response = self.client.get(&url).send().await?;

        if response.status() == 429 {
            return Err(SolanaError::RateLimitExceeded);
        }

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(SolanaError::DexScreenerError(error_text));
        }

        let result: DexPairsResponse = response.json().await?;
        let solana_pairs: Vec<DexPair> = result
            .pairs
            .unwrap_or_default()
            .into_iter()
            .filter(|p| p.chain_id == "solana")
            .collect();

        debug!(
            count = solana_pairs.len(),
            "Got token pairs from DexScreener"
        );
        Ok(solana_pairs)
    }

    /// Search for pairs by query.
    pub async fn search_pairs(&self, query: &str) -> Result<Vec<DexPair>> {
        let url = format!("{}/latest/dex/search", self.base_url);

        let response = self.client.get(&url).query(&[("q", query)]).send().await?;

        if response.status() == 429 {
            return Err(SolanaError::RateLimitExceeded);
        }

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(SolanaError::DexScreenerError(error_text));
        }

        let result: DexPairsResponse = response.json().await?;
        let solana_pairs: Vec<DexPair> = result
            .pairs
            .unwrap_or_default()
            .into_iter()
            .filter(|p| p.chain_id == "solana")
            .collect();

        debug!(query = query, count = solana_pairs.len(), "Searched pairs");
        Ok(solana_pairs)
    }

    /// Get trending/boosted tokens on Solana.
    pub async fn get_boosted_tokens(&self) -> Result<Vec<BoostedToken>> {
        let url = format!("{}/token-boosts/top/v1", self.base_url);

        let response = self.client.get(&url).send().await?;

        if response.status() == 429 {
            return Err(SolanaError::RateLimitExceeded);
        }

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(SolanaError::DexScreenerError(error_text));
        }

        let tokens: Vec<BoostedToken> = response.json().await?;
        let solana_tokens: Vec<BoostedToken> = tokens
            .into_iter()
            .filter(|t| t.chain_id == "solana")
            .collect();

        debug!(count = solana_tokens.len(), "Got boosted tokens");
        Ok(solana_tokens)
    }

    /// Get pair by address.
    pub async fn get_pair(&self, pair_address: &str) -> Result<Option<DexPair>> {
        let url = format!("{}/latest/dex/pairs/solana/{}", self.base_url, pair_address);

        let response = self.client.get(&url).send().await?;

        if response.status() == 429 {
            return Err(SolanaError::RateLimitExceeded);
        }

        if response.status() == 404 {
            return Ok(None);
        }

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(SolanaError::DexScreenerError(error_text));
        }

        let result: DexPairResponse = response.json().await?;
        Ok(result.pair)
    }
}

/// Response containing pairs.
#[derive(Debug, Deserialize)]
pub struct DexPairsResponse {
    /// List of pairs.
    pub pairs: Option<Vec<DexPair>>,
}

/// Response containing a single pair.
#[derive(Debug, Deserialize)]
pub struct DexPairResponse {
    /// The pair.
    pub pair: Option<DexPair>,
}

/// A DEX trading pair.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DexPair {
    /// Chain ID.
    pub chain_id: String,
    /// DEX ID (raydium, orca, etc.).
    pub dex_id: String,
    /// Pair URL on DexScreener.
    pub url: String,
    /// Pair address.
    pub pair_address: String,
    /// Base token.
    pub base_token: DexToken,
    /// Quote token.
    pub quote_token: DexToken,
    /// Price in native token.
    pub price_native: Option<String>,
    /// Price in USD.
    pub price_usd: Option<String>,
    /// Transaction counts.
    pub txns: Option<DexTransactions>,
    /// Volume.
    pub volume: Option<DexVolume>,
    /// Price change.
    pub price_change: Option<DexPriceChange>,
    /// Liquidity.
    pub liquidity: Option<DexLiquidity>,
    /// FDV (Fully Diluted Valuation).
    pub fdv: Option<f64>,
    /// Market cap.
    pub market_cap: Option<f64>,
    /// Pair created at timestamp.
    pub pair_created_at: Option<i64>,
}

impl DexPair {
    /// Get price as f64.
    pub fn price(&self) -> f64 {
        self.price_usd
            .as_ref()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.0)
    }

    /// Get 24h volume.
    pub fn volume_24h(&self) -> f64 {
        self.volume.as_ref().map(|v| v.h24).unwrap_or(0.0)
    }

    /// Get 24h price change percentage.
    pub fn price_change_24h(&self) -> f64 {
        self.price_change.as_ref().map(|p| p.h24).unwrap_or(0.0)
    }

    /// Get 5m price change percentage.
    pub fn price_change_5m(&self) -> f64 {
        self.price_change.as_ref().map(|p| p.m5).unwrap_or(0.0)
    }

    /// Get 1h price change percentage.
    pub fn price_change_1h(&self) -> f64 {
        self.price_change.as_ref().map(|p| p.h1).unwrap_or(0.0)
    }

    /// Get 6h price change percentage.
    pub fn price_change_6h(&self) -> f64 {
        self.price_change.as_ref().map(|p| p.h6).unwrap_or(0.0)
    }

    /// Get USD liquidity.
    pub fn liquidity_usd(&self) -> f64 {
        self.liquidity.as_ref().map(|l| l.usd).unwrap_or(0.0)
    }

    /// Get 24h buy count.
    pub fn buys_24h(&self) -> i32 {
        self.txns
            .as_ref()
            .and_then(|t| t.h24.as_ref())
            .map(|c| c.buys)
            .unwrap_or(0)
    }

    /// Get 24h sell count.
    pub fn sells_24h(&self) -> i32 {
        self.txns
            .as_ref()
            .and_then(|t| t.h24.as_ref())
            .map(|c| c.sells)
            .unwrap_or(0)
    }

    /// Get buy/sell ratio (>1 = more buys than sells).
    pub fn buy_sell_ratio(&self) -> f64 {
        let buys = self.buys_24h() as f64;
        let sells = self.sells_24h() as f64;
        if sells > 0.0 {
            buys / sells
        } else if buys > 0.0 {
            10.0 // High ratio if only buys
        } else {
            1.0 // Neutral if no transactions
        }
    }

    /// Get creation time.
    pub fn created_at(&self) -> Option<DateTime<Utc>> {
        self.pair_created_at
            .and_then(|ts| DateTime::from_timestamp(ts / 1000, 0))
    }

    /// Get age in hours.
    pub fn age_hours(&self) -> Option<f64> {
        self.created_at().map(|created| {
            let duration = Utc::now() - created;
            duration.num_minutes() as f64 / 60.0
        })
    }
}

/// Token information.
#[derive(Debug, Clone, Deserialize)]
pub struct DexToken {
    /// Token address.
    pub address: String,
    /// Token name.
    pub name: String,
    /// Token symbol.
    pub symbol: String,
}

/// Transaction counts.
#[derive(Debug, Clone, Deserialize)]
pub struct DexTransactions {
    /// 5 minute transactions.
    pub m5: Option<TxCount>,
    /// 1 hour transactions.
    pub h1: Option<TxCount>,
    /// 6 hour transactions.
    pub h6: Option<TxCount>,
    /// 24 hour transactions.
    pub h24: Option<TxCount>,
}

/// Buy/sell transaction count.
#[derive(Debug, Clone, Deserialize)]
pub struct TxCount {
    /// Buy count.
    pub buys: i32,
    /// Sell count.
    pub sells: i32,
}

/// Volume data.
#[derive(Debug, Clone, Deserialize)]
pub struct DexVolume {
    /// 5 minute volume.
    #[serde(default)]
    pub m5: f64,
    /// 1 hour volume.
    #[serde(default)]
    pub h1: f64,
    /// 6 hour volume.
    #[serde(default)]
    pub h6: f64,
    /// 24 hour volume.
    #[serde(default)]
    pub h24: f64,
}

/// Price change data.
#[derive(Debug, Clone, Deserialize)]
pub struct DexPriceChange {
    /// 5 minute change.
    #[serde(default)]
    pub m5: f64,
    /// 1 hour change.
    #[serde(default)]
    pub h1: f64,
    /// 6 hour change.
    #[serde(default)]
    pub h6: f64,
    /// 24 hour change.
    #[serde(default)]
    pub h24: f64,
}

/// Liquidity data.
#[derive(Debug, Clone, Deserialize)]
pub struct DexLiquidity {
    /// USD liquidity.
    #[serde(default)]
    pub usd: f64,
    /// Base token liquidity.
    #[serde(default)]
    pub base: f64,
    /// Quote token liquidity.
    #[serde(default)]
    pub quote: f64,
}

/// Boosted/trending token.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoostedToken {
    /// Chain ID.
    pub chain_id: String,
    /// Token address.
    pub token_address: String,
    /// Boost amount.
    pub amount: Option<f64>,
    /// Description.
    pub description: Option<String>,
    /// Icon URL.
    pub icon: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dex_pair_helpers() {
        let pair = DexPair {
            chain_id: "solana".to_string(),
            dex_id: "raydium".to_string(),
            url: "https://dexscreener.com/solana/...".to_string(),
            pair_address: "test".to_string(),
            base_token: DexToken {
                address: "test".to_string(),
                name: "Test".to_string(),
                symbol: "TEST".to_string(),
            },
            quote_token: DexToken {
                address: "usdc".to_string(),
                name: "USD Coin".to_string(),
                symbol: "USDC".to_string(),
            },
            price_native: Some("0.001".to_string()),
            price_usd: Some("0.05".to_string()),
            txns: None,
            volume: Some(DexVolume {
                m5: 1000.0,
                h1: 5000.0,
                h6: 25000.0,
                h24: 100000.0,
            }),
            price_change: Some(DexPriceChange {
                m5: 1.0,
                h1: 5.0,
                h6: -2.0,
                h24: 10.0,
            }),
            liquidity: Some(DexLiquidity {
                usd: 50000.0,
                base: 1000000.0,
                quote: 25000.0,
            }),
            fdv: Some(5000000.0),
            market_cap: Some(1000000.0),
            pair_created_at: Some(1700000000000),
        };

        assert_eq!(pair.price(), 0.05);
        assert_eq!(pair.volume_24h(), 100000.0);
        assert_eq!(pair.price_change_24h(), 10.0);
        assert_eq!(pair.liquidity_usd(), 50000.0);
        assert!(pair.created_at().is_some());
    }
}
