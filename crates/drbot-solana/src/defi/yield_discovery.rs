//! Yield opportunity discovery and aggregation.
//!
//! Aggregates yield opportunities across multiple DeFi protocols
//! and provides ranking and filtering capabilities.

use super::protocols::{
    DeFiProtocol, JitoClient, KaminoClient, MarginfiClient, MarinadeClient, Position, ProtocolType,
    SolendClient, YieldOpportunity,
};
use crate::{Result, SolanaError};
use serde::{Deserialize, Serialize};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Yield aggregator that discovers opportunities across protocols.
pub struct YieldAggregator {
    protocols: Vec<Box<dyn DeFiProtocol>>,
}

impl YieldAggregator {
    /// Create a new yield aggregator with default protocols.
    pub fn new(rpc_client: Arc<RpcClient>) -> Self {
        let protocols: Vec<Box<dyn DeFiProtocol>> = vec![
            Box::new(SolendClient::new(rpc_client.clone())),
            Box::new(MarginfiClient::new(rpc_client.clone())),
            Box::new(KaminoClient::new(rpc_client.clone())),
            Box::new(MarinadeClient::new(rpc_client.clone())),
            Box::new(JitoClient::new(rpc_client)),
        ];

        Self { protocols }
    }

    /// Create with custom protocols.
    pub fn with_protocols(protocols: Vec<Box<dyn DeFiProtocol>>) -> Self {
        Self { protocols }
    }

    /// Discover all yield opportunities across protocols.
    pub async fn discover_all(&self) -> Result<Vec<YieldOpportunity>> {
        let mut all_opportunities = Vec::new();

        for protocol in &self.protocols {
            match protocol.get_opportunities().await {
                Ok(opportunities) => {
                    debug!(
                        protocol = protocol.name(),
                        count = opportunities.len(),
                        "Discovered opportunities"
                    );
                    all_opportunities.extend(opportunities);
                }
                Err(e) => {
                    warn!(
                        protocol = protocol.name(),
                        error = %e,
                        "Failed to fetch opportunities"
                    );
                }
            }
        }

        info!(
            total = all_opportunities.len(),
            "Discovered total opportunities"
        );

        Ok(all_opportunities)
    }

    /// Discover opportunities with filtering.
    pub async fn discover(&self, filter: &YieldFilter) -> Result<Vec<YieldOpportunity>> {
        let mut opportunities = self.discover_all().await?;

        // Apply filters
        opportunities.retain(|opp| {
            // Filter by min APY
            if let Some(min_apy) = filter.min_apy {
                if opp.apy < min_apy {
                    return false;
                }
            }

            // Filter by max risk
            if let Some(max_risk) = filter.max_risk_score {
                if opp.risk_score > max_risk {
                    return false;
                }
            }

            // Filter by min TVL
            if let Some(min_tvl) = filter.min_tvl_usd {
                if opp.tvl_usd < min_tvl {
                    return false;
                }
            }

            // Filter by protocol type
            if let Some(ref protocols) = filter.protocols {
                if !protocols.contains(&opp.protocol) {
                    return false;
                }
            }

            // Filter by asset
            if let Some(ref assets) = filter.assets {
                if !assets.iter().any(|a| opp.asset.contains(a)) {
                    return false;
                }
            }

            true
        });

        // Sort by the specified criteria
        match filter.sort_by {
            SortBy::ApyDesc => {
                opportunities.sort_by(|a, b| {
                    b.apy
                        .partial_cmp(&a.apy)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
            }
            SortBy::ApyAsc => {
                opportunities.sort_by(|a, b| {
                    a.apy
                        .partial_cmp(&b.apy)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
            }
            SortBy::RiskAsc => {
                opportunities.sort_by_key(|o| o.risk_score);
            }
            SortBy::RiskDesc => {
                opportunities.sort_by_key(|o| std::cmp::Reverse(o.risk_score));
            }
            SortBy::TvlDesc => {
                opportunities.sort_by(|a, b| {
                    b.tvl_usd
                        .partial_cmp(&a.tvl_usd)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
            }
            SortBy::RiskAdjustedReturn => {
                // Sharpe-like ratio: APY / risk_score
                opportunities.sort_by(|a, b| {
                    let ratio_a = a.apy / (a.risk_score as f64);
                    let ratio_b = b.apy / (b.risk_score as f64);
                    ratio_b
                        .partial_cmp(&ratio_a)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
            }
        }

        // Apply limit
        if let Some(limit) = filter.limit {
            opportunities.truncate(limit);
        }

        Ok(opportunities)
    }

    /// Get the best opportunity for a given asset.
    pub async fn best_for_asset(&self, asset: &str) -> Result<Option<YieldOpportunity>> {
        let filter = YieldFilter::default()
            .with_assets(vec![asset.to_string()])
            .with_sort(SortBy::RiskAdjustedReturn)
            .with_limit(1);

        let opportunities = self.discover(&filter).await?;
        Ok(opportunities.into_iter().next())
    }

    /// Get opportunities grouped by protocol.
    pub async fn by_protocol(&self) -> Result<HashMap<String, Vec<YieldOpportunity>>> {
        let opportunities = self.discover_all().await?;

        let mut grouped: HashMap<String, Vec<YieldOpportunity>> = HashMap::new();
        for opp in opportunities {
            grouped.entry(opp.protocol.clone()).or_default().push(opp);
        }

        Ok(grouped)
    }

    /// Get all user positions across protocols.
    pub async fn get_all_positions(&self, user: &Pubkey) -> Result<Vec<Position>> {
        let mut all_positions = Vec::new();

        for protocol in &self.protocols {
            match protocol.get_positions(user).await {
                Ok(positions) => {
                    debug!(
                        protocol = protocol.name(),
                        count = positions.len(),
                        "Fetched positions"
                    );
                    all_positions.extend(positions);
                }
                Err(e) => {
                    warn!(
                        protocol = protocol.name(),
                        error = %e,
                        "Failed to fetch positions"
                    );
                }
            }
        }

        Ok(all_positions)
    }

    /// Get total portfolio value across all protocols.
    pub async fn get_portfolio_value(&self, user: &Pubkey) -> Result<PortfolioSummary> {
        let positions = self.get_all_positions(user).await?;

        let total_value: f64 = positions.iter().map(|p| p.usd_value).sum();
        let weighted_apy: f64 = if total_value > 0.0 {
            positions
                .iter()
                .map(|p| p.current_apy * p.usd_value)
                .sum::<f64>()
                / total_value
        } else {
            0.0
        };

        let mut by_protocol: HashMap<String, f64> = HashMap::new();
        for pos in &positions {
            *by_protocol.entry(pos.protocol.clone()).or_default() += pos.usd_value;
        }

        Ok(PortfolioSummary {
            total_value_usd: total_value,
            weighted_avg_apy: weighted_apy,
            position_count: positions.len(),
            value_by_protocol: by_protocol,
            positions,
        })
    }
}

/// Filter for yield discovery.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct YieldFilter {
    /// Minimum APY to include.
    pub min_apy: Option<f64>,
    /// Maximum risk score to include.
    pub max_risk_score: Option<u8>,
    /// Minimum TVL in USD.
    pub min_tvl_usd: Option<f64>,
    /// Filter to specific protocols.
    pub protocols: Option<Vec<String>>,
    /// Filter to specific assets.
    pub assets: Option<Vec<String>>,
    /// Sort order.
    #[serde(default)]
    pub sort_by: SortBy,
    /// Maximum results to return.
    pub limit: Option<usize>,
}

impl YieldFilter {
    /// Set minimum APY.
    pub fn with_min_apy(mut self, apy: f64) -> Self {
        self.min_apy = Some(apy);
        self
    }

    /// Set maximum risk score.
    pub fn with_max_risk(mut self, risk: u8) -> Self {
        self.max_risk_score = Some(risk);
        self
    }

    /// Set minimum TVL.
    pub fn with_min_tvl(mut self, tvl: f64) -> Self {
        self.min_tvl_usd = Some(tvl);
        self
    }

    /// Filter by protocols.
    pub fn with_protocols(mut self, protocols: Vec<String>) -> Self {
        self.protocols = Some(protocols);
        self
    }

    /// Filter by assets.
    pub fn with_assets(mut self, assets: Vec<String>) -> Self {
        self.assets = Some(assets);
        self
    }

    /// Set sort order.
    pub fn with_sort(mut self, sort: SortBy) -> Self {
        self.sort_by = sort;
        self
    }

    /// Set limit.
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }
}

/// Sort criteria for opportunities.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortBy {
    /// Sort by APY descending (highest first).
    #[default]
    ApyDesc,
    /// Sort by APY ascending (lowest first).
    ApyAsc,
    /// Sort by risk ascending (safest first).
    RiskAsc,
    /// Sort by risk descending (riskiest first).
    RiskDesc,
    /// Sort by TVL descending (largest first).
    TvlDesc,
    /// Sort by risk-adjusted return (APY/risk ratio).
    RiskAdjustedReturn,
}

/// Portfolio summary across all protocols.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioSummary {
    /// Total portfolio value in USD.
    pub total_value_usd: f64,
    /// Weighted average APY across positions.
    pub weighted_avg_apy: f64,
    /// Number of positions.
    pub position_count: usize,
    /// Value by protocol.
    pub value_by_protocol: HashMap<String, f64>,
    /// All positions.
    pub positions: Vec<Position>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_yield_filter() {
        let filter = YieldFilter::default()
            .with_min_apy(0.05)
            .with_max_risk(5)
            .with_limit(10);

        assert_eq!(filter.min_apy, Some(0.05));
        assert_eq!(filter.max_risk_score, Some(5));
        assert_eq!(filter.limit, Some(10));
    }

    #[test]
    fn test_sort_by_default() {
        let filter = YieldFilter::default();
        assert!(matches!(filter.sort_by, SortBy::ApyDesc));
    }
}
