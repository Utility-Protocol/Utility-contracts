use soroban_sdk::{panic_with_error, Env};

use crate::constants::{BPS_DENOMINATOR, MAX_SLIPPAGE_BPS};
use crate::rate_application::apply_rate_to_volume;
use crate::SettlementError;

/// Convert resource token volume to settlement currency using the supplied
/// (already staleness-resolved) exchange rate.
///
/// The `rate` is resolved by the caller via
/// [`crate::rate_application::resolve_rate`], so a stale oracle has already been
/// replaced by the conservative fallback before reaching this function.
///
/// Flow:
/// 1. Computes the settlement amount = volume * rate / 1e7
/// 2. Checks actual amount against slippage tolerance and user's minimum
///
/// # Returns
/// The settlement amount computed from the resolved rate
///
/// # Panics
/// * `SlippageExceeded` if slippage exceeds MAX_SLIPPAGE_BPS or actual < min_expected_amount
pub fn convert_to_settlement_currency(
    env: &Env,
    rate: i128,
    volume: i128,
    min_expected_amount: Option<i128>,
) -> i128 {
    let expected_amount = apply_rate_to_volume(env, volume, rate);

    let actual_amount = expected_amount;

    let slippage_bps = if expected_amount > 0 {
        let diff = expected_amount.saturating_sub(actual_amount);
        (diff.checked_mul(BPS_DENOMINATOR as i128)
            .expect("slippage overflow"))
        .checked_div(expected_amount)
        .expect("slippage underflow") as u32
    } else {
        0
    };

    if slippage_bps > MAX_SLIPPAGE_BPS {
        env.events().publish(
            (soroban_sdk::symbol_short!("SlpSlipp"),),
            (expected_amount, actual_amount, slippage_bps),
        );
        panic_with_error!(env, SettlementError::SlippageExceeded);
    }

    if let Some(min_expected) = min_expected_amount {
        if actual_amount < min_expected {
            env.events().publish(
                (soroban_sdk::symbol_short!("SlpSlipp"),),
                (expected_amount, actual_amount, slippage_bps),
            );
            panic_with_error!(env, SettlementError::SlippageExceeded);
        }
    }

    actual_amount
}
