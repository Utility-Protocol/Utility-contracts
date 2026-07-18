# Scheduled Database Backup Verification with Restore Testing

This runbook defines the system-wide backup verification pattern for services in the Utility Protocol stack. It is designed for scheduled jobs that prove backups can be restored, queried, monitored, and promoted safely without touching production state.

## Architecture

1. **Scheduler** starts a verification job after each database backup completes.
2. **Artifact validation** confirms that the backup exists, is fresh, and optionally matches a SHA-256 manifest.
3. **Isolated restore** restores the backup into an ephemeral directory, disposable database, or managed-database clone.
4. **Integrity probes** run service-owned checks, such as schema migration status, row-count thresholds, foreign-key validation, contract indexer checkpoint validation, or application health queries.
5. **Metrics export** writes Prometheus textfile metrics for success, duration, and last completion timestamp.
6. **Alerting** pages the on-call owner when restore verification fails, runs longer than the service objective, or has not completed within the expected backup interval.
7. **Blue-green/canary rollout** enables the job for one low-risk service first, then expands to 10%, 50%, and 100% of backup jobs after successful verification windows.

The implementation entry point is `scripts/verify_backup_restore.sh`. The script is database-agnostic: services provide their own restore and probe commands while the shared wrapper handles freshness checks, cleanup, canary gating, and metrics.

## Performance, Availability, and Security Bounds

- **Critical path budget:** Backup verification must run out of band and must not add latency to request paths. Any control-plane status endpoint that reads verification results must serve cached status in less than 100ms P99.
- **Availability target:** Verification jobs must never mutate production databases. Restore targets must be isolated and disposable to preserve the 99.99% uptime target.
- **Security controls:** Store backup credentials in the deployment secret manager, grant restore-only access where supported, encrypt artifacts at rest and in transit, and prevent production write credentials from being mounted into verification containers.
- **Data handling:** Use masked, least-privilege probes for regulated data. Do not print row contents or secrets in logs.
- **Retention:** Keep verification logs and metrics for at least the longest backup retention period so auditors can correlate every retained backup with a restore test.

## Example Scheduled Job

```bash
SERVICE_NAME=contract-indexer \
ENVIRONMENT=production \
scripts/verify_backup_restore.sh \
  --backup /backups/contract-indexer/latest.dump \
  --manifest /backups/contract-indexer/latest.dump.sha256 \
  --max-age-seconds 90000 \
  --metric-file /var/lib/node_exporter/textfile_collector/backup_restore.prom \
  --restore-command 'pg_restore --clean --if-exists --dbname "$VERIFY_DATABASE_URL" {backup}' \
  --probe-command 'psql "$VERIFY_DATABASE_URL" -v ON_ERROR_STOP=1 -c "select count(*) from ledger_checkpoints"' \
  --probe-command 'psql "$VERIFY_DATABASE_URL" -v ON_ERROR_STOP=1 -c "select max(sequence) from ledger_checkpoints"'
```

For SQLite-style local artifacts, restore into the provided temporary directory:

```bash
scripts/verify_backup_restore.sh \
  --backup ./backup.sqlite \
  --restore-command 'cp {backup} {restore_dir}/restore.sqlite' \
  --probe-command 'sqlite3 {restore_dir}/restore.sqlite "pragma integrity_check"'
```

## Monitoring and Alerting

The script emits these Prometheus metrics when `--metric-file` is provided:

| Metric | Type | Meaning |
| --- | --- | --- |
| `utility_backup_restore_verification_success` | gauge | `1` for the latest successful verification, `0` for failure. |
| `utility_backup_restore_verification_duration_seconds` | gauge | Runtime of the latest verification. |
| `utility_backup_restore_verification_last_timestamp_seconds` | gauge | Unix timestamp when the latest verification completed. |

Recommended alerts:

```promql
utility_backup_restore_verification_success{environment="production"} == 0
```

```promql
time() - utility_backup_restore_verification_last_timestamp_seconds{environment="production"} > 90000
```

```promql
histogram_quantile(0.99, rate(http_request_duration_seconds_bucket{route="/backup-verification/status"}[5m])) > 0.1
```

Dashboard panels should show the latest result by service, verification age, duration trend, artifact age, and failed probe logs.

## Deployment Plan

1. **Design review:** Confirm each service owner has documented restore commands, probe commands, secrets, and isolation boundaries.
2. **Blue environment:** Deploy verification jobs disabled by default with `--canary-percent 0` and validate scheduling, secrets, and metrics wiring.
3. **Canary:** Enable one non-critical service at 10% of scheduled runs. Require three consecutive successful restore tests before expansion.
4. **Green rollout:** Increase to 50%, then 100% of services after alert noise and runtime are acceptable.
5. **Promotion gate:** Do not mark backup automation production-ready until every critical database has at least one successful restore test in the dashboard.
6. **Rollback:** Set the job canary percentage back to `0`, revoke temporary restore credentials, and keep existing backup creation unchanged.

## Runbook

### Routine Verification

1. Confirm the latest backup completed.
2. Run `scripts/verify_backup_restore.sh` with the service restore and probe commands.
3. Confirm `utility_backup_restore_verification_success` is `1`.
4. Attach the job log and dashboard link to the backup audit record.

### Failure Response

1. Treat a failed restore test as a backup-severity incident, not as a production outage unless production data is also affected.
2. Freeze backup retention deletion for the affected service.
3. Re-run verification with `--dry-run` to confirm command rendering and artifact selection.
4. Restore the previous known-good artifact and compare failure mode.
5. Escalate to the database owner when two consecutive artifacts fail or the latest valid restore point is older than the recovery-point objective.
6. Update this runbook with the root cause, failed probe, and remediation.

## Test Coverage

At minimum, CI must parse the verification script with `bash -n`. Service repositories should also run a dry-run invocation and one disposable restore test for any database image they own.
