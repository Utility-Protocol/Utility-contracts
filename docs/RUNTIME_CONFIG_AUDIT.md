# Runtime Configuration Auditing and Drift Detection

This design introduces a system-wide runtime configuration audit layer for Utility Protocol services. The first implementation ships with the meter simulator and defines the rollout pattern for contracts, dashboards, relayers, and operational services.

## Goals

- Capture deterministic runtime configuration snapshots for each service.
- Hash redacted configuration state so operators can compare services without exposing secrets.
- Detect path-level drift between an approved baseline and current runtime state.
- Keep critical-path audits below the 100 ms P99 target by using in-process canonicalization and SHA-256 hashing only.
- Emit data that can be wired into monitoring, alerting, dashboards, blue-green deploys, and canary analysis.

## Architecture

1. **Baseline creation**: each service creates a redacted snapshot after configuration loading and stores the approved hash with deployment metadata.
2. **Runtime audit**: services periodically compare their current effective configuration against the baseline.
3. **Drift report**: reports include the service name, baseline hash, current hash, drift flag, path-level changes, runtime duration, and budget status.
4. **Monitoring**: drift reports should be exported as metrics and structured logs. Alert on `driftDetected=true` or `withinBudget=false`.
5. **Deployment safety**: blue-green and canary flows should compare baseline hashes before shifting traffic and fail the rollout if unexpected drift appears.

## Security Model

- Secret-like paths containing `password`, `private`, `secret`, `token`, or `key` are redacted before hashing and reporting.
- Drift reports expose paths and redacted values, not raw credentials.
- Baseline updates are treated as privileged operational changes and should go through the same security review as contract configuration changes.

## Runbook

1. Create or refresh the baseline for the target service during deployment.
2. Verify the baseline hash is recorded in deployment metadata.
3. Enable periodic audits and export reports to monitoring.
4. If drift is detected, compare changed paths against the approved change record.
5. If drift is unapproved, freeze rollout, page the service owner, and restore the approved configuration.
6. For canaries, block promotion until drift reports remain clean for the canary analysis window.

## Meter Simulator Implementation

The meter simulator exposes `RuntimeConfigAuditor` from `src/runtime-config-auditor.js`. It provides:

- `setBaseline(config)` to create the approved snapshot.
- `audit(config)` to produce a drift report.
- Deterministic hashing that is stable across object key ordering.
- Secret redaction before hashing or reporting.
