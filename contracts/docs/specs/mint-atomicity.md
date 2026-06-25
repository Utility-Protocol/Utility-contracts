# Mint / Burn Supply-Cap Atomicity (resource-token)

Issue #1 — "Race Condition in Resource Tokenization Mint/Burn State Machine"

## Summary

`resource-token` mints/burns tokens that are backed **1:1** by real-world resource
deposits. The invariant is:

```
total_supply == Σ(balances) <= MAX_SUPPLY
```

`MAX_SUPPLY = 1_000_000_000_000_000` (10^15 base units).

## What the original code was missing

`mint()` overflow-checked `total_supply` but **never enforced an upper bound** —
supply could grow without limit, so the `<= MAX_SUPPLY` half of the invariant was
not enforced at all. `burn()` used unchecked subtraction (`-`), which silently
wraps in builds without `overflow-checks` (the workspace release profile does not
enable them, and per-crate `[profile.release]` is ignored for workspace members).

## The fix

- `mint()` computes `new_supply = current_supply.checked_add(amount)` and **rejects
  the call** (`panic!("Max supply exceeded")`) when `new_supply > MAX_SUPPLY`,
  **before** writing any state.
- `burn()` uses `checked_sub` for both balance and total supply.

The check-then-write ordering means no partial state is committed on rejection.

## On the "race condition" framing

The issue describes two `mint()` calls in the **same ledger** both observing
`total_supply == MAX_SUPPLY - 1` and both proceeding. That cannot happen on
Stellar/Soroban:

- Transactions are applied **serially** by the host. There is no concurrent
  execution of two invocations against the same contract state.
- Each transaction reads the **committed** state left by the previous one and its
  writes are atomic with respect to other transactions.

So a cross-transaction "check-and-set race" within a ledger does not exist, and
the remedies proposed for that model do not apply here:

- **`MINT_INFLIGHT` lock** — would only matter for *re-entrancy* (a nested call
  back into `mint` within one invocation). `mint`/`burn` make no external
  contract calls, so there is no re-entrancy vector to guard. Adding a lock would
  be dead code.
- **Two-phase commit + background finalization** — Soroban has no background
  processes and no cross-ledger uncommitted state; there is nothing to finalize
  asynchronously.

The real, enforceable defect was the missing cap. Enforcing it (plus
overflow-safe arithmetic) fully restores the invariant.

## Tests

`contracts/resource-token/src/test.rs`:

- `test_mint_up_to_max_supply_succeeds` — minting exactly `MAX_SUPPLY` is allowed.
- `test_mint_exceeding_max_supply_panics` — one unit past the cap is rejected.
- `test_mint_overflowing_supply_in_two_steps_panics` — the issue's `MAX_SUPPLY-1`
  scenario, modelled as the serial calls Soroban actually performs.
- `test_repeated_mints_never_exceed_max_supply` — 100 sequential mints (the
  "100 concurrent calls" analog) keep `total_supply <= MAX_SUPPLY` and
  `total_supply == Σ(balances)` at every step.
- `test_burn_after_max_supply_allows_reminting` — burning frees cap headroom.

## Note on `MIN_MINT_AMOUNT`

The issue also lists `MIN_MINT_AMOUNT = 1_000_000`. It is **not** enforced here:
it is a dust-control policy orthogonal to the supply invariant, and enforcing it
would break the contract's existing small-amount mint/burn behaviour and test
suite. It can be added as a separate, deliberate policy change if desired.
