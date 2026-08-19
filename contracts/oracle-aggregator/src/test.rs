#![cfg(test)]

use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Symbol};

use crate::chainlink::ChainlinkRoundData;
use crate::constants::{DEFAULT_MAX_AGE_SECS, FALLBACK_VALUE};
use crate::direct::DirectPrice;
use crate::types::{AdapterKind, ProviderConfig};
use crate::{OracleAggregator, OracleAggregatorClient};

const T0: u64 = 1_000_000;

fn setup(env: &Env) -> (OracleAggregatorClient<'_>, Address) {
    env.mock_all_auths();
    env.ledger().set_timestamp(T0);
    let contract_id = env.register(OracleAggregator, ());
    let client = OracleAggregatorClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.initialize(&admin);
    (client, admin)
}

fn chainlink_config(env: &Env, feed: &Address) -> ProviderConfig {
    ProviderConfig {
        address: feed.clone(),
        adapter: AdapterKind::Chainlink,
        data_key: Symbol::new(env, "XLMUSD"),
        priority: 0,
        weight: 1,
        max_age_secs: DEFAULT_MAX_AGE_SECS,
    }
}

fn direct_config(env: &Env, feed: &Address) -> ProviderConfig {
    ProviderConfig {
        address: feed.clone(),
        adapter: AdapterKind::Direct,
        data_key: Symbol::new(env, "XLMUSD"),
        priority: 1,
        weight: 1,
        max_age_secs: DEFAULT_MAX_AGE_SECS,
    }
}

// --- Mock Chainlink feed -------------------------------------------------

#[contracttype]
#[derive(Clone)]
enum ChainKey {
    Round,
    Decimals,
}

#[contract]
struct MockChainlinkFeed;

#[contractimpl]
impl MockChainlinkFeed {
    pub fn initialize(env: Env, answer: i128, decimals: u32, updated_at: u64) {
        let round = ChainlinkRoundData {
            round_id: 1,
            answer,
            started_at: updated_at,
            updated_at,
            answered_in_round: 1,
        };
        env.storage().instance().set(&ChainKey::Round, &round);
        env.storage().instance().set(&ChainKey::Decimals, &decimals);
    }

    pub fn latest_round_data(env: Env) -> ChainlinkRoundData {
        env.storage().instance().get(&ChainKey::Round).unwrap()
    }

    pub fn decimals(env: Env) -> u32 {
        env.storage().instance().get(&ChainKey::Decimals).unwrap()
    }

    pub fn set_round(env: Env, answer: i128, updated_at: u64) {
        let round = ChainlinkRoundData {
            round_id: 1,
            answer,
            started_at: updated_at,
            updated_at,
            answered_in_round: 1,
        };
        env.storage().instance().set(&ChainKey::Round, &round);
    }
}

fn register_chainlink(env: &Env, answer: i128, decimals: u32, updated_at: u64) -> Address {
    let id = env.register(MockChainlinkFeed, ());
    let client = MockChainlinkFeedClient::new(env, &id);
    client.initialize(&answer, &decimals, &updated_at);
    id
}

// --- Mock direct feed ----------------------------------------------------

#[contracttype]
#[derive(Clone)]
enum DirectKey {
    Price,
}

#[contract]
struct MockDirectFeed;

#[contractimpl]
impl MockDirectFeed {
    pub fn initialize(env: Env, price: i128, decimals: u32, last_updated: u64) {
        let data = DirectPrice {
            price,
            decimals,
            last_updated,
        };
        env.storage().instance().set(&DirectKey::Price, &data);
    }

    pub fn get_price(env: Env) -> DirectPrice {
        env.storage().instance().get(&DirectKey::Price).unwrap()
    }

    pub fn set_price(env: Env, price: i128, last_updated: u64) {
        let data = DirectPrice {
            price,
            decimals: 7,
            last_updated,
        };
        env.storage().instance().set(&DirectKey::Price, &data);
    }
}

fn register_direct(env: &Env, price: i128, decimals: u32, last_updated: u64) -> Address {
    let id = env.register(MockDirectFeed, ());
    let client = MockDirectFeedClient::new(env, &id);
    client.initialize(&price, &decimals, &last_updated);
    id
}

// --- Lifecycle -----------------------------------------------------------

#[test]
fn test_initialize_and_default_config() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    assert_eq!(client.get_admin(), Some(admin));

    let config = client.get_config();
    assert_eq!(config.target_decimals, 7);
    assert_eq!(config.min_confirmations, 1);
    assert_eq!(config.max_deviation_bps, 500);
}

#[test]
fn test_double_initialize_fails() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    let other = Address::generate(&env);
    let res = client.try_initialize(&other);
    assert!(res.is_err());
}

#[test]
fn test_add_list_and_remove_provider() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    let feed = register_chainlink(&env, 100, 2, T0);
    let cfg = chainlink_config(&env, &feed);
    client.add_provider(&cfg);

    let providers = client.get_providers();
    assert_eq!(providers.len(), 1);
    assert_eq!(providers.get(0).unwrap().address, feed);

    client.remove_provider(&feed);
    assert_eq!(client.get_providers().len(), 0);
}

#[test]
fn test_add_duplicate_provider_fails() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    let feed = register_chainlink(&env, 100, 2, T0);
    client.add_provider(&chainlink_config(&env, &feed));
    let res = client.try_add_provider(&chainlink_config(&env, &feed));
    assert!(res.is_err());
}

#[test]
fn test_add_provider_with_zero_weight_fails() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    let feed = register_chainlink(&env, 100, 2, T0);
    let mut cfg = chainlink_config(&env, &feed);
    cfg.weight = 0;
    let res = client.try_add_provider(&cfg);
    assert!(res.is_err());
}

// --- Aggregation ---------------------------------------------------------

#[test]
fn test_report_aggregates_median_of_three() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    let f1 = register_chainlink(&env, 100, 2, T0);
    let f2 = register_chainlink(&env, 104, 2, T0);
    let f3 = register_chainlink(&env, 102, 2, T0);
    client.add_provider(&chainlink_config(&env, &f1));
    client.add_provider(&chainlink_config(&env, &f2));
    client.add_provider(&chainlink_config(&env, &f3));

    let result = client.report();
    assert!(!result.used_fallback);
    assert_eq!(result.value, 102_00000); // median 102, scaled 2 -> 7 decimals
    assert_eq!(result.decimals, 7);
    assert_eq!(result.providers_used, 3);
    assert_eq!(result.providers_total, 3);
    assert_eq!(client.latest_answer(), 102_00000);
}

#[test]
fn test_report_rejects_outlier() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    let f1 = register_chainlink(&env, 100, 2, T0);
    let f2 = register_chainlink(&env, 102, 2, T0);
    let f3 = register_chainlink(&env, 500, 2, T0); // wild outlier
    client.add_provider(&chainlink_config(&env, &f1));
    client.add_provider(&chainlink_config(&env, &f2));
    client.add_provider(&chainlink_config(&env, &f3));

    let result = client.report();
    assert!(!result.used_fallback);
    // Median of [100,102] after dropping 500 = 101 -> 101_00000.
    assert_eq!(result.value, 101_00000);
    assert_eq!(result.providers_used, 2);
}

#[test]
fn test_report_mixes_chainlink_and_direct_adapters() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    // Chainlink reports in 2 decimals; direct reports in 7 decimals.
    let cl = register_chainlink(&env, 200, 2, T0);
    let direct = register_direct(&env, 200_00000, 7, T0);
    client.add_provider(&chainlink_config(&env, &cl));
    client.add_provider(&direct_config(&env, &direct));

    let result = client.report();
    assert!(!result.used_fallback);
    assert_eq!(result.value, 200_00000); // both normalize to the same value
    assert_eq!(result.providers_used, 2);
}

// --- Staleness & fallback ------------------------------------------------

#[test]
fn test_stale_provider_is_excluded() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    let fresh = register_chainlink(&env, 100, 2, T0);
    let stale = register_chainlink(&env, 900, 2, 0); // ancient
    client.add_provider(&chainlink_config(&env, &fresh));
    client.add_provider(&chainlink_config(&env, &stale));

    let result = client.report();
    assert!(!result.used_fallback);
    assert_eq!(result.value, 100_00000);
    assert_eq!(result.providers_used, 1);
    assert_eq!(result.providers_total, 2);
}

#[test]
fn test_all_stale_falls_back_to_constant_on_cold_start() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    let stale = register_chainlink(&env, 100, 2, 0);
    client.add_provider(&chainlink_config(&env, &stale));

    let result = client.report();
    assert!(result.used_fallback);
    assert_eq!(result.value, FALLBACK_VALUE);
    assert_eq!(result.providers_used, 0);
}

#[test]
fn test_all_stale_falls_back_to_last_good() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    let feed = register_chainlink(&env, 250, 2, T0);
    client.add_provider(&chainlink_config(&env, &feed));

    // First report succeeds and persists 250_00000 as last-good.
    let ok = client.report();
    assert!(!ok.used_fallback);
    assert_eq!(ok.value, 250_00000);

    // Now make the feed stale and re-report -> last-good replayed.
    let feed_client = MockChainlinkFeedClient::new(&env, &feed);
    feed_client.set_round(&250, &0);
    env.ledger().set_timestamp(T0 + DEFAULT_MAX_AGE_SECS + 1);

    let fallback = client.report();
    assert!(fallback.used_fallback);
    assert_eq!(fallback.value, 250_00000);
    assert_eq!(client.latest_answer(), 250_00000);
}

#[test]
fn test_min_confirmations_not_met_falls_back() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    let feed = register_chainlink(&env, 100, 2, T0);
    client.add_provider(&chainlink_config(&env, &feed));

    let mut config = client.get_config();
    config.min_confirmations = 2;
    client.set_config(&config);

    let result = client.report();
    assert!(result.used_fallback);
    assert_eq!(result.providers_used, 0);
}

#[test]
fn test_invalid_config_rejected() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    let mut config = client.get_config();
    config.min_confirmations = 0;
    assert!(client.try_set_config(&config).is_err());
}

// --- Health monitoring ---------------------------------------------------

#[test]
fn test_health_tracking_and_summary() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    // `stale` reports a non-zero but ancient timestamp so the contract's
    // staleness path (not the adapter's zero-timestamp rejection) is exercised.
    let good = register_chainlink(&env, 100, 2, T0);
    let stale = register_chainlink(&env, 200, 2, T0 - DEFAULT_MAX_AGE_SECS - 100);
    client.add_provider(&chainlink_config(&env, &good));
    client.add_provider(&chainlink_config(&env, &stale));

    let _ = client.report();

    let good_health = client.get_health(&good).unwrap();
    assert!(good_health.is_healthy);
    assert_eq!(good_health.successful_reads, 1);
    assert_eq!(good_health.stale_reads, 0);

    let stale_health = client.get_health(&stale).unwrap();
    assert!(!stale_health.is_healthy);
    assert_eq!(stale_health.stale_reads, 1);

    let summary = client.get_health_summary();
    assert_eq!(summary.total_providers, 2);
    assert_eq!(summary.healthy_providers, 1);
    // good: 10000 bps, stale: 0 bps -> average 5000.
    assert_eq!(summary.aggregate_success_rate_bps, 5_000);
}

#[test]
fn test_health_records_failure_on_bad_value() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    // A non-positive Chainlink answer must surface as a failed read.
    let bad = register_chainlink(&env, -1, 2, T0);
    client.add_provider(&chainlink_config(&env, &bad));

    let result = client.report();
    assert!(result.used_fallback); // no valid reports

    let health = client.get_health(&bad).unwrap();
    assert!(!health.is_healthy);
    assert_eq!(health.failed_reads, 1);
}
