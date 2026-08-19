//! Value types stored and exchanged by the oracle aggregator.

use soroban_sdk::{contracterror, contracttype, Address, Symbol};

/// Identifies which adapter normalizes a provider's feed.
///
/// Each variant maps to a concrete [`crate::adapter::OracleAdapter`]
/// implementation. Adding a provider type is additive: register a new variant
/// here and dispatch it in [`crate::OracleAggregator`]'s read path.
#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum AdapterKind {
    /// Chainlink-style `AggregatorV3Interface` feed.
    Chainlink = 0,
    /// Direct price feed (e.g. the workspace `price_oracle` contract).
    Direct = 1,
}

/// A single, normalized feed report produced by an adapter.
///
/// Adapters translate heterogeneous provider responses into this common shape
/// so aggregation and validation are provider-agnostic.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OracleReport {
    /// Address of the provider contract that produced this report.
    pub provider: Address,
    /// Which adapter normalized the raw response.
    pub adapter: AdapterKind,
    /// The feed identifier within the provider (e.g. `"XLMUSD"`).
    pub data_key: Symbol,
    /// Raw value in the provider's own decimal precision.
    pub value: i128,
    /// Decimal precision of [`Self::value`].
    pub decimals: u32,
    /// Provider-reported timestamp of the last on-chain update (epoch-seconds).
    pub updated_at: u64,
}

/// Configuration for one registered oracle provider.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderConfig {
    /// Feed contract address.
    pub address: Address,
    /// Adapter used to read this provider.
    pub adapter: AdapterKind,
    /// Feed identifier passed to the adapter (e.g. `"XLMUSD"`).
    pub data_key: Symbol,
    /// Priority bucket: lower is preferred. `0` = primary, `1` = secondary, …
    pub priority: u32,
    /// Aggregation weight (`0` is invalid; higher weight = more influence when
    /// a weighted average is requested).
    pub weight: u32,
    /// Maximum age (seconds) of this provider's feed before it is stale.
    pub max_age_secs: u64,
}

/// Aggregation and validation policy.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AggregationConfig {
    /// Common precision all reports are normalized to before aggregation.
    pub target_decimals: u32,
    /// Maximum deviation (basis points) from the consensus median tolerated.
    pub max_deviation_bps: u32,
    /// Minimum number of fresh, valid reports required to avoid fallback.
    pub min_confirmations: u32,
}

/// Result of a [`crate::OracleAggregator::report`] run.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AggregationResult {
    /// Aggregated (or fallback) value in [`Self::decimals`] precision.
    pub value: i128,
    /// Decimal precision of [`Self::value`] (the configured target decimals).
    pub decimals: u32,
    /// Timestamp at which this result was computed (epoch-seconds).
    pub updated_at: u64,
    /// Number of provider reports actually used in the aggregation.
    pub providers_used: u32,
    /// Total number of providers registered at the time of the run.
    pub providers_total: u32,
    /// Whether the result came from the fallback path (no fresh consensus).
    pub used_fallback: bool,
}

/// The last-good aggregated value persisted for fallback use.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LastGood {
    /// Last successfully aggregated value in [`Self::decimals`] precision.
    pub value: i128,
    /// Decimal precision of [`Self::value`].
    pub decimals: u32,
    /// Timestamp the last-good value was recorded (epoch-seconds).
    pub updated_at: u64,
}

/// Per-provider health telemetry.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderHealth {
    /// Provider this telemetry describes.
    pub provider: Address,
    /// Total number of read attempts against this provider.
    pub total_reads: u64,
    /// Number of reads that produced a fresh, valid report.
    pub successful_reads: u64,
    /// Number of reads that failed (cross-contract error or invalid value).
    pub failed_reads: u64,
    /// Number of reads rejected for staleness.
    pub stale_reads: u64,
    /// Timestamp of the last successful read (0 = never).
    pub last_success_at: u64,
    /// Timestamp of the last failed/stale read (0 = never).
    pub last_failure_at: u64,
    /// Most recent value observed from this provider.
    pub last_value: i128,
    /// Whether the most recent read succeeded.
    pub is_healthy: bool,
}

/// Roll-up of provider health for monitoring dashboards and alerting.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HealthSummary {
    /// Number of providers registered.
    pub total_providers: u32,
    /// Number of providers whose most recent read succeeded.
    pub healthy_providers: u32,
    /// Aggregate success rate across all providers, in basis points.
    pub aggregate_success_rate_bps: u32,
}

/// Errors surfaced by the contract.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    /// `initialize` has not been called yet.
    NotInitialized = 1,
    /// `initialize` was called more than once.
    AlreadyInitialized = 2,
    /// Caller is not the configured admin.
    NotAuthorized = 3,
    /// No providers are registered.
    NoProviders = 4,
    /// The requested provider is not registered.
    ProviderNotFound = 5,
    /// A provider with the same address is already registered.
    ProviderAlreadyExists = 6,
    /// The provider registry is at capacity.
    TooManyProviders = 7,
    /// A feed's `updated_at` exceeds its configured maximum age.
    StaleFeed = 8,
    /// A feed reported a non-positive (or otherwise invalid) value.
    InvalidValue = 9,
    /// A report deviated from consensus by more than the configured bound.
    DeviationExceeded = 10,
    /// Every registered provider failed or was stale.
    AllFeedsFailed = 11,
    /// A provider or aggregation config failed validation.
    InvalidConfig = 12,
}
