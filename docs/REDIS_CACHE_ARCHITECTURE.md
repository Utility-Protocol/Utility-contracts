# Redis-backed Cache Layer

## Architecture

The meter simulator now uses a two-tier cache for read-heavy contract paths:

1. **In-process memory cache**: first lookup path for critical reads. Warm reads do
   not require network I/O and are intended to stay below the 100ms P99 target.
2. **Optional Redis cache**: shared cache across simulator processes. Enable with
   `REDIS_ENABLED=true` and configure `REDIS_URL`.
3. **Contract/RPC loader**: authoritative fallback when the cache misses or an
   entry has expired.

Cached read paths currently include meter status (`getMeter`) and usage rollups
(`getUsageData`). Usage submissions and ZK usage submissions invalidate both
meter and usage cache keys for the affected meter so subsequent reads observe a
fresh contract snapshot.

## Configuration

| Environment variable | Default | Purpose |
| --- | --- | --- |
| `CACHE_ENABLED` | `true` | Enables memory and Redis caching. Set to `false` for bypass mode. |
| `CACHE_TTL_SECONDS` | `60` | Default TTL used when a path-specific TTL is not provided. |
| `CACHE_METER_TTL_SECONDS` | `30` | TTL for meter status entries. |
| `CACHE_USAGE_TTL_SECONDS` | `15` | TTL for usage rollup entries. |
| `CACHE_KEY_PREFIX` | `utility:meter-simulator` | Redis key namespace. |
| `REDIS_ENABLED` | `false` | Enables Redis as the shared backing tier. |
| `REDIS_URL` | `redis://localhost:6379` | Redis connection URL. |
| `REDIS_CONNECT_TIMEOUT_MS` | `500` | Redis connection timeout budget. |

## Monitoring and Alerting

The cache exposes runtime counters through `getCacheMetrics()` and the CLI status
command prints the latest snapshot. Export these labels from the service wrapper
or process supervisor:

- `cache_hits`
- `cache_misses`
- `cache_sets`
- `cache_deletes`
- `cache_errors`
- `memory_entries`
- `redis_ready`

Recommended alerts:

- Page when `cache_errors` increases for 5 consecutive minutes while
  `REDIS_ENABLED=true`.
- Warn when hit ratio falls below 70% for 15 minutes on production read traffic.
- Warn when P99 status read latency exceeds 100ms for 5 minutes.

## Deployment Runbook

1. Deploy with `CACHE_ENABLED=true` and `REDIS_ENABLED=false` to validate local
   memory caching without adding a network dependency.
2. Blue/green deploy Redis-enabled instances with `REDIS_ENABLED=true` and the
   production `REDIS_URL`.
3. Send 5% canary read traffic to the green deployment and compare P99 latency,
   cache error rate, and contract/RPC request volume.
4. Increase canary traffic to 25%, 50%, and 100% only if error rate is flat and
   P99 remains below 100ms.
5. Roll back by setting `REDIS_ENABLED=false`; if needed set `CACHE_ENABLED=false`
   to bypass all cache layers.

## Security Notes

- Redis keys include only cache namespaces and meter IDs; do not place private
  keys or signatures in custom cache keys.
- Use Redis TLS/authentication in production via the `REDIS_URL` scheme and
  credentials supported by your Redis provider.
- Keep TTLs short for contract-derived balances and usage data to limit stale
  reads after operational incidents.
