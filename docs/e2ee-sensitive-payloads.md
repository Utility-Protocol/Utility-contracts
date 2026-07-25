# End-to-End Encryption for Sensitive Payload Fields

Issue #91 is implemented as an on-chain encrypted-envelope boundary. Because
Stellar/Soroban contract inputs and storage are visible to validators, plaintext
usage details, customer metadata, location, and device diagnostics must be
encrypted by the meter/provider off-chain before submission.

## Architecture

1. The provider registers a meter encryption key identifier with
   `set_meter_encryption_key`. The identifier is a SHA-256 fingerprint of the
   recipient public key or KMS key version; raw keys are never stored on-chain.
2. The meter encrypts sensitive fields off-chain (for example with XChaCha20 or
   AES-GCM) and submits an `EncryptedSensitivePayload` containing only metadata,
   nonce, ciphertext, AAD hash, and a deterministic commitment.
3. `submit_sensitive_payload` verifies the active key id, payload size, allowed
   field mask, freshness window, and commitment before storing the envelope.
4. Indexers and runbooks monitor `E2EEKey` and `E2EEData` events for key
   rotations, payload volume, failures, and latency SLOs.

## Operational bounds

- Critical-path work is O(ciphertext length) with a 1 KiB ciphertext cap to keep
  P99 latency below 100 ms.
- Availability is preserved by accepting ciphertext even when off-chain decryptors
  are unavailable; decryptor lag is monitored out-of-band.
- Blue-green/canary deployments should first enable key registration, then meter
  firmware encryption, then reject plaintext payload paths.

## Security review checklist

- Confirm no plaintext sensitive fields are passed to contract methods or events.
- Confirm key ids are fingerprints only, not decryptable secrets.
- Confirm AEAD AAD covers meter id, field mask, key id, nonce, and billing cycle.
- Confirm monitoring alerts on stale key ids, commitment mismatch, oversize
  payload attempts, and decryptor error-rate increases.
