# Database Migration Versioning with Rollback Support

This contract stores an explicit `StorageVersion` and advances schema changes through
versioned, resumable migration functions. Migrations run in small batches to keep
critical paths below the 100 ms P99 target and avoid exceeding Soroban instruction
budgets.

## Architecture

- `StorageVersion` records the active schema version.
- `MigrationCheckpoint` records `from_version`, `to_version`, cursor, and timestamps
  so a migration can be resumed after partial execution.
- `MigrationRollback` records rollback metadata prepared when a migration completes.
- Admin-only entrypoints gate mutation: `run_migration`, `rollback_migration`, and
  `cancel_migration`.

## Monitoring and alerting

The contract emits compact events for dashboards and alerting:

- `StrVer` when the storage version changes.
- `MigBatch` when a migration batch checkpoint advances.
- `MigDone` when a migration completes.
- `MigRoll` when a rollback completes.
- `MigCancel` when an active migration is cancelled.

Alert if a checkpoint remains active without cursor movement for more than one
operational window, or if rollback is invoked during canary analysis.

## Deployment runbook

1. Deploy new WASM with `finalize_upgrade_v2` after the veto window passes.
2. Run `run_migration(target_version)` repeatedly until it returns `true`.
3. Observe `MigBatch` and `MigDone` events during canary analysis.
4. If canary checks fail, call `rollback_migration(previous_version)`.
5. If a migration stalls before completion, call `cancel_migration` and investigate
   the last `MigrationCheckpoint`.

## Blue-green and canary guidance

Run the new contract code in green while the old version remains the blue fallback.
Advance storage in batches and route a small canary cohort first. Promote only after
version, rollback metadata, and service-level dashboards are healthy.
