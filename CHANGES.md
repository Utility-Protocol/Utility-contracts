# Issue #122 — Configuration Management with Hot-Reload and Schema Validation

## What this implements

Your README already documents a security property that wasn't built yet:
"Multi-sig veto for fleet-level config changes (48h staging window)". This
delivers that, framed as issue #122's config management feature:

- **Schema validation**: every config field is declared once (type,
  required, min/max bounds, default) via `register_config_schema`. Every
  write — staged or emergency — is validated against it before anything
  touches storage.
- **Staged rollout with veto** (the "hot-reload" mechanism, contract-side):
  `propose_config_change` → optional `veto_config_change` (5% of active
  users blocks it, same threshold as your existing admin-transfer veto) →
  `apply_config_change` after a 48h default window (1h–30d configurable per
  proposal). Mirrors your existing `initiate_admin_transfer` /
  `veto_admin_transfer` / `execute_admin_transfer` pattern.
- **Emergency override**: `emergency_set_config_value` bypasses staging for
  a single field, admin-only, still schema-validated, with its own event
  (`ConfigEmergencyOverride`) for loud off-chain alerting.
- **One-step rollback**: `rollback_config_field` restores the previous value.
- **Version counter**: `get_config_version()` bumps on every applied change.
  This is the piece any off-chain service (dashboard, meter-simulator,
  webhook service) would poll or watch events for to know when to reload its
  own local config — the actual "hot-reload" trigger.

## Files changed

- **NEW** `contracts/utility_contracts/src/config_manager.rs` — the module.
  Self-contained (own `ConfigDataKey` storage enum, own events), same
  pattern as your existing `velocity_limit.rs`. Unit tests at the bottom
  cover pure validation logic (type/range checks).
- **NEW** `contracts/utility_contracts/src/config_manager_tests.rs` —
  integration tests through the full `UtilityContractClient` (schema
  registration + default fallback, staged apply after time-travel, veto
  blocking apply, unauthorized-admin rejection, emergency override +
  rollback, missing-schema rejection, type-mismatch rejection).
- **MODIFIED** `contracts/utility_contracts/src/lib.rs`:
  - 9 new `ContractError` variants, codes 125–133 (next free range after
    existing `CommitmentMismatch = 124`)
  - `pub mod config_manager;` + `#[cfg(test)] mod config_manager_tests;`
    registered alongside your other modules
  - 9 new public contract entry points (search `ISSUE #122: CONFIG
    MANAGEMENT` in lib.rs) inserted right after `register_active_user`:
    `register_config_schema`, `get_config_schema`, `get_config_value`,
    `get_config_version`, `propose_config_change`, `veto_config_change`,
    `cancel_pending_config_change`, `apply_config_change`,
    `emergency_set_config_value`, `rollback_config_field`

## ⚠️ Not compiler-verified

I don't have a Rust toolchain or network access in my environment, so I
could not run `cargo build` / `cargo test` against this. I did a careful
manual review against your existing patterns (mirrored `velocity_limit.rs`
and the admin-transfer veto flow line-by-line, checked every
`Vec::get(i).unwrap()`, storage key type, and event-topic symbol length
against code that's already compiling in your repo), and the braces balance
out, but you need to actually build it before trusting it. See below.

## How to verify locally

Run these from your repo root, after copying the three files from this zip
over your local clone (same relative paths):

```
cargo build --workspace
cargo test -p utility_contracts config_manager
```

If `cargo build` fails, **paste me the exact error output** and I'll fix it
directly — don't try to debug rustc errors yourself.

## What's still open (scope you said "all" on)

This delivers the **on-chain** half only. The **off-chain** half — real
file-watch hot-reload + JSON-schema validation for `usage-dashboard` and
`meter-simulator`, extending your existing `runtime-config-auditor.js` /
`config.js` — is not in this zip. Say the word and I'll build that next as
a separate PR-sized piece (keeping this one small and reviewable), using
`get_config_version()`'s change events as the trigger those services watch.
