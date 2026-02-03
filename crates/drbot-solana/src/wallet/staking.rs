//! Native SOL staking to validators.
//!
//! Allows agents to stake SOL directly to validators, manage stake accounts,
//! and withdraw staked SOL.

use crate::{Result, SolanaError};
use serde::{Deserialize, Serialize};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    native_token::LAMPORTS_PER_SOL,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    stake::{
        self, instruction as stake_instruction,
        state::{Authorized, Lockup, StakeStateV2},
    },
    system_instruction,
    transaction::Transaction,
};
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use tracing::{debug, info};

/// Known validators with their vote account addresses.
#[derive(Debug, Clone)]
pub struct KnownValidators {
    validators: HashMap<String, Pubkey>,
}

impl Default for KnownValidators {
    fn default() -> Self {
        let mut validators = HashMap::new();

        // Popular validators
        validators.insert(
            "jito".to_string(),
            Pubkey::from_str("J1to1yufRnoWn81KYg1XkTWzmKjnYSnmE2VY8DGUJ9Qv").unwrap(),
        );
        validators.insert(
            "marinade".to_string(),
            Pubkey::from_str("mrgn28BhocwdAUEenen3Sw2MR9cPKDpLkDvzDdR7DBD").unwrap(),
        );
        validators.insert(
            "solflare".to_string(),
            Pubkey::from_str("SoLFLaReRVNagJzYYGppSkqzkhmHZ5ZR8EUpzqLEAaL").unwrap(),
        );
        validators.insert(
            "everstake".to_string(),
            Pubkey::from_str("EverSFw9uN5t1V8kS3ficHUcKffSjwpGzUSGd7mgmSks").unwrap(),
        );
        validators.insert(
            "coinbase".to_string(),
            Pubkey::from_str("beefKGBWeSpHzYBHZXwp5So7wdQGX6mu4ZHCsH3uTar").unwrap(),
        );
        validators.insert(
            "figment".to_string(),
            Pubkey::from_str("FiGmEVpfPR8V8LAwtKMqnhH1qzFqsABG4fTqXCW8Zmvq").unwrap(),
        );

        Self { validators }
    }
}

impl KnownValidators {
    /// Get all known validators.
    pub fn all(&self) -> &HashMap<String, Pubkey> {
        &self.validators
    }

    /// Resolve a validator name or address to a pubkey.
    pub fn resolve(&self, name_or_address: &str) -> Result<Pubkey> {
        // Check if it's a known name
        if let Some(pubkey) = self.validators.get(&name_or_address.to_lowercase()) {
            return Ok(*pubkey);
        }

        // Try parsing as pubkey
        Pubkey::from_str(name_or_address)
            .map_err(|_| SolanaError::InvalidPubkey(name_or_address.to_string()))
    }

    /// Add a custom validator.
    pub fn add(&mut self, name: String, vote_account: Pubkey) {
        self.validators.insert(name.to_lowercase(), vote_account);
    }
}

/// A stake account with its current state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StakeAccountInfo {
    /// Stake account address.
    pub address: Pubkey,
    /// Lamports in the account.
    pub lamports: u64,
    /// SOL value (lamports / LAMPORTS_PER_SOL).
    pub sol: f64,
    /// Current state of the stake.
    pub state: StakeState,
    /// Validator vote account (if delegated).
    pub validator: Option<Pubkey>,
    /// Epoch when stake was activated.
    pub activation_epoch: Option<u64>,
    /// Epoch when stake was deactivated.
    pub deactivation_epoch: Option<u64>,
}

/// State of a stake account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StakeState {
    /// Not initialized.
    Uninitialized,
    /// Initialized but not delegated.
    Initialized,
    /// Delegated to a validator.
    Delegated,
    /// Deactivating (cooldown).
    Deactivating,
    /// Fully deactivated, ready to withdraw.
    Inactive,
}

impl std::fmt::Display for StakeState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StakeState::Uninitialized => write!(f, "uninitialized"),
            StakeState::Initialized => write!(f, "initialized"),
            StakeState::Delegated => write!(f, "delegated"),
            StakeState::Deactivating => write!(f, "deactivating"),
            StakeState::Inactive => write!(f, "inactive"),
        }
    }
}

/// Result of a staking operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StakeResult {
    /// Transaction signature.
    pub signature: String,
    /// Stake account address.
    pub stake_account: Pubkey,
    /// Validator vote account (for delegate operations).
    pub validator: Option<Pubkey>,
    /// Amount in SOL.
    pub amount_sol: f64,
    /// Solscan explorer URL.
    pub explorer_url: String,
}

/// Native SOL staking manager.
pub struct StakingManager {
    rpc_client: Arc<RpcClient>,
    known_validators: KnownValidators,
}

impl StakingManager {
    /// Create a new staking manager.
    pub fn new(rpc_client: Arc<RpcClient>) -> Self {
        Self {
            rpc_client,
            known_validators: KnownValidators::default(),
        }
    }

    /// Get known validators.
    pub fn known_validators(&self) -> &KnownValidators {
        &self.known_validators
    }

    /// Get mutable reference to known validators.
    pub fn known_validators_mut(&mut self) -> &mut KnownValidators {
        &mut self.known_validators
    }

    /// Create a stake account and delegate to a validator.
    ///
    /// # Arguments
    /// * `payer` - Keypair that will pay for and own the stake
    /// * `validator` - Validator name or vote account address
    /// * `amount_sol` - Amount of SOL to stake
    pub async fn stake(
        &self,
        payer: &Keypair,
        validator: &str,
        amount_sol: f64,
    ) -> Result<StakeResult> {
        let vote_account = self.known_validators.resolve(validator)?;
        let lamports = (amount_sol * LAMPORTS_PER_SOL as f64) as u64;

        // Generate new stake account keypair
        let stake_account = Keypair::new();

        // Get minimum balance for rent exemption
        let rent_exemption = self
            .rpc_client
            .get_minimum_balance_for_rent_exemption(stake::state::StakeStateV2::size_of())
            .await?;

        let total_lamports = lamports + rent_exemption;

        info!(
            validator = %vote_account,
            amount = amount_sol,
            stake_account = %stake_account.pubkey(),
            "Creating stake account and delegating"
        );

        // Create stake account instruction
        let create_account_ix = stake_instruction::create_account(
            &payer.pubkey(),
            &stake_account.pubkey(),
            &Authorized {
                staker: payer.pubkey(),
                withdrawer: payer.pubkey(),
            },
            &Lockup::default(),
            total_lamports,
        );

        // Delegate stake instruction
        let delegate_ix = stake_instruction::delegate_stake(
            &stake_account.pubkey(),
            &payer.pubkey(),
            &vote_account,
        );

        // Build and send transaction
        let recent_blockhash = self.rpc_client.get_latest_blockhash().await?;

        let mut instructions = create_account_ix;
        instructions.push(delegate_ix);

        let transaction = Transaction::new_signed_with_payer(
            &instructions,
            Some(&payer.pubkey()),
            &[payer, &stake_account],
            recent_blockhash,
        );

        let signature = self
            .rpc_client
            .send_and_confirm_transaction_with_spinner(&transaction)
            .await?;

        let sig_str = signature.to_string();

        debug!(signature = %sig_str, "Stake transaction confirmed");

        Ok(StakeResult {
            signature: sig_str.clone(),
            stake_account: stake_account.pubkey(),
            validator: Some(vote_account),
            amount_sol,
            explorer_url: format!("https://solscan.io/tx/{}", sig_str),
        })
    }

    /// Get all stake accounts owned by a wallet.
    pub async fn get_stake_accounts(&self, owner: &Pubkey) -> Result<Vec<StakeAccountInfo>> {
        let accounts = self
            .rpc_client
            .get_program_accounts_with_config(
                &stake::program::id(),
                solana_client::rpc_config::RpcProgramAccountsConfig {
                    filters: Some(vec![
                        // Filter by authorized staker
                        solana_client::rpc_filter::RpcFilterType::Memcmp(
                            solana_client::rpc_filter::Memcmp::new_raw_bytes(
                                12, // Offset for staker in Authorized
                                owner.to_bytes().to_vec(),
                            ),
                        ),
                    ]),
                    account_config: solana_client::rpc_config::RpcAccountInfoConfig {
                        encoding: Some(solana_account_decoder::UiAccountEncoding::Base64),
                        commitment: Some(CommitmentConfig::confirmed()),
                        ..Default::default()
                    },
                    ..Default::default()
                },
            )
            .await?;

        let current_epoch = self.rpc_client.get_epoch_info().await?.epoch;

        let mut stake_accounts = Vec::new();

        for (pubkey, account) in accounts {
            let stake_state: StakeStateV2 = bincode::deserialize(&account.data).map_err(|e| {
                SolanaError::TransactionError(format!("Failed to parse stake state: {}", e))
            })?;

            let (state, validator, activation_epoch, deactivation_epoch) = match stake_state {
                StakeStateV2::Uninitialized => (StakeState::Uninitialized, None, None, None),
                StakeStateV2::Initialized(_) => (StakeState::Initialized, None, None, None),
                StakeStateV2::Stake(_, stake, _) => {
                    let delegation = stake.delegation;
                    let act_epoch = delegation.activation_epoch;
                    let deact_epoch = delegation.deactivation_epoch;

                    let state = if deact_epoch != u64::MAX {
                        if current_epoch >= deact_epoch {
                            StakeState::Inactive
                        } else {
                            StakeState::Deactivating
                        }
                    } else if current_epoch >= act_epoch {
                        StakeState::Delegated
                    } else {
                        StakeState::Initialized
                    };

                    (
                        state,
                        Some(delegation.voter_pubkey),
                        Some(act_epoch),
                        if deact_epoch != u64::MAX {
                            Some(deact_epoch)
                        } else {
                            None
                        },
                    )
                }
                StakeStateV2::RewardsPool => continue, // Skip rewards pool accounts
            };

            stake_accounts.push(StakeAccountInfo {
                address: pubkey,
                lamports: account.lamports,
                sol: account.lamports as f64 / LAMPORTS_PER_SOL as f64,
                state,
                validator,
                activation_epoch,
                deactivation_epoch,
            });
        }

        Ok(stake_accounts)
    }

    /// Deactivate a stake account (begin unstaking).
    ///
    /// After deactivation, the stake must cool down for the remainder of the
    /// current epoch plus one full epoch before it can be withdrawn.
    pub async fn unstake(
        &self,
        authority: &Keypair,
        stake_account: &Pubkey,
    ) -> Result<StakeResult> {
        info!(stake_account = %stake_account, "Deactivating stake");

        let deactivate_ix = stake_instruction::deactivate_stake(stake_account, &authority.pubkey());

        let recent_blockhash = self.rpc_client.get_latest_blockhash().await?;

        let transaction = Transaction::new_signed_with_payer(
            &[deactivate_ix],
            Some(&authority.pubkey()),
            &[authority],
            recent_blockhash,
        );

        let signature = self
            .rpc_client
            .send_and_confirm_transaction_with_spinner(&transaction)
            .await?;

        let sig_str = signature.to_string();

        debug!(signature = %sig_str, "Deactivation confirmed");

        Ok(StakeResult {
            signature: sig_str.clone(),
            stake_account: *stake_account,
            validator: None,
            amount_sol: 0.0,
            explorer_url: format!("https://solscan.io/tx/{}", sig_str),
        })
    }

    /// Withdraw from a deactivated stake account.
    ///
    /// The stake account must be fully deactivated (inactive) before
    /// withdrawal is possible.
    pub async fn withdraw(
        &self,
        authority: &Keypair,
        stake_account: &Pubkey,
    ) -> Result<StakeResult> {
        // Get stake account balance
        let balance = self.rpc_client.get_balance(stake_account).await?;

        info!(
            stake_account = %stake_account,
            balance = balance,
            "Withdrawing from stake account"
        );

        let withdraw_ix = stake_instruction::withdraw(
            stake_account,
            &authority.pubkey(),
            &authority.pubkey(),
            balance,
            None,
        );

        let recent_blockhash = self.rpc_client.get_latest_blockhash().await?;

        let transaction = Transaction::new_signed_with_payer(
            &[withdraw_ix],
            Some(&authority.pubkey()),
            &[authority],
            recent_blockhash,
        );

        let signature = self
            .rpc_client
            .send_and_confirm_transaction_with_spinner(&transaction)
            .await?;

        let sig_str = signature.to_string();
        let amount_sol = balance as f64 / LAMPORTS_PER_SOL as f64;

        debug!(signature = %sig_str, amount = amount_sol, "Withdrawal confirmed");

        Ok(StakeResult {
            signature: sig_str.clone(),
            stake_account: *stake_account,
            validator: None,
            amount_sol,
            explorer_url: format!("https://solscan.io/tx/{}", sig_str),
        })
    }

    /// Get the total staked SOL across all stake accounts.
    pub async fn get_total_staked(&self, owner: &Pubkey) -> Result<f64> {
        let accounts = self.get_stake_accounts(owner).await?;
        Ok(accounts.iter().map(|a| a.sol).sum())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_known_validators_default() {
        let validators = KnownValidators::default();
        assert!(validators.all().contains_key("jito"));
        assert!(validators.all().contains_key("marinade"));
        assert!(validators.all().contains_key("solflare"));
        assert!(validators.all().contains_key("everstake"));
    }

    #[test]
    fn test_resolve_by_name() {
        let validators = KnownValidators::default();
        let jito = validators.resolve("jito").unwrap();
        assert_eq!(
            jito.to_string(),
            "J1to1yufRnoWn81KYg1XkTWzmKjnYSnmE2VY8DGUJ9Qv"
        );
    }

    #[test]
    fn test_resolve_by_address() {
        let validators = KnownValidators::default();
        let pubkey = validators
            .resolve("J1to1yufRnoWn81KYg1XkTWzmKjnYSnmE2VY8DGUJ9Qv")
            .unwrap();
        assert_eq!(
            pubkey.to_string(),
            "J1to1yufRnoWn81KYg1XkTWzmKjnYSnmE2VY8DGUJ9Qv"
        );
    }

    #[test]
    fn test_resolve_case_insensitive() {
        let validators = KnownValidators::default();
        let jito1 = validators.resolve("JITO").unwrap();
        let jito2 = validators.resolve("Jito").unwrap();
        let jito3 = validators.resolve("jito").unwrap();
        assert_eq!(jito1, jito2);
        assert_eq!(jito2, jito3);
    }

    #[test]
    fn test_add_custom_validator() {
        let mut validators = KnownValidators::default();
        let custom = Pubkey::new_unique();
        validators.add("custom".to_string(), custom);
        assert_eq!(validators.resolve("custom").unwrap(), custom);
    }

    #[test]
    fn test_stake_state_display() {
        assert_eq!(StakeState::Delegated.to_string(), "delegated");
        assert_eq!(StakeState::Inactive.to_string(), "inactive");
        assert_eq!(StakeState::Deactivating.to_string(), "deactivating");
    }
}
