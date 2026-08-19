//! Data aggregation and validation (issue #130, step 3).
//!
//! Aggregation combines multiple provider reports into a single robust value.
//! The strategy is the classic oracle **median**, which is resistant to
//! outliers: a single manipulated or broken feed cannot move the aggregate
//! unless it becomes the majority. Before the median is taken:
//!
//! 1. every report is **normalized** to a common decimal precision, and
//! 2. a first-pass median is computed, then any report that deviates from it by
//!    more than `max_deviation_bps` is discarded, and the median is recomputed
//!    over the surviving reports.
//!
//! All arithmetic is overflow-safe (saturating scaling, 256-bit-free deviation
//! math) and stays `#![no_std]` — only fixed-size stack buffers are used.

use crate::constants::{BPS_DENOMINATOR, MAX_PROVIDERS};
use crate::types::OracleReport;

/// Median of `values`, sorted in place.
///
/// Returns `None` for an empty slice. For an even-length slice the two middle
/// values are averaged with floor rounding (`lo + (hi - lo) / 2`, which cannot
/// overflow `i128`).
pub fn median(values: &mut [i128]) -> Option<i128> {
    if values.is_empty() {
        return None;
    }
    values.sort_unstable();
    let n = values.len();
    if n % 2 == 1 {
        Some(values[n / 2])
    } else {
        let lo = values[n / 2 - 1];
        let hi = values[n / 2];
        Some(lo + (hi - lo) / 2)
    }
}

/// Rescale `value` from `from` decimals to `to` decimals.
///
/// Upscaling is saturating so a pathological scale factor can never wrap to a
/// wrong sign; downscaling truncates toward zero (rounding error < 1 base unit).
pub fn scale_to_decimals(value: i128, from: u32, to: u32) -> i128 {
    if from == to {
        return value;
    }
    if from < to {
        value.saturating_mul(10i128.pow(to - from))
    } else {
        value / 10i128.pow(from - to)
    }
}

/// Whether a feed stamped at `updated_at` is stale relative to `now`.
///
/// Staleness is `age > max_age_secs`; a feed exactly `max_age_secs` old is still
/// fresh. Saturating subtraction treats a clock-skewed future timestamp as age 0
/// rather than underflowing.
pub fn is_stale(now: u64, updated_at: u64, max_age_secs: u64) -> bool {
    now.saturating_sub(updated_at) > max_age_secs
}

/// Absolute deviation of `value` from `reference`, in basis points.
///
/// Returns `None` when the reference is zero (division is undefined) or when the
/// deviation cannot be represented in `u32`. Uses unsigned magnitude math so no
/// intermediate value can overflow.
pub fn deviation_bps(value: i128, reference: i128) -> Option<u32> {
    if reference == 0 {
        return if value == 0 { Some(0) } else { None };
    }
    let diff = value.abs_diff(reference);
    let base = reference.unsigned_abs();
    let bps = diff.saturating_mul(BPS_DENOMINATOR as u128) / base;
    u32::try_from(bps).ok()
}

/// Whether `value` is within `max_bps` basis points of `reference`.
pub fn within_deviation(value: i128, reference: i128, max_bps: u32) -> bool {
    match deviation_bps(value, reference) {
        Some(bps) => bps <= max_bps,
        None => false,
    }
}

/// Aggregate normalized reports into a single value.
///
/// Returns `Some((value, providers_used))`, where `providers_used` is the number
/// of reports that survived outlier rejection, or `None` when there are no
/// reports to aggregate. The caller decides whether `None` triggers fallback.
///
/// The buffer is fixed-size ([`MAX_PROVIDERS`]) so the call is bounded in both
/// time and memory.
pub fn aggregate(
    reports: &soroban_sdk::Vec<OracleReport>,
    target_decimals: u32,
    max_deviation_bps: u32,
) -> Option<(i128, u32)> {
    let mut normalized = [0i128; MAX_PROVIDERS];
    let mut count = 0usize;

    for report in reports.iter() {
        if count >= MAX_PROVIDERS {
            break;
        }
        normalized[count] = scale_to_decimals(report.value, report.decimals, target_decimals);
        count += 1;
    }

    if count == 0 {
        return None;
    }

    let slice = &mut normalized[..count];
    let first_pass = median(slice)?;

    // Reject outliers beyond the deviation bound, then recompute the median.
    let mut filtered = [0i128; MAX_PROVIDERS];
    let mut kept = 0usize;
    for &value in slice.iter() {
        if within_deviation(value, first_pass, max_deviation_bps) {
            filtered[kept] = value;
            kept += 1;
        }
    }

    let final_value = if kept > 0 {
        median(&mut filtered[..kept])?
    } else {
        first_pass
    };

    Some((final_value, kept.max(1) as u32))
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::{Address, Env, Symbol, Vec};

    #[test]
    fn median_odd_even_and_single() {
        assert_eq!(median(&mut [5]), Some(5));
        assert_eq!(median(&mut [3, 1, 2]), Some(2));
        // even: lower middle 2, upper middle 4 -> 3.
        assert_eq!(median(&mut [1, 4, 2, 9]), Some(3));
        assert_eq!(median(&mut []), None);
    }

    #[test]
    fn median_sorts_in_place() {
        let mut values = [9, 1, 8, 2, 7];
        assert_eq!(median(&mut values), Some(7));
        assert_eq!(values, [1, 2, 7, 8, 9]);
    }

    #[test]
    fn scale_up_down_and_equal() {
        assert_eq!(scale_to_decimals(150, 2, 7), 150_00000);
        assert_eq!(scale_to_decimals(150_00000, 7, 2), 150);
        assert_eq!(scale_to_decimals(42, 7, 7), 42);
    }

    #[test]
    fn scale_down_truncates() {
        // 150_000_009 @ 7 decimals = 15.0000009 -> 2 decimals = 1500 (15.00).
        assert_eq!(scale_to_decimals(150_000_009, 7, 2), 1500);
    }

    #[test]
    fn staleness_boundary() {
        let updated = 1_000u64;
        assert!(!is_stale(updated + 600, updated, 600));
        assert!(is_stale(updated + 601, updated, 600));
        // clock skew -> fresh.
        assert!(!is_stale(updated, updated + 50, 600));
    }

    #[test]
    fn deviation_bps_matches_expected() {
        // 105 vs 100 -> 5% -> 500 bps.
        assert_eq!(deviation_bps(105, 100), Some(500));
        assert_eq!(deviation_bps(100, 105), Some(476)); // 5/105 -> 476 bps
        assert_eq!(deviation_bps(100, 100), Some(0));
        // reference zero is undefined.
        assert_eq!(deviation_bps(1, 0), None);
        assert_eq!(deviation_bps(0, 0), Some(0));
    }

    #[test]
    fn within_deviation_checks_bound() {
        assert!(within_deviation(105, 100, 500));
        assert!(!within_deviation(106, 100, 500));
    }

    fn report(
        env: &Env,
        provider: &Address,
        value: i128,
        decimals: u32,
        updated_at: u64,
    ) -> OracleReport {
        OracleReport {
            provider: provider.clone(),
            adapter: crate::types::AdapterKind::Direct,
            data_key: Symbol::new(env, "XLMUSD"),
            value,
            decimals,
            updated_at,
        }
    }

    #[test]
    fn aggregate_median_of_three() {
        let env = Env::default();
        let p1 = Address::generate(&env);
        let p2 = Address::generate(&env);
        let p3 = Address::generate(&env);
        let mut reports = Vec::new(&env);
        reports.push_back(report(&env, &p1, 100, 2, 1));
        reports.push_back(report(&env, &p2, 104, 2, 1));
        reports.push_back(report(&env, &p3, 102, 2, 1));

        // Normalized to 7 decimals: median of 100/102/104 = 102 (all within
        // the 5% deviation bound, so all three survive).
        let (value, used) = aggregate(&reports, 7, 500).unwrap();
        assert_eq!(value, 102_00000);
        assert_eq!(used, 3);
    }

    #[test]
    fn aggregate_rejects_outlier() {
        let env = Env::default();
        let p1 = Address::generate(&env);
        let p2 = Address::generate(&env);
        let p3 = Address::generate(&env);
        let mut reports = Vec::new(&env);
        reports.push_back(report(&env, &p1, 100, 2, 1));
        reports.push_back(report(&env, &p2, 102, 2, 1));
        reports.push_back(report(&env, &p3, 500, 2, 1)); // wild outlier

        // Median of [100,102,500] = 102; 500 deviates > 500 bps and is dropped,
        // leaving median of [100,102] = 101.
        let (value, used) = aggregate(&reports, 7, 500).unwrap();
        assert_eq!(value, 101_00000);
        assert_eq!(used, 2);
    }

    #[test]
    fn aggregate_empty_is_none() {
        let env = Env::default();
        let reports = Vec::new(&env);
        assert_eq!(aggregate(&reports, 7, 500), None);
    }
}
