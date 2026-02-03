//! Balance queries for Solana wallets.

use super::{TokenBalance, WalletInfo};
use crate::{Result, SolanaError};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{native_token::LAMPORTS_PER_SOL, pubkey::Pubkey};
use spl_associated_token_account::get_associated_token_address;
use std::str::FromStr;
use std::sync::Arc;
use tracing::{debug, trace};

/// Type alias for BalanceQuery (for compatibility with solana-agent-kit API).
pub type BalanceChecker = BalanceQuery;

/// Query wallet balances.
pub struct BalanceQuery {
    rpc_client: Arc<RpcClient>,
}

impl BalanceQuery {
    /// Create a new balance query.
    pub fn new(rpc_client: Arc<RpcClient>) -> Self {
        Self { rpc_client }
    }

    /// Get SOL balance for an address (in lamports).
    pub async fn get_sol_balance_lamports(&self, address: &Pubkey) -> Result<u64> {
        let balance = self.rpc_client.get_balance(address).await?;
        debug!(address = %address, balance = balance, "Got SOL balance");
        Ok(balance)
    }

    /// Get SOL balance for an address (in SOL).
    pub async fn get_sol_balance(&self, address: &Pubkey) -> Result<f64> {
        let lamports = self.get_sol_balance_lamports(address).await?;
        Ok(lamports as f64 / LAMPORTS_PER_SOL as f64)
    }

    /// Get token balance for a specific mint.
    pub async fn get_token_balance(&self, wallet: &Pubkey, mint: &Pubkey) -> Result<TokenBalance> {
        let ata = get_associated_token_address(wallet, mint);
        trace!(ata = %ata, "Checking token account");

        let account = self
            .rpc_client
            .get_token_account(&ata)
            .await
            .map_err(|_| SolanaError::TokenAccountNotFound { mint: *mint })?;

        if let Some(account) = account {
            let amount = account.token_amount;
            Ok(TokenBalance::new(
                *mint,
                amount.amount.parse().unwrap_or(0),
                amount.decimals,
            ))
        } else {
            Err(SolanaError::TokenAccountNotFound { mint: *mint })
        }
    }

    /// Get all token balances for a wallet.
    pub async fn get_all_token_balances(&self, wallet: &Pubkey) -> Result<Vec<TokenBalance>> {
        let accounts = self
            .rpc_client
            .get_token_accounts_by_owner(
                wallet,
                solana_client::rpc_request::TokenAccountsFilter::ProgramId(spl_token::id()),
            )
            .await?;

        let mut balances = Vec::new();

        for account in accounts {
            // Serialize the data to JSON and parse it
            let data_json = serde_json::to_value(&account.account.data).ok();

            if let Some(parsed) = data_json {
                // Handle parsed JSON format
                if let Some(info) = parsed.get("parsed").and_then(|p| p.get("info")) {
                    let mint_str = info
                        .get("mint")
                        .and_then(|m: &serde_json::Value| m.as_str())
                        .unwrap_or("");
                    if let Ok(mint) = Pubkey::from_str(mint_str) {
                        let token_amount =
                            info.get("tokenAmount").unwrap_or(&serde_json::Value::Null);
                        let amount: u64 = token_amount
                            .get("amount")
                            .and_then(|a: &serde_json::Value| a.as_str())
                            .and_then(|a| a.parse().ok())
                            .unwrap_or(0);
                        let decimals = token_amount
                            .get("decimals")
                            .and_then(|d: &serde_json::Value| d.as_u64())
                            .unwrap_or(0) as u8;

                        if amount > 0 {
                            balances.push(TokenBalance::new(mint, amount, decimals));
                        }
                    }
                }
            }
        }

        debug!(wallet = %wallet, count = balances.len(), "Got token balances");
        Ok(balances)
    }

    /// Get full wallet info including SOL and all tokens.
    pub async fn get_wallet_info(&self, wallet: &Pubkey) -> Result<WalletInfo> {
        let sol_balance = self.get_sol_balance_lamports(wallet).await?;
        let token_balances = self.get_all_token_balances(wallet).await?;

        Ok(WalletInfo {
            address: *wallet,
            sol_balance,
            token_balances,
        })
    }

    /// Check if wallet has minimum SOL balance for transaction fees.
    pub async fn has_min_balance_for_fees(
        &self,
        wallet: &Pubkey,
        min_lamports: u64,
    ) -> Result<bool> {
        let balance = self.get_sol_balance_lamports(wallet).await?;
        Ok(balance >= min_lamports)
    }
}

/// Common token mints.
pub mod tokens {
    use solana_sdk::pubkey::Pubkey;
    use std::str::FromStr;

    lazy_static::lazy_static! {
        /// Wrapped SOL mint.
        pub static ref WSOL: Pubkey = Pubkey::from_str("So11111111111111111111111111111111111111112").unwrap();
        /// USDC mint (native).
        pub static ref USDC: Pubkey = Pubkey::from_str("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v").unwrap();
        /// USDT mint.
        pub static ref USDT: Pubkey = Pubkey::from_str("Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB").unwrap();
        /// Bonk mint.
        pub static ref BONK: Pubkey = Pubkey::from_str("DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263").unwrap();
    }

    /// Get the symbol for a known token mint.
    pub fn symbol_for_mint(mint: &Pubkey) -> Option<&'static str> {
        if *mint == *WSOL {
            Some("SOL")
        } else if *mint == *USDC {
            Some("USDC")
        } else if *mint == *USDT {
            Some("USDT")
        } else if *mint == *BONK {
            Some("BONK")
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_known_tokens() {
        assert_eq!(tokens::symbol_for_mint(&tokens::WSOL), Some("SOL"));
        assert_eq!(tokens::symbol_for_mint(&tokens::USDC), Some("USDC"));
        assert_eq!(tokens::symbol_for_mint(&Pubkey::new_unique()), None);
    }
}
