#![no_std]

#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

//! # Oracle Aggregator
//!
//! A secure, multi-provider oracle integration framework for external data
//! feeds (issue #130). It normalizes heterogeneous providers behind a single
//! adapter interface, aggregates them into a robust median, validates feed
//! freshness and deviation from consensus, falls back gracefully when providers
//! fail, and tracks per-provider health.
//!
//! ## Architecture
//!
//! * [`adapter`] — the [`OracleAdapter`](adapter::OracleAdapter) normalization
//!   seam and the [`read_provider`](adapter::read_provider) dispatch.
//! * [`chainlink`] — Chainlink `AggregatorV3Interface` adapter.
//! * [`direct`] — direct price-feed adapter (multi-provider support).
//! * [`aggregation`] — median aggregation, decimal normalization, deviation and
//!   staleness validation.
//! * [`fallback`] — last-good-value / conservative-constant fallback.
//! * [`health`] — per-provider success/failure/staleness telemetry.
//!
//! ## Safety properties
//!
//! * A single broken or manipulated feed cannot move the aggregate unless it is
//!   the majority (median aggregation) and stays within the configured
//!   deviation bound (outlier rejection).
//! * A stale feed is rejected per-provider via its `max_age_secs`.
//! * If consensus cannot be formed, the aggregator falls back to the last-good
//!   value (or a conservative constant on cold start) and flags it via
//!   `used_fallback`, so consumers can react rather than trust a silent value.

pub mod adapter;
mod aggregation;
pub mod chainlink;
pub mod constants;
pub mod direct;
mod events;
mod fallback;
mod health;
mod storage;
pub mod types;

#[cfg(test)]
mod test;

use soroban_sdk::{contract, contractimpl, panic_with_error, Address, Env, Vec};

use adapter::read_provider;
use constants::{
    DEFAULT_MAX_DEVIATION_BPS, DEFAULT_MIN_CONFIRMATIONS, DEFAULT_TARGET_DECIMALS, MAX_DECIMALS,
    MAX_PROVIDERS,
};
use events::{Aggregated, Fallback, ProviderAdded, ProviderRemoved};
use health::{default_health, is_healthy, record_failure, record_stale, record_success};
use types::{
    AggregationConfig, AggregationResult, Error, HealthSummary, LastGood, ProviderConfig,
    ProviderHealth,
};

fn require_admin(env: &Env) -> Address {
    storage::get_admin(env).unwrap_or_else(|| panic_with_error!(env, Error::NotInitialized))
}

fn require_config(env: &Env) -> AggregationConfig {
    storage::get_config(env).unwrap_or_else(|| panic_with_error!(env, Error::NotInitialized))
}

/// Validate an aggregation config, trapping on out-of-bounds values.
fn validate_config(env: &Env, config: &AggregationConfig) {
    if config.target_decimals > MAX_DECIMALS {
        panic_with_error!(env, Error::InvalidConfig);
    }
    if config.min_confirmations == 0 || config.min_confirmations > MAX_PROVIDERS as u32 {
        panic_with_error!(env, Error::InvalidConfig);
    }
}

/// Validate a provider config, trapping on out-of-bounds values.
fn validate_provider(env: &Env, provider: &ProviderConfig) {
    if provider.weight == 0 {
        panic_with_error!(env, Error::InvalidConfig);
    }
    if provider.max_age_secs == 0 {
        panic_with_error!(env, Error::InvalidConfig);
    }
}

/// Build the fallback result for a run that failed to form a consensus.
fn fallback_result(
    env: &Env,
    config: &AggregationConfig,
    providers_total: u32,
) -> AggregationResult {
    let last_good = storage::get_last_good(env);
    let (value, decimals, updated_at) =
        fallback::resolve_fallback(last_good.as_ref(), config.target_decimals);
    Fallback { value, updated_at }.publish(env);
    AggregationResult {
        value,
        decimals,
        updated_at,
        providers_used: 0,
        providers_total,
        used_fallback: true,
    }
}

#[contract]
pub struct OracleAggregator;

#[contractimpl]
impl OracleAggregator {
    /// Initialize the aggregator with an admin. Callable once.
    pub fn initialize(env: Env, admin: Address) {
        if storage::get_admin(&env).is_some() {
            panic_with_error!(&env, Error::AlreadyInitialized);
        }
        admin.require_auth();
        storage::set_admin(&env, &admin);

        let config = AggregationConfig {
            target_decimals: DEFAULT_TARGET_DECIMALS,
            max_deviation_bps: DEFAULT_MAX_DEVIATION_BPS,
            min_confirmations: DEFAULT_MIN_CONFIRMATIONS,
        };
        storage::set_config(&env, &config);
    }

    /// Register a provider. Admin only.
    pub fn add_provider(env: Env, provider: ProviderConfig) {
        let admin = require_admin(&env);
        admin.require_auth();
        validate_provider(&env, &provider);

        let mut providers = storage::get_providers(&env);
        if providers.len() >= MAX_PROVIDERS as u32 {
            panic_with_error!(&env, Error::TooManyProviders);
        }
        for existing in providers.iter() {
            if existing.address == provider.address {
                panic_with_error!(&env, Error::ProviderAlreadyExists);
            }
        }

        providers.push_back(provider.clone());
        storage::set_providers(&env, &providers);

        let health = default_health(&provider.address);
        storage::set_health(&env, &provider.address, &health);

        ProviderAdded {
            provider: provider.address.clone(),
            adapter: provider.adapter,
        }
        .publish(&env);
    }

    /// Remove a registered provider. Admin only.
    pub fn remove_provider(env: Env, provider: Address) {
        let admin = require_admin(&env);
        admin.require_auth();

        let providers = storage::get_providers(&env);
        let mut remaining = Vec::new(&env);
        let mut found = false;
        for existing in providers.iter() {
            if existing.address == provider {
                found = true;
            } else {
                remaining.push_back(existing);
            }
        }
        if !found {
            panic_with_error!(&env, Error::ProviderNotFound);
        }
        storage::set_providers(&env, &remaining);
        ProviderRemoved { provider }.publish(&env);
    }

    /// Replace the aggregation/validation policy. Admin only.
    pub fn set_config(env: Env, config: AggregationConfig) {
        let admin = require_admin(&env);
        admin.require_auth();
        validate_config(&env, &config);
        storage::set_config(&env, &config);
    }

    /// Read every registered provider and produce the aggregated value.
    ///
    /// Each provider is read through its adapter, rejected if stale (vs. its own
    /// `max_age_secs`) or invalid, and its health telemetry is updated. If at
    /// least `min_confirmations` fresh reports survive, they are normalized to
    /// `target_decimals`, outliers beyond `max_deviation_bps` are dropped, and
    /// the median of the survivors is returned (and persisted as last-good).
    /// Otherwise the fallback path is taken.
    pub fn report(env: Env) -> AggregationResult {
        let config = require_config(&env);
        let providers = storage::get_providers(&env);
        let providers_total = providers.len();

        if providers_total == 0 {
            return fallback_result(&env, &config, providers_total);
        }

        let now = env.ledger().timestamp();
        let mut reports = Vec::new(&env);

        for provider in providers.iter() {
            let mut health = storage::get_health(&env, &provider.address)
                .unwrap_or_else(|| default_health(&provider.address));

            match read_provider(&env, &provider) {
                Ok(report) => {
                    if aggregation::is_stale(now, report.updated_at, provider.max_age_secs) {
                        record_stale(&mut health, now);
                        storage::set_health(&env, &provider.address, &health);
                        continue;
                    }
                    record_success(&mut health, now, report.value);
                    storage::set_health(&env, &provider.address, &health);
                    reports.push_back(report);
                }
                Err(_) => {
                    record_failure(&mut health, now);
                    storage::set_health(&env, &provider.address, &health);
                }
            }
        }

        if reports.len() < config.min_confirmations {
            return fallback_result(&env, &config, providers_total);
        }

        match aggregation::aggregate(&reports, config.target_decimals, config.max_deviation_bps) {
            Some((value, providers_used)) => {
                storage::set_last_good(&env, value, config.target_decimals, now);
                Aggregated {
                    value,
                    providers_used,
                    updated_at: now,
                }
                .publish(&env);
                AggregationResult {
                    value,
                    decimals: config.target_decimals,
                    updated_at: now,
                    providers_used,
                    providers_total,
                    used_fallback: false,
                }
            }
            None => fallback_result(&env, &config, providers_total),
        }
    }

    // --- View accessors --------------------------------------------------

    /// The most recently aggregated value (fallback constant if none recorded).
    pub fn latest_answer(env: Env) -> i128 {
        storage::get_last_good(&env)
            .map(|last| last.value)
            .unwrap_or(constants::FALLBACK_VALUE)
    }

    pub fn get_admin(env: Env) -> Option<Address> {
        storage::get_admin(&env)
    }

    pub fn get_config(env: Env) -> AggregationConfig {
        require_config(&env)
    }

    pub fn get_providers(env: Env) -> Vec<ProviderConfig> {
        storage::get_providers(&env)
    }

    pub fn get_last_good(env: Env) -> Option<LastGood> {
        storage::get_last_good(&env)
    }

    pub fn get_health(env: Env, provider: Address) -> Option<ProviderHealth> {
        storage::get_health(&env, &provider)
    }

    /// Aggregate health across all registered providers.
    pub fn get_health_summary(env: Env) -> HealthSummary {
        let providers = storage::get_providers(&env);
        let total = providers.len();
        let mut healthy = 0u32;
        let mut rate_sum = 0u64;

        for provider in providers.iter() {
            let health = storage::get_health(&env, &provider.address)
                .unwrap_or_else(|| default_health(&provider.address));
            if is_healthy(&health) {
                healthy += 1;
            }
            rate_sum += health::success_rate_bps(&health) as u64;
        }

        let aggregate_success_rate_bps = if total == 0 {
            0
        } else {
            (rate_sum / total as u64) as u32
        };

        HealthSummary {
            total_providers: total,
            healthy_providers: healthy,
            aggregate_success_rate_bps,
        }
    }
}
