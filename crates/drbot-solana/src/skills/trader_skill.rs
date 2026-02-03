//! Autonomous trading skill.

use crate::discovery::{DexScreenerClient, GeckoTerminalClient};
use crate::trading::{
    JupiterClient, MomentumScorer, Position, PositionSummary, ScoredOpportunity, TradingEvent,
    TradingMonitor, TradingStrategyConfig,
};
use crate::wallet::KeypairManager;
use crate::{Result, SolanaError};
use async_trait::async_trait;
use drbot_skills::{
    ManifestCapability, ManifestInput, ManifestOutput, Skill, SkillContext, SkillInput,
    SkillManifest, SkillOutput,
};
use serde::{Deserialize, Serialize};
use solana_client::nonblocking::rpc_client::RpcClient;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Autonomous trading skill for Solana.
pub struct TraderSkill {
    manifest: SkillManifest,
    rpc_client: Arc<RpcClient>,
    jupiter_url: String,
    dexscreener_url: String,
    geckoterminal_url: String,
    keypair_manager: Arc<RwLock<Option<KeypairManager>>>,
    monitor: Arc<RwLock<Option<TradingMonitor>>>,
}

impl TraderSkill {
    /// Create a new trader skill.
    pub fn new(rpc_client: Arc<RpcClient>, keypair_manager: Option<KeypairManager>) -> Self {
        let manifest = SkillManifest {
            name: "trader".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            description: "Autonomous trading with momentum-based strategy, take profit, stop loss, and trailing stops".to_string(),
            author: Some("drbot".to_string()),
            license: Some("MIT".to_string()),
            homepage: None,
            repository: None,
            tags: vec![
                "solana".to_string(),
                "trading".to_string(),
                "momentum".to_string(),
                "automated".to_string(),
            ],
            inputs: vec![
                ManifestInput {
                    name: "action".to_string(),
                    description: "Action: start, stop, status, positions, history, summary, scan, close".to_string(),
                    param_type: "string".to_string(),
                    required: true,
                    default: None,
                    pattern: None,
                    enum_values: vec![],
                },
                ManifestInput {
                    name: "strategy".to_string(),
                    description: "Strategy preset: default, aggressive, conservative, scalping".to_string(),
                    param_type: "string".to_string(),
                    required: false,
                    default: Some(serde_json::json!("default")),
                    pattern: None,
                    enum_values: vec![],
                },
                ManifestInput {
                    name: "take_profit_pct".to_string(),
                    description: "Take profit percentage (overrides strategy preset)".to_string(),
                    param_type: "number".to_string(),
                    required: false,
                    default: None,
                    pattern: None,
                    enum_values: vec![],
                },
                ManifestInput {
                    name: "stop_loss_pct".to_string(),
                    description: "Stop loss percentage (overrides strategy preset)".to_string(),
                    param_type: "number".to_string(),
                    required: false,
                    default: None,
                    pattern: None,
                    enum_values: vec![],
                },
                ManifestInput {
                    name: "max_position_size_usd".to_string(),
                    description: "Maximum position size in USD".to_string(),
                    param_type: "number".to_string(),
                    required: false,
                    default: None,
                    pattern: None,
                    enum_values: vec![],
                },
                ManifestInput {
                    name: "position_id".to_string(),
                    description: "Position ID (for close action)".to_string(),
                    param_type: "string".to_string(),
                    required: false,
                    default: None,
                    pattern: None,
                    enum_values: vec![],
                },
                ManifestInput {
                    name: "limit".to_string(),
                    description: "Limit results (for scan action)".to_string(),
                    param_type: "number".to_string(),
                    required: false,
                    default: Some(serde_json::json!(10)),
                    pattern: None,
                    enum_values: vec![],
                },
            ],
            outputs: vec![
                ManifestOutput {
                    name: "result".to_string(),
                    description: "Operation result".to_string(),
                    output_type: "object".to_string(),
                },
            ],
            capabilities: vec![
                ManifestCapability::required("blockchain"),
                ManifestCapability::required("defi"),
                ManifestCapability::required("trading"),
                ManifestCapability::required("autonomous"),
            ],
            entry_point: None,
            runtime: None,
        };

        Self {
            manifest,
            rpc_client,
            jupiter_url: "https://quote-api.jup.ag/v6".to_string(),
            dexscreener_url: "https://api.dexscreener.com".to_string(),
            geckoterminal_url: "https://api.geckoterminal.com/api/v2".to_string(),
            keypair_manager: Arc::new(RwLock::new(keypair_manager)),
            monitor: Arc::new(RwLock::new(None)),
        }
    }

    /// Create a trading monitor with the given config.
    async fn create_monitor(&self, config: TradingStrategyConfig) -> Result<TradingMonitor> {
        let keypair_manager = self.keypair_manager.write().await.take().ok_or_else(|| {
            SolanaError::ConfigError("No keypair configured or already in use".to_string())
        })?;

        let jupiter = JupiterClient::new(self.jupiter_url.clone());
        let dexscreener = DexScreenerClient::new(self.dexscreener_url.clone());
        let geckoterminal = GeckoTerminalClient::new(self.geckoterminal_url.clone());

        Ok(TradingMonitor::new(
            config,
            self.rpc_client.clone(),
            jupiter,
            keypair_manager,
            dexscreener,
            geckoterminal,
        ))
    }
}

#[async_trait]
impl Skill for TraderSkill {
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
            "start" | "stop" | "status" | "positions" | "history" | "summary" | "scan" => Ok(()),
            "close" => {
                if input
                    .params
                    .get("position_id")
                    .and_then(|v| v.as_str())
                    .is_none()
                {
                    Err(drbot_skills::SkillError::ValidationFailed(
                        "close action requires position_id".to_string(),
                    ))
                } else {
                    Ok(())
                }
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
            .unwrap_or("status");

        match action {
            "start" => self.handle_start(&input).await,
            "stop" => self.handle_stop().await,
            "status" => self.handle_status().await,
            "positions" => self.handle_positions().await,
            "history" => self.handle_history().await,
            "summary" => self.handle_summary().await,
            "scan" => self.handle_scan(&input).await,
            "close" => self.handle_close(&input).await,
            _ => Err(drbot_skills::SkillError::ExecutionFailed(format!(
                "Unknown action: {}",
                action
            ))),
        }
    }
}

impl TraderSkill {
    async fn handle_start(&self, input: &SkillInput) -> drbot_skills::Result<SkillOutput> {
        // Check if already running
        {
            let monitor = self.monitor.read().await;
            if let Some(ref m) = *monitor {
                if m.is_running().await {
                    return Err(drbot_skills::SkillError::ExecutionFailed(
                        "Trading monitor already running".to_string(),
                    ));
                }
            }
        }

        // Build config from inputs
        let strategy = input
            .params
            .get("strategy")
            .and_then(|v| v.as_str())
            .unwrap_or("default");

        let mut config = match strategy {
            "aggressive" => TradingStrategyConfig::aggressive(),
            "conservative" => TradingStrategyConfig::conservative(),
            "scalping" => TradingStrategyConfig::scalping(),
            _ => TradingStrategyConfig::default(),
        };

        // Apply overrides
        if let Some(tp) = input.params.get("take_profit_pct").and_then(|v| v.as_f64()) {
            config.take_profit_pct = tp;
        }
        if let Some(sl) = input.params.get("stop_loss_pct").and_then(|v| v.as_f64()) {
            config.stop_loss_pct = sl;
        }
        if let Some(size) = input
            .params
            .get("max_position_size_usd")
            .and_then(|v| v.as_f64())
        {
            config.max_position_size_usd = size;
        }

        // Create and start monitor
        let new_monitor = self
            .create_monitor(config.clone())
            .await
            .map_err(|e| drbot_skills::SkillError::ExecutionFailed(e.to_string()))?;

        // Store monitor
        {
            let mut monitor = self.monitor.write().await;
            *monitor = Some(new_monitor);
        }

        // Start in background
        let monitor_clone = self.monitor.clone();
        tokio::spawn(async move {
            let monitor = monitor_clone.read().await;
            if let Some(ref m) = *monitor {
                let _ = m.start().await;
            }
        });

        let output = TraderStartOutput {
            status: "started".to_string(),
            strategy: strategy.to_string(),
            config: TraderConfigOutput {
                take_profit_pct: config.take_profit_pct,
                stop_loss_pct: config.stop_loss_pct,
                trailing_stop_enabled: config.trailing_stop_enabled,
                max_position_size_usd: config.max_position_size_usd,
                max_positions: config.max_positions,
                monitor_interval_secs: config.monitor_interval_secs,
            },
        };

        Ok(SkillOutput::new(serde_json::to_value(output).map_err(
            |e| drbot_skills::SkillError::ExecutionFailed(e.to_string()),
        )?))
    }

    async fn handle_stop(&self) -> drbot_skills::Result<SkillOutput> {
        let monitor = self.monitor.read().await;
        if let Some(ref m) = *monitor {
            m.stop().await;
        }

        let output = TraderStatusOutput {
            running: false,
            message: "Trading monitor stopped".to_string(),
            open_positions: 0,
            total_pnl_usd: 0.0,
        };

        Ok(SkillOutput::new(serde_json::to_value(output).map_err(
            |e| drbot_skills::SkillError::ExecutionFailed(e.to_string()),
        )?))
    }

    async fn handle_status(&self) -> drbot_skills::Result<SkillOutput> {
        let monitor = self.monitor.read().await;

        let (running, open_positions, total_pnl) = if let Some(ref m) = *monitor {
            let is_running = m.is_running().await;
            let positions = m.get_positions().await;
            let pnl: f64 = positions.iter().map(|p| p.pnl_usd).sum();
            (is_running, positions.len(), pnl)
        } else {
            (false, 0, 0.0)
        };

        let output = TraderStatusOutput {
            running,
            message: if running {
                "Trading monitor active"
            } else {
                "Trading monitor stopped"
            }
            .to_string(),
            open_positions,
            total_pnl_usd: total_pnl,
        };

        Ok(SkillOutput::new(serde_json::to_value(output).map_err(
            |e| drbot_skills::SkillError::ExecutionFailed(e.to_string()),
        )?))
    }

    async fn handle_positions(&self) -> drbot_skills::Result<SkillOutput> {
        let monitor = self.monitor.read().await;

        let positions = if let Some(ref m) = *monitor {
            m.get_positions().await
        } else {
            vec![]
        };

        let output: Vec<PositionOutput> = positions.into_iter().map(PositionOutput::from).collect();

        Ok(SkillOutput::new(serde_json::to_value(output).map_err(
            |e| drbot_skills::SkillError::ExecutionFailed(e.to_string()),
        )?))
    }

    async fn handle_history(&self) -> drbot_skills::Result<SkillOutput> {
        let monitor = self.monitor.read().await;

        let history = if let Some(ref m) = *monitor {
            m.get_history().await
        } else {
            vec![]
        };

        let output: Vec<PositionOutput> = history.into_iter().map(PositionOutput::from).collect();

        Ok(SkillOutput::new(serde_json::to_value(output).map_err(
            |e| drbot_skills::SkillError::ExecutionFailed(e.to_string()),
        )?))
    }

    async fn handle_summary(&self) -> drbot_skills::Result<SkillOutput> {
        let monitor = self.monitor.read().await;

        let summary = if let Some(ref m) = *monitor {
            m.get_summary().await
        } else {
            PositionSummary::from_positions(&[])
        };

        Ok(SkillOutput::new(serde_json::to_value(summary).map_err(
            |e| drbot_skills::SkillError::ExecutionFailed(e.to_string()),
        )?))
    }

    async fn handle_scan(&self, input: &SkillInput) -> drbot_skills::Result<SkillOutput> {
        let limit = input
            .params
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(10) as usize;

        // Create clients for scanning
        let dexscreener = DexScreenerClient::new(self.dexscreener_url.clone());
        let geckoterminal = GeckoTerminalClient::new(self.geckoterminal_url.clone());

        use crate::discovery::{OpportunityFilter, OpportunityFinder};
        let finder = OpportunityFinder::new(dexscreener, geckoterminal);

        let filter = OpportunityFilter::new_tokens();
        let opportunities = finder
            .find_all(&filter)
            .await
            .map_err(|e| drbot_skills::SkillError::ExecutionFailed(e.to_string()))?;

        let scorer = MomentumScorer::default();
        let mut scored: Vec<ScoredOpportunity> = opportunities
            .into_iter()
            .map(|opp| ScoredOpportunity::new(opp, &scorer))
            .collect();

        scored.sort_by(|a, b| b.score.total.partial_cmp(&a.score.total).unwrap());

        let output: Vec<ScanResultOutput> = scored
            .into_iter()
            .take(limit)
            .map(|s| ScanResultOutput {
                address: s.opportunity.address,
                symbol: s.opportunity.symbol,
                name: s.opportunity.name,
                price_usd: s.opportunity.price_usd,
                volume_24h: s.opportunity.volume_24h,
                liquidity_usd: s.opportunity.liquidity_usd,
                price_change_24h: s.opportunity.price_change_24h,
                age_hours: s.opportunity.age_hours,
                momentum_score: s.score.total,
                risk_level: format!("{}", s.risk_level),
                source: format!("{:?}", s.opportunity.source),
            })
            .collect();

        Ok(SkillOutput::new(serde_json::to_value(output).map_err(
            |e| drbot_skills::SkillError::ExecutionFailed(e.to_string()),
        )?))
    }

    async fn handle_close(&self, input: &SkillInput) -> drbot_skills::Result<SkillOutput> {
        let position_id = input
            .params
            .get("position_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                drbot_skills::SkillError::ValidationFailed("Missing position_id".to_string())
            })?;

        let monitor = self.monitor.read().await;

        if let Some(ref m) = *monitor {
            m.manual_close(position_id)
                .await
                .map_err(|e| drbot_skills::SkillError::ExecutionFailed(e.to_string()))?;

            Ok(SkillOutput::new(serde_json::json!({
                "status": "closed",
                "position_id": position_id,
            })))
        } else {
            Err(drbot_skills::SkillError::ExecutionFailed(
                "Trading monitor not running".to_string(),
            ))
        }
    }
}

/// Trader start output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraderStartOutput {
    pub status: String,
    pub strategy: String,
    pub config: TraderConfigOutput,
}

/// Trader config output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraderConfigOutput {
    pub take_profit_pct: f64,
    pub stop_loss_pct: f64,
    pub trailing_stop_enabled: bool,
    pub max_position_size_usd: f64,
    pub max_positions: usize,
    pub monitor_interval_secs: u64,
}

/// Trader status output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraderStatusOutput {
    pub running: bool,
    pub message: String,
    pub open_positions: usize,
    pub total_pnl_usd: f64,
}

/// Position output for API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionOutput {
    pub id: String,
    pub token_symbol: String,
    pub token_mint: String,
    pub entry_price: f64,
    pub current_price: f64,
    pub amount: f64,
    pub entry_value_usd: f64,
    pub current_value_usd: f64,
    pub pnl_pct: f64,
    pub pnl_usd: f64,
    pub duration: String,
    pub trailing_stop_active: bool,
    pub status: String,
}

impl From<Position> for PositionOutput {
    fn from(p: Position) -> Self {
        let duration = p.duration_str();
        let status = format!("{:?}", p.status);
        Self {
            id: p.id,
            token_symbol: p.token_symbol,
            token_mint: p.token_mint,
            entry_price: p.entry_price,
            current_price: p.current_price,
            amount: p.amount,
            entry_value_usd: p.entry_value_usd,
            current_value_usd: p.current_value_usd,
            pnl_pct: p.pnl_pct,
            pnl_usd: p.pnl_usd,
            duration,
            trailing_stop_active: p.trailing_stop_active,
            status,
        }
    }
}

/// Scan result output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanResultOutput {
    pub address: String,
    pub symbol: String,
    pub name: String,
    pub price_usd: f64,
    pub volume_24h: f64,
    pub liquidity_usd: f64,
    pub price_change_24h: f64,
    pub age_hours: Option<f64>,
    pub momentum_score: f64,
    pub risk_level: String,
    pub source: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trader_skill_manifest() {
        let rpc = Arc::new(RpcClient::new("https://api.devnet.solana.com".to_string()));
        let skill = TraderSkill::new(rpc, None);
        let manifest = skill.manifest();

        assert_eq!(manifest.name, "trader");
        assert!(manifest.inputs.iter().any(|i| i.name == "action"));
        assert!(manifest.inputs.iter().any(|i| i.name == "strategy"));
    }
}
