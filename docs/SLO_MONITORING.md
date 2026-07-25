# Service Level Objective Monitoring and Burn-Rate Alerts

## Objectives

Utility Protocol services share two production SLOs:

- **Availability:** 99.99% successful critical-path requests over a rolling 30-day window.
- **Latency:** P99 critical-path latency below 100 ms.

The implementation uses pre-aggregated request counters so the SLO path stays below the 100 ms P99 performance target. Services should emit counters for total requests, failed requests, latency violations, and P99 latency per rolling window.

## Architecture

1. Every service exports rolling-window measurements for the fast and slow burn-rate windows.
2. `meter-simulator/src/slo-monitor.js` evaluates the measurements against the shared SLO configuration.
3. Alert payloads include stable dashboard labels: `slo`, `window`, and `severity`.
4. Dashboards group the same labels by service, environment, and deployment color.
5. Runbooks route `page` alerts to the on-call engineer and `ticket` alerts to the service team backlog.

## Burn-rate policy

| Window | Duration | Burn-rate threshold | Severity |
| --- | ---: | ---: | --- |
| Fast | 5 minutes | 14.4x | Page |
| Slow | 60 minutes | 6x | Ticket |

A burn rate compares the observed bad-event ratio with the 0.01% error budget implied by 99.99% availability. Bad events include failed requests and requests whose latency exceeds the critical-path latency objective.

## Blue-green and canary rollout

- Deploy the green environment with SLO monitoring in shadow mode first.
- Add a custom `canary` burn-rate window for the canary slice.
- Promote traffic only when the canary window is healthy for the full analysis period.
- Roll back immediately on a page-level burn-rate alert or sustained P99 latency above 100 ms.

## Security review checklist

- Do not emit user consumption volumes, wallet secrets, raw device signatures, or personally identifying metadata in alert labels.
- Alert labels must remain bounded-cardinality to prevent dashboard and log-index exhaustion.
- Validate all externally supplied SLO configuration before deploying it.
