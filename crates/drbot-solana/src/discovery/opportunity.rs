//! Opportunity discovery and filtering.

use super::{DexPair, DexScreenerClient, GeckoPool, GeckoTerminalClient};
use crate::{Result, SolanaError};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;
use tracing::debug;

/// Token trading opportunity.
#[derive(Debug, Clone, Serialize)]
pub struct TokenOpportunity {
    /// Token address.
    pub address: String,
    /// Token symbol.
    pub symbol: String,
    /// Token name.
    pub name: String,
    /// Current price in USD.
    pub price_usd: f64,
    /// 24h volume in USD.
    pub volume_24h: f64,
    /// Liquidity in USD.
    pub liquidity_usd: f64,
    /// 24h price change percentage.
    pub price_change_24h: f64,
    /// 5m price change percentage.
    pub price_change_5m: f64,
    /// 1h price change percentage.
    pub price_change_1h: f64,
    /// 6h price change percentage.
    pub price_change_6h: f64,
    /// Pool/pair created at.
    pub created_at: Option<DateTime<Utc>>,
    /// Age in hours.
    pub age_hours: Option<f64>,
    /// Data source.
    pub source: OpportunitySource,
    /// Market cap (if available).
    pub market_cap: Option<f64>,
    /// FDV (if available).
    pub fdv: Option<f64>,
    /// DEX/AMM name.
    pub dex: String,
    /// Pair/pool address.
    pub pair_address: String,
    /// URL to view on explorer.
    pub url: Option<String>,
    /// 24h buy transaction count.
    pub buys_24h: i32,
    /// 24h sell transaction count.
    pub sells_24h: i32,
}

impl TokenOpportunity {
    /// Get pubkey if valid.
    pub fn pubkey(&self) -> Option<Pubkey> {
        Pubkey::from_str(&self.address).ok()
    }

    /// Get buy/sell ratio (>1 = more buys than sells).
    pub fn buy_sell_ratio(&self) -> f64 {
        let buys = self.buys_24h as f64;
        let sells = self.sells_24h as f64;
        if sells > 0.0 {
            buys / sells
        } else if buys > 0.0 {
            10.0 // High ratio if only buys
        } else {
            1.0 // Neutral if no transactions
        }
    }

    /// Get volume to liquidity ratio.
    pub fn volume_liquidity_ratio(&self) -> f64 {
        if self.liquidity_usd > 0.0 {
            self.volume_24h / self.liquidity_usd
        } else {
            0.0
        }
    }

    /// Check if this matches the filter criteria.
    pub fn matches_filter(&self, filter: &OpportunityFilter) -> bool {
        if let Some(min_liq) = filter.min_liquidity_usd {
            if self.liquidity_usd < min_liq {
                return false;
            }
        }

        if let Some(max_liq) = filter.max_liquidity_usd {
            if self.liquidity_usd > max_liq {
                return false;
            }
        }

        if let Some(min_vol) = filter.min_volume_24h {
            if self.volume_24h < min_vol {
                return false;
            }
        }

        if let Some(max_age) = filter.max_age_hours {
            if let Some(age) = self.age_hours {
                if age > max_age {
                    return false;
                }
            }
        }

        if let Some(min_age) = filter.min_age_hours {
            if let Some(age) = self.age_hours {
                if age < min_age {
                    return false;
                }
            }
        }

        if let Some(min_change) = filter.price_change_min {
            if self.price_change_24h < min_change {
                return false;
            }
        }

        if let Some(max_change) = filter.price_change_max {
            if self.price_change_24h > max_change {
                return false;
            }
        }

        true
    }
}

impl From<DexPair> for TokenOpportunity {
    fn from(pair: DexPair) -> Self {
        // Call methods before moving fields
        let buys_24h = pair.buys_24h();
        let sells_24h = pair.sells_24h();

        Self {
            address: pair.base_token.address.clone(),
            symbol: pair.base_token.symbol.clone(),
            name: pair.base_token.name.clone(),
            price_usd: pair.price(),
            volume_24h: pair.volume_24h(),
            liquidity_usd: pair.liquidity_usd(),
            price_change_24h: pair.price_change_24h(),
            price_change_5m: pair.price_change_5m(),
            price_change_1h: pair.price_change_1h(),
            price_change_6h: pair.price_change_6h(),
            created_at: pair.created_at(),
            age_hours: pair.age_hours(),
            source: OpportunitySource::DexScreener,
            market_cap: pair.market_cap,
            fdv: pair.fdv,
            dex: pair.dex_id,
            pair_address: pair.pair_address,
            url: Some(pair.url),
            buys_24h,
            sells_24h,
        }
    }
}

impl From<GeckoPool> for TokenOpportunity {
    fn from(pool: GeckoPool) -> Self {
        // Parse token info from pool name (usually "TOKEN/QUOTE")
        let parts: Vec<&str> = pool.name.split('/').collect();
        let symbol = parts.first().map(|s| s.to_string()).unwrap_or_default();

        Self {
            address: pool.address.clone(),
            symbol,
            name: pool.name.clone(),
            price_usd: pool.price(),
            volume_24h: pool.volume_24h(),
            liquidity_usd: pool.liquidity_usd(),
            price_change_24h: pool.price_change_24h(),
            price_change_5m: 0.0, // GeckoTerminal doesn't provide granular changes
            price_change_1h: 0.0,
            price_change_6h: 0.0,
            created_at: pool.created_at(),
            age_hours: pool.created_at().map(|created| {
                let duration = Utc::now() - created;
                duration.num_minutes() as f64 / 60.0
            }),
            source: OpportunitySource::GeckoTerminal,
            market_cap: pool.market_cap_usd.as_ref().and_then(|s| s.parse().ok()),
            fdv: pool.fdv_usd.as_ref().and_then(|s| s.parse().ok()),
            dex: "unknown".to_string(),
            pair_address: pool.address,
            url: None,
            buys_24h: 0, // GeckoTerminal doesn't provide transaction counts
            sells_24h: 0,
        }
    }
}

/// Opportunity data source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OpportunitySource {
    /// From DexScreener.
    DexScreener,
    /// From GeckoTerminal.
    GeckoTerminal,
}

/// Filter criteria for opportunities.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OpportunityFilter {
    /// Minimum liquidity in USD.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_liquidity_usd: Option<f64>,
    /// Maximum liquidity in USD.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_liquidity_usd: Option<f64>,
    /// Minimum 24h volume in USD.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_volume_24h: Option<f64>,
    /// Maximum age in hours.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_age_hours: Option<f64>,
    /// Minimum age in hours.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_age_hours: Option<f64>,
    /// Minimum 24h price change percentage.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price_change_min: Option<f64>,
    /// Maximum 24h price change percentage.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price_change_max: Option<f64>,
}

impl OpportunityFilter {
    /// Create a filter for new tokens.
    pub fn new_tokens() -> Self {
        Self {
            min_liquidity_usd: Some(10_000.0),
            max_age_hours: Some(24.0),
            min_volume_24h: Some(5_000.0),
            ..Default::default()
        }
    }

    /// Create a filter for established tokens.
    pub fn established() -> Self {
        Self {
            min_liquidity_usd: Some(100_000.0),
            min_age_hours: Some(168.0), // 1 week
            min_volume_24h: Some(50_000.0),
            ..Default::default()
        }
    }

    /// Create a filter for high momentum tokens.
    pub fn high_momentum() -> Self {
        Self {
            min_liquidity_usd: Some(25_000.0),
            min_volume_24h: Some(25_000.0),
            price_change_min: Some(10.0), // +10% or more
            ..Default::default()
        }
    }

    /// Set minimum liquidity.
    pub fn with_min_liquidity(mut self, usd: f64) -> Self {
        self.min_liquidity_usd = Some(usd);
        self
    }

    /// Set minimum volume.
    pub fn with_min_volume(mut self, usd: f64) -> Self {
        self.min_volume_24h = Some(usd);
        self
    }

    /// Set maximum age.
    pub fn with_max_age_hours(mut self, hours: f64) -> Self {
        self.max_age_hours = Some(hours);
        self
    }
}

/// Opportunity finder that aggregates data from multiple sources.
pub struct OpportunityFinder {
    dexscreener: DexScreenerClient,
    geckoterminal: GeckoTerminalClient,
}

impl OpportunityFinder {
    /// Create a new opportunity finder.
    pub fn new(dexscreener: DexScreenerClient, geckoterminal: GeckoTerminalClient) -> Self {
        Self {
            dexscreener,
            geckoterminal,
        }
    }

    /// Find opportunities from all sources.
    pub async fn find_all(&self, filter: &OpportunityFilter) -> Result<Vec<TokenOpportunity>> {
        let mut opportunities = Vec::new();

        // Get from DexScreener
        match self.find_from_dexscreener(filter).await {
            Ok(ops) => opportunities.extend(ops),
            Err(e) => debug!(error = %e, "Failed to get DexScreener opportunities"),
        }

        // Get from GeckoTerminal
        match self.find_from_geckoterminal(filter).await {
            Ok(ops) => opportunities.extend(ops),
            Err(e) => debug!(error = %e, "Failed to get GeckoTerminal opportunities"),
        }

        // Deduplicate by token address
        let mut seen = std::collections::HashSet::new();
        opportunities.retain(|op| seen.insert(op.address.clone()));

        // Sort by volume
        opportunities.sort_by(|a, b| {
            b.volume_24h
                .partial_cmp(&a.volume_24h)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        debug!(count = opportunities.len(), "Found opportunities");
        Ok(opportunities)
    }

    /// Find opportunities from DexScreener only.
    pub async fn find_from_dexscreener(
        &self,
        filter: &OpportunityFilter,
    ) -> Result<Vec<TokenOpportunity>> {
        let boosted = self.dexscreener.get_boosted_tokens().await?;

        let mut opportunities = Vec::new();

        for token in boosted.into_iter().take(50) {
            let pairs = match self.dexscreener.get_token_pairs(&token.token_address).await {
                Ok(p) => p,
                Err(_) => continue,
            };

            if let Some(best_pair) = pairs.into_iter().max_by(|a, b| {
                a.liquidity_usd()
                    .partial_cmp(&b.liquidity_usd())
                    .unwrap_or(std::cmp::Ordering::Equal)
            }) {
                let opportunity = TokenOpportunity::from(best_pair);
                if opportunity.matches_filter(filter) {
                    opportunities.push(opportunity);
                }
            }
        }

        Ok(opportunities)
    }

    /// Find opportunities from GeckoTerminal only.
    pub async fn find_from_geckoterminal(
        &self,
        filter: &OpportunityFilter,
    ) -> Result<Vec<TokenOpportunity>> {
        let pools = self.geckoterminal.get_trending_pools().await?;

        let opportunities: Vec<TokenOpportunity> = pools
            .into_iter()
            .map(TokenOpportunity::from)
            .filter(|op| op.matches_filter(filter))
            .collect();

        Ok(opportunities)
    }

    /// Search for opportunities by query.
    pub async fn search(
        &self,
        query: &str,
        filter: &OpportunityFilter,
    ) -> Result<Vec<TokenOpportunity>> {
        let mut opportunities = Vec::new();

        // Search DexScreener
        let dex_pairs = self.dexscreener.search_pairs(query).await?;
        for pair in dex_pairs {
            let op = TokenOpportunity::from(pair);
            if op.matches_filter(filter) {
                opportunities.push(op);
            }
        }

        // Search GeckoTerminal
        let gecko_results = self.geckoterminal.search(query).await?;
        for result in gecko_results {
            if let Ok(Some(pool)) = self.geckoterminal.get_pool(&result.address).await {
                let op = TokenOpportunity::from(pool);
                if op.matches_filter(filter) {
                    opportunities.push(op);
                }
            }
        }

        // Deduplicate
        let mut seen = std::collections::HashSet::new();
        opportunities.retain(|op| seen.insert(op.address.clone()));

        Ok(opportunities)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_matching() {
        let opportunity = TokenOpportunity {
            address: "test".to_string(),
            symbol: "TEST".to_string(),
            name: "Test Token".to_string(),
            price_usd: 0.05,
            volume_24h: 50000.0,
            liquidity_usd: 25000.0,
            price_change_24h: 15.0,
            price_change_5m: 2.0,
            price_change_1h: 5.0,
            price_change_6h: 10.0,
            created_at: None,
            age_hours: Some(12.0),
            source: OpportunitySource::DexScreener,
            market_cap: None,
            fdv: None,
            dex: "raydium".to_string(),
            pair_address: "pair".to_string(),
            url: None,
            buys_24h: 150,
            sells_24h: 100,
        };

        // Should match default filter
        assert!(opportunity.matches_filter(&OpportunityFilter::default()));

        // Should match new tokens filter
        assert!(opportunity.matches_filter(&OpportunityFilter::new_tokens()));

        // Should not match established filter (age too low)
        assert!(!opportunity.matches_filter(&OpportunityFilter::established()));

        // Should match high momentum filter
        assert!(opportunity.matches_filter(&OpportunityFilter::high_momentum()));

        // Test specific filters
        assert!(opportunity.matches_filter(&OpportunityFilter {
            min_liquidity_usd: Some(20000.0),
            ..Default::default()
        }));

        assert!(!opportunity.matches_filter(&OpportunityFilter {
            min_liquidity_usd: Some(30000.0),
            ..Default::default()
        }));
    }

    #[test]
    fn test_filter_presets() {
        let new_tokens = OpportunityFilter::new_tokens();
        assert!(new_tokens.min_liquidity_usd.is_some());
        assert!(new_tokens.max_age_hours.is_some());

        let established = OpportunityFilter::established();
        assert!(established.min_age_hours.is_some());
        assert!(established.min_liquidity_usd.unwrap() > new_tokens.min_liquidity_usd.unwrap());
    }
}
