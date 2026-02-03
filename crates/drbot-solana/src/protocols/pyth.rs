//! Pyth Network price oracle integration.
//!
//! Provides access to real-time and historical price feeds from Pyth Network.

use crate::{Result, SolanaError};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, trace};

/// Pyth Network base URL for Hermes API.
const PYTH_HERMES_URL: &str = "https://hermes.pyth.network";

/// Known Pyth price feed IDs.
pub struct PythFeedIds;

impl PythFeedIds {
    /// SOL/USD price feed.
    pub const SOL_USD: &'static str =
        "0xef0d8b6fda2ceba41da15d4095d1da392a0d2f8ed0c6c7bc0f4cfac8c280b56d";
    /// BTC/USD price feed.
    pub const BTC_USD: &'static str =
        "0xe62df6c8b4a85fe1a67db44dc12de5db330f7ac66b72dc658afedf0f4a415b43";
    /// ETH/USD price feed.
    pub const ETH_USD: &'static str =
        "0xff61491a931112ddf1bd8147cd1b641375f79f5825126d665480874634fd0ace";
    /// USDC/USD price feed.
    pub const USDC_USD: &'static str =
        "0xeaa020c61cc479712813461ce153894a96a6c00b21ed0cfc2798d1f9a9e9c94a";
    /// BONK/USD price feed.
    pub const BONK_USD: &'static str =
        "0x72b021217ca3fe68922a19aaf990109cb9d84e9ad004b4d2025ad6f529314419";
    /// JUP/USD price feed.
    pub const JUP_USD: &'static str =
        "0x0a0408d619e9380abad35060f9192039ed5042fa6f82301d0e4ae55a2d8817cd";

    /// Get feed ID for a symbol.
    pub fn get(symbol: &str) -> Option<&'static str> {
        match symbol.to_uppercase().as_str() {
            "SOL" | "SOL/USD" => Some(Self::SOL_USD),
            "BTC" | "BTC/USD" => Some(Self::BTC_USD),
            "ETH" | "ETH/USD" => Some(Self::ETH_USD),
            "USDC" | "USDC/USD" => Some(Self::USDC_USD),
            "BONK" | "BONK/USD" => Some(Self::BONK_USD),
            "JUP" | "JUP/USD" => Some(Self::JUP_USD),
            _ => None,
        }
    }

    /// Get all known feed IDs.
    pub fn all() -> HashMap<&'static str, &'static str> {
        let mut feeds = HashMap::new();
        feeds.insert("SOL", Self::SOL_USD);
        feeds.insert("BTC", Self::BTC_USD);
        feeds.insert("ETH", Self::ETH_USD);
        feeds.insert("USDC", Self::USDC_USD);
        feeds.insert("BONK", Self::BONK_USD);
        feeds.insert("JUP", Self::JUP_USD);
        feeds
    }
}

/// Price data from Pyth.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceData {
    /// Symbol (e.g., "SOL").
    pub symbol: String,
    /// Price in USD.
    pub price: f64,
    /// Confidence interval.
    pub confidence: f64,
    /// Publish timestamp (Unix seconds).
    pub timestamp: i64,
    /// Exponent used for price calculation.
    pub exponent: i32,
}

/// EMA (Exponential Moving Average) price data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmaPriceData {
    /// Symbol.
    pub symbol: String,
    /// EMA price.
    pub ema_price: f64,
    /// EMA confidence.
    pub ema_confidence: f64,
    /// Publish timestamp.
    pub timestamp: i64,
}

/// Pyth price feed client.
pub struct PythClient {
    client: Client,
    base_url: String,
}

impl PythClient {
    /// Create a new Pyth client.
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            base_url: PYTH_HERMES_URL.to_string(),
        }
    }

    /// Create a new Pyth client with custom URL.
    pub fn with_url(url: String) -> Self {
        Self {
            client: Client::new(),
            base_url: url,
        }
    }

    /// Get current price for a symbol.
    pub async fn get_price(&self, symbol: &str) -> Result<PriceData> {
        let feed_id = PythFeedIds::get(symbol)
            .ok_or_else(|| SolanaError::DeFiProtocolError(format!("Unknown symbol: {}", symbol)))?;

        self.get_price_by_feed_id(feed_id, symbol).await
    }

    /// Get current price by feed ID.
    pub async fn get_price_by_feed_id(&self, feed_id: &str, symbol: &str) -> Result<PriceData> {
        let url = format!("{}/api/latest_price_feeds?ids[]={}", self.base_url, feed_id);

        trace!(feed_id = feed_id, "Fetching Pyth price");

        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(SolanaError::DeFiProtocolError(format!(
                "Pyth API error: {}",
                error_text
            )));
        }

        let data: Vec<PythPriceFeedResponse> = response.json().await?;

        let feed = data.first().ok_or_else(|| {
            SolanaError::DeFiProtocolError(format!("No price data for {}", symbol))
        })?;

        let price_data = &feed.price;
        let price =
            price_data.price.parse::<i64>().unwrap_or(0) as f64 * 10f64.powi(price_data.expo);
        let confidence =
            price_data.conf.parse::<i64>().unwrap_or(0) as f64 * 10f64.powi(price_data.expo);

        debug!(
            symbol = symbol,
            price = price,
            confidence = confidence,
            "Got Pyth price"
        );

        Ok(PriceData {
            symbol: symbol.to_string(),
            price,
            confidence,
            timestamp: price_data.publish_time,
            exponent: price_data.expo,
        })
    }

    /// Get prices for multiple symbols.
    pub async fn get_prices(&self, symbols: &[&str]) -> Result<Vec<PriceData>> {
        let mut prices = Vec::new();

        for symbol in symbols {
            match self.get_price(symbol).await {
                Ok(price) => prices.push(price),
                Err(e) => {
                    debug!(symbol = symbol, error = %e, "Failed to get price");
                }
            }
        }

        Ok(prices)
    }

    /// Get EMA price for a symbol.
    pub async fn get_ema_price(&self, symbol: &str) -> Result<EmaPriceData> {
        let feed_id = PythFeedIds::get(symbol)
            .ok_or_else(|| SolanaError::DeFiProtocolError(format!("Unknown symbol: {}", symbol)))?;

        let url = format!("{}/api/latest_price_feeds?ids[]={}", self.base_url, feed_id);

        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(SolanaError::DeFiProtocolError(format!(
                "Pyth API error: {}",
                error_text
            )));
        }

        let data: Vec<PythPriceFeedResponse> = response.json().await?;

        let feed = data.first().ok_or_else(|| {
            SolanaError::DeFiProtocolError(format!("No price data for {}", symbol))
        })?;

        let ema_data = &feed.ema_price;
        let ema_price =
            ema_data.price.parse::<i64>().unwrap_or(0) as f64 * 10f64.powi(ema_data.expo);
        let ema_confidence =
            ema_data.conf.parse::<i64>().unwrap_or(0) as f64 * 10f64.powi(ema_data.expo);

        Ok(EmaPriceData {
            symbol: symbol.to_string(),
            ema_price,
            ema_confidence,
            timestamp: ema_data.publish_time,
        })
    }

    /// Get all supported symbols.
    pub fn supported_symbols() -> Vec<&'static str> {
        vec!["SOL", "BTC", "ETH", "USDC", "BONK", "JUP"]
    }
}

impl Default for PythClient {
    fn default() -> Self {
        Self::new()
    }
}

/// Pyth API response types.
#[derive(Debug, Deserialize)]
struct PythPriceFeedResponse {
    price: PythPriceComponent,
    ema_price: PythPriceComponent,
}

#[derive(Debug, Deserialize)]
struct PythPriceComponent {
    price: String,
    conf: String,
    expo: i32,
    publish_time: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_feed_id_lookup() {
        assert_eq!(PythFeedIds::get("SOL"), Some(PythFeedIds::SOL_USD));
        assert_eq!(PythFeedIds::get("sol"), Some(PythFeedIds::SOL_USD));
        assert_eq!(PythFeedIds::get("BTC"), Some(PythFeedIds::BTC_USD));
        assert_eq!(PythFeedIds::get("unknown"), None);
    }

    #[test]
    fn test_all_feed_ids() {
        let feeds = PythFeedIds::all();
        assert!(feeds.contains_key("SOL"));
        assert!(feeds.contains_key("BTC"));
        assert!(feeds.contains_key("ETH"));
    }

    #[test]
    fn test_supported_symbols() {
        let symbols = PythClient::supported_symbols();
        assert!(symbols.contains(&"SOL"));
        assert!(symbols.contains(&"BTC"));
    }
}
