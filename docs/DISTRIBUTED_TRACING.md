# Distributed Tracing and Trace Context Propagation

This design adds OpenTelemetry-compatible trace context propagation across the utility protocol's off-chain services while keeping Soroban contract calls deterministic and free of tracing side effects.

## Architecture

```text
ESP32 meter / simulator
  └─ W3C traceparent + baggage in MQTT payload
      └─ MQTT broker
          └─ ingestion / contract submitter
              └─ Stellar RPC / Soroban contract invocation
                  └─ dashboard and alert telemetry
```

- The meter simulator creates a W3C `traceparent` for every usage and heartbeat publish.
- If a caller supplies a parent `traceparent`, the simulator preserves the trace id and creates a child span id.
- MQTT JSON payloads include `traceparent`, `tracestate`, `baggage`, `span_id`, `parent_span_id`, and `trace_started_at`.
- Downstream services must forward the same `traceparent` into HTTP headers or message metadata before submitting contract transactions.
- Smart contracts do not store trace ids. Contract logs should use existing domain identifiers (`meter_id`, ledger sequence, transaction hash) and be correlated off-chain.

## Performance and Availability Targets

- Critical publish and ingestion paths have a P99 latency budget of **100 ms**.
- The trace context helper generates ids locally with `crypto.randomBytes`, so propagation does not depend on an external collector.
- Telemetry exporters must fail open: dropped spans must not block MQTT publish, contract submission, or emergency operations.
- Sampling defaults to `1.0` in local simulation and should be lowered in high-throughput production environments only after canary validation.

## Security Review Checklist

- Do not put private keys, signatures beyond existing payload requirements, wallet secrets, or personally identifiable information in `baggage`.
- Treat `traceparent` from devices as untrusted input; reject malformed values and all-zero ids.
- Keep trace ids out of on-chain persistent storage to avoid durable user correlation.
- Validate dashboards and runbooks before enabling alerts that page operators.

## Monitoring, Alerts, and Dashboards

Recommended metrics and alerts:

| Signal | Alert threshold | Purpose |
| --- | --- | --- |
| `meter_publish_latency_ms` P99 | `> 100 ms` for 5 minutes | Protect critical path latency target |
| `trace_context_invalid_total` | any sustained increase | Detect malformed or spoofed propagation headers |
| `otel_export_failures_total` | `> 1%` of exports | Detect collector or network instability |
| `mqtt_publish_errors_total` | `> 0` for critical meters | Maintain 99.99% availability |

Dashboards should show trace id search, P50/P95/P99 latency, MQTT publish errors, contract submission status, and collector export health.

## Blue-Green and Canary Deployment

1. Deploy the tracing-enabled simulator and ingestion service to the green environment with exporters in fail-open mode.
2. Route 5% of meters to green and verify malformed trace rate, MQTT publish latency, and contract submission success.
3. Increase to 25%, 50%, and 100% only if P99 remains below 100 ms and error budgets are not consumed.
4. Roll back by routing meters to blue; trace context is additive JSON metadata, so older consumers can ignore it.

## Runbook

1. Search the dashboard by `traceparent` trace id from a failed payload.
2. Confirm MQTT publish span latency and ingestion span latency are under budget.
3. If collector exports are failing but publishes are healthy, keep service online and repair the collector path.
4. If MQTT publish latency exceeds 100 ms P99, reduce tracing sample rate and inspect broker saturation.
5. If malformed trace context spikes, quarantine the producing meter fleet segment and rotate device credentials if spoofing is suspected.
