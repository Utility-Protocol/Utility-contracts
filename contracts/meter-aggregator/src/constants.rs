//! Tunable bounds for raw-reading retention, pruning, and time-windowed rollups.
//!
//! These constants encode the invariants from the storage-exhaustion mitigation:
//! raw readings are short-lived audit records that get rolled up into compact
//! hourly/daily buckets and then pruned, keeping per-device storage bounded
//! regardless of how long a device runs or how fast it submits.

/// How long raw readings are retained before they become eligible for pruning.
/// 7 days = `7 * 86_400` seconds. After this window the data lives only in the
/// rolled-up hourly/daily buckets.
pub const MAX_RAW_RETENTION_SECS: u64 = 604_800;

/// Maximum number of stale raw readings deleted during a single submission.
///
/// Pruning is amortized across submissions: each `submit_reading` call deletes
/// at most this many entries, so a backlog of stale readings is drained over
/// several calls instead of blowing the instruction budget of one call. Each
/// deletion costs on the order of a few thousand instructions, so a small batch
/// keeps the per-call cost predictable.
pub const PRUNE_BATCH_SIZE: u32 = 10;

/// Number of seconds covered by one hourly rollup bucket (1 hour).
pub const ROLLUP_INTERVAL_SECS: u64 = 3_600;

/// Number of seconds in one day, used to derive daily bucket epochs.
pub const SECONDS_PER_DAY: u64 = 86_400;

/// Number of hourly buckets that make up one day.
pub const HOURS_PER_DAY: u64 = SECONDS_PER_DAY / ROLLUP_INTERVAL_SECS;

/// Fixed-point scale for reported volumes: 7 decimal places (1.0 == 10_000_000).
/// Values are summed as raw scaled `i128` integers; this constant documents the
/// interpretation and is exposed for clients that need to format volumes.
pub const FIXED_POINT_SCALE: i128 = 10_000_000;

/// TTL bump (in ledgers) applied to long-lived rollup/bookkeeping entries so the
/// aggregated history is not archived out from under an active device.
pub const BUCKET_TTL_LEDGERS: u32 = 30 * 17_280; // ~30 days at ~5s ledgers
