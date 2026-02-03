//! Keypair management for Solana wallets.

use crate::{Result, SolanaError};
use solana_sdk::{
    pubkey::Pubkey,
    signature::{Keypair, Signer},
};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, warn};

/// Manages keypairs for Solana operations.
pub struct KeypairManager {
    /// Secret key name in drbot-secrets.
    secret_key_name: String,
    /// Cached keypair.
    cached_keypair: Arc<RwLock<Option<Keypair>>>,
}

impl KeypairManager {
    /// Create a new keypair manager.
    pub fn new(secret_key_name: String) -> Self {
        Self {
            secret_key_name,
            cached_keypair: Arc::new(RwLock::new(None)),
        }
    }

    /// Get the secret key name.
    pub fn secret_key_name(&self) -> &str {
        &self.secret_key_name
    }

    /// Load the keypair from the secrets vault.
    pub async fn load_keypair(&self) -> Result<Keypair> {
        // Check cache first
        {
            let cache = self.cached_keypair.read().await;
            if let Some(ref kp) = *cache {
                return Ok(kp.insecure_clone());
            }
        }

        // Load from secrets
        let secret_value = self.load_secret_value().await?;
        let keypair = self.parse_keypair(&secret_value)?;

        // Cache it
        {
            let mut cache = self.cached_keypair.write().await;
            *cache = Some(keypair.insecure_clone());
        }

        debug!(pubkey = %keypair.pubkey(), "Loaded keypair from secrets");
        Ok(keypair)
    }

    /// Get the public key without exposing the private key.
    pub async fn pubkey(&self) -> Result<Pubkey> {
        let keypair = self.load_keypair().await?;
        Ok(keypair.pubkey())
    }

    /// Clear the cached keypair.
    pub async fn clear_cache(&self) {
        let mut cache = self.cached_keypair.write().await;
        *cache = None;
        debug!("Cleared keypair cache");
    }

    /// Load secret value from drbot-secrets.
    async fn load_secret_value(&self) -> Result<String> {
        // In a real implementation, this would use drbot-secrets
        // For now, we'll check environment variables as a fallback
        if let Ok(value) = std::env::var(&self.secret_key_name) {
            return Ok(value);
        }

        // Try common patterns
        let env_var = format!("DRBOT_SOLANA_{}", self.secret_key_name.to_uppercase());
        if let Ok(value) = std::env::var(&env_var) {
            return Ok(value);
        }

        Err(SolanaError::SecretNotFound(self.secret_key_name.clone()))
    }

    /// Parse keypair from various formats.
    fn parse_keypair(&self, value: &str) -> Result<Keypair> {
        let value = value.trim();

        // Try base58 encoded private key
        if let Ok(bytes) = bs58::decode(value).into_vec() {
            if bytes.len() == 64 {
                return Keypair::try_from(bytes.as_slice())
                    .map_err(|e| SolanaError::KeypairError(e.to_string()));
            }
            // Some wallets export just the secret key (32 bytes)
            if bytes.len() == 32 {
                warn!("Loaded 32-byte secret key, deriving full keypair");
                let mut secret = [0u8; 32];
                secret.copy_from_slice(&bytes);
                return Ok(Keypair::new_from_array(secret));
            }
        }

        // Try JSON array format [1,2,3,...]
        if value.starts_with('[') {
            let bytes: Vec<u8> = serde_json::from_str(value)
                .map_err(|e| SolanaError::KeypairError(e.to_string()))?;
            if bytes.len() == 64 {
                return Keypair::try_from(bytes.as_slice())
                    .map_err(|e| SolanaError::KeypairError(e.to_string()));
            }
            if bytes.len() == 32 {
                let mut secret = [0u8; 32];
                secret.copy_from_slice(&bytes);
                return Ok(Keypair::new_from_array(secret));
            }
            return Err(SolanaError::KeypairError(format!(
                "Invalid keypair length: expected 32 or 64, got {}",
                bytes.len()
            )));
        }

        Err(SolanaError::KeypairError(
            "Unknown keypair format. Expected base58 or JSON array.".to_string(),
        ))
    }

    /// Generate a new keypair (for testing/development).
    pub fn generate() -> Keypair {
        Keypair::new()
    }

    /// Get the base58 representation of a keypair (for backup).
    pub fn to_base58(keypair: &Keypair) -> String {
        bs58::encode(keypair.to_bytes()).into_string()
    }
}

/// A keypair manager loaded from a file.
///
/// This is a simpler, synchronous alternative to KeypairManager
/// for use with file-based wallets.
pub struct FileKeypairManager {
    keypair: Keypair,
    path: std::path::PathBuf,
}

impl FileKeypairManager {
    /// Load keypair from a JSON file (solana-keygen format).
    pub fn from_file(path: &std::path::Path) -> Result<Self> {
        let data = std::fs::read_to_string(path)
            .map_err(|e| SolanaError::KeypairError(format!("Failed to read file: {}", e)))?;

        // Parse the JSON array of bytes
        let bytes: Vec<u8> = serde_json::from_str(&data)
            .map_err(|e| SolanaError::KeypairError(format!("Failed to parse JSON: {}", e)))?;

        let keypair = if bytes.len() == 64 {
            Keypair::try_from(bytes.as_slice())
                .map_err(|e| SolanaError::KeypairError(e.to_string()))?
        } else if bytes.len() == 32 {
            let mut secret = [0u8; 32];
            secret.copy_from_slice(&bytes);
            Keypair::new_from_array(secret)
        } else {
            return Err(SolanaError::KeypairError(format!(
                "Invalid keypair length in file: expected 32 or 64, got {}",
                bytes.len()
            )));
        };

        Ok(Self {
            keypair,
            path: path.to_path_buf(),
        })
    }

    /// Get a reference to the keypair.
    pub fn keypair(&self) -> &Keypair {
        &self.keypair
    }

    /// Get the public key.
    pub fn pubkey(&self) -> Pubkey {
        self.keypair.pubkey()
    }

    /// Get the file path.
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    /// Save the keypair to a file.
    pub fn save(&self, path: &std::path::Path) -> Result<()> {
        let bytes = self.keypair.to_bytes().to_vec();
        let json = serde_json::to_string(&bytes)
            .map_err(|e| SolanaError::KeypairError(format!("Failed to serialize: {}", e)))?;

        std::fs::write(path, json)
            .map_err(|e| SolanaError::KeypairError(format!("Failed to write file: {}", e)))?;

        // Set permissions on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
        }

        Ok(())
    }
}

/// Wallet information.
#[derive(Debug, Clone)]
pub struct WalletInfo {
    /// Wallet public key (address).
    pub address: Pubkey,
    /// SOL balance in lamports.
    pub sol_balance: u64,
    /// Token balances.
    pub token_balances: Vec<TokenBalance>,
}

impl WalletInfo {
    /// Get SOL balance in SOL (not lamports).
    pub fn sol_balance_ui(&self) -> f64 {
        self.sol_balance as f64 / 1_000_000_000.0
    }

    /// Get total USD value (if prices are available).
    pub fn total_usd_value(&self) -> Option<f64> {
        let mut total = 0.0;
        let mut has_price = false;

        for balance in &self.token_balances {
            if let Some(usd) = balance.usd_value {
                total += usd;
                has_price = true;
            }
        }

        if has_price {
            Some(total)
        } else {
            None
        }
    }
}

/// Token balance information.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TokenBalance {
    /// Token mint address.
    pub mint: Pubkey,
    /// Token symbol.
    pub symbol: Option<String>,
    /// Token name.
    pub name: Option<String>,
    /// Raw balance (in smallest units).
    pub balance: u64,
    /// Token decimals.
    pub decimals: u8,
    /// UI-friendly balance.
    pub ui_amount: f64,
    /// USD value (if price is available).
    pub usd_value: Option<f64>,
}

impl TokenBalance {
    /// Create a new token balance.
    pub fn new(mint: Pubkey, balance: u64, decimals: u8) -> Self {
        let ui_amount = balance as f64 / 10f64.powi(decimals as i32);
        Self {
            mint,
            symbol: None,
            name: None,
            balance,
            decimals,
            ui_amount,
            usd_value: None,
        }
    }

    /// Set token metadata.
    pub fn with_metadata(mut self, symbol: &str, name: &str) -> Self {
        self.symbol = Some(symbol.to_string());
        self.name = Some(name.to_string());
        self
    }

    /// Set USD value.
    pub fn with_usd_value(mut self, usd: f64) -> Self {
        self.usd_value = Some(usd);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_keypair() {
        let keypair = KeypairManager::generate();
        assert!(!keypair.pubkey().to_string().is_empty());
    }

    #[test]
    fn test_keypair_to_base58() {
        let keypair = KeypairManager::generate();
        let base58 = KeypairManager::to_base58(&keypair);
        assert!(!base58.is_empty());

        // Should be able to decode back
        let bytes = bs58::decode(&base58).into_vec().unwrap();
        assert_eq!(bytes.len(), 64);
    }

    #[test]
    fn test_token_balance() {
        let mint = Pubkey::new_unique();
        let balance = TokenBalance::new(mint, 1_000_000, 6)
            .with_metadata("USDC", "USD Coin")
            .with_usd_value(1.0);

        assert_eq!(balance.ui_amount, 1.0);
        assert_eq!(balance.symbol, Some("USDC".to_string()));
        assert_eq!(balance.usd_value, Some(1.0));
    }

    #[test]
    fn test_wallet_info() {
        let wallet = WalletInfo {
            address: Pubkey::new_unique(),
            sol_balance: 1_000_000_000, // 1 SOL
            token_balances: vec![],
        };

        assert_eq!(wallet.sol_balance_ui(), 1.0);
    }
}
