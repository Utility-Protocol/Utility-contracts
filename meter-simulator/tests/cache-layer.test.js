const CacheLayer = require('../src/cache-layer');

describe('CacheLayer', () => {
  test('returns cached values from memory before TTL expiry', async () => {
    const cache = new CacheLayer({ enabled: true, defaultTtlSeconds: 30 });
    const loader = jest.fn().mockResolvedValue({ meter_id: 1, balance: 100 });

    const first = await cache.remember('meter:1', 30, loader);
    const second = await cache.remember('meter:1', 30, loader);

    expect(first).toEqual(second);
    expect(loader).toHaveBeenCalledTimes(1);
    expect(cache.getMetrics()).toMatchObject({ hits: 1, misses: 1, sets: 1 });
  });

  test('reloads values after TTL expiry', async () => {
    jest.useFakeTimers();
    const cache = new CacheLayer({ enabled: true, defaultTtlSeconds: 1 });
    const loader = jest
      .fn()
      .mockResolvedValueOnce({ version: 1 })
      .mockResolvedValueOnce({ version: 2 });

    await expect(cache.remember('usage:1', 1, loader)).resolves.toEqual({ version: 1 });
    jest.advanceTimersByTime(1001);
    await expect(cache.remember('usage:1', 1, loader)).resolves.toEqual({ version: 2 });

    expect(loader).toHaveBeenCalledTimes(2);
    jest.useRealTimers();
  });

  test('delete invalidates memory entries', async () => {
    const cache = new CacheLayer({ enabled: true, defaultTtlSeconds: 30 });

    await cache.set('meter:2', { balance: 10 });
    await expect(cache.get('meter:2')).resolves.toEqual({ balance: 10 });
    await cache.delete('meter:2');

    await expect(cache.get('meter:2')).resolves.toBeNull();
  });
});
