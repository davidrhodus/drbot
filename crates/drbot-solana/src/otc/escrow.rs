//! On-chain escrow for OTC trades (DvP).
//!
//! This module wraps the `drbot-otc-escrow-program` on-chain program to provide:
//! - deterministic escrow PDA derivation
//! - create / fund / cancel transactions
//! - reading escrow state from chain
//!
//! The escrow program supports:
//! - **native SOL deposits** (lamports held by the escrow PDA account)
//! - **SPL Token deposits** (token vault ATA owned by the escrow PDA)
//! - **auto-settlement** in the second funding transaction (settles and closes)
//! - **either party can pay rent/fees** (create is idempotent; first caller pays rent)

use crate::{Result, SolanaError};
use borsh::BorshDeserialize;
use drbot_otc_escrow_program::{derive_escrow_pda, EscrowInstruction, EscrowTerms, Leg, LegKind};
use solana_client::client_error::ClientError;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::instruction::{AccountMeta, Instruction};
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Signature;
use solana_sdk::signer::keypair::Keypair;
use solana_sdk::signer::Signer;
use solana_sdk::system_program;
use solana_sdk::transaction::Transaction;
use std::sync::Arc;
use uuid::Uuid;

pub type OnChainEscrowState = drbot_otc_escrow_program::EscrowState;

/// Escrow state with address convenience.
#[derive(Debug, Clone)]
pub struct EscrowAccount {
    pub address: Pubkey,
    pub state: OnChainEscrowState,
}

/// Parameters for creating an escrow (client-side convenience).
#[derive(Debug, Clone)]
pub struct CreateEscrowParams {
    pub negotiation_id: Uuid,
    pub party_a: Pubkey,
    pub party_b: Pubkey,
    pub a_owes: Leg,
    pub b_owes: Leg,
    pub expiry_unix_ts: i64,
}

impl CreateEscrowParams {
    pub fn new(
        negotiation_id: Uuid,
        party_a: Pubkey,
        party_b: Pubkey,
        a_owes: Leg,
        b_owes: Leg,
        expiry_unix_ts: i64,
    ) -> Self {
        Self {
            negotiation_id,
            party_a,
            party_b,
            a_owes,
            b_owes,
            expiry_unix_ts,
        }
    }

    pub fn to_terms(&self) -> EscrowTerms {
        EscrowTerms {
            negotiation_id: *self.negotiation_id.as_bytes(),
            party_a: self.party_a,
            party_b: self.party_b,
            a_owes: self.a_owes,
            b_owes: self.b_owes,
            expiry_unix_ts: self.expiry_unix_ts,
        }
    }
}

/// Escrow manager for creating and managing on-chain escrows.
pub struct EscrowManager {
    rpc_client: Arc<RpcClient>,
    escrow_program_id: Option<Pubkey>,
}

impl EscrowManager {
    /// Create a new escrow manager.
    pub fn new(rpc_client: Arc<RpcClient>) -> Self {
        Self {
            rpc_client,
            escrow_program_id: None,
        }
    }

    /// Set the escrow program ID.
    pub fn with_program_id(mut self, program_id: Pubkey) -> Self {
        self.escrow_program_id = Some(program_id);
        self
    }

    fn program_id(&self) -> Result<Pubkey> {
        self.escrow_program_id
            .ok_or_else(|| SolanaError::ConfigError("OTC escrow program id not configured".to_string()))
    }

    /// Deterministically derive the escrow PDA for a given negotiation and counterparty pair.
    pub fn derive_address(&self, negotiation_id: Uuid, party_a: Pubkey, party_b: Pubkey) -> Result<(Pubkey, u8)> {
        let program_id = self.program_id()?;
        Ok(derive_escrow_pda(
            &program_id,
            negotiation_id.as_bytes(),
            &party_a,
            &party_b,
        ))
    }

    pub fn associated_token_address(owner: &Pubkey, mint: &Pubkey) -> Pubkey {
        spl_associated_token_account::get_associated_token_address(owner, mint)
    }

    /// Create (or verify) an escrow on-chain. Idempotent if escrow already exists with matching terms.
    pub async fn create_escrow(&self, payer: &Keypair, params: CreateEscrowParams) -> Result<(Pubkey, Signature)> {
        let program_id = self.program_id()?;
        let terms = params.to_terms();
        let (escrow, _bump) = self.derive_address(params.negotiation_id, params.party_a, params.party_b)?;

        let mut accounts = vec![
            AccountMeta::new(payer.pubkey(), true),
            AccountMeta::new(escrow, false),
            AccountMeta::new_readonly(params.party_a, false),
            AccountMeta::new_readonly(params.party_b, false),
            AccountMeta::new_readonly(system_program::id(), false),
            AccountMeta::new_readonly(spl_token::id(), false),
            AccountMeta::new_readonly(spl_associated_token_account::id(), false),
            AccountMeta::new_readonly(solana_sdk::sysvar::rent::id(), false),
        ];

        if params.a_owes.kind == LegKind::SplToken {
            let vault = Self::associated_token_address(&escrow, &params.a_owes.mint);
            accounts.push(AccountMeta::new_readonly(params.a_owes.mint, false));
            accounts.push(AccountMeta::new(vault, false));
        }
        if params.b_owes.kind == LegKind::SplToken {
            let vault = Self::associated_token_address(&escrow, &params.b_owes.mint);
            accounts.push(AccountMeta::new_readonly(params.b_owes.mint, false));
            accounts.push(AccountMeta::new(vault, false));
        }

        let ix = Instruction {
            program_id,
            accounts,
            data: borsh::to_vec(&EscrowInstruction::CreateEscrow { terms })
                .map_err(|e| SolanaError::TransactionError(e.to_string()))?,
        };

        let bh = self
            .rpc_client
            .get_latest_blockhash()
            .await
            .map_err(|e| SolanaError::RpcError(e.to_string()))?;

        let tx = Transaction::new_signed_with_payer(&[ix], Some(&payer.pubkey()), &[payer], bh);

        let sig = self
            .rpc_client
            .send_and_confirm_transaction(&tx)
            .await
            .map_err(|e| SolanaError::TransactionError(e.to_string()))?;

        Ok((escrow, sig))
    }

    /// Read escrow state from chain.
    pub async fn get_escrow(&self, address: &Pubkey) -> Result<EscrowAccount> {
        let program_id = self.program_id()?;
        let account = self
            .rpc_client
            .get_account(address)
            .await
            .map_err(|e| SolanaError::RpcError(e.to_string()))?;

        if account.owner != program_id {
            return Err(SolanaError::TransactionError(format!(
                "Account owner mismatch (expected {}, got {})",
                program_id, account.owner
            )));
        }

        let state = OnChainEscrowState::try_from_slice(&account.data)
            .map_err(|e| SolanaError::TransactionError(format!("Failed to decode escrow state: {e}")))?;

        Ok(EscrowAccount {
            address: *address,
            state,
        })
    }

    /// Fund Party A. If this is the second fund, it will auto-settle and close escrow/vaults.
    pub async fn fund_party_a(&self, funder: &Keypair, escrow: Pubkey, token_source: Option<Pubkey>) -> Result<Signature> {
        self.fund(funder, escrow, drbot_otc_escrow_program::EscrowParty::PartyA, token_source)
            .await
    }

    /// Fund Party B. If this is the second fund, it will auto-settle and close escrow/vaults.
    pub async fn fund_party_b(&self, funder: &Keypair, escrow: Pubkey, token_source: Option<Pubkey>) -> Result<Signature> {
        self.fund(funder, escrow, drbot_otc_escrow_program::EscrowParty::PartyB, token_source)
            .await
    }

    async fn fund(
        &self,
        funder: &Keypair,
        escrow: Pubkey,
        party: drbot_otc_escrow_program::EscrowParty,
        token_source: Option<Pubkey>,
    ) -> Result<Signature> {
        let program_id = self.program_id()?;
        let escrow_acct = self.get_escrow(&escrow).await?;
        let st = &escrow_acct.state;

        // Best-effort: ensure recipient ATAs exist before funding so auto-settlement cannot fail
        // due to a missing destination token account.
        let mut pre_instructions: Vec<Instruction> = Vec::new();
        if st.a_owes.kind == LegKind::SplToken {
            let recipient = Self::associated_token_address(&st.party_b, &st.a_owes.mint);
            if should_create_account(self.rpc_client.get_account(&recipient).await.as_ref().err()) {
                pre_instructions.push(
                    spl_associated_token_account::instruction::create_associated_token_account(
                        &funder.pubkey(),
                        &st.party_b,
                        &st.a_owes.mint,
                        &spl_token::id(),
                    ),
                );
            }
        }
        if st.b_owes.kind == LegKind::SplToken {
            let recipient = Self::associated_token_address(&st.party_a, &st.b_owes.mint);
            if should_create_account(self.rpc_client.get_account(&recipient).await.as_ref().err()) {
                pre_instructions.push(
                    spl_associated_token_account::instruction::create_associated_token_account(
                        &funder.pubkey(),
                        &st.party_a,
                        &st.b_owes.mint,
                        &spl_token::id(),
                    ),
                );
            }
        }

        let mut accounts = vec![
            AccountMeta::new(funder.pubkey(), true),
            AccountMeta::new(escrow, false),
            AccountMeta::new(st.party_a, false),
            AccountMeta::new(st.party_b, false),
            AccountMeta::new(st.rent_refund, false),
            AccountMeta::new_readonly(system_program::id(), false),
            AccountMeta::new_readonly(spl_token::id(), false),
        ];

        // Provide vault + recipient for any token legs (required for auto-settle).
        if st.a_owes.kind == LegKind::SplToken {
            let vault = Self::associated_token_address(&escrow, &st.a_owes.mint);
            let recipient = Self::associated_token_address(&st.party_b, &st.a_owes.mint);
            accounts.push(AccountMeta::new(vault, false));
            accounts.push(AccountMeta::new(recipient, false));
        }
        if st.b_owes.kind == LegKind::SplToken {
            let vault = Self::associated_token_address(&escrow, &st.b_owes.mint);
            let recipient = Self::associated_token_address(&st.party_a, &st.b_owes.mint);
            accounts.push(AccountMeta::new(vault, false));
            accounts.push(AccountMeta::new(recipient, false));
        }

        // Provide token source for the funding party if they owe a token leg.
        let (leg_kind, expected_mint) = match party {
            drbot_otc_escrow_program::EscrowParty::PartyA => (st.a_owes.kind, st.a_owes.mint),
            drbot_otc_escrow_program::EscrowParty::PartyB => (st.b_owes.kind, st.b_owes.mint),
        };

        if leg_kind == LegKind::SplToken {
            let source = token_source.ok_or_else(|| {
                SolanaError::ConfigError(format!(
                    "token_source is required to fund token leg (mint={})",
                    expected_mint
                ))
            })?;
            accounts.push(AccountMeta::new(source, false));
        }

        let ix = Instruction {
            program_id,
            accounts,
            data: borsh::to_vec(&EscrowInstruction::Fund { party })
                .map_err(|e| SolanaError::TransactionError(e.to_string()))?,
        };

        let bh = self
            .rpc_client
            .get_latest_blockhash()
            .await
            .map_err(|e| SolanaError::RpcError(e.to_string()))?;

        let mut ixs = pre_instructions;
        ixs.push(ix);
        let tx = Transaction::new_signed_with_payer(&ixs, Some(&funder.pubkey()), &[funder], bh);
        let sig = self
            .rpc_client
            .send_and_confirm_transaction(&tx)
            .await
            .map_err(|e| SolanaError::TransactionError(e.to_string()))?;
        Ok(sig)
    }

    /// Cancel and refund any funded legs, then close escrow and vaults.
    pub async fn cancel(&self, caller: &Keypair, escrow: Pubkey) -> Result<Signature> {
        let program_id = self.program_id()?;
        let escrow_acct = self.get_escrow(&escrow).await?;
        let st = &escrow_acct.state;

        let mut accounts = vec![
            AccountMeta::new(caller.pubkey(), true),
            AccountMeta::new(escrow, false),
            AccountMeta::new(st.party_a, false),
            AccountMeta::new(st.party_b, false),
            AccountMeta::new(st.rent_refund, false),
            AccountMeta::new_readonly(system_program::id(), false),
            AccountMeta::new_readonly(spl_token::id(), false),
        ];

        if st.a_owes.kind == LegKind::SplToken {
            let vault = Self::associated_token_address(&escrow, &st.a_owes.mint);
            let refund = Self::associated_token_address(&st.party_a, &st.a_owes.mint);
            accounts.push(AccountMeta::new(vault, false));
            accounts.push(AccountMeta::new(refund, false));
        }
        if st.b_owes.kind == LegKind::SplToken {
            let vault = Self::associated_token_address(&escrow, &st.b_owes.mint);
            let refund = Self::associated_token_address(&st.party_b, &st.b_owes.mint);
            accounts.push(AccountMeta::new(vault, false));
            accounts.push(AccountMeta::new(refund, false));
        }

        let ix = Instruction {
            program_id,
            accounts,
            data: borsh::to_vec(&EscrowInstruction::Cancel)
                .map_err(|e| SolanaError::TransactionError(e.to_string()))?,
        };

        let bh = self
            .rpc_client
            .get_latest_blockhash()
            .await
            .map_err(|e| SolanaError::RpcError(e.to_string()))?;

        let tx = Transaction::new_signed_with_payer(&[ix], Some(&caller.pubkey()), &[caller], bh);
        let sig = self
            .rpc_client
            .send_and_confirm_transaction(&tx)
            .await
            .map_err(|e| SolanaError::TransactionError(e.to_string()))?;
        Ok(sig)
    }
}

fn should_create_account(err: Option<&ClientError>) -> bool {
    let Some(err) = err else { return false };
    let msg = err.to_string();
    msg.contains("AccountNotFound") || msg.contains("could not find account") || msg.contains("could not find")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_escrow_params_to_terms() {
        let id = Uuid::new_v4();
        let a = Pubkey::new_unique();
        let b = Pubkey::new_unique();
        let params = CreateEscrowParams::new(
            id,
            a,
            b,
            Leg::native_sol(1),
            Leg::spl_token(Pubkey::new_unique(), 2),
            123,
        );

        let terms = params.to_terms();
        assert_eq!(terms.negotiation_id, *id.as_bytes());
        assert_eq!(terms.party_a, a);
        assert_eq!(terms.party_b, b);
        assert_eq!(terms.a_owes.kind, LegKind::NativeSol);
        assert_eq!(terms.b_owes.kind, LegKind::SplToken);
        assert_eq!(terms.expiry_unix_ts, 123);
    }
}
