//! Tunable bounds for the multi-provider oracle aggregator.
//!
//! These constants encode the invariants from issue #130: a secure oracle
//! integration framework must bound provider count, decimal scaling, feed
//! staleness, deviation from consensus, and the minimum number of confirming
//! providers — so a single manipulated or downed feed can never skew (or stall)
//! the aggregated value.

/// Namespace prefix: `"OAGG"`. Prevents storage-key collisions if this contract
/// is ever co-deployed or migrated alongside the other workspace contracts.
pub const NAMESPACE_PREFIX: [u8; 4] = [0x4f, 0x41, 0x47, 0x47];

/// Maximum number of registered oracle providers.
///
/// Keeps `report()` cross-contract calls and the aggregation buffer bounded so
/// a single call stays comfortably inside the Soroban instruction budget.
pub const MAX_PROVIDERS: usize = 16;

/// Maximum number of decimals a feed may report.
///
/// Bounds the `10^decimals` scale factors used when normalizing heterogeneous
/// feeds to a common precision (`10^38` fits in `i128`).
pub const MAX_DECIMALS: u32 = 38;

/// Default target decimals for aggregated values.
///
/// 7 decimal places matches the settlement contract's fixed-point
/// [`DECIMAL_DENOMINATOR`](contracts/settlement/src/constants.rs) so the two
/// interoperate without further conversion.
pub const DEFAULT_TARGET_DECIMALS: u32 = 7;

/// Default maximum age (epoch-seconds) of a feed before it is considered stale.
///
/// Falls within the settlement contract's `[300, 3600]` staleness window.
pub const DEFAULT_MAX_AGE_SECS: u64 = 600;

/// Default maximum deviation (basis points) of a provider from the consensus
/// median before its report is discarded as an outlier. `500 bps = 5%`.
pub const DEFAULT_MAX_DEVIATION_BPS: u32 = 500;

/// Default minimum number of fresh, valid reports required to form a consensus.
///
/// Set conservatively to `1` so a lone primary provider still works out of the
/// box; operators should raise it to `3` for production-grade redundancy.
pub const DEFAULT_MIN_CONFIRMATIONS: u32 = 1;

/// Conservative fallback value used when no feed is fresh and no last-good
/// value has been recorded. `50_000_000` in 7-decimal fixed point (= 5.0),
/// matching the settlement contract's [`FALLBACK_RATE`].
pub const FALLBACK_VALUE: i128 = 50_000_000;

/// TTL bump (in ledgers) applied to long-lived bookkeeping entries so health
/// and provider state is not archived out from under an active aggregator.
pub const BOOKKEEPING_TTL_LEDGERS: u32 = 30 * 17_280; // ~30 days at ~5s ledgers

/// Basis-point denominator (`100% = 10_000 bps`).
pub const BPS_DENOMINATOR: u32 = 10_000;
