# Graceful Degradation & Capacity Shedding Runbook

This document outlines the architecture, implementation, and operational procedures for system-wide Graceful Degradation and Capacity Shedding.

## 1. Solution Architecture & Design

### 1.1 Overview
Under normal operations, the Utility-Protocol system runs all features including high-frequency dashboard telemetry, complex capacity forecasting, and expensive cryptographic validations. However, under high load or network congestion, the system must protect its core path (critical billing and liveness reporting) to maintain its **99.99% availability target** and **< 100ms P99 latency** for critical paths.

We employ a layered **Feature Flag and Capacity Shedding state machine** across both our smart contracts (on-chain) and our Next.js dashboard (off-chain/client-side).

```
   System Load / Latency Spike
              │
              ▼
   ┌────────────────────────────────────────────────────────┐
   │  Capacity Shedding State Machine                       │
   │  - LEVEL 0 (NORMAL): All features active               │
   │  - LEVEL 1 (MODERATE): Slow refresh, cached forecasts   │
   │  - LEVEL 2 (HIGH): Disable heavy graphs, restrict non-critical paths
   │  - LEVEL 3 (CRITICAL): Bypass ZK proofs, pause sweeper │
   └────────────────────────────────────────────────────────┘
```

---

## 2. Dynamic Degradation Tiers

We define four dynamic degradation tiers based on simulated system load and network P99 latency:

| Tier | Name | Triggers (Load / Latency) | Degraded/Shed Features | Target SLA |
| :--- | :--- | :--- | :--- | :--- |
| **Level 0** | **NORMAL** | Load < 80% AND Latency < 100ms | None. Full features (1s polling, interactive graphs, full ZK verification, dust sweeping). | 100% features, < 10ms latency |
| **Level 1** | **MODERATE** | Load >= 80% OR Latency >= 100ms | • Reduce dashboard polling rate (5s -> 15s)<br>• Serve cached capacity forecasts instead of recalculating. | < 50ms latency, 99.99% uptime |
| **Level 2** | **HIGH** | Load >= 90% OR Latency >= 200ms | • Level 1 restrictions<br>• Disable heavy charts/animations<br>• Restrict new postpaid stream creations. | < 80ms latency, 99.99% uptime |
| **Level 3** | **CRITICAL** | Load >= 95% OR Latency >= 500ms | • Level 1 & 2 restrictions<br>• Bypass expensive ZK-SNARK verifications (switch to optimistic / merkle verification)<br>• Suspend dust sweeper background jobs<br>• Shed 75% of non-essential device reporting frequency. | < 100ms latency, 99.99% uptime |

---

## 3. On-Chain Smart Contract Logic

In Rust (`contracts/common/src/graceful_degradation.rs`), we implement:
- `DegradationConfig`: Holds feature flags, active shedding level, and capacity constraints (e.g. `max_active_streams`).
- `is_feature_enabled`: A helper to check if a specific operational feature (e.g., `Symbol::new(env, "dust_sweeper")`) is currently authorized.
- `check_capacity`: Validates whether the active stream count violates the capacity limit.
- Under Level 3 (CRITICAL), the contract permits an optimistic path to verify meter usage report submissions to keep critical execution paths extremely fast and low-gas.

---

## 4. Frontend Next.js Dashboard Implementation

Inside `usage-dashboard/`, we implement:
1. **State Machine (`gracefulDegradation.ts`)**: Pure, optimized logic to map simulated load/latency to shedding tiers and active flags, calculated in < 1ms.
2. **Interactive Controls (`DegradationPanel.tsx`)**:
   - **System Load Slider** (0% to 150%) and **Network Latency Slider** (10ms to 500ms) to simulate real-world storms.
   - **Manual Feature Override Toggles** for developers to force-degrade individual paths.
   - **Live P99 Latency Gauge** showing how shedding actively bounds latency to keep critical paths under 100ms.
   - **Active Alert Feed** broadcasting status changes.
   - **Availability Gauge** displaying simulated uptime (guaranteeing 99.99% uptime when shedding is enabled, and illustrating how uptime collapses if shedding is bypassed under load).

---

## 5. Deployment & Rollout Strategy

To safely deploy feature flags and capacity shedding policies without risk of regressions:

### 5.1 Blue-Green Strategy
1. **Stage 1 (Blue Only)**: Live production runs stable code.
2. **Stage 2 (Green Rollout)**: Deploy the new dashboard and contract code containing Graceful Degradation mechanisms to the Green environment. Set initial `shedding_level = 0` (NORMAL) to avoid premature shedding.
3. **Stage 3 (Validation)**: Run simulated load spikes on Green and verify that:
   - Feature flags correctly activate/deactivate.
   - Verification times for critical paths remain < 100ms.
4. **Stage 4 (Cutover)**: Re-route 100% of traffic to the Green environment. Keep Blue environment online for 24 hours as a hot standby.

### 5.2 Canary Analysis
During rollout, split traffic 95/5 (95% to Blue, 5% to Green):
- **Canary Metrics**: Check Green for increased transaction reverts or frontend loading errors.
- **Latency Verification**: Ensure Green's P99 latency on the critical billing path is strictly lower than Blue's under simulated heavy load.
- **Alert Sync**: Verify that canary notifications and active capacity alert feeds match expectation.

---

## 6. Operational Runbook & Troubleshooting

### Scenario A: Manual Override of a Shed Feature
*Problem*: A critical system is at Capacity Shedding Level 2, disabling non-essential stream creations, but an enterprise partner requires immediate onboarding.
*Action*:
1. Open the Operational Control Panel on the Dashboard.
2. Force the `POSTPAID_STREAMS` feature flag to `ENABLED` manually via the toggle override.
3. Once the onboarding is complete, restore the flag to `AUTO` to resume automatic shedding protection.

### Scenario B: Emergency Shedding Activation (On-Chain)
*Problem*: Network congestion on Stellar causes transaction inclusion latency to exceed 500ms.
*Action*: Run the following CLI command to immediately force Level 3 (CRITICAL) shedding, bypassing on-chain ZK verification in favor of optimistic billing to keep latency fast:
```bash
stellar contract invoke \
  --id $CONTRACT \
  --network testnet \
  --source $ADMIN_KEY \
  -- \
  set_degradation_config \
  --shedding_level 3 \
  --is_zk_bypass_allowed true \
  --is_dust_sweeper_suspended true
```
