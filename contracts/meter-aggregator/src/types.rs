//! Value types stored by the meter aggregator.

use soroban_sdk::{contracterror, contracttype, Address};

/// A single raw meter reading as submitted by a device/source.
///
/// Raw readings are short-lived: they are immediately folded into the matching
/// hourly bucket and pruned once older than [`crate::constants::MAX_RAW_RETENTION_SECS`].
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RawReading {
    /// Ledger timestamp (seconds) at which the reading was recorded.
    pub timestamp: u64,
    /// Consumption value in fixed-point units (7 decimals).
    pub value: i128,
    /// Address that submitted the reading.
    pub source: Address,
}

/// Aggregated consumption for one hour window (`hour_epoch = timestamp / 3600`).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HourlyBucket {
    /// Hour index since the unix epoch (`timestamp / ROLLUP_INTERVAL_SECS`).
    pub hour_epoch: u64,
    /// Sum of all reading values in this hour (fixed-point, 7 decimals).
    pub total: i128,
    /// Number of readings folded into this bucket.
    pub count: u32,
}

/// Aggregated consumption for one day window (`day_epoch = timestamp / 86400`).
///
/// Produced by consolidating the 24 hourly buckets of a day via
/// [`crate::MeterAggregator::rollup_day`].
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DailyBucket {
    /// Day index since the unix epoch (`timestamp / SECONDS_PER_DAY`).
    pub day_epoch: u64,
    /// Sum of all reading values in this day (fixed-point, 7 decimals).
    pub total: i128,
    /// Number of readings folded into this bucket.
    pub count: u32,
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
    /// Aggregation would overflow `i128`.
    Overflow = 4,
    /// `from_ts` is greater than `to_ts` in a range query.
    InvalidTimeRange = 5,
    /// Reading value must be non-negative.
    NegativeValue = 6,
}
