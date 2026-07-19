# Utility-Protocol Contracts

Soroban smart contracts for a decentralized utility metering and streaming protocol on Stellar. Supports prepaid/postpaid billing, continuous streaming, variable-rate tariffs, gas buffers, ZK-SNARK sensor privacy, multi-sig governance, and emergency response.

## Features

- **Utility Metering** — Track energy/water consumption with precision billing
- **Prepaid & Postpaid Billing** — Both models supported
- **Continuous Streaming** — Real-time balance monitoring with buffer protection
- **Variable Rate Tariffs** — Peak/off-peak pricing (18:00–21:00 UTC at 1.5× rate)
- **Gas Buffer** — Pre-paid XLM buffer ensures withdrawals clear during network congestion
- **ZK-SNARK Privacy** — Groth16 proofs let meters prove usage without revealing raw readings
- **Firmware Update Gate** — Time-limited, cryptographically signed update authorization
- **Multi-Sig Governance** — 3-of-5 finance wallet quorum for large withdrawals
- **Emergency Response** — Circuit breakers, legal freezes, velocity limits, protocol pauses
- **Dust Sweeper** — Prunes fractional remainders from depleted streams
- **Grant Stream** — Conservation goals trigger automatic grant matching

## Project Structure

```
Utility-contracts/
├── contracts/
│   ├── Cargo.toml                  # Workspace root
│   ├── utility_contracts/          # Main contract
│   │   ├── src/lib.rs              # Core implementation
│   │   ├── src/test.rs             # Test suite
│   │   └── Cargo.toml
│   └── price_oracle/               # Price oracle contract
├── meter-simulator/                # Device simulator (JS)
├── examples/                       # Usage examples
├── scripts/                        # Deployment scripts
├── .github/workflows/ci.yml        # CI pipeline
├── SECURITY.md                     # Security policy & formal proofs
├── CONTRIBUTING.md                 # Contribution guidelines
└── EMERGENCY_RUNBOOK.md            # Emergency procedures
```

## Architecture

### Variable Rate Tariffs

Peak hours: **18:00–21:00 UTC** (1.5× off-peak rate).

```
Peak rate = off_peak_rate × 3 / 2

Example: off_peak = 10 tokens/sec
         peak     = 15 tokens/sec
```

| UTC Hour | Seconds | Status |
|----------|---------|--------|
| 00:00    | 0       | OFF-PEAK |
| 12:00    | 43,200  | OFF-PEAK |
| 18:00    | 64,800  | PEAK |
| 20:59    | 75,599  | PEAK |
| 21:00    | 75,600  | OFF-PEAK |


### Observability

The meter simulator propagates W3C Trace Context metadata in MQTT usage and heartbeat payloads, and the dashboard includes trace health indicators for the 100 ms P99 critical-path target. See [Distributed Tracing and Trace Context Propagation](docs/DISTRIBUTED_TRACING.md) for architecture, rollout, alerting, security review, and runbook guidance.

### Gas Buffer

Ensures 100% service availability during network congestion.

| Constant | Value | Description |
|----------|-------|-------------|
| `MIN_GAS_BUFFER` | 100 XLM | Minimum required buffer |
| `MAX_GAS_BUFFER` | 10,000 XLM | Maximum buffer capacity |
| `GAS_BUFFER_TOP_UP_THRESHOLD` | 200 XLM | Auto top-up trigger |

### Firmware Update Authorization Gate

Provider-initiated, device-completed firmware updates with Ed25519 signature verification and a 2-hour maximum window.

### Stream Balance Invariant (Formal Proof)

> For every active stream: `current_time ≤ start_time + ⌊initial_balance / flow_rate⌋`

Verified via 15 property tests with 100+ randomized cases each, covering pause/resume cycles, rounding direction, and overflow protection.

### Security Properties

- **Nonce sync** prevents replay attacks on IoT heartbeats
- **Multi-sig veto** for fleet-level config changes (48h staging window)
- **Carbon-credit streaming** with fractional accumulator and deferred minting
- **Auto-rent deduction** capped at 1,000 stroops per claim

## Deployment

- **Network:** Stellar Testnet
- **Contract ID:** `CB7PSJZALNWNX7NLOAM6LOEL4OJZMFPQZJMIYO522ZSACYWXTZIDEDSS`

## Development

```bash
# Build
cd contracts && cargo build --target wasm32-unknown-unknown --release

# Test
cargo test

# Lint
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
```

## CI/CD Pipeline

The GitHub Actions workflow (`.github/workflows/ci.yml`) automatically runs on:
- **Push to main branch** - Ensures main branch is always tested
- **Pull Requests to main** - Prevents breaking changes from being merged

### Testing Stages

1. **Environment Setup**: Rust toolchain with WASM target, Stellar CLI v25.1.0, dependency caching
2. **Code Quality**: `cargo fmt --all -- --check` + `cargo clippy --target wasm32-unknown-unknown -- -D warnings`
3. **Build**: `cargo build --target wasm32-unknown-unknown --release`
4. **Unit Tests**: `cargo test` including fuzz tests
5. **Performance Regression Gate**: Critical-path P99 snapshots are checked against `.github/performance/baseline.json` with a 100ms hard SLO and 10% regression budget
6. **Fuzz Tests**: Auto-detection and validation of fuzz infrastructure

### Local Development

```bash
cargo fmt --all -- --check
cargo clippy --target wasm32-unknown-unknown -- -D warnings
cargo build --target wasm32-unknown-unknown --release
cargo test
python3 scripts/performance_regression_gate.py --baseline .github/performance/baseline.json --current .github/performance/baseline.json --max-p99-ms 100 --regression-percent 10
```

## ZK-SNARK Circuits for Sensor Privacy

Hardware devices (meters) prove consumed energy/water amounts without revealing raw sensor readings using Groth16 proofs.

**Circuit (Circom):**
- **Private inputs**: `usage_raw`, `salt`, `last_usage`
- **Public inputs**: `units_consumed`, `is_peak_hour`, `nullifier`, `commitment`
- **Constraints**: Integrity, range proof, commitment hash (Poseidon), nullifier uniqueness

**Flow**: Device generates proof → submits via `submit_zk_usage_report` → contract verifies with BN254 host functions (`pairing_check`, `g1_add`, `g1_mul`) → nullifier checked → balance deducted.

**Optimization**: Pre-computed verification key components, optimized host functions for EC ops, no big-integer WASM arithmetic.

See [docs/PERFORMANCE_REGRESSION_CI.md](docs/PERFORMANCE_REGRESSION_CI.md) for the CI performance gate and [EMERGENCY_RUNBOOK.md](EMERGENCY_RUNBOOK.md) for operational procedures and [SECURITY.md](SECURITY.md) for formal verification results.

## License

By contributing, you agree that your contributions will be licensed under the same license as the project.
