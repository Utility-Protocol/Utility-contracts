# DR Failover Runbook — Utility Protocol

**Issue:** #121  
**Scope:** System-wide  
**Classification:** Engineering — On-Call  
**Last updated:** 2026-08-23

This runbook guides on-call operators through a controlled disaster recovery failover for the Utility Protocol stack. Follow each section in order. Do not skip steps without documenting the reason.

---

## Table of Contents

1. [Pre-Failover Checklist](#1-pre-failover-checklist)
2. [Severity Assessment](#2-severity-assessment)
3. [Automatic Failover Conditions](#3-automatic-failover-conditions)
4. [Manual Failover Procedure](#4-manual-failover-procedure)
5. [Post-Failover Validation](#5-post-failover-validation)
6. [Failback Procedure](#6-failback-procedure)
7. [Canary Promotion for DR Config Changes](#7-canary-promotion-for-dr-config-changes)
8. [Alert Reference](#8-alert-reference)
9. [Contact Tree](#9-contact-tree)

---

## 1. Pre-Failover Checklist

Run every check before executing a failover. Do not proceed to step 4 without completing all checks.

```bash
# 1. Confirm alert is genuine (not a metrics scrape glitch).
#    Look for at least 3 consecutive CRITICAL health evaluations.
kubectl get prometheusrule utility-contracts-multi-region-dr -n monitoring

# 2. Verify primary region is actually failing.
scripts/dr-test.sh --region us-east-1 --test-scenario connectivity

# 3. Check secondary region health and replication lag.
scripts/dr-test.sh --region eu-west-1 --test-scenario replication-lag

# 4. Confirm no active failover is already in progress.
# (Look for FAILOVER_IN_PROGRESS state in region health metrics.)

# 5. Notify incident commander and communications lead.
#    Declare incident in incident management system.
#    Record the incident ticket number.

# 6. Confirm rollback owner is online and has reviewed this runbook.
```

---

## 2. Severity Assessment

| Symptom | Region state | Action |
|---|---|---|
| `RegionHealthCritical` alert for primary | `CRITICAL` | Proceed to section 4 |
| `ReplicationLagHigh` alert | Secondary lag > 60s | Do NOT failover — investigate replication first |
| `DRConsecutiveCriticalEvaluations` ≥ 3 | `CRITICAL` × 3 | Evaluate automatic failover |
| `MultiRegionAvailabilityLow` | Availability < 99.99% | Check all regions; may not require failover |
| AZ failure (not full region) | Some `DEGRADED` | Route within region, do not failover |
| Full primary region failure | All primary services down | Execute full failover (section 4) |

---

## 3. Automatic Failover Conditions

The DR controller triggers automatic failover when **all** of the following are true:

1. Primary region health check fails for **3 consecutive intervals** (30-second polling = 90 seconds total).
2. Secondary region health check is `HEALTHY`.
3. Secondary replication lag is ≤ 60 seconds (within RPO).
4. No active failover is in progress.
5. Last successful failover was more than **10 minutes** ago.

If automatic failover does not fire within 5 minutes of primary failure, escalate to manual failover (section 4).

---

## 4. Manual Failover Procedure

### Step 1 — Dry run (always start here)

```bash
scripts/dr-failover.sh \
  --from-region us-east-1 \
  --to-region eu-west-1 \
  --dry-run
```

Review the output. Confirm all steps are correct before removing `--dry-run`.

### Step 2 — Execute failover

```bash
scripts/dr-failover.sh \
  --from-region us-east-1 \
  --to-region eu-west-1 \
  --force \
  --metric-file /var/lib/node_exporter/textfile_collector/dr_failover.prom
```

> **CAUTION:** `--force` bypasses interactive confirmation. Use only after dry-run review and explicit incident commander approval.

### Step 3 — Monitor during failover

Watch the following metrics in Grafana (`monitoring/multi-region-dr-dashboard.json`):

- `utility_region_health_status` — eu-west-1 should reach `HEALTHY` (1).
- `utility_replication_lag_seconds` — lag should stabilise at 0.
- Istio P99 latency — must return below 100ms within RTO window (5 minutes).
- Error rate — must return below 0.01%.

### Step 4 — Abort if needed

If any of the following occur, abort and run rollback:

```bash
# Rollback: restore us-east-1 as primary.
scripts/dr-failover.sh \
  --from-region eu-west-1 \
  --to-region us-east-1 \
  --rollback \
  --force
```

Abort conditions:
- eu-west-1 does not reach `HEALTHY` within 5 minutes.
- P99 latency remains above 100ms for more than 5 minutes after cutover.
- Error rate above 0.01% for more than 5 minutes.
- Replication lag in eu-west-1 is rising (not stabilising).
- Any billing invariant or settlement accounting discrepancy detected.

---

## 5. Post-Failover Validation

Run immediately after failover completes:

```bash
# Connectivity and latency.
scripts/dr-test.sh --region eu-west-1 --test-scenario connectivity

# Replication lag baseline in the new primary.
scripts/dr-test.sh --region eu-west-1 --test-scenario replication-lag

# Validate RPO and RTO targets were met.
scripts/dr-test.sh --region eu-west-1 --test-scenario rpo-validation
scripts/dr-test.sh --region eu-west-1 --test-scenario rto-validation
```

Expected results:
- All connectivity checks pass.
- Replication lag < 30s (new primary replicating to remaining standby).
- No RPO violations during the failover window.
- Observed RTO < 300s.

Monitor for **30 minutes** after failover before closing the incident.

---

## 6. Failback Procedure

After the original primary region is restored:

1. Verify us-east-1 health is `HEALTHY`.
2. Confirm replication from eu-west-1 (current primary) to us-east-1 has caught up (lag < 5s).
3. Run a DR test against us-east-1 to verify readiness.
4. Execute failback:

```bash
scripts/dr-failover.sh \
  --from-region eu-west-1 \
  --to-region us-east-1 \
  --force \
  --metric-file /var/lib/node_exporter/textfile_collector/dr_failback.prom
```

5. Validate with post-failover tests (section 5) targeting us-east-1.
6. Update documentation with root cause and resolution timeline.

---

## 7. Canary Promotion for DR Config Changes

Use the canary promotion script to safely roll out changes to DR configuration:

```bash
# Stage 1: 5% of traffic to green DR slice.
scripts/dr-canary-promote.sh --stage 5 --namespace utility-contracts --dry-run
scripts/dr-canary-promote.sh --stage 5 --namespace utility-contracts

# Wait 15 minutes. Check P99 latency, availability, and replication lag.

# Stage 2: 25%.
scripts/dr-canary-promote.sh --stage 25 --namespace utility-contracts

# Wait 15 minutes.

# Stage 3: 50% (requires --force).
scripts/dr-canary-promote.sh --stage 50 --namespace utility-contracts --force

# Wait 30 minutes.

# Stage 4: Production (100%, requires --force).
scripts/dr-canary-promote.sh --stage 100 --namespace utility-contracts --force
```

**Rollback at any stage:**

```bash
scripts/dr-canary-promote.sh --rollback --namespace utility-contracts
```

### Canary abort criteria

Roll back immediately if any of the following occur:

- P99 latency > 100ms sustained for 5 minutes.
- Availability drops below 99.99%.
- Replication lag > 60s in canary slice.
- Error rate > 0.01%.
- `DRCanaryAnalyzer` returns `ROLLBACK` decision for 3 consecutive windows.

---

## 8. Alert Reference

| Alert | Meaning | Action |
|---|---|---|
| `ReplicationLagHigh` | Lag > 60s for 5 min | Investigate WAL or MirrorMaker; do not failover until resolved |
| `RegionHealthCritical` | Region unhealthy for 2 min | Assess for manual failover |
| `FailoverRPOViolation` | RPO breached | Page incident commander; assess data loss impact |
| `FailoverRTOViolation` | RTO > 300s | Post-incident review; improve automation |
| `CrossRegionLatencyHigh` | P99 > 100ms cross-region | Investigate network path; may not require failover |
| `ReplicationBytesZero` | No replication for 10 min | Check replication process; restart MirrorMaker if needed |
| `DRTestStale` | No DR test in 24h | Run `scripts/dr-test.sh` |
| `MultiRegionAvailabilityLow` | Availability < 99.99% | Urgent: check all regions; escalate if primary failing |
| `DRConsecutiveCriticalEvaluations` | 3+ critical evaluations | Evaluate automatic failover criteria |
| `CanaryPromotionBlocked` | Canary ROLLBACK decisions | Review canary metrics; fix before promoting |

---

## 9. Contact Tree

| Role | Responsibility | Escalation |
|---|---|---|
| On-call engineer | First responder, executes runbook | 5 min |
| Incident commander | Go/no-go for failover, communicates to stakeholders | 10 min |
| Rollback approver | Approves rollback if failover goes wrong | 15 min |
| DB team lead | PostgreSQL promotion and WAL validation | 20 min |
| Network/infra lead | DNS cutover, Kafka rebalance | 20 min |

---

## Related Documents

- Architecture: `docs/MULTI_REGION_DR_ARCHITECTURE.md`
- Emergency Runbook: `EMERGENCY_RUNBOOK.md`
- Chaos Engineering Blueprint: `docs/runbooks/chaos-engineering-staging.md`
- Backup Verification: `docs/SCHEDULED_BACKUP_VERIFICATION.md`
- SLO Monitoring: `docs/SLO_MONITORING.md`
