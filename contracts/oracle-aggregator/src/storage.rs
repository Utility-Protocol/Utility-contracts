//! Storage key definitions and typed accessors.
//!
//! Keys are namespaced (prefixed with `"OAGG"`) and XDR-encoded into `Bytes`,
//! matching the convention used by `meter-aggregator`, `settlement`, and
//! `price_oracle`. The provider registry is stored as a single `Vec`, which
//! keeps iteration simple and bounded by [`crate::constants::MAX_PROVIDERS`].

use soroban_sdk::xdr::ToXdr;
use soroban_sdk::{contracttype, Address, Bytes, Env, Vec};

use crate::constants::{BOOKKEEPING_TTL_LEDGERS, NAMESPACE_PREFIX};
use crate::types::{AggregationConfig, LastGood, ProviderConfig, ProviderHealth};

/// Namespaced storage keys.
#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    /// Admin address with privileged operations.
    Admin,
    /// Aggregation/validation policy.
    AggregationConfig,
    /// Registered provider list (a single `Vec<ProviderConfig>`).
    Providers,
    /// Last-good aggregated value used by the fallback path.
    LastGood,
    /// Per-provider health telemetry.
    Health(Address),
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

// --- Aggregation config --------------------------------------------------

pub fn get_config(env: &Env) -> Option<AggregationConfig> {
    let key = DataKey::AggregationConfig.encode(env);
    env.storage().instance().get(&key)
}

pub fn set_config(env: &Env, config: &AggregationConfig) {
    let key = DataKey::AggregationConfig.encode(env);
    env.storage().instance().set(&key, config);
}

// --- Provider registry ---------------------------------------------------

/// The registered providers, or an empty `Vec` if none have been added.
pub fn get_providers(env: &Env) -> Vec<ProviderConfig> {
    let key = DataKey::Providers.encode(env);
    env.storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(|| Vec::new(env))
}

pub fn set_providers(env: &Env, providers: &Vec<ProviderConfig>) {
    let key = DataKey::Providers.encode(env);
    env.storage().persistent().set(&key, providers);
    env.storage()
        .persistent()
        .extend_ttl(&key, BOOKKEEPING_TTL_LEDGERS, BOOKKEEPING_TTL_LEDGERS);
}

// --- Last-good value -----------------------------------------------------

pub fn get_last_good(env: &Env) -> Option<LastGood> {
    let key = DataKey::LastGood.encode(env);
    env.storage().persistent().get(&key)
}

pub fn set_last_good(env: &Env, value: i128, decimals: u32, updated_at: u64) {
    let key = DataKey::LastGood.encode(env);
    let last_good = LastGood {
        value,
        decimals,
        updated_at,
    };
    env.storage().persistent().set(&key, &last_good);
    env.storage()
        .persistent()
        .extend_ttl(&key, BOOKKEEPING_TTL_LEDGERS, BOOKKEEPING_TTL_LEDGERS);
}

// --- Provider health -----------------------------------------------------

pub fn get_health(env: &Env, provider: &Address) -> Option<ProviderHealth> {
    let key = DataKey::Health(provider.clone()).encode(env);
    env.storage().persistent().get(&key)
}

pub fn set_health(env: &Env, provider: &Address, health: &ProviderHealth) {
    let key = DataKey::Health(provider.clone()).encode(env);
    env.storage().persistent().set(&key, health);
    env.storage()
        .persistent()
        .extend_ttl(&key, BOOKKEEPING_TTL_LEDGERS, BOOKKEEPING_TTL_LEDGERS);
}
