//! Jupiter DEX aggregator client.

use super::SwapQuote;
use crate::{Result, SolanaError};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    transaction::VersionedTransaction,
};
use std::str::FromStr;
use std::sync::Arc;
use tracing::{debug, info, trace};

/// Jupiter API client.
pub struct JupiterClient {
    base_url: String,
    client: Client,
}

impl JupiterClient {
    /// Create a new Jupiter client.
    pub fn new(base_url: String) -> Self {
        Self {
            base_url,
            client: Client::new(),
        }
    }

    /// Get the base URL.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Get a quote for a swap using a QuoteRequest.
    pub async fn quote(&self, request: &QuoteRequest) -> Result<SwapQuote> {
        let url = format!("{}/quote", self.base_url);

        trace!(
            input = %request.input_mint,
            output = %request.output_mint,
            amount = request.amount,
            "Requesting Jupiter quote"
        );

        let response = self
            .client
            .get(&url)
            .query(&[
                ("inputMint", request.input_mint.to_string()),
                ("outputMint", request.output_mint.to_string()),
                ("amount", request.amount.to_string()),
                ("slippageBps", request.slippage_bps.to_string()),
            ])
            .send()
            .await?;

        if response.status() == 429 {
            return Err(SolanaError::RateLimitExceeded);
        }

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(SolanaError::JupiterError(error_text));
        }

        let quote: SwapQuote = response.json().await?;

        debug!(
            in_amount = quote.in_amount,
            out_amount = quote.out_amount,
            price_impact = ?quote.price_impact_pct,
            "Got Jupiter quote"
        );

        Ok(quote)
    }

    /// Get swap transaction data.
    pub async fn get_swap_transaction(
        &self,
        quote: &SwapQuote,
        user_pubkey: &Pubkey,
    ) -> Result<SwapTransaction> {
        let url = format!("{}/swap", self.base_url);

        let request = SwapRequest {
            user_public_key: user_pubkey.to_string(),
            quote_response: quote.clone(),
            wrap_and_unwrap_sol: true,
            compute_unit_price_micro_lamports: Some(1000), // Priority fee
            dynamic_compute_unit_limit: true,
        };

        let response = self.client.post(&url).json(&request).send().await?;

        if response.status() == 429 {
            return Err(SolanaError::RateLimitExceeded);
        }

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(SolanaError::JupiterError(error_text));
        }

        let swap_tx: SwapTransaction = response.json().await?;

        debug!(
            last_valid_block = swap_tx.last_valid_block_height,
            "Got Jupiter swap transaction"
        );

        Ok(swap_tx)
    }

    /// Get token list from Jupiter.
    pub async fn get_tokens(&self) -> Result<Vec<JupiterToken>> {
        let url = "https://token.jup.ag/strict";

        let response = self.client.get(url).send().await?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(SolanaError::JupiterError(error_text));
        }

        let tokens: Vec<JupiterToken> = response.json().await?;
        Ok(tokens)
    }

    /// Get token price in USDC.
    pub async fn get_price(&self, mint: &Pubkey) -> Result<f64> {
        let url = format!("{}/price", self.base_url);

        let response = self
            .client
            .get(&url)
            .query(&[("ids", mint.to_string())])
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(SolanaError::JupiterError(error_text));
        }

        let prices: PriceResponse = response.json().await?;

        prices
            .data
            .get(&mint.to_string())
            .map(|p| p.price)
            .ok_or_else(|| SolanaError::JupiterError("Price not found".to_string()))
    }

    /// Get a quote for a swap using string mint addresses.
    ///
    /// This is a convenience method for CLI/API usage.
    pub async fn get_quote(
        &self,
        input_mint: &str,
        output_mint: &str,
        amount: u64,
        slippage_bps: u16,
    ) -> Result<SwapQuote> {
        let input = Pubkey::from_str(input_mint)
            .map_err(|_| SolanaError::InvalidPubkey(input_mint.to_string()))?;
        let output = Pubkey::from_str(output_mint)
            .map_err(|_| SolanaError::InvalidPubkey(output_mint.to_string()))?;

        let request = QuoteRequest::new(input, output, amount).with_slippage_bps(slippage_bps);
        self.quote(&request).await
    }

    /// Execute a full swap.
    ///
    /// This gets a quote, creates the swap transaction, signs it, and sends it.
    pub async fn swap(
        &self,
        rpc_client: Arc<RpcClient>,
        signer: &Keypair,
        input_mint: &str,
        output_mint: &str,
        amount: u64,
        slippage_bps: u16,
    ) -> Result<SwapResult> {
        // Get quote
        let quote = self
            .get_quote(input_mint, output_mint, amount, slippage_bps)
            .await?;

        // Get swap transaction
        let swap_tx = self.get_swap_transaction(&quote, &signer.pubkey()).await?;

        // Deserialize the transaction
        let tx_bytes = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            &swap_tx.swap_transaction,
        )
        .map_err(|e| {
            SolanaError::TransactionError(format!("Failed to decode transaction: {}", e))
        })?;

        let mut transaction: VersionedTransaction = bincode::deserialize(&tx_bytes)
            .map_err(|e| SolanaError::TransactionError(format!("Failed to deserialize: {}", e)))?;

        // Sign the transaction using the message's presigner interface
        let message_bytes = transaction.message.serialize();
        let signature = signer.sign_message(&message_bytes);

        // Replace the placeholder signature with our actual signature
        if let Some(sig) = transaction.signatures.get_mut(0) {
            *sig = signature;
        }

        info!(
            user = %signer.pubkey(),
            input = input_mint,
            output = output_mint,
            amount = amount,
            "Executing Jupiter swap"
        );

        // Send transaction
        let tx_signature = rpc_client
            .send_and_confirm_transaction(&transaction)
            .await?;

        info!(signature = %tx_signature, "Swap transaction confirmed");

        Ok(SwapResult {
            signature: tx_signature.to_string(),
            input_mint: input_mint.to_string(),
            output_mint: output_mint.to_string(),
            input_amount: quote.in_amount,
            output_amount: quote.out_amount,
        })
    }
}

/// Result of a swap operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwapResult {
    /// Transaction signature.
    pub signature: String,
    /// Input token mint.
    pub input_mint: String,
    /// Output token mint.
    pub output_mint: String,
    /// Input amount (in smallest units).
    pub input_amount: u64,
    /// Output amount (in smallest units).
    pub output_amount: u64,
}

/// Quote request parameters.
#[derive(Debug, Clone)]
pub struct QuoteRequest {
    /// Input token mint.
    pub input_mint: Pubkey,
    /// Output token mint.
    pub output_mint: Pubkey,
    /// Amount in smallest units.
    pub amount: u64,
    /// Slippage tolerance in basis points.
    pub slippage_bps: u16,
}

impl QuoteRequest {
    /// Create a new quote request.
    pub fn new(input_mint: Pubkey, output_mint: Pubkey, amount: u64) -> Self {
        Self {
            input_mint,
            output_mint,
            amount,
            slippage_bps: 50, // Default 0.5%
        }
    }

    /// Set slippage tolerance.
    pub fn with_slippage_bps(mut self, bps: u16) -> Self {
        self.slippage_bps = bps;
        self
    }
}

/// Swap request to Jupiter API.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SwapRequest {
    user_public_key: String,
    quote_response: SwapQuote,
    wrap_and_unwrap_sol: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    compute_unit_price_micro_lamports: Option<u64>,
    dynamic_compute_unit_limit: bool,
}

/// Swap transaction response from Jupiter.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwapTransaction {
    /// Base64-encoded transaction.
    pub swap_transaction: String,
    /// Last valid block height.
    pub last_valid_block_height: u64,
    /// Priority fee paid.
    #[serde(default)]
    pub prioritization_fee_lamports: u64,
}

/// Jupiter token information.
#[derive(Debug, Clone, Deserialize)]
pub struct JupiterToken {
    /// Token mint address.
    pub address: String,
    /// Token symbol.
    pub symbol: String,
    /// Token name.
    pub name: String,
    /// Decimals.
    pub decimals: u8,
    /// Logo URI.
    #[serde(rename = "logoURI")]
    pub logo_uri: Option<String>,
    /// Tags.
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Price response from Jupiter.
#[derive(Debug, Clone, Deserialize)]
struct PriceResponse {
    data: std::collections::HashMap<String, TokenPrice>,
}

#[derive(Debug, Clone, Deserialize)]
struct TokenPrice {
    price: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quote_request() {
        let request = QuoteRequest::new(Pubkey::new_unique(), Pubkey::new_unique(), 1_000_000_000)
            .with_slippage_bps(100);

        assert_eq!(request.slippage_bps, 100);
    }

    #[test]
    fn test_jupiter_client_url() {
        let client = JupiterClient::new("https://quote-api.jup.ag/v6".to_string());
        assert!(client.base_url().contains("jup.ag"));
    }
}
