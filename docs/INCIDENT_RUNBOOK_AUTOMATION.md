# Incident Response Runbook Automation

This design adds a lightweight incident automation layer for Utility Protocol operators. It evaluates health snapshots locally, maps failures to runbook incident rules, and sends actionable PagerDuty Events API v2 triggers only after a rule matches.

## Architecture

1. Monitoring jobs collect contract and service signals such as pause state, TTL, oracle freshness, latency, and error rate.
2. `IncidentRunbookAutomation.evaluate()` performs deterministic in-process rule evaluation for the critical path.
3. `IncidentRunbookAutomation.process()` applies deduplication suppression and calls `PagerDutyClient.trigger()` for unsuppressed incidents.
4. PagerDuty routes alerts to the configured escalation policy, where responders follow `EMERGENCY_RUNBOOK.md` containment and recovery procedures.

The critical-path evaluator is synchronous and does not perform network I/O. PagerDuty calls happen after incident detection, preserving the sub-100ms P99 target for health evaluation loops.

## PagerDuty Configuration

Set these environment variables in the monitoring runtime:

- `PAGERDUTY_ROUTING_KEY`: Events API v2 integration routing key for the Utility Protocol service.
- `PAGERDUTY_EVENTS_API_URL`: Optional override for tests or private event gateways. Defaults to `https://events.pagerduty.com/v2/enqueue`.

## Built-in Incident Rules

| Rule | Severity | Condition | Runbook action |
| --- | --- | --- | --- |
| `contract-paused` | critical | Contract pause flag is true | Begin emergency governance coordination. |
| `ttl-low` | error | Remaining TTL is below 1,000 ledgers | Run TTL extension procedure. |
| `stale-oracle` | error | Oracle age exceeds 300 seconds | Fail over oracle operations and investigate feed health. |
| `latency-slo-breach` | warning | Critical path P99 latency is at least 100ms | Inspect recent deployments and canary metrics. |
| `error-rate-high` | error | Service error rate is at least 1% | Triage service logs and consider rollback. |

## Deployment Strategy

Deploy automation in blue-green mode beside the existing monitoring worker. During canary, set a non-production PagerDuty routing key and compare generated incidents against dashboard signals. Promote to production routing only after false positives and duplicate suppression are validated.

## Security Notes

Store the PagerDuty routing key in the deployment secret manager, never in source control. Treat generated PagerDuty incidents as operational metadata; do not place private keys, signatures, or customer PII in `customDetails` snapshots.
