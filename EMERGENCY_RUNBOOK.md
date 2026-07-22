# Emergency Runbook — Utility-Protocol Contracts

**Contract ID (Testnet):** `CB7PSJZALNWNX7NLOAM6LOEL4OJZMFPQZJMIYO522ZSACYWXTZIDEDSS`  
**Network:** Stellar Testnet — replace `--network testnet` with `--network mainnet` for production  
**Last updated:** 2026-04-26  
**Classification:** CONFIDENTIAL — DAO Core Team Only

---

## Table of Contents

1. [Roles and Responsibilities](#1-roles-and-responsibilities)
2. [Pre-Incident Checklist](#2-pre-incident-checklist)
3. [Scenario A — Active Exploit / Hack in Progress](#3-scenario-a--active-exploit--hack-in-progress)
4. [Scenario B — Protocol Pause (Planned or Precautionary)](#4-scenario-b--protocol-pause-planned-or-precautionary)
5. [Scenario C — Wasm Hash Upgrade](#5-scenario-c--wasm-hash-upgrade)
6. [Scenario D — Migrating Trapped State](#6-scenario-d--migrating-trapped-state)
7. [Scenario E — Multi-Sig Withdrawal Freeze](#7-scenario-e--multi-sig-withdrawal-freeze)
8. [Scenario F — Legal Freeze](#8-scenario-f--legal-freeze)
9. [Scenario G — Gas Buffer Exhaustion](#9-scenario-g--gas-buffer-exhaustion)
10. [Scenario H — Admin Key Compromise](#10-scenario-h--admin-key-compromise)
11. [Scenario I — Oracle Failure](#11-scenario-i--oracle-failure)
12. [Scenario J — Velocity Limit Breach / Flash Drain](#12-scenario-j--velocity-limit-breach--flash-drain)
13. [Scenario N — Kafka Consumer Group Lag Spike](#13-scenario-n--kafka-consumer-group-lag-spike)
14. [Scenario O — Kafka Auto-Scaling Actuator Failure](#14-scenario-o--kafka-auto-scaling-actuator-failure)
15. [Post-Incident Procedures](#15-post-incident-procedures)
14. [Multi-Sig Signer Reference Card](#14-multi-sig-signer-reference-card)
15. [Contact Tree](#15-contact-tree)

---

## 1. Roles and Responsibilities

| Role | On-chain Key / Storage | Duty |
|---|---|---|
| **DAO Admin** | `DataKey::CurrentAdmin` | Propose/finalize Wasm upgrades, set compliance officer, grant provider verification, set velocity limits |
| **Compliance Officer** | `DataKey::ComplianceOfficer` | Trigger and release legal freezes |
| **Finance Wallet (×3–5)** | `MultiSigConfig.finance_wallets` | Propose, approve, revoke, and cancel large withdrawal requests; quorum = `required_signatures` |
| **Oracle / Resolver** | `DataKey::Oracle` | Resolve service challenges (`resolve_challenge`) |
| **Provider** | Per-meter `provider` field | Pause/shutdown individual meters, initiate firmware updates, manage gas buffer |
| **Compliance Council** | Off-chain multi-sig (≥2) | Release legal freezes |

### Multi-sig quorum rule

Any action requiring `required_signatures` approvals **must be coordinated off-chain first** (Signal group, emergency Telegram, or PagerDuty). Confirm quorum is available before submitting the first on-chain transaction. The contract enforces the threshold — a request with insufficient approvals will revert on execution.

### Key storage locations (for incident verification)

```
DataKey::CurrentAdmin          → DAO Admin address
DataKey::ComplianceOfficer     → Compliance Officer address
DataKey::Oracle                → Oracle/Resolver address
DataKey::MultiSigConfig(addr)  → Per-provider multi-sig config
DataKey::VetoDeadline          → Active upgrade veto deadline (Unix timestamp)
DataKey::ProposedUpgrade       → Active UpgradeProposal struct
```

---

## 2. Pre-Incident Checklist

Run every check before executing any emergency command. Do not skip steps.

```bash
# 1. Confirm Stellar CLI is installed and on PATH
stellar --version

# 2. Confirm you are targeting the correct network
stellar network ls

# 3. Export the contract address
export CONTRACT=CB7PSJZALNWNX7NLOAM6LOEL4OJZMFPQZJMIYO522ZSACYWXTZIDEDSS

# 4. Export signing identities for your role
export ADMIN_KEY=<admin-secret-key-or-identity-alias>
export PROVIDER_KEY=<provider-secret-key-or-identity-alias>
export FINANCE_KEY=<finance-wallet-secret-key-or-identity-alias>

# 5. Verify the contract is responsive
stellar contract invoke \
  --id $CONTRACT \
  --network testnet \
  -- \
  get_count

# 6. Check the current meter count and note it
export METER_COUNT=$(stellar contract invoke \
  --id $CONTRACT \
  --network testnet \
  -- \
  get_count)
echo "Total meters: $METER_COUNT"

# 7. Verify your key matches the expected admin address
stellar keys address $ADMIN_KEY
# Compare output against the address stored in DataKey::CurrentAdmin

# 8. Check block explorer for any anomalous recent transactions
# https://stellar.expert/explorer/testnet/contract/$CONTRACT
```

> **If the contract is unresponsive:** The Stellar network may be congested or the contract TTL may have expired. Check https://status.stellar.org and the block explorer before proceeding.

---

## 3. Scenario A — Active Exploit / Hack in Progress

**Trigger:** Anomalous withdrawals detected, funds draining faster than expected, or a known vulnerability is being actively exploited.

**Goal:** Stop all outflows immediately and preserve remaining funds.

**Time budget:** Act within 5 minutes of detection. Every ledger (~5 seconds) is a potential loss.

### Step 1 — Pause affected meters (Provider key)

`challenge_service` sets `is_disputed = true` and `is_paused = true`, blocking all `claim` and `deduct_units` calls immediately.

```bash
# Run once per affected meter. Replace METER_ID with each affected ID.
stellar contract invoke \
  --id $CONTRACT \
  --network testnet \
  --source $PROVIDER_KEY \
  -- \
  challenge_service \
  --meter_id <METER_ID>
```

To pause all meters in a loop:

```bash
for i in $(seq 1 $METER_COUNT); do
  stellar contract invoke \
    --id $CONTRACT \
    --network testnet \
    --source $PROVIDER_KEY \
    -- \
    challenge_service \
    --meter_id $i
  echo "Challenged meter $i"
done
```

### Step 2 — Hard shutdown (Provider key)

If `challenge_service` is insufficient (e.g., the exploit bypasses the dispute flag), use the unconditional hard stop:

```bash
stellar contract invoke \
  --id $CONTRACT \
  --network testnet \
  --source $PROVIDER_KEY \
  -- \
  emergency_shutdown \
  --meter_id <METER_ID>
```

`emergency_shutdown` sets `is_active = false` regardless of balance or dispute state.

### Step 3 — Pause all continuous flow streams (Provider key)

```bash
# Pause each stream by setting flow rate to 0
stellar contract invoke \
  --id $CONTRACT \
  --network testnet \
  --source $PROVIDER_KEY \
  -- \
  pause_continuous_flow \
  --stream_id <STREAM_ID>
```

### Step 4 — Revoke any active velocity overrides (Admin key)

If an attacker obtained an admin override to bypass velocity limits:

```bash
# Revoke global override (meter_id = 0)
stellar contract invoke \
  --id $CONTRACT \
  --network testnet \
  --source $ADMIN_KEY \
  -- \
  revoke_velocity_override \
  --admin <ADMIN_ADDRESS> \
  --meter_id 0

# Revoke per-meter override
stellar contract invoke \
  --id $CONTRACT \
  --network testnet \
  --source $ADMIN_KEY \
  -- \
  revoke_velocity_override \
  --admin <ADMIN_ADDRESS> \
  --meter_id <METER_ID>
```

### Step 5 — Enable global velocity limiting (Admin key)

Cap all outflows system-wide while the incident is investigated:

```bash
stellar contract invoke \
  --id $CONTRACT \
  --network testnet \
  --source $ADMIN_KEY \
  -- \
  set_velocity_limit_config \
  --admin <ADMIN_ADDRESS> \
  --global_limit 1000000 \
  --per_stream_limit 100000 \
  --is_enabled true
```

Adjust `global_limit` and `per_stream_limit` (in stroops) to the minimum needed for legitimate operations.

### Step 6 — Cancel all pending multi-sig withdrawal requests (Provider key)

```bash
# Get the total request count first
stellar contract invoke \
  --id $CONTRACT \
  --network testnet \
  -- \
  get_withdrawal_request_count \
  --provider <PROVIDER_ADDRESS>

# Cancel each pending request
stellar contract invoke \
  --id $CONTRACT \
  --network testnet \
  --source $PROVIDER_KEY \
  -- \
  cancel_multisig_withdrawal \
  --provider <PROVIDER_ADDRESS> \
  --request_id <REQUEST_ID>
```

### Step 7 — Notify the DAO and begin post-mortem

See [Section 13](#13-post-incident-procedures).

---

## 4. Scenario B — Protocol Pause (Planned or Precautionary)

**Trigger:** Scheduled maintenance, oracle outage, or precautionary halt before a known vulnerability is patched.

### Pause a single meter (Provider key)

```bash
stellar contract invoke \
  --id $CONTRACT \
  --network testnet \
  --source $PROVIDER_KEY \
  -- \
  set_meter_pause \
  --meter_id <METER_ID> \
  --paused true
```

### Pause a continuous flow stream (Provider key)

```bash
stellar contract invoke \
  --id $CONTRACT \
  --network testnet \
  --source $PROVIDER_KEY \
  -- \
  pause_continuous_flow \
  --stream_id <STREAM_ID>
```

### Enable global velocity limiting (Admin key)

```bash
stellar contract invoke \
  --id $CONTRACT \
  --network testnet \
  --source $ADMIN_KEY \
  -- \
  set_velocity_limit_config \
  --admin <ADMIN_ADDRESS> \
  --global_limit 1000000 \
  --per_stream_limit 100000 \
  --is_enabled true
```

### Resume after the all-clear

```bash
# Resume a meter
stellar contract invoke \
  --id $CONTRACT \
  --network testnet \
  --source $PROVIDER_KEY \
  -- \
  set_meter_pause \
  --meter_id <METER_ID> \
  --paused false

# Resume a continuous flow stream with the original flow rate
stellar contract invoke \
  --id $CONTRACT \
  --network testnet \
  --source $PROVIDER_KEY \
  -- \
  resume_continuous_flow \
  --stream_id <STREAM_ID> \
  --flow_rate_per_second <ORIGINAL_RATE>

# Disable velocity limiting once normal operations resume
stellar contract invoke \
  --id $CONTRACT \
  --network testnet \
  --source $ADMIN_KEY \
  -- \
  set_velocity_limit_config \
  --admin <ADMIN_ADDRESS> \
  --global_limit 1000000000 \
  --per_stream_limit 100000000 \
  --is_enabled false
```

---

## 5. Scenario C — Wasm Hash Upgrade

**Trigger:** A critical bug is patched and a new Wasm binary is ready to deploy.

**Timelock:** The contract enforces a veto window (`UPGRADE_VETO_PERIOD_SECONDS`). Users may veto during this window. The upgrade only finalizes if the veto count stays below the threshold (`VETO_THRESHOLD_BPS`). There is **no on-chain bypass** of the timelock — it is a safety feature.

### Step 1 — Build and upload the new Wasm

```bash
# Build the contract (from repo root)
cd contracts/utility_contracts
cargo build --target wasm32-unknown-unknown --release

# Verify the binary size is reasonable (Soroban limit is 64 KB)
ls -lh target/wasm32-unknown-unknown/release/utility_contracts.wasm

# Upload the Wasm to the network — this registers the binary but does NOT deploy it
stellar contract upload \
  --network testnet \
  --source $ADMIN_KEY \
  --wasm target/wasm32-unknown-unknown/release/utility_contracts.wasm

# The command prints a 32-byte hex Wasm hash. Save it immediately.
export NEW_WASM_HASH=<printed-hash>
echo "New Wasm hash: $NEW_WASM_HASH"
```

> **Verify the hash independently.** Every signer should compute `sha256` of the Wasm file locally and compare it to `NEW_WASM_HASH` before approving the proposal.
>
> ```bash
> sha256sum target/wasm32-unknown-unknown/release/utility_contracts.wasm
> ```

### Step 2 — Propose the upgrade (Admin key)

```bash
stellar contract invoke \
  --id $CONTRACT \
  --network testnet \
  --source $ADMIN_KEY \
  -- \
  propose_upgrade \
  --new_wasm_hash $NEW_WASM_HASH
```

The contract emits an `UpgrdPrp` event and stores the proposal at `DataKey::ProposedUpgrade`. The veto window starts immediately. **Announce the proposal to the DAO governance forum and all stakeholders now.**

### Step 3 — Communicate the veto window

Post the following information to the DAO forum:

- New Wasm hash (`NEW_WASM_HASH`)
- SHA-256 of the Wasm file (for independent verification)
- Link to the audited diff / changelog
- Veto deadline (read from `DataKey::VetoDeadline`)
- Instructions for users who wish to veto (see below)

### Step 4 — Monitor the veto window

```bash
# Read the veto deadline from contract storage (via block explorer or CLI)
# DataKey::VetoDeadline stores the Unix timestamp of the deadline.
# If veto count exceeds VETO_THRESHOLD_BPS of total meters, the upgrade is blocked.

# Check the block explorer for VetoSubmt events:
# https://stellar.expert/explorer/testnet/contract/$CONTRACT
```

**Do NOT call `finalize_upgrade` before the deadline expires.**

### Step 5 — Finalize the upgrade (Admin key)

Only after the veto window has passed and the veto count is below the threshold:

```bash
stellar contract invoke \
  --id $CONTRACT \
  --network testnet \
  --source $ADMIN_KEY \
  -- \
  finalize_upgrade
```

The contract emits `UpgrdFin`, clears the proposal, and the contract now runs the new Wasm.

### Step 6 — Verify the upgrade

```bash
# Confirm the contract is responsive under the new Wasm
stellar contract invoke \
  --id $CONTRACT \
  --network testnet \
  -- \
  get_count

# Check a known meter to confirm state was preserved
stellar contract invoke \
  --id $CONTRACT \
  --network testnet \
  -- \
  get_meter \
  --meter_id 1
```

### Emergency fast-track (critical zero-day)

If the veto window is too long for a zero-day patch:

1. The DAO must vote off-chain (governance forum + Signal) to accept the risk.
2. Document the decision with a timestamped record before calling `finalize_upgrade`.
3. There is **no on-chain bypass** — the timelock must expire naturally.
4. If the window is truly unacceptable, consider pausing all meters (Scenario B) while waiting for the window to expire.

### Rollback procedure

If the new Wasm introduces a regression:

1. Build and upload the previous known-good Wasm binary.
2. Repeat Steps 1–5 with the rollback hash.
3. The same veto window applies to rollbacks.

---

## 6. Scenario D — Migrating Trapped State

**Trigger:** A bug causes state to become inaccessible or corrupted, and a migration contract is needed to rescue funds or re-initialize storage.

**Warning:** State migration is the highest-risk operation in this runbook. Require DAO approval and an independent audit of the migration contract before proceeding.

### Overview

Soroban contracts cannot iterate all storage keys natively. Migration must be performed key-by-key using known meter IDs and stream IDs obtained from the `Count` storage key.

### Step 1 — Pause the old contract (prevent state changes during migration)

```bash
# Pause every meter
for i in $(seq 1 $METER_COUNT); do
  stellar contract invoke \
    --id $CONTRACT \
    --network testnet \
    --source $PROVIDER_KEY \
    -- \
    set_meter_pause \
    --meter_id $i \
    --paused true
  echo "Paused meter $i"
done
```

### Step 2 — Enumerate and dump all meter state

```bash
# Get the total meter count
export METER_COUNT=$(stellar contract invoke \
  --id $CONTRACT \
  --network testnet \
  -- \
  get_count)

# Dump each meter to a JSON file
mkdir -p migration_state
for i in $(seq 1 $METER_COUNT); do
  stellar contract invoke \
    --id $CONTRACT \
    --network testnet \
    -- \
    get_meter \
    --meter_id $i > migration_state/meter_$i.json
  echo "Dumped meter $i"
done
```

### Step 3 — Dump continuous flow stream state

```bash
# Stream IDs share the same counter as meters (DataKey::Count)
for i in $(seq 1 $METER_COUNT); do
  stellar contract invoke \
    --id $CONTRACT \
    --network testnet \
    -- \
    get_continuous_flow \
    --stream_id $i > migration_state/stream_$i.json 2>/dev/null || true
done
```

### Step 4 — Dump gas buffer state for each provider

```bash
# Collect unique provider addresses from the meter dumps, then:
stellar contract invoke \
  --id $CONTRACT \
  --network testnet \
  -- \
  get_gas_buffer \
  --provider <PROVIDER_ADDRESS> > migration_state/gas_buffer_<PROVIDER_ADDRESS>.json
```

### Step 5 — Deploy the migration contract

The migration contract must be:
- Pre-audited by an independent security firm
- Approved by DAO governance vote
- Able to accept the old contract address and re-register state on the new contract

```bash
# Deploy the migration contract
stellar contract deploy \
  --network testnet \
  --source $ADMIN_KEY \
  --wasm migration_contract.wasm

export MIGRATION_CONTRACT=<deployed-migration-contract-id>

# Initialize the migration
stellar contract invoke \
  --id $MIGRATION_CONTRACT \
  --network testnet \
  --source $ADMIN_KEY \
  -- \
  initialize \
  --old_contract $CONTRACT \
  --new_contract <NEW_CONTRACT_ADDRESS>
```

### Step 6 — Execute migration meter by meter

```bash
for i in $(seq 1 $METER_COUNT); do
  stellar contract invoke \
    --id $MIGRATION_CONTRACT \
    --network testnet \
    --source $ADMIN_KEY \
    -- \
    migrate_meter \
    --meter_id $i
  echo "Migrated meter $i"
done
```

### Step 7 — Verify migrated state

For each meter, compare the balance and key fields between the state dump and the new contract:

```bash
for i in $(seq 1 $METER_COUNT); do
  stellar contract invoke \
    --id <NEW_CONTRACT_ADDRESS> \
    --network testnet \
    -- \
    get_meter \
    --meter_id $i > migration_state/new_meter_$i.json

  # Diff the old and new state (balance, user, provider must match)
  diff <(jq '{balance,user,provider}' migration_state/meter_$i.json) \
       <(jq '{balance,user,provider}' migration_state/new_meter_$i.json)
done
echo "Verification complete"
```

**Do not decommission the old contract until all diffs are clean.**

### Step 8 — Transfer token balances

Token balances held by the old contract must be transferred to the new contract. This requires a separate token transfer transaction authorized by the old contract's admin:

```bash
# Transfer the full token balance from old contract to new contract
stellar contract invoke \
  --id <TOKEN_CONTRACT_ADDRESS> \
  --network testnet \
  --source $ADMIN_KEY \
  -- \
  transfer \
  --from $CONTRACT \
  --to <NEW_CONTRACT_ADDRESS> \
  --amount <TOTAL_BALANCE>
```

---

## 7. Scenario E — Multi-Sig Withdrawal Freeze

**Trigger:** A suspicious large withdrawal request is detected, a finance wallet is compromised, or a request was submitted with incorrect parameters.

### Understand the multi-sig lifecycle

```
propose_multisig_withdrawal  →  approve_multisig_withdrawal (×N)  →  execute_multisig_withdrawal
                                         ↕
                              revoke_multisig_approval (undo one approval)
                                         ↕
                              cancel_multisig_withdrawal (cancel entire request)
```

A request expires after `WITHDRAWAL_REQUEST_EXPIRY` seconds. Expired requests cannot be executed.

### Check pending withdrawal requests

```bash
# Get total request count for a provider
stellar contract invoke \
  --id $CONTRACT \
  --network testnet \
  -- \
  get_withdrawal_request_count \
  --provider <PROVIDER_ADDRESS>

# Inspect a specific request via block explorer events (MSigProp, MSigAppr)
# https://stellar.expert/explorer/testnet/contract/$CONTRACT
```

### Cancel a pending withdrawal (Provider key)

```bash
stellar contract invoke \
  --id $CONTRACT \
  --network testnet \
  --source $PROVIDER_KEY \
  -- \
  cancel_multisig_withdrawal \
  --provider <PROVIDER_ADDRESS> \
  --request_id <REQUEST_ID>
```

### Revoke an individual approval (Finance wallet key)

If a finance wallet was compromised and already approved a request:

```bash
stellar contract invoke \
  --id $CONTRACT \
  --network testnet \
  --source <COMPROMISED_FINANCE_KEY> \
  -- \
  revoke_multisig_approval \
  --provider <PROVIDER_ADDRESS> \
  --request_id <REQUEST_ID>
```

After revoking, the approval count drops below the threshold and the request cannot be executed until re-approved.

### Reconfigure multi-sig after a wallet compromise (Provider key)

```bash
# Step 1: Disable the current multi-sig config
stellar contract invoke \
  --id $CONTRACT \
  --network testnet \
  --source $PROVIDER_KEY \
  -- \
  disable_multisig \
  --provider <PROVIDER_ADDRESS>

# Step 2: Re-configure with new wallet set (replace compromised wallet)
stellar contract invoke \
  --id $CONTRACT \
  --network testnet \
  --source $PROVIDER_KEY \
  -- \
  configure_multisig_withdrawal \
  --provider <PROVIDER_ADDRESS> \
  --finance_wallets '["<WALLET_1>","<WALLET_2>","<WALLET_3>","<WALLET_4>","<WALLET_5>"]' \
  --required_signatures 3 \
  --threshold_amount 100000
```

> **Note:** `configure_multisig_withdrawal` will revert if a config already exists and `is_active = true`. You must call `disable_multisig` first.

### Multi-sig signer duties during a freeze

See [Section 14 — Multi-Sig Signer Reference Card](#14-multi-sig-signer-reference-card) for the complete step-by-step guide for finance wallet holders.

---

## 8. Scenario F — Legal Freeze

**Trigger:** Regulatory order, court injunction, AML/KYC flag, or law enforcement request.

### Freeze a meter (Compliance Officer key)

```bash
stellar contract invoke \
  --id $CONTRACT \
  --network testnet \
  --source <COMPLIANCE_OFFICER_KEY> \
  -- \
  legal_freeze \
  --meter_id <METER_ID> \
  --reason "Regulatory order #<CASE_NUMBER> — <JURISDICTION>"
```

Funds are transferred to the `LegalVault` address. The meter is paused immediately. The `LegalFreeze` struct is stored at `DataKey::LegalFreeze(meter_id)`.

### Verify the freeze was applied

```bash
stellar contract invoke \
  --id $CONTRACT \
  --network testnet \
  -- \
  get_legal_freeze \
  --meter_id <METER_ID>
```

Confirm `is_released = false` and `frozen_amount` matches expectations.

### Release a freeze (Compliance Council — minimum 2 signatures)

Both council members must coordinate off-chain before submitting. The transaction requires `require_auth` from each address in `council_signatures`.

```bash
stellar contract invoke \
  --id $CONTRACT \
  --network testnet \
  --source <COUNCIL_MEMBER_1_KEY> \
  -- \
  release_legal_freeze \
  --meter_id <METER_ID> \
  --council_signatures '["<COUNCIL_ADDR_1>","<COUNCIL_ADDR_2>"]'
```

Funds are returned from the `LegalVault` to the meter's user. The meter is unpaused.

### Update the compliance officer (Admin key)

If the compliance officer role needs to be rotated:

```bash
stellar contract invoke \
  --id $CONTRACT \
  --network testnet \
  --source $ADMIN_KEY \
  -- \
  set_compliance_officer \
  --officer <NEW_COMPLIANCE_OFFICER_ADDRESS>
```

---

## 9. Scenario G — Gas Buffer Exhaustion

**Trigger:** Provider withdrawals are failing due to network congestion and the gas buffer is depleted or below the minimum threshold (100 XLM).

### Check current gas buffer balance

```bash
stellar contract invoke \
  --id $CONTRACT \
  --network testnet \
  -- \
  get_gas_buffer_balance \
  --provider <PROVIDER_ADDRESS>
```

### Check full gas buffer details

```bash
stellar contract invoke \
  --id $CONTRACT \
  --network testnet \
  -- \
  get_gas_buffer \
  --provider <PROVIDER_ADDRESS>
```

### Top up the gas buffer (Provider key)

```bash
stellar contract invoke \
  --id $CONTRACT \
  --network testnet \
  --source $PROVIDER_KEY \
  -- \
  top_up_gas_buffer \
  --provider <PROVIDER_ADDRESS> \
  --token <XLM_TOKEN_ADDRESS> \
  --amount 500
```

- Minimum buffer: **100 XLM**
- Maximum buffer: **10,000 XLM**
- Auto-top-up trigger threshold: **200 XLM**
- Recommended top-up during congestion: **500–1,000 XLM**

### Initialize a new gas buffer if none exists (Provider key)

```bash
stellar contract invoke \
  --id $CONTRACT \
  --network testnet \
  --source $PROVIDER_KEY \
  -- \
  initialize_gas_buffer \
  --provider <PROVIDER_ADDRESS> \
  --token <XLM_TOKEN_ADDRESS> \
  --initial_amount 500
```

### Withdraw excess buffer after congestion clears (Provider key)

```bash
# Minimum of 100 XLM must remain after withdrawal
stellar contract invoke \
  --id $CONTRACT \
  --network testnet \
  --source $PROVIDER_KEY \
  -- \
  withdraw_from_gas_buffer \
  --provider <PROVIDER_ADDRESS> \
  --token <XLM_TOKEN_ADDRESS> \
  --amount <AMOUNT_TO_WITHDRAW>
```

---

## 10. Scenario H — Admin Key Compromise

**Trigger:** The DAO Admin private key is suspected or confirmed to be compromised.

**Time budget:** Initiate the admin transfer immediately. The 48-hour timelock means you have a window — but so does the attacker.

### Step 1 — Initiate admin transfer to a new key (Current Admin key)

```bash
stellar contract invoke \
  --id $CONTRACT \
  --network testnet \
  --source $ADMIN_KEY \
  -- \
  initiate_admin_transfer \
  --proposed_admin <NEW_ADMIN_ADDRESS>
```

The contract stores an `AdminTransferProposal` with a 48-hour execution window. An `AdminXfer` event is emitted.

### Step 2 — Announce to the DAO

Post to the governance forum immediately with:
- The new admin address
- Reason for the transfer
- Veto instructions (users can call `veto_admin_transfer` if they object)

### Step 3 — Execute the transfer after 48 hours (Current Admin key)

```bash
stellar contract invoke \
  --id $CONTRACT \
  --network testnet \
  --source $ADMIN_KEY \
  -- \
  execute_admin_transfer
```

The transfer is blocked if the veto count reaches the threshold (10% of active users). If vetoed, coordinate with the DAO to resolve the dispute before retrying.

### Step 4 — Rotate all dependent keys

After the admin transfer, rotate:
- Compliance Officer (`set_compliance_officer`)
- Oracle address (`set_oracle`)
- Any finance wallets that shared infrastructure with the compromised key

### If the attacker acts first

If the attacker uses the compromised key to initiate their own admin transfer:

1. Mobilize the DAO to call `veto_admin_transfer` immediately — 10% of active users vetoing will block the transfer.
2. Simultaneously, if the attacker has not yet changed the admin, use the legitimate key to cancel by initiating a competing transfer.
3. Contact Stellar Foundation Security (see [Section 15](#15-contact-tree)).

---

## 11. Scenario I — Oracle Failure

**Trigger:** The price oracle is returning stale data, returning zero, or is unreachable, causing USD/XLM conversions to fail or produce incorrect billing amounts.

### Symptoms

- `top_up` or `withdraw_earnings` calls reverting with `OracleNotSet` or `PriceConversionFailed`
- Billing amounts that are orders of magnitude too high or too low
- `get_current_rate` returning `None`

### Check oracle status

```bash
stellar contract invoke \
  --id $CONTRACT \
  --network testnet \
  -- \
  get_current_rate
```

If this returns `None`, the oracle address is not set. If it returns stale data, check the `last_updated` field in the `PriceData` struct.

### Update the oracle address (Admin key)

```bash
stellar contract invoke \
  --id $CONTRACT \
  --network testnet \
  --source $ADMIN_KEY \
  -- \
  set_oracle \
  --oracle_address <NEW_ORACLE_CONTRACT_ADDRESS>
```

### Resolve pending challenges caused by oracle failure

If meters were challenged due to incorrect billing from bad oracle data, resolve them after the oracle is fixed:

```bash
stellar contract invoke \
  --id $CONTRACT \
  --network testnet \
  --source <ORACLE_KEY> \
  -- \
  resolve_challenge \
  --meter_id <METER_ID> \
  --restored true
```

---

## 12. Scenario J — Velocity Limit Breach / Flash Drain

**Trigger:** The velocity limit circuit breaker fires, blocking legitimate withdrawals, or a flash drain is detected that is consuming the daily withdrawal allowance.

### Check current velocity configuration

```bash
stellar contract invoke \
  --id $CONTRACT \
  --network testnet \
  -- \
  get_velocity_limits
```

### Apply a temporary override for a legitimate high-value withdrawal (Admin key)

```bash
stellar contract invoke \
  --id $CONTRACT \
  --network testnet \
  --source $ADMIN_KEY \
  -- \
  apply_velocity_override \
  --admin <ADMIN_ADDRESS> \
  --meter_id <METER_ID> \
  --expires_at <UNIX_TIMESTAMP> \
  --reason "maintenance"
```

Set `meter_id = 0` for a global override. Set `expires_at` to the minimum time needed — do not leave overrides open indefinitely.

### Tighten velocity limits during a suspected flash drain (Admin key)

```bash
stellar contract invoke \
  --id $CONTRACT \
  --network testnet \
  --source $ADMIN_KEY \
  -- \
  set_velocity_limit_config \
  --admin <ADMIN_ADDRESS> \
  --global_limit 100000 \
  --per_stream_limit 10000 \
  --is_enabled true
```

### Revoke an override after the incident (Admin key)

```bash
stellar contract invoke \
  --id $CONTRACT \
  --network testnet \
  --source $ADMIN_KEY \
  -- \
  revoke_velocity_override \
  --admin <ADMIN_ADDRESS> \
  --meter_id <METER_ID>
```

---

## 13. Scenario N — Kafka Consumer Group Lag Spike

**Trigger:** `Group Lag Alert` (Warning/Critical) webhook triggered, or total consumer lag rises past the configured threshold (e.g. 600 or 1000 messages) but the consumer group is unable to reduce queue backlog.

**Goal:** Resume high throughput processing to maintain billing, settlement, and telemetry streams under the P99 latency target (< 100ms).

### Step 1 — Check Cluster Topology and Current Lag
Review the current partition lag on the Dashboard's **Kafka Lag & Auto-Scaler** tab. Check if a single partition is bottlenecked (indicates partition skew or consumer thread starvation) or if all partitions have rising lag (indicates overall consumer group capacity deficit).

### Step 2 — Verify Active Consumer Count
Confirm that `activeConsumers` matches the scaling target. If the monitor scaled up to `MAX_CONSUMERS` but the queue continues to grow, execute Step 3.

### Step 3 — Dynamically Adjust Auto-Scaling Parameters (Grid Admin key)
Use the administrative config override on the dashboard or invoke the configuration override endpoint to temporarily lift max consumer bounds and partition limits:
```bash
# Temporarily double max allowed consumers and raise scale thresholds
# This increases parallelism to flush the queue backlog
stellar contract invoke \
  --id $CONTRACT \
  --network testnet \
  --source $GRID_ADMIN_KEY \
  -- \
  override_kafka_scaler_config \
  --max_consumers 16 \
  --target_lag_per_consumer 250
```

### Step 4 — Check for Unhealthy or Locked Consumers
If some partitions are completely starved of commits:
1. Restart stale consumer pods to trigger a partition rebalance.
2. Monitor the dynamic event logs on the dashboard to verify the `REBALANCE` completes within the 3-second recovery window.

---

## 14. Scenario O — Kafka Auto-Scaling Actuator Failure

**Trigger:** `LIMIT_REACHED` or scale-up commands are formulated by the controller but the underlying container orchestrator (K8s HPA, Docker API, or KEDA) fails to spawn new consumer instances due to resource exhaustion or permission errors.

**Goal:** Fallback to safe limits, isolate non-critical ingestion pipelines, and protect critical utility metering billing paths.

### Step 1 — Verify Actuator Connection
Verify if the scaling API can receive Bearer-token authorized commands. If the orchestrator is throwing `401 Unauthorized` or `503 Service Unavailable`, proceed with recovery.

### Step 2 — Fallback to Manual Provisioning
If the auto-scaler controller remains blocked by cooldown or actuator failures, manually spawn consumer instances on local servers:
```bash
# Force start backup consumer group instances directly
docker compose up -d --scale billing-consumer=6
```

### Step 3 — Divert Non-critical streams
If resource limits prevent spinning up new consumers, temporarily pause non-critical consumption streams (e.g., carbon-grant matching streams) to direct all available processing power to prepaid/postpaid billing settlement engines:
```bash
# Temporarily pause green-grant topics to relieve consumer memory pressure
kafka-configs --bootstrap-server localhost:9092 --entity-type topics --entity-name green-grant-events --alter --add-config cleanup.policy=compact
```

---

## 15. Post-Incident Procedures

Complete these steps for every incident, regardless of severity.

### 1. Preserve evidence

Export all relevant transaction hashes, ledger numbers, and event logs from the block explorer before they age out of the horizon. Save to a timestamped file:

```bash
# Example: export events for the contract from the block explorer API
curl "https://horizon-testnet.stellar.org/accounts/$CONTRACT/transactions?limit=200&order=desc" \
  > incident_$(date +%Y%m%d_%H%M%S)_transactions.json
```

### 2. Resolve open challenges

After the incident is contained, the Oracle must resolve any meters left in `is_disputed = true`:

```bash
stellar contract invoke \
  --id $CONTRACT \
  --network testnet \
  --source <ORACLE_KEY> \
  -- \
  resolve_challenge \
  --meter_id <METER_ID> \
  --restored true   # or false if service was not restored
```

### 3. Resume paused meters

Once the all-clear is given, unpause each affected meter:

```bash
for i in $(seq 1 $METER_COUNT); do
  stellar contract invoke \
    --id $CONTRACT \
    --network testnet \
    --source $PROVIDER_KEY \
    -- \
    set_meter_pause \
    --meter_id $i \
    --paused false
  echo "Resumed meter $i"
done
```

### 4. Disable emergency velocity limits

```bash
stellar contract invoke \
  --id $CONTRACT \
  --network testnet \
  --source $ADMIN_KEY \
  -- \
  set_velocity_limit_config \
  --admin <ADMIN_ADDRESS> \
  --global_limit 1000000000 \
  --per_stream_limit 100000000 \
  --is_enabled false
```

### 5. Publish a post-mortem

The DAO Admin must publish a post-mortem to the governance forum **within 72 hours**. Include:

- Incident timeline (UTC timestamps)
- Root cause analysis
- Funds at risk and funds recovered
- Actions taken and by whom
- Remediation steps and timeline
- Changes to this runbook

### 6. Rotate compromised keys

If any signing key was exposed, initiate an admin transfer with the 48-hour timelock (see [Scenario H](#10-scenario-h--admin-key-compromise)).

### 7. Update this runbook

If any procedure was unclear, missing, or failed, update this document and submit a PR before closing the incident ticket.

---

## 14. Multi-Sig Signer Reference Card

This section is written for **Finance Wallet holders** who may not be familiar with the full contract. Print this section and keep it accessible offline.

### Your role

You are one of 3–5 authorized Finance Department wallet holders for your provider. Large withdrawals (above `threshold_amount` in USD cents) require `required_signatures` approvals from this group before they can execute. Your job is to:

1. Verify that a withdrawal request is legitimate before approving it.
2. Revoke your approval immediately if you suspect fraud.
3. Cancel the request if you are the provider and the request is fraudulent.

### Before approving any request — verification checklist

- [ ] You received the request notification through the agreed secure channel (not email alone).
- [ ] The `amount_usd_cents` matches the amount discussed off-chain.
- [ ] The `destination` address is the known treasury address — verify character by character.
- [ ] The `meter_id` is a meter you recognize as belonging to your provider.
- [ ] The `expires_at` timestamp gives you enough time to coordinate with other signers.
- [ ] At least one other signer has independently verified the above.

**If any item is unchecked, do not approve. Contact the DAO Admin immediately.**

### Approve a withdrawal request

```bash
stellar contract invoke \
  --id $CONTRACT \
  --network testnet \
  --source <YOUR_FINANCE_WALLET_KEY> \
  -- \
  approve_multisig_withdrawal \
  --provider <PROVIDER_ADDRESS> \
  --request_id <REQUEST_ID>
```

### Revoke your approval (if you approved in error or suspect fraud)

```bash
stellar contract invoke \
  --id $CONTRACT \
  --network testnet \
  --source <YOUR_FINANCE_WALLET_KEY> \
  -- \
  revoke_multisig_approval \
  --provider <PROVIDER_ADDRESS> \
  --request_id <REQUEST_ID>
```

Revoking drops the approval count. If it falls below `required_signatures`, the request cannot execute until re-approved.

### Cancel the entire request (Provider key only)

Only the provider key can cancel. If you are the provider:

```bash
stellar contract invoke \
  --id $CONTRACT \
  --network testnet \
  --source $PROVIDER_KEY \
  -- \
  cancel_multisig_withdrawal \
  --provider <PROVIDER_ADDRESS> \
  --request_id <REQUEST_ID>
```

### Execute an approved request (after quorum is reached)

Once `approval_count >= required_signatures`, any party can trigger execution:

```bash
stellar contract invoke \
  --id $CONTRACT \
  --network testnet \
  --source <YOUR_FINANCE_WALLET_KEY> \
  -- \
  execute_multisig_withdrawal \
  --provider <PROVIDER_ADDRESS> \
  --request_id <REQUEST_ID>
```

### Key constants

| Constant | Value | Meaning |
|---|---|---|
| `MIN_FINANCE_WALLETS` | 3 | Minimum wallets in a multi-sig config |
| `MAX_FINANCE_WALLETS` | 5 | Maximum wallets in a multi-sig config |
| `WITHDRAWAL_REQUEST_EXPIRY` | See contract | Seconds before a request auto-expires |
| `threshold_amount` | Configured per provider | USD cents below which multi-sig is not required |

### What to do if your wallet is compromised

1. **Immediately** call `revoke_multisig_approval` for any pending requests your key has approved.
2. Contact the DAO Admin and other finance wallet holders via the emergency Signal group.
3. The provider must call `disable_multisig` and then `configure_multisig_withdrawal` with a replacement wallet.
4. Do not use the compromised key for any other purpose.

---

## 15. Contact Tree

| Priority | Role | Contact Method |
|---|---|---|
| 1 | DAO Admin | Signal / PagerDuty (primary) |
| 2 | Finance Wallet Holders (×3–5) | Signal group |
| 3 | Compliance Officer | Signal + Email |
| 4 | Oracle Operator | PagerDuty |
| 5 | Stellar Foundation Security | security@stellar.org |

> **Fill in actual names, handles, and contact details before deploying to mainnet. This table is a template.**

### Escalation thresholds

| Severity | Criteria | Response time | Escalate to |
|---|---|---|---|
| **P1 — Critical** | Active exploit, funds draining | < 5 minutes | All roles simultaneously |
| **P2 — High** | Suspected exploit, oracle down, key compromise | < 15 minutes | DAO Admin + Finance Wallets |
| **P3 — Medium** | Planned pause, upgrade, legal freeze | < 1 hour | DAO Admin |
| **P4 — Low** | Gas buffer low, velocity limit false positive | < 4 hours | Provider |

---

*This runbook covers the contract as deployed at commit `main`. Re-validate all commands after any Wasm upgrade. Last reviewed: 2026-04-26.*
