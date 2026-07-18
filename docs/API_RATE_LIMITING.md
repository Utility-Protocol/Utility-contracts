# API Rate Limiting with Per-Tenant Token Buckets

## Architecture

The simulator enforces a local per-tenant token bucket before critical API paths call the contract simulator or MQTT broker. Tenants are keyed as `meter:<meter_id>` for usage submissions and as narrower operational tenants for heartbeats and status updates. Each tenant receives an independent bucket, so a burst from one meter cannot starve another meter.

The implementation is dependency-free and synchronous in the hot path. A limiter check performs a map lookup, timestamp arithmetic, and a token decrement, keeping the expected P99 overhead well below the 100ms target for local simulator paths.

## Defaults and Configuration

Defaults are defined in `meter-simulator/src/config.js` and can be tuned with environment variables:

| Environment variable | Default | Meaning |
| --- | ---: | --- |
| `RATE_LIMIT_CAPACITY` | `60` | Maximum burst tokens per tenant. |
| `RATE_LIMIT_REFILL_PER_SECOND` | `1` | Tokens restored per second per tenant. |
| `RATE_LIMIT_IDLE_TTL_MS` | `600000` | Idle bucket eviction window. |

## Enforcement Points

- Direct usage submissions call `assertAllowed("meter:<meter_id>")` before validation and contract simulation.
- ZK usage submissions use the same tenant key so privacy and non-privacy submissions share quota.
- MQTT usage publishing uses the same quota, while heartbeat and status topics use weighted operational buckets.

## Monitoring and Alerting

`PerTenantRateLimiter.snapshotMetrics()` exposes counters for allowed requests, blocked requests, bucket creation, idle eviction, active buckets, capacity, and refill rate. Production adapters should export these as Prometheus counters/gauges:

- `rate_limiter_allowed_total`
- `rate_limiter_blocked_total`
- `rate_limiter_active_buckets`
- `rate_limiter_evicted_buckets_total`

Recommended alerts:

1. **Sustained throttling:** blocked / (allowed + blocked) > 5% for 10 minutes.
2. **Tenant abuse:** any single tenant blocked more than 100 times in 5 minutes.
3. **Cardinality spike:** active buckets > expected meter count by 20%.
4. **Limiter disabled/misconfigured:** capacity or refill rate resolves to zero or missing.

## Deployment Plan

Use blue-green deployment with a canary phase:

1. Deploy the limiter disabled in shadow mode if the adapter supports it, exporting decisions only.
2. Enable the limiter for 5% of tenants and compare request success rate, P99 latency, and blocked-rate dashboards.
3. Increase canary to 25%, then 50%, then 100% after at least one refill window with no alert regressions.
4. Keep the previous deployment warm for rollback. Roll back if P99 latency exceeds 100ms or legitimate traffic is blocked.

## Runbook

1. Check `rate_limiter_blocked_total` and identify the top tenants by blocked decisions.
2. Confirm tenant traffic shape against expected meter reporting intervals.
3. If legitimate traffic is throttled, temporarily raise `RATE_LIMIT_CAPACITY` for bursts or `RATE_LIMIT_REFILL_PER_SECOND` for sustained reporting.
4. If traffic is abusive, keep quotas in place and inspect tenant credentials and MQTT topics.
5. After mitigation, verify P99 latency, blocked ratio, and active bucket cardinality return to baseline.
