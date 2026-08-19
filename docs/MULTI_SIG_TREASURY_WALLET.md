# Multi-Signature Treasury Wallet

## Overview

The protocol treasury previously relied on a single-signature wallet, creating a
single point of failure for high-value operations. The
`contracts/treasury-wallet` contract implements a standalone **M-of-N
multi-signature wallet** that holds protocol treasury funds. Executing a
transfer requires a configurable quorum of signers, and transfers at or above a
configured amount are additionally **time-locked** so that even a fully
compromised quorum cannot move funds instantly.

## Key Features

### 1. Configurable Threshold (M-of-N)
- Wallets support **2–7 signers** with a configurable required quorum.
- The threshold must be a strict majority (and never less than 2), so a single
  key can never move treasury funds. A 3-of-5 wallet is the recommended default.
- Threshold, high-value bound, timelock, and expiry can be updated by the owner
  (typically a governance DAO address).

### 2. Signer Management
- The **owner** (set at initialization) can add or remove signers.
- Adding a signer is rejected once the 7-signer cap is reached or the signer is
  already present.
- Removing a signer is rejected if it would leave fewer than 2 signers or make
  the current threshold unsatisfiable.

### 3. Transaction Proposal & Approval Workflow
- Any **signer** can `submit_transaction(token, to, amount)`; the proposer
  becomes the implicit first approver.
- Other signers add their approval with `approve_transaction`.
- Approvals can be **revoked**; dropping below the quorum disarms any armed
  timelock (re-approving re-arms it from scratch).
- The owner or the proposer can **cancel** a pending proposal.

### 4. Time-Locked Execution for Large Amounts
- Transfers at or above `high_value_threshold` cannot execute immediately:
  the clock starts when the quorum is first reached and the transfer becomes
  executable only after `timelock_seconds` have elapsed.
- Transfers below the threshold execute immediately once the quorum is met.
- Proposals expire after `expiry_seconds` if not executed.

## Architecture

### Core Components

```
TreasuryWalletContract
├── TreasuryConfig      # owner, signers, required quorum, thresholds, timelock
├── TreasuryProposal    # per-transfer proposal: recipient, amount, approvals, timelock state
├── DataKey             # Config, ProposalCount, Proposal(id), Approval(id, signer)
└── Events              # typed #[contractevent] events for indexers
```

### Security Properties

- **Quorum enforcement** — execution requires `approval_count >= required`,
  recomputed from stored approvals at execution time.
- **Single-approval limit** — each signer can approve a proposal at most once.
- **Timelock** — high-value transfers wait `timelock_seconds` after the quorum
  is reached; revocation resets the clock.
- **Expiry** — stale proposals cannot be approved or executed after
  `expiry_seconds`.
- **Balance guard** — execution fails cleanly if the wallet lacks the tokens.
- **Owner-gated config** — signer and threshold changes require the owner.

## Deployment

```bash
cd contracts
cargo build -p treasury-wallet --target wasm32-unknown-unknown --release
```

Initialize with the owner, the initial signer set, the quorum, and the
high-value/timelock/expiry parameters:

```
initialize(owner, signers, required_signatures, high_value_threshold,
           timelock_seconds, expiry_seconds)
```

## Testing

```bash
cd contracts
cargo test -p treasury-wallet
```

The test suite (47 tests) covers initialization bounds, duplicate/unauthorized
signer management, proposal lifecycle (submit/approve/revoke/execute/cancel),
quorum enforcement, expiry, and the high-value timelock path including the
revoke-then-re-approve re-arming scenario.
