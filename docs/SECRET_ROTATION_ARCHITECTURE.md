# Secret Rotation Service Architecture

## Goals

The Secret Rotation Service coordinates database credential and API key lifecycle management across Utility Protocol services while keeping critical-path lookups under the 100 ms P99 target. It never logs or persists secret material in application state; only metadata such as secret IDs, versions, rollout windows, and retirement timestamps are exported.

## Components

1. **Rotation coordinator**: evaluates `SecretRecord` metadata against a `RotationPolicy`, creates a `RolloutPlan`, and delegates material creation/retirement to a secret provider.
2. **Secret provider adapter**: production implementations wrap the cloud KMS or secret manager. The Rust trait boundary is `SecretProvider`, which supports creating the next version and retiring the previous version.
3. **Metadata store**: durable storage for the fields represented by `SecretRecord`. The store should be replicated across zones and read-through cached by services.
4. **Service reload path**: each consumer watches version changes, warms a new connection/client pool with the canary credential, then flips traffic after canary analysis.

## Rotation Flow

1. Scheduler reads each record and policy.
2. Coordinator rotates once `now >= expires_at - overlap`.
3. Provider creates `version + 1` without exposing material to logs.
4. Blue-green rollout starts with `canary_percent` of eligible traffic.
5. Monitoring gates full cutover on authentication error rate, dependency latency, and application error budget burn.
6. Previous version remains valid until `retire_previous_at`, then is disabled and deleted according to provider retention policy.

## Security Controls

- Least-privilege provider roles: create/read current version only for producers, read current/previous only for consumers during overlap.
- Secret metadata is safe to export; raw values must stay inside KMS/secret-manager APIs.
- Rotation events require structured audit records with actor, secret ID, old/new versions, and rollout decision.
- Break-glass rotations must use the same coordinator path so monitoring and retirement guarantees remain intact.

## Availability and Performance

- Consumers use locally cached active version metadata and provider-side client caching to avoid network calls on hot request paths.
- Overlap windows permit zero-downtime pool warming and rollback to the previous version.
- Scheduler instances should use leader election or compare-and-swap writes to avoid double rotations.
