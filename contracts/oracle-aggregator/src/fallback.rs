//! Fallback oracle logic (issue #130, step 4).
//!
//! When aggregation cannot form a consensus (too few fresh reports, or every
//! provider failing), the aggregator must not silently return a stale or zero
//! price. It degrades in a controlled, observable way:
//!
//! 1. **Last-good value** — the most recent successfully aggregated value is
//!    replayed, preserving continuity through a brief provider outage.
//! 2. **Conservative constant** — if no last-good value has ever been recorded
//!    (cold start), a hard-coded conservative [`FALLBACK_VALUE`] is used.
//!
//! Callers always learn that fallback engaged via the `used_fallback` flag on
//! [`crate::types::AggregationResult`] and the `Fbk` event emitted by
//! [`crate::OracleAggregator`].

use crate::constants::FALLBACK_VALUE;
use crate::types::LastGood;

/// The resolved fallback `(value, decimals, updated_at)`.
///
/// `decimals` is always the caller's target precision (the last-good value is
/// already stored normalized, and the constant is defined in the default target
/// precision).
pub fn resolve_fallback(last_good: Option<&LastGood>, target_decimals: u32) -> (i128, u32, u64) {
    match last_good {
        Some(last) => (last.value, target_decimals, last.updated_at),
        None => (FALLBACK_VALUE, target_decimals, 0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::LastGood;

    #[test]
    fn fallback_prefers_last_good() {
        let last = LastGood {
            value: 123_456,
            decimals: 7,
            updated_at: 42,
        };
        assert_eq!(resolve_fallback(Some(&last), 7), (123_456, 7, 42));
    }

    #[test]
    fn fallback_uses_constant_on_cold_start() {
        assert_eq!(resolve_fallback(None, 7), (FALLBACK_VALUE, 7, 0));
    }
}
