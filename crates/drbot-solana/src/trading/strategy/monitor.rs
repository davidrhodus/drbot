//! Trading monitor for autonomous execution.

use super::{
    MomentumScore, MomentumScorer, Position, PositionSummary, RiskLevel, ScoredOpportunity,
    TradingAction, TradingStrategyConfig, TrailingStopManager,
};
use crate::discovery::{
    DexScreenerClient, GeckoTerminalClient, OpportunityFilter, OpportunityFinder, TokenOpportunity,
};
use crate::trading::{JupiterClient, SwapExecutor, SwapExecutorConfig};
use crate::wallet::{BalanceQuery, KeypairManager};
use crate::{Result, SolanaError};
use chrono::{DateTime, Utc};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, RwLock};
use tracing::{debug, error, info, warn};

/// Trading monitor for autonomous strategy execution.
pub struct TradingMonitor {
    /// Configuration.
    config: TradingStrategyConfig,
    /// RPC client.
    rpc_client: Arc<RpcClient>,
    /// Swap executor.
    swap_executor: SwapExecutor,
    /// Keypair manager.
    keypair_manager: KeypairManager,
    /// Opportunity finder.
    opportunity_finder: OpportunityFinder,
    /// Momentum scorer.
    momentum_scorer: MomentumScorer,
    /// Trailing stop manager.
    trailing_stop_manager: TrailingStopManager,
    /// Open positions.
    positions: Arc<RwLock<HashMap<String, Position>>>,
    /// Position history.
    history: Arc<RwLock<Vec<Position>>>,
    /// Running state.
    running: Arc<RwLock<bool>>,
    /// Event sender.
    event_tx: broadcast::Sender<TradingEvent>,
    /// Cooldown tracking (token_mint -> last_exit_time).
    cooldowns: Arc<RwLock<HashMap<String, DateTime<Utc>>>>,
    /// SOL balance in lamports.
    sol_balance: Arc<RwLock<u64>>,
    /// USDC balance (in smallest units).
    usdc_balance: Arc<RwLock<u64>>,
}

impl TradingMonitor {
    /// Create a new trading monitor.
    pub fn new(
        config: TradingStrategyConfig,
        rpc_client: Arc<RpcClient>,
        jupiter: JupiterClient,
        keypair_manager: KeypairManager,
        dexscreener: DexScreenerClient,
        geckoterminal: GeckoTerminalClient,
    ) -> Self {
        let swap_executor = SwapExecutor::new(
            rpc_client.clone(),
            jupiter,
            SwapExecutorConfig::from(&config),
        );

        let trailing_stop_manager = TrailingStopManager::from_config(&config);
        let opportunity_finder = OpportunityFinder::new(dexscreener, geckoterminal);
        let (event_tx, _) = broadcast::channel(256);

        Self {
            config,
            rpc_client,
            swap_executor,
            keypair_manager,
            opportunity_finder,
            momentum_scorer: MomentumScorer::default(),
            trailing_stop_manager,
            positions: Arc::new(RwLock::new(HashMap::new())),
            history: Arc::new(RwLock::new(Vec::new())),
            running: Arc::new(RwLock::new(false)),
            event_tx,
            cooldowns: Arc::new(RwLock::new(HashMap::new())),
            sol_balance: Arc::new(RwLock::new(0)),
            usdc_balance: Arc::new(RwLock::new(0)),
        }
    }

    /// Subscribe to trading events.
    pub fn subscribe(&self) -> broadcast::Receiver<TradingEvent> {
        self.event_tx.subscribe()
    }

    /// Start the trading monitor.
    pub async fn start(&self) -> Result<()> {
        // Validate config
        self.config
            .validate()
            .map_err(|e| SolanaError::ConfigError(e))?;

        let mut running = self.running.write().await;
        if *running {
            return Err(SolanaError::ConfigError(
                "Monitor already running".to_string(),
            ));
        }
        *running = true;
        drop(running);

        info!(
            take_profit = self.config.take_profit_pct,
            stop_loss = self.config.stop_loss_pct,
            trailing_stop = self.config.trailing_stop_enabled,
            max_positions = self.config.max_positions,
            "Starting trading monitor"
        );

        self.emit_event(TradingEvent::MonitorStarted {
            config: self.config.clone(),
        });

        // Main monitoring loop
        loop {
            if !*self.running.read().await {
                break;
            }

            // Run one monitoring cycle
            if let Err(e) = self.monitor_cycle().await {
                error!(error = %e, "Monitor cycle error");
                self.emit_event(TradingEvent::Error {
                    message: e.to_string(),
                });
            }

            // Wait for next cycle
            tokio::time::sleep(Duration::from_secs(self.config.monitor_interval_secs)).await;
        }

        info!("Trading monitor stopped");
        self.emit_event(TradingEvent::MonitorStopped);

        Ok(())
    }

    /// Stop the trading monitor.
    pub async fn stop(&self) {
        let mut running = self.running.write().await;
        *running = false;
    }

    /// Check if monitor is running.
    pub async fn is_running(&self) -> bool {
        *self.running.read().await
    }

    /// Run one monitoring cycle.
    async fn monitor_cycle(&self) -> Result<()> {
        // 1. Update prices and check existing positions
        self.check_positions().await?;

        // 2. SOL swing trading (if enabled)
        if self.config.sol_swing_enabled {
            self.check_sol_swing().await?;
        }

        // 3. Look for new opportunities if we have room
        let positions = self.positions.read().await;
        let open_count = positions.len();
        drop(positions);

        if open_count < self.config.max_positions {
            self.scan_opportunities().await?;
        }

        Ok(())
    }

    /// Check SOL swing trading conditions.
    async fn check_sol_swing(&self) -> Result<()> {
        // Get SOL 24h price change
        let sol_info = self.get_sol_price_info().await?;
        let sol_24h_change = sol_info.1;

        info!(
            sol_price = sol_info.0,
            change_24h = sol_24h_change,
            "Checking SOL swing conditions"
        );

        // Buy on dip
        if sol_24h_change < self.config.sol_buy_dip_threshold {
            info!(
                change = sol_24h_change,
                threshold = self.config.sol_buy_dip_threshold,
                "SOL dip detected, attempting to buy"
            );

            if let Err(e) = self.execute_sol_swing_buy().await {
                warn!(error = %e, "SOL swing buy failed");
            }
        }
        // Sell on pump
        else if sol_24h_change > self.config.sol_sell_pump_threshold {
            info!(
                change = sol_24h_change,
                threshold = self.config.sol_sell_pump_threshold,
                "SOL pump detected, attempting to sell"
            );

            if let Err(e) = self.execute_sol_swing_sell().await {
                warn!(error = %e, "SOL swing sell failed");
            }
        }

        Ok(())
    }

    /// Execute SOL swing buy (buy SOL on dip).
    async fn execute_sol_swing_buy(&self) -> Result<()> {
        use solana_sdk::signature::Signer;
        let keypair = self.keypair_manager.load_keypair().await?;

        // Check USDC balance
        let balance_query = BalanceQuery::new(self.rpc_client.clone());
        let usdc_mint = Pubkey::from_str("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v")?;
        let usdc_balance = balance_query
            .get_token_balance(&keypair.pubkey(), &usdc_mint)
            .await
            .map(|tb| tb.balance)
            .unwrap_or(0);
        let usdc_balance_usd = usdc_balance as f64 / 1_000_000.0;

        // Calculate buy amount (max of config limit or 25% of balance)
        let buy_amount_usd = self.config.sol_swing_max_usd.min(usdc_balance_usd * 0.25);

        if buy_amount_usd < 1.0 {
            debug!("Insufficient USDC for SOL swing buy");
            return Ok(());
        }

        let buy_amount_usdc = (buy_amount_usd * 1_000_000.0) as u64;
        let sol_mint = Pubkey::from_str("So11111111111111111111111111111111111111112")?;

        info!(amount_usd = buy_amount_usd, "Executing SOL swing buy");

        let result = self
            .swap_executor
            .swap(
                &keypair,
                usdc_mint,
                sol_mint,
                buy_amount_usdc,
                Some(self.config.slippage_bps),
            )
            .await?;

        self.emit_event(TradingEvent::SolSwingExecuted {
            action: "buy".to_string(),
            amount_usd: buy_amount_usd,
            signature: result.signature.to_string(),
        });

        info!(
            signature = %result.signature,
            amount_usd = buy_amount_usd,
            "SOL swing buy executed"
        );

        Ok(())
    }

    /// Execute SOL swing sell (sell SOL on pump).
    async fn execute_sol_swing_sell(&self) -> Result<()> {
        use solana_sdk::signature::Signer;
        let keypair = self.keypair_manager.load_keypair().await?;

        // Check SOL balance
        let sol_balance = self.rpc_client.get_balance(&keypair.pubkey()).await?;
        let sol_balance_amount = sol_balance as f64 / 1_000_000_000.0;

        // Don't sell below minimum balance
        if sol_balance_amount <= self.config.sol_min_balance {
            debug!(
                balance = sol_balance_amount,
                min = self.config.sol_min_balance,
                "SOL balance at minimum, skipping swing sell"
            );
            return Ok(());
        }

        // Calculate sell amount (% of holdings above minimum)
        let sellable = sol_balance_amount - self.config.sol_min_balance;
        let sell_amount = sellable * (self.config.sol_sell_pct / 100.0);

        if sell_amount < 0.01 {
            debug!("Sell amount too small, skipping");
            return Ok(());
        }

        let sell_lamports = (sell_amount * 1_000_000_000.0) as u64;
        let sol_mint = Pubkey::from_str("So11111111111111111111111111111111111111112")?;
        let usdc_mint = Pubkey::from_str("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v")?;

        let sol_price = self.get_sol_price().await?;
        let sell_value_usd = sell_amount * sol_price;

        info!(
            amount_sol = sell_amount,
            value_usd = sell_value_usd,
            "Executing SOL swing sell"
        );

        let result = self
            .swap_executor
            .swap(
                &keypair,
                sol_mint,
                usdc_mint,
                sell_lamports,
                Some(self.config.slippage_bps),
            )
            .await?;

        self.emit_event(TradingEvent::SolSwingExecuted {
            action: "sell".to_string(),
            amount_usd: sell_value_usd,
            signature: result.signature.to_string(),
        });

        info!(
            signature = %result.signature,
            amount_sol = sell_amount,
            value_usd = sell_value_usd,
            "SOL swing sell executed"
        );

        Ok(())
    }

    /// Get SOL price and 24h change.
    async fn get_sol_price_info(&self) -> Result<(f64, f64)> {
        let dex_client = DexScreenerClient::new("https://api.dexscreener.com".to_string());
        let pairs = dex_client
            .get_token_pairs("So11111111111111111111111111111111111111112")
            .await?;

        // Find SOL/USDC pair
        let pair = pairs
            .iter()
            .find(|p| p.quote_token.symbol.to_uppercase() == "USDC")
            .ok_or_else(|| SolanaError::JupiterError("No SOL/USDC pair found".to_string()))?;

        Ok((pair.price(), pair.price_change_24h()))
    }

    /// Check existing positions for exit conditions.
    async fn check_positions(&self) -> Result<()> {
        let mut positions = self.positions.write().await;
        let mut to_close = Vec::new();

        for (id, position) in positions.iter_mut() {
            // Get price with momentum data
            if let Ok((price, m5, h1)) = self
                .get_current_price_with_momentum(&position.token_mint)
                .await
            {
                position.update_price_with_momentum(price, m5, h1);

                // Check exit conditions
                let action = self.evaluate_position(position);

                match action {
                    TradingAction::Hold => {
                        debug!(
                            token = %position.token_symbol,
                            pnl_pct = position.pnl_pct,
                            m5 = m5,
                            h1 = h1,
                            "Holding position"
                        );
                    }
                    TradingAction::TakeProfit => {
                        info!(
                            token = %position.token_symbol,
                            pnl_pct = position.pnl_pct,
                            "Take profit triggered"
                        );
                        to_close.push((id.clone(), action));
                    }
                    TradingAction::StopLoss => {
                        warn!(
                            token = %position.token_symbol,
                            pnl_pct = position.pnl_pct,
                            "Stop loss triggered"
                        );
                        to_close.push((id.clone(), action));
                    }
                    TradingAction::TrailingStop => {
                        info!(
                            token = %position.token_symbol,
                            pnl_pct = position.pnl_pct,
                            peak_price = position.peak_price,
                            "Trailing stop triggered"
                        );
                        to_close.push((id.clone(), action));
                    }
                    TradingAction::MomentumDeath => {
                        warn!(
                            token = %position.token_symbol,
                            pnl_pct = position.pnl_pct,
                            m5 = m5,
                            h1 = h1,
                            "Momentum death triggered"
                        );
                        to_close.push((id.clone(), action));
                    }
                    TradingAction::ManualExit => {
                        to_close.push((id.clone(), action));
                    }
                }
            } else {
                warn!(token = %position.token_symbol, "Failed to get price");
            }
        }
        drop(positions);

        // Close positions that triggered exit
        for (id, action) in to_close {
            self.close_position(&id, action).await?;
        }

        Ok(())
    }

    /// Evaluate a position for exit conditions.
    fn evaluate_position(&self, position: &mut Position) -> TradingAction {
        let pnl_pct = position.pnl_pct;

        // Check take profit
        if pnl_pct >= self.config.take_profit_pct {
            return TradingAction::TakeProfit;
        }

        // Check stop loss
        if pnl_pct <= -self.config.stop_loss_pct {
            return TradingAction::StopLoss;
        }

        // Check trailing stop
        if self.config.trailing_stop_enabled {
            let action = self.trailing_stop_manager.check(position);
            if action != TradingAction::Hold {
                return action;
            }
        }

        // Check momentum death (5m <-8% AND 1h <-15% by default)
        if self.config.momentum_death_enabled {
            if position.is_momentum_dead(
                self.config.momentum_death_5m_threshold,
                self.config.momentum_death_1h_threshold,
            ) {
                return TradingAction::MomentumDeath;
            }
        }

        TradingAction::Hold
    }

    /// Scan for new trading opportunities.
    async fn scan_opportunities(&self) -> Result<()> {
        let filter = OpportunityFilter {
            min_liquidity_usd: Some(self.config.min_liquidity_usd),
            min_volume_24h: Some(self.config.min_volume_24h_usd),
            max_age_hours: self.config.max_token_age_hours,
            ..Default::default()
        };

        let opportunities = self.opportunity_finder.find_all(&filter).await?;

        debug!(count = opportunities.len(), "Found opportunities");

        // Score and filter opportunities
        let mut scored: Vec<ScoredOpportunity> = opportunities
            .into_iter()
            .map(|opp| ScoredOpportunity::new(opp, &self.momentum_scorer))
            .filter(|s| s.score.meets_threshold(self.config.min_momentum_score))
            .filter(|s| !matches!(s.risk_level, RiskLevel::Extreme))
            .collect();

        // Sort by score (highest first)
        scored.sort_by(|a, b| b.score.total.partial_cmp(&a.score.total).unwrap());

        // Try to open positions
        for scored_opp in scored.into_iter().take(3) {
            // Check if we're at max positions
            let positions = self.positions.read().await;
            if positions.len() >= self.config.max_positions {
                break;
            }

            // Check if we already have a position in this token
            if positions
                .values()
                .any(|p| p.token_mint == scored_opp.opportunity.address)
            {
                continue;
            }
            drop(positions);

            // Check cooldown
            if self.is_in_cooldown(&scored_opp.opportunity.address).await {
                debug!(
                    token = %scored_opp.opportunity.symbol,
                    "Token in cooldown period"
                );
                continue;
            }

            // Emit opportunity found event
            self.emit_event(TradingEvent::OpportunityFound {
                opportunity: scored_opp.opportunity.clone(),
                score: scored_opp.score.clone(),
                risk: scored_opp.risk_level,
            });

            // Try to open position
            if let Err(e) = self.open_position(&scored_opp).await {
                warn!(
                    token = %scored_opp.opportunity.symbol,
                    error = %e,
                    "Failed to open position"
                );
            }
        }

        Ok(())
    }

    /// Open a new position.
    async fn open_position(&self, scored: &ScoredOpportunity) -> Result<()> {
        let opp = &scored.opportunity;

        info!(
            token = %opp.symbol,
            price = opp.price_usd,
            score = scored.score.total,
            risk = %scored.risk_level,
            "Opening position"
        );

        // Calculate position size
        let position_size_usd = self.config.max_position_size_usd.min(
            // Could add dynamic sizing based on score/risk here
            self.config.max_position_size_usd,
        );

        // Load keypair
        let keypair = self.keypair_manager.load_keypair().await?;

        // Execute swap: SOL -> Token
        let sol_amount = position_size_usd / self.get_sol_price().await?;
        let sol_lamports = (sol_amount * 1_000_000_000.0) as u64;

        let token_mint = Pubkey::from_str(&opp.address)?;
        let sol_mint = Pubkey::from_str("So11111111111111111111111111111111111111112")?;

        let result = self
            .swap_executor
            .swap(
                &keypair,
                sol_mint,
                token_mint,
                sol_lamports,
                Some(self.config.slippage_bps),
            )
            .await?;

        // Calculate actual entry price and amount
        let token_amount = result.output_amount as f64 / 1_000_000.0; // Assuming 6 decimals
        let actual_entry_price = position_size_usd / token_amount;

        // Create position
        let position = Position::new(
            opp.address.clone(),
            opp.symbol.clone(),
            actual_entry_price,
            token_amount,
            result.signature.to_string(),
            scored.score.total,
            format!("{:?}", opp.source),
        );

        // Store position
        let mut positions = self.positions.write().await;
        positions.insert(position.id.clone(), position.clone());

        self.emit_event(TradingEvent::PositionOpened {
            position: position.clone(),
        });

        info!(
            token = %opp.symbol,
            entry_price = actual_entry_price,
            amount = token_amount,
            signature = %result.signature,
            "Position opened"
        );

        Ok(())
    }

    /// Close a position.
    async fn close_position(&self, position_id: &str, action: TradingAction) -> Result<()> {
        let mut positions = self.positions.write().await;
        let position = positions.get_mut(position_id).ok_or_else(|| {
            SolanaError::ConfigError(format!("Position not found: {}", position_id))
        })?;

        info!(
            token = %position.token_symbol,
            pnl_pct = position.pnl_pct,
            action = %action,
            "Closing position"
        );

        // Load keypair
        let keypair = self.keypair_manager.load_keypair().await?;

        // Execute swap: Token -> SOL
        let token_mint = Pubkey::from_str(&position.token_mint)?;
        let sol_mint = Pubkey::from_str("So11111111111111111111111111111111111111112")?;

        let token_amount = (position.amount * 1_000_000.0) as u64; // Assuming 6 decimals

        let result = self
            .swap_executor
            .swap(
                &keypair,
                token_mint,
                sol_mint,
                token_amount,
                Some(self.config.slippage_bps),
            )
            .await?;

        // Close position
        position.close(result.signature.to_string(), &action.to_string());

        // Move to history
        let closed_position = position.clone();
        let token_mint = position.token_mint.clone();
        positions.remove(position_id);
        drop(positions);

        // Add to history
        let mut history = self.history.write().await;
        history.push(closed_position.clone());

        // Set cooldown
        let mut cooldowns = self.cooldowns.write().await;
        cooldowns.insert(token_mint, Utc::now());

        self.emit_event(TradingEvent::PositionClosed {
            position: closed_position.clone(),
            action,
        });

        info!(
            token = %closed_position.token_symbol,
            pnl_pct = closed_position.pnl_pct,
            pnl_usd = closed_position.pnl_usd,
            duration = %closed_position.duration_str(),
            "Position closed"
        );

        Ok(())
    }

    /// Check if a token is in cooldown.
    async fn is_in_cooldown(&self, token_mint: &str) -> bool {
        let cooldowns = self.cooldowns.read().await;
        if let Some(last_exit) = cooldowns.get(token_mint) {
            let elapsed = Utc::now() - *last_exit;
            elapsed.num_seconds() < self.config.cooldown_secs as i64
        } else {
            false
        }
    }

    /// Get current price for a token.
    async fn get_current_price(&self, token_mint: &str) -> Result<f64> {
        // Use DexScreener to get current price
        let dex_client = DexScreenerClient::new("https://api.dexscreener.com".to_string());
        let pairs = dex_client.get_token_pairs(token_mint).await?;

        pairs
            .first()
            .map(|p| p.price())
            .ok_or_else(|| SolanaError::JupiterError("No price data".to_string()))
    }

    /// Get current price with momentum data (5m, 1h changes).
    async fn get_current_price_with_momentum(&self, token_mint: &str) -> Result<(f64, f64, f64)> {
        let dex_client = DexScreenerClient::new("https://api.dexscreener.com".to_string());
        let pairs = dex_client.get_token_pairs(token_mint).await?;

        let pair = pairs
            .first()
            .ok_or_else(|| SolanaError::JupiterError("No price data".to_string()))?;

        Ok((pair.price(), pair.price_change_5m(), pair.price_change_1h()))
    }

    /// Get SOL price in USD.
    async fn get_sol_price(&self) -> Result<f64> {
        let dex_client = DexScreenerClient::new("https://api.dexscreener.com".to_string());
        let pairs = dex_client
            .get_token_pairs("So11111111111111111111111111111111111111112")
            .await?;

        // Find SOL/USDC pair
        pairs
            .iter()
            .find(|p| p.quote_token.symbol.to_uppercase() == "USDC")
            .map(|p| p.price())
            .ok_or_else(|| SolanaError::JupiterError("No SOL price".to_string()))
    }

    /// Get all open positions.
    pub async fn get_positions(&self) -> Vec<Position> {
        let positions = self.positions.read().await;
        positions.values().cloned().collect()
    }

    /// Get position history.
    pub async fn get_history(&self) -> Vec<Position> {
        let history = self.history.read().await;
        history.clone()
    }

    /// Get trading summary.
    pub async fn get_summary(&self) -> PositionSummary {
        let positions = self.positions.read().await;
        let history = self.history.read().await;

        let all_positions: Vec<Position> = positions
            .values()
            .cloned()
            .chain(history.iter().cloned())
            .collect();

        PositionSummary::from_positions(&all_positions)
    }

    /// Manually close a position.
    pub async fn manual_close(&self, position_id: &str) -> Result<()> {
        self.close_position(position_id, TradingAction::ManualExit)
            .await
    }

    /// Emit a trading event.
    fn emit_event(&self, event: TradingEvent) {
        let _ = self.event_tx.send(event);
    }
}

/// Trading events.
#[derive(Debug, Clone)]
pub enum TradingEvent {
    /// Monitor started.
    MonitorStarted { config: TradingStrategyConfig },
    /// Monitor stopped.
    MonitorStopped,
    /// Opportunity found.
    OpportunityFound {
        opportunity: TokenOpportunity,
        score: MomentumScore,
        risk: RiskLevel,
    },
    /// Position opened.
    PositionOpened { position: Position },
    /// Position closed.
    PositionClosed {
        position: Position,
        action: TradingAction,
    },
    /// Position updated.
    PositionUpdated { position: Position },
    /// SOL swing trade executed.
    SolSwingExecuted {
        action: String,
        amount_usd: f64,
        signature: String,
    },
    /// Error occurred.
    Error { message: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_validation() {
        let config = TradingStrategyConfig::default();
        assert!(config.validate().is_ok());
    }
}
