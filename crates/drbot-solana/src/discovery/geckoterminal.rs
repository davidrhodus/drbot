//! GeckoTerminal API client for Solana.

use crate::{Result, SolanaError};
use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::Deserialize;
use tracing::{debug, trace};

/// GeckoTerminal API client.
pub struct GeckoTerminalClient {
    base_url: String,
    client: Client,
}

impl GeckoTerminalClient {
    /// Create a new GeckoTerminal client.
    pub fn new(base_url: String) -> Self {
        Self {
            base_url,
            client: Client::new(),
        }
    }

    /// Get trending pools on Solana.
    pub async fn get_trending_pools(&self) -> Result<Vec<GeckoPool>> {
        let url = format!("{}/networks/solana/trending_pools", self.base_url);
        trace!(url = %url, "Fetching trending pools");

        let response = self.client.get(&url).send().await?;

        if response.status() == 429 {
            return Err(SolanaError::RateLimitExceeded);
        }

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(SolanaError::GeckoTerminalError(error_text));
        }

        let result: GeckoPoolsResponse = response.json().await?;
        let pools: Vec<GeckoPool> = result.data.into_iter().map(|d| d.attributes).collect();

        debug!(count = pools.len(), "Got trending pools from GeckoTerminal");
        Ok(pools)
    }

    /// Get new pools on Solana.
    pub async fn get_new_pools(&self) -> Result<Vec<GeckoPool>> {
        let url = format!("{}/networks/solana/new_pools", self.base_url);

        let response = self.client.get(&url).send().await?;

        if response.status() == 429 {
            return Err(SolanaError::RateLimitExceeded);
        }

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(SolanaError::GeckoTerminalError(error_text));
        }

        let result: GeckoPoolsResponse = response.json().await?;
        let pools: Vec<GeckoPool> = result.data.into_iter().map(|d| d.attributes).collect();

        debug!(count = pools.len(), "Got new pools from GeckoTerminal");
        Ok(pools)
    }

    /// Get pool info by address.
    pub async fn get_pool(&self, pool_address: &str) -> Result<Option<GeckoPool>> {
        let url = format!("{}/networks/solana/pools/{}", self.base_url, pool_address);

        let response = self.client.get(&url).send().await?;

        if response.status() == 429 {
            return Err(SolanaError::RateLimitExceeded);
        }

        if response.status() == 404 {
            return Ok(None);
        }

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(SolanaError::GeckoTerminalError(error_text));
        }

        let result: GeckoPoolResponse = response.json().await?;
        Ok(Some(result.data.attributes))
    }

    /// Get token info by address.
    pub async fn get_token(&self, token_address: &str) -> Result<Option<GeckoToken>> {
        let url = format!("{}/networks/solana/tokens/{}", self.base_url, token_address);

        let response = self.client.get(&url).send().await?;

        if response.status() == 429 {
            return Err(SolanaError::RateLimitExceeded);
        }

        if response.status() == 404 {
            return Ok(None);
        }

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(SolanaError::GeckoTerminalError(error_text));
        }

        let result: GeckoTokenResponse = response.json().await?;
        Ok(Some(result.data.attributes))
    }

    /// Search for tokens by query.
    pub async fn search(&self, query: &str) -> Result<Vec<GeckoSearchResult>> {
        let url = format!("{}/search/pools", self.base_url);

        let response = self
            .client
            .get(&url)
            .query(&[("query", query), ("network", "solana")])
            .send()
            .await?;

        if response.status() == 429 {
            return Err(SolanaError::RateLimitExceeded);
        }

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(SolanaError::GeckoTerminalError(error_text));
        }

        let result: GeckoSearchResponse = response.json().await?;
        debug!(
            query = query,
            count = result.data.len(),
            "Searched GeckoTerminal"
        );
        Ok(result.data)
    }

    /// Get OHLCV data for a pool.
    pub async fn get_ohlcv(
        &self,
        pool_address: &str,
        timeframe: &str,
        aggregate: u32,
    ) -> Result<Vec<OhlcvCandle>> {
        let url = format!(
            "{}/networks/solana/pools/{}/ohlcv/{}",
            self.base_url, pool_address, timeframe
        );

        let response = self
            .client
            .get(&url)
            .query(&[("aggregate", aggregate.to_string())])
            .send()
            .await?;

        if response.status() == 429 {
            return Err(SolanaError::RateLimitExceeded);
        }

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(SolanaError::GeckoTerminalError(error_text));
        }

        let result: GeckoOhlcvResponse = response.json().await?;
        Ok(result.data.attributes.ohlcv_list)
    }
}

/// Response wrapper for pools list.
#[derive(Debug, Deserialize)]
pub struct GeckoPoolsResponse {
    pub data: Vec<GeckoPoolData>,
}

/// Pool data wrapper.
#[derive(Debug, Deserialize)]
pub struct GeckoPoolData {
    pub id: String,
    #[serde(rename = "type")]
    pub data_type: String,
    pub attributes: GeckoPool,
}

/// Response wrapper for single pool.
#[derive(Debug, Deserialize)]
pub struct GeckoPoolResponse {
    pub data: GeckoPoolData,
}

/// Pool information from GeckoTerminal.
#[derive(Debug, Clone, Deserialize)]
pub struct GeckoPool {
    /// Pool name.
    pub name: String,
    /// Pool address.
    pub address: String,
    /// Base token price in USD.
    pub base_token_price_usd: Option<String>,
    /// Quote token price in USD.
    pub quote_token_price_usd: Option<String>,
    /// Base token price in quote token.
    pub base_token_price_quote_token: Option<String>,
    /// Quote token price in base token.
    pub quote_token_price_base_token: Option<String>,
    /// Reserve in USD.
    pub reserve_in_usd: Option<String>,
    /// FDV in USD.
    pub fdv_usd: Option<String>,
    /// Market cap in USD.
    pub market_cap_usd: Option<String>,
    /// 24h volume in USD.
    pub volume_usd: Option<GeckoVolume>,
    /// Price change percentage.
    pub price_change_percentage: Option<GeckoPriceChange>,
    /// Pool created at.
    pub pool_created_at: Option<String>,
}

impl GeckoPool {
    /// Get base token price as f64.
    pub fn price(&self) -> f64 {
        self.base_token_price_usd
            .as_ref()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.0)
    }

    /// Get reserve/liquidity as f64.
    pub fn liquidity_usd(&self) -> f64 {
        self.reserve_in_usd
            .as_ref()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.0)
    }

    /// Get 24h volume.
    pub fn volume_24h(&self) -> f64 {
        self.volume_usd.as_ref().map(|v| v.h24).unwrap_or(0.0)
    }

    /// Get 24h price change.
    pub fn price_change_24h(&self) -> f64 {
        self.price_change_percentage
            .as_ref()
            .map(|p| p.h24)
            .unwrap_or(0.0)
    }

    /// Get creation time.
    pub fn created_at(&self) -> Option<DateTime<Utc>> {
        self.pool_created_at.as_ref().and_then(|s| {
            DateTime::parse_from_rfc3339(s)
                .ok()
                .map(|dt| dt.with_timezone(&Utc))
        })
    }
}

/// Volume data.
#[derive(Debug, Clone, Deserialize)]
pub struct GeckoVolume {
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
pub struct GeckoPriceChange {
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

/// Token response.
#[derive(Debug, Deserialize)]
pub struct GeckoTokenResponse {
    pub data: GeckoTokenData,
}

/// Token data wrapper.
#[derive(Debug, Deserialize)]
pub struct GeckoTokenData {
    pub id: String,
    pub attributes: GeckoToken,
}

/// Token information.
#[derive(Debug, Clone, Deserialize)]
pub struct GeckoToken {
    /// Token address.
    pub address: String,
    /// Token name.
    pub name: String,
    /// Token symbol.
    pub symbol: String,
    /// Decimals.
    pub decimals: Option<u8>,
    /// Total supply.
    pub total_supply: Option<String>,
    /// Price in USD.
    pub price_usd: Option<String>,
    /// FDV.
    pub fdv_usd: Option<String>,
    /// Total reserve in USD.
    pub total_reserve_in_usd: Option<String>,
    /// Volume 24h.
    pub volume_usd: Option<GeckoVolume>,
}

impl GeckoToken {
    /// Get price as f64.
    pub fn price(&self) -> f64 {
        self.price_usd
            .as_ref()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.0)
    }
}

/// Search response.
#[derive(Debug, Deserialize)]
pub struct GeckoSearchResponse {
    pub data: Vec<GeckoSearchResult>,
}

/// Search result.
#[derive(Debug, Clone, Deserialize)]
pub struct GeckoSearchResult {
    /// Pool ID.
    pub id: String,
    /// Pool address.
    pub address: String,
    /// Pool name.
    pub name: String,
    /// Network ID.
    pub network: String,
}

/// OHLCV response.
#[derive(Debug, Deserialize)]
pub struct GeckoOhlcvResponse {
    pub data: GeckoOhlcvData,
}

/// OHLCV data wrapper.
#[derive(Debug, Deserialize)]
pub struct GeckoOhlcvData {
    pub attributes: GeckoOhlcvAttributes,
}

/// OHLCV attributes.
#[derive(Debug, Deserialize)]
pub struct GeckoOhlcvAttributes {
    pub ohlcv_list: Vec<OhlcvCandle>,
}

/// OHLCV candle data.
#[derive(Debug, Clone)]
pub struct OhlcvCandle {
    /// Timestamp.
    pub timestamp: i64,
    /// Open price.
    pub open: f64,
    /// High price.
    pub high: f64,
    /// Low price.
    pub low: f64,
    /// Close price.
    pub close: f64,
    /// Volume.
    pub volume: f64,
}

impl<'de> Deserialize<'de> for OhlcvCandle {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // GeckoTerminal returns OHLCV as arrays: [timestamp, open, high, low, close, volume]
        let arr: Vec<serde_json::Value> = serde::Deserialize::deserialize(deserializer)?;

        if arr.len() < 6 {
            return Err(serde::de::Error::custom("Invalid OHLCV array length"));
        }

        Ok(OhlcvCandle {
            timestamp: arr[0].as_i64().unwrap_or(0),
            open: arr[1].as_f64().unwrap_or(0.0),
            high: arr[2].as_f64().unwrap_or(0.0),
            low: arr[3].as_f64().unwrap_or(0.0),
            close: arr[4].as_f64().unwrap_or(0.0),
            volume: arr[5].as_f64().unwrap_or(0.0),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gecko_pool_helpers() {
        let pool = GeckoPool {
            name: "TEST/USDC".to_string(),
            address: "test".to_string(),
            base_token_price_usd: Some("0.05".to_string()),
            quote_token_price_usd: Some("1.0".to_string()),
            base_token_price_quote_token: None,
            quote_token_price_base_token: None,
            reserve_in_usd: Some("100000".to_string()),
            fdv_usd: Some("5000000".to_string()),
            market_cap_usd: Some("1000000".to_string()),
            volume_usd: Some(GeckoVolume {
                m5: 1000.0,
                h1: 5000.0,
                h6: 25000.0,
                h24: 100000.0,
            }),
            price_change_percentage: Some(GeckoPriceChange {
                m5: 1.0,
                h1: 5.0,
                h6: -2.0,
                h24: 10.0,
            }),
            pool_created_at: Some("2024-01-01T00:00:00Z".to_string()),
        };

        assert_eq!(pool.price(), 0.05);
        assert_eq!(pool.liquidity_usd(), 100000.0);
        assert_eq!(pool.volume_24h(), 100000.0);
        assert_eq!(pool.price_change_24h(), 10.0);
        assert!(pool.created_at().is_some());
    }
}
