//! OTC settlement helpers (on-chain escrow terms).
//!
//! This module derives deterministic on-chain escrow terms from RFQ/Quote messages.
//! It is intentionally small and opinionated:
//! - RFQ initiator wallet is Party A
//! - Quote maker wallet is Party B
//! - SOL trades use **native SOL** legs when the RFQ asset mint is the canonical wSOL mint

use super::desk::wsol_mint;
use super::escrow::CreateEscrowParams;
use super::protocol::{OTCMessage, TradeDirection};
use crate::{Result, SolanaError};
use chrono::DateTime;
use drbot_otc_escrow_program::Leg;
use solana_sdk::pubkey::Pubkey;
use uuid::Uuid;

/// Build on-chain escrow parameters from an RFQ + Quote pair.
pub fn build_escrow_params(rfq: &OTCMessage, quote: &OTCMessage) -> Result<CreateEscrowParams> {
    let OTCMessage::Rfq {
        id,
        asset_mint,
        direction,
        amount: rfq_amount,
        initiator_wallet,
        expires_at,
        ..
    } = rfq
    else {
        return Err(SolanaError::OTCError("Expected RFQ message".to_string()));
    };

    let OTCMessage::Quote {
        rfq_id,
        price: _,
        quantity,
        settlement_amount,
        settlement_mint,
        maker_wallet,
        valid_until,
        ..
    } = quote
    else {
        return Err(SolanaError::OTCError("Expected Quote message".to_string()));
    };

    if rfq_id != id {
        return Err(SolanaError::OTCError(
            "Quote does not match RFQ".to_string(),
        ));
    }
    if *quantity == 0 || *settlement_amount == 0 {
        return Err(SolanaError::OTCError("Invalid quote amounts".to_string()));
    }
    if *quantity > *rfq_amount {
        return Err(SolanaError::OTCError(
            "Quote quantity exceeds RFQ amount".to_string(),
        ));
    }

    let expiry_unix_ts = min_timestamp(expires_at, valid_until);

    let asset_leg = if *asset_mint == wsol_mint() {
        Leg::native_sol(*quantity)
    } else {
        Leg::spl_token(*asset_mint, *quantity)
    };
    let settlement_leg = Leg::spl_token(*settlement_mint, *settlement_amount);

    let (a_owes, b_owes) = match direction {
        TradeDirection::Buy => (settlement_leg, asset_leg),
        TradeDirection::Sell => (asset_leg, settlement_leg),
    };

    Ok(CreateEscrowParams::new(
        *id,
        *initiator_wallet,
        *maker_wallet,
        a_owes,
        b_owes,
        expiry_unix_ts,
    ))
}

fn min_timestamp(a: &DateTime<chrono::Utc>, b: &DateTime<chrono::Utc>) -> i64 {
    let a_ts = a.timestamp();
    let b_ts = b.timestamp();
    if a_ts <= b_ts {
        a_ts
    } else {
        b_ts
    }
}

/// Determine whether a mint should be treated as native SOL for escrow legs.
pub fn is_native_sol_mint(mint: &Pubkey) -> bool {
    mint == &wsol_mint()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};
    use solana_sdk::pubkey::Pubkey;

    #[test]
    fn test_build_escrow_params_sol_usdc_buy() {
        let initiator = Pubkey::new_unique();
        let maker = Pubkey::new_unique();
        let rfq = OTCMessage::Rfq {
            id: Uuid::new_v4(),
            asset: "SOL".to_string(),
            asset_mint: wsol_mint(),
            direction: TradeDirection::Buy,
            amount: 1_000_000_000,
            initiator_wallet: initiator,
            price_limit: None,
            expires_at: Utc::now() + Duration::seconds(120),
        };

        let rfq_id = match &rfq {
            OTCMessage::Rfq { id, .. } => *id,
            _ => unreachable!(),
        };

        let quote = OTCMessage::Quote {
            id: Uuid::new_v4(),
            rfq_id,
            price: 100.0,
            quantity: 1_000_000_000,
            settlement_amount: 100_000_000,
            valid_until: Utc::now() + Duration::seconds(60),
            settlement_asset: "USDC".to_string(),
            settlement_mint: Pubkey::new_unique(),
            maker_wallet: maker,
        };

        let params = build_escrow_params(&rfq, &quote).unwrap();
        assert_eq!(params.party_a, initiator);
        assert_eq!(params.party_b, maker);
        assert_eq!(
            params.a_owes.kind,
            drbot_otc_escrow_program::LegKind::SplToken
        );
        assert_eq!(
            params.b_owes.kind,
            drbot_otc_escrow_program::LegKind::NativeSol
        );
    }
}
