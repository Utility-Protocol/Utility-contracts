'use strict';

const {
  DRCanaryAnalyzer,
  CANARY_DECISION,
  CANARY_STAGES,
  DEFAULT_THRESHOLDS,
} = require('../src/dr-canary-analyzer');

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Healthy metrics snapshot (within all SLOs). */
const HEALTHY_METRICS = Object.freeze({
  p99LatencyMs: 70,
  availabilityFraction: 0.99995,
  errorRateFraction: 0.00005,
  replicationLagSeconds: 10,
});

/** Degraded metrics snapshot (P99 over budget). */
const HIGH_LATENCY_METRICS = Object.freeze({
  p99LatencyMs: 150,
  availabilityFraction: 0.99995,
  errorRateFraction: 0.00005,
  replicationLagSeconds: 10,
});

/** Low availability metrics snapshot. */
const LOW_AVAILABILITY_METRICS = Object.freeze({
  p99LatencyMs: 70,
  availabilityFraction: 0.9998,
  errorRateFraction: 0.0002,
  replicationLagSeconds: 10,
});

/** RPO violation metrics. */
const RPO_VIOLATION_METRICS = Object.freeze({
  p99LatencyMs: 70,
  availabilityFraction: 0.99995,
  errorRateFraction: 0.00005,
  replicationLagSeconds: 90,
});

function makeAnalyzer(options = {}) {
  let clock = 3_000_000;
  const nowFn = () => clock;
  const analyzer = new DRCanaryAnalyzer({ nowFn, ...options });
  return { analyzer };
}

// ---------------------------------------------------------------------------
// Construction
// ---------------------------------------------------------------------------

describe('DRCanaryAnalyzer — construction', () => {
  test('starts at stage 0', () => {
    const { analyzer } = makeAnalyzer();
    expect(analyzer.getCurrentStage()).toBe(0);
  });

  test('next stage from 0 is 5', () => {
    const { analyzer } = makeAnalyzer();
    analyzer.setCurrentStage(5);
    expect(analyzer.getCurrentStage()).toBe(5);
    expect(analyzer.getNextStage()).toBe(25);
  });

  test('getNextStage returns null at 100', () => {
    const { analyzer } = makeAnalyzer();
    analyzer.setCurrentStage(100);
    expect(analyzer.getNextStage()).toBeNull();
  });

  test('setCurrentStage throws for invalid stage', () => {
    const { analyzer } = makeAnalyzer();
    expect(() => analyzer.setCurrentStage(15)).toThrow('Invalid stage');
  });

  test('accepts custom thresholds', () => {
    const { analyzer } = makeAnalyzer({ thresholds: { p99LatencyMs: 50 } });
    const report = analyzer.generateCanaryReport();
    expect(report.thresholds.p99LatencyMs).toBe(50);
  });
});

// ---------------------------------------------------------------------------
// compareRegionMetrics
// ---------------------------------------------------------------------------

describe('DRCanaryAnalyzer — compareRegionMetrics', () => {
  test('returns PROMOTE when canary metrics are healthy', () => {
    const { analyzer } = makeAnalyzer();
    const result = analyzer.compareRegionMetrics(
      'us-east-1', 'eu-west-1', HEALTHY_METRICS, HEALTHY_METRICS
    );
    expect(result.decision).toBe(CANARY_DECISION.PROMOTE);
    expect(result.violations).toHaveLength(0);
    expect(result.regressions).toHaveLength(0);
  });

  test('returns ROLLBACK when canary P99 exceeds threshold', () => {
    const { analyzer } = makeAnalyzer();
    const result = analyzer.compareRegionMetrics(
      'us-east-1', 'eu-west-1', HEALTHY_METRICS, HIGH_LATENCY_METRICS
    );
    expect(result.decision).toBe(CANARY_DECISION.ROLLBACK);
    expect(result.violations.some((v) => v.includes('P99'))).toBe(true);
  });

  test('returns ROLLBACK when availability is below threshold', () => {
    const { analyzer } = makeAnalyzer();
    const result = analyzer.compareRegionMetrics(
      'us-east-1', 'eu-west-1', HEALTHY_METRICS, LOW_AVAILABILITY_METRICS
    );
    expect(result.decision).toBe(CANARY_DECISION.ROLLBACK);
    expect(result.violations.some((v) => v.includes('vailability'))).toBe(true);
  });

  test('returns ROLLBACK when error rate exceeds threshold', () => {
    const { analyzer } = makeAnalyzer();
    const badMetrics = { ...HEALTHY_METRICS, errorRateFraction: 0.001 };
    const result = analyzer.compareRegionMetrics(
      'us-east-1', 'eu-west-1', HEALTHY_METRICS, badMetrics
    );
    expect(result.decision).toBe(CANARY_DECISION.ROLLBACK);
    expect(result.violations.some((v) => v.includes('Error rate') || v.includes('rror'))).toBe(true);
  });

  test('returns ROLLBACK when replication lag violates RPO', () => {
    const { analyzer } = makeAnalyzer();
    const result = analyzer.compareRegionMetrics(
      'us-east-1', 'eu-west-1', HEALTHY_METRICS, RPO_VIOLATION_METRICS
    );
    expect(result.decision).toBe(CANARY_DECISION.ROLLBACK);
    expect(result.violations.some((v) => v.includes('lag') || v.includes('Replication'))).toBe(true);
  });

  test('detects relative P99 regression against baseline', () => {
    const { analyzer } = makeAnalyzer();
    const baseline = { ...HEALTHY_METRICS, p99LatencyMs: 50 };
    // Canary is 20% slower than baseline, above the 10% tolerance.
    const canary = { ...HEALTHY_METRICS, p99LatencyMs: 61 };
    const result = analyzer.compareRegionMetrics('us-east-1', 'eu-west-1', baseline, canary);
    expect(result.decision).toBe(CANARY_DECISION.ROLLBACK);
    expect(result.regressions.some((r) => r.includes('regression'))).toBe(true);
  });

  test('does not flag regression within tolerance', () => {
    const { analyzer } = makeAnalyzer();
    const baseline = { ...HEALTHY_METRICS, p99LatencyMs: 50 };
    // 5% regression — within the 10% tolerance.
    const canary = { ...HEALTHY_METRICS, p99LatencyMs: 52 };
    const result = analyzer.compareRegionMetrics('us-east-1', 'eu-west-1', baseline, canary);
    expect(result.regressions).toHaveLength(0);
  });

  test('throws when metrics have invalid fields', () => {
    const { analyzer } = makeAnalyzer();
    expect(() =>
      analyzer.compareRegionMetrics('a', 'b', { p99LatencyMs: NaN, availabilityFraction: 1, errorRateFraction: 0, replicationLagSeconds: 0 }, HEALTHY_METRICS)
    ).toThrow();
    expect(() =>
      analyzer.compareRegionMetrics('a', 'b', HEALTHY_METRICS, { p99LatencyMs: 10, availabilityFraction: 1.5, errorRateFraction: 0, replicationLagSeconds: 0 })
    ).toThrow('between 0 and 1');
  });
});

// ---------------------------------------------------------------------------
// evaluatePromotionCriteria
// ---------------------------------------------------------------------------

describe('DRCanaryAnalyzer — evaluatePromotionCriteria', () => {
  test('returns HOLD when history is empty', () => {
    const { analyzer } = makeAnalyzer();
    const result = analyzer.evaluatePromotionCriteria();
    expect(result.decision).toBe(CANARY_DECISION.HOLD);
  });

  test('returns PROMOTE after required consecutive PROMOTE decisions', () => {
    const { analyzer } = makeAnalyzer();
    // Record 3 PROMOTE decisions.
    for (let i = 0; i < 3; i++) {
      analyzer.compareRegionMetrics('us-east-1', 'eu-west-1', HEALTHY_METRICS, HEALTHY_METRICS);
    }
    const result = analyzer.evaluatePromotionCriteria(3);
    expect(result.decision).toBe(CANARY_DECISION.PROMOTE);
  });

  test('returns HOLD when not enough consecutive PROMOTE decisions', () => {
    const { analyzer } = makeAnalyzer();
    analyzer.compareRegionMetrics('us-east-1', 'eu-west-1', HEALTHY_METRICS, HEALTHY_METRICS);
    analyzer.compareRegionMetrics('us-east-1', 'eu-west-1', HEALTHY_METRICS, HEALTHY_METRICS);
    const result = analyzer.evaluatePromotionCriteria(3);
    expect(result.decision).toBe(CANARY_DECISION.HOLD);
  });

  test('returns ROLLBACK when a recent ROLLBACK decision exists', () => {
    const { analyzer } = makeAnalyzer();
    analyzer.compareRegionMetrics('us-east-1', 'eu-west-1', HEALTHY_METRICS, HIGH_LATENCY_METRICS);
    analyzer.compareRegionMetrics('us-east-1', 'eu-west-1', HEALTHY_METRICS, HEALTHY_METRICS);
    const result = analyzer.evaluatePromotionCriteria(3);
    expect(result.decision).toBe(CANARY_DECISION.ROLLBACK);
  });

  test('advances stage on PROMOTE', () => {
    const { analyzer } = makeAnalyzer();
    analyzer.setCurrentStage(5);
    for (let i = 0; i < 3; i++) {
      analyzer.compareRegionMetrics('us-east-1', 'eu-west-1', HEALTHY_METRICS, HEALTHY_METRICS);
    }
    const result = analyzer.evaluatePromotionCriteria(3);
    expect(result.decision).toBe(CANARY_DECISION.PROMOTE);
    expect(analyzer.getCurrentStage()).toBe(25);
  });
});

// ---------------------------------------------------------------------------
// analyzeCanaryWindow
// ---------------------------------------------------------------------------

describe('DRCanaryAnalyzer — analyzeCanaryWindow', () => {
  test('returns PROMOTE when all samples are healthy', () => {
    const { analyzer } = makeAnalyzer();
    const samples = Array.from({ length: 5 }, () => ({
      baselineMetrics: HEALTHY_METRICS,
      canaryMetrics: HEALTHY_METRICS,
    }));
    const result = analyzer.analyzeCanaryWindow(samples);
    expect(result.decision).toBe(CANARY_DECISION.PROMOTE);
    expect(result.rollbackCount).toBe(0);
    expect(result.sampleCount).toBe(5);
  });

  test('returns ROLLBACK if any sample is unhealthy', () => {
    const { analyzer } = makeAnalyzer();
    const samples = [
      { baselineMetrics: HEALTHY_METRICS, canaryMetrics: HEALTHY_METRICS },
      { baselineMetrics: HEALTHY_METRICS, canaryMetrics: HIGH_LATENCY_METRICS },
      { baselineMetrics: HEALTHY_METRICS, canaryMetrics: HEALTHY_METRICS },
    ];
    const result = analyzer.analyzeCanaryWindow(samples);
    expect(result.decision).toBe(CANARY_DECISION.ROLLBACK);
    expect(result.rollbackCount).toBe(1);
  });

  test('throws when samples array is empty', () => {
    const { analyzer } = makeAnalyzer();
    expect(() => analyzer.analyzeCanaryWindow([])).toThrow('non-empty array');
  });
});

// ---------------------------------------------------------------------------
// generateCanaryReport
// ---------------------------------------------------------------------------

describe('DRCanaryAnalyzer — generateCanaryReport', () => {
  test('returns a structured report', () => {
    const { analyzer } = makeAnalyzer();
    const report = analyzer.generateCanaryReport();
    expect(report).toHaveProperty('currentStage');
    expect(report).toHaveProperty('nextStage');
    expect(report).toHaveProperty('decisionCounts');
    expect(report).toHaveProperty('recentDecisions');
    expect(report).toHaveProperty('thresholds');
    expect(report).toHaveProperty('stages');
    expect(report).toHaveProperty('timestamp');
  });

  test('decisionCounts reflects made decisions', () => {
    const { analyzer } = makeAnalyzer();
    analyzer.compareRegionMetrics('us-east-1', 'eu-west-1', HEALTHY_METRICS, HEALTHY_METRICS);
    analyzer.compareRegionMetrics('us-east-1', 'eu-west-1', HEALTHY_METRICS, HIGH_LATENCY_METRICS);
    const report = analyzer.generateCanaryReport();
    expect(report.decisionCounts[CANARY_DECISION.PROMOTE]).toBe(1);
    expect(report.decisionCounts[CANARY_DECISION.ROLLBACK]).toBe(1);
  });
});

// ---------------------------------------------------------------------------
// Prometheus metrics
// ---------------------------------------------------------------------------

describe('DRCanaryAnalyzer — getPrometheusMetrics', () => {
  test('returns utility_dr_canary_stage metric', () => {
    const { analyzer } = makeAnalyzer();
    analyzer.setCurrentStage(25);
    const text = analyzer.getPrometheusMetrics();
    expect(text).toContain('utility_dr_canary_stage 25');
  });

  test('returns utility_dr_canary_decision_total counters', () => {
    const { analyzer } = makeAnalyzer();
    analyzer.compareRegionMetrics('us-east-1', 'eu-west-1', HEALTHY_METRICS, HEALTHY_METRICS);
    const text = analyzer.getPrometheusMetrics();
    expect(text).toContain('utility_dr_canary_decision_total{decision="promote"}');
    expect(text).toContain('utility_dr_canary_decision_total{decision="rollback"}');
    expect(text).toContain('utility_dr_canary_decision_total{decision="hold"}');
  });
});
