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
- **Multi-Signature Treasury Wallet** — Standalone M-of-N wallet (`contracts/treasury-wallet`) for the protocol treasury with signer add/remove, transaction proposal & approval workflow, and time-locked execution for high-value transfers
- **Emergency Response** — Circuit breakers, legal freezes, velocity limits, protocol pauses
- **Dust Sweeper** — Prunes fractional remainders from depleted streams
- **Grant Stream** — Conservation goals trigger automatic grant matching
- **Scheduled Backup Verification** — Restore-tested database backups with metrics, alerts, and canary rollout guidance
- **Oracle Aggregation Framework** — Multi-provider oracle aggregation with a Chainlink `AggregatorV3Interface` adapter, median consensus, deviation/staleness validation, graceful fallback, and per-provider health monitoring (`contracts/oracle-aggregator`)
- **Multi-Region Replication and Disaster Recovery** — Active-passive cross-region replication (us-east-1 → eu-west-1 → ap-southeast-1) with RPO ≤ 60s, RTO ≤ 5 min, automated health monitoring, blue-green canary promotion, and scheduled DR validation tests (`docs/MULTI_REGION_DR_ARCHITECTURE.md`)

## Project Structure

```
Utility-contracts/
├── contracts/
│   ├── Cargo.toml                  # Workspace root
│   ├── utility_contracts/          # Main contract
│   │   ├── src/lib.rs              # Core implementation
│   │   ├── src/test.rs             # Test suite
│   │   └── Cargo.toml
│   ├── price_oracle/               # Price oracle contract
│   └── treasury-wallet/            # Multi-signature treasury wallet (M-of-N, timelock)
├── webhook-delivery-service/       # High-performance off-chain Webhook service with retry & SSRF shielding (TS)
├── meter-simulator/                # Device simulator (JS)
├── usage-dashboard/                # Real-time Next.js analytics & Webhook monitor dashboard
├── docs/                           # Architecture, deployment and operational runbooks
├── examples/                       # Usage examples
├── scripts/                        # Deployment scripts
├── .github/workflows/ci.yml        # CI pipeline
├── SECURITY.md                     # Security policy & formal proofs
├── CONTRIBUTING.md                 # Contribution guidelines
└── EMERGENCY_RUNBOOK.md            # Emergency procedures
```

### Webhook Delivery Service

An enterprise-grade, high-performance off-chain delivery daemon for real-time Soroban alerts (e.g. `LowBalanceAlert`, device tampers).
- **Performance**: `< 100ms` P99 ingestion latency target via an asynchronous event-driven memory queue.
- **Robust Security**: Includes HMAC-SHA256 and Ed25519 signature headers, strict replay protection windowing, and thorough SSRF IP/DNS blacklisting.
- **Resiliency**: Built-in exponential backoff retry schedules with full randomized jitter to survive downstream subscriber downtimes and network drops.
- **Operational Guides**: See [WEBHOOK_ARCHITECTURE.md](docs/WEBHOOK_ARCHITECTURE.md), [WEBHOOK_DEPLOYMENT.md](docs/WEBHOOK_DEPLOYMENT.md), and [WEBHOOK_RUNBOOK.md](docs/WEBHOOK_RUNBOOK.md).

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


### Chaos Engineering in Staging

Staging resilience exercises are governed by the [Chaos Engineering Testing Blueprint](docs/runbooks/chaos-engineering-staging.md). The blueprint defines approved fault scenarios, security guardrails, P99 and availability SLOs, monitoring requirements, and blue-green/canary rollout steps for chaos-enabled staging deployments.

### Multi-Region Replication and Disaster Recovery

The Utility Protocol stack operates across three regions in active-passive configuration to meet its 99.99% availability and < 100 ms P99 targets:

| Region | Role | Replication |
|---|---|---|
| `us-east-1` | Primary (active) | — |
| `eu-west-1` | Secondary (hot standby) | Synchronous PostgreSQL streaming, Kafka MirrorMaker 2 |
| `ap-southeast-1` | Tertiary (warm standby) | Async PostgreSQL streaming, Kafka MirrorMaker 2 |

**Recovery targets:**
- **RPO:** ≤ 60 seconds (maximum data loss on failover)
- **RTO:** ≤ 5 minutes (time to restore service after region failure)

**Key components:**
- `meter-simulator/src/multi-region-replication.js` — Replication state tracking, health monitoring, failover orchestration
- `meter-simulator/src/dr-health-checker.js` — Cross-region health probes and failover readiness reports
- `meter-simulator/src/dr-canary-analyzer.js` — Canary promotion decisions (PROMOTE / HOLD / ROLLBACK)
- `scripts/dr-failover.sh` — Controlled DR failover with dry-run mode and Prometheus metrics
- `scripts/dr-test.sh` — DR validation test runner (connectivity, replication-lag, rto-validation, rpo-validation)
- `scripts/dr-canary-promote.sh` — Canary stage promotion (5% → 25% → 50% → 100%) with SLO gates
- `deploy/service-mesh/dr-blue-green.yaml` — Istio VirtualService/DestinationRule for DR-aware blue-green routing
- `monitoring/multi-region-dr-alerts.yml` — Prometheus alert rules for replication lag, RPO/RTO, and region health
- `monitoring/multi-region-dr-dashboard.json` — Grafana dashboard for DR observability
- `usage-dashboard/src/components/MultiRegionDRPanel.tsx` — React component for DR status in the operator dashboard

See [Multi-Region DR Architecture](docs/MULTI_REGION_DR_ARCHITECTURE.md) and [DR Failover Runbook](docs/runbooks/DR_FAILOVER_RUNBOOK.md) for full details.

### Security Properties

- **Nonce sync** prevents replay attacks on IoT heartbeats
- **Multi-sig veto** for fleet-level config changes (48h staging window)
- **Carbon-credit streaming** with fractional accumulator and deferred minting
- **Auto-rent deduction** capped at 1,000 stroops per claim

## Deployment

- **Network:** Stellar Testnet
- **Contract ID:** `CB7PSJZALNWNX7NLOAM6LOEL4OJZMFPQZJMIYO522ZSACYWXTZIDEDSS`

## Development

### One-command local onboarding

Run the repository onboarding script before your first local build. It validates Git, ripgrep, Rust/Cargo, rustup, the WASM target, Node.js, and npm; installs npm dependencies for the JavaScript workspaces unless skipped; and prints the recommended validation commands.

```bash
./scripts/onboard.sh

# Validate prerequisites without installing dependencies
./scripts/onboard.sh --check-only
```

### Manual build and test commands

```bash
# Build
cd contracts && cargo build --target wasm32-unknown-unknown --release

# Test
cargo test

# Coverage (requires cargo-llvm-cov)
COVERAGE_THRESHOLD=80 scripts/coverage.sh

# Lint
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
```

## CI/CD Pipeline

The GitHub Actions workflow (`.github/workflows/ci.yml`) automatically runs on:
- **Push to main branch** - Ensures main branch is always tested
- **Pull Requests to main** - Prevents breaking changes from being merged

### Dependency Vulnerability Scanning

A dedicated GitHub Actions workflow (`.github/workflows/dependency-vulnerability-scan.yml`) runs on pull requests, pushes to `main`, a daily schedule, and manual dispatch. It blocks vulnerable dependency changes with GitHub Dependency Review, audits Rust lockfiles with `cargo audit`, audits Node.js projects with `npm audit`, and publishes a workflow summary for security review. See `docs/runbooks/DEPENDENCY_VULNERABILITY_SCANNING.md` for triage, monitoring, and rollout procedures.

### Automated Performance Regression Detection

A dedicated GitHub Actions workflow (`.github/workflows/performance-regression.yml`) benchmarks the webhook ingestion critical path on every pull request and push to `main`, compares the results against the committed baseline (`.perf/perf-baselines.json`), and fails the check when a metric regresses beyond tolerance (default 30%) or breaches the absolute **<100ms P99** ingestion SLO. Runtime Prometheus alerts (`monitoring/performance-alerts.yml`) catch post-deploy degradation. See [docs/runbooks/PERFORMANCE_REGRESSION.md](docs/runbooks/PERFORMANCE_REGRESSION.md) for the architecture and runbook.

```bash
# Run the full gate locally (build → benchmark → compare)
./scripts/perf-regression-gate.sh

# Refresh the baseline after a verified improvement
./scripts/perf-regression-gate.sh --update-baseline
```

### Testing Stages

1. **Environment Setup**: Rust toolchain with WASM target, Stellar CLI v25.1.0, dependency caching
2. **Code Quality**: `cargo fmt --all -- --check` + `cargo clippy --target wasm32-unknown-unknown -- -D warnings`
3. **Build**: `cargo build --target wasm32-unknown-unknown --release`
4. **Unit Tests**: `cargo test` including fuzz tests
5. **Coverage Gate**: `scripts/coverage.sh` enforces the configured line coverage threshold (`COVERAGE_THRESHOLD`, default 80%) for both the root package and contracts workspace
6. **Fuzz Tests**: Auto-detection and validation of fuzz infrastructure

### Local Development

```bash
cargo fmt --all -- --check
cargo clippy --target wasm32-unknown-unknown -- -D warnings
cargo build --target wasm32-unknown-unknown --release
cargo test
COVERAGE_THRESHOLD=80 scripts/coverage.sh
```

## ZK-SNARK Circuits for Sensor Privacy

Hardware devices (meters) prove consumed energy/water amounts without revealing raw sensor readings using Groth16 proofs.

**Circuit (Circom):**
- **Private inputs**: `usage_raw`, `salt`, `last_usage`
- **Public inputs**: `units_consumed`, `is_peak_hour`, `nullifier`, `commitment`
- **Constraints**: Integrity, range proof, commitment hash (Poseidon), nullifier uniqueness

**Flow**: Device generates proof → submits via `submit_zk_usage_report` → contract verifies with BN254 host functions (`pairing_check`, `g1_add`, `g1_mul`) → nullifier checked → balance deducted.

**Optimization**: Pre-computed verification key components, optimized host functions for EC ops, no big-integer WASM arithmetic.

See [EMERGENCY_RUNBOOK.md](EMERGENCY_RUNBOOK.md) for operational procedures and [SECURITY.md](SECURITY.md) for formal verification results.

## License

By contributing, you agree that your contributions will be licensed under the same license as the project.
