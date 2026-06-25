#![no_std]
//! # Meter Aggregator
//!
//! Collects raw meter readings per device and keeps per-device storage **bounded**
//! regardless of device lifetime or submission frequency.
//!
//! ## The problem this solves
//!
//! Appending every raw reading to an ever-growing per-device vector exhausts
//! Soroban contract storage. At one reading every ~5s (≈17,280/day) a naive
//! design blows the contract storage budget within hours, after which **all**
//! further readings and settlements for the device fail — a cheap denial of
//! service.
//!
//! ## The mechanism
//!
//! * Each raw reading is stored under its own key `RawReading(device, seq)` with
//!   a monotonically increasing sequence number, so submissions are O(1) and seq
//!   order matches time order.
//! * On every submission the value is folded into the matching **hourly** and
//!   **daily** rollup buckets (`checked_add`, overflow-safe).
//! * Raw readings older than [`constants::MAX_RAW_RETENTION_SECS`] are pruned
//!   **inline**, amortized to O(1) per submission via a watermark cursor
//!   (`PruneCursor(device)`), deleting at most [`constants::PRUNE_BATCH_SIZE`]
//!   entries per call so a backlog drains over several submissions.
//! * Long-term volume lives in the compact rollup buckets, queried by
//!   [`MeterAggregator::get_aggregated_volume`] which reads daily → hourly → raw
//!   in that order of preference.
//!
//! Net effect: live raw storage is capped at roughly one retention window of
//! readings; everything older is represented by a handful of bytes per
//! hour/day.

use soroban_sdk::{contract, contractimpl, panic_with_error, Address, Env};

pub mod constants;
pub mod storage;
pub mod types;

#[cfg(test)]
mod test;

use constants::{
    HOURS_PER_DAY, MAX_RAW_RETENTION_SECS, PRUNE_BATCH_SIZE, ROLLUP_INTERVAL_SECS, SECONDS_PER_DAY,
};
use types::{DailyBucket, Error, HourlyBucket, RawReading};

/// Overflow-checked `i128` addition that traps with [`Error::Overflow`].
fn checked_add(env: &Env, a: i128, b: i128) -> i128 {
    match a.checked_add(b) {
        Some(v) => v,
        None => panic_with_error!(env, Error::Overflow),
    }
}

fn require_admin(env: &Env) -> Address {
    match storage::get_admin(env) {
        Some(a) => a,
        None => panic_with_error!(env, Error::NotInitialized),
    }
}

#[contract]
pub struct MeterAggregator;

#[contractimpl]
impl MeterAggregator {
    /// Initialize the contract with an admin. Callable once.
    pub fn initialize(env: Env, admin: Address) {
        if storage::get_admin(&env).is_some() {
            panic_with_error!(&env, Error::AlreadyInitialized);
        }
        admin.require_auth();
        storage::set_admin(&env, &admin);
    }

    /// Submit a raw meter reading for `device`, signed by `source`.
    ///
    /// Stores the raw reading, folds it into the hourly/daily rollups, then
    /// prunes up to [`PRUNE_BATCH_SIZE`] stale readings. Returns the sequence
    /// number assigned to the reading.
    pub fn submit_reading(env: Env, device: Address, source: Address, value: i128) -> u64 {
        // Must be initialized (guarantees an admin exists for privileged ops).
        require_admin(&env);

        if value < 0 {
            panic_with_error!(&env, Error::NegativeValue);
        }

        source.require_auth();

        let ts = env.ledger().timestamp();
        let seq = storage::get_next_seq(&env, &device);

        let reading = RawReading {
            timestamp: ts,
            value,
            source: source.clone(),
        };
        storage::set_raw_reading(&env, &device, seq, &reading);
        storage::set_next_seq(&env, &device, seq + 1);

        rollup_raw_to_hourly(&env, &device, ts, value);

        prune_stale_readings(&env, &device);

        seq
    }

    /// Maintenance entry point: prune up to [`PRUNE_BATCH_SIZE`] stale raw
    /// readings for `device`. Callable by anyone (purely deterministic cleanup).
    /// Returns the number of readings pruned by this call.
    pub fn prune(env: Env, device: Address) -> u32 {
        prune_stale_readings(&env, &device)
    }

    /// Consolidate a completed day's hourly buckets for `device`, reclaiming
    /// their storage.
    ///
    /// The daily bucket is maintained incrementally on each submission, so this
    /// only deletes the now-redundant hourly buckets for `day_epoch` to keep
    /// hourly-bucket growth bounded over the device's lifetime. Idempotent.
    /// Admin only. Returns the day's total volume.
    pub fn rollup_day(env: Env, device: Address, day_epoch: u64) -> i128 {
        let admin = require_admin(&env);
        admin.require_auth();

        let start_hour = day_epoch * HOURS_PER_DAY;
        let mut i = 0u64;
        while i < HOURS_PER_DAY {
            storage::remove_hourly_bucket(&env, &device, start_hour + i);
            i += 1;
        }

        storage::get_daily_bucket(&env, &device, day_epoch)
            .map(|d| d.total)
            .unwrap_or(0)
    }

    /// Total volume for `device` over the inclusive hour window covering
    /// `[from_ts, to_ts]`.
    ///
    /// Evaluated at hour-bucket granularity. Reads are tiered for efficiency and
    /// correctness: a fully-covered day uses its `DailyBucket`; otherwise each
    /// hour uses its `HourlyBucket`; if a bucket is missing (e.g. not yet rolled
    /// up) the live raw readings for that hour are summed as a fallback.
    pub fn get_aggregated_volume(env: Env, device: Address, from_ts: u64, to_ts: u64) -> i128 {
        if from_ts > to_ts {
            panic_with_error!(&env, Error::InvalidTimeRange);
        }

        let from_hour = from_ts / ROLLUP_INTERVAL_SECS;
        let to_hour = to_ts / ROLLUP_INTERVAL_SECS;

        let mut total: i128 = 0;
        let mut h = from_hour;
        while h <= to_hour {
            let day = h / HOURS_PER_DAY;
            let day_start_hour = day * HOURS_PER_DAY;
            let day_end_hour = day_start_hour + HOURS_PER_DAY - 1;

            // Fast path: the whole day fits inside the window — use the daily bucket.
            if h == day_start_hour && day_end_hour <= to_hour {
                if let Some(d) = storage::get_daily_bucket(&env, &device, day) {
                    total = checked_add(&env, total, d.total);
                    h = day_end_hour + 1;
                    continue;
                }
            }

            // Hour tier, falling back to the live raw readings for the hour.
            match storage::get_hourly_bucket(&env, &device, h) {
                Some(b) => total = checked_add(&env, total, b.total),
                None => total = checked_add(&env, total, sum_raw_for_hour(&env, &device, h)),
            }
            h += 1;
        }

        total
    }

    // --- View accessors --------------------------------------------------

    pub fn get_admin(env: Env) -> Option<Address> {
        storage::get_admin(&env)
    }

    pub fn get_hourly_bucket(env: Env, device: Address, hour_epoch: u64) -> Option<HourlyBucket> {
        storage::get_hourly_bucket(&env, &device, hour_epoch)
    }

    pub fn get_daily_bucket(env: Env, device: Address, day_epoch: u64) -> Option<DailyBucket> {
        storage::get_daily_bucket(&env, &device, day_epoch)
    }

    pub fn get_raw_reading(env: Env, device: Address, seq: u64) -> Option<RawReading> {
        storage::get_raw_reading(&env, &device, seq)
    }

    /// The pruning watermark: the next sequence number that will be examined.
    pub fn get_prune_cursor(env: Env, device: Address) -> u64 {
        storage::get_prune_cursor(&env, &device)
    }

    /// Total number of raw readings ever submitted for `device` (next seq).
    pub fn get_reading_count(env: Env, device: Address) -> u64 {
        storage::get_next_seq(&env, &device)
    }

    /// Number of raw readings still live in storage (submitted minus pruned).
    pub fn get_live_reading_count(env: Env, device: Address) -> u64 {
        storage::get_next_seq(&env, &device) - storage::get_prune_cursor(&env, &device)
    }
}

/// Fold a single reading into its hourly and daily rollup buckets.
fn rollup_raw_to_hourly(env: &Env, device: &Address, ts: u64, value: i128) {
    let hour = ts / ROLLUP_INTERVAL_SECS;
    let mut hb = storage::get_hourly_bucket(env, device, hour).unwrap_or(HourlyBucket {
        hour_epoch: hour,
        total: 0,
        count: 0,
    });
    hb.total = checked_add(env, hb.total, value);
    hb.count += 1;
    storage::set_hourly_bucket(env, device, hour, &hb);

    let day = ts / SECONDS_PER_DAY;
    let mut db = storage::get_daily_bucket(env, device, day).unwrap_or(DailyBucket {
        day_epoch: day,
        total: 0,
        count: 0,
    });
    db.total = checked_add(env, db.total, value);
    db.count += 1;
    storage::set_daily_bucket(env, device, day, &db);
}

/// Prune stale raw readings using the watermark cursor.
///
/// Readings are stored in sequence (= time) order, so we advance the cursor from
/// the oldest unexamined sequence number, deleting readings whose age exceeds
/// the retention window, and stop at the first still-fresh reading. At most
/// [`PRUNE_BATCH_SIZE`] readings are deleted per call.
fn prune_stale_readings(env: &Env, device: &Address) -> u32 {
    let now = env.ledger().timestamp();
    // A reading is stale when `now - timestamp > retention`, i.e. `timestamp < cutoff`.
    // A reading exactly `retention` seconds old (timestamp == cutoff) is kept.
    let cutoff = now.saturating_sub(MAX_RAW_RETENTION_SECS);

    let next = storage::get_next_seq(env, device);
    let mut cursor = storage::get_prune_cursor(env, device);
    let mut pruned: u32 = 0;

    while cursor < next && pruned < PRUNE_BATCH_SIZE {
        match storage::get_raw_reading(env, device, cursor) {
            Some(r) => {
                if r.timestamp < cutoff {
                    storage::remove_raw_reading(env, device, cursor);
                    cursor += 1;
                    pruned += 1;
                } else {
                    // First fresh reading in time order — nothing older remains.
                    break;
                }
            }
            // Already-removed gap: skip without counting against the batch.
            None => cursor += 1,
        }
    }

    storage::set_prune_cursor(env, device, cursor);
    pruned
}

/// Sum live raw readings whose timestamp falls in `hour_epoch`. Fallback used by
/// [`MeterAggregator::get_aggregated_volume`] when a bucket is absent; bounded by
/// the live (un-pruned) sequence range.
fn sum_raw_for_hour(env: &Env, device: &Address, hour_epoch: u64) -> i128 {
    let next = storage::get_next_seq(env, device);
    let mut seq = storage::get_prune_cursor(env, device);
    let mut sum: i128 = 0;
    while seq < next {
        if let Some(r) = storage::get_raw_reading(env, device, seq) {
            if r.timestamp / ROLLUP_INTERVAL_SECS == hour_epoch {
                sum = checked_add(env, sum, r.value);
            }
        }
        seq += 1;
    }
    sum
}
