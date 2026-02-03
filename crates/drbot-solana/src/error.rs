//! Error types for Solana integration.

use solana_sdk::pubkey::Pubkey;

/// Solana errors.
#[derive(Debug, thiserror::Error)]
pub enum SolanaError {
    /// Configuration error.
    #[error("Configuration error: {0}")]
    ConfigError(String),

    /// Keypair error.
    #[error("Keypair error: {0}")]
    KeypairError(String),

    /// Insufficient balance.
    #[error("Insufficient balance: have {have} lamports, need {need}")]
    InsufficientBalance { have: u64, need: u64 },

    /// Token account not found.
    #[error("Token account not found for mint {mint}")]
    TokenAccountNotFound { mint: Pubkey },

    /// Transaction error.
    #[error("Transaction error: {0}")]
    TransactionError(String),

    /// Transaction confirmation timeout.
    #[error("Transaction confirmation timeout")]
    ConfirmationTimeout,

    /// RPC error.
    #[error("RPC error: {0}")]
    RpcError(String),

    /// Jupiter API error.
    #[error("Jupiter API error: {0}")]
    JupiterError(String),

    /// Quote not available.
    #[error("No quote available for swap")]
    NoQuoteAvailable,

    /// Slippage exceeded.
    #[error("Slippage exceeded: expected {expected}, got {actual}")]
    SlippageExceeded { expected: u64, actual: u64 },

    /// DexScreener API error.
    #[error("DexScreener API error: {0}")]
    DexScreenerError(String),

    /// GeckoTerminal API error.
    #[error("GeckoTerminal API error: {0}")]
    GeckoTerminalError(String),

    /// Rate limit exceeded.
    #[error("Rate limit exceeded")]
    RateLimitExceeded,

    /// Secret not found.
    #[error("Secret not found: {0}")]
    SecretNotFound(String),

    /// Invalid address.
    #[error("Invalid address: {0}")]
    InvalidAddress(String),

    /// Invalid pubkey.
    #[error("Invalid pubkey: {0}")]
    InvalidPubkey(String),

    /// Network error.
    #[error("Network error: {0}")]
    NetworkError(#[from] reqwest::Error),

    /// Serialization error.
    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    /// IO error.
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    /// DeFi protocol error.
    #[error("DeFi protocol error: {0}")]
    DeFiProtocolError(String),

    /// Approval required.
    #[error("Approval required for transaction: {0}")]
    ApprovalRequired(String),

    /// OTC negotiation error.
    #[error("OTC negotiation error: {0}")]
    OTCError(String),

    /// Risk analysis error.
    #[error("Risk analysis error: {0}")]
    RiskError(String),

    /// Hedging error.
    #[error("Hedging error: {0}")]
    HedgingError(String),

    /// Program monitoring error.
    #[error("Program monitoring error: {0}")]
    MonitorError(String),
}

/// Result type for Solana operations.
pub type Result<T> = std::result::Result<T, SolanaError>;

impl From<solana_client::client_error::ClientError> for SolanaError {
    fn from(e: solana_client::client_error::ClientError) -> Self {
        SolanaError::RpcError(e.to_string())
    }
}

impl From<solana_sdk::pubkey::ParsePubkeyError> for SolanaError {
    fn from(e: solana_sdk::pubkey::ParsePubkeyError) -> Self {
        SolanaError::InvalidAddress(e.to_string())
    }
}
