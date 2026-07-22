# Webhook Delivery Service Architecture

This document describes the design and architecture of the enterprise-grade **Webhook Delivery Service** built for the Utility-Protocol suite.

## 1. Overview and Problem Statement

Decentralized smart contracts on Soroban cannot make direct out-of-band HTTP requests. To deliver real-time notifications (such as low balance alerts, tamper detection, or stream status updates) to external client servers and subscriber endpoints, an off-chain Webhook Delivery Service is required.

### 1.1 Technical Bounds & SLA Targets
- **Scope**: System-wide integration affecting all protocol components.
- **Latency**: Critical ingestion paths must execute in **< 100ms P99**.
- **Availability**: High availability target of **99.99% uptime**.
- **Security**: Robust cryptographic signatures, strict replay attack protection, and rigorous Server-Side Request Forgery (SSRF) defense.

---

## 2. Architecture Diagram

```
+--------------------------+
|  Soroban Smart Contract  |
+------------+-------------+
             | (Events/Alerts)
             v
+------------+-------------+
|    On-chain Poller /     |
|   Webhook Generator      |
+------------+-------------+
             |
             | POST /webhooks
             v
+------------+-------------------------------------------------------------+
| Webhook Delivery Service (Node.js/TypeScript)                            |
|                                                                          |
|  +-----------------------+      +-------------------------------------+  |
|  |   API Ingestion       | ---> |        Async In-Memory Queue        |  |
|  |   (<10ms Response)    |      | (Maintains < 100ms P99 Latency)     |  |
|  +-----------------------+      +------------------+------------------+  |
|                                                    |                     |
|                                                    v                     |
|                                 +------------------+------------------+  |
|                                 |       Background Dispatcher         |  |
|                                 +------------------+------------------+  |
|                                                    |                     |
|            +---------------------------------------+------------------+  |
|            |                                       |                  |  |
|            v                                       v                  v  |
|  +-------------------+                   +-------------------+  +-----+  |
|  | Security Engine:  |                   | Retry Controller: |  | Prom|  |
|  | SSRF Shield       |                   | Exponential       |  | Mets|  |
|  | Ed25519/HMAC-SHA  |                   | Backoff & Jitter  |  +-----+  |
|  +-------------------+                   +-------------------+           |
+------------+---------------------------------------+---------------------+
             |
             | Signed POST Request
             v
+------------+-------------+
|  Client Webhook Endpoint |
+--------------------------+
```

---

## 3. Core Capabilities

### 3.1 Asynchronous Delivery Engine (<100ms P99)
- **Non-blocking Ingestion**: When an alert/webhook payload is received, the ingestion API immediately validates the payload schema, enqueues it to an in-memory queue, and returns `202 Accepted`. This decouples request ingestion from network delivery, guaranteeing extremely low response latency (typically **<10ms**).
- **Asynchronous Dispatching**: An event-driven background loop processes the queue, executing up to configured maximum concurrent requests to avoid thread starvation or exhausting client connections.

### 3.2 Secure Cryptographic Signatures
To ensure authenticity and integrity, outgoing requests are cryptographically signed using two methods:
1. **HMAC-SHA256**: Generates a signature hash using a shared secret.
2. **Ed25519 Asymmetric Signatures**: Generates a signature of the message bytes using the Webhook Service's private key, which can be verified by the recipient using the service's public key.

#### Custom Headers:
- `X-Webhook-Timestamp`: POSIX epoch timestamp of the transmission.
- `X-Webhook-Signature-256`: `t=<timestamp>,v1=<hmac-sha256-signature>`
- `X-Webhook-Signature-Ed25519`: `t=<timestamp>,v1=<ed25519-signature>`

### 3.3 Replay Attack Protection
Recipient endpoints verify the signature and check that the difference between the `X-Webhook-Timestamp` and their current local clock is within a tolerance window (e.g., **5 minutes**). This prevents attackers from eavesdropping on and replaying historical messages.

### 3.4 Strict SSRF (Server-Side Request Forgery) Defense
The security engine validates destination URLs before dispatching requests:
- **Private/Local IP Filtering**: Rejects URLs resolving to private subnets (`127.0.0.0/8`, `10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`, `169.254.169.254` metadata endpoint, etc.).
- **Protocol Whitelisting**: Strictly restricts protocols to `http:` and `https:`.
- **Port Whitelisting**: Rejects requests targeting non-standard administrative/internal ports.

### 3.5 Exponential Backoff with Random Jitter
Transient network drops, rate limits (HTTP 429), and short-term receiver outages are mitigated by a multi-attempt retry model:
- **Retry Schedule**: Uses base backoff factor ($t_{backoff} = \min(t_{max}, t_{base} \times 2^{attempt})$).
- **Full Jitter**: Prevents "thundering herd" issues by introducing randomized delay ($t_{jitter} = \text{random}(0, t_{backoff})$).
- **Max Retries**: Defaulted to **5 attempts** before a webhook is classified as failed.

---

## 4. Monitoring & Metrics

The service registers Prometheus counters and histograms to measure health indicators:
- `webhook_delivery_attempts_total`: Total POST attempts, labeled by status code, target host, and attempt count.
- `webhook_delivery_duration_seconds`: Histogram of endpoint response latency.
- `webhook_queue_size_current`: Gauge representing current queue occupancy.
- `webhook_failures_total`: Total dropped or exhausted delivery alerts.
