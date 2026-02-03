//! Integration tests for Native SOL Staking.
//!
//! Tests the staking manager and known validators.

use drbot_solana::wallet::{KnownValidators, StakeAccountInfo, StakeResult, StakeState};
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;

#[test]
fn test_known_validators_default() {
    let validators = KnownValidators::default();
    let all = validators.all();

    assert!(all.contains_key("jito"));
    assert!(all.contains_key("marinade"));
    assert!(all.contains_key("solflare"));
    assert!(all.contains_key("everstake"));
    assert!(all.contains_key("coinbase"));
    assert!(all.contains_key("figment"));
}

#[test]
fn test_resolve_validator_by_name() {
    let validators = KnownValidators::default();

    let jito = validators.resolve("jito").unwrap();
    assert_eq!(
        jito.to_string(),
        "J1to1yufRnoWn81KYg1XkTWzmKjnYSnmE2VY8DGUJ9Qv"
    );

    let marinade = validators.resolve("marinade").unwrap();
    assert_eq!(
        marinade.to_string(),
        "mrgn28BhocwdAUEenen3Sw2MR9cPKDpLkDvzDdR7DBD"
    );
}

#[test]
fn test_resolve_validator_case_insensitive() {
    let validators = KnownValidators::default();

    let jito1 = validators.resolve("JITO").unwrap();
    let jito2 = validators.resolve("Jito").unwrap();
    let jito3 = validators.resolve("jito").unwrap();

    assert_eq!(jito1, jito2);
    assert_eq!(jito2, jito3);
}

#[test]
fn test_resolve_validator_by_address() {
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
fn test_resolve_validator_unknown() {
    let validators = KnownValidators::default();

    let result = validators.resolve("unknown_validator");
    assert!(result.is_err());
}

#[test]
fn test_add_custom_validator() {
    let mut validators = KnownValidators::default();
    let custom = Pubkey::new_unique();

    validators.add("custom".to_string(), custom);

    let resolved = validators.resolve("custom").unwrap();
    assert_eq!(resolved, custom);
}

#[test]
fn test_stake_state_display() {
    assert_eq!(StakeState::Uninitialized.to_string(), "uninitialized");
    assert_eq!(StakeState::Initialized.to_string(), "initialized");
    assert_eq!(StakeState::Delegated.to_string(), "delegated");
    assert_eq!(StakeState::Deactivating.to_string(), "deactivating");
    assert_eq!(StakeState::Inactive.to_string(), "inactive");
}

#[test]
fn test_stake_state_equality() {
    assert_eq!(StakeState::Delegated, StakeState::Delegated);
    assert_ne!(StakeState::Delegated, StakeState::Inactive);
}

#[test]
fn test_stake_account_info_serialization() {
    let info = StakeAccountInfo {
        address: Pubkey::new_unique(),
        lamports: 1_000_000_000,
        sol: 1.0,
        state: StakeState::Delegated,
        validator: Some(Pubkey::from_str("J1to1yufRnoWn81KYg1XkTWzmKjnYSnmE2VY8DGUJ9Qv").unwrap()),
        activation_epoch: Some(100),
        deactivation_epoch: None,
    };

    let json = serde_json::to_string(&info);
    assert!(json.is_ok());

    let json_str = json.unwrap();
    assert!(json_str.contains("delegated"));
    // Pubkey serializes as an array of bytes, not a base58 string
    assert!(json_str.contains("validator"));
}

#[test]
fn test_stake_result_serialization() {
    let result = StakeResult {
        signature: "abc123".to_string(),
        stake_account: Pubkey::new_unique(),
        validator: Some(Pubkey::from_str("J1to1yufRnoWn81KYg1XkTWzmKjnYSnmE2VY8DGUJ9Qv").unwrap()),
        amount_sol: 1.5,
        explorer_url: "https://solscan.io/tx/abc123".to_string(),
    };

    let json = serde_json::to_string(&result);
    assert!(json.is_ok());

    let json_str = json.unwrap();
    assert!(json_str.contains("abc123"));
    assert!(json_str.contains("1.5"));
}

#[test]
fn test_known_validator_addresses_valid() {
    let validators = vec![
        ("jito", "J1to1yufRnoWn81KYg1XkTWzmKjnYSnmE2VY8DGUJ9Qv"),
        ("marinade", "mrgn28BhocwdAUEenen3Sw2MR9cPKDpLkDvzDdR7DBD"),
        ("solflare", "SoLFLaReRVNagJzYYGppSkqzkhmHZ5ZR8EUpzqLEAaL"),
        ("everstake", "EverSFw9uN5t1V8kS3ficHUcKffSjwpGzUSGd7mgmSks"),
        ("coinbase", "beefKGBWeSpHzYBHZXwp5So7wdQGX6mu4ZHCsH3uTar"),
        ("figment", "FiGmEVpfPR8V8LAwtKMqnhH1qzFqsABG4fTqXCW8Zmvq"),
    ];

    for (name, address) in validators {
        let pubkey = Pubkey::from_str(address);
        assert!(pubkey.is_ok(), "Invalid pubkey for validator {}", name);
    }
}

#[test]
fn test_stake_account_info_inactive_state() {
    let info = StakeAccountInfo {
        address: Pubkey::new_unique(),
        lamports: 500_000_000,
        sol: 0.5,
        state: StakeState::Inactive,
        validator: Some(Pubkey::new_unique()),
        activation_epoch: Some(100),
        deactivation_epoch: Some(110),
    };

    // Inactive stake can be withdrawn
    assert_eq!(info.state, StakeState::Inactive);
    assert!(info.deactivation_epoch.is_some());
}

#[test]
fn test_stake_account_info_deactivating_state() {
    let info = StakeAccountInfo {
        address: Pubkey::new_unique(),
        lamports: 2_000_000_000,
        sol: 2.0,
        state: StakeState::Deactivating,
        validator: Some(Pubkey::new_unique()),
        activation_epoch: Some(100),
        deactivation_epoch: Some(115),
    };

    // Deactivating stake is still locked
    assert_eq!(info.state, StakeState::Deactivating);
    assert!(info.deactivation_epoch.is_some());
}
