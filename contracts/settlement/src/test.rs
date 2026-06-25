extern crate std;

use crate::{
    constants::{BPS_DENOMINATOR, MAX_FEE_RATE_BPS, MAX_SETTLEMENT, MIN_FEE_RATE_BPS},
    fees::compute_fee,
    token_utils::verify_fee_invariant,
};

/// Round-half-up invariant: |fee * 10000 - amount * rate_bps| <= 5000
fn invariant_error_bounded(amount: i128, rate_bps: u32) -> bool {
    let fee = compute_fee(amount, rate_bps);
    let scaled_fee = fee * BPS_DENOMINATOR as i128;
    let exact = amount * rate_bps as i128;
    (scaled_fee - exact).abs() <= 5000
}

#[test]
fn test_edge_cases_property_based() {
    let rates = [
        MIN_FEE_RATE_BPS,
        10,
        50,
        100,
        250,
        500,
        750,
        1000,
        MAX_FEE_RATE_BPS,
    ];
    let amounts = [
        1,
        10,
        100,
        1000,
        10_000,
        100_000,
        1_000_000,
        10_000_000,
        100_000_000,
        1_000_000_000,
        10_000_000_000,
        100_000_000_000,
        1_000_000_000_000,
        10_000_000_000_000,
        100_000_000_000_000,
        1_000_000_000_000_000,
        10_000_000_000_000_000,
        100_000_000_000_000_000,
        MAX_SETTLEMENT,
    ];

    for &amount in &amounts {
        for &rate_bps in &rates {
            let fee = compute_fee(amount, rate_bps);

            assert!(
                invariant_error_bounded(amount, rate_bps),
                "FAIL error_bounded: amount={}, rate_bps={}, fee={}",
                amount,
                rate_bps,
                fee
            );

            assert!(
                verify_fee_invariant(amount, rate_bps, fee),
                "FAIL verify_fee_invariant: amount={}, rate_bps={}, fee={}",
                amount,
                rate_bps,
                fee
            );
        }
    }
}

#[test]
fn test_micro_settlement_extraction_prevention() {
    let rate_bps = 100;
    let micro_amount: i128 = 1;

    let per_txn_fee = compute_fee(micro_amount, rate_bps);
    assert_eq!(
        per_txn_fee, 0,
        "Micro-settlement fee should be 0 for rate_bps < 5000"
    );

    let micro_fee_higher_rate = compute_fee(micro_amount, 5001);
    assert_eq!(
        micro_fee_higher_rate, 1,
        "Round-half-up should round 5001/10000 = 1 for micro amount"
    );

    let num_txns: i128 = 1_000_000;
    let mut total_truncation_style: i128 = 0;
    for _ in 0..num_txns {
        total_truncation_style += compute_fee(micro_amount, rate_bps);
    }
    assert_eq!(total_truncation_style, 0);

    let mut total_at_5001: i128 = 0;
    for _ in 0..num_txns {
        total_at_5001 += compute_fee(micro_amount, 5001);
    }
    assert_eq!(total_at_5001, num_txns);

    let lump_sum = compute_fee(num_txns * micro_amount, 5001);
    let diff = (total_at_5001 - lump_sum).abs();
    let max_per_txn_error = 5000i128;
    let max_cumulative_error = num_txns * max_per_txn_error;
    assert!(
        diff <= max_cumulative_error,
        "Cumulative micro fee error {} exceeds max {} (micro={}, lump={})",
        diff,
        max_cumulative_error,
        total_at_5001,
        lump_sum
    );

    assert!(verify_fee_invariant(micro_amount, 5001, micro_fee_higher_rate));
}

#[test]
fn test_truncation_vs_rounding_comparison() {
    let test_cases: [(i128, u32, i128, i128); 5] = [
        (1, 5000, 0, 1),
        (1, 5001, 0, 1),
        (1, 9999, 0, 1),
        (3, 3333, 0, 1),
        (2, 2500, 0, 1),
    ];

    for &(amount, rate_bps, truncation, rounding) in &test_cases {
        let trunc_result = (amount * rate_bps as i128) / BPS_DENOMINATOR as i128;
        assert_eq!(trunc_result, truncation, "Truncation mismatch");
        let round_result = compute_fee(amount, rate_bps);
        assert_eq!(round_result, rounding, "Rounding mismatch");
        assert!(
            round_result >= trunc_result,
            "Rounding must not reduce fee vs truncation"
        );
    }
}

#[test]
fn test_zero_and_edge_inputs() {
    assert_eq!(compute_fee(0, 100), 0, "Zero amount");
    assert_eq!(compute_fee(0, 0), 0, "Zero amount and rate");
    assert_eq!(compute_fee(100, 0), 0, "Zero rate");

    let fee = compute_fee(MAX_SETTLEMENT, MAX_FEE_RATE_BPS);
    let expected = (MAX_SETTLEMENT * MAX_FEE_RATE_BPS as i128 + 5000) / 10000;
    assert_eq!(fee, expected);
}

#[test]
fn test_randomized_properties() {
    for amount in 1..=1000 {
        for rate_bps in 1..=100 {
            let fee = compute_fee(amount, rate_bps);
            assert!(
                verify_fee_invariant(amount, rate_bps, fee),
                "Failed at amount={}, rate_bps={}, fee={}",
                amount,
                rate_bps,
                fee
            );
        }
    }
}

use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{contract, contractimpl, contracttype, token, Address, Env};

use crate::constants::{DECIMAL_DENOMINATOR, FALLBACK_RATE, MAX_ORACLE_AGE};

/// A malicious token whose `transfer` re-enters the settlement contract,
/// modelling the cross-contract reentrancy attack from issue #8.
#[contracttype]
#[derive(Clone)]
enum MalKey {
    Settlement,
    Payer,
    Collector,
}

#[contract]
struct MaliciousToken;

#[contractimpl]
impl MaliciousToken {
    pub fn setup(env: Env, settlement: Address, payer: Address, collector: Address) {
        env.storage().instance().set(&MalKey::Settlement, &settlement);
        env.storage().instance().set(&MalKey::Payer, &payer);
        env.storage().instance().set(&MalKey::Collector, &collector);
    }

    /// SEP-41 `transfer` entry point invoked by the settlement contract's
    /// `collect_fee`. Instead of moving tokens it attempts to re-enter `settle`,
    /// which must be rejected by the reentrancy guard.
    pub fn transfer(env: Env, _from: Address, _to: Address, _amount: i128) {
        let settlement: Address = env.storage().instance().get(&MalKey::Settlement).unwrap();
        let payer: Address = env.storage().instance().get(&MalKey::Payer).unwrap();
        let collector: Address = env.storage().instance().get(&MalKey::Collector).unwrap();
        let me = env.current_contract_address();

        let client = SettlementContractClient::new(&env, &settlement);
        // Reentrant call: settle again using this malicious token as the token.
        client.settle(&me, &payer, &payer, &collector, &10_000i128, &100u32);
    }
}

use crate::types::SettlementArgs;
use crate::{OraclePrice, SettlementContract, SettlementContractClient};

#[contracttype]
#[derive(Clone)]
enum OracleDataKey {
    Price,
    Updated,
}

#[contract]
struct MockOracle;

#[contractimpl]
impl MockOracle {
    pub fn initialize(env: Env, _admin: Address, _updater: Address, initial_price: i128, _decimals: u32) {
        env.storage().instance().set(&OracleDataKey::Price, &initial_price);
        let now = env.ledger().timestamp();
        env.storage().instance().set(&OracleDataKey::Updated, &now);
    }

    pub fn get_price_value(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&OracleDataKey::Price)
            .unwrap_or(0)
    }

    /// Full snapshot consumed by the settlement staleness check.
    pub fn get_price(env: Env) -> OraclePrice {
        OraclePrice {
            price: env.storage().instance().get(&OracleDataKey::Price).unwrap_or(0),
            decimals: 7,
            last_updated: env.storage().instance().get(&OracleDataKey::Updated).unwrap_or(0),
        }
    }

    /// Test helper: override the price's last-updated timestamp to simulate a
    /// stale (or freshly-updated) feed.
    pub fn set_updated(env: Env, ts: u64) {
        env.storage().instance().set(&OracleDataKey::Updated, &ts);
    }
}

fn setup_env() -> (Env, Address, Address, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let payer = Address::generate(&env);
    let payee = Address::generate(&env);
    let fee_collector = Address::generate(&env);

    let oracle_id = env.register(MockOracle, ());
    let oracle_mock_client = MockOracleClient::new(&env, &oracle_id);
    oracle_mock_client.initialize(&admin, &admin, &100_000_0000i128, &7);

    (env, oracle_id, admin, payer, payee, fee_collector)
}

fn setup_token(env: &Env, payer: &Address, balance: i128) -> Address {
    let token = env.register_stellar_asset_contract_v2(payer.clone());
    let token_addr = token.address();
    let admin_client = token::StellarAssetClient::new(env, &token_addr);
    admin_client.mint(payer, &balance);
    token_addr
}

fn settlement_amount(volume: i128, rate: i128) -> i128 {
    volume * rate / DECIMAL_DENOMINATOR
}

fn setup_settlement(env: &Env) -> Address {
    env.register(SettlementContract, ())
}

fn make_args(
    token_address: Address,
    volume: i128,
    recipient: Address,
    min_expected_amount: Option<i128>,
) -> SettlementArgs {
    SettlementArgs {
        token_address,
        volume,
        recipient,
        min_expected_amount,
    }
}

#[test]
fn test_zero_slippage_succeeds() {
    let (env, oracle_id, _admin, payer, payee, fee_collector) = setup_env();
    let settlement_id = setup_settlement(&env);
    let settlement_client = SettlementContractClient::new(&env, &settlement_id);

    let rate = 100_000_0000i128;
    let volume = 1_000_0000i128;
    let expected_amount = settlement_amount(volume, rate);
    let token_id = setup_token(&env, &payer, expected_amount);

    let args = make_args(token_id, volume, payee.clone(), None);

    let result = settlement_client.finalize_settlement(
        &oracle_id,
        &payer,
        &fee_collector,
        &args,
        &100u32,
    );

    assert!(result.net_amount > 0);
    assert_eq!(result.rate_used, rate);
}

#[test]
fn test_slippage_within_tolerance_succeeds() {
    let (env, oracle_id, _admin, payer, payee, fee_collector) = setup_env();
    let settlement_id = setup_settlement(&env);
    let settlement_client = SettlementContractClient::new(&env, &settlement_id);

    let rate = 100_000_0000i128;
    let volume = 1_000_0000i128;
    let expected_amount = settlement_amount(volume, rate);
    let token_id = setup_token(&env, &payer, expected_amount);

    let min_expected = expected_amount * 99 / 100;

    let args = make_args(token_id, volume, payee.clone(), Some(min_expected));

    let result = settlement_client.finalize_settlement(
        &oracle_id,
        &payer,
        &fee_collector,
        &args,
        &100u32,
    );

    assert!(result.net_amount > 0);
    assert_eq!(result.rate_used, rate);
}

#[test]
fn test_slippage_exceeds_tolerance_fails() {
    let (env, oracle_id, _admin, payer, payee, fee_collector) = setup_env();
    let settlement_id = setup_settlement(&env);
    let settlement_client = SettlementContractClient::new(&env, &settlement_id);

    let rate = 100_000_0000i128;
    let volume = 1_000_0000i128;
    let expected_amount = settlement_amount(volume, rate);
    let token_id = setup_token(&env, &payer, expected_amount);

    let min_expected = expected_amount + 1;

    let args = make_args(token_id, volume, payee.clone(), Some(min_expected));

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        settlement_client.finalize_settlement(
            &oracle_id,
            &payer,
            &fee_collector,
            &args,
            &100u32,
        );
    }));

    assert!(result.is_err());
}

#[test]
fn test_user_min_expected_amount_higher_than_actual_fails() {
    let (env, oracle_id, _admin, payer, payee, fee_collector) = setup_env();
    let settlement_id = setup_settlement(&env);
    let settlement_client = SettlementContractClient::new(&env, &settlement_id);

    let rate = 100_000_0000i128;
    let volume = 1_000_0000i128;
    let expected_amount = settlement_amount(volume, rate);
    let token_id = setup_token(&env, &payer, expected_amount);

    let min_expected = expected_amount * 200;

    let args = make_args(token_id, volume, payee.clone(), Some(min_expected));

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        settlement_client.finalize_settlement(
            &oracle_id,
            &payer,
            &fee_collector,
            &args,
            &100u32,
        );
    }));

    assert!(result.is_err());
}

// --- Oracle staleness protection (issue #7) ------------------------------

/// Pure unit tests for the staleness predicate and rate application.
mod staleness_unit {
    use crate::constants::{
        DECIMAL_DENOMINATOR, FALLBACK_RATE, MAX_ORACLE_AGE, MAX_ORACLE_AGE_BOUND,
        MIN_ORACLE_AGE_BOUND,
    };
    use crate::rate_application::{apply_rate_to_volume, compute_fallback_rate, is_stale};

    #[test]
    fn test_max_oracle_age_within_bounds() {
        assert!(MAX_ORACLE_AGE >= MIN_ORACLE_AGE_BOUND);
        assert!(MAX_ORACLE_AGE <= MAX_ORACLE_AGE_BOUND);
    }

    #[test]
    fn test_is_stale_boundary() {
        let updated = 1_000u64;
        // age == MAX_ORACLE_AGE -> still fresh.
        assert!(!is_stale(updated + MAX_ORACLE_AGE, updated));
        // age == MAX_ORACLE_AGE + 1 -> stale.
        assert!(is_stale(updated + MAX_ORACLE_AGE + 1, updated));
        // clock skew (updated in the future) -> treated as fresh, no underflow.
        assert!(!is_stale(updated, updated + 50));
    }

    #[test]
    fn test_apply_rate_to_volume() {
        // volume * rate / 1e7
        assert_eq!(apply_rate_to_volume(1_000_0000, FALLBACK_RATE), 5_000_000);
        assert_eq!(apply_rate_to_volume(0, FALLBACK_RATE), 0);
        assert_eq!(
            apply_rate_to_volume(DECIMAL_DENOMINATOR, DECIMAL_DENOMINATOR),
            DECIMAL_DENOMINATOR
        );
    }

    #[test]
    fn test_compute_fallback_rate() {
        assert_eq!(compute_fallback_rate(), FALLBACK_RATE);
    }
}

#[test]
fn test_settle_normal_flow_with_guard() {
    // The reentrancy guard must not break a legitimate single (non-reentrant) call.
    let (env, _oracle_id, _admin, payer, payee, fee_collector) = setup_env();
    let settlement_id = setup_settlement(&env);
    let settlement_client = SettlementContractClient::new(&env, &settlement_id);

    let token_id = setup_token(&env, &payer, 1_000_000i128);

    let (net, fee) =
        settlement_client.settle(&token_id, &payer, &payee, &fee_collector, &10_000i128, &100u32);

    assert_eq!(fee, 100);
    assert_eq!(net, 9_900);
}

#[test]
fn test_reentrancy_attack_is_blocked() {
    // settle -> collect_fee -> malicious token.transfer -> reentrant settle.
    // The second `settle` must hit the held lock and panic, aborting the tx.
    let (env, _oracle_id, _admin, payer, _payee, fee_collector) = setup_env();
    let settlement_id = setup_settlement(&env);
    let settlement_client = SettlementContractClient::new(&env, &settlement_id);

    let mal_id = env.register(MaliciousToken, ());
    let mal_client = MaliciousTokenClient::new(&env, &mal_id);
    mal_client.setup(&settlement_id, &payer, &fee_collector);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        settlement_client.settle(
            &mal_id,
            &payer,
            &payer, // payee
            &fee_collector,
            &10_000i128,
            &100u32,
        );
    }));

    assert!(result.is_err(), "reentrant settle must be rejected by the guard");
}

#[test]
fn test_reentrancy_attack_via_finalize_is_blocked() {
    // Same attack through the oracle-based entry point: collect_fee invokes the
    // malicious token, which re-enters settle while finalize_settlement holds
    // the lock.
    let (env, oracle_id, _admin, payer, payee, fee_collector) = setup_env();
    let settlement_id = setup_settlement(&env);
    let settlement_client = SettlementContractClient::new(&env, &settlement_id);

    let mal_id = env.register(MaliciousToken, ());
    let mal_client = MaliciousTokenClient::new(&env, &mal_id);
    mal_client.setup(&settlement_id, &payer, &fee_collector);

    let volume = 1_000_0000i128;
    let args = make_args(mal_id, volume, payee.clone(), None);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        settlement_client.finalize_settlement(&oracle_id, &payer, &fee_collector, &args, &100u32);
    }));

    assert!(result.is_err(), "reentrant finalize_settlement must be rejected");
}

#[test]
fn test_fresh_oracle_under_threshold_uses_oracle_rate() {
    // Fresh feed (age 0) -> the oracle price is used directly.
    let (env, oracle_id, _admin, payer, payee, fee_collector) = setup_env();
    let settlement_id = setup_settlement(&env);
    let settlement_client = SettlementContractClient::new(&env, &settlement_id);

    let rate = 100_000_0000i128;
    let volume = 1_000_0000i128;
    let token_id = setup_token(&env, &payer, settlement_amount(volume, rate));

    let args = make_args(token_id, volume, payee.clone(), None);

    let result =
        settlement_client.finalize_settlement(&oracle_id, &payer, &fee_collector, &args, &100u32);

    assert_eq!(result.rate_used, rate);
    assert!(result.net_amount > 0);
}

#[test]
fn test_fresh_oracle_at_boundary_uses_oracle_rate() {
    // Age exactly MAX_ORACLE_AGE -> still fresh -> oracle price used.
    let (env, oracle_id, _admin, payer, payee, fee_collector) = setup_env();
    let settlement_id = setup_settlement(&env);
    let settlement_client = SettlementContractClient::new(&env, &settlement_id);
    let oracle_client = MockOracleClient::new(&env, &oracle_id);

    let rate = 100_000_0000i128;
    let volume = 1_000_0000i128;
    let token_id = setup_token(&env, &payer, settlement_amount(volume, rate));

    // Price stamped at t=0; advance the ledger to exactly MAX_ORACLE_AGE.
    oracle_client.set_updated(&0u64);
    env.ledger().set_timestamp(MAX_ORACLE_AGE);

    let args = make_args(token_id, volume, payee.clone(), None);
    let result =
        settlement_client.finalize_settlement(&oracle_id, &payer, &fee_collector, &args, &100u32);

    assert_eq!(result.rate_used, rate, "boundary age must be treated as fresh");
}

#[test]
fn test_stale_oracle_falls_back_to_conservative_rate() {
    // Feed older than MAX_ORACLE_AGE -> stale price rejected, FALLBACK_RATE used.
    let (env, oracle_id, _admin, payer, payee, fee_collector) = setup_env();
    let settlement_id = setup_settlement(&env);
    let settlement_client = SettlementContractClient::new(&env, &settlement_id);
    let oracle_client = MockOracleClient::new(&env, &oracle_id);

    let volume = 1_000_0000i128;
    // Fund generously: settlement uses the (smaller) fallback rate.
    let token_id = setup_token(&env, &payer, 1_000_000_000i128);

    // Price stamped at t=0; push the clock one second past the staleness window.
    oracle_client.set_updated(&0u64);
    env.ledger().set_timestamp(MAX_ORACLE_AGE + 1);

    let args = make_args(token_id, volume, payee.clone(), None);
    let result =
        settlement_client.finalize_settlement(&oracle_id, &payer, &fee_collector, &args, &100u32);

    assert_eq!(
        result.rate_used, FALLBACK_RATE,
        "stale oracle must fall back to the conservative rate"
    );
    // Settlement amount reflects the fallback rate, not the stale oracle price.
    let expected_settlement = volume * FALLBACK_RATE / DECIMAL_DENOMINATOR;
    let expected_fee = crate::fees::compute_fee(expected_settlement, 100);
    assert_eq!(result.fee_amount, expected_fee);
    assert_eq!(result.net_amount, expected_settlement - expected_fee);
}

#[test]
fn test_get_fresh_rate_rejects_stale_feed() {
    // The strict, Result-returning variant (issue #7 step 7): Ok when fresh,
    // Err(OracleStale) when stale — no fallback.
    let (env, oracle_id, _admin, _payer, _payee, _fee_collector) = setup_env();
    let settlement_id = setup_settlement(&env);
    let oracle_client = MockOracleClient::new(&env, &oracle_id);
    oracle_client.set_updated(&0u64);

    // Fresh.
    env.ledger().set_timestamp(MAX_ORACLE_AGE);
    let fresh = env.as_contract(&settlement_id, || {
        crate::rate_application::get_fresh_rate(&env, &oracle_id)
    });
    assert_eq!(fresh, Ok(100_000_0000i128));

    // Stale.
    env.ledger().set_timestamp(MAX_ORACLE_AGE + 1);
    let stale = env.as_contract(&settlement_id, || {
        crate::rate_application::get_fresh_rate(&env, &oracle_id)
    });
    assert_eq!(stale, Err(crate::SettlementError::OracleStale));
}
