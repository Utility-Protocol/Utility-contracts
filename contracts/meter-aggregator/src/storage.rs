//! Storage key definitions and typed accessors.
//!
//! Keys are namespaced (prefixed with `"MTAG"`) and XDR-encoded into `Bytes`,
//! matching the convention used by the other contracts in this workspace
//! (see `resource-token` / `price_oracle`). Namespacing prevents collisions if
//! this contract is ever co-deployed or migrated alongside others.

use soroban_sdk::xdr::ToXdr;
use soroban_sdk::{contracttype, Address, Bytes, Env};

use crate::constants::BUCKET_TTL_LEDGERS;
use crate::types::{DailyBucket, HourlyBucket, RawReading};

/// Namespace prefix: "MTAG".
pub const NAMESPACE_PREFIX: [u8; 4] = [0x4d, 0x54, 0x41, 0x47];

#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    /// Admin address with privileged operations.
    Admin,
    /// Next raw-reading sequence number to assign for a device.
    ReadingSeq(Address),
    /// A raw reading for a device at a given sequence number.
    RawReading(Address, u64),
    /// Pruning watermark: the next sequence number to examine for a device.
    PruneCursor(Address),
    /// Aggregated hourly bucket: (device, hour_epoch).
    HourlyBucket(Address, u64),
    /// Aggregated daily bucket: (device, day_epoch).
    DailyBucket(Address, u64),
}

impl DataKey {
    /// Encode the key with the contract namespace prefix.
    pub fn encode(&self, env: &Env) -> Bytes {
        let mut key = Bytes::new(env);
        key.append(&Bytes::from_array(env, &NAMESPACE_PREFIX));
        key.append(&self.clone().to_xdr(env));
        key
    }
}

// --- Admin ---------------------------------------------------------------

pub fn get_admin(env: &Env) -> Option<Address> {
    let key = DataKey::Admin.encode(env);
    env.storage().instance().get(&key)
}

pub fn set_admin(env: &Env, admin: &Address) {
    let key = DataKey::Admin.encode(env);
    env.storage().instance().set(&key, admin);
}

// --- Sequence counter ----------------------------------------------------

/// The next sequence number that will be assigned to a device's raw reading.
pub fn get_next_seq(env: &Env, device: &Address) -> u64 {
    let key = DataKey::ReadingSeq(device.clone()).encode(env);
    env.storage().persistent().get(&key).unwrap_or(0)
}

pub fn set_next_seq(env: &Env, device: &Address, seq: u64) {
    let key = DataKey::ReadingSeq(device.clone()).encode(env);
    env.storage().persistent().set(&key, &seq);
    env.storage()
        .persistent()
        .extend_ttl(&key, BUCKET_TTL_LEDGERS, BUCKET_TTL_LEDGERS);
}

// --- Raw readings --------------------------------------------------------

pub fn get_raw_reading(env: &Env, device: &Address, seq: u64) -> Option<RawReading> {
    let key = DataKey::RawReading(device.clone(), seq).encode(env);
    env.storage().persistent().get(&key)
}

pub fn set_raw_reading(env: &Env, device: &Address, seq: u64, reading: &RawReading) {
    let key = DataKey::RawReading(device.clone(), seq).encode(env);
    env.storage().persistent().set(&key, reading);
}

pub fn remove_raw_reading(env: &Env, device: &Address, seq: u64) {
    let key = DataKey::RawReading(device.clone(), seq).encode(env);
    env.storage().persistent().remove(&key);
}

// --- Prune cursor --------------------------------------------------------

/// The next sequence number to examine when pruning a device's stale readings.
pub fn get_prune_cursor(env: &Env, device: &Address) -> u64 {
    let key = DataKey::PruneCursor(device.clone()).encode(env);
    env.storage().persistent().get(&key).unwrap_or(0)
}

pub fn set_prune_cursor(env: &Env, device: &Address, cursor: u64) {
    let key = DataKey::PruneCursor(device.clone()).encode(env);
    env.storage().persistent().set(&key, &cursor);
    env.storage()
        .persistent()
        .extend_ttl(&key, BUCKET_TTL_LEDGERS, BUCKET_TTL_LEDGERS);
}

// --- Hourly buckets ------------------------------------------------------

pub fn get_hourly_bucket(env: &Env, device: &Address, hour_epoch: u64) -> Option<HourlyBucket> {
    let key = DataKey::HourlyBucket(device.clone(), hour_epoch).encode(env);
    env.storage().persistent().get(&key)
}

pub fn set_hourly_bucket(env: &Env, device: &Address, hour_epoch: u64, bucket: &HourlyBucket) {
    let key = DataKey::HourlyBucket(device.clone(), hour_epoch).encode(env);
    env.storage().persistent().set(&key, bucket);
    env.storage()
        .persistent()
        .extend_ttl(&key, BUCKET_TTL_LEDGERS, BUCKET_TTL_LEDGERS);
}

pub fn remove_hourly_bucket(env: &Env, device: &Address, hour_epoch: u64) {
    let key = DataKey::HourlyBucket(device.clone(), hour_epoch).encode(env);
    env.storage().persistent().remove(&key);
}

// --- Daily buckets -------------------------------------------------------

pub fn get_daily_bucket(env: &Env, device: &Address, day_epoch: u64) -> Option<DailyBucket> {
    let key = DataKey::DailyBucket(device.clone(), day_epoch).encode(env);
    env.storage().persistent().get(&key)
}

pub fn set_daily_bucket(env: &Env, device: &Address, day_epoch: u64, bucket: &DailyBucket) {
    let key = DataKey::DailyBucket(device.clone(), day_epoch).encode(env);
    env.storage().persistent().set(&key, bucket);
    env.storage()
        .persistent()
        .extend_ttl(&key, BUCKET_TTL_LEDGERS, BUCKET_TTL_LEDGERS);
}
