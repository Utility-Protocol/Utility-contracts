const PostgresPoolHealthProbe = require('../src/postgres-pool-health');

describe('PostgresPoolHealthProbe', () => {
  test('reports healthy status and pool metrics for fast probes', async () => {
    let now = 1000;
    const pool = { totalCount: 10, idleCount: 3, waitingCount: 0, query: jest.fn(async () => ({ rows: [{ '?column?': 1 }] })) };
    const probe = new PostgresPoolHealthProbe(pool, { currentSize: 10, now: () => now });

    const checkPromise = probe.check();
    now += 20;
    const result = await checkPromise;

    expect(result.status).toBe('healthy');
    expect(result.latencyMs).toBe(20);
    expect(result.metrics.utilization).toBe(0.7);
    expect(result.recommendation.action).toBe('hold');
    expect(pool.query).toHaveBeenCalledWith('SELECT 1');
  });

  test('recommends scale up when utilization is high', () => {
    const pool = { totalCount: 10, idleCount: 1, waitingCount: 0, query: jest.fn() };
    const probe = new PostgresPoolHealthProbe(pool, { currentSize: 10, maxSize: 12 });

    expect(probe.recommendSize()).toEqual({ currentSize: 10, desiredSize: 12, action: 'scale_up' });
  });

  test('recommends scale down when utilization is low', () => {
    const pool = { totalCount: 10, idleCount: 9, waitingCount: 0, query: jest.fn() };
    const probe = new PostgresPoolHealthProbe(pool, { currentSize: 10, minSize: 2 });

    expect(probe.recommendSize()).toEqual({ currentSize: 10, desiredSize: 9, action: 'scale_down' });
  });

  test('returns unhealthy and scale-up recommendation on probe failure', async () => {
    const pool = { totalCount: 5, idleCount: 0, waitingCount: 2, query: jest.fn(async () => { throw new Error('connection refused'); }) };
    const probe = new PostgresPoolHealthProbe(pool, { currentSize: 5, maxSize: 10 });

    const result = await probe.check();

    expect(result.status).toBe('unhealthy');
    expect(result.error).toBe('connection refused');
    expect(result.recommendation.action).toBe('scale_up');
  });

  test('applies sizing changes with cooldown protection', () => {
    let now = 60000;
    const pool = { totalCount: 10, idleCount: 0, waitingCount: 1, query: jest.fn() };
    const probe = new PostgresPoolHealthProbe(pool, { currentSize: 10, maxSize: 20, coolDownMs: 30000, now: () => now });

    const first = probe.applySizing();
    expect(first.resized).toBe(true);
    expect(first.currentSize).toBe(13);

    pool.totalCount = 13;
    pool.idleCount = 0;
    const second = probe.applySizing();
    expect(second.resized).toBe(false);
    expect(second.reason).toBe('cooldown');
  });
});
