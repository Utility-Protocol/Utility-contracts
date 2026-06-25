# meter-aggregator

Per-device meter reading aggregation with **bounded storage**.

## Why

Appending every raw meter reading to an unbounded per-device vector exhausts
Soroban contract storage. At one reading every ~5 seconds (~17,280/day) a naive
design overruns the contract storage budget within hours, after which all further
readings *and* settlements for that device fail — a cheap denial of service.

This contract keeps live storage bounded regardless of device lifetime or
submission frequency.

## How

- **Raw readings** are stored under individual keys `RawReading(device, seq)`
  with a monotonically increasing sequence number (O(1) append; seq order == time
  order).
- On every submission the value is folded into the matching **hourly** and
  **daily** rollup buckets using overflow-checked `i128` addition.
- Raw readings older than `MAX_RAW_RETENTION_SECS` (7 days) are pruned **inline**,
  amortized to O(1) per submission via a watermark cursor `PruneCursor(device)`,
  deleting at most `PRUNE_BATCH_SIZE` (10) entries per call so a backlog drains
  over several submissions instead of blowing one call's instruction budget.
- Long-term volume lives in compact rollup buckets, read by
  `get_aggregated_volume` which prefers **daily → hourly → raw** in that order.
- `rollup_day` consolidates a completed day by reclaiming its now-redundant
  hourly buckets (the daily total is maintained incrementally), keeping
  hourly-bucket growth bounded too.

## Public API

| fn | description |
|----|-------------|
| `initialize(admin)` | one-time admin setup |
| `submit_reading(device, source, value) -> seq` | store + rollup + inline prune |
| `prune(device) -> u32` | manual batch prune (callable by anyone) |
| `rollup_day(device, day_epoch) -> i128` | admin; reclaim a day's hourly buckets |
| `get_aggregated_volume(device, from_ts, to_ts) -> i128` | tiered windowed total |
| `get_hourly_bucket` / `get_daily_bucket` / `get_raw_reading` | views |
| `get_prune_cursor` / `get_reading_count` / `get_live_reading_count` | views |

## Constants

| name | value | meaning |
|------|-------|---------|
| `MAX_RAW_RETENTION_SECS` | `604_800` | 7-day raw retention window |
| `PRUNE_BATCH_SIZE` | `10` | max deletions per call |
| `ROLLUP_INTERVAL_SECS` | `3_600` | hourly bucket width |
| `SECONDS_PER_DAY` | `86_400` | daily bucket width |
| `FIXED_POINT_SCALE` | `10_000_000` | 7-decimal fixed point |

## Limitation

Sub-day query resolution is retained only while hourly buckets exist. After a day
is consolidated via `rollup_day` (and its raw readings pruned), that day is
queryable at day granularity only. Full-day and multi-day windows remain exact.

## Test

```sh
cargo test --package meter-aggregator
```

Covers: hourly/daily rollup correctness, daily fast-path reads, `rollup_day`
reclamation, overflow rejection, negative-value rejection, the pruning retention
boundary, batch-size limiting, and end-to-end storage-exhaustion prevention.
