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

    // Round-half-up: fee = 0 when rate_bps < 5000
    let per_txn_fee = compute_fee(micro_amount, rate_bps);
    assert_eq!(
        per_txn_fee, 0,
        "Micro-settlement fee should be 0 for rate_bps < 5000"
    );

    // With rate_bps >= 5000, round-half-up rounds up to 1
    let micro_fee_higher_rate = compute_fee(micro_amount, 5001);
    assert_eq!(
        micro_fee_higher_rate, 1,
        "Round-half-up should round 5001/10000 = 1 for micro amount"
    );

    // Under truncation: 1M micro-txns at rate_bps=100 yields 0 total fee
    // Under round-half-up: still 0 (correct — each fee < 0.5 rounds to 0)
    let num_txns: i128 = 1_000_000;
    let mut total_truncation_style: i128 = 0;
    for _ in 0..num_txns {
        total_truncation_style += compute_fee(micro_amount, rate_bps);
    }
    assert_eq!(total_truncation_style, 0);

    // At rate 5001, each micro-txn yields fee=1 (rounds up at >= 0.5001)
    let mut total_at_5001: i128 = 0;
    for _ in 0..num_txns {
        total_at_5001 += compute_fee(micro_amount, 5001);
    }
    assert_eq!(total_at_5001, num_txns);

    // Each micro-tx at rate=5001 has ~0.4999 excess rounding (0.5001→1.0).
    // Cumulative error across N txns is bounded by N * 5000 in scaled units.
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

    // Verify each individual fee satisfies the round-half-up invariant
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
    // Exhaustive check over small domain to verify round-half-up invariants
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
