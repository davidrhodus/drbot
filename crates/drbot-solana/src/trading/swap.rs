//! Swap execution for Solana.

use super::{JupiterClient, QuoteRequest, SwapQuote, SwapTransaction};
use crate::{Result, SolanaConfig, SolanaError};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    pubkey::Pubkey,
    signature::{Keypair, Signature, Signer},
    transaction::VersionedTransaction,
};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info, warn};

/// Executes token swaps via Jupiter.
pub struct SwapExecutor {
    rpc_client: Arc<RpcClient>,
    jupiter: JupiterClient,
    config: SwapExecutorConfig,
}

/// Configuration for swap execution.
#[derive(Debug, Clone)]
pub struct SwapExecutorConfig {
    /// Default slippage in basis points.
    pub default_slippage_bps: u16,
    /// Whether to simulate before executing.
    pub simulate_first: bool,
    /// Confirmation timeout.
    pub confirmation_timeout: Duration,
    /// Maximum retries.
    pub max_retries: u32,
    /// Maximum acceptable price impact percentage.
    pub max_price_impact_pct: f64,
}

impl Default for SwapExecutorConfig {
    fn default() -> Self {
        Self {
            default_slippage_bps: 50,
            simulate_first: true,
            confirmation_timeout: Duration::from_secs(60),
            max_retries: 3,
            max_price_impact_pct: 5.0,
        }
    }
}

impl From<&SolanaConfig> for SwapExecutorConfig {
    fn from(config: &SolanaConfig) -> Self {
        Self {
            default_slippage_bps: config.default_slippage_bps,
            simulate_first: config.simulate_before_execute,
            confirmation_timeout: Duration::from_secs(config.confirmation_timeout_secs),
            max_retries: config.max_retries,
            ..Default::default()
        }
    }
}

impl From<&super::strategy::TradingStrategyConfig> for SwapExecutorConfig {
    fn from(config: &super::strategy::TradingStrategyConfig) -> Self {
        Self {
            default_slippage_bps: config.slippage_bps,
            ..Default::default()
        }
    }
}

impl SwapExecutor {
    /// Create a new swap executor.
    pub fn new(
        rpc_client: Arc<RpcClient>,
        jupiter: JupiterClient,
        config: SwapExecutorConfig,
    ) -> Self {
        Self {
            rpc_client,
            jupiter,
            config,
        }
    }

    /// Get a quote for a swap.
    pub async fn get_quote(
        &self,
        input_mint: Pubkey,
        output_mint: Pubkey,
        amount: u64,
        slippage_bps: Option<u16>,
    ) -> Result<SwapQuote> {
        let request = QuoteRequest::new(input_mint, output_mint, amount)
            .with_slippage_bps(slippage_bps.unwrap_or(self.config.default_slippage_bps));

        self.jupiter.quote(&request).await
    }

    /// Execute a swap.
    pub async fn execute_swap(&self, keypair: &Keypair, quote: &SwapQuote) -> Result<SwapResult> {
        // Check price impact
        let price_impact = quote.price_impact();
        if price_impact > self.config.max_price_impact_pct {
            warn!(
                price_impact = price_impact,
                max = self.config.max_price_impact_pct,
                "Price impact too high"
            );
            return Err(SolanaError::SlippageExceeded {
                expected: (self.config.max_price_impact_pct * 100.0) as u64,
                actual: (price_impact * 100.0) as u64,
            });
        }

        // Get swap transaction from Jupiter
        let swap_tx = self
            .jupiter
            .get_swap_transaction(quote, &keypair.pubkey())
            .await?;

        // Decode and sign transaction
        let tx_bytes = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            &swap_tx.swap_transaction,
        )
        .map_err(|e| {
            SolanaError::TransactionError(format!("Failed to decode transaction: {}", e))
        })?;

        let mut transaction: VersionedTransaction = bincode::deserialize(&tx_bytes)
            .map_err(|e| SolanaError::TransactionError(format!("Failed to deserialize: {}", e)))?;

        // Sign the transaction
        transaction.signatures[0] = keypair.sign_message(&transaction.message.serialize());

        // Simulate if configured
        if self.config.simulate_first {
            debug!("Simulating swap transaction");
            let sim_result = self.rpc_client.simulate_transaction(&transaction).await?;

            if let Some(err) = sim_result.value.err {
                return Err(SolanaError::TransactionError(format!(
                    "Simulation failed: {:?}",
                    err
                )));
            }
            debug!("Simulation successful");
        }

        // Send transaction
        let signature = self.send_with_retry(&transaction, &swap_tx).await?;

        info!(
            signature = %signature,
            input_mint = %quote.input_mint,
            output_mint = %quote.output_mint,
            in_amount = quote.in_amount,
            out_amount = quote.out_amount,
            "Swap executed successfully"
        );

        Ok(SwapResult {
            signature,
            input_mint: quote.input_mint,
            output_mint: quote.output_mint,
            input_amount: quote.in_amount,
            output_amount: quote.out_amount,
            price_impact_pct: price_impact,
            confirmed: true,
        })
    }

    /// Quote and execute a swap in one call.
    pub async fn swap(
        &self,
        keypair: &Keypair,
        input_mint: Pubkey,
        output_mint: Pubkey,
        amount: u64,
        slippage_bps: Option<u16>,
    ) -> Result<SwapResult> {
        let quote = self
            .get_quote(input_mint, output_mint, amount, slippage_bps)
            .await?;
        self.execute_swap(keypair, &quote).await
    }

    /// Send transaction with retry logic.
    async fn send_with_retry(
        &self,
        transaction: &VersionedTransaction,
        swap_tx: &SwapTransaction,
    ) -> Result<Signature> {
        let mut last_error = None;

        for attempt in 0..self.config.max_retries {
            if attempt > 0 {
                debug!(attempt = attempt + 1, "Retrying transaction");
            }

            match self
                .send_and_confirm(transaction, swap_tx.last_valid_block_height)
                .await
            {
                Ok(sig) => return Ok(sig),
                Err(e) => {
                    warn!(attempt = attempt + 1, error = %e, "Transaction failed");
                    last_error = Some(e);
                }
            }
        }

        Err(last_error
            .unwrap_or_else(|| SolanaError::TransactionError("Unknown error".to_string())))
    }

    /// Send and confirm a transaction.
    async fn send_and_confirm(
        &self,
        transaction: &VersionedTransaction,
        last_valid_block_height: u64,
    ) -> Result<Signature> {
        let signature = self.rpc_client.send_transaction(transaction).await?;

        debug!(signature = %signature, "Transaction sent, awaiting confirmation");

        // Wait for confirmation
        let start = std::time::Instant::now();
        loop {
            if start.elapsed() > self.config.confirmation_timeout {
                return Err(SolanaError::ConfirmationTimeout);
            }

            let current_block = self.rpc_client.get_block_height().await?;
            if current_block > last_valid_block_height {
                return Err(SolanaError::TransactionError(
                    "Transaction expired".to_string(),
                ));
            }

            let status = self
                .rpc_client
                .get_signature_status_with_commitment(&signature, CommitmentConfig::confirmed())
                .await?;

            match status {
                Some(Ok(())) => {
                    debug!(signature = %signature, "Transaction confirmed");
                    return Ok(signature);
                }
                Some(Err(e)) => {
                    return Err(SolanaError::TransactionError(format!(
                        "Transaction failed: {:?}",
                        e
                    )));
                }
                None => {
                    tokio::time::sleep(Duration::from_millis(500)).await;
                }
            }
        }
    }
}

/// Result of a swap execution.
#[derive(Debug, Clone)]
pub struct SwapResult {
    /// Transaction signature.
    pub signature: Signature,
    /// Input token mint.
    pub input_mint: Pubkey,
    /// Output token mint.
    pub output_mint: Pubkey,
    /// Input amount (actual).
    pub input_amount: u64,
    /// Output amount (actual).
    pub output_amount: u64,
    /// Price impact percentage.
    pub price_impact_pct: f64,
    /// Whether transaction was confirmed.
    pub confirmed: bool,
}

impl SwapResult {
    /// Get the explorer URL for this transaction.
    pub fn explorer_url(&self, cluster: &str) -> String {
        let cluster_param = match cluster {
            "mainnet-beta" | "mainnet" => "",
            _ => &format!("?cluster={}", cluster),
        };
        format!(
            "https://explorer.solana.com/tx/{}{}",
            self.signature, cluster_param
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_swap_executor_config_default() {
        let config = SwapExecutorConfig::default();
        assert_eq!(config.default_slippage_bps, 50);
        assert!(config.simulate_first);
        assert_eq!(config.max_retries, 3);
    }

    #[test]
    fn test_swap_result_explorer_url() {
        let result = SwapResult {
            signature: Signature::new_unique(),
            input_mint: Pubkey::new_unique(),
            output_mint: Pubkey::new_unique(),
            input_amount: 1_000_000_000,
            output_amount: 100_000_000,
            price_impact_pct: 0.1,
            confirmed: true,
        };

        assert!(result.explorer_url("devnet").contains("devnet"));
        assert!(!result.explorer_url("mainnet").contains("cluster"));
    }
}
