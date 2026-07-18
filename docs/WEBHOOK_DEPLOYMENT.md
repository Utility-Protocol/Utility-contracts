# Webhook Delivery Service Deployment & Rollout Guide

This document describes the deployment strategy, Blue-Green rollout procedure, and Canary Analysis parameters for the off-chain Webhook Delivery Service.

---

## 1. Rollout Overview
Our goal is to deploy updates with **zero downtime** and verify performance targets ($<100\text{ms}$ P99 latency) under real-world traffic patterns before fully promoting new builds.

### SLA & Quality Gates
During rollouts, the deployment pipeline must automatically monitor the following quality gates:
- **Error Rate**: $<0.01\%$ (HTTP 5xx responses).
- **Latency**: P99 latency $<100\text{ms}$ on ingestion endpoints (`POST /webhooks`).
- **CPU / Memory**: No memory leaks or sustained CPU utilization exceeding $80\%$.

---

## 2. Blue-Green Deployment Strategy

We employ a classic Blue-Green deployment model to completely isolate the production environment ("Blue") from the staging environment of the upcoming release ("Green").

```
                  +------------------+
                  |  Traffic Router  |
                  |   (Nginx / ALB)  |
                  +--------+---------+
                           |
             +-------------+-------------+
             |                           |
             v                           v
     +---------------+           +---------------+
     |  Active Blue  |           |  Idle Green   |
     |   (v1.0.0)    |           |   (v1.1.0)    |
     +---------------+           +---------------+
```

### Step 1: Deploy Green Infrastructure
1. Provision a separate instance group or container fleet running the new version (Green).
2. Configure the Green environment to use a dedicated staging endpoint (e.g. `http://green.webhook.internal`).
3. Set up environment variables to match production, ensuring the Green database/caches are fully isolated or configured for backward-compatible schema changes.

### Step 2: Green Health & Sanity Check
Run an automated verification suite against the Green service before routing any live traffic:
```bash
# 1. Verify health endpoint
curl -s -f http://green.webhook.internal/health

# 2. Test simple ingestion call
curl -s -X POST -H "Content-Type: application/json" \
  -d '{"payload":{"event":"test"}, "url":"https://httpbin.org/post", "secret":"test_secret"}' \
  http://green.webhook.internal/webhooks
```

### Step 3: Switch Traffic Router
Once the Green environment passes all sanity tests:
1. Update the load balancer (Nginx or ALB) configurations to redirect $100\%$ of incoming traffic from Blue to Green.
2. Monitor active connections on Blue and allow a **5-minute graceful drain window** to complete any outstanding retry attempts or delivery backlogs.
3. Shut down or idle the Blue infrastructure.

---

## 3. Canary Analysis Strategy

For high-risk updates, a Canary deployment strategy is used to gradually shift traffic and observe behavior.

```
Traffic Shift:  [ 90% Production (Blue) ]  ===>  [ 10% Canary (Green) ]
```

### Step-by-Step Canary Rollout
1. **Stage 1 (10% Traffic)**: Route $10\%$ of live webhook ingestion traffic to the new Green (Canary) instances.
2. **Analysis Window (30 Minutes)**:
   - Query Canary Prometheus metrics: `http://canary.webhook.internal/metrics`.
   - Verify P99 latency:
     ```promql
     histogram_quantile(0.99, sum(rate(webhook_delivery_duration_seconds_bucket[5m])) by (le))
     ```
   - Verify success rates:
     ```promql
     sum(rate(webhook_delivery_attempts_total{status=~"2.."}[5m])) / sum(rate(webhook_delivery_attempts_total[5m])) * 100
     ```
3. **Stage 2 (50% Traffic)**: Increase traffic allocation to $50\%$ if error rates are zero and P99 latency is $<100\text{ms}$. Monitor for an additional 15 minutes.
4. **Stage 3 (100% Traffic)**: Promote Canary to full production (100% traffic) and deprecate the old version.

---

## 4. Rollback Plan

If any quality gate is breached (e.g., P99 latency spikes above 100ms or 5xx error rates exceed 0.1%), trigger an **instantaneous rollback**:

```bash
# 1. Immediate traffic reversion via ALB/Nginx configuration change
nginx -s reload -c /etc/nginx/nginx-blue-stable.conf

# 2. Force drain Green queue (if needed, transfer pending jobs back to Blue)
# 3. Terminate Green container group
```
Using Nginx or ALB configuration symlinks allows traffic redirection to take place in **<1 second**, minimizing blast radius during unexpected issues.
