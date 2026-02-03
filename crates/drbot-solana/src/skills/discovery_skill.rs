//! Discovery skill for finding trading opportunities.

use crate::discovery::{
    DexScreenerClient, GeckoTerminalClient, OpportunityFilter, OpportunityFinder,
    OpportunitySource, TokenOpportunity,
};
use async_trait::async_trait;
use drbot_skills::{
    ManifestCapability, ManifestInput, ManifestOutput, Skill, SkillContext, SkillInput,
    SkillManifest, SkillOutput,
};
use serde::{Deserialize, Serialize};

/// Discovery skill for finding trading opportunities.
pub struct DiscoverySkill {
    manifest: SkillManifest,
    dexscreener: DexScreenerClient,
    geckoterminal: GeckoTerminalClient,
}

impl DiscoverySkill {
    /// Create a new discovery skill.
    pub fn new(dexscreener: DexScreenerClient, geckoterminal: GeckoTerminalClient) -> Self {
        let manifest = SkillManifest {
            name: "discover".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            description: "Find trading opportunities on Solana via DexScreener and GeckoTerminal"
                .to_string(),
            author: Some("drbot".to_string()),
            license: Some("MIT".to_string()),
            homepage: None,
            repository: None,
            tags: vec![
                "solana".to_string(),
                "discovery".to_string(),
                "dexscreener".to_string(),
                "geckoterminal".to_string(),
            ],
            inputs: vec![
                ManifestInput {
                    name: "source".to_string(),
                    description: "Data source: dexscreener, geckoterminal, or both".to_string(),
                    param_type: "string".to_string(),
                    required: false,
                    default: Some(serde_json::json!("both")),
                    pattern: None,
                    enum_values: vec![],
                },
                ManifestInput {
                    name: "filter".to_string(),
                    description: "Filter preset: new_tokens, established, high_momentum, or custom"
                        .to_string(),
                    param_type: "string".to_string(),
                    required: false,
                    default: Some(serde_json::json!("new_tokens")),
                    pattern: None,
                    enum_values: vec![],
                },
                ManifestInput {
                    name: "min_liquidity_usd".to_string(),
                    description: "Minimum liquidity in USD".to_string(),
                    param_type: "number".to_string(),
                    required: false,
                    default: None,
                    pattern: None,
                    enum_values: vec![],
                },
                ManifestInput {
                    name: "min_volume_24h".to_string(),
                    description: "Minimum 24h volume in USD".to_string(),
                    param_type: "number".to_string(),
                    required: false,
                    default: None,
                    pattern: None,
                    enum_values: vec![],
                },
                ManifestInput {
                    name: "max_age_hours".to_string(),
                    description: "Maximum token/pool age in hours".to_string(),
                    param_type: "number".to_string(),
                    required: false,
                    default: None,
                    pattern: None,
                    enum_values: vec![],
                },
                ManifestInput {
                    name: "query".to_string(),
                    description: "Search query for finding specific tokens".to_string(),
                    param_type: "string".to_string(),
                    required: false,
                    default: None,
                    pattern: None,
                    enum_values: vec![],
                },
                ManifestInput {
                    name: "limit".to_string(),
                    description: "Maximum number of results to return".to_string(),
                    param_type: "number".to_string(),
                    required: false,
                    default: Some(serde_json::json!(20)),
                    pattern: None,
                    enum_values: vec![],
                },
            ],
            outputs: vec![ManifestOutput {
                name: "opportunities".to_string(),
                description: "List of trading opportunities".to_string(),
                output_type: "array".to_string(),
            }],
            capabilities: vec![
                ManifestCapability::required("blockchain"),
                ManifestCapability::required("defi"),
                ManifestCapability::required("research"),
            ],
            entry_point: None,
            runtime: None,
        };

        Self {
            manifest,
            dexscreener,
            geckoterminal,
        }
    }
}

#[async_trait]
impl Skill for DiscoverySkill {
    fn manifest(&self) -> &SkillManifest {
        &self.manifest
    }

    fn validate_input(&self, input: &SkillInput) -> drbot_skills::Result<()> {
        if let Some(source) = input.params.get("source").and_then(|v| v.as_str()) {
            match source {
                "dexscreener" | "geckoterminal" | "both" => {}
                _ => {
                    return Err(drbot_skills::SkillError::ValidationFailed(format!(
                        "Invalid source: {}. Use 'dexscreener', 'geckoterminal', or 'both'",
                        source
                    )));
                }
            }
        }

        if let Some(filter) = input.params.get("filter").and_then(|v| v.as_str()) {
            match filter {
                "new_tokens" | "established" | "high_momentum" | "custom" => {}
                _ => {
                    return Err(drbot_skills::SkillError::ValidationFailed(
                        format!("Invalid filter preset: {}. Use 'new_tokens', 'established', 'high_momentum', or 'custom'", filter),
                    ));
                }
            }
        }

        Ok(())
    }

    async fn execute(
        &self,
        input: SkillInput,
        _context: &SkillContext,
    ) -> drbot_skills::Result<SkillOutput> {
        let source = input
            .params
            .get("source")
            .and_then(|v| v.as_str())
            .unwrap_or("both");

        let filter_preset = input
            .params
            .get("filter")
            .and_then(|v| v.as_str())
            .unwrap_or("new_tokens");

        let limit = input
            .params
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(20) as usize;

        // Build filter
        let mut filter = match filter_preset {
            "new_tokens" => OpportunityFilter::new_tokens(),
            "established" => OpportunityFilter::established(),
            "high_momentum" => OpportunityFilter::high_momentum(),
            _ => OpportunityFilter::default(),
        };

        // Apply custom filter values if provided
        if let Some(min_liq) = input
            .params
            .get("min_liquidity_usd")
            .and_then(|v| v.as_f64())
        {
            filter.min_liquidity_usd = Some(min_liq);
        }
        if let Some(min_vol) = input.params.get("min_volume_24h").and_then(|v| v.as_f64()) {
            filter.min_volume_24h = Some(min_vol);
        }
        if let Some(max_age) = input.params.get("max_age_hours").and_then(|v| v.as_f64()) {
            filter.max_age_hours = Some(max_age);
        }

        let finder = OpportunityFinder::new(
            DexScreenerClient::new(self.dexscreener.base_url().to_string()),
            GeckoTerminalClient::new(self.geckoterminal.base_url().to_string()),
        );

        let opportunities = if let Some(query) = input.params.get("query").and_then(|v| v.as_str())
        {
            finder
                .search(query, &filter)
                .await
                .map_err(|e| drbot_skills::SkillError::ExecutionFailed(e.to_string()))?
        } else {
            match source {
                "dexscreener" => finder
                    .find_from_dexscreener(&filter)
                    .await
                    .map_err(|e| drbot_skills::SkillError::ExecutionFailed(e.to_string()))?,
                "geckoterminal" => finder
                    .find_from_geckoterminal(&filter)
                    .await
                    .map_err(|e| drbot_skills::SkillError::ExecutionFailed(e.to_string()))?,
                _ => finder
                    .find_all(&filter)
                    .await
                    .map_err(|e| drbot_skills::SkillError::ExecutionFailed(e.to_string()))?,
            }
        };

        let limited: Vec<_> = opportunities.into_iter().take(limit).collect();

        let output = DiscoveryOutput {
            count: limited.len(),
            filter_used: filter_preset.to_string(),
            source: source.to_string(),
            opportunities: limited.into_iter().map(OpportunityOutput::from).collect(),
        };

        Ok(SkillOutput::new(serde_json::to_value(output).map_err(
            |e| drbot_skills::SkillError::ExecutionFailed(e.to_string()),
        )?))
    }
}

impl DexScreenerClient {
    fn base_url(&self) -> &str {
        // Hacky way to get base URL - in real impl, store it
        "https://api.dexscreener.com"
    }
}

impl GeckoTerminalClient {
    fn base_url(&self) -> &str {
        "https://api.geckoterminal.com/api/v2"
    }
}

/// Discovery output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryOutput {
    /// Number of opportunities found.
    pub count: usize,
    /// Filter preset used.
    pub filter_used: String,
    /// Data source.
    pub source: String,
    /// List of opportunities.
    pub opportunities: Vec<OpportunityOutput>,
}

/// Opportunity output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpportunityOutput {
    /// Token address.
    pub address: String,
    /// Token symbol.
    pub symbol: String,
    /// Token name.
    pub name: String,
    /// Price in USD.
    pub price_usd: f64,
    /// 24h volume in USD.
    pub volume_24h: f64,
    /// Liquidity in USD.
    pub liquidity_usd: f64,
    /// 24h price change percentage.
    pub price_change_24h: f64,
    /// Age in hours.
    pub age_hours: Option<f64>,
    /// Data source.
    pub source: String,
    /// DEX name.
    pub dex: String,
    /// Market cap.
    pub market_cap: Option<f64>,
    /// FDV.
    pub fdv: Option<f64>,
    /// URL to view.
    pub url: Option<String>,
}

impl From<TokenOpportunity> for OpportunityOutput {
    fn from(op: TokenOpportunity) -> Self {
        Self {
            address: op.address,
            symbol: op.symbol,
            name: op.name,
            price_usd: op.price_usd,
            volume_24h: op.volume_24h,
            liquidity_usd: op.liquidity_usd,
            price_change_24h: op.price_change_24h,
            age_hours: op.age_hours,
            source: match op.source {
                OpportunitySource::DexScreener => "dexscreener".to_string(),
                OpportunitySource::GeckoTerminal => "geckoterminal".to_string(),
            },
            dex: op.dex,
            market_cap: op.market_cap,
            fdv: op.fdv,
            url: op.url,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discovery_skill_manifest() {
        let dexscreener = DexScreenerClient::new("https://api.dexscreener.com".to_string());
        let geckoterminal =
            GeckoTerminalClient::new("https://api.geckoterminal.com/api/v2".to_string());
        let skill = DiscoverySkill::new(dexscreener, geckoterminal);
        let manifest = skill.manifest();

        assert_eq!(manifest.name, "discover");
        assert!(manifest.inputs.iter().any(|i| i.name == "source"));
        assert!(manifest.inputs.iter().any(|i| i.name == "filter"));
    }
}
