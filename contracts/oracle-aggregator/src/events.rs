//! Typed contract events published by the aggregator.
//!
//! Defined with the modern `#[contractevent]` macro so the events are included
//! in the contract's interface specification and usable by indexers, SDKs, and
//! generated clients. Static topics are kept short (≤ 10 bytes) so they remain
//! inline "short" symbols.

use soroban_sdk::{contractevent, Address};

use crate::types::AdapterKind;

/// Emitted when a provider is registered.
#[contractevent(topics = ["prov_add"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderAdded {
    /// The provider that was added (dynamic topic).
    #[topic]
    pub provider: Address,
    /// The adapter used to read the provider.
    pub adapter: AdapterKind,
}

/// Emitted when a provider is removed.
#[contractevent(topics = ["prov_rm"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderRemoved {
    /// The provider that was removed (dynamic topic).
    #[topic]
    pub provider: Address,
}

/// Emitted when a fresh consensus value is produced.
#[contractevent(topics = ["agg"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Aggregated {
    /// The aggregated value.
    pub value: i128,
    /// Number of provider reports used in the aggregation.
    pub providers_used: u32,
    /// Timestamp the aggregation was computed.
    pub updated_at: u64,
}

/// Emitted when the fallback path is taken (no fresh consensus).
#[contractevent(topics = ["fbk"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Fallback {
    /// The fallback value.
    pub value: i128,
    /// Timestamp of the last-good value (0 for the cold-start constant).
    pub updated_at: u64,
}
