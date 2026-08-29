import { compareResults, summarize, DEFAULT_REGRESSION_TOLERANCE_PERCENT } from '../src/perf/compare-results';

describe('compareResults - performance regression gate', () => {
  const latencyBaselines = {
    ingestion_p99_ms: { value: 8, slaMs: 100 },
    ingestion_p95_ms: { value: 5 },
    ingestion_mean_ms: { value: 2 },
    ingestion_throughput_rps: { value: 4000, direction: 'higher' as const },
  };

  test('passes when all metrics are within tolerance of baseline', () => {
    const comparisons = compareResults({
      baselines: latencyBaselines,
      current: {
        ingestion_p99_ms: 9,
        ingestion_p95_ms: 5.2,
        ingestion_mean_ms: 2.1,
        ingestion_throughput_rps: 4100,
      },
    });
    expect(comparisons.every((c) => c.status === 'pass')).toBe(true);
    expect(summarize(comparisons).status).toBe('pass');
  });

  test('fails when p99 exceeds the absolute SLO ceiling', () => {
    const comparisons = compareResults({
      baselines: latencyBaselines,
      current: { ingestion_p99_ms: 250, ingestion_p95_ms: 3, ingestion_mean_ms: 1, ingestion_throughput_rps: 4000 },
    });
    const p99 = comparisons.find((c) => c.metric === 'ingestion_p99_ms')!;
    expect(p99.status).toBe('fail');
    // The SLO breach captures the hard-bound even when relative to baseline would pass.
    expect(p99.message).toContain('SLO');
    expect(summarize(comparisons).status).toBe('fail');
  });

  test('fails when a lower-is-better metric regresses beyond tolerance', () => {
    const comparisons = compareResults({
      baselines: latencyBaselines,
      current: { ingestion_p95_ms: 12, ingestion_p99_ms: 10, ingestion_mean_ms: 2, ingestion_throughput_rps: 4000 },
    });
    // Baseline 5 -> 12 is +140% regression.
    const p95 = comparisons.find((c) => c.metric === 'ingestion_p95_ms')!;
    expect(p95.status).toBe('fail');
    expect(p95.deltaPercent).toBeGreaterThan(DEFAULT_REGRESSION_TOLERANCE_PERCENT);
  });

  test('fails when a higher-is-better metric (throughput) drops beyond tolerance', () => {
    const comparisons = compareResults({
      baselines: latencyBaselines,
      current: { ingestion_throughput_rps: 2000, ingestion_p99_ms: 5, ingestion_p95_ms: 3, ingestion_mean_ms: 1 },
    });
    const tp = comparisons.find((c) => c.metric === 'ingestion_throughput_rps')!;
    expect(tp.direction).toBe('higher');
    // Baseline 4000 -> 2000 is -50% -> regression for higher-is-better.
    expect(tp.status).toBe('fail');
  });

  test('fails when a required metric was not measured', () => {
    const comparisons = compareResults({
      baselines: latencyBaselines,
      current: { ingestion_p99_ms: 5, ingestion_p95_ms: 3, ingestion_mean_ms: 1 },
    });
    const tp = comparisons.find((c) => c.metric === 'ingestion_throughput_rps')!;
    expect(tp.status).toBe('fail');
    expect(tp.message).toContain('not measured');
  });

  test('warns (not fails) when a measured metric has no baseline yet', () => {
    const comparisons = compareResults({
      baselines: { only_tracked: { value: 10 } },
      current: { only_tracked: 10, brand_new_metric: 4 },
    });
    const brandNew = comparisons.find((c) => c.metric === 'brand_new_metric')!;
    expect(brandNew.status).toBe('warn');
    const summary = summarize(comparisons);
    expect(summary.status).toBe('warn');
    expect(summary.failures).toHaveLength(0);
  });

  test('honours a custom regression tolerance', () => {
    // With a tight 5% tolerance a 20% drift must fail.
    const comparisons = compareResults({
      baselines: { ingestion_p99_ms: { value: 100 } },
      current: { ingestion_p99_ms: 120 },
      regressionTolerancePercent: 5,
    });
    expect(comparisons[0].status).toBe('fail');
  });

  test('summarize counts passes, warnings and failures', () => {
    const comparisons = compareResults({
      baselines: latencyBaselines,
      current: { ingestion_p99_ms: 5, ingestion_p95_ms: 3, ingestion_mean_ms: 1, ingestion_throughput_rps: 4000 },
    });
    const summary = summarize(comparisons);
    expect(summary.passes.length).toBe(4);
    expect(summary.failures.length).toBe(0);
    expect(summary.warnings.length).toBe(0);
    expect(summary.status).toBe('pass');
  });
});