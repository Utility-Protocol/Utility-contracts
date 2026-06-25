# Mint/Burn Authorization — Model & Safety Invariants

Issue #4 — "Custom Validation Macro Override in Resource Tokenization
Authorization"

## Scope correction

The issue describes a procedural macro `#[requires_role(Role::Minter)]` whose
expansion skips the authorization check when an `#[allow(unused)]` attribute is
present, allowing unauthorized minting.

**No such macro exists in this codebase.** There is no `#[requires_role]`
attribute, no `Role` enum, and no `role_check` proc-macro crate. Authorization is
not attribute-driven, so the described macro-expansion bypass is not applicable.

This document records the **actual** authorization model, the invariant it
upholds, and the tests that now lock it in.

## Actual authorization model (`resource-token`)

```
mint(env, to, amount)   -> authorize_mint(env)  -> authorize_with_chain(env)
burn(env, from, amount) -> authorize_burn(env)  -> authorize_with_chain(env)

authorize_with_chain(env):
    admin = get_admin(env)            // panics "NoAdmin" if unset
    admin.require_auth()              // panics if the admin has not authorized
```

`authorize_*` is the **first statement** of `mint`/`burn`, before any state is
read or written. The gate is `Address::require_auth()`, which the Soroban host
validates against the actual authorization context — it cannot be spoofed by an
intermediate contract in the call chain. There is no code path that reaches the
balance/supply mutation without passing `admin.require_auth()`.

### Invariant

```
∀ mint/burn operation:  the admin has authorized the operation (require_auth)
```

## Why this could regress silently (the real gap the issue points at)

Every pre-existing test uses `env.mock_all_auths()`, which auto-approves **all**
authorization. Under that mode the gate is never exercised: a regression that
deleted `authorize_mint()`/`authorize_burn()` from `mint`/`burn` would still pass
the entire suite.

### Tests added

`contracts/resource-token/src/test.rs`:

- `test_mint_rejected_without_authorization` — after setup, drop all auth with
  `env.set_auths(&[])`; `mint` must panic.
- `test_burn_rejected_without_authorization` — same, for `burn`.
- `test_mint_rejected_without_auth_leaves_state_unchanged` — `try_mint` returns
  `Err` and neither balance nor total supply changes.

These exercise the gate with an empty authorization set, so removing or bypassing
the authorization call now fails the suite.

## Known limitation / recommended follow-up (out of scope here)

`authorize_with_chain` only honors the **admin**. The contract also exposes
`authorize_operator` / `is_valid_operator` and documents that "the admin or a
valid operator can mint", but the mint/burn path **never checks operators** — an
operator cannot actually mint, because `require_auth()` is only ever called on
the admin's address.

Making operator-delegated minting work securely requires threading the caller's
`Address` into `mint`/`burn` (so the contract can `require_auth()` the operator
and verify it is currently valid). That is a **breaking signature change** and is
intentionally left as a separate change; the misleading documentation should be
corrected at the same time.

The current behaviour is **safe** (only the admin can mint/burn); the limitation
is reduced functionality, not an authorization bypass.
```
