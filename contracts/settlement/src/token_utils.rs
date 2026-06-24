use soroban_sdk::{Address, Env};

use crate::constants::BPS_DENOMINATOR;
use crate::fees::compute_fee;

/// Transfer the protocol fee from payer to the fee collector.
///
/// # Arguments
/// * `env` - Contract environment
/// * `token` - Address of the token contract
/// * `payer` - Address paying the fee
/// * `fee_collector` - Address receiving the fee
/// * `amount` - Settlement amount (gross, before fee deduction)
/// * `rate_bps` - Fee rate in basis points
///
/// # Returns
/// The fee amount that was transferred
pub fn collect_fee(
    env: &Env,
    token: &Address,
    payer: &Address,
    fee_collector: &Address,
    amount: i128,
    rate_bps: u32,
) -> i128 {
    if rate_bps == 0 {
        return 0;
    }

    let fee = compute_fee(amount, rate_bps);

    if fee == 0 {
        return 0;
    }

    // Transfer fee from payer to fee collector
    let token_client = soroban_sdk::token::Client::new(env, token);
    token_client.transfer(payer, fee_collector, &fee);

    fee
}

/// Verify fee satisfies the round-half-up rounding invariants.
/// |fee * 10000 - amount * rate_bps| <= 5000  (max 0.5 unit error)
#[allow(dead_code)]
pub fn verify_fee_invariant(amount: i128, rate_bps: u32, fee: i128) -> bool {
    let scaled_fee = fee * BPS_DENOMINATOR as i128;
    let exact = amount * rate_bps as i128;
    (scaled_fee - exact).abs() <= 5000
}
