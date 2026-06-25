#![cfg(test)]

use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{Address, Env};

use crate::constants::{MAX_RAW_RETENTION_SECS, PRUNE_BATCH_SIZE, ROLLUP_INTERVAL_SECS};
use crate::{MeterAggregator, MeterAggregatorClient};

fn setup(env: &Env) -> (MeterAggregatorClient<'_>, Address, Address) {
    env.mock_all_auths();
    let contract_id = env.register(MeterAggregator, ());
    let client = MeterAggregatorClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let device = Address::generate(env);
    client.initialize(&admin);
    (client, device, admin)
}

#[test]
fn test_initialize_and_admin() {
    let env = Env::default();
    let (client, _device, admin) = setup(&env);
    assert_eq!(client.get_admin(), Some(admin));
}

#[test]
#[should_panic]
fn test_double_initialize_panics() {
    let env = Env::default();
    let (client, _device, _admin) = setup(&env);
    let other = Address::generate(&env);
    client.initialize(&other);
}

// --- Hour rollup correctness --------------------------------------------

#[test]
fn test_hour_rollup_correctness() {
    let env = Env::default();
    let (client, device, _admin) = setup(&env);
    let source = Address::generate(&env);

    let hour0 = 100u64;
    let base = hour0 * ROLLUP_INTERVAL_SECS;

    // Three readings inside hour0.
    for (i, v) in [10i128, 20, 30].iter().enumerate() {
        env.ledger().set_timestamp(base + (i as u64) * 10);
        client.submit_reading(&device, &source, v);
    }
    // One reading inside hour1.
    env.ledger().set_timestamp((hour0 + 1) * ROLLUP_INTERVAL_SECS + 5);
    client.submit_reading(&device, &source, &5);

    let b0 = client.get_hourly_bucket(&device, &hour0).unwrap();
    assert_eq!(b0.total, 60);
    assert_eq!(b0.count, 3);

    let b1 = client.get_hourly_bucket(&device, &(hour0 + 1)).unwrap();
    assert_eq!(b1.total, 5);
    assert_eq!(b1.count, 1);

    // Aggregate over both hours.
    let from_ts = base;
    let to_ts = (hour0 + 1) * ROLLUP_INTERVAL_SECS + ROLLUP_INTERVAL_SECS - 1;
    assert_eq!(client.get_aggregated_volume(&device, &from_ts, &to_ts), 65);
}

#[test]
fn test_daily_bucket_fast_path() {
    let env = Env::default();
    let (client, device, _admin) = setup(&env);
    let source = Address::generate(&env);

    // Day 10 in epoch days.
    let day = 10u64;
    let day_start = day * 86_400;

    // Two readings on day 10, in different hours.
    env.ledger().set_timestamp(day_start + 100);
    client.submit_reading(&device, &source, &7);
    env.ledger().set_timestamp(day_start + 3_700);
    client.submit_reading(&device, &source, &3);

    let d = client.get_daily_bucket(&device, &day).unwrap();
    assert_eq!(d.total, 10);
    assert_eq!(d.count, 2);

    // Querying the full day exercises the daily fast path.
    let vol = client.get_aggregated_volume(&device, &day_start, &(day_start + 86_399));
    assert_eq!(vol, 10);
}

#[test]
fn test_rollup_day_reclaims_hourly_but_keeps_total() {
    let env = Env::default();
    let (client, device, _admin) = setup(&env);
    let source = Address::generate(&env);

    let day = 20u64;
    let day_start = day * 86_400;

    env.ledger().set_timestamp(day_start + 10);
    client.submit_reading(&device, &source, &40);
    env.ledger().set_timestamp(day_start + 7_300);
    client.submit_reading(&device, &source, &60);

    let hour_a = (day_start + 10) / ROLLUP_INTERVAL_SECS;
    assert!(client.get_hourly_bucket(&device, &hour_a).is_some());

    // Consolidate the day: hourly buckets are reclaimed, daily total persists.
    let total = client.rollup_day(&device, &day);
    assert_eq!(total, 100);
    assert!(client.get_hourly_bucket(&device, &hour_a).is_none());
    assert!(client.get_daily_bucket(&device, &day).is_some());

    // Full-day aggregation still correct via the daily bucket.
    let vol = client.get_aggregated_volume(&device, &day_start, &(day_start + 86_399));
    assert_eq!(vol, 100);
}

// --- Overflow aggregation -----------------------------------------------

#[test]
fn test_overflow_aggregation_is_rejected() {
    let env = Env::default();
    let (client, device, _admin) = setup(&env);
    let source = Address::generate(&env);

    let base = 500u64 * ROLLUP_INTERVAL_SECS;

    // First reading saturates the hourly bucket near i128::MAX.
    env.ledger().set_timestamp(base);
    client.submit_reading(&device, &source, &i128::MAX);

    // Second reading in the same hour overflows the checked_add and must trap.
    env.ledger().set_timestamp(base + 10);
    let res = client.try_submit_reading(&device, &source, &1);
    assert!(res.is_err(), "overflowing aggregation should be rejected");

    // State unchanged: the saturating reading is still the only one folded in.
    let hour = base / ROLLUP_INTERVAL_SECS;
    let b = client.get_hourly_bucket(&device, &hour).unwrap();
    assert_eq!(b.total, i128::MAX);
    assert_eq!(b.count, 1);
}

#[test]
fn test_negative_value_rejected() {
    let env = Env::default();
    let (client, device, _admin) = setup(&env);
    let source = Address::generate(&env);
    env.ledger().set_timestamp(1_000);
    let res = client.try_submit_reading(&device, &source, &-1);
    assert!(res.is_err());
}

// --- Pruning boundary ----------------------------------------------------

#[test]
fn test_prune_retention_boundary() {
    let env = Env::default();
    let (client, device, _admin) = setup(&env);
    let source = Address::generate(&env);

    let t: u64 = 2_000_000;
    env.ledger().set_timestamp(t);
    client.submit_reading(&device, &source, &42);
    assert_eq!(client.get_live_reading_count(&device), 1);

    // Exactly at the retention boundary: age == retention -> kept.
    env.ledger().set_timestamp(t + MAX_RAW_RETENTION_SECS);
    let pruned = client.prune(&device);
    assert_eq!(pruned, 0);
    assert_eq!(client.get_live_reading_count(&device), 1);

    // One second past the boundary: age == retention + 1 -> pruned.
    env.ledger().set_timestamp(t + MAX_RAW_RETENTION_SECS + 1);
    let pruned = client.prune(&device);
    assert_eq!(pruned, 1);
    assert_eq!(client.get_live_reading_count(&device), 0);

    // Aggregated history survives pruning of the raw reading.
    let hour = t / ROLLUP_INTERVAL_SECS;
    assert_eq!(client.get_hourly_bucket(&device, &hour).unwrap().total, 42);
}

#[test]
fn test_prune_batch_size_limit() {
    let env = Env::default();
    let (client, device, _admin) = setup(&env);
    let source = Address::generate(&env);

    let t: u64 = 3_000_000;
    // Submit (PRUNE_BATCH_SIZE * 2) readings, all in distinct seconds.
    let n = (PRUNE_BATCH_SIZE * 2) as u64;
    for i in 0..n {
        env.ledger().set_timestamp(t + i);
        client.submit_reading(&device, &source, &1);
    }
    assert_eq!(client.get_live_reading_count(&device), n);

    // Age everything past retention, then prune once: at most batch-size deleted.
    env.ledger().set_timestamp(t + MAX_RAW_RETENTION_SECS + n + 1);
    let pruned = client.prune(&device);
    assert_eq!(pruned, PRUNE_BATCH_SIZE);
    assert_eq!(
        client.get_live_reading_count(&device),
        n - PRUNE_BATCH_SIZE as u64
    );

    // A second prune drains the remainder.
    let pruned = client.prune(&device);
    assert_eq!(pruned, PRUNE_BATCH_SIZE);
    assert_eq!(client.get_live_reading_count(&device), 0);
}

// --- Storage exhaustion prevention --------------------------------------

#[test]
fn test_storage_exhaustion_prevention() {
    let env = Env::default();
    let (client, device, _admin) = setup(&env);
    let source = Address::generate(&env);

    let t0: u64 = 10_000_000;

    // Backlog: 100 readings inside one retention window.
    let backlog = 100u64;
    for i in 0..backlog {
        env.ledger().set_timestamp(t0 + i);
        client.submit_reading(&device, &source, &1);
    }
    assert_eq!(client.get_live_reading_count(&device), backlog);

    // Jump well past retention so the whole backlog is stale, then keep
    // submitting. Inline pruning (batch 10/call) drains the backlog: after
    // `backlog / batch` submissions the live set collapses to just the fresh
    // readings, proving storage stays bounded instead of growing without limit.
    let jump = t0 + MAX_RAW_RETENTION_SECS + 1_000_000;
    let calls = backlog / PRUNE_BATCH_SIZE as u64; // 10 submissions
    for i in 0..calls {
        env.ledger().set_timestamp(jump + i);
        client.submit_reading(&device, &source, &1);
    }

    // 100 stale pruned (10 calls * 10), 10 fresh added -> 10 live.
    let live = client.get_live_reading_count(&device);
    assert_eq!(live, calls);
    assert!(live < backlog, "live storage must shrink, not grow unbounded");
}

#[test]
fn test_invalid_time_range_rejected() {
    let env = Env::default();
    let (client, device, _admin) = setup(&env);
    let res = client.try_get_aggregated_volume(&device, &100, &50);
    assert!(res.is_err());
}
