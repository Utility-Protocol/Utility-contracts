# Tariff Time-Weighted Average — Precision & Overflow Safety

Issue #2 — "Temporal Tariff State Calculator Integer Precision Loss in
Time-Weighted Rate Averaging"

## Goal

Compute the time-weighted average rate over tariff windows

```
weighted_avg = Σ(rate_i × duration_i) / Σ(duration_i)
```

with **no intermediate overflow** and **no precision loss**, for the full input
domain (`rate` up to 18-decimal token units, `duration` up to a 30-day window,
up to `MAX_TARIFF_INTERVALS = 2880` intervals).

Invariant restored:

```
∀ schedules:  Σ(rate_i × duration_i) / Σ(duration_i)  ∈  [min_rate, max_rate]
```

## The two defects

1. **Overflow.** The tariff calculator multiplied with `saturating_mul`. On
   overflow that *silently clamps* to `u128::MAX` and produces a wrong (capped)
   average instead of failing — exactly the kind of silent corruption that
   yields the reported double-digit billing error.
2. **Precision loss.** `Σ / total` via integer division truncates. The naive
   "fix" of reordering to `rate_i / total × duration_i` is **worse**: it throws
   away the fractional part of *every* term before summing.

## The implementation

`contracts/common/src/weighted_rate.rs` (pure `#![no_std]`, no dependencies):

- `mul_full(a, b) -> (hi, lo)` — exact 128×128 → 256-bit multiply (two `u128`
  limbs, schoolbook on 64-bit halves).
- `add_256` — 256-bit accumulation with overflow detection.
- `div_256_by_128(hi, lo, d)` — exact restoring long division, returns
  `(quotient, remainder)`; `None` if the quotient would exceed `u128`.
- `round_half_up(q, r, d)` — `ROUND_HALF_UP` using the remainder (overflow-safe
  comparison `2r ≥ d`).
- `mul_div_floor` / `mul_div_round` — `a*b/d` with no intermediate overflow.
- `weighted_average(&[(rate, duration)])` — accumulates the numerator in full
  256-bit precision, divides exactly, rounds half-up.
- `interval_product_fits_u128(rate, duration)` — optional per-interval
  pre-validation for callers that want to reject extreme schedules at creation
  time (blueprint step 3).

### Why full-width instead of `Decimal128`

A `(mantissa, scale)` decimal type with a 38-digit intermediate scale (as the
blueprint sketches) still rounds at each `mul`/`div`. Accumulating the numerator
in **exact 256-bit integers** and dividing once is simpler *and* exact — the
relative error is **0** across the domain, far tighter than the `1e-15` target.
No big-int crate is needed and the crate stays `no_std`.

### Overflow guarantees

- Per-term `rate × duration`: exact (256-bit), never overflows.
- Numerator sum over N intervals: exact unless it exceeds 2²⁵⁶ — unreachable for
  any real schedule (`2880 × max_term ≪ 2²⁵⁶`); reported as `None` if it ever
  occurs, never silently wrong.
- Result: exact `u128` (the weighted average is bounded by `max_rate`, so it
  always fits); `None` only in degenerate/empty/zero-duration inputs.

## Tests

`contracts/common/src/weighted_rate.rs` `#[cfg(test)]`:

- `mul_full` low-bits-match-`wrapping_mul` and known high products.
- `mul_div_round` half-up behaviour and zero-divisor / quotient-overflow `None`.
- weighted-average: constant rate, two/weighted windows, empty/zero-duration,
  18-decimal 30-day scale, beyond-`u128` numerator (exact via 256-bit).
- `weighted_average_property_small_domain` — 2000-iteration deterministic sweep
  over `rate ∈ [1, 10¹⁸]`, `duration ∈ [60, 2_592_000]`,
  `interval_count ∈ [1, 2880]`, asserting **exact** equality with a native
  reference (relative error 0).

## Wiring into the tariff oracle

The production consumer is `TariffOracle::calculate_flow_for_period`
(`contracts/utility_contracts/src/tariff_oracle.rs`), which currently uses
`saturating_mul` + truncating division. That crate **does not compile today**
(129+ pre-existing Soroban-SDK-23 errors — see
`contracts/utility_contracts/COMPILATION_STATUS.md`), so this PR lands the
verified algorithm in the `common` crate. Once `utility_contracts` builds again,
`calculate_flow_for_period` should delegate to
`utility_contracts_common::weighted_rate::weighted_average` instead of the
saturating/truncating arithmetic.
