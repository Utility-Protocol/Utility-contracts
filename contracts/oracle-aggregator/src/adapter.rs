//! Oracle adapter interface (issue #130, step 1).
//!
//! Heterogeneous price feeds (Chainlink, direct price oracles, future
//! providers) expose different method names, argument lists, and response
//! shapes. [`OracleAdapter`] is the normalization seam: each implementation
//! knows how to call one kind of feed and returns a single [`OracleReport`]
//! regardless of the underlying contract.
//!
//! Adding a provider type is additive:
//!
//! 1. add an [`AdapterKind`] variant,
//! 2. implement [`OracleAdapter`] for a new adapter struct, and
//! 3. extend [`read_provider`] to dispatch on the new variant.

use soroban_sdk::{Address, Env, Symbol};

use crate::types::{AdapterKind, Error, OracleReport, ProviderConfig};

/// A provider-agnostic oracle reader.
///
/// Implementations must translate a provider-specific response into an
/// [`OracleReport`], applying the *minimum* per-provider validation (positive
/// value, sane timestamp). Aggregation-level validation (staleness vs. the
/// provider's `max_age_secs`, deviation vs. consensus) happens in the contract
/// so it stays consistent across adapters.
pub trait OracleAdapter {
    /// Read the feed at `feed` identified by `data_key`, returning a normalized
    /// report or the first validation error encountered.
    fn read(env: &Env, feed: &Address, data_key: &Symbol) -> Result<OracleReport, Error>;
}

/// Dispatch a [`ProviderConfig`] to its concrete adapter.
///
/// The single place adapters are selected from on-chain state; the mapping from
/// `AdapterKind` to implementation is exhaustive (no silent "unknown adapter").
pub fn read_provider(env: &Env, provider: &ProviderConfig) -> Result<OracleReport, Error> {
    match provider.adapter {
        AdapterKind::Chainlink => {
            crate::chainlink::ChainlinkAdapter::read(env, &provider.address, &provider.data_key)
        }
        AdapterKind::Direct => {
            crate::direct::DirectAdapter::read(env, &provider.address, &provider.data_key)
        }
    }
}
