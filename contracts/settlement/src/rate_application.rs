//! Oracle rate fetching with staleness protection (issue #7).
//!
//! The settlement contract prices resource volume using an external price-feed
//! oracle. If that feed goes stale (its `last_updated` falls further behind the
//! ledger clock than [`MAX_ORACLE_AGE`]), continuing to use the old price lets an
//! attacker accumulate under-priced settlements during the stale window.
//!
//! This module reads the *authoritative* `last_updated` from the oracle's own
//! `get_price()` response (rather than caching oracle metadata locally), rejects
//! prices older than the staleness window, and substitutes a conservative
//! [`FALLBACK_RATE`] — emitting a `StaleFbk` event so the fallback is observable.
//! A strict, `Result`-returning variant ([`get_fresh_rate`]) is provided for
//! callers that prefer to abort rather than fall back.

use soroban_sdk::{symbol_short, Address, Env};

use crate::constants::{DECIMAL_DENOMINATOR, FALLBACK_RATE, MAX_ORACLE_AGE};
use crate::{PriceOracleClient, SettlementError};

/// Whether a price stamped at `last_updated` is stale relative to `now`.
///
/// Staleness is `age > MAX_ORACLE_AGE`; a price exactly `MAX_ORACLE_AGE` seconds
/// old is still considered fresh (boundary is inclusive of "fresh"). Uses
/// saturating subtraction so a `last_updated` in the (clock-skewed) future is
/// treated as age 0 / fresh rather than underflowing.
pub fn is_stale(now: u64, last_updated: u64) -> bool {
    now.saturating_sub(last_updated) > MAX_ORACLE_AGE
}

/// The conservative rate used when the oracle price is stale.
pub fn compute_fallback_rate() -> i128 {
    FALLBACK_RATE
}

use soroban_sdk::panic_with_error;
use utility_contracts_common::errors::ArithmeticError;

/// Apply a 7-decimal fixed-point `rate` to `volume`: `volume * rate / 1e7`.
/// Overflow-checked.
pub fn apply_rate_to_volume(env: &Env, volume: i128, rate: i128) -> i128 {
    let product = volume.checked_mul(rate).unwrap_or_else(|| {
        panic_with_error!(env, ArithmeticError::Overflow);
    });
    product.checked_div(DECIMAL_DENOMINATOR).unwrap_or_else(|| {
        panic_with_error!(env, ArithmeticError::DivisionByZero);
    })
}

/// Fetch the current oracle rate, **rejecting** stale data.
///
/// Returns `Err(SettlementError::OracleStale)` if the oracle's price is older
/// than [`MAX_ORACLE_AGE`]. This is the strict, halt-on-stale variant referenced
/// by issue #7 step 7 (propagate a `Result` instead of panicking).
pub fn get_fresh_rate(env: &Env, oracle: &Address) -> Result<i128, SettlementError> {
    let client = PriceOracleClient::new(env, oracle);
    let price = client.get_price();

    if is_stale(env.ledger().timestamp(), price.last_updated) {
        return Err(SettlementError::OracleStale);
    }

    Ok(price.price)
}

/// Resolve the rate to use for settlement: the fresh oracle price when
/// available, otherwise the conservative [`FALLBACK_RATE`].
///
/// On fallback a `StaleFbk` event `(now, last_updated, fallback_rate)` is emitted
/// so downstream monitors can detect that the stale-price protection engaged.
pub fn resolve_rate(env: &Env, oracle: &Address) -> i128 {
    let client = PriceOracleClient::new(env, oracle);
    let price = client.get_price();
    let now = env.ledger().timestamp();

    if is_stale(now, price.last_updated) {
        let fallback = compute_fallback_rate();
        env.events().publish(
            (symbol_short!("StaleFbk"),),
            (now, price.last_updated, fallback),
        );
        fallback
    } else {
        price.price
    }
}
