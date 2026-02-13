//! Integration tests for Agent-to-Agent OTC Negotiation.
//!
//! Tests the OTC protocol messages and trade direction types.

use drbot_solana::otc::{OTCMessage, TradeDirection};
use solana_sdk::pubkey::Pubkey;
use uuid::Uuid;

#[test]
fn test_otc_message_rfq() {
    let msg = OTCMessage::rfq(
        "SOL",
        Pubkey::new_unique(),
        TradeDirection::Buy,
        100_000_000_000, // 100 SOL
        300,             // 5 minutes
        Pubkey::new_unique(),
    );

    match msg {
        OTCMessage::Rfq {
            asset,
            amount,
            direction,
            ..
        } => {
            assert_eq!(asset, "SOL");
            assert_eq!(amount, 100_000_000_000);
            assert!(matches!(direction, TradeDirection::Buy));
        }
        _ => panic!("Expected RFQ message"),
    }
}

#[test]
fn test_otc_message_quote() {
    let rfq_id = Uuid::new_v4();
    let msg = OTCMessage::quote(
        rfq_id,
        100.0,              // price
        100_000_000_000,    // quantity
        10_000_000_000_000, // settlement amount (micros)
        "USDC",
        Pubkey::new_unique(),
        Pubkey::new_unique(), // maker wallet
        120,                  // valid for 2 minutes
    );

    match msg {
        OTCMessage::Quote {
            rfq_id: rid,
            price,
            quantity,
            ..
        } => {
            assert_eq!(rid, rfq_id);
            assert_eq!(price, 100.0);
            assert_eq!(quantity, 100_000_000_000);
        }
        _ => panic!("Expected Quote message"),
    }
}

#[test]
fn test_otc_message_counter_offer_direct() {
    use chrono::{Duration, Utc};

    let quote_id = Uuid::new_v4();
    let msg = OTCMessage::CounterOffer {
        id: Uuid::new_v4(),
        quote_id,
        new_price: 98.0,
        new_quantity: None,
        valid_until: Utc::now() + Duration::seconds(120),
    };

    match msg {
        OTCMessage::CounterOffer {
            quote_id: qid,
            new_price,
            ..
        } => {
            assert_eq!(qid, quote_id);
            assert_eq!(new_price, 98.0);
        }
        _ => panic!("Expected CounterOffer message"),
    }
}

#[test]
fn test_otc_message_accept_direct() {
    let quote_id = Uuid::new_v4();
    let wallet = Pubkey::new_unique();
    let msg = OTCMessage::Accept {
        quote_id,
        accepting_wallet: wallet,
    };

    match msg {
        OTCMessage::Accept {
            quote_id: qid,
            accepting_wallet,
        } => {
            assert_eq!(qid, quote_id);
            assert_eq!(accepting_wallet, wallet);
        }
        _ => panic!("Expected Accept message"),
    }
}

#[test]
fn test_otc_message_reject_direct() {
    let quote_id = Uuid::new_v4();
    let msg = OTCMessage::Reject {
        quote_id,
        reason: Some("Price too high".to_string()),
    };

    match msg {
        OTCMessage::Reject {
            quote_id: qid,
            reason,
        } => {
            assert_eq!(qid, quote_id);
            assert_eq!(reason, Some("Price too high".to_string()));
        }
        _ => panic!("Expected Reject message"),
    }
}

#[test]
fn test_trade_direction() {
    assert!(matches!(TradeDirection::Buy, TradeDirection::Buy));
    assert!(matches!(TradeDirection::Sell, TradeDirection::Sell));
    assert_ne!(TradeDirection::Buy, TradeDirection::Sell);
}

#[test]
fn test_trade_direction_opposite() {
    assert_eq!(TradeDirection::Buy.opposite(), TradeDirection::Sell);
    assert_eq!(TradeDirection::Sell.opposite(), TradeDirection::Buy);
}

#[test]
fn test_calculate_trade_value() {
    let price = 100.0;
    let quantity = 50_000_000_000u64; // 50 SOL in lamports
    let decimals = 9;

    let ui_quantity = quantity as f64 / 10f64.powi(decimals);
    let value = price * ui_quantity;

    assert_eq!(ui_quantity, 50.0);
    assert_eq!(value, 5000.0);
}

#[test]
fn test_otc_message_serialization() {
    let msg = OTCMessage::rfq(
        "SOL",
        Pubkey::new_unique(),
        TradeDirection::Buy,
        100_000_000_000,
        300,
        Pubkey::new_unique(),
    );

    // Should be serializable
    let json = serde_json::to_string(&msg);
    assert!(json.is_ok());

    // Should contain expected fields
    let json_str = json.unwrap();
    assert!(json_str.contains("rfq"));
    assert!(json_str.contains("SOL"));
}

#[test]
fn test_otc_message_type() {
    let rfq = OTCMessage::rfq(
        "SOL",
        Pubkey::new_unique(),
        TradeDirection::Buy,
        1_000_000_000,
        300,
        Pubkey::new_unique(),
    );
    assert_eq!(rfq.message_type(), "rfq");

    let quote = OTCMessage::quote(
        Uuid::new_v4(),
        100.0,
        1_000_000_000,
        100_000_000,
        "USDC",
        Pubkey::new_unique(),
        Pubkey::new_unique(),
        60,
    );
    assert_eq!(quote.message_type(), "quote");
}
