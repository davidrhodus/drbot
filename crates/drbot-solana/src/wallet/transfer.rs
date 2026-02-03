//! Transfer operations for Solana.

use crate::{Result, SolanaError};
use serde::{Deserialize, Serialize};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    native_token::LAMPORTS_PER_SOL,
    pubkey::Pubkey,
    signature::{Keypair, Signature, Signer},
    system_instruction,
    transaction::Transaction,
};
use spl_associated_token_account::{
    get_associated_token_address, instruction::create_associated_token_account,
};
use spl_token::instruction as token_instruction;
use std::sync::Arc;
use tracing::{debug, info};

/// Type alias for TransferExecutor (for compatibility with solana-agent-kit API).
pub type TransferManager = TransferExecutor;

/// Transfer executor for SOL and SPL tokens.
pub struct TransferExecutor {
    rpc_client: Arc<RpcClient>,
    simulate_first: bool,
}

impl TransferExecutor {
    /// Create a new transfer executor.
    pub fn new(rpc_client: Arc<RpcClient>) -> Self {
        Self {
            rpc_client,
            simulate_first: true,
        }
    }

    /// Set whether to simulate transactions before sending.
    pub fn with_simulation(mut self, simulate: bool) -> Self {
        self.simulate_first = simulate;
        self
    }

    /// Transfer SOL to another address.
    pub async fn transfer_sol(
        &self,
        from: &Keypair,
        to: &Pubkey,
        lamports: u64,
    ) -> Result<TransferResult> {
        // Check balance
        let balance = self.rpc_client.get_balance(&from.pubkey()).await?;
        if balance < lamports {
            return Err(SolanaError::InsufficientBalance {
                have: balance,
                need: lamports,
            });
        }

        let recent_blockhash = self.rpc_client.get_latest_blockhash().await?;

        let instruction = system_instruction::transfer(&from.pubkey(), to, lamports);
        let transaction = Transaction::new_signed_with_payer(
            &[instruction],
            Some(&from.pubkey()),
            &[from],
            recent_blockhash,
        );

        if self.simulate_first {
            self.simulate_transaction(&transaction).await?;
        }

        let signature = self.send_and_confirm(&transaction).await?;

        info!(
            from = %from.pubkey(),
            to = %to,
            lamports = lamports,
            signature = %signature,
            "Transferred SOL"
        );

        Ok(TransferResult {
            signature,
            from: from.pubkey(),
            to: *to,
            amount: lamports,
            mint: None,
        })
    }

    /// Transfer SPL tokens to another address.
    pub async fn transfer_token(
        &self,
        from: &Keypair,
        to: &Pubkey,
        mint: &Pubkey,
        amount: u64,
    ) -> Result<TransferResult> {
        let from_ata = get_associated_token_address(&from.pubkey(), mint);
        let to_ata = get_associated_token_address(to, mint);

        let recent_blockhash = self.rpc_client.get_latest_blockhash().await?;

        let mut instructions = Vec::new();

        // Check if recipient ATA exists, create if not
        let to_account = self.rpc_client.get_account(&to_ata).await;
        if to_account.is_err() {
            debug!(to_ata = %to_ata, "Creating recipient token account");
            instructions.push(create_associated_token_account(
                &from.pubkey(),
                to,
                mint,
                &spl_token::id(),
            ));
        }

        // Add transfer instruction
        instructions.push(
            token_instruction::transfer(
                &spl_token::id(),
                &from_ata,
                &to_ata,
                &from.pubkey(),
                &[],
                amount,
            )
            .map_err(|e| SolanaError::TransactionError(e.to_string()))?,
        );

        let transaction = Transaction::new_signed_with_payer(
            &instructions,
            Some(&from.pubkey()),
            &[from],
            recent_blockhash,
        );

        if self.simulate_first {
            self.simulate_transaction(&transaction).await?;
        }

        let signature = self.send_and_confirm(&transaction).await?;

        info!(
            from = %from.pubkey(),
            to = %to,
            mint = %mint,
            amount = amount,
            signature = %signature,
            "Transferred tokens"
        );

        Ok(TransferResult {
            signature,
            from: from.pubkey(),
            to: *to,
            amount,
            mint: Some(*mint),
        })
    }

    /// Simulate a transaction.
    async fn simulate_transaction(&self, transaction: &Transaction) -> Result<()> {
        let result = self.rpc_client.simulate_transaction(transaction).await?;

        if let Some(err) = result.value.err {
            return Err(SolanaError::TransactionError(format!(
                "Simulation failed: {:?}",
                err
            )));
        }

        debug!("Transaction simulation successful");
        Ok(())
    }

    /// Send and confirm a transaction.
    async fn send_and_confirm(&self, transaction: &Transaction) -> Result<Signature> {
        let signature = self
            .rpc_client
            .send_and_confirm_transaction_with_spinner_and_commitment(
                transaction,
                CommitmentConfig::confirmed(),
            )
            .await?;

        Ok(signature)
    }

    /// Send SOL to an address (amount in SOL, not lamports).
    ///
    /// This is a convenience method matching the solana-agent-kit API.
    pub async fn send_sol(
        &self,
        from: &Keypair,
        to: &Pubkey,
        amount_sol: f64,
    ) -> Result<TransferResultJson> {
        let lamports = (amount_sol * LAMPORTS_PER_SOL as f64) as u64;
        let result = self.transfer_sol(from, to, lamports).await?;

        Ok(TransferResultJson {
            signature: result.signature.to_string(),
            from: result.from.to_string(),
            to: result.to.to_string(),
            amount: amount_sol,
            mint: None,
            explorer_url: format!("https://solscan.io/tx/{}", result.signature),
        })
    }

    /// Send SPL tokens to an address.
    ///
    /// This is a convenience method matching the solana-agent-kit API.
    pub async fn send_token(
        &self,
        from: &Keypair,
        to: &Pubkey,
        mint: &Pubkey,
        amount: u64,
    ) -> Result<TransferResultJson> {
        let result = self.transfer_token(from, to, mint, amount).await?;

        Ok(TransferResultJson {
            signature: result.signature.to_string(),
            from: result.from.to_string(),
            to: result.to.to_string(),
            amount: amount as f64,
            mint: Some(mint.to_string()),
            explorer_url: format!("https://solscan.io/tx/{}", result.signature),
        })
    }
}

/// JSON-serializable transfer result for CLI/API responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferResultJson {
    /// Transaction signature.
    pub signature: String,
    /// Sender address.
    pub from: String,
    /// Recipient address.
    pub to: String,
    /// Amount transferred.
    pub amount: f64,
    /// Token mint (None for SOL transfers).
    pub mint: Option<String>,
    /// Solscan explorer URL.
    pub explorer_url: String,
}

/// Result of a transfer operation.
#[derive(Debug, Clone)]
pub struct TransferResult {
    /// Transaction signature.
    pub signature: Signature,
    /// Sender address.
    pub from: Pubkey,
    /// Recipient address.
    pub to: Pubkey,
    /// Amount transferred (lamports for SOL, raw amount for tokens).
    pub amount: u64,
    /// Token mint (None for SOL transfers).
    pub mint: Option<Pubkey>,
}

impl TransferResult {
    /// Check if this was a SOL transfer.
    pub fn is_sol_transfer(&self) -> bool {
        self.mint.is_none()
    }

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
    fn test_transfer_result() {
        let result = TransferResult {
            signature: Signature::new_unique(),
            from: Pubkey::new_unique(),
            to: Pubkey::new_unique(),
            amount: 1_000_000_000,
            mint: None,
        };

        assert!(result.is_sol_transfer());
        assert!(result.explorer_url("devnet").contains("devnet"));
        assert!(!result.explorer_url("mainnet").contains("cluster"));
    }
}
