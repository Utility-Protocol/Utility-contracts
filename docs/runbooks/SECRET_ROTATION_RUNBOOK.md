# Secret Rotation Runbook

## Normal Rotation

1. Confirm dashboards show green dependency health and no active incident.
2. Trigger the scheduler or wait for the configured rotation window.
3. Verify a `Rotate` decision was emitted for each due secret ID.
4. Watch canary traffic for authentication failures, database connection errors, and P99 latency regressions.
5. Promote to full cutover when canary analysis passes.
6. Confirm the previous version is retired after the overlap window.

## Emergency Rotation

1. Open an incident and identify the compromised secret IDs.
2. Run an immediate rotation through the coordinator path, not a manual provider-only change.
3. Set canary to 100% only when the blast radius requires immediate revocation; otherwise keep overlap long enough for client reload.
4. Confirm all consumers observe the new version.
5. Retire the compromised version and record audit evidence.

## Rollback

1. Stop promotion if canary error budget burn exceeds the alert threshold.
2. Route traffic back to the previous version while it remains inside `previous_version_expires_at`.
3. Keep the failed version disabled for new traffic.
4. File a post-incident review before retrying rotation.
