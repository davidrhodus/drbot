//! NFT marketplace integrations (Tensor, Magic Eden).
//!
//! Provides access to NFT trading on Solana's major marketplaces.

use crate::{Result, SolanaError};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use std::sync::Arc;
use tracing::{debug, info};

/// Tensor program ID.
pub const TENSOR_SWAP_PROGRAM_ID: &str = "TSWAPaqyCSx2KABk68Shruf4rp7CxcNi8hAsbdwmHbN";

/// Magic Eden program ID (v2).
pub const MAGIC_EDEN_PROGRAM_ID: &str = "M2mx93ekt1fmXSVkTrUL9xVFHkmME8HTUi5Cyc5aF7K";

/// Tensor API base URL.
const TENSOR_API_URL: &str = "https://api.tensor.so/graphql";

/// Magic Eden API base URL.
const MAGIC_EDEN_API_URL: &str = "https://api-mainnet.magiceden.dev/v2";

/// NFT collection information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NftCollection {
    /// Collection symbol/slug.
    pub symbol: String,
    /// Collection name.
    pub name: String,
    /// Floor price in SOL.
    pub floor_price: f64,
    /// 24h volume in SOL.
    pub volume_24h: f64,
    /// Number of listed items.
    pub listed_count: u64,
    /// Total supply.
    pub total_supply: Option<u64>,
    /// Collection image URL.
    pub image_url: Option<String>,
}

/// NFT listing information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NftListing {
    /// NFT mint address.
    pub mint: String,
    /// NFT name.
    pub name: String,
    /// Collection symbol.
    pub collection: String,
    /// Listing price in SOL.
    pub price: f64,
    /// Seller address.
    pub seller: String,
    /// Marketplace (Tensor, MagicEden).
    pub marketplace: String,
    /// Image URL.
    pub image_url: Option<String>,
    /// Rarity rank (if available).
    pub rarity_rank: Option<u32>,
}

/// NFT owned by a user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OwnedNft {
    /// NFT mint address.
    pub mint: String,
    /// NFT name.
    pub name: String,
    /// Collection symbol.
    pub collection: Option<String>,
    /// Image URL.
    pub image_url: Option<String>,
    /// Estimated value in SOL (based on floor).
    pub estimated_value: Option<f64>,
}

/// Parameters for listing an NFT.
#[derive(Debug, Clone)]
pub struct ListNftParams {
    /// NFT mint address.
    pub mint: String,
    /// Listing price in SOL.
    pub price: f64,
    /// Marketplace to list on.
    pub marketplace: Marketplace,
}

/// Parameters for buying an NFT.
#[derive(Debug, Clone)]
pub struct BuyNftParams {
    /// NFT mint address.
    pub mint: String,
    /// Maximum price in SOL.
    pub max_price: f64,
    /// Preferred marketplace (or any).
    pub marketplace: Option<Marketplace>,
}

/// Supported marketplaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Marketplace {
    Tensor,
    MagicEden,
}

impl std::fmt::Display for Marketplace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Marketplace::Tensor => write!(f, "Tensor"),
            Marketplace::MagicEden => write!(f, "Magic Eden"),
        }
    }
}

/// NFT marketplace client.
pub struct NftClient {
    rpc_client: Arc<RpcClient>,
    http_client: Client,
    tensor_program_id: Pubkey,
    magic_eden_program_id: Pubkey,
}

impl NftClient {
    /// Create a new NFT client.
    pub fn new(rpc_client: Arc<RpcClient>) -> Self {
        Self {
            rpc_client,
            http_client: Client::new(),
            tensor_program_id: TENSOR_SWAP_PROGRAM_ID.parse().unwrap(),
            magic_eden_program_id: MAGIC_EDEN_PROGRAM_ID.parse().unwrap(),
        }
    }

    /// Get the Tensor program ID.
    pub fn tensor_program_id(&self) -> &Pubkey {
        &self.tensor_program_id
    }

    /// Get the Magic Eden program ID.
    pub fn magic_eden_program_id(&self) -> &Pubkey {
        &self.magic_eden_program_id
    }

    /// Get collection stats from Magic Eden.
    pub async fn get_collection(&self, symbol: &str) -> Result<Option<NftCollection>> {
        debug!(symbol = symbol, "Fetching collection stats");

        let url = format!("{}/collections/{}/stats", MAGIC_EDEN_API_URL, symbol);

        let response = self.http_client.get(&url).send().await?;

        if !response.status().is_success() {
            return Ok(None);
        }

        let data: MagicEdenCollectionStats = match response.json().await {
            Ok(d) => d,
            Err(_) => return Ok(None),
        };

        Ok(Some(NftCollection {
            symbol: symbol.to_string(),
            name: symbol.to_string(), // ME stats don't include name
            floor_price: data.floor_price.unwrap_or(0.0) / 1e9, // lamports to SOL
            volume_24h: data.volume_24hr.unwrap_or(0.0) / 1e9,
            listed_count: data.listed_count.unwrap_or(0),
            total_supply: None,
            image_url: None,
        }))
    }

    /// Get popular collections.
    pub async fn get_popular_collections(&self) -> Result<Vec<NftCollection>> {
        debug!("Fetching popular collections");

        let url = format!("{}/collections?offset=0&limit=20", MAGIC_EDEN_API_URL);

        let response = self.http_client.get(&url).send().await?;

        if !response.status().is_success() {
            return Ok(self.default_collections());
        }

        let data: Vec<MagicEdenCollection> = response.json().await.unwrap_or_default();

        let collections: Vec<NftCollection> = data
            .into_iter()
            .take(20)
            .map(|c| NftCollection {
                symbol: c.symbol,
                name: c.name,
                floor_price: c.floor_price.unwrap_or(0.0) / 1e9,
                volume_24h: c.volume_all.unwrap_or(0.0) / 1e9,
                listed_count: c.listed_count.unwrap_or(0),
                total_supply: c.total_items,
                image_url: c.image,
            })
            .collect();

        Ok(collections)
    }

    /// Search collections by name.
    pub async fn search_collections(&self, query: &str) -> Result<Vec<NftCollection>> {
        let collections = self.get_popular_collections().await?;
        let query_lower = query.to_lowercase();

        Ok(collections
            .into_iter()
            .filter(|c| {
                c.name.to_lowercase().contains(&query_lower)
                    || c.symbol.to_lowercase().contains(&query_lower)
            })
            .collect())
    }

    /// Get listings for a collection.
    pub async fn get_listings(&self, collection: &str, limit: usize) -> Result<Vec<NftListing>> {
        debug!(collection = collection, "Fetching listings");

        let url = format!(
            "{}/collections/{}/listings?offset=0&limit={}",
            MAGIC_EDEN_API_URL, collection, limit
        );

        let response = self.http_client.get(&url).send().await?;

        if !response.status().is_success() {
            return Ok(vec![]);
        }

        let data: Vec<MagicEdenListing> = response.json().await.unwrap_or_default();

        let listings: Vec<NftListing> = data
            .into_iter()
            .map(|l| NftListing {
                mint: l.token_mint,
                name: l.token_name.unwrap_or_default(),
                collection: collection.to_string(),
                price: l.price / 1e9,
                seller: l.seller,
                marketplace: "MagicEden".to_string(),
                image_url: l.extra.and_then(|e| e.img),
                rarity_rank: None,
            })
            .collect();

        debug!(count = listings.len(), "Fetched listings");
        Ok(listings)
    }

    /// Get NFTs owned by a user.
    pub async fn get_owned_nfts(&self, owner: &Pubkey) -> Result<Vec<OwnedNft>> {
        debug!(owner = %owner, "Fetching owned NFTs");

        let url = format!(
            "{}/wallets/{}/tokens?offset=0&limit=100&listStatus=both",
            MAGIC_EDEN_API_URL, owner
        );

        let response = self.http_client.get(&url).send().await?;

        if !response.status().is_success() {
            return Ok(vec![]);
        }

        let data: Vec<MagicEdenWalletToken> = response.json().await.unwrap_or_default();

        let nfts: Vec<OwnedNft> = data
            .into_iter()
            .map(|t| OwnedNft {
                mint: t.mint_address,
                name: t.name,
                collection: t.collection,
                image_url: t.image,
                estimated_value: t.floor_price.map(|p| p / 1e9),
            })
            .collect();

        debug!(count = nfts.len(), "Fetched owned NFTs");
        Ok(nfts)
    }

    /// List an NFT for sale.
    pub async fn list_nft(&self, params: ListNftParams) -> Result<String> {
        info!(
            mint = %params.mint,
            price = params.price,
            marketplace = %params.marketplace,
            "NFT listing requested"
        );

        Err(SolanaError::DeFiProtocolError(
            "NFT listing not yet implemented - requires on-chain interaction".to_string(),
        ))
    }

    /// Buy an NFT.
    pub async fn buy_nft(&self, params: BuyNftParams) -> Result<String> {
        info!(
            mint = %params.mint,
            max_price = params.max_price,
            "NFT purchase requested"
        );

        Err(SolanaError::DeFiProtocolError(
            "NFT purchase not yet implemented - requires on-chain interaction".to_string(),
        ))
    }

    /// Cancel an NFT listing.
    pub async fn cancel_listing(&self, mint: &str) -> Result<String> {
        info!(mint = %mint, "NFT listing cancellation requested");

        Err(SolanaError::DeFiProtocolError(
            "NFT listing cancellation not yet implemented - requires on-chain interaction"
                .to_string(),
        ))
    }

    /// Get floor price for a collection.
    pub async fn get_floor_price(&self, collection: &str) -> Result<f64> {
        match self.get_collection(collection).await? {
            Some(c) => Ok(c.floor_price),
            None => Err(SolanaError::DeFiProtocolError(format!(
                "Collection not found: {}",
                collection
            ))),
        }
    }

    /// Get default/popular collections.
    fn default_collections(&self) -> Vec<NftCollection> {
        vec![
            NftCollection {
                symbol: "degods".to_string(),
                name: "DeGods".to_string(),
                floor_price: 0.0,
                volume_24h: 0.0,
                listed_count: 0,
                total_supply: Some(10000),
                image_url: None,
            },
            NftCollection {
                symbol: "okay_bears".to_string(),
                name: "Okay Bears".to_string(),
                floor_price: 0.0,
                volume_24h: 0.0,
                listed_count: 0,
                total_supply: Some(10000),
                image_url: None,
            },
            NftCollection {
                symbol: "mad_lads".to_string(),
                name: "Mad Lads".to_string(),
                floor_price: 0.0,
                volume_24h: 0.0,
                listed_count: 0,
                total_supply: Some(10000),
                image_url: None,
            },
        ]
    }
}

/// Magic Eden API response types.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MagicEdenCollectionStats {
    floor_price: Option<f64>,
    listed_count: Option<u64>,
    volume_24hr: Option<f64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MagicEdenCollection {
    symbol: String,
    name: String,
    floor_price: Option<f64>,
    volume_all: Option<f64>,
    listed_count: Option<u64>,
    total_items: Option<u64>,
    image: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MagicEdenListing {
    token_mint: String,
    token_name: Option<String>,
    price: f64,
    seller: String,
    extra: Option<MagicEdenListingExtra>,
}

#[derive(Debug, Deserialize)]
struct MagicEdenListingExtra {
    img: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MagicEdenWalletToken {
    mint_address: String,
    name: String,
    collection: Option<String>,
    image: Option<String>,
    floor_price: Option<f64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_program_ids() {
        assert!(TENSOR_SWAP_PROGRAM_ID.parse::<Pubkey>().is_ok());
        assert!(MAGIC_EDEN_PROGRAM_ID.parse::<Pubkey>().is_ok());
    }

    #[test]
    fn test_marketplace_display() {
        assert_eq!(Marketplace::Tensor.to_string(), "Tensor");
        assert_eq!(Marketplace::MagicEden.to_string(), "Magic Eden");
    }

    #[test]
    fn test_collection_serialization() {
        let collection = NftCollection {
            symbol: "degods".to_string(),
            name: "DeGods".to_string(),
            floor_price: 5.5,
            volume_24h: 100.0,
            listed_count: 500,
            total_supply: Some(10000),
            image_url: None,
        };

        let json = serde_json::to_string(&collection);
        assert!(json.is_ok());
    }

    #[test]
    fn test_listing_serialization() {
        let listing = NftListing {
            mint: "test_mint".to_string(),
            name: "Test NFT #1".to_string(),
            collection: "test_collection".to_string(),
            price: 1.5,
            seller: "seller_address".to_string(),
            marketplace: "MagicEden".to_string(),
            image_url: None,
            rarity_rank: Some(100),
        };

        let json = serde_json::to_string(&listing);
        assert!(json.is_ok());
    }
}
