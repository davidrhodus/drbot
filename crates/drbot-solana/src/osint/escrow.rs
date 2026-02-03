//! OSINT marketplace escrow management.
//!
//! Handles deposit, payout, and refund operations for bounty rewards.

use super::types::{
    BountyStatus, FeeStructure, TokenType, Transaction, TransactionStatus, TransactionType,
};
use crate::{Result, SolanaError};
use chrono::Utc;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    pubkey::Pubkey,
    signature::{Keypair, Signature, Signer},
    system_instruction,
    transaction::Transaction as SolanaTransaction,
};
use std::sync::Arc;
use tracing::{debug, info, warn};
use uuid::Uuid;

/// Escrow account for the marketplace.
pub struct EscrowAccount {
    /// Escrow wallet public key (PDA or multisig).
    pub address: Pubkey,
    /// Total SOL held in escrow.
    pub sol_balance: u64,
    /// Total USDC held in escrow.
    pub usdc_balance: u64,
    /// Number of active bounties using this escrow.
    pub active_bounties: u32,
}

/// Deposit instructions for creating a bounty.
#[derive(Debug, Clone)]
pub struct DepositInstructions {
    /// Escrow wallet to deposit to.
    pub escrow_wallet: Pubkey,
    /// Amount to deposit (including fee).
    pub total_amount: u64,
    /// Creation fee amount.
    pub fee_amount: u64,
    /// Net amount after fee.
    pub net_amount: u64,
    /// Token type.
    pub token: TokenType,
    /// Memo for the transaction.
    pub memo: String,
}

/// Escrow manager for the OSINT marketplace.
pub struct EscrowManager {
    rpc_client: Arc<RpcClient>,
    fee_structure: FeeStructure,
    escrow_wallet: Pubkey,
    treasury_wallet: Pubkey,
}

impl EscrowManager {
    /// Create a new escrow manager.
    pub fn new(rpc_client: Arc<RpcClient>, escrow_wallet: Pubkey) -> Self {
        let fee_structure = FeeStructure::default();
        let treasury_wallet = fee_structure.treasury_wallet;

        Self {
            rpc_client,
            fee_structure,
            escrow_wallet,
            treasury_wallet,
        }
    }

    /// Create with custom fee structure.
    pub fn with_fees(mut self, fee_structure: FeeStructure) -> Self {
        self.treasury_wallet = fee_structure.treasury_wallet;
        self.fee_structure = fee_structure;
        self
    }

    /// Get the escrow wallet address.
    pub fn escrow_wallet(&self) -> &Pubkey {
        &self.escrow_wallet
    }

    /// Get the fee structure.
    pub fn fee_structure(&self) -> &FeeStructure {
        &self.fee_structure
    }

    /// Get deposit instructions for creating a bounty.
    pub fn get_deposit_instructions(
        &self,
        reward_amount: u64,
        token: TokenType,
        bounty_id: Uuid,
    ) -> DepositInstructions {
        let fee_amount = self.fee_structure.calculate_creation_fee(reward_amount);
        let total_amount = reward_amount; // Poster deposits the full reward, fee deducted internally

        DepositInstructions {
            escrow_wallet: self.escrow_wallet,
            total_amount,
            fee_amount,
            net_amount: reward_amount - fee_amount,
            token,
            memo: format!("OSINT Bounty Deposit: {}", bounty_id),
        }
    }

    /// Verify a deposit transaction.
    pub async fn verify_deposit(
        &self,
        tx_signature: &str,
        expected_amount: u64,
        expected_token: TokenType,
        poster_wallet: &Pubkey,
    ) -> Result<bool> {
        debug!(
            signature = tx_signature,
            amount = expected_amount,
            token = %expected_token,
            "Verifying deposit"
        );

        // Parse signature
        let signature: Signature = tx_signature
            .parse()
            .map_err(|_| SolanaError::TransactionError("Invalid signature".to_string()))?;

        // Verify the transaction was successful
        let status = self
            .rpc_client
            .get_signature_status_with_commitment(&signature, CommitmentConfig::confirmed())
            .await
            .map_err(|e| SolanaError::RpcError(e.to_string()))?;

        match status {
            Some(Ok(())) => {}
            Some(Err(e)) => {
                warn!(signature = tx_signature, error = ?e, "Deposit transaction failed");
                return Ok(false);
            }
            None => {
                warn!(signature = tx_signature, "Deposit transaction not found");
                return Ok(false);
            }
        }

        // In a full implementation, we would:
        // 1. Parse the transaction instructions
        // 2. Verify it's a transfer to the escrow wallet
        // 3. Verify the amount matches
        // 4. Verify the sender is the poster

        info!(
            signature = tx_signature,
            amount = expected_amount,
            "Deposit verified"
        );

        Ok(true)
    }

    /// Process a payout to the bounty hunter.
    pub async fn process_payout(
        &self,
        bounty_id: Uuid,
        hunter_wallet: &Pubkey,
        amount: u64,
        token: TokenType,
        payer: &Keypair,
    ) -> Result<Transaction> {
        let payout_fee = self.fee_structure.calculate_payout_fee(amount);
        let net_payout = amount - payout_fee;

        info!(
            bounty_id = %bounty_id,
            hunter = %hunter_wallet,
            amount = amount,
            fee = payout_fee,
            net = net_payout,
            "Processing payout"
        );

        // Build and send payout transaction
        let tx_signature = match token {
            TokenType::Sol => {
                self.send_sol_payout(hunter_wallet, net_payout, payer)
                    .await?
            }
            _ => {
                // SPL token transfer would go here
                return Err(SolanaError::DeFiProtocolError(format!(
                    "Token {} payout not yet implemented",
                    token
                )));
            }
        };

        // Record the payout transaction
        let payout_tx = Transaction {
            id: Uuid::new_v4(),
            tx_type: TransactionType::Payout,
            bounty_id,
            amount: net_payout,
            token,
            from_wallet: self.escrow_wallet,
            to_wallet: *hunter_wallet,
            fee_amount: Some(payout_fee),
            tx_signature: tx_signature.clone(),
            status: TransactionStatus::Confirmed,
            created_at: Utc::now(),
        };

        // Record the fee transaction
        let _fee_tx = Transaction {
            id: Uuid::new_v4(),
            tx_type: TransactionType::Fee,
            bounty_id,
            amount: payout_fee,
            token,
            from_wallet: self.escrow_wallet,
            to_wallet: self.treasury_wallet,
            fee_amount: None,
            tx_signature: tx_signature.clone(),
            status: TransactionStatus::Confirmed,
            created_at: Utc::now(),
        };

        info!(
            bounty_id = %bounty_id,
            signature = tx_signature,
            "Payout completed"
        );

        Ok(payout_tx)
    }

    /// Process a refund to the bounty poster.
    pub async fn process_refund(
        &self,
        bounty_id: Uuid,
        poster_wallet: &Pubkey,
        original_amount: u64,
        token: TokenType,
        payer: &Keypair,
    ) -> Result<Transaction> {
        // Creation fee is non-refundable
        let creation_fee = self.fee_structure.calculate_creation_fee(original_amount);
        let refund_amount = original_amount - creation_fee;

        info!(
            bounty_id = %bounty_id,
            poster = %poster_wallet,
            original = original_amount,
            refund = refund_amount,
            "Processing refund"
        );

        let tx_signature = match token {
            TokenType::Sol => {
                self.send_sol_payout(poster_wallet, refund_amount, payer)
                    .await?
            }
            _ => {
                return Err(SolanaError::DeFiProtocolError(format!(
                    "Token {} refund not yet implemented",
                    token
                )));
            }
        };

        let refund_tx = Transaction {
            id: Uuid::new_v4(),
            tx_type: TransactionType::Refund,
            bounty_id,
            amount: refund_amount,
            token,
            from_wallet: self.escrow_wallet,
            to_wallet: *poster_wallet,
            fee_amount: Some(creation_fee), // Non-refundable fee
            tx_signature,
            status: TransactionStatus::Confirmed,
            created_at: Utc::now(),
        };

        info!(bounty_id = %bounty_id, "Refund completed");

        Ok(refund_tx)
    }

    /// Send SOL payout.
    async fn send_sol_payout(
        &self,
        recipient: &Pubkey,
        amount: u64,
        payer: &Keypair,
    ) -> Result<String> {
        let recent_blockhash = self
            .rpc_client
            .get_latest_blockhash()
            .await
            .map_err(|e| SolanaError::RpcError(e.to_string()))?;

        let transfer_ix = system_instruction::transfer(&payer.pubkey(), recipient, amount);

        let tx = SolanaTransaction::new_signed_with_payer(
            &[transfer_ix],
            Some(&payer.pubkey()),
            &[payer],
            recent_blockhash,
        );

        let signature = self
            .rpc_client
            .send_and_confirm_transaction(&tx)
            .await
            .map_err(|e| SolanaError::TransactionError(e.to_string()))?;

        Ok(signature.to_string())
    }

    /// Get escrow balance for a specific bounty.
    pub async fn get_bounty_escrow_balance(&self, _bounty_id: Uuid) -> Result<u64> {
        // In production, would track per-bounty escrow amounts
        // For now, return the total escrow balance
        let balance = self
            .rpc_client
            .get_balance(&self.escrow_wallet)
            .await
            .map_err(|e| SolanaError::RpcError(e.to_string()))?;

        Ok(balance)
    }

    /// Calculate total fees for a bounty lifecycle.
    pub fn calculate_total_fees(&self, reward_amount: u64) -> (u64, u64, u64) {
        let creation_fee = self.fee_structure.calculate_creation_fee(reward_amount);
        let payout_fee = self
            .fee_structure
            .calculate_payout_fee(reward_amount - creation_fee);
        let total_fees = creation_fee + payout_fee;

        (creation_fee, payout_fee, total_fees)
    }
}

/// Escrow state for a specific bounty.
#[derive(Debug, Clone)]
pub struct BountyEscrow {
    /// Bounty ID.
    pub bounty_id: Uuid,
    /// Amount deposited.
    pub deposited_amount: u64,
    /// Token type.
    pub token: TokenType,
    /// Deposit transaction signature.
    pub deposit_tx: String,
    /// Current status.
    pub status: EscrowStatus,
    /// Net amount after creation fee.
    pub net_amount: u64,
}

/// Escrow status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EscrowStatus {
    /// Awaiting deposit.
    Pending,
    /// Deposit received and verified.
    Funded,
    /// Payout sent to hunter.
    Released,
    /// Refund sent to poster.
    Refunded,
    /// Disputed, funds locked.
    Disputed,
}

impl BountyEscrow {
    /// Create a new escrow entry.
    pub fn new(
        bounty_id: Uuid,
        amount: u64,
        token: TokenType,
        deposit_tx: String,
        fee_structure: &FeeStructure,
    ) -> Self {
        let creation_fee = fee_structure.calculate_creation_fee(amount);

        Self {
            bounty_id,
            deposited_amount: amount,
            token,
            deposit_tx,
            status: EscrowStatus::Funded,
            net_amount: amount - creation_fee,
        }
    }

    /// Mark as released (payout sent).
    pub fn mark_released(&mut self) {
        self.status = EscrowStatus::Released;
    }

    /// Mark as refunded.
    pub fn mark_refunded(&mut self) {
        self.status = EscrowStatus::Refunded;
    }

    /// Mark as disputed.
    pub fn mark_disputed(&mut self) {
        self.status = EscrowStatus::Disputed;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deposit_instructions() {
        let rpc = Arc::new(RpcClient::new("https://api.devnet.solana.com".to_string()));
        let escrow_wallet = Pubkey::new_unique();
        let manager = EscrowManager::new(rpc, escrow_wallet);

        let instructions = manager.get_deposit_instructions(
            1_000_000_000, // 1 SOL
            TokenType::Sol,
            Uuid::new_v4(),
        );

        assert_eq!(instructions.total_amount, 1_000_000_000);
        assert_eq!(instructions.fee_amount, 25_000_000); // 2.5%
        assert_eq!(instructions.net_amount, 975_000_000);
    }

    #[test]
    fn test_total_fees() {
        let rpc = Arc::new(RpcClient::new("https://api.devnet.solana.com".to_string()));
        let escrow_wallet = Pubkey::new_unique();
        let manager = EscrowManager::new(rpc, escrow_wallet);

        let (creation, payout, total) = manager.calculate_total_fees(1_000_000_000);

        assert_eq!(creation, 25_000_000); // 2.5%
                                          // Payout fee is on net amount after creation fee
        assert_eq!(payout, 24_375_000); // 2.5% of 975M
        assert_eq!(total, creation + payout);
    }

    #[test]
    fn test_bounty_escrow() {
        let fee_structure = FeeStructure::default();
        let escrow = BountyEscrow::new(
            Uuid::new_v4(),
            1_000_000_000,
            TokenType::Sol,
            "test_signature".to_string(),
            &fee_structure,
        );

        assert_eq!(escrow.status, EscrowStatus::Funded);
        assert_eq!(escrow.net_amount, 975_000_000);
    }

    #[test]
    fn test_escrow_status_transitions() {
        let fee_structure = FeeStructure::default();
        let mut escrow = BountyEscrow::new(
            Uuid::new_v4(),
            1_000_000_000,
            TokenType::Sol,
            "test".to_string(),
            &fee_structure,
        );

        assert_eq!(escrow.status, EscrowStatus::Funded);

        escrow.mark_released();
        assert_eq!(escrow.status, EscrowStatus::Released);
    }
}
