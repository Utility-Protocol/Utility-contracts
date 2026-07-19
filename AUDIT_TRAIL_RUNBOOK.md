# Audit Trail Hash Chain Runbook

## Purpose

The utility contract maintains a compact, tamper-evident audit trail for critical service operations. Each audit entry commits to the previous entry hash, operation metadata, and a privacy-preserving payload digest so off-chain monitors can detect deletion, reordering, or mutation of historical records.

## Covered operations

The initial system-wide trail appends records for the critical billing and lifecycle paths:

- `register_meter`: meter registration and service provisioning.
- `top_up`: prepaid/postpaid funding events.
- `deduct_units`: signed IoT usage deductions.

Additional services should call the same internal append helper when adding state-changing critical paths.

## Verification workflow

1. Poll `get_audit_head()` and record the returned `sequence` and `record_hash` in monitoring storage.
2. Backfill missing records with `get_audit_record(sequence)`.
3. Run `verify_audit_chain(start_sequence, end_sequence)` for every contiguous range received from the contract.
4. Alert immediately if verification returns `false`, a sequence is missing, or the observed head hash regresses.

## Monitoring and alerting

Recommended production alerts:

- **AuditChainVerificationFailed**: `verify_audit_chain` returns `false` for any range.
- **AuditSequenceGap**: monitor observes a missing sequence between the previous head and current head.
- **AuditHeadRegression**: current head sequence is less than the last finalized monitor checkpoint.
- **AuditIngestionLag**: latest indexed sequence lags on-chain head for more than five minutes.

## Deployment guidance

Deploy the audit trail with the standard blue-green contract upgrade process:

1. Deploy the upgraded WASM to the green environment.
2. Replay a canary workload that registers a meter, funds it, and posts a signed usage report.
3. Verify the green audit chain from sequence `1` through the canary head.
4. Compare P99 latency against the `<100ms` critical-path target.
5. Promote green only after chain verification and latency checks pass.

## Incident response

If a monitor detects tampering:

1. Freeze downstream settlement automation that depends on the affected sequence range.
2. Capture `get_audit_head()` and all records around the failed range.
3. Re-run verification from the last known-good sequence.
4. Escalate to security review with the failed sequence, expected previous hash, observed previous hash, and record hash.
