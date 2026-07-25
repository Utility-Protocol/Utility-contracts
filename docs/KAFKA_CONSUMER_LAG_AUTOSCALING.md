# Kafka Consumer Lag Monitoring & Auto-Scaling Architecture

This document defines the system-wide architecture, design specifications, technical bounds, deployment strategies, and operational runbooks for the decentralized utility metering platform's **Kafka Consumer Lag Monitoring and Auto-Scaling Consumer Groups** system.

---

## 1. Overview & Problem Statement

In a decentralized utility streaming protocol, IoT devices (meters) submit high-frequency consumption reports. These messages are ingested into Apache Kafka topics for stream processing (e.g., balance calculations, rate applications, billing settlements).

If consumer instances (consumer groups) process messages slower than the ingestion rate, **consumer lag** grows. Unchecked lag delays critical paths (such as emergency circuit breaking or real-time credit-depletion stream halts), violating the platform's SLAs.

This system provides:
1. **Real-time Lag Monitoring:** Dynamic retrieval and calculation of Log End Offsets (LEO) and Committed Offsets per partition.
2. **Auto-Scaling Consumer Engine:** Dynamic adjustment of consumer instance counts in consumer groups to automatically match load spikes while avoiding resource thrashing.
3. **Proactive Alerting & Visualization:** Rich visual dashboards, low-latency metrics retrieval, and webhook-based alerting.

---

## 2. Technical Bounds & Targets

### 2.1 Performance Target: < 100ms P99 Latency
- **Computation Overhead:** Retrieval of broker metadata (LEO) and committed offsets must resolve in `< 50ms` on average. The auto-scaling decision pipeline must evaluate the scaling algorithm within `< 10ms`.
- **P99 Critical Path:** Total latency for offset checking, lag calculation, auto-scaling decision formulation, and dispatch of actuator signals must remain **under 100ms for 99% of all polling cycles**.
- **Non-blocking Loop:** The monitoring agent runs asynchronously to prevent blocking the data processing pipeline.

### 2.2 Availability Target: 99.99% Uptime
- **Stateless Monitors:** The lag monitors are fully stateless and run in a highly-available, active-passive or active-active consensus model (via Raft/ZooKeeper or Kubernetes Leader Election).
- **Graceful Degradation:** If the monitoring system is temporarily unreachable, consumer groups remain at their last stable scaled count. They do not downscale, preserving maximum processing capacity.
- **Circuit Breakers:** Standard backoffs and retries prevent cascade failures when querying Kafka broker metadata.

### 2.3 Security Hardening
- **Encryption in Transit:** All connections between brokers, consumer groups, monitoring servers, and the controller use **mTLS (TLS v1.3)** with strict certificate validation.
- **Authentication & Access Control:**
  - Monitored components use **SASL/SCRAM-SHA-512** for secure broker authentication.
  - Scaling Actuator APIs require **Bearer token authentication** or signed JSON Web Tokens (JWT) to authorize scaling requests, preventing malicious scale-down or denial-of-service (DoS) attacks on consumers.
- **Secure Webhooks:** Alert notifications to Slack/PagerDuty are sent over encrypted HTTPS endpoints with cryptographically signed payloads.

---

## 3. System Architecture

```
                               +------------------------+
                               |    Stellar Network     |
                               +-----------+------------+
                                           ^
                                           | (Settlement Calls)
                                           v
+------------------+         +-------------+------------+
|  IoT Devices     +-------->+   Ingestion Gateways     |
+------------------+         +-------------+------------+
                                           |
                                           v (Produce Messages)
                               +-----------+------------+
                               |     Apache Kafka       |
                               |  - Ingestion Topics    |
                               +-----+------------+-----+
                                     |            ^
                     (Read Offsets)  |            | (Commit Offsets)
                                     v            |
+------------------+         +-------+------------+-----+
|   Auto-Scaler    |<--------+     Lag Monitor          |
|   Controller     |         |  - LEO / Commit Offsets  |
+--------+---------+         +--------------------------+
         |
         | (Actuate Scaling / Adjust Replicas)
         v
+--------+---------+         +--------------------------+
|  Consumer Group  |<--------+  Active Consumers        |
|  Orchestrator    |         |  (Instances C1, C2...)   |
+--------+---------+         +--------------------------+
         |
         v
+--------+---------+         +--------------------------+
|  Alert Manager   +-------->+  Ops Alerts (PagerDuty)  |
+------------------+         +--------------------------+
```

### 3.1 Architecture Components

1. **Lag Monitor Service:**
   - Periodically polls Kafka topics for Log End Offsets (LEO) and consumer group Committed Offsets.
   - Calculates **Partition Lag**: $Lag_p = LEO_p - Committed_p$
   - Calculates **Group Lag**: $Lag_{group} = \sum Lag_p$
   - Computes rolling **Lag Rate of Change** (velocity) to predict upcoming queue breaches.

2. **Auto-Scaling Controller:**
   - Evaluates the current Group Lag and consumer processing rates.
   - Determines the target consumer count based on scaling rules, threshold boundaries, and cooling-down parameters.
   - Prevents scaling past the number of partitions (as idle consumers would have no partition assigned).

3. **Actuator (Orchestrator):**
   - Integrates with the deployment orchestrator (e.g., Kubernetes HPA, KEDA, or Docker API) to scale consumer pods/instances.
   - Triggers dynamic partition rebalancing gracefully.

4. **Alert Manager:**
   - Dispatches priority events when lag thresholds are crossed or when auto-scaler errors occur.

---

## 4. Scaling Algorithms & Cooldown Logic

To maintain high availability and prevent resource thrashing (rapid, wasteful cycle of scaling up and scaling down), the controller employs the following mathematical guidelines and cooldown locks:

### 4.1 Scaling Calculation
The desired consumer count is calculated using:
$$DesiredConsumers = \left\lceil \frac{GroupLag}{TargetLagPerConsumer} \right\rceil$$

Subject to:
$$MIN\_CONSUMERS \le DesiredConsumers \le \min(MAX\_CONSUMERS, PartitionCount)$$

### 4.2 Scaling Policy Rules

- **Scale Up Rule:**
  - Triggered if total group lag exceeds `SCALE_UP_THRESHOLD` (e.g., 1000 messages) for `SCALE_UP_EVALUATION_PERIODS` (e.g., 2 consecutive checks).
  - Actuated immediately to resolve lag spikes quickly.
  - Sets `LAST_SCALE_UP_TIMESTAMP`.

- **Scale Down Rule:**
  - Triggered if total group lag is below `SCALE_DOWN_THRESHOLD` (e.g., 100 messages) for `SCALE_DOWN_EVALUATION_PERIODS` (e.g., 5 consecutive checks).
  - Actuated only if the **Scale-Down Cooldown** period has elapsed.

- **Cooldown Locks:**
  - `SCALE_UP_COOLDOWN`: 30 seconds. Prevents multiple scale-up actions before the previously provisioned consumers have finished boot-up and partition rebalancing.
  - `SCALE_DOWN_COOLDOWN`: 300 seconds (5 minutes). A longer downscale cooldown ensures the cluster remains stable during intermittent traffic lulls.

---

## 5. Deployment & Release Strategy

### 5.1 Blue-Green Deployment
- **Green (New Release):** Deploy a fully mirrored consumer group and auto-scaling controller.
- **Verification Stage:** Point the new Green consumer group to the active Kafka brokers with unique, isolated group-id names. Run offset collection checks to ensure Green is calculating lag metrics with `< 100ms P99`.
- **Switch-Over:** Gracefully stop/divert traffic from the Blue consumer group and update the deployment orchestrator.

### 5.2 Canary Analysis
A subset of traffic (e.g., 10% of topics/partitions) is assigned to Canary consumers.
The deployment pipeline monitors the canary for **60 minutes** before fully promoting the release, checking:
1. **Error Rate:** Must be `< 0.01%`.
2. **Lag Calculation latency:** P99 must be `< 100ms`.
3. **Memory/CPU consumption:** Must stay within standard margins.

---

## 6. Operational Runbook

### Scenario A: Consumer Group Lag Spike
- **Symptom:** Total consumer lag rises rapidly, exceeding `SCALE_UP_THRESHOLD`, but the consumer count has reached `MAX_CONSUMERS`.
- **Recovery:**
  1. Verify if partitions are unevenly loaded (hot partitions).
  2. Temporary override: If appropriate, temporarily scale the partitions of the Kafka topic and increase `MAX_CONSUMERS`.
  3. Ensure no network partition is blocking consumer databases or Stellar gateways.

### Scenario B: Autoscaling Actuator Failure
- **Symptom:** Scale up is triggered by the controller, but the infrastructure fails to spawn new consumer instances (e.g., resource exhaustion in the cluster).
- **Recovery:**
  1. Check orchestrator logs (e.g., Kubernetes `kubectl describe` or Docker events).
  2. Fire immediate PagerDuty alert indicating scale actuation failure.
  3. Fallback: Temporarily divert non-critical streams (e.g., grant streams) to secondary queues to preserve capacity for critical billing and credit checking.
