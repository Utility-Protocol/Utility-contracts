const { PerTenantRateLimiter, TenantTokenBucket } = require('../src/rate-limiter');

describe('TenantTokenBucket', () => {
  test('allows requests while tokens are available', () => {
    let now = 0;
    const bucket = new TenantTokenBucket({
      capacity: 2,
      refillRatePerSecond: 1,
      now: () => now
    });

    expect(bucket.tryRemove()).toMatchObject({ allowed: true, remaining: 1 });
    expect(bucket.tryRemove()).toMatchObject({ allowed: true, remaining: 0 });
    expect(bucket.tryRemove()).toMatchObject({ allowed: false, retryAfterMs: 1000 });
  });

  test('refills tokens over time without exceeding capacity', () => {
    let now = 0;
    const bucket = new TenantTokenBucket({
      capacity: 5,
      refillRatePerSecond: 2,
      now: () => now
    });

    expect(bucket.tryRemove(5).allowed).toBe(true);
    now = 1500;

    expect(bucket.tryRemove(3)).toMatchObject({ allowed: true, remaining: 0 });
    now = 10000;
    bucket.refill();

    expect(bucket.tokens).toBe(5);
  });
});

describe('PerTenantRateLimiter', () => {
  test('isolates token buckets per tenant', () => {
    const limiter = new PerTenantRateLimiter({ capacity: 1, refillRatePerSecond: 1 });

    expect(limiter.check('tenant-a').allowed).toBe(true);
    expect(limiter.check('tenant-a').allowed).toBe(false);
    expect(limiter.check('tenant-b').allowed).toBe(true);
  });

  test('throws structured errors for blocked tenants', () => {
    const limiter = new PerTenantRateLimiter({ capacity: 1, refillRatePerSecond: 1 });
    limiter.assertAllowed('tenant-a');

    expect(() => limiter.assertAllowed('tenant-a')).toThrow('Rate limit exceeded for tenant tenant-a');
    try {
      limiter.assertAllowed('tenant-a');
    } catch (error) {
      expect(error.code).toBe('RATE_LIMIT_EXCEEDED');
      expect(error.retryAfterMs).toBeGreaterThan(0);
      expect(error.decision).toMatchObject({ tenantId: 'tenant-a', limit: 1 });
    }
  });

  test('reports metrics and evicts idle buckets', () => {
    let now = 0;
    const limiter = new PerTenantRateLimiter({
      capacity: 1,
      refillRatePerSecond: 1,
      idleTtlMs: 1000,
      now: () => now
    });

    limiter.check('tenant-a');
    limiter.check('tenant-a');
    now = 2001;
    limiter.check('tenant-b');

    expect(limiter.snapshotMetrics()).toMatchObject({
      allowed: 2,
      blocked: 1,
      createdBuckets: 2,
      evictedBuckets: 1,
      activeBuckets: 1
    });
  });
});
