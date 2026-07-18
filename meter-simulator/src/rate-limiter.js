/**
 * Per-tenant token bucket rate limiter for simulator API paths.
 *
 * The limiter is intentionally dependency-free so it can be used by direct
 * contract submissions, MQTT publishing, CLI commands, and tests without
 * introducing a network call on the critical path.
 */
class TenantTokenBucket {
  constructor({ capacity, refillRatePerSecond, now = () => Date.now() }) {
    if (!Number.isFinite(capacity) || capacity <= 0) {
      throw new Error('capacity must be a positive number');
    }
    if (!Number.isFinite(refillRatePerSecond) || refillRatePerSecond <= 0) {
      throw new Error('refillRatePerSecond must be a positive number');
    }

    this.capacity = capacity;
    this.refillRatePerSecond = refillRatePerSecond;
    this.now = now;
    this.tokens = capacity;
    this.updatedAt = now();
  }

  refill() {
    const currentTime = this.now();
    const elapsedSeconds = Math.max(0, currentTime - this.updatedAt) / 1000;

    if (elapsedSeconds > 0) {
      this.tokens = Math.min(
        this.capacity,
        this.tokens + elapsedSeconds * this.refillRatePerSecond
      );
      this.updatedAt = currentTime;
    }
  }

  tryRemove(tokens = 1) {
    if (!Number.isFinite(tokens) || tokens <= 0) {
      throw new Error('tokens must be a positive number');
    }

    this.refill();

    if (this.tokens >= tokens) {
      this.tokens -= tokens;
      return {
        allowed: true,
        remaining: Math.floor(this.tokens),
        retryAfterMs: 0,
        resetAt: this.updatedAt
      };
    }

    const deficit = tokens - this.tokens;
    const retryAfterMs = Math.ceil((deficit / this.refillRatePerSecond) * 1000);

    return {
      allowed: false,
      remaining: Math.floor(this.tokens),
      retryAfterMs,
      resetAt: this.updatedAt + retryAfterMs
    };
  }
}

class PerTenantRateLimiter {
  constructor(options = {}) {
    this.capacity = options.capacity || 60;
    this.refillRatePerSecond = options.refillRatePerSecond || 1;
    this.idleTtlMs = options.idleTtlMs || 10 * 60 * 1000;
    this.now = options.now || (() => Date.now());
    this.buckets = new Map();
    this.metrics = {
      allowed: 0,
      blocked: 0,
      createdBuckets: 0,
      evictedBuckets: 0
    };
  }

  check(tenantId, tokens = 1) {
    const bucket = this.getBucket(tenantId);
    const decision = bucket.tryRemove(tokens);

    if (decision.allowed) {
      this.metrics.allowed += 1;
    } else {
      this.metrics.blocked += 1;
    }

    return {
      ...decision,
      tenantId,
      limit: this.capacity
    };
  }

  assertAllowed(tenantId, tokens = 1) {
    const decision = this.check(tenantId, tokens);
    if (!decision.allowed) {
      const error = new Error(`Rate limit exceeded for tenant ${tenantId}`);
      error.code = 'RATE_LIMIT_EXCEEDED';
      error.retryAfterMs = decision.retryAfterMs;
      error.decision = decision;
      throw error;
    }
    return decision;
  }

  getBucket(tenantId) {
    if (!tenantId) {
      throw new Error('tenantId is required');
    }

    this.evictIdleBuckets();

    if (!this.buckets.has(tenantId)) {
      this.buckets.set(
        tenantId,
        new TenantTokenBucket({
          capacity: this.capacity,
          refillRatePerSecond: this.refillRatePerSecond,
          now: this.now
        })
      );
      this.metrics.createdBuckets += 1;
    }

    return this.buckets.get(tenantId);
  }

  evictIdleBuckets() {
    const cutoff = this.now() - this.idleTtlMs;
    for (const [tenantId, bucket] of this.buckets.entries()) {
      if (bucket.updatedAt < cutoff) {
        this.buckets.delete(tenantId);
        this.metrics.evictedBuckets += 1;
      }
    }
  }

  snapshotMetrics() {
    return {
      ...this.metrics,
      activeBuckets: this.buckets.size,
      capacity: this.capacity,
      refillRatePerSecond: this.refillRatePerSecond
    };
  }
}

module.exports = {
  TenantTokenBucket,
  PerTenantRateLimiter
};
