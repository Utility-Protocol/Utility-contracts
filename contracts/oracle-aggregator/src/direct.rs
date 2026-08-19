//! Direct price-feed adapter (second provider type).
//!
//! Demonstrates multi-provider support by normalizing a non-Chainlink feed —
//! the workspace `price_oracle` contract's `get_price()` response — into the
//! same [`OracleReport`] used by [`crate::chainlink::ChainlinkAdapter`]. The
//! aggregation layer cannot tell the two apart.

use soroban_sdk::{contractclient, contracttype, Address, Env, Symbol};

use crate::adapter::OracleAdapter;
use crate::types::{AdapterKind, Error, OracleReport};

/// Response shape of the direct feed. Field names/types/order match the
/// workspace `price_oracle` contract's `PriceData` so the two interoperate
/// across the cross-contract boundary.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirectPrice {
    /// Price in the feed's smallest units.
    pub price: i128,
    /// Number of decimal places `price` is expressed in.
    pub decimals: u32,
    /// Timestamp of the last on-chain update (epoch-seconds).
    pub last_updated: u64,
}

/// Cross-contract interface to a direct price feed.
#[contractclient(name = "DirectFeedClient")]
pub trait DirectFeed {
    /// Full price snapshot (mirrors `price_oracle::get_price`).
    fn get_price(env: Env) -> DirectPrice;
}

/// Adapter for direct price feeds (e.g. the `price_oracle` contract).
pub struct DirectAdapter;

impl OracleAdapter for DirectAdapter {
    fn read(env: &Env, feed: &Address, data_key: &Symbol) -> Result<OracleReport, Error> {
        let client = DirectFeedClient::new(env, feed);
        let price = client.get_price();

        if price.price <= 0 {
            return Err(Error::InvalidValue);
        }

        Ok(OracleReport {
            provider: feed.clone(),
            adapter: AdapterKind::Direct,
            data_key: data_key.clone(),
            value: price.price,
            decimals: price.decimals,
            updated_at: price.last_updated,
        })
    }
}
