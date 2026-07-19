# Automated Performance Regression Detection

This project enforces a CI performance gate for critical paths so latency regressions are caught before merge.

## Architecture

1. Critical-path latency snapshots are expressed as JSON under `critical_paths` with a `p99_ms` metric.
2. `.github/performance/baseline.json` stores the reviewed baseline for system-wide critical paths.
3. CI produces a current snapshot, runs `scripts/performance_regression_gate.py`, and uploads a Markdown report artifact.
4. The gate fails if any critical path exceeds the hard SLO of **100ms P99** or regresses by more than the configured budget, currently **10%**.

## CI Flow

The `test-and-lint` job runs the performance gate after unit tests and before fuzz-test detection. This keeps the feedback loop early while still requiring code to compile and tests to pass first.

```bash
python3 scripts/performance_regression_gate.py \
  --baseline .github/performance/baseline.json \
  --current target/performance/current.json \
  --max-p99-ms 100 \
  --regression-percent 10 \
  --report target/performance/performance-report.md
```

## Monitoring, Alerting, and Dashboards

Production monitoring should export the same JSON shape used by CI. Dashboards should track these panels for every critical path:

- P50, P95, and P99 latency.
- Error rate and availability against the **99.99% uptime** target.
- Current P99 compared with the committed CI baseline.
- Canary vs. stable P99 deltas during deployments.

Alerts should page on any critical path above 100ms P99 for two consecutive evaluation windows and open a ticket for regressions above 10% that remain below the hard SLO.

## Deployment Strategy

Use blue-green deployments with canary analysis:

1. Deploy the candidate release to the idle environment.
2. Route 5% of traffic to the candidate for at least two evaluation windows.
3. Compare candidate P99, error rate, and availability with the stable environment.
4. Increase traffic to 25%, 50%, and 100% only if canary metrics remain within budget.
5. Roll back immediately if P99 exceeds 100ms, availability drops below 99.99%, or security checks fail.

## Runbook

When the gate fails:

1. Download the `performance-regression-report` artifact from the CI run.
2. Identify the failing critical path and compare current P99 with baseline P99.
3. Re-run the relevant benchmark locally with production-like inputs.
4. Optimize or revert the suspected change.
5. Update `.github/performance/baseline.json` only after an intentional performance change has been reviewed for security, availability, and operational impact.
