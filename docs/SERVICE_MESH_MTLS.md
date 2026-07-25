# Service Mesh Integration with Mutual TLS

This runbook documents the Utility Contracts service mesh architecture, mTLS
security posture, observability requirements, and blue-green deployment process.
It is designed for Istio-compatible clusters and keeps critical-path P99 latency
below 100 ms while preserving the 99.99% availability objective.

## Architecture

- All workloads in the `utility-contracts` namespace are injected with an Istio
  sidecar and communicate through the mesh.
- `PeerAuthentication/utility-contracts-strict-mtls` enforces strict mutual TLS
  for every in-namespace service connection.
- `DestinationRule/utility-contracts-default-mtls` originates `ISTIO_MUTUAL` TLS
  for service-to-service calls and applies connection pools plus outlier
  detection to bound tail latency.
- `VirtualService/utility-contracts-blue-green` routes normal traffic to the
  active `blue` subset, supports `green` canary traffic with the `x-canary: true`
  header, and caps request timeouts at 100 ms.
- Prometheus alerts detect P99 latency violations, availability drops below
  99.99%, and policy drift where traffic is not protected by mutual TLS.

## Deployment Procedure

1. Label the namespace for sidecar injection:
   `kubectl label namespace utility-contracts istio-injection=enabled --overwrite`.
2. Apply the mesh manifests:
   `kubectl apply -f deploy/service-mesh/`.
3. Deploy the inactive color (`green` for a `blue` active deployment) with the
   `deployment-slot=green` label.
4. Send smoke traffic with `x-canary: true` and verify latency, error rate, and
   mTLS policy metrics.
5. Increase the `green` route weight in controlled steps: 1%, 5%, 25%, 50%, and
   100%. Hold each step for at least 15 minutes or one business-critical batch,
   whichever is longer.
6. Keep the previous color healthy for fast rollback until the new color has met
   the SLOs for one full observation window.

## Canary Analysis Gates

Promote the canary only when all gates pass:

- P99 request latency is at or below 100 ms for critical paths.
- Successful request ratio is at least 99.99% over the observation window.
- `UtilityContractsMTLSPolicyDrift` is inactive.
- No new 5xx spike or outlier-ejection pattern appears in mesh telemetry.
- Security review confirms namespace, authorization, and TLS policies match the
  intended allowlist.

## Rollback

1. Set the `green` route weight to `0` and the last known-good `blue` route to
   `100`.
2. Confirm `istio_requests_total` returns to the pre-deployment error budget
   burn rate.
3. Keep the failed color running only long enough to collect diagnostics, then
   scale it down.
4. Open a post-incident review and update this runbook if any gate or alert did
   not trigger as expected.

## Validation

Run the offline manifest validator before review and deployment:

```bash
python3 scripts/validate_service_mesh.py
```

Run the pytest coverage when Python test dependencies are available:

```bash
python3 -m pytest tests/test_service_mesh_manifests.py
```
