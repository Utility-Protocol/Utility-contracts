//! Overflow-safe, precision-preserving time-weighted average rate.
//!
//! Issue #2 — time-weighted tariff averaging computes
//!
//! ```text
//!     weighted_avg = Σ(rate_i × duration_i) / Σ(duration_i)
//! ```
//!
//! over peak / off-peak / shoulder windows. Two failure modes plague a naive
//! `u128` implementation:
//!
//! 1. **Overflow** — `rate_i × duration_i` (and the running sum over up to
//!    `MAX_TARIFF_INTERVALS` intervals) can exceed `u128::MAX`. The original
//!    tariff code used `saturating_mul`, which silently clamps and yields a
//!    wrong (capped) result instead of failing loudly.
//! 2. **Precision loss** — integer division `sum / total` truncates. Re-ordering
//!    to `rate_i / total × duration_i` (as some "fixes" do) is *worse*: it
//!    discards the fractional part of every term before summing.
//!
//! This module accumulates the numerator in **full 256-bit precision** (a pair
//! of `u128` limbs) and performs an exact 256-by-128 division with
//! **round-half-up** rounding. The result is exact for the whole representable
//! input domain, with no intermediate overflow short of a numerator that
//! genuinely exceeds 2²⁵⁶ (reported as `None` rather than silently wrong).
//!
//! All arithmetic is pure `u128`/`u64`; no `Decimal128` type or big-int
//! dependency is required, and the crate stays `#![no_std]`.

/// Maximum number of tariff intervals in a 30-day window at 15-minute
/// granularity. Exposed for callers that want to pre-validate schedule length.
pub const MAX_TARIFF_INTERVALS: usize = 2880;

const LOW64: u128 = u64::MAX as u128;

/// Full 128×128 → 256-bit unsigned multiply. Returns `(hi, lo)`.
fn mul_full(a: u128, b: u128) -> (u128, u128) {
    let a0 = a & LOW64;
    let a1 = a >> 64;
    let b0 = b & LOW64;
    let b1 = b >> 64;

    let p00 = a0 * b0; // < 2^128
    let p01 = a0 * b1;
    let p10 = a1 * b0;
    let p11 = a1 * b1;

    // Combine the partial products. `mid` collects the bits that land in the
    // 64..128 column plus the carry out of the low column.
    let mid = (p00 >> 64) + (p01 & LOW64) + (p10 & LOW64);
    let lo = (p00 & LOW64) | ((mid & LOW64) << 64);
    let hi = p11 + (p01 >> 64) + (p10 >> 64) + (mid >> 64);
    (hi, lo)
}

/// Add a 256-bit value to a 256-bit accumulator, returning the new accumulator
/// and `true` on 256-bit overflow.
fn add_256(acc: (u128, u128), add: (u128, u128)) -> ((u128, u128), bool) {
    let (lo, c0) = acc.1.overflowing_add(add.1);
    let (hi1, c1) = acc.0.overflowing_add(add.0);
    let (hi, c2) = hi1.overflowing_add(c0 as u128);
    ((hi, lo), c1 || c2)
}

/// Divide a 256-bit value `(hi, lo)` by `d`, returning `(quotient, remainder)`.
/// Returns `None` if `d == 0` or the quotient would not fit in `u128`
/// (i.e. `hi >= d`). Restoring binary long division; the remainder is exact.
fn div_256_by_128(hi: u128, lo: u128, d: u128) -> Option<(u128, u128)> {
    if d == 0 || hi >= d {
        return None;
    }

    // `rem` is always kept < d (and therefore the conceptual remainder fits).
    let mut rem = hi;
    let mut quot: u128 = 0;
    let mut i = 128;
    while i > 0 {
        i -= 1;
        let bit = (lo >> i) & 1;
        let carry_out = rem >> 127; // bit shifted out of the top of `rem`
        let shifted = (rem << 1) | bit;
        // Conceptual current remainder == carry_out * 2^128 + shifted.
        if carry_out == 1 || shifted >= d {
            // Always >= d when carry_out == 1 (since d < 2^128).
            rem = shifted.wrapping_sub(d);
            quot = (quot << 1) | 1;
        } else {
            rem = shifted;
            quot <<= 1;
        }
    }
    Some((quot, rem))
}

/// `floor(a * b / d)` computed without intermediate overflow.
/// `None` if `d == 0` or the exact quotient exceeds `u128::MAX`.
pub fn mul_div_floor(a: u128, b: u128, d: u128) -> Option<u128> {
    let (hi, lo) = mul_full(a, b);
    div_256_by_128(hi, lo, d).map(|(q, _)| q)
}

/// `round_half_up(a * b / d)` computed without intermediate overflow.
/// `None` if `d == 0` or the rounded quotient exceeds `u128::MAX`.
pub fn mul_div_round(a: u128, b: u128, d: u128) -> Option<u128> {
    let (hi, lo) = mul_full(a, b);
    let (q, r) = div_256_by_128(hi, lo, d)?;
    round_half_up(q, r, d)
}

/// Apply round-half-up given quotient `q`, remainder `r` and divisor `d`.
fn round_half_up(q: u128, r: u128, d: u128) -> Option<u128> {
    let round_up = match r.checked_mul(2) {
        Some(two_r) => two_r >= d,
        // r*2 overflowed u128 => r > 2^127 >= d/2 => definitely round up.
        None => true,
    };
    if round_up {
        q.checked_add(1)
    } else {
        Some(q)
    }
}

/// Whether `rate * duration` fits in `u128` — useful to pre-validate (reject) a
/// tariff interval at schedule-creation time if a caller wants the stricter
/// "no per-term overflow" guarantee. The averaging functions themselves do not
/// require this (they accumulate in 256 bits).
pub fn interval_product_fits_u128(rate: u128, duration: u64) -> bool {
    rate.checked_mul(duration as u128).is_some()
}

/// Time-weighted average rate over `intervals`, each `(rate, duration_seconds)`.
///
/// Computes `round_half_up(Σ(rate_i × duration_i) / Σ(duration_i))` with exact
/// 256-bit intermediate precision.
///
/// Returns `None` when:
/// * `intervals` is empty or the total duration is zero, or
/// * the numerator sum exceeds 2²⁵⁶ (astronomically large input), or
/// * the (mathematically valid) average would exceed `u128::MAX`.
///
/// For any well-formed tariff schedule the weighted average lies within
/// `[min_rate, max_rate]`, so the `u128`-fit conditions never trip in practice.
pub fn weighted_average(intervals: &[(u128, u64)]) -> Option<u128> {
    let mut acc: (u128, u128) = (0, 0);
    let mut total_duration: u128 = 0;

    for &(rate, duration) in intervals {
        let d = duration as u128;
        let product = mul_full(rate, d);
        let (next, overflow) = add_256(acc, product);
        if overflow {
            return None;
        }
        acc = next;
        total_duration = total_duration.checked_add(d)?;
    }

    if total_duration == 0 {
        return None;
    }

    let (q, r) = div_256_by_128(acc.0, acc.1, total_duration)?;
    round_half_up(q, r, total_duration)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reference numerator/denominator computed in native u128 — only valid when
    /// the numerator is known to fit u128 (small/realistic domain).
    fn naive_round_half_up(intervals: &[(u128, u64)]) -> u128 {
        let mut sum: u128 = 0;
        let mut total: u128 = 0;
        for &(rate, dur) in intervals {
            sum += rate * dur as u128;
            total += dur as u128;
        }
        let q = sum / total;
        let r = sum % total;
        if r * 2 >= total {
            q + 1
        } else {
            q
        }
    }

    #[test]
    fn mul_full_low_bits_match_wrapping_mul() {
        // The low 128 bits of the full product must equal wrapping_mul, always.
        let samples: [u128; 8] = [
            0,
            1,
            2,
            u64::MAX as u128,
            (u64::MAX as u128) + 1,
            123_456_789_012_345_678,
            u128::MAX,
            u128::MAX / 3,
        ];
        for &a in &samples {
            for &b in &samples {
                let (_, lo) = mul_full(a, b);
                assert_eq!(lo, a.wrapping_mul(b), "lo mismatch for {a} * {b}");
            }
        }
    }

    #[test]
    fn mul_full_known_high_products() {
        // 2^64 * 2^64 = 2^128 -> hi = 1, lo = 0.
        assert_eq!(mul_full(1u128 << 64, 1u128 << 64), (1, 0));
        // u128::MAX * 2 = 2^129 - 2 -> hi = 1, lo = u128::MAX - 1.
        assert_eq!(mul_full(u128::MAX, 2), (1, u128::MAX - 1));
        // u128::MAX * u128::MAX = 2^256 - 2^129 + 1.
        let (hi, lo) = mul_full(u128::MAX, u128::MAX);
        assert_eq!(hi, u128::MAX - 1);
        assert_eq!(lo, 1);
    }

    #[test]
    fn mul_div_matches_when_product_fits_u128() {
        // When a*b fits u128, mul_div_floor == a*b/d.
        let a = 500_000_000_000_000_000_000u128; // 500 tokens @ 18 decimals
        let b = 2_592_000u128; // 30 days in seconds
        let d = 2_592_000u128;
        assert_eq!(mul_div_floor(a, b, d), Some(a)); // a*b/b == a
        assert_eq!(mul_div_round(a, b, d), Some(a));
    }

    #[test]
    fn mul_div_round_rounds_half_up() {
        assert_eq!(mul_div_floor(10, 1, 4), Some(2)); // 2.5 -> floor 2
        assert_eq!(mul_div_round(10, 1, 4), Some(3)); // 2.5 -> half-up 3
        assert_eq!(mul_div_round(9, 1, 4), Some(2)); // 2.25 -> 2
        assert_eq!(mul_div_round(11, 1, 4), Some(3)); // 2.75 -> 3
    }

    #[test]
    fn mul_div_zero_divisor_is_none() {
        assert_eq!(mul_div_floor(1, 1, 0), None);
        assert_eq!(mul_div_round(1, 1, 0), None);
    }

    #[test]
    fn weighted_average_constant_rate_is_that_rate() {
        let rate = 500_000_000_000_000_000_000u128;
        let intervals = [(rate, 900u64); 2880]; // 2880 * 15min = 30 days
        assert_eq!(weighted_average(&intervals), Some(rate));
    }

    #[test]
    fn weighted_average_two_windows() {
        // peak 100 for 3600s, off-peak 50 for 3600s -> avg 75.
        let intervals = [(100u128, 3600u64), (50u128, 3600u64)];
        assert_eq!(weighted_average(&intervals), Some(75));
    }

    #[test]
    fn weighted_average_weights_by_duration() {
        // 100 for 3h, 40 for 1h -> (300+40)/4 = 85.
        let intervals = [(100u128, 10_800u64), (40u128, 3_600u64)];
        assert_eq!(weighted_average(&intervals), Some(85));
    }

    #[test]
    fn weighted_average_empty_or_zero_duration_is_none() {
        assert_eq!(weighted_average(&[]), None);
        assert_eq!(weighted_average(&[(100u128, 0u64)]), None);
    }

    #[test]
    fn weighted_average_no_overflow_at_max_decimals_scale() {
        // 18-decimal rates with full 30-day windows: the product per term is
        // ~1.3e27 and the sum ~3.7e30 — both well inside u128 here, but the
        // exact-256-bit path must still match the naive reference.
        let intervals = [
            (500_000_000_000_000_000_000u128, 2_592_000u64),
            (300_000_000_000_000_000_000u128, 2_592_000u64),
            (123_000_000_000_000_000_000u128, 1_000u64),
        ];
        assert_eq!(weighted_average(&intervals), Some(naive_round_half_up(&intervals)));
    }

    #[test]
    fn weighted_average_property_small_domain() {
        // Deterministic pseudo-random sweep; numerator stays within u128 so the
        // naive reference is valid. Verifies exactness (relative error == 0,
        // far tighter than the 1e-15 target) across the parameter ranges.
        let mut seed: u64 = 0x9E3779B97F4A7C15;
        let mut next = || {
            // xorshift64*
            seed ^= seed >> 12;
            seed ^= seed << 25;
            seed ^= seed >> 27;
            seed.wrapping_mul(0x2545F4914F6CDD1D)
        };

        for _ in 0..2000 {
            let count = (next() % 2880) as usize + 1; // [1, 2880]
            let mut intervals = [(0u128, 0u64); 2880];
            for slot in intervals.iter_mut().take(count) {
                let rate = (next() % 1_000_000_000_000_000_000u64) as u128 + 1; // [1, 1e18]
                let dur = (next() % (2_592_000 - 60 + 1)) as u64 + 60; // [60, 2_592_000]
                *slot = (rate, dur);
            }
            let slice = &intervals[..count];
            assert_eq!(
                weighted_average(slice),
                Some(naive_round_half_up(slice)),
                "mismatch for {count} intervals"
            );
        }
    }

    #[test]
    fn weighted_average_handles_beyond_u128_numerator() {
        // Two maximal terms: u128::MAX * (2^63) each. The numerator far exceeds
        // u128 but fits 256 bits; the average is exact and fits u128.
        let big_dur = 1u64 << 62;
        let intervals = [(u128::MAX, big_dur), (u128::MAX, big_dur)];
        // avg = (MAX*D + MAX*D) / (2D) = MAX.
        assert_eq!(weighted_average(&intervals), Some(u128::MAX));
    }

    #[test]
    fn mul_div_quotient_exceeding_u128_is_none() {
        // u128::MAX * u128::MAX / 1 ~= 2^256 — far beyond u128 — must report
        // None rather than a silently truncated value.
        assert_eq!(mul_div_floor(u128::MAX, u128::MAX, 1), None);
        assert_eq!(mul_div_round(u128::MAX, u128::MAX, 1), None);
    }

    #[test]
    fn interval_product_fits_helper() {
        assert!(interval_product_fits_u128(500_000_000_000_000_000_000u128, 2_592_000));
        assert!(!interval_product_fits_u128(u128::MAX, 2));
    }
}
