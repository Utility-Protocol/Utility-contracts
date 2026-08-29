# Performance Regression Detection — Utility Protocol

**Issue:** #126  
**Scope:** System-wide (webhook-delivery-service critical path; framework extensible to contracts)  
**Classification:** Engineering — CI/CD + On-Call  
**Last updated:** 2026-08-28

Automated performance regression detection keeps the protocol's **<100ms P99 critical-path SLO**
from silently degrading. Every pull request and push to `main` re-runs a benchmark of the
webhook ingestion critical path, compares the results against a **committed baseline**, and
fails the check when a metric regresses beyond tolerance or breaches the absolute SLO. Runtime
Prometheus rules catch post-deploy degradation (noisy neighbours, resource exhaustion) that
pre-merge gates cannot see.

---

## Table of Contents

1. [Architecture](#1-architecture)
2. [Metrics and Baseline](#2-metrics-and-baseline)
3. [How the Gate Works](#3-how-the-gate-works)
4. [Running Locally](#4-running-locally)
5. [CI Integration](#5-ci-integration)
6. [Refreshing the Baseline](#6-refreshing-the-baseline)
7. [Alerting and Dashboards](#7-alerting-and-dashboards)
8. [Blue-Green / Canary Tie-in](#8-blue-green--canary-tie-in)
9. [Investigating a Failed Gate](#9-investigating-a-failed-gate)
10. [Troubleshooting](#10-troubleshooting)

---

## 1. Architecture

```
                         ┌──────────────────────────────────────────────┐
                         │            GitHub Actions                    │
                         │  .github/workflows/performance-regression.yml│
                         │                                              │
  PR / push to main ────►│  benchmark-gate job                         │
                         │  1. npm ci + build                          │
                         │  2. node dist/perf/benchmark.js  (measure)  │
                         │  3. compare-results.js vs baseline (gate)   │
                         │  4. fail check on regression / SLO breach   │
                         └──────────────────────────────────────────────┘
                                        │ results JSON (artifact)
                                        ▼
                         ┌──────────────────────────────────────────────┐
                         │   .perf/perf-baselines.json  (committed)     │
                         │   authoritative reference for the gate       │
                         └──────────────────────────────────────────────┘

  Runtime (after deploy):
  webhook-delivery-service /metrics ──► Prometheus ──► monitoring/performance-alerts.yml
                                                   └──► Grafana dashboards (P99 latency)
```

Components:

| Component | Path | Role |
|---|---|---|
| Benchmark suite | `webhook-delivery-service/src/perf/benchmark.ts` | Measures ingestion latency percentiles + throughput over the live HTTP API |
| Regression gate | `webhook-delivery-service/src/perf/compare-results.ts` | Compares fresh metrics vs baseline, classifies pass/warn/fail |
| Wrapper | `scripts/perf-regression-gate.sh` | Build → benchmark → compare in one command; used by CI and locally |
| Baseline | `.perf/perf-baselines.json` | Committed reference values + SLOs + directions |
| CI pipeline | `.github/workflows/performance-regression.yml` | Runs the gate on PRs/pushes; manual baseline refresh |
| Runtime alerts | `monitoring/performance-alerts.yml` | Prometheus rules on live ingestion/delivery metrics |

The framework is intentionally **metric-agnostic**: the benchmark produces a flat
`{ metric: number }` map and the gate compares any map against the baseline file. Extending it
to the Soroban contracts (e.g. WASM instruction counts per operation) only requires a new
benchmark producer and baseline entries.

---

## 2. Metrics and Baseline

The benchmark (`webhook-ingestion` suite) reports:

| Metric | Meaning | Direction | SLO |
|---|---|---|---|
| `ingestion_p50_ms` | Median ingestion latency | lower | — |
| `ingestion_p95_ms` | P95 ingestion latency | lower | — |
| `ingestion_p99_ms` | P99 ingestion latency | lower | **< 100ms** |
| `ingestion_mean_ms` | Mean ingestion latency | lower | — |
| `ingestion_throughput_rps` | Requests per second | higher | — |

`min`/`max` latency are measured and reported in the results (under `meta.percentiles`) but are
**not gated**: they are dominated by scheduling noise and a single outlier would cause spurious
failures. The gate runs on `p50/p95/p99/mean/throughput` only.

Baseline format (`.perf/perf-baselines.json`):

```json
{
  "schemaVersion": 1,
  "generatedAt": "2026-08-28T23:57:00.000Z",
  "baselines": {
    "ingestion_p99_ms": { "value": 10, "slaMs": 100, "direction": "lower" },
    "ingestion_throughput_rps": { "value": 500, "direction": "higher" }
  }
}
```

- `value` — reference value the fresh run is compared against.
- `slaMs` — absolute ceiling (enforced only for `direction: "lower"`).
- `direction` — `lower` (latency) or `higher` (throughput); regression means drifting the wrong way.

---

## 3. How the Gate Works

For every metric in the union of (fresh run metrics ∪ baseline metrics), `compare-results.ts`
applies, in order:

1. **Not measured** → FAIL (`ingestion_throughput_rps` missing from the run = broken harness).
2. **SLO breach** → FAIL when `direction=lower` and `current > slaMs` (e.g. p99 > 100ms).
3. **Regression** → FAIL when drift from baseline exceeds `regressionTolerancePercent`
   (default **30%**). For `lower` metrics: `current > baseline × (1 + tolerance)`. For `higher`
   metrics (throughput): `current < baseline × (1 - tolerance)`.
4. **No baseline** → WARN (informational; add a baseline entry to enable detection).
5. Otherwise → PASS.

Exit codes: `0` pass · `1` any failure · `2` warnings with `--strict-warn` · `3` harness error.

---

## 4. Running Locally

```bash
# One-shot: build, benchmark (800 samples) and compare against the committed baseline
./scripts/perf-regression-gate.sh

# Tune the workload for faster iteration
PERF_SAMPLES=300 PERF_WARMUP=100 ./scripts/perf-regression-gate.sh

# Tighten or loosen the regression tolerance
PERF_TOLERANCE=15 ./scripts/perf-regression-gate.sh
```

To inspect a single run without the gate:

```bash
cd webhook-delivery-service
npm run build
node dist/perf/benchmark.js --samples 800 --warmup 200 --out ../.perf/results/latest.json
node dist/perf/compare-results.js --current ../.perf/results/latest.json \
  --baseline ../.perf/perf-baselines.json
```

---

## 5. CI Integration

`.github/workflows/performance-regression.yml`:

- **On PRs** touching `webhook-delivery-service/**`, `.perf/**`, or the gate itself: runs the
  benchmark and fails the check on regression/SLO breach. This is the primary developer loop —
  a perf regression can never merge unnoticed.
- **On push to `main`**: re-validates the committed baseline against the current code.
- **Artifacts**: the raw results JSON is uploaded as `perf-results` on every run.

GitHub Actions `ubuntu-latest` runners are a consistent-enough environment for relative
regression detection. Baselines are deliberately padded above measured values (see §6) so
runner-to-runner noise does not produce false positives; the 100ms SLO is the real gate.

---

## 6. Refreshing the Baseline

The baseline is **never updated by a passing PR run** — that would let a regression silently
become the new normal. Refresh it only after a *verified, intentional* improvement:

**Option A — local (recommended for review):**

```bash
./scripts/perf-regression-gate.sh --update-baseline
# review the diff in .perf/perf-baselines.json, then commit it with the perf change
```

**Option B — CI:**

1. Go to **Actions → Performance Regression Detection → Run workflow**.
2. Set **update_baseline** to `true`.
3. The `baseline-refresh` job opens a PR bumping the baseline; a maintainer reviews and merges.

---

## 7. Alerting and Dashboards

Runtime alerts (`monitoring/performance-alerts.yml`) are consumed by Prometheus from the
webhook-delivery-service `/metrics` endpoint:

| Alert | Expression (abridged) | Severity |
|---|---|---|
| `WebhookIngestionP99High` | P99 of `webhook_ingestion_duration_milliseconds` > 100 | page |
| `WebhookIngestionP95Elevated` | P95 > 50ms for 10m | warning |
| `WebhookDeliveryP99High` | P99 of `webhook_delivery_duration_seconds` > 1s | warning |
| `WebhookQueueBacklog` | `webhook_queue_size_current` > 5000 | warning |
| `WebhookFailureRateHigh` | 5xx share of delivery attempts > 10% | warning |

Dashboards: the existing usage-dashboard (`/stats`, `p99LatencyMs`) and any Grafana board
scraping the webhook service show the same series the CI gate protects. Add
`webhook_ingestion_duration_milliseconds` P99 to the primary SLO dashboard panel.

---

## 8. Blue-Green / Canary Tie-in

Pre-merge gates protect `main`; deploy-time analysis protects production:

- **Blue-green**: after promoting the new build, watch `WebhookIngestionP99High` for 15 minutes
  before cutting over 100% of traffic. If the alert fires, route back to the previous build.
- **Canary**: send 5% → 25% → 50% → 100% through the canary slice. Compare the canary's
  ingestion P99 against the live baseline slice using the same 30% tolerance. Hold or roll back
  when the canary's P99 exceeds the baseline slice by >30% (mirrors `compare-results.ts` logic).
- **Staging chaos**: `docs/runbooks/chaos-engineering-staging.md` exercises the same P99 SLO —
  run the gate (`scripts/perf-regression-gate.sh`) on the staging build before and after chaos
  to quantify degradation.

---

## 9. Investigating a Failed Gate

A failing gate means one of:

1. **Real regression** — the PR changed hot-path code (express middleware, `enqueueWebhook`,
   metrics instrumentation, JSON serialization). Profile before/after:
   ```bash
   cd webhook-delivery-service
   npm run build
   node --prof dist/perf/benchmark.js --samples 2000
   ```
2. **SLO breach** — p99 > 100ms. Usually environment-related (shared runner, cold cache) or a
   true pathological change (blocking I/O in the request path). Never merge a PR that trips the
   SLO without a written explanation.
3. **Flaky runner** — re-run the workflow. If the failure is not reproducible across two
   consecutive runs and the delta is within a few ms of the tolerance, treat as noise, but
   prefer widening the sample count (`PERF_SAMPLES`) over raising tolerance.
4. **Harness breakage** — metric missing / exit 3. The benchmark itself changed shape (new
   metric names) without updating the baseline. Keep metric names in sync.

---

## 10. Troubleshooting

| Symptom | Cause | Fix |
|---|---|---|
| `error: no baseline found` | `.perf/perf-baselines.json` missing | Never delete it; restore from git |
| `Unexpected status 400` from `/webhooks` | Benchmark payload invalid | Check `postWebhook()` in `benchmark.ts` |
| All metrics show `WARN (no baseline)` | Metric renamed | Update baseline entries to the new names |
| Throughput fails on a faster machine | Baseline too optimistic | Refresh baseline (§6) |
| `npm ci` fails in CI | Lockfile out of date | Commit `package-lock.json` changes |
| High variance run-to-run | Too few samples | Raise `PERF_SAMPLES` (CI uses 800) |

---

## Related

- [Webhook Architecture](docs/WEBHOOK_ARCHITECTURE.md) — the <100ms P99 ingestion design
- [Webhook Runbook](docs/WEBHOOK_RUNBOOK.md) — operational procedures
- [SLO Monitoring](docs/SLO_MONITORING.md) — availability/latency SLO framework
- [Chaos Engineering Blueprint](docs/runbooks/chaos-engineering-staging.md) — staging P99 validation
