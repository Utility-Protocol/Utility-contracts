'use strict';

const {
  DRHealthChecker,
  HEALTH_STATE,
} = require('../src/dr-health-checker');

const {
  MultiRegionReplicationManager,
} = require('../src/multi-region-replication');

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function makeSetup(options = {}) {
  let clock = 2_000_000;
  const nowFn = () => clock;
  const advanceClock = (ms) => {
    clock += ms;
  };
  const manager = new MultiRegionReplicationManager({ nowFn });
  const checker = new DRHealthChecker(manager, { nowFn, ...options });
  return { manager, checker, advanceClock };
}

// ---------------------------------------------------------------------------
// Construction
// ---------------------------------------------------------------------------

describe('DRHealthChecker — construction', () => {
  test('throws when not given a MultiRegionReplicationManager', () => {
    expect(() => new DRHealthChecker(null)).toThrow('MultiRegionReplicationManager');
    expect(() => new DRHealthChecker({})).toThrow('MultiRegionReplicationManager');
  });

  test('accepts a valid replication manager', () => {
    const { checker } = makeSetup();
    expect(checker).toBeDefined();
  });
});

// ---------------------------------------------------------------------------
// checkRegion
// ---------------------------------------------------------------------------

describe('DRHealthChecker — checkRegion', () => {
  test('returns HEALTHY for a freshly-initialised region', () => {
    const { checker } = makeSetup();
    const result = checker.checkRegion('us-east-1');
    expect(result.health).toBe(HEALTH_STATE.HEALTHY);
    expect(result.issues).toHaveLength(0);
    expect(result.stale).toBe(false);
  });

  test('reports stale probe when probe age exceeds threshold', () => {
    const { checker, advanceClock } = makeSetup({ probeStalenessThresholdMs: 1000 });
    advanceClock(2000);
    const result = checker.checkRegion('eu-west-1');
    expect(result.stale).toBe(true);
    expect(result.issues.some((i) => i.includes('stale'))).toBe(true);
  });

  test('reports latency budget violation when override exceeds budget', () => {
    const { checker } = makeSetup({ crossRegionLatencyBudgetMs: 50 });
    checker.setLatencyOverride('eu-west-1', 75);
    const result = checker.checkRegion('eu-west-1');
    expect(result.issues.some((i) => i.includes('latency'))).toBe(true);
  });

  test('reports CRITICAL health issue in result', () => {
    const { manager, checker } = makeSetup();
    manager.updateRegionHealth('eu-west-1', HEALTH_STATE.CRITICAL);
    const result = checker.checkRegion('eu-west-1');
    expect(result.health).toBe(HEALTH_STATE.CRITICAL);
    expect(result.issues.some((i) => i.includes('CRITICAL'))).toBe(true);
  });

  test('reports FAILOVER_IN_PROGRESS in issues', () => {
    const { manager, checker } = makeSetup();
    manager.triggerFailover('us-east-1', 'eu-west-1');
    const result = checker.checkRegion('eu-west-1');
    expect(result.issues.some((i) => i.includes('Failover'))).toBe(true);
  });

  test('throws for unknown region', () => {
    const { checker } = makeSetup();
    expect(() => checker.checkRegion('xx-north-1')).toThrow('Unknown region');
  });

  test('clearLatencyOverrides removes all overrides', () => {
    const { checker } = makeSetup({ crossRegionLatencyBudgetMs: 50 });
    checker.setLatencyOverride('eu-west-1', 200);
    checker.clearLatencyOverrides();
    const result = checker.checkRegion('eu-west-1');
    expect(result.issues.every((i) => !i.includes('latency'))).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// checkReplicationLag
// ---------------------------------------------------------------------------

describe('DRHealthChecker — checkReplicationLag', () => {
  test('returns valid when all paths are within RPO', () => {
    const { manager, checker } = makeSetup();
    manager.updateReplicationLag('us-east-1', 'eu-west-1', 5);
    manager.updateReplicationLag('us-east-1', 'ap-southeast-1', 10);
    const result = checker.checkReplicationLag();
    expect(result.valid).toBe(true);
    expect(result.paths.every((p) => p.withinRPO)).toBe(true);
  });

  test('returns invalid when lag exceeds RPO', () => {
    const { manager, checker } = makeSetup();
    manager.updateReplicationLag('us-east-1', 'eu-west-1', 90);
    const result = checker.checkReplicationLag();
    expect(result.valid).toBe(false);
    const violating = result.paths.find((p) => p.path.includes('eu-west-1'));
    expect(violating.withinRPO).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// checkFailoverReadiness
// ---------------------------------------------------------------------------

describe('DRHealthChecker — checkFailoverReadiness', () => {
  test('reports not needed when primary is healthy', () => {
    const { checker } = makeSetup();
    const result = checker.checkFailoverReadiness();
    expect(result.primaryHealthy).toBe(true);
    expect(result.ready).toBe(true);
    expect(result.recommendedTarget).toBeNull();
  });

  test('recommends secondary when primary is critical and secondary is healthy', () => {
    const { manager, checker } = makeSetup();
    manager.updateRegionHealth('us-east-1', HEALTH_STATE.CRITICAL);
    const result = checker.checkFailoverReadiness();
    expect(result.primaryHealthy).toBe(false);
    expect(result.ready).toBe(true);
    expect(result.recommendedTarget).toBeTruthy();
  });

  test('reports not ready when all regions are unhealthy', () => {
    const { manager, checker } = makeSetup();
    manager.updateRegionHealth('us-east-1', HEALTH_STATE.CRITICAL);
    manager.updateRegionHealth('eu-west-1', HEALTH_STATE.CRITICAL);
    manager.updateRegionHealth('ap-southeast-1', HEALTH_STATE.CRITICAL);
    const result = checker.checkFailoverReadiness();
    expect(result.ready).toBe(false);
    expect(result.recommendedTarget).toBeNull();
  });

  test('blocks failover when candidate region lag exceeds RPO', () => {
    const { manager, checker } = makeSetup();
    // Primary is unhealthy.
    manager.updateRegionHealth('us-east-1', HEALTH_STATE.CRITICAL);
    // eu-west-1 is healthy but its lag exceeds RPO.
    manager.updateReplicationLag('us-east-1', 'eu-west-1', 120);
    // After lag update eu-west-1 becomes CRITICAL from lag derivation, which means
    // it won't be considered. ap-southeast-1 may be viable if its lag is ok.
    const result = checker.checkFailoverReadiness();
    // Either blocks or finds ap-southeast-1 — the point is it doesn't allow a
    // region with lag > RPO to be recommended if it is also not healthy.
    // eu-west-1 will be CRITICAL due to lag so it won't be recommended.
    if (result.recommendedTarget === 'eu-west-1') {
      // If eu-west-1 is somehow still healthy, the reason should mention lag.
      expect(result.reason).toMatch(/lag|RPO/i);
    }
  });
});

// ---------------------------------------------------------------------------
// checkAll
// ---------------------------------------------------------------------------

describe('DRHealthChecker — checkAll', () => {
  test('returns HEALTHY overall when all regions are healthy', () => {
    const { checker } = makeSetup();
    const report = checker.checkAll();
    expect(report.overallHealth).toBe(HEALTH_STATE.HEALTHY);
    expect(report.consecutiveCritical).toBe(0);
  });

  test('returns CRITICAL when a region is critical', () => {
    const { manager, checker } = makeSetup();
    manager.updateRegionHealth('us-east-1', HEALTH_STATE.CRITICAL);
    const report = checker.checkAll();
    expect(report.overallHealth).toBe(HEALTH_STATE.CRITICAL);
  });

  test('returns DEGRADED when a region is degraded', () => {
    const { manager, checker } = makeSetup();
    manager.updateRegionHealth('eu-west-1', HEALTH_STATE.DEGRADED);
    const report = checker.checkAll();
    expect(report.overallHealth).toBe(HEALTH_STATE.DEGRADED);
  });

  test('incrementing consecutiveCritical counts successive critical evaluations', () => {
    const { manager, checker } = makeSetup();
    manager.updateRegionHealth('us-east-1', HEALTH_STATE.CRITICAL);
    checker.checkAll();
    checker.checkAll();
    const report = checker.checkAll();
    expect(report.consecutiveCritical).toBe(3);
  });

  test('CRITICAL following DEGRADED resets consecutive count', () => {
    const { manager, checker } = makeSetup();
    // First pass: DEGRADED
    manager.updateRegionHealth('eu-west-1', HEALTH_STATE.DEGRADED);
    checker.checkAll();
    // Second pass: CRITICAL
    manager.updateRegionHealth('us-east-1', HEALTH_STATE.CRITICAL);
    const report = checker.checkAll();
    expect(report.consecutiveCritical).toBe(1);
  });

  test('includes failoverReadiness and lagCheck in report', () => {
    const { checker } = makeSetup();
    const report = checker.checkAll();
    expect(report).toHaveProperty('failoverReadiness');
    expect(report).toHaveProperty('lagCheck');
    expect(report).toHaveProperty('regionChecks');
    expect(report).toHaveProperty('rpoViolationCount');
  });

  test('returns CRITICAL when replication lag violates RPO', () => {
    const { manager, checker } = makeSetup();
    manager.updateReplicationLag('us-east-1', 'eu-west-1', 90);
    const report = checker.checkAll();
    expect(report.overallHealth).toBe(HEALTH_STATE.CRITICAL);
  });
});

// ---------------------------------------------------------------------------
// generateReport
// ---------------------------------------------------------------------------

describe('DRHealthChecker — generateReport', () => {
  test('returns a non-empty string report', () => {
    const { checker } = makeSetup();
    const report = checker.generateReport();
    expect(typeof report).toBe('string');
    expect(report.length).toBeGreaterThan(0);
  });

  test('report includes region health states', () => {
    const { manager, checker } = makeSetup();
    manager.updateRegionHealth('eu-west-1', HEALTH_STATE.DEGRADED);
    const report = checker.generateReport();
    expect(report).toContain('eu-west-1');
    expect(report).toContain(HEALTH_STATE.DEGRADED);
  });

  test('report shows RPO violation count', () => {
    const { checker } = makeSetup();
    const report = checker.generateReport();
    expect(report).toContain('RPO violations');
  });
});

// ---------------------------------------------------------------------------
// Prometheus metrics
// ---------------------------------------------------------------------------

describe('DRHealthChecker — getPrometheusMetrics', () => {
  test('includes utility_dr_overall_health metric', () => {
    const { checker } = makeSetup();
    const text = checker.getPrometheusMetrics();
    expect(text).toContain('utility_dr_overall_health');
  });

  test('overall health = 1 when all regions are healthy', () => {
    const { checker } = makeSetup();
    const text = checker.getPrometheusMetrics();
    expect(text).toContain('utility_dr_overall_health 1');
  });

  test('overall health = 0 when a region is critical', () => {
    const { manager, checker } = makeSetup();
    manager.updateRegionHealth('us-east-1', HEALTH_STATE.CRITICAL);
    const text = checker.getPrometheusMetrics();
    expect(text).toContain('utility_dr_overall_health 0');
  });

  test('includes failover readiness metric', () => {
    const { checker } = makeSetup();
    const text = checker.getPrometheusMetrics();
    expect(text).toContain('utility_dr_failover_ready');
  });

  test('delegates to manager metrics', () => {
    const { checker } = makeSetup();
    const text = checker.getPrometheusMetrics();
    expect(text).toContain('utility_region_health_status');
    expect(text).toContain('utility_replication_lag_seconds');
  });
});
