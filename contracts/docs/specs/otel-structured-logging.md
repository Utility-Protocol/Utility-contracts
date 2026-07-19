# OpenTelemetry Structured Logging Architecture

Issue #73 adds a system-wide structured logging contract event format aligned with OpenTelemetry semantic conventions while remaining deterministic for Soroban execution.

## Architecture

The utility contract emits an `otel.log` event alongside existing domain events for critical billing paths:

- `meter.registered`
- `meter.top_up`
- `usage.updated`
- `provider.claim`

Each event carries an `OtelLogRecord` payload. Rust field names use snake_case equivalents of OpenTelemetry attributes because Soroban contract types cannot expose dotted field identifiers:

| Contract field | OpenTelemetry semantic attribute |
| --- | --- |
| `service_name` | `service.name` |
| `service_version` | `service.version` |
| `deployment_environment_name` | `deployment.environment.name` |
| `event_name` | `event.name` |
| `event_domain` | `event.domain` |
| `log_severity` | `log.severity` |
| `enduser_id` | `enduser.id` |
| `server_address` | `server.address` |
| `url_scheme` | `url.scheme` |

## Performance and availability

The logging path performs a single deterministic event publication and avoids storage writes, token transfers, dynamic allocation loops, and cross-contract calls. Downstream collectors should alert if observed P99 latency for instrumented critical paths exceeds the included `critical_path_budget_ms` value of 100 ms.

## Monitoring and alerting

Indexers should route `otel.log` events into the telemetry pipeline and build dashboards for:

- P50/P95/P99 transaction latency by `event.name`.
- Error and panic rates by contract function.
- Event volume by meter and provider.
- Missing-log detection for successful critical-path transactions.

Recommended alerts:

- Critical-path P99 latency greater than 100 ms for five minutes.
- Any drop in `otel.log` ingestion rate while domain events continue.
- Provider claim failures or repeated usage update rejections.

## Deployment and runbook

Use a blue-green contract deployment with canary traffic routed through a small provider/meter cohort first. During canary analysis, compare domain-event counts with `otel.log` counts for the same ledgers. Promote only when counts match and P99 latency remains under 100 ms.

Rollback by routing clients back to the previous contract deployment if telemetry counts diverge or latency budget is exceeded.
