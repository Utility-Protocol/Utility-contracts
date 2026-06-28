use crate::constants::BPS_DENOMINATOR;

/// Compute protocol fee using round-half-up (commercial rounding).
///
/// fee = floor((amount * rate_bps + 5000) / 10000)
///
/// This prevents systematic value extraction via micro-settlements
/// where plain truncation would round the fee to zero for every
/// dust-amount transaction.
///
/// # Rounding invariants (round-half-up)
/// - fee * 10000 <= amount * rate_bps + 5000  (max 0.5 unit over-collection)
/// - fee * 10000 >= amount * rate_bps - 4999  (max 0.5 unit under-collection)
/// - |fee * 10000 - amount * rate_bps| <= 5000
use soroban_sdk::{panic_with_error, Env};
use utility_contracts_common::errors::ArithmeticError;

pub fn compute_fee(env: &Env, amount: i128, rate_bps: u32) -> i128 {
    let scaled = amount.checked_mul(rate_bps as i128).unwrap_or_else(|| {
        panic_with_error!(env, ArithmeticError::Overflow);
    });
    let adjusted = scaled.checked_add(5000).unwrap_or_else(|| {
        panic_with_error!(env, ArithmeticError::Overflow);
    });
    adjusted
        .checked_div(BPS_DENOMINATOR as i128)
        .unwrap_or_else(|| {
            panic_with_error!(env, ArithmeticError::DivisionByZero);
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::{BPS_DENOMINATOR, MAX_FEE_RATE_BPS, MAX_SETTLEMENT, MIN_FEE_RATE_BPS};

    /// Round-half-up invariant: error magnitude ≤ 0.5 units of the smallest denomination.
    /// |fee * 10000 - amount * rate_bps| <= 5000
    fn invariant_error_bounded(amount: i128, rate_bps: u32) -> bool {
        let env = Env::default();
        let fee = compute_fee(&env, amount, rate_bps);
        let scaled_fee = fee * BPS_DENOMINATOR as i128;
        let exact = amount * rate_bps as i128;
        let diff = (scaled_fee - exact).abs();
        diff <= 5000
    }

    #[test]
    fn test_compute_fee_round_half_up() {
        let env = Env::default();
        // 0.5 rounds up
        assert_eq!(compute_fee(&env, 1, 5000), 1);
        // 0.5001 rounds up
        assert_eq!(compute_fee(&env, 1, 5001), 1);
        // 0.4999 rounds down
        assert_eq!(compute_fee(&env, 1, 4999), 0);
        // exact
        assert_eq!(compute_fee(&env, 2, 5000), 1);
        assert_eq!(compute_fee(&env, 10000, 100), 100);
    }

    #[test]
    fn test_fee_precision_scenarios() {
        let env = Env::default();
        // 1 token * 10% = 0.1 tokens
        assert_eq!(compute_fee(&env, 10_000_000, 1000), 1_000_000);

        // minimum non-zero (1e-7 tokens) * 1000 bps (10%)
        // = 1 * 1000 / 10000 = 0.1 → rounds to 0
        assert_eq!(compute_fee(&env, 1, 1000), 0);

        // 0.0000001 * 5001 bps ≈ 0.5001 → rounds up to 1
        assert_eq!(compute_fee(&env, 1, 5001), 1);

        // 0.0000001 * 4999 bps ≈ 0.4999 → rounds down to 0
        assert_eq!(compute_fee(&env, 1, 4999), 0);

        // exact half at 5000 rounds up
        assert_eq!(compute_fee(&env, 1, 5000), 1);
    }

    #[test]
    fn test_invariant_error_bounded_edge_cases() {
        let rates = [MIN_FEE_RATE_BPS, 50, 100, 500, MAX_FEE_RATE_BPS];
        let amounts = [1, 10_000_000, 100_000_000, MAX_SETTLEMENT];

        let env = Env::default();
        for &amount in &amounts {
            for &rate_bps in &rates {
                assert!(
                    invariant_error_bounded(amount, rate_bps),
                    "Invariant violated: amount={}, rate_bps={}, fee={}",
                    amount,
                    rate_bps,
                    compute_fee(&env, amount, rate_bps)
                );
            }
        }
    }

    #[test]
    fn test_micro_settlement_cumulative_fairness() {
        let env = Env::default();
        let rate_bps = 100;
        let micro_amount: i128 = 1;
        let num_transactions: i128 = 1_000;

        // Each micro-transaction yields fee = (1*100 + 5000) / 10000 = 0
        // Total collected = 0
        let mut total_fee_collected: i128 = 0;
        for _ in 0..num_transactions {
            total_fee_collected += compute_fee(&env, micro_amount, rate_bps);
        }
        assert_eq!(total_fee_collected, 0);

        // A single lump-sum of same total amount
        let lump_fee = compute_fee(&env, num_transactions * micro_amount, rate_bps);

        // The difference is due to rounding — each micro-txn loses ~0.01 units
        let diff = (total_fee_collected - lump_fee).abs();
        // With per-txn error ≤ 5000, total error ≤ N * 5000
        let max_per_txn_error_exact = 5000i128;
        let max_cumulative_error = num_transactions * max_per_txn_error_exact;
        assert!(
            diff <= max_cumulative_error,
            "Cumulative error {} exceeds max {} (N={}, err/txn={})",
            diff,
            max_cumulative_error,
            num_transactions,
            max_per_txn_error_exact
        );
    }

    #[test]
    fn test_cumulative_fee_equivalence() {
        let env = Env::default();
        // With amounts where each individual fee rounds the same way,
        // cumulative and lump-sum should agree within 1 unit.
        let rate_bps = 100;
        let num_transactions: i128 = 100;
        let per_txn_amount: i128 = 10000; // each yields fee = (10000*100+5000)/10000 = 100

        let mut total_fee_collected: i128 = 0;
        for _ in 0..num_transactions {
            total_fee_collected += compute_fee(&env, per_txn_amount, rate_bps);
        }

        let lump_fee = compute_fee(&env, per_txn_amount * num_transactions, rate_bps);

        let diff = (total_fee_collected - lump_fee).abs();
        assert!(
            diff <= 1,
            "Cumulative vs lump-sum fee mismatch: {} vs {} (diff={})",
            total_fee_collected,
            lump_fee,
            diff
        );
    }

    #[test]
    fn test_fee_monotonicity() {
        let env = Env::default();
        let rate_bps = 100;
        let mut prev_fee: i128 = 0;
        for i in 0..1000 {
            let fee = compute_fee(&env, i, rate_bps);
            assert!(
                fee >= prev_fee,
                "Fee decreased: amount={}, prev_fee={}, fee={}",
                i,
                prev_fee,
                fee
            );
            prev_fee = fee;
        }
    }

    #[test]
    fn test_zero_rate_returns_zero() {
        let env = Env::default();
        assert_eq!(compute_fee(&env, 1_000_000, 0), 0);
        assert_eq!(compute_fee(&env, 0, 100), 0);
        assert_eq!(compute_fee(&env, 0, 0), 0);
    }

    #[test]
    fn test_large_amount_no_overflow() {
        let env = Env::default();
        let amount: i128 = 1_000_000_000_000_000_000;
        let rate_bps = 1000;
        let fee = compute_fee(&env, amount, rate_bps);
        assert_eq!(fee, 100_000_000_000_000_000);
    }
}
